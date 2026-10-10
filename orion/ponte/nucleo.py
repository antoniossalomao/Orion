"""O lado do cliente da ponte (regra 50): teclas globais, bandeja, janelas e área de transferência.

Nada aqui importa pynput, pystray, webview nem tesseract: os adaptadores entram por construtor
(`Adaptadores`), como a fonte de áudio em `orion.wake`. Assim a ponte inteira é provada com
falsos (`tests/test_ponte.py`) e só o `adaptadores.py` depende do sistema.

A ponte **não decide nada**: reage à tecla que o Antônio apertou, ao item da bandeja que ele
clicou ou a um comando validado do servidor (`abrir`, `colar`). Não lê a tela nem a área de
transferência fora disso, não guarda imagem e não executa o que vem da rede.
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import threading
import time
from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from typing import Any, Protocol
from urllib.parse import quote

import httpx

from .config import mapa_de_teclas, para_pynput
from .hub import ComandoInvalido, validar

log = logging.getLogger("orion.ponte")

BACKOFF_MAX_S = 30.0
AVISAR_DEPOIS_DE = 3  # falhas seguidas de conexão antes de avisar na tela


class Teclado(Protocol):
    def iniciar(self, mapa: dict[str, Callable[[], None]]) -> None:
        """Registra as teclas globais (chave no formato do pynput: `<ctrl>+<alt>+<space>`)."""
        ...

    def parar(self) -> None: ...


class Bandeja(Protocol):
    def iniciar(self, itens: Sequence[tuple[str, Callable[[], None]]]) -> None: ...

    def parar(self) -> None: ...


class Janela(Protocol):
    def abrir(self, url: str, titulo: str, largura: int, altura: int) -> None:
        """Abre uma janela pequena, sem barras, em outro processo (nunca trava a ponte)."""
        ...


class AreaDeTransferencia(Protocol):
    def ler(self) -> str | None: ...

    def gravar(self, texto: str) -> None: ...


class Colador(Protocol):
    def colar(self) -> None:
        """Envia Ctrl+V (Cmd+V no macOS) ao programa em foco."""
        ...


@dataclass
class Adaptadores:
    teclado: Teclado
    bandeja: Bandeja
    janela: Janela
    area: AreaDeTransferencia
    colador: Colador
    navegador: Callable[[str], Any]  # abre o endereço no navegador padrão
    notificar: Callable[[str, str], None]  # (título, texto): aviso curto do sistema
    # E2.4 e E2.5 acrescentam: seleção de área + OCR, captura da janela em foco
    ocr_da_area: Callable[[], str | None] | None = None
    captura_da_janela: Callable[[], bytes | None] | None = field(default=None)


class Ponte:
    def __init__(
        self,
        url: str,
        token: str,
        adaptadores: Adaptadores,
        *,
        teclas: str = "",
        http: httpx.Client | None = None,
        dormir: Callable[[float], None] = time.sleep,
        atraso_colar_s: float = 1.0,
    ) -> None:
        self.url = url.rstrip("/")
        self._token = token
        self.a = adaptadores
        self.teclas = mapa_de_teclas(teclas)
        self._http = http or httpx.Client(
            base_url=self.url, headers={"Authorization": f"Bearer {token}"}, timeout=10
        )
        self._dormir = dormir
        self._atraso_colar = atraso_colar_s
        self._parar = threading.Event()
        self._colar_lock = threading.Lock()

    # ── endereços ─────────────────────────────────────────────────────────
    @property
    def ws_url(self) -> str:
        return self.url.replace("http", "ws", 1) + "/ws/ponte"

    def pagina(self, modo: str, **busca: str) -> str:
        """Página mínima da ponte (`/ui/ponte.html`). O token vai no fragmento: não sai do
        computador, não entra em log do servidor e a página o tira da barra de endereços."""
        extra = "".join(f"&{k}={quote(v)}" for k, v in busca.items())
        return f"{self.url}/ui/ponte.html?modo={modo}{extra}#t={self._token}"

    # ── ações (teclas e bandeja) ──────────────────────────────────────────
    def captura_rapida(self) -> None:
        self.a.janela.abrir(self.pagina("rapido"), "Captura rápida", 560, 190)

    def abrir_orion(self) -> None:
        self.a.navegador(f"{self.url}/ui/#/chat")

    def panico(self) -> None:
        """Entrar no pânico é sempre seguro (sair exige a senha, no painel)."""
        try:
            self._http.post("/modo/panico", json={"ativo": True}).raise_for_status()
            self.a.notificar("Orion", "Modo pânico ligado. Saia pelo painel, com a senha.")
        except httpx.HTTPError as e:
            log.warning("pânico pela ponte falhou: %s", type(e).__name__)
            self.a.notificar("Orion", "Não consegui ligar o modo pânico: servidor fora do ar?")

    def acoes_das_teclas(self) -> dict[str, Callable[[], None]]:
        """Chaves no formato do pynput. Só entram as ações que esta ponte sabe fazer."""
        acoes: dict[str, Callable[[], None]] = {
            "captura": self.captura_rapida,
            "panico": self.panico,
        }
        if self.a.ocr_da_area is not None:
            acoes["ocr"] = self.copiar_texto_da_tela
        if self.a.captura_da_janela is not None:
            acoes["isso"] = self.o_que_e_isso
        return {para_pynput(self.teclas[nome]): fn for nome, fn in acoes.items()}

    def itens_da_bandeja(self) -> list[tuple[str, Callable[[], None]]]:
        itens = [
            ("Abrir Orion", self.abrir_orion),
            ("Captura rápida", self.captura_rapida),
            ("Modo pânico", self.panico),
            ("Sair", self.sair),
        ]
        return itens

    # E2.4 e E2.5 (preenchidos nas próprias etapas)
    def copiar_texto_da_tela(self) -> None:
        raise NotImplementedError

    def o_que_e_isso(self) -> None:
        raise NotImplementedError

    # ── comandos que vêm do servidor ──────────────────────────────────────
    def tratar(self, bruto: dict[str, Any]) -> None:
        """Valida de novo (a ponte não confia em quem manda) e executa `abrir` ou `colar`."""
        try:
            cmd = validar(bruto)
        except ComandoInvalido:
            log.warning("comando recusado pela ponte")
            return
        if cmd["cmd"] == "abrir":
            self.a.navegador(f"{self.url}/ui/{cmd['rota']}")
        else:
            threading.Thread(target=self.colar, args=(cmd["texto"],), daemon=True).start()

    def colar(self, texto: str) -> None:
        """Põe o texto na área de transferência, cola no programa em foco e devolve o que havia
        (uma colagem de cada vez: duas seguidas não trocam o "anterior" uma da outra)."""
        with self._colar_lock:
            antes = None
            with contextlib.suppress(Exception):
                antes = self.a.area.ler()
            self.a.area.gravar(texto)
            self.a.colador.colar()
            self._dormir(self._atraso_colar)
            if antes is not None:
                with contextlib.suppress(Exception):
                    self.a.area.gravar(antes)

    # ── ciclo de vida ─────────────────────────────────────────────────────
    def sair(self) -> None:
        self._parar.set()

    @property
    def parada(self) -> bool:
        return self._parar.is_set()

    async def escutar(
        self, conectar: Callable[..., Any], *, dormir_async: Any = asyncio.sleep
    ) -> str:
        """Mantém o WebSocket com o servidor, reconectando com espera crescente. Devolve o motivo
        de ter parado: `sair` ou `pareamento` (token recusado: parear de novo)."""
        espera, falhas = 1.0, 0
        while not self._parar.is_set():
            conectou = False
            try:
                async with conectar(self.ws_url, {"Authorization": f"Bearer {self._token}"}) as ws:
                    conectou, espera, falhas = True, 1.0, 0
                    async for mensagem in ws:
                        if self._parar.is_set():
                            return "sair"
                        with contextlib.suppress(ValueError, TypeError):
                            comando = json.loads(mensagem)
                            if isinstance(comando, dict):
                                self.tratar(comando)
            except Exception as e:  # noqa: BLE001 — qualquer queda de rede: tenta de novo
                if _recusado(e):
                    self.a.notificar("Orion", "A ponte foi recusada: rode `orion ponte --parear`.")
                    return "pareamento"
                falhas += 1
                if falhas == AVISAR_DEPOIS_DE:
                    self.a.notificar("Orion", "A ponte perdeu a conexão com o servidor.")
                log.info("ponte desconectada (%s); tentando em %.0f s", type(e).__name__, espera)
            if self._parar.is_set():
                break
            await dormir_async(espera)
            espera = 1.0 if conectou else min(espera * 2, BACKOFF_MAX_S)
        return "sair"

    def rodar(self, conectar: Callable[..., Any]) -> str:
        """Sobe teclado e bandeja e fica no WebSocket até `sair` (ou o servidor recusar)."""
        self.a.teclado.iniciar(self.acoes_das_teclas())
        self.a.bandeja.iniciar(self.itens_da_bandeja())
        try:
            return asyncio.run(self.escutar(conectar))
        finally:
            self.a.teclado.parar()
            self.a.bandeja.parar()


def _recusado(e: BaseException) -> bool:
    """O servidor respondeu 401/403/1008 ao abrir o WebSocket (token inválido ou pânico)."""
    resposta = getattr(e, "response", None)
    if getattr(resposta, "status_code", None) in (401, 403):
        return True
    return getattr(e, "code", None) == 1008

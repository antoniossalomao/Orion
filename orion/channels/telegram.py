"""Canal Telegram (fase 5 do NUCLEO): o Orion no celular, sem expor porta.

Long polling (`getUpdates`): o notebook só faz chamadas de saída, não precisa de IP
público nem de Tailscale para este canal. O que o canal garante:

- **default-deny**: só responde a usuários da lista, em conversa privada; qualquer outro
  é ignorado em silêncio (só vai para o log), para o bot não revelar que existe;
- **aprovação fora de banda** (REGRAS.md: confirmação fora de banda (canal autenticado)): ação que a
  política manda confirmar vira uma
  mensagem com botões ✅/❌; só o clique de um usuário permitido decide, nunca uma frase;
- **avisos**: lembretes e agendamentos da fila (`orion.memory.ops`) saem aqui e só são
  confirmados (`ack`) depois de enviados;
- **segredo**: o token vai na URL da API do Telegram, então nenhuma mensagem de erro ou
  log leva a URL (só o tipo do erro) e o logger do `httpx` fica em WARNING.

Voz e foto: a mensagem de voz é transcrita (Whisper, `orion.transcribe`) e o texto entendido é
mostrado antes da resposta, para quem falou conferir; a foto vai ao modelo como imagem **só
naquele turno** (o histórico guarda um aviso, não a imagem). Aprovação continua só por botão.

`/capturar` guarda a próxima mensagem (texto, link, foto ou voz) como nota no vault, sem passar
pelo modelo (`orion.capture`, REGRAS.md: captura rápida e briefing não passam pelo modelo);
`/briefing` manda o resumo do dia (`orion.briefing`);
`/painel` manda o painel único (`orion.painel`).

Texto puro (sem `parse_mode`): resposta de modelo com `*` ou `_` não quebra a mensagem.
"""

from __future__ import annotations

import asyncio
import base64
import contextlib
import json
import logging
import re
import time
from collections.abc import AsyncIterator, Awaitable, Callable, Coroutine, Sequence
from typing import Any

import httpx

from .. import saidas
from ..agent import Agent, AgentEvent
from ..briefing import build_briefing
from ..capture import EXTENSOES_FOTO, CaptureError, Capturer
from ..memory import MemoryStore
from ..memory.ops import Operations
from ..modos import ModoError, Modos
from ..policy import ApprovalStore, redact
from ..transcribe import MAX_AUDIO, TranscribeError, Transcriber

log = logging.getLogger("orion.telegram")

CANAL = "telegram"
API = "https://api.telegram.org"
LIMITE_MENSAGEM = 4000  # o Telegram aceita 4096; folga para o que o cliente conta diferente
LIMITE_ENTRADA = 8000  # o mesmo teto do POST /chat
_METODOS_DE_SAIDA = ("send", "edit", "answer")
_CONVERTE = re.compile(r"\bconvert\w*\s+(?:para|pra|em|to)\s+\.?([A-Za-z0-9]{2,12})", re.I)
MAX_DOCUMENTO = 20 * 1024 * 1024  # limite de download do Telegram para bots
MAX_VOZ_S = 300  # áudio mais longo que 5 min não é transcrito
MAX_FOTO = 4 * 1024 * 1024  # bytes; escolhe o maior tamanho que couber
_MIME_FOTO = {
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".png": "image/png",
    ".webp": "image/webp",
}
_CAMINHO_ARQUIVO = re.compile(r"^[A-Za-z0-9_./-]{1,200}$")
_CALLBACK = re.compile(r"^ap:([A-Za-z0-9_-]{6,32}):([yn])$")  # cabe nos 64 bytes do Telegram
_LEMBRETE = re.compile(r"^lem:(\d{1,9}):([fa])$")  # "Feito" / "Adiar 1 h" no aviso de lembrete
_LINK = re.compile(r"^https?://\S+$")
CAPTURA_ESPERA_S = 300  # depois do `/capturar` sozinho, o próximo envio vale por 5 minutos

BOAS_VINDAS = (
    "Orion aqui. Escreva o que precisa e eu respondo.\n\n"
    "/nova — começa uma conversa nova (a atual fica arquivada)\n"
    "/ler <link> — guarda para ler depois (resumo de 5 linhas no vault)\n"
    "/capturar — guarda no vault o próximo texto, link, foto ou voz (ou /capturar <texto>)\n"
    "/briefing — o que há para hoje\n"
    "/painel — modelos, CLIs, aprovações e política num só lugar\n"
    "/tela — memória da tela: estado, pausar, retomar ou limpar\n"
    "/camera [pergunta] — uma foto da webcam, descrita (pede aprovação no app)\n"
    "/panico — corta rede, execução, tela e escuta; /panico sair para voltar\n"
    "/ajuda — mostra isto de novo\n\n"
    "Ações que mexem no computador chegam aqui com botões para aprovar ou negar."
)


class TelegramError(RuntimeError):
    """Erro da API do Telegram. Nunca carrega a URL (ela contém o token)."""

    def __init__(self, mensagem: str, codigo: int | None = None) -> None:
        super().__init__(mensagem)
        self.codigo = codigo


class TelegramChannel:
    def __init__(
        self,
        *,
        token: str,
        allowed_users: Sequence[int],
        agent: Agent,
        memory: MemoryStore,
        approvals: ApprovalStore,
        ops: Operations,
        client: httpx.AsyncClient | None = None,
        transcriber: Transcriber | None = None,
        capture: Capturer | None = None,
        clock: Callable[[], float] = time.monotonic,
        base_url: str = API,
        poll_timeout_s: int = 25,
        notify_every_s: float = 15.0,
        sleep: Callable[[float], Coroutine[Any, Any, None]] = asyncio.sleep,
    ) -> None:
        if not token:
            raise ValueError("token do bot vazio")
        if not allowed_users:
            raise ValueError("lista de usuários vazia: o canal não sobe aberto (default-deny)")
        self._token = token
        self._allowed = frozenset(int(u) for u in allowed_users)
        self._home = int(allowed_users[0])  # onde caem os avisos
        self.agent, self.memory, self.approvals, self.ops = agent, memory, approvals, ops
        self._dono_do_cliente = client is None
        self._client = client or httpx.AsyncClient()
        self._base = base_url.rstrip("/")
        self._poll_timeout = poll_timeout_s
        self._notify_every = notify_every_s
        self._sleep = sleep
        self._transcriber = transcriber
        self._capture = capture
        self.painel: Callable[[], str] | None = None  # texto do /painel (o app liga depois)
        # agenda do briefing (REGRAS.md: agenda do briefing é leitura fixa, sem modelo)
        self.agenda: Callable[[float], str | None] | None = None
        # `/tela` (REGRAS.md: memória da tela: só texto, local e com opt-in); o app liga depois
        self.tela: Callable[[str], str] | None = None
        # `/camera` (REGRAS.md: a câmera só liga por pedido seu, e cada uso fica registrado); só com
        # ORION_CAMERA
        self.camera: Callable[[str], str] | None = None
        # `/panico` (REGRAS.md: pânico é corte; não perturbe só segura); o app liga depois
        self.modos: Modos | None = None
        self.desligar: Callable[[], str] | None = None  # `/desligar` (E9.5); o app liga depois
        # E9.9: (nome, bytes, formato) → (nome novo, bytes); levanta ConversaoError. O app liga.
        self.converter: Callable[[str, bytes, str], tuple[str, bytes]] | None = None
        # E9.10: quem manda voz recebe voz; o app liga com ORION_TELEGRAM_VOICE_REPLY
        self.speaker: Any = None  # tem `async falar(texto) -> bytes` (mp3)
        self.ler_depois: Any = None  # LerDepois (E9.8); o app liga depois
        self.vida: Any = None  # VidaCanal (E10): /financas, /contas, /foco, /revisao, gastos
        # RotinaStore (REGRAS.md: automação sua vale pelo arquivo concedido); o app liga depois
        self.rotinas: Any = None
        self.executar_rotina: Callable[[str, str], Awaitable[Any]] | None = None
        self._clock = clock
        self._armados: dict[int, float] = {}  # chat -> até quando o próximo envio vira nota
        self._offset = 0
        self._tarefas: set[asyncio.Task[None]] = set()
        self._recusados: set[int] = set()

    async def aclose(self) -> None:
        if self._dono_do_cliente:
            await self._client.aclose()
        if self._transcriber is not None:
            await self._transcriber.aclose()

    def _panico(self, argumento: str) -> str:
        """`/panico` liga; `/panico sair` desliga (comando explícito do usuário da lista, em
        conversa privada: REGRAS.md: Telegram com default-deny e aprovação só por botão; pânico é
        corte; não perturbe só segura); `/panico estado` só mostra."""
        if self.modos is None:
            return "Modo pânico indisponível neste canal."
        acao = argumento.strip().lower()
        if acao == "estado":
            return "Modo pânico LIGADO." if self.modos.panico() else "Modo pânico desligado."
        if acao == "sair":
            try:
                saiu = self.modos.sair_panico("telegram")
            except ModoError as e:
                return str(e)
            return "Modo pânico desligado: tudo volta ao normal." if saiu else "Não estava ligado."
        if acao:
            return "Use /panico, /panico sair ou /panico estado."
        self.modos.entrar_panico("telegram")
        return (
            "🚨 Modo pânico LIGADO: ferramentas de rede e de execução cortadas, memória da tela, "
            "escuta e jobs de rede parados. Nada volta sozinho: /panico sair para desligar."
        )

    # ── API do Telegram ───────────────────────────────────────────────────
    async def _api(self, metodo: str, **payload: Any) -> Any:
        url = f"{self._base}/bot{self._token}/{metodo}"
        # o que leva texto seu para fora (enviar, editar, responder) entra no registro de saída
        # (REGRAS.md: toda saída de dado fica registrada, sem conteúdo); a escuta (getUpdates) e o
        # download (getFile) não levam nada daqui
        medir = (
            saidas.medir(
                "telegram",
                "canal",
                bytes_out=len(json.dumps(payload, ensure_ascii=False).encode()),
            )
            if metodo.startswith(_METODOS_DE_SAIDA)
            else contextlib.nullcontext(saidas.Medida())
        )
        with medir as m:
            try:
                resp = await self._client.post(url, json=payload, timeout=self._poll_timeout + 15)
            except httpx.HTTPError as e:
                raise TelegramError(f"{metodo}: {type(e).__name__}") from None  # sem a URL
            m.ok, m.bytes_in = resp.status_code < 400, len(resp.content)
        try:
            corpo = resp.json()
        except ValueError:
            corpo = {}
        if not isinstance(corpo, dict):
            corpo = {}
        if resp.status_code >= 400 or not corpo.get("ok"):
            raise TelegramError(
                f"{metodo}: HTTP {resp.status_code} {str(corpo.get('description', ''))[:120]}",
                resp.status_code,
            )
        return corpo.get("result")

    async def _baixar(self, file_id: str, limite: int) -> tuple[bytes, str]:
        """Baixa um arquivo do Telegram (até `limite` bytes). Devolve (conteúdo, caminho remoto).
        O token vai na URL de download: nenhum erro carrega a URL."""
        info = await self._api("getFile", file_id=file_id)
        caminho = str((info or {}).get("file_path", ""))
        if not _CAMINHO_ARQUIVO.match(caminho) or ".." in caminho:
            raise TelegramError("getFile: caminho de arquivo inesperado")
        if int((info or {}).get("file_size") or 0) > limite:
            raise TelegramError("arquivo grande demais")
        try:
            resp = await self._client.get(
                f"{self._base}/file/bot{self._token}/{caminho}", timeout=self._poll_timeout + 15
            )
        except httpx.HTTPError as e:
            raise TelegramError(f"download: {type(e).__name__}") from None
        if resp.status_code >= 400:
            raise TelegramError(f"download: HTTP {resp.status_code}", resp.status_code)
        if len(resp.content) > limite:
            raise TelegramError("arquivo grande demais")
        return resp.content, caminho

    async def _enviar(
        self, chat_id: int, texto: str, teclado: list[list[dict]] | None = None
    ) -> int | None:
        """Envia em pedaços de até 4000 caracteres; o teclado vai no último. Devolve o id dele."""
        ultimo = None
        pedacos = _dividir(texto) or ["(vazio)"]
        for i, pedaco in enumerate(pedacos):
            extra: dict[str, Any] = {}
            if teclado and i == len(pedacos) - 1:
                extra["reply_markup"] = {"inline_keyboard": teclado}
            r = await self._api("sendMessage", chat_id=chat_id, text=pedaco, **extra)
            ultimo = (r or {}).get("message_id")
        return ultimo

    # ── laço principal ────────────────────────────────────────────────────
    async def run(self) -> None:
        """Roda até ser cancelado. Token inválido encerra (nada a tentar de novo)."""
        try:
            eu = await self._api("getMe")
        except TelegramError as e:
            log.error("telegram não iniciou: %s", e)
            return
        log.info(
            "telegram: bot @%s, %d usuário(s) permitido(s)",
            (eu or {}).get("username"),
            len(self._allowed),
        )
        avisos = asyncio.create_task(self._laco_avisos())
        espera = 2.0
        try:
            while True:
                try:
                    await self.poll_once()
                    espera = 2.0
                    await asyncio.sleep(0)  # cede o laço mesmo se o servidor responder sem esperar
                except TelegramError as e:
                    if e.codigo == 409:
                        log.error("telegram: outro cliente usa este token (pare o bot do legado)")
                    elif e.codigo == 401:
                        log.error("telegram: token recusado; canal encerrado")
                        return
                    else:
                        log.warning("telegram: %s; nova tentativa em %.0fs", e, espera)
                    await self._sleep(espera)
                    espera = min(espera * 2, 60.0)
        finally:
            avisos.cancel()
            for t in list(self._tarefas):
                t.cancel()
            await asyncio.gather(avisos, *self._tarefas, return_exceptions=True)

    async def poll_once(self) -> int:
        """Um `getUpdates`; cada atualização vira uma tarefa (um turno longo não trava o resto)."""
        atualizacoes = await self._api(
            "getUpdates",
            offset=self._offset,
            timeout=self._poll_timeout,
            allowed_updates=["message", "callback_query"],
        )
        for u in atualizacoes or []:
            self._offset = max(self._offset, int(u.get("update_id", 0)) + 1)
            self._spawn(self.handle_update(u))
        return len(atualizacoes or [])

    def _spawn(self, coro: Coroutine[Any, Any, None]) -> asyncio.Task[None]:
        tarefa = asyncio.create_task(coro)
        self._tarefas.add(tarefa)
        tarefa.add_done_callback(self._fim_da_tarefa)
        return tarefa

    def _fim_da_tarefa(self, tarefa: asyncio.Task[None]) -> None:
        self._tarefas.discard(tarefa)
        if not tarefa.cancelled() and (erro := tarefa.exception()):
            log.error("telegram: tratamento de atualização falhou: %s", type(erro).__name__)

    async def _laco_avisos(self) -> None:
        while True:
            await self._sleep(self._notify_every)
            try:
                await self.deliver_notifications()
            except Exception as e:  # noqa: BLE001 — o laço de avisos nunca morre
                log.warning("telegram: entrega de avisos falhou: %s", type(e).__name__)

    async def deliver_notifications(self) -> int:
        """Entrega a fila de avisos; só confirma o que foi enviado (falha: tenta na próxima)."""
        enviados = 0
        for n in await asyncio.to_thread(self.ops.pending_notifications):
            try:
                await self._enviar(self._home, f"🔔 {n['text']}", _teclado_lembrete(n))
            except TelegramError as e:
                log.warning("telegram: aviso %s não enviado: %s", n["id"], e)
                break
            await asyncio.to_thread(self.ops.ack_notification, n["id"])
            enviados += 1
        return enviados

    # ── atualizações ──────────────────────────────────────────────────────
    async def handle_update(self, u: dict[str, Any]) -> None:
        if "callback_query" in u:
            await self._on_callback(u["callback_query"])
        elif "message" in u:
            await self._on_message(u["message"])

    def _permitido(self, user_id: Any, chat: dict[str, Any]) -> bool:
        if chat.get("type") == "private" and user_id in self._allowed:
            return True
        if isinstance(user_id, int) and user_id not in self._recusados:
            self._recusados.add(user_id)  # um aviso por pessoa, sem encher o log
            log.warning(
                "telegram: usuário %s recusado (fora da lista ou fora de conversa privada)", user_id
            )
        return False

    async def _on_message(self, m: dict[str, Any]) -> None:
        chat = m.get("chat") or {}
        if not self._permitido((m.get("from") or {}).get("id"), chat):
            return
        chat_id = int(chat["id"])
        texto = (m.get("text") or "").strip()
        comando = texto.split()[0].split("@")[0].lower() if texto else ""
        if comando == "/capturar":
            await self._on_capturar(chat_id, texto)
            return
        if texto.startswith("/"):
            self._armados.pop(chat_id, None)  # outro comando cancela a captura que esperava
        elif self._armados.pop(chat_id, 0.0) > self._clock():
            await self._capturar_mensagem(chat_id, m, texto)
            return
        if not texto and (m.get("voice") or m.get("audio")):
            await self._on_voz(chat_id, m)
            return
        if not texto and m.get("photo"):
            await self._on_foto(chat_id, m)
            return
        if not texto and m.get("document"):
            await self._on_documento(chat_id, m)
            return
        if not texto:
            await self._enviar(chat_id, "Entendo texto, voz e foto por aqui.")
            return
        if self.vida is not None:
            resposta_vida = await self.vida.tratar_async(chat_id, texto)
            if resposta_vida is not None:  # E10: é da vida pessoal; o resto segue para o agente
                await self._enviar(chat_id, resposta_vida)
                return
        if comando in ("/start", "/ajuda"):
            await self._enviar(chat_id, BOAS_VINDAS)
        elif comando == "/painel":
            ver = self.painel
            await self._enviar(
                chat_id,
                await asyncio.to_thread(ver) if ver else "Painel indisponível neste canal.",
            )
        elif comando == "/tela":
            resposta = self.tela
            argumento = texto.split(None, 1)[1] if len(texto.split(None, 1)) > 1 else ""
            await self._enviar(
                chat_id,
                await asyncio.to_thread(resposta, argumento)
                if resposta
                else "Memória da tela indisponível neste canal.",
            )
        elif comando == "/camera":
            resposta = self.camera
            argumento = texto.split(None, 1)[1] if len(texto.split(None, 1)) > 1 else ""
            await self._enviar(
                chat_id,
                await asyncio.to_thread(resposta, argumento)
                if resposta
                else "Câmera desligada (ORION_CAMERA).",
            )
        elif comando == "/panico":
            argumento = texto.split(None, 1)[1] if len(texto.split(None, 1)) > 1 else ""
            await self._enviar(chat_id, await asyncio.to_thread(self._panico, argumento))
        elif comando == "/desligar":
            ver = self.desligar
            await self._enviar(
                chat_id,
                await asyncio.to_thread(ver) if ver else "Conferência indisponível neste canal.",
            )
        elif comando == "/ler":
            argumento = texto.split(None, 1)[1].strip() if len(texto.split(None, 1)) > 1 else ""
            await self._enviar(chat_id, await self._ler_depois(argumento))
        elif comando == "/rotina":
            argumento = texto.split(None, 1)[1].strip() if len(texto.split(None, 1)) > 1 else ""
            await self._on_rotina(chat_id, argumento)
        elif comando == "/briefing":
            await self._enviar(chat_id, await asyncio.to_thread(self._briefing))
        elif comando == "/nova":
            atual = await asyncio.to_thread(self.memory.active_session, CANAL)
            await asyncio.to_thread(self.memory.archive_session, atual.id)
            await self._enviar(chat_id, "Conversa nova. A anterior ficou arquivada.")
        elif self.rotinas is not None and (c := self.rotinas.por_frase(texto)) is not None:
            await self._on_rotina(chat_id, c.nome)  # a frase é exatamente a de uma rotina sua
        else:
            await self._api("sendChatAction", chat_id=chat_id, action="typing")
            await self._consumir(chat_id, self.agent.run(CANAL, texto[:LIMITE_ENTRADA]))

    async def _on_rotina(self, chat_id: int, nome: str) -> None:
        """`/rotina` lista as suas; `/rotina <nome>` roda uma concedida (REGRAS.md: automação sua
        vale pelo arquivo concedido). Passo que pede
        aprovação vira o cartão de sempre aqui mesmo, e a rotina para nele."""
        if self.rotinas is None or self.executar_rotina is None:
            await self._enviar(chat_id, "Rotinas indisponíveis neste canal.")
            return
        if not nome:
            lista = await asyncio.to_thread(self.rotinas.listar)
            linhas = [f"• {c.nome} — {c.estado.replace('_', ' ')}" for c in lista]
            await self._enviar(
                chat_id,
                "Suas rotinas:\n" + "\n".join(linhas)
                if linhas
                else "Nenhuma rotina em <dados>/rotinas.",
            )
            return
        r = await self.executar_rotina(nome, "telegram")
        icone = {"concluida": "✅", "aguardando_aprovacao": "⏸", "recusada": "⛔"}.get(
            r.status, "⚠️"
        )
        resumo = f"{icone} Rotina {nome}: {r.status.replace('_', ' ')}" + (
            f" — {r.motivo}" if r.motivo else ""
        )
        await self._enviar(chat_id, resumo)
        if r.aprovacao_id:
            a = self.approvals.get(r.aprovacao_id)
            if a is not None:
                cartao = {
                    "id": a.id,
                    "tool": a.tool,
                    "reason": a.reason,
                    "args": redact(a.args, limite=2000),
                }
                texto_cartao, revisavel = _cartao(cartao)
                await self._enviar(chat_id, texto_cartao, _teclado(str(a.id), aprovar=revisavel))

    def _briefing(self) -> str:
        return build_briefing(self.ops, self.memory.clock(), agenda=self.agenda)

    # ── captura rápida (REGRAS.md: captura rápida e briefing não passam pelo modelo): comando do
    # Antônio, não ferramenta do modelo ──
    async def _on_documento(self, chat_id: int, m: dict[str, Any]) -> None:
        """E9.9: arquivo com a legenda "converte para pdf" volta convertido (tudo local)."""
        from ..conversor import ConversaoError, formatos_para

        doc = m.get("document") or {}
        legenda = str(m.get("caption") or "")
        nome = str(doc.get("file_name") or "arquivo")
        achou = _CONVERTE.search(legenda)
        if self.converter is None:
            await self._enviar(chat_id, "Conversão indisponível neste canal.")
            return
        if achou is None:
            ext = nome.rsplit(".", 1)[-1] if "." in nome else ""
            opcoes = ", ".join(formatos_para(ext))
            await self._enviar(
                chat_id,
                "Mande o arquivo com a legenda “converte para <formato>”."
                + (f" Para .{ext} posso: {opcoes}." if opcoes else " Não sei converter esse tipo."),
            )
            return
        if int(doc.get("file_size") or 0) > MAX_DOCUMENTO:
            await self._enviar(chat_id, "Arquivo grande demais (o Telegram libera até 20 MB).")
            return
        await self._api("sendChatAction", chat_id=chat_id, action="upload_document")
        try:
            dados, _ = await self._baixar(str(doc.get("file_id", "")), MAX_DOCUMENTO)
            novo, saida = await asyncio.to_thread(self.converter, nome, dados, achou.group(1))
        except (TelegramError, ConversaoError) as e:
            await self._enviar(chat_id, f"⚠️ Não converti: {e}")
            return
        await self._enviar_documento(chat_id, novo, saida)

    async def _enviar_documento(self, chat_id: int, nome: str, dados: bytes) -> None:
        await self._enviar_arquivo("sendDocument", "document", chat_id, nome, dados)

    async def _enviar_arquivo(
        self, metodo: str, campo: str, chat_id: int, nome: str, dados: bytes
    ) -> None:
        url = f"{self._base}/bot{self._token}/{metodo}"
        with saidas.medir("telegram", "canal", bytes_out=len(dados)) as med:
            try:
                resp = await self._client.post(
                    url,
                    data={"chat_id": str(chat_id)},
                    files={campo: (nome, dados)},
                    timeout=self._poll_timeout + 60,
                )
            except httpx.HTTPError as e:
                raise TelegramError(f"{metodo}: {type(e).__name__}") from None
            med.ok, med.bytes_in = resp.status_code < 400, len(resp.content)
        if resp.status_code >= 400:
            raise TelegramError(f"{metodo}: HTTP {resp.status_code}", resp.status_code)

    async def _ler_depois(self, link: str) -> str:
        """`/ler <link>` (E9.8): guarda na fila, resume em segundo plano e deixa a nota no vault."""
        if self.ler_depois is None:
            return "Ler depois indisponível neste canal."
        if not link:
            lista = await asyncio.to_thread(self.ler_depois.listar, "pendente", 10)
            if not lista:
                return "Nada na fila. Use /ler <link>."
            return "Na fila para ler:\n" + "\n".join(f"• {i['title'] or i['url']}" for i in lista)
        try:
            item = await asyncio.to_thread(self.ler_depois.adicionar, link)
        except ValueError as e:
            return str(e)
        if item["status"] == "pendente":
            self.ler_depois.agendar(item["id"])
            return "📚 Guardei. Vou resumir e deixar a nota em 'Ler depois' no vault."
        return f"Esse link já estava na lista ({item['status']})."

    async def _on_capturar(self, chat_id: int, texto: str) -> None:
        if self._capture is None:
            await self._enviar(
                chat_id, "Captura desligada: defina ORION_VAULT_DIR (a pasta do seu vault)."
            )
            return
        partes = texto.split(None, 1)
        if len(partes) > 1 and partes[1].strip():
            self._armados.pop(chat_id, None)
            await self._guardar_texto(chat_id, partes[1].strip())
            return
        self._armados[chat_id] = self._clock() + CAPTURA_ESPERA_S
        await self._enviar(
            chat_id,
            "Pode mandar: texto, link, foto ou voz. A próxima mensagem vira nota no vault "
            f"(vale por {CAPTURA_ESPERA_S // 60} minutos; qualquer comando cancela).",
        )

    async def _capturar_mensagem(self, chat_id: int, m: dict[str, Any], texto: str) -> None:
        if texto:
            await self._guardar_texto(chat_id, texto)
        elif m.get("voice") or m.get("audio"):
            entendido = await self._transcrever_voz(chat_id, m)
            if entendido is not None:
                await self._enviar(chat_id, f"🎙️ Entendi: {entendido}")
                await self._guardar_texto(chat_id, entendido, origem="voz")
        elif m.get("photo"):
            baixada = await self._baixar_foto(chat_id, m)
            if baixada is not None:
                imagem, caminho = baixada
                ext = "." + caminho.rsplit(".", 1)[-1].lower() if "." in caminho else ".jpg"
                legenda = (m.get("caption") or "").strip()
                await self._guardar(
                    chat_id, self._capture_foto, imagem, ext if ext in EXTENSOES_FOTO else ".jpg",
                    legenda,
                )  # fmt: skip
        else:
            await self._enviar(chat_id, "Só guardo texto, link, foto ou voz.")

    def _capture_foto(self, imagem: bytes, ext: str, legenda: str) -> str:
        assert self._capture is not None
        return self._capture.relativo(self._capture.save_photo(imagem, ext, legenda))

    def _capture_texto(self, texto: str, origem: str) -> str:
        assert self._capture is not None
        return self._capture.relativo(self._capture.save_text(texto, origem))

    async def _guardar_texto(self, chat_id: int, texto: str, origem: str = "") -> None:
        origem = origem or ("link" if _LINK.match(texto) else "texto")
        await self._guardar(chat_id, self._capture_texto, texto[: LIMITE_ENTRADA * 2], origem)

    async def _guardar(self, chat_id: int, gravar: Callable[..., str], *args: Any) -> None:
        try:
            relativo = await asyncio.to_thread(gravar, *args)
        except (CaptureError, OSError) as e:
            log.warning("telegram: captura não gravada: %s", type(e).__name__)
            await self._enviar(chat_id, f"⚠️ Não guardei: {e}")
            return
        await self._enviar(chat_id, f"📝 Guardado no vault: {relativo}")

    async def _transcrever_voz(self, chat_id: int, m: dict[str, Any]) -> str | None:
        """Baixa e transcreve a voz; em qualquer falha avisa o Antônio e devolve None."""
        if self._transcriber is None:
            await self._enviar(
                chat_id,
                "Ainda não transcrevo áudio (falta ORION_TRANSCRIBE_API_KEY). Escreva, por favor.",
            )
            return None
        voz = m.get("voice") or m.get("audio") or {}
        if int(voz.get("duration") or 0) > MAX_VOZ_S:
            await self._enviar(chat_id, f"Áudio longo demais (máximo {MAX_VOZ_S // 60} minutos).")
            return None
        await self._api("sendChatAction", chat_id=chat_id, action="typing")
        try:
            audio, caminho = await self._baixar(str(voz.get("file_id", "")), MAX_AUDIO)
            nome = caminho.rsplit("/", 1)[-1] or "voz.ogg"
            texto = await self._transcriber.transcribe(
                audio, nome, str(voz.get("mime_type") or "audio/ogg")
            )
        except (TelegramError, TranscribeError) as e:
            log.warning("telegram: voz não transcrita: %s", e)
            await self._enviar(chat_id, f"⚠️ Não consegui entender o áudio: {e}")
            return None
        return texto[:LIMITE_ENTRADA]

    async def _on_voz(self, chat_id: int, m: dict[str, Any]) -> None:
        texto = await self._transcrever_voz(chat_id, m)
        if texto is None:
            return
        # mostra o que foi entendido: quem falou confere (e aprovação continua só por botão)
        await self._enviar(chat_id, f"🎙️ Entendi: {texto}")
        resposta = await self._consumir(chat_id, self.agent.run(CANAL, texto))
        await self._responder_em_audio(chat_id, resposta)

    async def _responder_em_audio(self, chat_id: int, resposta: str) -> None:
        """E9.10: só com ORION_TELEGRAM_VOICE_REPLY. O texto da resposta vai ao serviço de voz
        (edge-tts, Microsoft), já registrado na saída; falha da voz nunca esconde o texto."""
        if self.speaker is None or not resposta:
            return
        from ..voice import SpeakError, falavel

        fala = falavel(resposta)
        if not fala:
            return
        try:
            audio = await self.speaker.falar(fala)
            await self._api("sendChatAction", chat_id=chat_id, action="upload_voice")
            await self._enviar_arquivo("sendAudio", "audio", chat_id, "resposta.mp3", audio)
        except (SpeakError, TelegramError) as e:
            log.warning("telegram: resposta em áudio falhou: %s", e)

    async def _baixar_foto(self, chat_id: int, m: dict[str, Any]) -> tuple[bytes, str] | None:
        """O maior tamanho da foto que couber no limite; falha avisa e devolve None."""
        tamanhos = [p for p in m.get("photo") or [] if isinstance(p, dict) and p.get("file_id")]
        cabem = [p for p in tamanhos if int(p.get("file_size") or 0) <= MAX_FOTO] or tamanhos[:1]
        if not cabem:
            await self._enviar(chat_id, "Não consegui ler essa foto.")
            return None
        await self._api("sendChatAction", chat_id=chat_id, action="typing")
        try:
            return await self._baixar(str(cabem[-1]["file_id"]), MAX_FOTO)
        except TelegramError as e:
            log.warning("telegram: foto não baixada: %s", e)
            await self._enviar(chat_id, f"⚠️ Não consegui baixar a foto: {e}")
            return None

    async def _on_foto(self, chat_id: int, m: dict[str, Any]) -> None:
        baixada = await self._baixar_foto(chat_id, m)
        if baixada is None:
            return
        imagem, caminho = baixada
        ext = "." + caminho.rsplit(".", 1)[-1].lower() if "." in caminho else ".jpg"
        mime = _MIME_FOTO.get(ext, "image/jpeg")
        url = f"data:{mime};base64,{base64.b64encode(imagem).decode('ascii')}"
        legenda = (m.get("caption") or "").strip() or "O que você vê nesta imagem?"
        await self._consumir(chat_id, self.agent.run(CANAL, legenda[:LIMITE_ENTRADA], [url]))

    async def _consumir(self, chat_id: int, eventos: AsyncIterator[AgentEvent]) -> str:
        """Junta o turno do agente e responde: texto, depois um cartão por aprovação pedida.
        Devolve o texto da resposta (a voz do Telegram, E9.10, fala o mesmo texto)."""
        texto: list[str] = []
        cartoes: list[dict[str, Any]] = []
        erros: list[str] = []
        try:
            async for ev in eventos:
                if ev.kind == "text":
                    texto.append(str(ev.data.get("text", "")))
                elif ev.kind == "approval":
                    cartoes.append(ev.data)
                elif ev.kind == "error":
                    erros.append(str(ev.data.get("message", "erro")))
        except Exception as e:  # noqa: BLE001 — falha do turno vira mensagem, não derruba o canal
            log.error("telegram: turno do agente falhou: %s", type(e).__name__)
            erros.append("falha interna no turno")
        resposta = "".join(texto).strip()
        if resposta:
            await self._enviar(chat_id, resposta)
        for c in cartoes:
            texto_cartao, revisavel = _cartao(c)
            await self._enviar(chat_id, texto_cartao, _teclado(str(c["id"]), aprovar=revisavel))
        for e in erros:
            await self._enviar(chat_id, f"⚠️ {e}")
        if not (resposta or cartoes or erros):
            await self._enviar(chat_id, "(sem resposta)")
        return resposta

    async def _on_callback(self, cb: dict[str, Any]) -> None:
        msg = cb.get("message") or {}
        chat = msg.get("chat") or {}
        uid = (cb.get("from") or {}).get("id")
        if not self._permitido(uid, chat) or "id" not in cb:
            return
        chat_id = int(chat["id"])
        lembrete = _LEMBRETE.match(str(cb.get("data", "")))
        if lembrete:
            await self._on_lembrete(cb, chat_id, msg, int(lembrete.group(1)), lembrete.group(2))
            return
        casou = _CALLBACK.match(str(cb.get("data", "")))
        if not casou:
            await self._api(
                "answerCallbackQuery", callback_query_id=cb["id"], text="Botão inválido."
            )
            return
        aprovacao_id, resposta = casou.group(1), casou.group(2) == "y"
        a = self.approvals.get(aprovacao_id)
        sessao = self.memory.get_session(a.session_id) if a else None
        if a is None or sessao is None or sessao.channel != CANAL:
            await self._fechar_botoes(cb, chat_id, msg, "Aprovação inexistente ou de outro canal.")
            return
        if resposta and not _revisavel(
            a.args
        ):  # o cartão nem oferece o botão; confere de novo aqui
            await self._fechar_botoes(
                cb, chat_id, msg, "Argumentos grandes demais: só dá para negar."
            )
            return
        try:
            self.approvals.decide(aprovacao_id, resposta, channel=CANAL, actor=f"telegram:{uid}")
        except (KeyError, ValueError):
            await self._fechar_botoes(cb, chat_id, msg, "Já decidida ou expirada.")
            return
        await self._fechar_botoes(cb, chat_id, msg, "Aprovado ✅" if resposta else "Negado ❌")
        if resposta:
            await self._consumir(chat_id, self.agent.resume(CANAL, aprovacao_id))
        else:
            await self._enviar(chat_id, f"Ação negada: {a.tool}. Não foi executada.")

    async def _on_lembrete(
        self, cb: dict[str, Any], chat_id: int, msg: dict[str, Any], rid: int, acao: str
    ) -> None:
        """ "Feito" conclui o lembrete e "Adiar 1 h" o marca para daqui a uma hora (E9.3)."""
        try:
            if acao == "f":
                await asyncio.to_thread(self.ops.complete_reminder, rid)
                texto = "Feito ✅"
            else:
                await asyncio.to_thread(self.ops.snooze_reminder, rid, 60)
                texto = "Adiado 1 h ⏳"
        except (KeyError, ValueError):
            texto = "Esse lembrete não existe mais."
        await self._fechar_botoes(cb, chat_id, msg, texto)

    async def _fechar_botoes(
        self, cb: dict[str, Any], chat_id: int, msg: dict[str, Any], texto: str
    ) -> None:
        """Responde ao clique e tira os botões (um cartão não se decide duas vezes)."""
        await self._api("answerCallbackQuery", callback_query_id=cb["id"], text=texto)
        if msg.get("message_id"):
            try:
                await self._api(
                    "editMessageReplyMarkup",
                    chat_id=chat_id,
                    message_id=msg["message_id"],
                    reply_markup={"inline_keyboard": []},
                )
            except TelegramError as e:  # mensagem antiga demais para editar: o clique já valeu
                log.info("telegram: botões não removidos: %s", e)


def _teclado_lembrete(n: dict[str, Any]) -> list[list[dict[str, str]]] | None:
    """Botões só nos avisos de lembrete (`ref` = `reminder:<id>` ou `reminder:<id>:n<k>`)."""
    achou = re.match(r"^reminder:(\d{1,9})(?::n\d+)?$", str(n.get("ref") or ""))
    if not achou:
        return None
    rid = achou.group(1)
    return [
        [
            {"text": "✅ Feito", "callback_data": f"lem:{rid}:f"},
            {"text": "⏳ Adiar 1 h", "callback_data": f"lem:{rid}:a"},
        ]
    ]


def _teclado(aprovacao_id: str, *, aprovar: bool = True) -> list[list[dict[str, str]]]:
    negar = {"text": "❌ Negar", "callback_data": f"ap:{aprovacao_id}:n"}
    if not aprovar:
        return [[negar]]
    return [[{"text": "✅ Aprovar", "callback_data": f"ap:{aprovacao_id}:y"}, negar]]


MAX_ARGS_REVISAVEL = 3000  # cabe numa mensagem do Telegram junto do resto do cartão


def _revisavel(args: dict[str, Any]) -> bool:
    """Dá para mostrar os argumentos inteiros no cartão (sem corte e dentro do limite)?"""
    mostrados = redact(args, limite=2000)
    if mostrados != redact(args, limite=10**9):
        return False
    return len(json.dumps(mostrados, ensure_ascii=False, indent=1)) <= MAX_ARGS_REVISAVEL


def _cartao(c: dict[str, Any]) -> tuple[str, bool]:
    """Texto do cartão e se dá para aprovar por aqui. Argumento cortado (ou grande demais para
    caber inteiro) esconderia o fim do comando de quem decide: então só dá para negar."""
    args = json.dumps(c.get("args", {}), ensure_ascii=False, indent=1)
    revisavel = not c.get("args_truncated") and len(args) <= MAX_ARGS_REVISAVEL
    if not revisavel:
        args = args[:MAX_ARGS_REVISAVEL] + "…"
    rodape = (
        "Vale por 10 minutos e uma única execução."
        if revisavel
        else "⛔ Os argumentos são grandes demais para revisar aqui, então só dá para negar. "
        "Se for legítimo, peça de novo em partes menores."
    )
    return (
        f"⚠️ Aprovar esta ação?\n\nFerramenta: {c.get('tool')}\nMotivo: {c.get('reason')}\n"
        f"Argumentos (segredos mascarados):\n{args}\n\n{rodape}",
        revisavel,
    )


def _dividir(texto: str, limite: int = LIMITE_MENSAGEM) -> list[str]:
    """Quebra em pedaços de até `limite`, preferindo fim de linha."""
    texto = texto.strip()
    pedacos: list[str] = []
    while len(texto) > limite:
        corte = texto.rfind("\n", 0, limite)
        corte = corte if corte > limite // 2 else limite
        pedacos.append(texto[:corte].rstrip())
        texto = texto[corte:].lstrip("\n")
    if texto:
        pedacos.append(texto)
    return pedacos

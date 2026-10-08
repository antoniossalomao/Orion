"""Captura rápida: o que o Antônio manda pelo celular vira nota na caixa de entrada do vault.

Não é ferramenta do modelo: é um comando do **próprio Antônio** no canal (`/capturar` no Telegram),
já autenticado pela lista de usuários. O modelo não decide o que gravar nem onde. Por isso não passa
pela política de risco, e por isso o destino é fixo e estreito (regra 30 do ORION_REGRAS.md):

- só dentro de `<vault>/<pasta de captura>` (padrão `00 Inbox`, a caixa de entrada do vault);
- o nome do arquivo é gerado aqui (data, hora e um resumo em ASCII); nada do texto vira caminho;
- um arquivo novo por captura, criado com `x` (nunca sobrescreve);
- texto até `MAX_TEXTO` e foto até `MAX_FOTO`; só jpg, png e webp.

A nota leva só o frontmatter que o Orion sabe (`date`, `hora`, `fonte`, `tags`): sem `type`, porque
captura ainda não foi triada (o vault classifica quando vira Projeto, Área ou Conceito). A foto vai
para `<pasta>/anexos/` e a nota a incorpora. O índice do vault (`orion.jobs`) lê a nota depois.
"""

from __future__ import annotations

import re
import time
import unicodedata
from collections.abc import Callable
from datetime import datetime
from pathlib import Path

MAX_TEXTO = 20_000
MAX_FOTO = 4 * 1024 * 1024
EXTENSOES_FOTO = (".jpg", ".png", ".webp")
_NAO_ALFANUM = re.compile(r"[^a-z0-9]+")


class CaptureError(RuntimeError):
    """A captura não pôde ser gravada (vault ausente, pasta fora do vault, conteúdo grande)."""


def _resumo(texto: str, padrao: str = "captura") -> str:
    """Até 6 palavras em ASCII minúsculo, para o nome do arquivo."""
    ascii_ = unicodedata.normalize("NFKD", texto).encode("ascii", "ignore").decode("ascii")
    palavras = [p for p in _NAO_ALFANUM.split(ascii_.lower()) if p][:6]
    return "-".join(palavras)[:50].strip("-") or padrao


class Capturer:
    def __init__(
        self,
        vault_dir: Path,
        pasta: str = "00 Inbox",
        clock: Callable[[], float] = time.time,
    ) -> None:
        self._vault = vault_dir
        self._pasta = pasta
        self._clock = clock

    def _destino(self) -> Path:
        vault = self._vault.expanduser().resolve()
        if not vault.is_dir():
            raise CaptureError(f"o vault não existe: {vault.name}")
        destino = (vault / self._pasta).resolve()
        if destino == vault or not destino.is_relative_to(vault):
            raise CaptureError("a pasta de captura precisa ficar dentro do vault")
        destino.mkdir(parents=True, exist_ok=True)
        return destino

    @staticmethod
    def _gravar(pasta: Path, base: str, extensao: str, dado: bytes) -> Path:
        """Cria `<base><extensao>` (ou `<base> (2)<extensao>`...) sem sobrescrever nada."""
        for n in range(1, 100):
            nome = f"{base}{extensao}" if n == 1 else f"{base} ({n}){extensao}"
            try:
                with (pasta / nome).open("xb") as f:
                    f.write(dado)
            except FileExistsError:
                continue
            return pasta / nome
        raise CaptureError("não consegui um nome livre para a nota")

    def _carimbo(self) -> tuple[str, str, str]:
        dt = datetime.fromtimestamp(self._clock())
        return f"{dt:%Y-%m-%d}", f"{dt:%H:%M}", f"{dt:%Y-%m-%d %H%M}"

    def save_text(self, texto: str, origem: str = "texto", fonte: str = "telegram") -> Path:
        """Grava uma nota com o texto (ou link). `origem`: texto, link, voz ou pesquisa (vai nas
        tags); `fonte`: quem mandou (telegram, ou orion para a pesquisa noturna)."""
        texto = texto.strip()
        if not texto:
            raise CaptureError("nada para guardar")
        if len(texto) > MAX_TEXTO:
            raise CaptureError(f"texto passa de {MAX_TEXTO} caracteres")
        data, hora, carimbo = self._carimbo()
        tag = origem if origem in ("texto", "link", "voz", "pesquisa") else "texto"
        fonte = fonte if fonte in ("telegram", "orion") else "telegram"
        nota = (
            f"---\ndate: {data}\nhora: {hora}\nfonte: {fonte}\ntags: [captura, {tag}]\n---\n\n"
            f"{texto}\n"
        )
        return self._gravar(
            self._destino(), f"{carimbo} - {_resumo(texto)}", ".md", nota.encode("utf-8")
        )

    def save_photo(self, imagem: bytes, extensao: str, legenda: str = "") -> Path:
        """Grava a foto em `anexos/` e uma nota que a incorpora (com a legenda, se houver)."""
        extensao = extensao.lower()
        if extensao not in EXTENSOES_FOTO:
            raise CaptureError(f"formato de imagem não suportado: {extensao or '(nenhum)'}")
        if not imagem:
            raise CaptureError("imagem vazia")
        if len(imagem) > MAX_FOTO:
            raise CaptureError(f"imagem passa de {MAX_FOTO // 1024 // 1024} MB")
        legenda = legenda.strip()[:MAX_TEXTO]
        data, hora, carimbo = self._carimbo()
        pasta = self._destino()
        anexos = pasta / "anexos"
        anexos.mkdir(exist_ok=True)
        foto = self._gravar(anexos, f"{carimbo} - {_resumo(legenda, 'foto')}", extensao, imagem)
        nota = (
            f"---\ndate: {data}\nhora: {hora}\nfonte: telegram\ntags: [captura, foto]\n---\n\n"
            f"![[{foto.name}]]\n" + (f"\n{legenda}\n" if legenda else "")
        )
        return self._gravar(pasta, f"{carimbo} - {_resumo(legenda, 'foto')}", ".md", nota.encode())

    def relativo(self, caminho: Path) -> str:
        """Caminho da nota relativo ao vault (é o que se mostra no chat, nunca o absoluto)."""
        return caminho.relative_to(self._vault.expanduser().resolve()).as_posix()

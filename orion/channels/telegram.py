"""Canal Telegram (fase 5 do NUCLEO): o Orion no celular, sem expor porta.

Long polling (`getUpdates`): o notebook só faz chamadas de saída, não precisa de IP
público nem de Tailscale para este canal. O que o canal garante:

- **default-deny**: só responde a usuários da lista, em conversa privada; qualquer outro
  é ignorado em silêncio (só vai para o log), para o bot não revelar que existe;
- **aprovação fora de banda** (regra 2): ação que a política manda confirmar vira uma
  mensagem com botões ✅/❌; só o clique de um usuário permitido decide, nunca uma frase;
- **avisos**: lembretes e agendamentos da fila (`orion.memory.ops`) saem aqui e só são
  confirmados (`ack`) depois de enviados;
- **segredo**: o token vai na URL da API do Telegram, então nenhuma mensagem de erro ou
  log leva a URL (só o tipo do erro) e o logger do `httpx` fica em WARNING.

Voz e foto: a mensagem de voz é transcrita (Whisper, `orion.transcribe`) e o texto entendido é
mostrado antes da resposta, para quem falou conferir; a foto vai ao modelo como imagem **só
naquele turno** (o histórico guarda um aviso, não a imagem). Aprovação continua só por botão.

`/capturar` guarda a próxima mensagem (texto, link, foto ou voz) como nota no vault, sem passar
pelo modelo (`orion.capture`, regra 30); `/briefing` manda o resumo do dia (`orion.briefing`);
`/painel` manda o painel único (`orion.painel`).

Texto puro (sem `parse_mode`): resposta de modelo com `*` ou `_` não quebra a mensagem.
"""

from __future__ import annotations

import asyncio
import base64
import json
import logging
import re
import time
from collections.abc import AsyncIterator, Callable, Coroutine, Sequence
from typing import Any

import httpx

from ..agent import Agent, AgentEvent
from ..briefing import build_briefing
from ..capture import EXTENSOES_FOTO, CaptureError, Capturer
from ..memory import MemoryStore
from ..memory.ops import Operations
from ..policy import ApprovalStore, redact
from ..transcribe import MAX_AUDIO, TranscribeError, Transcriber

log = logging.getLogger("orion.telegram")

CANAL = "telegram"
API = "https://api.telegram.org"
LIMITE_MENSAGEM = 4000  # o Telegram aceita 4096; folga para o que o cliente conta diferente
LIMITE_ENTRADA = 8000  # o mesmo teto do POST /chat
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
_LINK = re.compile(r"^https?://\S+$")
CAPTURA_ESPERA_S = 300  # depois do `/capturar` sozinho, o próximo envio vale por 5 minutos

BOAS_VINDAS = (
    "Orion aqui. Escreva o que precisa e eu respondo.\n\n"
    "/nova — começa uma conversa nova (a atual fica arquivada)\n"
    "/capturar — guarda no vault o próximo texto, link, foto ou voz (ou /capturar <texto>)\n"
    "/briefing — o que há para hoje\n"
    "/painel — modelos, CLIs, aprovações e política num só lugar\n"
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

    # ── API do Telegram ───────────────────────────────────────────────────
    async def _api(self, metodo: str, **payload: Any) -> Any:
        url = f"{self._base}/bot{self._token}/{metodo}"
        try:
            resp = await self._client.post(url, json=payload, timeout=self._poll_timeout + 15)
        except httpx.HTTPError as e:
            raise TelegramError(f"{metodo}: {type(e).__name__}") from None  # sem a URL
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
                await self._enviar(self._home, f"🔔 {n['text']}")
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
        if not texto:
            await self._enviar(chat_id, "Entendo texto, voz e foto por aqui.")
            return
        if comando in ("/start", "/ajuda"):
            await self._enviar(chat_id, BOAS_VINDAS)
        elif comando == "/painel":
            ver = self.painel
            await self._enviar(
                chat_id,
                await asyncio.to_thread(ver) if ver else "Painel indisponível neste canal.",
            )
        elif comando == "/briefing":
            await self._enviar(chat_id, await asyncio.to_thread(self._briefing))
        elif comando == "/nova":
            atual = await asyncio.to_thread(self.memory.active_session, CANAL)
            await asyncio.to_thread(self.memory.archive_session, atual.id)
            await self._enviar(chat_id, "Conversa nova. A anterior ficou arquivada.")
        else:
            await self._api("sendChatAction", chat_id=chat_id, action="typing")
            await self._consumir(chat_id, self.agent.run(CANAL, texto[:LIMITE_ENTRADA]))

    def _briefing(self) -> str:
        return build_briefing(self.ops, self.memory.clock())

    # ── captura rápida (regra 30): comando do Antônio, não ferramenta do modelo ──
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
        await self._consumir(chat_id, self.agent.run(CANAL, texto))

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

    async def _consumir(self, chat_id: int, eventos: AsyncIterator[AgentEvent]) -> None:
        """Junta o turno do agente e responde: texto, depois um cartão por aprovação pedida."""
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

    async def _on_callback(self, cb: dict[str, Any]) -> None:
        msg = cb.get("message") or {}
        chat = msg.get("chat") or {}
        uid = (cb.get("from") or {}).get("id")
        if not self._permitido(uid, chat) or "id" not in cb:
            return
        chat_id = int(chat["id"])
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

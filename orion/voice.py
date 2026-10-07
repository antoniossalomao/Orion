"""Voz do Orion (fase 6): dois caminhos, com riscos diferentes.

**A — voz como canal do agente (`/ws/voz`).** O navegador manda uma fala gravada; o Orion
transcreve (Whisper no Groq), roda o turno do agente normalmente (memória, ferramentas, política,
audit) e fala a resposta (edge-tts, `pt-BR-AntonioNeural`). Nada muda na segurança: a aprovação de
uma ação continua só por botão (regra 2) — se o turno pede uma, a voz avisa e o cartão aparece.

**B — voz ao vivo (`/ws/voice`).** Ponte do microfone para o Gemini Live (voz Charon). O Gemini
conversa direto: **sem ferramentas, sem memória e sem acesso ao computador**; é só conversa.
Cada sessão vai para o audit, tem duração máxima e só abre com opt-in (o áudio vai ao Google).

**Não validado contra as APIs reais** (Groq, edge-tts, Gemini Live): os testes usam falsos
(ver ORION_MELHORIAS).
"""

from __future__ import annotations

import asyncio
import contextlib
import json
import logging
import re
from collections.abc import AsyncIterator
from contextlib import AbstractAsyncContextManager
from typing import Any, Protocol

from fastapi import WebSocket, WebSocketDisconnect

from .agent import Agent
from .transcribe import MAX_AUDIO, TranscribeError, Transcriber

log = logging.getLogger("orion.voice")

LIVE_MODELO = "gemini-2.5-flash-native-audio-latest"
LIVE_VOZ = "Charon"  # voz masculina pré-definida do Gemini
MAX_QUADRO_LIVE = 256 * 1024  # um pedaço de PCM do microfone tem ~8 KB; mais que isso é abuso
MAX_FALA_CHARS = 700  # o que se fala da resposta: o resto está na tela
AVISO_APROVACAO = "Preciso da sua aprovação. O cartão está na tela."

# O Gemini não recebe a persona inteira do agente (ela cita ferramentas que ele não tem).
PERSONA_AO_VIVO = (
    "[Orion] Assistente pessoal do Antônio. Identidade masculina: técnico, direto, não-servil; "
    "fale de si no masculino. Nesta conversa por voz você não tem acesso ao computador, a "
    "arquivos nem à memória: só conversa. Se ele pedir uma ação, diga que isso se faz pelo "
    "chat de texto. REGRAS DE VOZ: respostas curtas (no máximo 2 ou 3 frases), em português do "
    "Brasil, sem markdown, sem asteriscos, sem listas."
)


class SpeakError(RuntimeError):
    """Falha ao sintetizar a fala. A mensagem nunca leva o texto falado."""


class Speaker(Protocol):
    async def falar(self, texto: str) -> bytes: ...


# ── A: fala e escuta ──────────────────────────────────────────────────────


def tipo_do_audio(dados: bytes) -> tuple[str, str] | None:
    """(nome de arquivo, mime) pelo cabeçalho do áudio, ou None se não for um formato de fala.
    O navegador grava webm/opus (Chrome, Firefox) ou mp4 (Safari); o Groq decide pela extensão."""
    if dados[:4] == b"\x1a\x45\xdf\xa3":
        return "fala.webm", "audio/webm"
    if dados[:4] == b"OggS":
        return "fala.ogg", "audio/ogg"
    if dados[4:8] == b"ftyp":
        return "fala.m4a", "audio/mp4"
    if dados[:4] == b"RIFF" and dados[8:12] == b"WAVE":
        return "fala.wav", "audio/wav"
    if dados[:3] == b"ID3" or dados[:2] in (b"\xff\xfb", b"\xff\xf3"):
        return "fala.mp3", "audio/mpeg"
    if dados[:4] == b"fLaC":
        return "fala.flac", "audio/flac"
    return None


_CERCA = re.compile(r"```.*?```", re.S)
_LINK = re.compile(r"\[([^\]]*)\]\([^)]*\)")
_URL = re.compile(r"https?://\S+")
_MARCAS = re.compile(r"(?m)^\s{0,3}(#{1,6}|[-*+>]|\d+[.)])\s+")
_ENFASE = re.compile(r"[*_`~|]+")


def falavel(texto: str, limite: int = MAX_FALA_CHARS) -> str:
    """O texto da resposta sem o que não se fala: código, endereços, marcas do markdown. Corta em
    fim de frase; quem quer o resto lê na tela."""
    t = _CERCA.sub(" (o código está na tela) ", texto)
    t = _LINK.sub(r"\1", t)
    t = _URL.sub("o link", t)
    t = _MARCAS.sub("", t)
    t = _ENFASE.sub("", t)
    t = re.sub(r"\s+", " ", t).strip()
    if len(t) <= limite:
        return t
    corte = t[:limite]
    fim = max(corte.rfind(". "), corte.rfind("! "), corte.rfind("? "))
    return corte[: fim + 1] if fim > limite // 2 else corte.rstrip() + "…"


class EdgeSpeaker:
    """Fala por edge-tts (voz neural gratuita). **O texto da resposta vai para o serviço da
    Microsoft**: por isso a fala só liga com `ORION_VOICE_ENABLED` e se desliga em
    `ORION_VOICE_SPEAK=false`."""

    def __init__(self, voz: str = "pt-BR-AntonioNeural", timeout_s: float = 30.0) -> None:
        self._voz, self._timeout = voz, timeout_s

    async def falar(self, texto: str) -> bytes:
        import aiohttp
        import edge_tts
        from edge_tts.exceptions import EdgeTTSException

        async def coletar() -> bytes:
            partes = [
                p.get("data") or b""
                async for p in edge_tts.Communicate(texto, self._voz).stream()
                if p["type"] == "audio"
            ]
            return b"".join(partes)

        try:
            audio = await asyncio.wait_for(coletar(), self._timeout)
        except (TimeoutError, OSError, aiohttp.ClientError, EdgeTTSException) as e:
            raise SpeakError(f"a fala falhou: {type(e).__name__}") from None
        if not audio:
            raise SpeakError("a fala veio vazia")
        return audio


async def turno_de_voz(
    *,
    agent: Agent,
    transcriber: Transcriber,
    speaker: Speaker | None,
    audio: bytes,
    canal: str = "web",
) -> AsyncIterator[dict[str, Any] | bytes]:
    """Uma fala vira um turno do agente. Produz mensagens JSON (dict) e, no fim, o áudio da
    resposta (bytes) logo depois de `{"type": "audio"}`. O turno sempre termina em `done`, com ou
    sem erro no meio."""
    tipo = tipo_do_audio(audio)
    if tipo is None:
        yield {"type": "error", "msg": "formato de áudio não reconhecido"}
        yield {"type": "done"}
        return
    try:
        texto = await transcriber.transcribe(audio, *tipo)
    except TranscribeError as e:
        yield {"type": "error", "msg": str(e)}
        yield {"type": "done"}
        return
    yield {"type": "heard", "text": texto}
    falado: list[str] = []
    precisa_aprovar = False
    async for ev in agent.run(canal, texto):
        if ev.kind == "text":
            falado.append(str(ev.data.get("text") or ""))
        elif ev.kind == "approval":
            precisa_aprovar = True
        corpo = ev.corpo()
        if corpo is not None:
            yield {"type": "ev", "ev": corpo}
    resposta = falavel("".join(falado))
    if precisa_aprovar:
        resposta = f"{resposta} {AVISO_APROVACAO}".strip()
    if speaker is not None and resposta:
        try:
            mp3 = await speaker.falar(resposta)
        except SpeakError as e:
            yield {"type": "error", "msg": str(e)}
        else:
            yield {"type": "audio", "mime": "audio/mpeg"}
            yield mp3
    yield {"type": "done"}


# ── B: voz ao vivo (Gemini Live, só conversa) ─────────────────────────────


def conexao_gemini(api_key: str, modelo: str, voz: str) -> AbstractAsyncContextManager[Any]:
    """Abre a sessão Gemini Live. **Sem `tools` no config**: o modelo não tem ferramenta alguma.
    Um modelo native-audio só responde em UMA modalidade (áudio); o texto vem pela transcrição."""
    from google import genai
    from google.genai import types

    config = types.LiveConnectConfig(
        response_modalities=[types.Modality.AUDIO],
        system_instruction=PERSONA_AO_VIVO,
        speech_config=types.SpeechConfig(
            voice_config=types.VoiceConfig(
                prebuilt_voice_config=types.PrebuiltVoiceConfig(voice_name=voz)
            )
        ),
        output_audio_transcription=types.AudioTranscriptionConfig(),
        input_audio_transcription=types.AudioTranscriptionConfig(),
    )
    return genai.Client(api_key=api_key).aio.live.connect(model=modelo, config=config)


async def _enviar(ws: WebSocket, msg: dict[str, Any]) -> None:
    with contextlib.suppress(RuntimeError, WebSocketDisconnect):  # o navegador já saiu
        await ws.send_text(json.dumps(msg, ensure_ascii=False))


async def ponte_ao_vivo(
    ws: WebSocket, conexao: AbstractAsyncContextManager[Any], *, max_s: float
) -> str:
    """Liga o microfone do navegador (PCM 16 kHz) à sessão ao vivo e devolve o áudio (PCM 24 kHz)
    e a transcrição. Devolve por que terminou: "navegador" | "tempo" | "erro"."""
    from google.genai import types

    async def subida(sessao: Any) -> None:
        while True:
            msg = await ws.receive()
            if msg["type"] == "websocket.disconnect":
                return
            dados = msg.get("bytes")
            if dados:
                if len(dados) > MAX_QUADRO_LIVE:
                    raise ValueError("quadro de áudio grande demais")
                await sessao.send_realtime_input(
                    audio=types.Blob(data=dados, mime_type="audio/pcm;rate=16000")
                )
            elif msg.get("text"):
                try:
                    cmd = json.loads(msg["text"]).get("cmd")
                except (ValueError, AttributeError):
                    continue
                if cmd == "stop":
                    return

    async def descida(sessao: Any) -> None:
        # `receive()` termina a cada fim de turno: precisa de laço. Uma volta sem nada é conexão
        # fechada, não silêncio (o silêncio do usuário não gera volta vazia).
        while True:
            houve = False
            async for resp in sessao.receive():
                houve = True
                sc = resp.server_content
                if sc is None:
                    continue
                if sc.model_turn:
                    for parte in sc.model_turn.parts or []:
                        if parte.inline_data and parte.inline_data.data:
                            await ws.send_bytes(parte.inline_data.data)
                if sc.input_transcription and sc.input_transcription.text:
                    await _enviar(ws, {"type": "heard", "text": sc.input_transcription.text})
                if sc.output_transcription and sc.output_transcription.text:
                    await _enviar(ws, {"type": "text", "text": sc.output_transcription.text})
                if sc.turn_complete:
                    await _enviar(ws, {"type": "done"})
            if not houve:
                return

    motivo = "navegador"
    try:
        async with asyncio.timeout(max_s), conexao as sessao:
            tarefas = [asyncio.create_task(subida(sessao)), asyncio.create_task(descida(sessao))]
            try:
                feitas, _ = await asyncio.wait(tarefas, return_when=asyncio.FIRST_COMPLETED)
                for t in feitas:
                    t.result()  # propaga a falha de quem terminou
            finally:
                for t in tarefas:
                    t.cancel()
                await asyncio.gather(*tarefas, return_exceptions=True)
    except TimeoutError:
        motivo = "tempo"
        await _enviar(ws, {"type": "error", "msg": "sessão ao vivo encerrada pelo tempo máximo"})
    except (WebSocketDisconnect, RuntimeError):
        pass  # o navegador saiu no meio
    except Exception as e:  # noqa: BLE001 — qualquer falha do SDK/rede vira erro curto, sem detalhe
        motivo = "erro"
        log.warning("voz ao vivo falhou: %s", type(e).__name__)
        await _enviar(ws, {"type": "error", "msg": f"falha na voz ao vivo ({type(e).__name__})"})
    return motivo


__all__ = [
    "AVISO_APROVACAO",
    "LIVE_MODELO",
    "LIVE_VOZ",
    "MAX_AUDIO",
    "EdgeSpeaker",
    "SpeakError",
    "Speaker",
    "conexao_gemini",
    "falavel",
    "ponte_ao_vivo",
    "tipo_do_audio",
    "turno_de_voz",
]

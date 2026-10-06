"""Transcrição de voz por API gratuita (Whisper no Groq): o áudio do Telegram vira texto.

Fala o formato da API de transcrição da OpenAI (`POST /audio/transcriptions`, multipart), que
o Groq e outros provedores aceitam. A chave vai só no cabeçalho `Authorization`; nenhuma
mensagem de erro carrega a chave nem a URL. **Não validado contra a API real** (ver
ORION_MELHORIAS): o formato segue a documentação do provedor.
"""

from __future__ import annotations

import httpx

GROQ = "https://api.groq.com/openai/v1"
MAX_AUDIO = 20 * 1024 * 1024  # o limite de download de arquivo do bot do Telegram


class TranscribeError(RuntimeError):
    """Falha ao transcrever. A mensagem nunca leva a chave nem a URL."""


class Transcriber:
    def __init__(
        self,
        api_key: str,
        *,
        base_url: str = GROQ,
        model: str = "whisper-large-v3-turbo",
        language: str = "pt",
        client: httpx.AsyncClient | None = None,
        timeout_s: float = 90.0,
    ) -> None:
        if not api_key:
            raise ValueError("chave de transcrição vazia")
        self._key = api_key
        self._url = f"{base_url.rstrip('/')}/audio/transcriptions"
        self._model, self._language = model, language
        self._timeout = timeout_s
        self._dono_do_cliente = client is None
        self._client = client or httpx.AsyncClient()

    async def aclose(self) -> None:
        if self._dono_do_cliente:
            await self._client.aclose()

    async def transcribe(
        self, audio: bytes, filename: str = "audio.ogg", mime: str = "audio/ogg"
    ) -> str:
        if not audio:
            raise TranscribeError("áudio vazio")
        if len(audio) > MAX_AUDIO:
            raise TranscribeError("áudio grande demais")
        try:
            r = await self._client.post(
                self._url,
                headers={"Authorization": f"Bearer {self._key}"},
                data={
                    "model": self._model,
                    "language": self._language,
                    "response_format": "json",
                    "temperature": "0",
                },
                files={"file": (filename, audio, mime)},
                timeout=self._timeout,
            )
        except httpx.HTTPError as e:
            raise TranscribeError(f"a transcrição falhou: {type(e).__name__}") from None
        if r.status_code >= 400:
            raise TranscribeError(f"a transcrição falhou (HTTP {r.status_code})")
        try:
            texto = str(r.json().get("text", "")).strip()
        except (ValueError, AttributeError):
            raise TranscribeError("resposta da transcrição inválida") from None
        if not texto:
            raise TranscribeError("não entendi nada neste áudio")
        return texto

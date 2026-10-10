"""Transcrição de voz por API gratuita (Whisper no Groq): o áudio do Telegram vira texto.

Fala o formato da API de transcrição da OpenAI (`POST /audio/transcriptions`, multipart), que
o Groq e outros provedores aceitam. A chave vai só no cabeçalho `Authorization`; nenhuma
mensagem de erro carrega a chave nem a URL. **Não validado contra a API real** (ver
ORION_MELHORIAS): o formato segue a documentação do provedor.
"""

from __future__ import annotations

from contextlib import AbstractContextManager
from typing import Any

import httpx

from . import saidas

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
        self._provedor = saidas.provedor_da_url(base_url)  # registro de saída (regra 47)
        self._model, self._language = model, language
        self._timeout = timeout_s
        self._dono_do_cliente = client is None
        self._client = client  # criado só quando a versão async for usada (a síncrona não precisa)

    async def aclose(self) -> None:
        if self._dono_do_cliente and self._client is not None:
            await self._client.aclose()

    def _pedido(self, audio: bytes, filename: str, mime: str) -> dict[str, Any]:
        if not audio:
            raise TranscribeError("áudio vazio")
        if len(audio) > MAX_AUDIO:
            raise TranscribeError("áudio grande demais")
        return {
            "url": self._url,
            "headers": {"Authorization": f"Bearer {self._key}"},
            "data": {
                "model": self._model,
                "language": self._language,
                "response_format": "json",
                "temperature": "0",
            },
            "files": {"file": (filename, audio, mime)},
            "timeout": self._timeout,
        }

    @staticmethod
    def _texto(r: httpx.Response) -> str:
        if r.status_code >= 400:
            raise TranscribeError(f"a transcrição falhou (HTTP {r.status_code})")
        try:
            texto = str(r.json().get("text", "")).strip()
        except (ValueError, AttributeError):
            raise TranscribeError("resposta da transcrição inválida") from None
        if not texto:
            raise TranscribeError("não entendi nada neste áudio")
        return texto

    async def transcribe(
        self, audio: bytes, filename: str = "audio.ogg", mime: str = "audio/ogg"
    ) -> str:
        pedido = self._pedido(audio, filename, mime)
        if self._client is None:
            self._client = httpx.AsyncClient()
        with self._medir(audio) as m:
            try:
                r = await self._client.post(**pedido)
            except httpx.HTTPError as e:
                raise TranscribeError(f"a transcrição falhou: {type(e).__name__}") from None
            m.bytes_in = len(r.content)
            texto = self._texto(r)
            m.ok = True
        return texto

    def _medir(self, audio: bytes) -> AbstractContextManager[saidas.Medida]:
        return saidas.medir(
            self._provedor,
            "transcribe",
            model=self._model,
            bytes_out=len(audio),
            content_kind="audio",
        )

    def transcribe_sync(
        self,
        audio: bytes,
        filename: str = "audio.ogg",
        mime: str = "audio/ogg",
        transport: httpx.BaseTransport | None = None,
    ) -> str:
        """Mesma coisa para ferramentas síncronas (rodam em thread): um cliente por chamada, para
        não usar o cliente async do laço principal de outra thread."""
        pedido = self._pedido(audio, filename, mime)
        with self._medir(audio) as m:
            try:
                with httpx.Client(transport=transport) as c:
                    r = c.post(**pedido)
            except httpx.HTTPError as e:
                raise TranscribeError(f"a transcrição falhou: {type(e).__name__}") from None
            m.bytes_in = len(r.content)
            texto = self._texto(r)
            m.ok = True
        return texto

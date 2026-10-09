"""Registro de saída (regra 47): toda chamada que leva dado para fora do computador vira uma linha.

Cada cliente de rede chama `registrar` (ou usa `medir`) com o provedor, o tipo da chamada, o modelo,
se deu certo, a latência, os **tamanhos** de ida e volta e o **tipo** do conteúdo (texto, imagem,
áudio). Nunca o conteúdo, a URL nem a chave. O destino (a tabela `external_calls`) é ligado pelo app
no lifespan; sem destino (testes, CLI) o registro não faz nada.

Registrar é telemetria: uma falha aqui nunca derruba a chamada que está sendo medida. É a base da
cota gratuita (regra 46, `orion/costs.py`), do card "Provedores" do painel e da tela de privacidade.
"""

from __future__ import annotations

import logging
import threading
import time
from collections.abc import Callable, Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from typing import Any
from urllib.parse import urlsplit

log = logging.getLogger("orion.saidas")

# Módulos que falam com a rede e registram cada chamada. O teste estático (tests/test_saidas.py)
# varre `orion/` e exige que todo módulo que importa um cliente de rede esteja aqui (e chame
# `saidas.`) ou em `SEM_SAIDA_PROPRIA`, com o motivo.
CLIENTES_REGISTRADOS = frozenset(
    {
        "orion.gateway",
        "orion.transcribe",
        "orion.vision",
        "orion.tools.web",
        "orion.memory.embedders",
        "orion.voice",
        "orion.delegate",
        "orion.channels.telegram",
        "orion.tools.n8n",
        "orion.mcp_client",
    }
)
SEM_SAIDA_PROPRIA = {
    "orion.log": "só ajusta o nível do logger do httpx",
    "orion.netguard": "é o transporte do `buscar_url`; quem registra é orion.tools.web",
    "orion.tools.vision": "só o tipo do transporte; a chamada é do orion.vision",
    "orion.tools.audio": "só o tipo do transporte; a chamada é do orion.transcribe",
    "orion.doctor": "só pergunta ao Ollama deste computador (127.0.0.1) se o modelo está baixado",
}

# Provedores que rodam neste computador: entram no registro (telemetria), mas não são "saída".
LOCAIS = frozenset({"ollama", "local"})

_HOSTS = (
    ("groq.com", "groq"),
    ("googleapis.com", "gemini"),
    ("brave.com", "brave"),
    ("telegram.org", "telegram"),
    ("open-meteo.com", "open-meteo"),
)

Destino = Callable[..., Any]
_destino: Destino | None = None
_trava = threading.Lock()


def definir_destino(fn: Destino | None) -> None:
    """O app liga (`memory.add_external_call`) ao subir e desliga (`None`) ao parar."""
    global _destino
    with _trava:
        _destino = fn


def provedor_da_url(url: str, padrao: str = "") -> str:
    """Nome curto do provedor pelo host (sem guardar a URL): groq, gemini, brave... ou o host."""
    host = (urlsplit(url).hostname or "").lower()
    if host in ("127.0.0.1", "localhost", "::1"):
        return padrao or "local"
    for sufixo, nome in _HOSTS:
        if host == sufixo or host.endswith("." + sufixo):
            return nome
    return padrao or host[:60] or "desconhecido"


def registrar(
    provider: str,
    kind: str,
    *,
    ok: bool,
    latency_ms: float,
    model: str = "",
    bytes_out: int = 0,
    bytes_in: int = 0,
    content_kind: str = "texto",
) -> None:
    """Uma linha em `external_calls`. Nunca levanta: é telemetria, não pode quebrar a chamada."""
    fn = _destino
    if fn is None:
        return
    try:
        fn(
            provider=provider,
            kind=kind,
            ok=ok,
            latency_ms=round(latency_ms),
            model=model,
            bytes_out=bytes_out,
            bytes_in=bytes_in,
            content_kind=content_kind,
        )
    except Exception:  # registrar é telemetria: falhar não derruba a chamada
        log.warning("não consegui registrar a saída para %s", provider, exc_info=True)


@dataclass
class Medida:
    """Preenchida por quem mede: `ok` só vira True se a chamada deu certo."""

    ok: bool = False
    bytes_in: int = 0
    model: str = ""


@contextmanager
def medir(
    provider: str,
    kind: str,
    *,
    model: str = "",
    bytes_out: int = 0,
    content_kind: str = "texto",
    clock: Callable[[], float] = time.monotonic,
) -> Iterator[Medida]:
    """Mede a latência e registra ao sair, com ou sem exceção (exceção = `ok` falso)."""
    m = Medida(model=model)
    inicio = clock()
    try:
        yield m
    finally:
        registrar(
            provider,
            kind,
            ok=m.ok,
            latency_ms=(clock() - inicio) * 1000,
            model=m.model,
            bytes_out=bytes_out,
            bytes_in=m.bytes_in,
            content_kind=content_kind,
        )


__all__ = [
    "CLIENTES_REGISTRADOS",
    "LOCAIS",
    "SEM_SAIDA_PROPRIA",
    "Medida",
    "definir_destino",
    "medir",
    "provedor_da_url",
    "registrar",
]

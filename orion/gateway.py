"""Cliente do gateway de modelos (API compatível com a da OpenAI) com streaming.

O Orion fala um formato só; quem roteia entre free tiers e controla cota é o
OmniRoute (decisão #2 do NUCLEO). Mesmo assim aceita uma lista de endpoints:
se o gateway cair ou recusar (429, 5xx, timeout), tenta o próximo; um 429
põe o endpoint em quarentena. Falha no meio do texto não troca de endpoint
(a resposta já começou) e vira `GatewayError`.
"""

from __future__ import annotations

import json
import logging
import time
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass
from typing import Any

import httpx

log = logging.getLogger("orion.gateway")


@dataclass(frozen=True)
class Endpoint:
    name: str
    base_url: str  # ex.: http://127.0.0.1:20128/v1
    model: str
    api_key: str | None = None
    timeout_s: float = 60.0


@dataclass(frozen=True)
class TextDelta:
    text: str


@dataclass(frozen=True)
class ToolCallRequest:
    id: str
    name: str
    arguments: dict[str, Any]
    error: str | None = None  # argumentos que não são JSON válido


@dataclass(frozen=True)
class Finish:
    reason: str
    endpoint: str
    model: str


Event = TextDelta | ToolCallRequest | Finish


class GatewayError(RuntimeError):
    def __init__(self, mensagem: str, tentativas: list[tuple[str, str]] | None = None) -> None:
        super().__init__(mensagem)
        self.tentativas = tentativas or []


class _FalhaAntesDoTexto(Exception):
    pass


class ChatGateway:
    def __init__(
        self,
        endpoints: list[Endpoint],
        client: httpx.AsyncClient | None = None,
        clock: Callable[[], float] = time.monotonic,
        cooldown_s: float = 60.0,
    ) -> None:
        if not endpoints:
            raise ValueError("ao menos um endpoint")
        self.endpoints = endpoints
        self._client = client or httpx.AsyncClient()
        self._clock = clock
        self._cooldown_s = cooldown_s
        self._quarentena: dict[str, float] = {}

    async def aclose(self) -> None:
        await self._client.aclose()

    async def stream(
        self, messages: list[dict[str, Any]], tools: list[dict[str, Any]] | None = None
    ) -> AsyncIterator[Event]:
        tentativas: list[tuple[str, str]] = []
        for ep in self.endpoints:
            if self._quarentena.get(ep.name, 0.0) > self._clock():
                tentativas.append((ep.name, "em quarentena (cota)"))
                continue
            comecou = False
            try:
                async for evento in self._run(ep, messages, tools):
                    comecou = True
                    yield evento
                return
            except _FalhaAntesDoTexto as e:
                tentativas.append((ep.name, str(e)))
                log.warning("endpoint %s falhou, tentando o próximo: %s", ep.name, e)
            except (httpx.HTTPError, ValueError) as e:
                if comecou:
                    raise GatewayError(f"{ep.name} interrompeu no meio da resposta: {e}") from e
                tentativas.append((ep.name, f"{type(e).__name__}: {e}"))
                log.warning("endpoint %s falhou, tentando o próximo: %s", ep.name, e)
        raise GatewayError("nenhum endpoint respondeu", tentativas)

    async def _run(
        self, ep: Endpoint, messages: list[dict[str, Any]], tools: list[dict[str, Any]] | None
    ) -> AsyncIterator[Event]:
        corpo: dict[str, Any] = {"model": ep.model, "messages": messages, "stream": True}
        if tools:
            corpo["tools"] = tools
        headers = {"Authorization": f"Bearer {ep.api_key}"} if ep.api_key else {}
        url = ep.base_url.rstrip("/") + "/chat/completions"

        chamadas: dict[int, dict[str, str]] = {}
        razao = "stop"
        houve_saida = False
        async with self._client.stream(
            "POST", url, json=corpo, headers=headers, timeout=ep.timeout_s
        ) as resp:
            if resp.status_code >= 400:
                await resp.aread()
                if resp.status_code == 429:
                    espera = _retry_after(resp.headers.get("retry-after"), self._cooldown_s)
                    self._quarentena[ep.name] = self._clock() + espera
                raise _FalhaAntesDoTexto(f"HTTP {resp.status_code}: {resp.text[:200]}")
            async for linha in resp.aiter_lines():
                if not linha.startswith("data:"):
                    continue
                dado = linha[5:].strip()
                if dado == "[DONE]":
                    break
                if not dado:
                    continue
                try:
                    pedaco = json.loads(dado)
                except json.JSONDecodeError:
                    continue
                for escolha in pedaco.get("choices") or []:
                    delta = escolha.get("delta") or {}
                    if texto := delta.get("content"):
                        houve_saida = True
                        yield TextDelta(texto)
                    for tc in delta.get("tool_calls") or []:
                        slot = chamadas.setdefault(
                            tc.get("index", len(chamadas)), {"id": "", "name": "", "args": ""}
                        )
                        slot["id"] = tc.get("id") or slot["id"]
                        fn = tc.get("function") or {}
                        slot["name"] += fn.get("name") or ""
                        slot["args"] += fn.get("arguments") or ""
                    if escolha.get("finish_reason"):
                        razao = escolha["finish_reason"]
        if not houve_saida and not chamadas:
            raise _FalhaAntesDoTexto("resposta vazia")
        for i, slot in sorted(chamadas.items()):
            yield _tool_call(i, slot)
        yield Finish(razao, ep.name, ep.model)


def _tool_call(indice: int, slot: dict[str, str]) -> ToolCallRequest:
    ident = slot["id"] or f"call_{indice}"
    try:
        args = json.loads(slot["args"] or "{}")
        if not isinstance(args, dict):
            raise ValueError("argumentos não são um objeto JSON")
    except ValueError as e:
        return ToolCallRequest(ident, slot["name"], {}, error=f"argumentos inválidos: {e}")
    return ToolCallRequest(ident, slot["name"], args)


def _retry_after(valor: str | None, padrao: float) -> float:
    try:
        return max(1.0, float(valor)) if valor else padrao
    except ValueError:
        return padrao

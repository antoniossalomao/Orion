"""Cliente do gateway de modelos (API compatível com a da OpenAI) com streaming.

O Orion fala um formato só com cada provedor (orion/provedores.py), sem gateway externo. Recebe
uma lista de endpoints (um por modelo) e, se um cair ou recusar (429, 5xx, timeout), tenta o
próximo:

- 429 põe aquele modelo em quarentena pelo `Retry-After` (ou `cooldown_s`);
- `FALHAS_PARA_PAUSA` falhas seguidas o pausam, com espera que dobra até `PAUSA_MAX_S`;
- `orcamento(ep)` falso (o Orion já gastou o `limite_dia` do provedor hoje) o manda para o fim
  da fila: só é tentado se nenhum outro responder, porque o chat nunca é bloqueado (regra 46).

Falha no meio do texto não troca de endpoint (a resposta já começou) e vira `GatewayError`.

Cada tentativa por endpoint vira uma linha no registro de saída (regra 47, `on_call`): provedor,
modelo, ok, latência e bytes, nunca o conteúdo. O modelo local (regra 49) entra como endpoint de
camada `local`: é sempre o último e nunca recebe `tools`.
"""

from __future__ import annotations

import json
import logging
import time
from collections.abc import AsyncIterator, Callable
from dataclasses import dataclass
from typing import Any

import httpx

from . import saidas

log = logging.getLogger("orion.gateway")

CAMADA_LOCAL = "local"  # modelo local de reserva (regra 49): último da fila e sem ferramentas
AVISO_RESERVA = "Modo reserva: sem ferramentas. Responda só com texto; não prometa executar ações."


@dataclass(frozen=True)
class Endpoint:
    name: str
    base_url: str  # ex.: http://127.0.0.1:20128/v1
    model: str
    api_key: str | None = None
    timeout_s: float = 60.0
    # camada de roteamento (`orion.router`): "rapido", "pesado", "visao" ou vazio (modelo padrão).
    # Endpoint de camada só é tentado quando a mensagem é daquela camada; o padrão é o reserva.
    # "local" (regra 49) é o modelo deste computador: sempre por último, em qualquer camada.
    tier: str = ""
    tools: bool = True  # False: a requisição sai sem `tools` (modelo local, regra 49)
    provedor: str = ""  # id em orion/provedores.py; vazio = endpoint avulso (ORION_GATEWAY_URL)

    @property
    def provider(self) -> str:
        """Nome no registro de saída: `gateway:<provedor>`, `gateway:<camada>` (endpoint avulso)
        ou `ollama` para o modelo local. O prefixo `gateway:` soma tudo na cota geral."""
        if self.tier == CAMADA_LOCAL:
            return "ollama"
        return f"gateway:{self.provedor or self.tier or 'padrão'}"


@dataclass
class EndpointStats:
    """Contadores de um endpoint desde que o Orion subiu (o painel mostra; reiniciar zera).
    Não é a cota do provedor (outra chave igual também gasta): é o que o Orion viu."""

    chamadas: int = 0  # tentativas de resposta (cada uma conta, com ou sem sucesso)
    ok: int = 0
    falhas: int = 0
    limitada: int = 0  # respostas 429 (cota ou limite de taxa)
    pulos: int = 0  # vezes que ficou de fora por estar em quarentena
    ultimo_ok: float | None = None  # epoch
    ultimo_erro: str | None = None  # só o tipo ("HTTP 502", "ConnectError"), nunca corpo nem URL
    seguidas: int = 0  # falhas seguidas (zera no primeiro sucesso)
    pausas: int = 0  # vezes que ficou em pausa por falhas seguidas
    orcamento: int = 0  # vezes que foi para o fim da fila por ter gasto o limite do dia


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


FALHAS_PARA_PAUSA = 3  # falhas seguidas (sem ser 429) que pausam o modelo
PAUSA_MAX_S = 900.0  # a pausa dobra a cada nova falha, até 15 min


class _FalhaAntesDoTexto(Exception):
    pass


@dataclass
class _Tentativa:
    """O que uma tentativa levou e trouxe (tamanho, nunca conteúdo), para o registro de saída."""

    bytes_out: int = 0
    bytes_in: int = 0
    falhou: bool = False


def _tipo_de_conteudo(messages: list[dict[str, Any]]) -> str:
    for m in messages:
        partes = m.get("content")
        if isinstance(partes, list) and any(
            isinstance(p, dict) and p.get("type") == "image_url" for p in partes
        ):
            return "imagem"
    return "texto"


class ChatGateway:
    def __init__(
        self,
        endpoints: list[Endpoint],
        client: httpx.AsyncClient | None = None,
        clock: Callable[[], float] = time.monotonic,
        cooldown_s: float = 60.0,
        wall: Callable[[], float] = time.time,
        on_call: Callable[..., None] = saidas.registrar,
    ) -> None:
        if not endpoints:
            raise ValueError("ao menos um endpoint")
        self.endpoints = endpoints
        self._on_call = on_call
        self._client = client or httpx.AsyncClient()
        self._clock = clock
        self._cooldown_s = cooldown_s
        self._quarentena: dict[str, float] = {}
        self._motivo: dict[str, str] = {}  # "cota" (429) ou "falhas" (pausa)
        self._wall = wall
        self._stats = {ep.name: EndpointStats() for ep in endpoints}
        # Orçamento diário por provedor (ligado pelo app a `Custos`): False = já gastou o de hoje
        self.orcamento: Callable[[Endpoint], bool] | None = None

    def stats(self) -> list[dict[str, Any]]:
        """Um dicionário por endpoint, na ordem de tentativa (para o painel)."""
        agora = self._clock()
        saida = []
        for ep in self.endpoints:
            st = self._stats[ep.name]
            restante = max(0.0, self._quarentena.get(ep.name, 0.0) - agora)
            saida.append(
                {
                    "nome": ep.name,
                    "modelo": ep.model,
                    "camada": ep.tier or "padrão",
                    "chamadas": st.chamadas,
                    "ok": st.ok,
                    "falhas": st.falhas,
                    "limitada": st.limitada,
                    "pulos": st.pulos,
                    "quarentena_s": round(restante),
                    "motivo": self._motivo.get(ep.name, "") if restante else "",
                    "provedor": ep.provedor,
                    "ultimo_ok": st.ultimo_ok,
                    "ultimo_erro": st.ultimo_erro,
                    "pausas": st.pausas,
                    "orcamento": st.orcamento,
                }
            )
        return saida

    async def aclose(self) -> None:
        await self._client.aclose()

    def _ordem(self, tier: str | None) -> list[Endpoint]:
        """Quem tentar, em ordem: os da camada pedida, depois os padrão (reserva). Sem camada, só
        os padrão; se não houver nenhum padrão, todos (config só com camadas ainda roda)."""
        locais = [e for e in self.endpoints if e.tier == CAMADA_LOCAL]
        remotos = [e for e in self.endpoints if e.tier != CAMADA_LOCAL]
        padrao = [e for e in remotos if not e.tier]
        proprios = [e for e in remotos if tier and e.tier == tier]
        return [*([*proprios, *padrao] or remotos), *locais]

    async def stream(
        self,
        messages: list[dict[str, Any]],
        tools: list[dict[str, Any]] | None = None,
        tier: str | None = None,
    ) -> AsyncIterator[Event]:
        tentativas: list[tuple[str, str]] = []
        ordem, adiados = [], []
        for ep in self._ordem(tier):
            if self.orcamento is not None and ep.provedor and not self.orcamento(ep):
                self._stats[ep.name].orcamento += 1
                adiados.append(ep)
            else:
                ordem.append(ep)
        for ep in [*ordem, *adiados]:
            st = self._stats[ep.name]
            if self._quarentena.get(ep.name, 0.0) > self._clock():
                motivo = (
                    "pausa (falhas seguidas)"
                    if self._motivo.get(ep.name) == "falhas"
                    else ("quarentena (cota)")
                )
                tentativas.append((ep.name, f"em {motivo}"))
                st.pulos += 1
                continue
            st.chamadas += 1
            comecou = False
            tentativa = _Tentativa()
            inicio = self._clock()
            try:
                async for evento in self._run(ep, messages, tools, tentativa):
                    comecou = True
                    yield evento
                st.ok += 1
                st.seguidas = 0
                st.ultimo_ok = self._wall()
                return
            except _FalhaAntesDoTexto as e:
                tentativa.falhou = True
                self._falhou(ep, str(e).split(":", 1)[0], limitada=str(e).startswith("HTTP 429"))
                tentativas.append((ep.name, str(e)))
                log.warning("endpoint %s falhou, tentando o próximo: %s", ep.name, e)
            except (httpx.HTTPError, ValueError) as e:
                tentativa.falhou = True
                self._falhou(ep, type(e).__name__)
                if comecou:
                    raise GatewayError(f"{ep.name} interrompeu no meio da resposta: {e}") from e
                tentativas.append((ep.name, f"{type(e).__name__}: {e}"))
                log.warning("endpoint %s falhou, tentando o próximo: %s", ep.name, e)
            finally:
                # quem parou de ler no meio (cancelou) não é falha do provedor: conta o que veio
                self._on_call(
                    ep.provider,
                    "chat",
                    ok=not tentativa.falhou and comecou,
                    latency_ms=(self._clock() - inicio) * 1000,
                    model=ep.model,
                    bytes_out=tentativa.bytes_out,
                    bytes_in=tentativa.bytes_in,
                    content_kind=_tipo_de_conteudo(messages),
                )
        raise GatewayError("nenhum endpoint respondeu", tentativas)

    def _falhou(self, ep: Endpoint, tipo: str, limitada: bool = False) -> None:
        """Conta a falha; a partir de `FALHAS_PARA_PAUSA` seguidas, pausa o modelo. O 429 já pôs
        a quarentena pelo `Retry-After` e não entra na conta (cota não é instabilidade)."""
        st = self._stats[ep.name]
        st.falhas += 1
        st.ultimo_erro = tipo[:40]
        if limitada:
            return
        st.seguidas += 1
        if st.seguidas >= FALHAS_PARA_PAUSA:
            espera = min(self._cooldown_s * 2 ** (st.seguidas - FALHAS_PARA_PAUSA), PAUSA_MAX_S)
            self._quarentena[ep.name] = self._clock() + espera
            self._motivo[ep.name] = "falhas"
            st.pausas += 1
            log.warning(
                "modelo %s em pausa por %.0f s (%d falhas seguidas)", ep.name, espera, st.seguidas
            )

    async def complete(self, messages: list[dict[str, Any]]) -> str:
        """Resposta inteira em texto, sem ferramentas (jobs como a consolidação da memória)."""
        texto = ""
        async for ev in self.stream(messages):
            if isinstance(ev, TextDelta):
                texto += ev.text
        return texto

    async def _run(
        self,
        ep: Endpoint,
        messages: list[dict[str, Any]],
        tools: list[dict[str, Any]] | None,
        tentativa: _Tentativa | None = None,
    ) -> AsyncIterator[Event]:
        tentativa = tentativa or _Tentativa()
        if not ep.tools:  # modelo local (regra 49): nunca recebe ferramentas, e sabe disso
            messages = [{"role": "system", "content": AVISO_RESERVA}, *messages]
        corpo: dict[str, Any] = {"model": ep.model, "messages": messages, "stream": True}
        if tools and ep.tools:
            corpo["tools"] = tools
        tentativa.bytes_out = len(json.dumps(corpo, ensure_ascii=False).encode())
        headers = {"Authorization": f"Bearer {ep.api_key}"} if ep.api_key else {}
        url = ep.base_url.rstrip("/") + "/chat/completions"

        chamadas: dict[int, dict[str, str]] = {}
        razao = "stop"
        houve_saida = False
        async with self._client.stream(
            "POST", url, json=corpo, headers=headers, timeout=ep.timeout_s
        ) as resp:
            if resp.status_code >= 400:
                tentativa.bytes_in = len(await resp.aread())
                if resp.status_code == 429:
                    espera = _retry_after(resp.headers.get("retry-after"), self._cooldown_s)
                    self._quarentena[ep.name] = self._clock() + espera
                    self._motivo[ep.name] = "cota"
                    self._stats[ep.name].limitada += 1
                raise _FalhaAntesDoTexto(f"HTTP {resp.status_code}: {resp.text[:200]}")
            async for linha in resp.aiter_lines():
                tentativa.bytes_in += len(linha) + 1
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

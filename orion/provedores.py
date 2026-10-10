"""Provedores de modelo ligados direto no Orion (sem gateway externo).

Cada provedor do catálogo fala a API compatível com a da OpenAI (`/chat/completions` com
streaming e `tools`). Quem você liga e em que ordem é a configuração `ORION_PROVEDORES` (lista
JSON, a ordem é a prioridade); a chave de cada um vem do cofre do SO ou do ambiente, em
`ORION_KEY_<ID>` (ex.: `ORION_KEY_GROQ`). O catálogo traz só endereço e nome: **modelos e limites
mudam sem aviso**, então ficam na sua configuração, não no código.

Exemplo::

    ORION_PROVEDORES='[
      {"id": "groq", "rapido": "llama-3.3-70b-versatile", "limite_dia": 1000},
      {"id": "gemini", "padrao": "gemini-2.5-flash", "visao": "gemini-2.5-flash"},
      {"id": "meu", "url": "https://exemplo.com/v1", "padrao": "modelo-x"}
    ]'

Cada modelo configurado vira um `Endpoint` (`groq`, `groq:rapido`, ...): a quarentena por 429 e a
pausa por falhas seguidas valem por modelo, não pelo provedor inteiro.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from pydantic import BaseModel, Field, field_validator, model_validator


@dataclass(frozen=True)
class Catalogado:
    nome: str
    base_url: str


# Endereços conferidos na documentação de cada provedor (todos com API compatível com a da OpenAI).
CATALOGO: dict[str, Catalogado] = {
    "gemini": Catalogado(
        "Google AI Studio (Gemini)", "https://generativelanguage.googleapis.com/v1beta/openai"
    ),
    "groq": Catalogado("Groq", "https://api.groq.com/openai/v1"),
    "cerebras": Catalogado("Cerebras", "https://api.cerebras.ai/v1"),
    "openrouter": Catalogado("OpenRouter", "https://openrouter.ai/api/v1"),
    "mistral": Catalogado("Mistral", "https://api.mistral.ai/v1"),
    "github": Catalogado("GitHub Models", "https://models.github.ai/inference"),
    "nvidia": Catalogado("NVIDIA NIM", "https://integrate.api.nvidia.com/v1"),
    "zai": Catalogado("Z.ai (GLM)", "https://api.z.ai/api/paas/v4"),
}

CAMADAS_CONF = ("padrao", "rapido", "pesado", "visao")
_ID = re.compile(r"^[a-z0-9][a-z0-9_-]{0,23}$")


class ProvedorConf(BaseModel):
    """Um item de `ORION_PROVEDORES`."""

    id: str
    url: str = ""  # obrigatório para provedor fora do catálogo
    padrao: str = ""  # modelo para qualquer mensagem (e reserva das camadas)
    rapido: str = ""
    pesado: str = ""
    visao: str = ""
    limite_dia: int = Field(default=0, ge=0)  # chamadas/dia que o Orion se permite; 0 = sem teto

    @field_validator("id")
    @classmethod
    def _id_valido(cls, v: str) -> str:
        v = v.strip().lower()
        if not _ID.match(v):
            raise ValueError("id: letras minúsculas, números, '-' ou '_' (até 24)")
        return v

    @model_validator(mode="after")
    def _completo(self) -> ProvedorConf:
        if not self.url and self.id not in CATALOGO:
            raise ValueError(f"provedor '{self.id}' fora do catálogo: informe 'url'")
        if self.url and not self.url.startswith(
            ("https://", "http://127.0.0.1", "http://localhost")
        ):
            raise ValueError(f"provedor '{self.id}': url precisa ser https (ou local)")
        if not self.modelos():
            raise ValueError(f"provedor '{self.id}': configure ao menos um modelo")
        return self

    def base_url(self) -> str:
        if self.url:
            return self.url
        if self.id in CATALOGO:
            return CATALOGO[self.id].base_url
        raise ValueError(f"provedor '{self.id}' fora do catálogo: informe 'url'")

    def modelos(self) -> list[tuple[str, str]]:
        """(camada, modelo) configurados; camada "" é o padrão."""
        return [("" if c == "padrao" else c, m) for c in CAMADAS_CONF if (m := getattr(self, c))]


def chave_env(provedor_id: str) -> str:
    return "ORION_KEY_" + provedor_id.upper().replace("-", "_")

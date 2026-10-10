"""Brave Search em leitura e fetch público; limites locais não substituem cotas da conta."""

from __future__ import annotations

import asyncio
import hashlib
import json
from html.parser import HTMLParser
from urllib.parse import urlencode

from pydantic import BaseModel, ConfigDict, Field

from ..policy import PolicyEngine, Risk, ToolSpec
from ..secrets import get_secret
from ..tools.registry import Tool, ToolRegistry
from .http import PublicHTTP, ResearchError, endpoint


class ResearchConfig(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    enabled: bool = False
    secret_ref: str = Field(
        default="ORION_RESEARCH_BRAVE_KEY", pattern=r"^ORION_RESEARCH_[A-Z0-9_]{1,80}$"
    )


class Text(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.skip = 0
        self.parts: list[str] = []

    def handle_starttag(self, tag: str, attrs) -> None:
        if tag in {"script", "style", "noscript"}:
            self.skip += 1
        if tag in {"p", "div", "br", "li", "h1", "h2", "h3"}:
            self.parts.append("\n")

    def handle_endtag(self, tag: str) -> None:
        if tag in {"script", "style", "noscript"} and self.skip:
            self.skip -= 1

    def handle_data(self, data: str) -> None:
        if not self.skip:
            self.parts.append(data)


class Research:
    def __init__(self, config: ResearchConfig, http: PublicHTTP | None = None):
        self.config = config
        self.http = http or PublicHTTP()

    async def search(self, consulta: str, limite: int = 5) -> dict:
        key = await asyncio.to_thread(get_secret, self.config.secret_ref)
        if not key:
            return {"ok": False, "codigo": "research_auth_missing"}
        try:
            url = "https://api.search.brave.com/res/v1/web/search?" + urlencode(
                {"q": consulta, "count": min(10, max(1, limite)), "safesearch": "moderate"}
            )
            response = await asyncio.to_thread(
                self.http.get,
                url,
                headers={"X-Subscription-Token": key, "Accept": "application/json"},
                limit=500000,
                redirects=0,
            )
            if response.status != 200:
                return {
                    "ok": False,
                    "codigo": "research_quota"
                    if response.status == 429
                    else "research_auth_failed"
                    if response.status in {401, 403}
                    else "research_provider_failed",
                }
            data = json.loads(response.content)
            rows = data.get("web", {}).get("results", [])
            if not isinstance(rows, list):
                raise ValueError("results invalid")
            results = []
            for row in rows[: min(10, limite)]:
                if not isinstance(row, dict):
                    continue
                source = str(row.get("url", ""))
                try:
                    endpoint(source)
                except ResearchError:
                    continue
                results.append(
                    {
                        "titulo": str(row.get("title", ""))[:500],
                        "url": source,
                        "trecho": str(row.get("description", ""))[:2000],
                        "provider": "Brave Search",
                    }
                )
            return {"ok": True, "fontes": results}
        except (ResearchError, ValueError, TypeError, AttributeError) as error:
            return {
                "ok": False,
                "codigo": str(error)
                if isinstance(error, ResearchError)
                else "research_response_invalid",
            }

    async def fetch(self, url: str) -> dict:
        try:
            response = await asyncio.to_thread(self.http.get, url)
            if response.status != 200:
                return {"ok": False, "codigo": "research_fetch_failed"}
            kind = response.headers.get("content-type", "").split(";")[0].lower()
            if kind not in {"text/html", "text/plain", "text/markdown", "application/json"}:
                raise ResearchError("research_type_unsupported")
            raw = response.content.decode("utf-8", errors="replace")
            if kind == "text/html":
                parser = Text()
                parser.feed(raw)
                raw = "\n".join(
                    line.strip() for line in "".join(parser.parts).splitlines() if line.strip()
                )
            text = raw.encode()[:16000].decode(errors="ignore")
            return {
                "ok": True,
                "url": response.url,
                "texto": text,
                "digest": hashlib.sha256(response.content).hexdigest(),
                "truncated": len(text) < len(raw),
            }
        except ResearchError as error:
            return {"ok": False, "codigo": str(error)}

    def attach(self, registry: ToolRegistry, policy: PolicyEngine) -> None:
        if not self.config.enabled:
            return
        for name, description, properties, required, function in [
            (
                "pesquisar_internet",
                "Pesquisa pública Brave Search; retorna trechos e URLs para conferir evidências.",
                {
                    "consulta": {"type": "string", "minLength": 1, "maxLength": 1000},
                    "limite": {"type": "integer", "minimum": 1, "maximum": 10},
                },
                ["consulta"],
                self.search,
            ),
            (
                "buscar_url",
                "Lê fonte HTTPS pública com limites de rede, tamanho e redirecionamentos.",
                {"url": {"type": "string", "minLength": 1, "maxLength": 4096}},
                ["url"],
                self.fetch,
            ),
        ]:
            registry.register(
                Tool(
                    name,
                    description,
                    {
                        "type": "object",
                        "properties": properties,
                        "required": required,
                        "additionalProperties": False,
                    },
                    function,
                    origin="provider:brave",
                    validar=True,
                )
            )
            policy.tools[name] = ToolSpec(
                name,
                Risk.READ,
                external=True,
                origin="provider:brave",
                revision="orion-brave-v1",
                display_name=name,
                origin_label="Brave Search"
                if name == "pesquisar_internet"
                else "Fonte web pública",
            )
            policy.rate.set_limit(name, (10, 60))

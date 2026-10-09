"""`acionar_n8n`: dispara um workflow do n8n por webhook (regra 41).

O destino NUNCA vem do modelo: o Antônio cadastra os webhooks em `ORION_N8N_WEBHOOKS`
(`{"nome": "https://..."}`) e o modelo só escolhe o nome entre os cadastrados, mais um corpo JSON
pequeno. Mesmo assim é EXECUÇÃO (sempre confirma): um workflow pode fazer qualquer coisa no mundo.
A resposta do n8n é texto de terceiros: volta marcada como conteúdo externo e contamina a sessão.
Só http/https, sem usuário na URL, resposta cortada, timeout curto.
"""

from __future__ import annotations

import json
from typing import Any
from urllib.parse import urlsplit

import httpx

from .. import saidas
from ..policy.classes import Risk, ToolSpec
from .registry import Tool

TOOL_SPEC = ToolSpec("acionar_n8n", Risk.EXEC, external=True)
MAX_CORPO = 8_000
MAX_RESPOSTA = 2_000


class N8nConfigError(ValueError):
    pass


def parse_webhooks(bruto: str) -> dict[str, str]:
    """Valida a configuração; erro de configuração derruba cedo e com mensagem clara."""
    if not bruto.strip():
        return {}
    try:
        dados = json.loads(bruto)
    except ValueError:
        raise N8nConfigError("ORION_N8N_WEBHOOKS não é JSON válido") from None
    if not isinstance(dados, dict):
        raise N8nConfigError('ORION_N8N_WEBHOOKS deve ser {"nome": "url"}')
    saida: dict[str, str] = {}
    for nome, url in dados.items():
        partes = urlsplit(str(url))
        if partes.scheme not in ("http", "https") or not partes.hostname or partes.username:
            raise N8nConfigError(f"webhook '{nome}': use http(s) sem usuário na URL")
        if not str(nome).strip() or len(str(nome)) > 60:
            raise N8nConfigError("nome de webhook vazio ou longo demais")
        saida[str(nome).strip().lower()] = str(url)
    return saida


def n8n_tool(webhooks: dict[str, str], transport: httpx.BaseTransport | None = None) -> Tool:
    def acionar_n8n(nome: str, dados: dict[str, Any] | None = None) -> dict[str, Any]:
        chave = str(nome).strip().lower()
        url = webhooks.get(chave)
        if url is None:
            return {"erro": f"webhook '{nome}' não cadastrado", "disponiveis": sorted(webhooks)}
        corpo = json.dumps(dados or {}, ensure_ascii=False)
        if len(corpo) > MAX_CORPO:
            return {"erro": f"corpo passa de {MAX_CORPO} caracteres"}
        try:
            # n8n local (127.0.0.1) é "local" no registro de saída; o seu servidor, o host dele
            with (
                saidas.medir(
                    saidas.provedor_da_url(url, "n8n"), "webhook", bytes_out=len(corpo.encode())
                ) as m,
                httpx.Client(timeout=15.0, transport=transport, follow_redirects=False) as c,
            ):
                r = c.post(
                    url, content=corpo.encode(), headers={"Content-Type": "application/json"}
                )
                m.ok, m.bytes_in = r.status_code < 400, len(r.content)
        except httpx.HTTPError as e:
            return {"erro": f"falha ao chamar o webhook: {type(e).__name__}"}
        return {"status": r.status_code, "resposta": r.text[:MAX_RESPOSTA]}

    return Tool(
        "acionar_n8n",
        "Dispara um workflow do n8n já cadastrado pelo Antônio (pelo nome), com um corpo JSON "
        "pequeno. Pede confirmação a cada uso.",
        {
            "type": "object",
            "properties": {"nome": {"type": "string"}, "dados": {"type": "object"}},
            "required": ["nome"],
        },
        acionar_n8n,
    )

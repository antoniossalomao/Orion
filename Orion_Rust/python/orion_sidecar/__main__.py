"""Sidecar Python residual do Orion (Fase 7 / B-11 — ARQUITETURA.md §7).

Roda exclusivamente sob demanda, lendo uma linha JSON no ``stdin`` e escrevendo
uma linha JSON no ``stdout``, sem servidor HTTP e sem abrir portas de rede.
Usado pelo crate Rust ``orion-sidecar`` apenas para ``crawl4ai`` e ``browser-use``.
"""

from __future__ import annotations

import asyncio
import json
import sys
from typing import Any
from urllib.parse import urlparse


def _is_public_http_url(raw_url: str) -> bool:
    """Recusa rede local (REGRAS.md: URL do modelo só para host público)."""
    try:
        parsed = urlparse(raw_url.strip())
    except ValueError:
        return False
    if parsed.scheme not in ("http", "https"):
        return False
    host = (parsed.hostname or "").rstrip(".").lower()
    if not host or host == "localhost" or host.endswith((".localhost", ".local", ".internal")):
        return False
    if host.startswith(("127.", "10.", "192.168.", "169.254.", "0.")) or host == "::1":
        return False
    return True


async def _run_crawl4ai(url: str, max_chars: int) -> dict[str, Any]:
    if not _is_public_http_url(url):
        return {"ok": False, "error": f"URL recusada pela barreira de rede: {url}"}
    try:
        from crawl4ai import AsyncWebCrawler  # type: ignore[import-not-found]
    except ImportError:
        return {
            "ok": False,
            "error": "pacote 'crawl4ai' nao esta instalado neste ambiente Python",
        }

    async with AsyncWebCrawler() as crawler:
        result = await crawler.arun(url=url)
        final_url = getattr(result, "url", None) or url
        if not _is_public_http_url(str(final_url)):
            return {
                "ok": False,
                "error": f"redirecionamento bloqueado para host nao publico: {final_url}",
            }
        md = str(getattr(result, "markdown", "") or "")[: max(256, int(max_chars))]
        metadata = getattr(result, "metadata", None) or {}
        title = metadata.get("title") if isinstance(metadata, dict) else None
        return {
            "ok": True,
            "final_url": str(final_url),
            "title": str(title) if title else None,
            "markdown": md,
            "visited_urls": [str(final_url)],
        }


async def _run_browser_use(start_url: str, goal: str, max_steps: int) -> dict[str, Any]:
    if not _is_public_http_url(start_url):
        return {"ok": False, "error": f"URL inicial recusada pela barreira de rede: {start_url}"}
    try:
        import browser_use  # type: ignore[import-not-found]  # noqa: F401
    except ImportError:
        return {
            "ok": False,
            "error": "pacote 'browser-use' nao esta instalado neste ambiente Python",
        }
    return {
        "ok": False,
        "error": "execucao autonoma do browser-use exige provedor configurado pela sessao aprovada",
    }


def handle_request(req: dict[str, Any]) -> dict[str, Any]:
    """Processa um único pedido JSON vindo do Rust e devolve o dicionário de resposta."""
    req_id = str(req.get("id") or "")
    op = str(req.get("op") or "ping")

    if op == "ping":
        return {
            "id": req_id,
            "ok": True,
            "final_url": None,
            "title": "orion-sidecar",
            "markdown": "pong",
            "visited_urls": [],
        }

    if op == "crawl4_ai":
        url = str(req.get("url") or "")
        max_chars = int(req.get("max_chars") or 32000)
        res = asyncio.run(_run_crawl4ai(url, max_chars))
        res["id"] = req_id
        return res

    if op == "browser_use":
        start_url = str(req.get("start_url") or "")
        goal = str(req.get("goal") or "")
        max_steps = int(req.get("max_steps") or 12)
        res = asyncio.run(_run_browser_use(start_url, goal, max_steps))
        res["id"] = req_id
        return res

    return {
        "id": req_id,
        "ok": False,
        "error": f"operacao desconhecida no sidecar: {op}",
    }


def main() -> int:
    raw_line = sys.stdin.readline()
    if not raw_line or not raw_line.strip():
        return 0
    try:
        payload = json.loads(raw_line)
        if not isinstance(payload, dict):
            raise ValueError("o payload JSON deve ser um objeto")
        response = handle_request(payload)
    except Exception as exc:  # noqa: BLE001
        response = {
            "id": "",
            "ok": False,
            "error": f"falha ao processar JSON no sidecar: {exc}",
        }
    sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
    sys.stdout.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

"""Pacote Python residual do Orion (Fase 7 / B-11 — ARQUITETURA.md §7).

Contém apenas a ponte ``stdin``/``stdout`` para ``crawl4ai`` e ``browser-use``.
"""

from .__main__ import handle_request, main

__all__ = ["handle_request", "main"]

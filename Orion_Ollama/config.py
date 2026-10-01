"""config.py — single source of truth for ports, URLs, model IDs, and limits.

Every hardcoded value previously scattered across the codebase (127 magic
strings counted during the OOP refactor of 2026-08-07) is defined here once.
All services bind to 127.0.0.1 only (project rule: nothing listens on 0.0.0.0).
"""
import os
from pathlib import Path
from dotenv import load_dotenv

load_dotenv(Path(__file__).parent / ".env")

# ── Service ports ────────────────────────────────────────────────────────────
CEREBRO_PORT = 8000   # cerebro_maestro FastAPI
EMBED_PORT   = 8001   # embed_service (BGE-M3)
SURREAL_PORT = 8090   # SurrealDB HTTP
QDRANT_PORT  = 6333   # Qdrant HTTP

# ── Derived URLs (one definition, used everywhere) ───────────────────────────
SURREAL_URL = f"http://127.0.0.1:{SURREAL_PORT}/sql"
EMBED_URL   = f"http://127.0.0.1:{EMBED_PORT}/embed"
RERANK_URL  = f"http://127.0.0.1:{EMBED_PORT}/rerank"
EMBED_HEALTH_URL = f"http://127.0.0.1:{EMBED_PORT}/health"
QDRANT_URL  = f"http://127.0.0.1:{QDRANT_PORT}"

# ── SurrealDB connection parameters ──────────────────────────────────────────
SURREAL_NS      = os.getenv("SURREAL_NS", "orion_core")
SURREAL_DB      = "Db_CORTEX"
SURREAL_AUTH    = (os.getenv("SURREAL_USER", "root"), os.getenv("SURREAL_PASS", "root"))
SURREAL_HEADERS = {
    "Accept": "application/json",
    "surreal-ns": SURREAL_NS,
    "surreal-db": SURREAL_DB,
}

# ── Model identifiers ────────────────────────────────────────────────────────
GROQ_MODEL       = "openai/gpt-oss-120b"   # llama-3.3-70b deprecated 17/jun/2026
GEMINI_MODEL     = "gemini-3.5-flash"

# ── Identidade ───────────────────────────────────────────────────────────────
NOME_ASSISTENTE   = "Orion"
# Nomes de ator gravados por versões anteriores da base (ex.: "Nome1,Nome2")
# — a leitura trata todos como fala do assistente.
ATORES_ASSISTENTE_NOMES = [NOME_ASSISTENTE] + [
    n.strip() for n in os.getenv("ATORES_LEGADOS", "").split(",") if n.strip()]
ATORES_ASSISTENTE = {n.lower() for n in ATORES_ASSISTENTE_NOMES}

# ── Memory / Qdrant ──────────────────────────────────────────────────────────
QDRANT_COLLECTION = os.getenv("QDRANT_COLLECTION", "orion_memory")  # BGE-M3 1024d
EMBED_DIM         = 1024

# ── Operational limits ───────────────────────────────────────────────────────
MAX_TOOL_ITERATIONS = 25    # safety cap on tool-calling rounds per chat
MAX_HISTORY_MSGS    = 14    # history length that triggers compression
LLM_TIMEOUT_S       = 120   # per-call timeout for batch LLM requests
SURREAL_TIMEOUT_S   = 10    # default SurrealDB request timeout

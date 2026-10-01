"""
models/sessions.py — schemas Pydantic dos endpoints de sessão de conversa.
Extraído de cerebro_maestro.py (reorganização OOP de 08/2026).
"""
from pydantic import BaseModel


class SessaoAtivar(BaseModel):
    sessao_id: str

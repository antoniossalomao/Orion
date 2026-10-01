"""
models/agents.py — schemas Pydantic dos endpoints de enxame.
Extraído de cerebro_maestro.py (reorganização OOP de 08/2026).
"""
from pydantic import BaseModel


class EnxameRequest(BaseModel):
    objetivo:     str
    subtarefas:   list[str]
    max_paralelo: int = 3

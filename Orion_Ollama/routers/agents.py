"""
routers/agents.py — endpoints de enxame de sub-agentes.

Extraído de cerebro_maestro.py na reorganização OOP (08/2026). Comportamento
idêntico ao original — só move código de lugar, não muda lógica.
"""
from fastapi import APIRouter

from models.agents import EnxameRequest


class AgentsRouter:
    """Agrupa os endpoints `/enxame*` (orion_agentes.py), chamados pelas
    ferramentas de enxame (tools/specialist.py) via HTTP.

    Dependências injetadas via construtor — o módulo de enxame e o
    `notificar_usuario` que o handler de consolidação chama.
    """

    def __init__(self, *, agentes_module, notificar_usuario):
        self._agentes = agentes_module
        self._notificar_usuario = notificar_usuario

        self.router = APIRouter()
        self.router.add_api_route("/enxame", self.enxame_criar, methods=["POST"])
        self.router.add_api_route("/enxame/{enxame_id}", self.enxame_status, methods=["GET"])
        self.router.add_api_route("/enxame/{enxame_id}/consolidar", self.enxame_consolidar, methods=["POST"])

    async def enxame_criar(self, req: EnxameRequest):
        return await self._agentes.criar_enxame(req.objetivo, req.subtarefas, req.max_paralelo)

    async def enxame_status(self, enxame_id: str):
        return await self._agentes.status_enxame(enxame_id)

    async def enxame_consolidar(self, enxame_id: str):
        resultado = await self._agentes.consolidar_enxame(enxame_id)
        # Notifica proativamente quando consolidação termina
        if "resumo" in resultado:
            self._notificar_usuario(
                titulo="Enxame consolidado",
                mensagem=resultado["resumo"][:200],
                urgencia="normal",
            )
        return resultado

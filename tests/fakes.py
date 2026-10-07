"""Gateway falso compartilhado pelos testes do agente e do app."""

import asyncio
import copy

from orion.gateway import Finish, TextDelta, ToolCallRequest


class FakeGateway:
    """Cada chamada a `stream` consome o próximo roteiro (lista de eventos ou exceção)."""

    def __init__(self, *roteiros):
        self.roteiros = list(roteiros)
        self.chamadas: list[list[dict]] = []
        self.ferramentas: list = []
        self.camadas: list = []  # a camada de roteamento de cada chamada (None: sem roteamento)

    async def stream(self, messages, tools=None, tier=None):
        self.chamadas.append(copy.deepcopy(messages))
        self.ferramentas.append(tools)
        self.camadas.append(tier)
        roteiro = self.roteiros.pop(0)
        if isinstance(roteiro, Exception):
            raise roteiro
        for ev in roteiro:
            await asyncio.sleep(0)
            yield ev


def fala(t: str, endpoint="omni") -> list:
    return [TextDelta(t), Finish("stop", endpoint, "modelo-x")]


def chama(nome: str, **args) -> ToolCallRequest:
    return ToolCallRequest(f"call_{nome}", nome, args)


def pede(*chamadas) -> list:
    return [*chamadas, Finish("tool_calls", "omni", "modelo-x")]

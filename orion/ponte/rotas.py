"""As rotas da ponte: o WebSocket que ela abre, os comandos que o Antônio manda pela interface e a
captura do "O que é isso?"."""

from __future__ import annotations

import asyncio
import contextlib
import json
from typing import TYPE_CHECKING, Any

from fastapi import (
    APIRouter,
    Depends,
    HTTPException,
    Request,
    WebSocket,
    WebSocketDisconnect,
)
from pydantic import BaseModel, ConfigDict, Field

from .hub import ComandoInvalido, PonteHub

if TYPE_CHECKING:
    from ..app import AppState

ESCOPO = "ponte"
# Tudo que o token da ponte alcança (regra 50). O resto responde 403 em `require_auth`.
ROTAS_DA_PONTE = ("/captura",)
ENTRAR_PANICO = ("POST", "/modo/panico")  # cortar é sempre seguro; sair exige a senha
VIGIA_S = 2.0  # de quanto em quanto tempo o handler confere o pânico


def rota_permitida(metodo: str, caminho: str) -> bool:
    if (metodo, caminho) == ENTRAR_PANICO:
        return True
    return any(caminho == r or caminho.startswith(r + "/") for r in ROTAS_DA_PONTE)


class ComandoPonte(BaseModel):
    model_config = ConfigDict(extra="forbid")
    cmd: str = Field(max_length=20)
    rota: str | None = Field(default=None, max_length=60)
    texto: str | None = Field(default=None, max_length=9000)


def router(require_auth: Any) -> APIRouter:
    api = APIRouter()

    @api.get("/ponte/estado", dependencies=[Depends(require_auth)])
    def estado(request: Request) -> dict[str, Any]:
        s: AppState = request.app.state.orion
        return s.ponte.estado()

    @api.post("/ponte/comando", dependencies=[Depends(require_auth)])
    def comando(corpo: ComandoPonte, request: Request) -> dict[str, Any]:
        """Manda `abrir` ou `colar` à ponte conectada. Só com login (nunca o token da ponte)."""
        s: AppState = request.app.state.orion
        if _em_panico(s):
            raise HTTPException(409, "modo pânico: a ponte está desligada")
        try:
            entregue = s.ponte.enviar(corpo.model_dump(exclude_none=True))
        except ComandoInvalido as e:
            raise HTTPException(422, str(e)) from None
        if not entregue:
            raise HTTPException(503, "a ponte não está conectada (rode `orion ponte`)")
        return {"ok": True}

    @api.websocket("/ws/ponte")
    async def ws_ponte(ws: WebSocket) -> None:
        s: AppState = ws.app.state.orion
        token = (ws.headers.get("authorization") or "").removeprefix("Bearer ").strip()
        if s.auth.device_scope(token) != ESCOPO or _em_panico(s):
            await ws.close(code=1008)
            return
        await ws.accept()
        hub: PonteHub = s.ponte
        fila = hub.anexar()
        saiu = asyncio.Event()

        async def ouvir() -> None:  # o cliente só avisa que está vivo; desconectar encerra
            try:
                while True:
                    await ws.receive_text()
            except (WebSocketDisconnect, RuntimeError):
                saiu.set()

        leitor = asyncio.create_task(ouvir())
        try:
            while not saiu.is_set():
                try:
                    comando = await asyncio.wait_for(fila.get(), VIGIA_S)
                except TimeoutError:
                    if _em_panico(s) or s.auth.device_scope(token) != ESCOPO:
                        break  # pânico ou pareamento desfeito: derruba a ponte
                    continue
                if comando is None:  # outra ponte assumiu
                    break
                if _em_panico(s):
                    break
                await ws.send_text(json.dumps(comando))
        except (WebSocketDisconnect, RuntimeError):
            pass
        finally:
            leitor.cancel()
            hub.soltar(fila)
            with contextlib.suppress(Exception):
                await ws.close(code=1000)

    return api


def _em_panico(s: AppState) -> bool:
    return s.modos is not None and s.modos.panico()

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
    Response,
    WebSocket,
    WebSocketDisconnect,
)
from pydantic import BaseModel, ConfigDict, Field

from .hub import ComandoInvalido, PonteHub
from .imagens import ImagemInvalida, Imagens

if TYPE_CHECKING:
    from ..app import AppState

ESCOPO = "ponte"
# Tudo que o token da ponte alcança (regra 50). O resto responde 403 em `require_auth`.
ROTAS_DA_PONTE = ("/captura", "/ponte/imagem", "/ponte/explicar")
ENTRAR_PANICO = ("POST", "/modo/panico")  # cortar é sempre seguro; sair exige a senha
CHAVE_AVISO = "ponte:isso_aviso_aceito"
MAX_BYTES = 6 * 1024 * 1024
VIGIA_S = 2.0  # de quanto em quanto tempo o handler confere o pânico


def rota_permitida(metodo: str, caminho: str) -> bool:
    if (metodo, caminho) == ENTRAR_PANICO:
        return True
    return any(caminho == r or caminho.startswith(r + "/") for r in ROTAS_DA_PONTE)


class PedidoExplicar(BaseModel):
    model_config = ConfigDict(extra="forbid")
    imagem_id: str = Field(min_length=8, max_length=40, pattern=r"^[A-Za-z0-9_-]+$")
    pergunta: str = Field(default="", max_length=500)
    aceito: bool = False  # o aviso da primeira vez (a imagem vai ao provedor de visão)


class ComandoPonte(BaseModel):
    model_config = ConfigDict(extra="forbid")
    cmd: str = Field(max_length=20)
    rota: str | None = Field(default=None, max_length=60)
    texto: str | None = Field(default=None, max_length=9000)


def router(require_auth: Any) -> APIRouter:
    api = APIRouter()
    imagens = Imagens()

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

    @api.post("/ponte/imagem", dependencies=[Depends(require_auth)])
    async def guardar_imagem(request: Request) -> dict[str, Any]:
        """A captura do "O que é isso?": só na memória do servidor, 5 minutos, nunca em disco."""
        dados = bytearray()
        async for pedaco in request.stream():
            dados.extend(pedaco)
            if len(dados) > MAX_BYTES:
                raise HTTPException(413, "imagem grande demais")
        try:
            return {"id": imagens.guardar(bytes(dados))}
        except ImagemInvalida as e:
            raise HTTPException(422, str(e)) from None

    @api.get("/ponte/imagem/{id_}", dependencies=[Depends(require_auth)])
    def ver_imagem(id_: str) -> Response:
        achou = imagens.pegar(id_)
        if achou is None:
            raise HTTPException(404, "a captura expirou (5 minutos)")
        dados, mime = achou
        return Response(
            dados,
            media_type=mime,
            headers={"Cache-Control": "no-store", "X-Content-Type-Options": "nosniff"},
        )

    @api.post("/ponte/explicar", dependencies=[Depends(require_auth)])
    async def explicar(corpo: PedidoExplicar, request: Request) -> dict[str, Any]:
        """Pergunta ao modelo de visão sobre a captura. A imagem **sai do computador** (regra 28):
        exige `ORION_VISION_TOOLS`, não roda em pânico e, na primeira vez, só devolve
        `{"aviso": "primeira_vez"}` até vir `aceito: true` (nada sai antes)."""
        from ..app import vision_from_settings
        from ..vision import VisionError

        s: AppState = request.app.state.orion
        if _em_panico(s):
            raise HTTPException(409, "modo pânico: nada sai do computador")
        vision = vision_from_settings(s.settings)
        if vision is None:
            raise HTTPException(409, "ligue ORION_VISION_TOOLS para perguntar sobre imagens")
        achou = imagens.pegar(corpo.imagem_id)
        if achou is None:
            raise HTTPException(404, "a captura expirou (5 minutos): aperte a tecla de novo")
        if not s.memory.meta_get(CHAVE_AVISO):
            if not corpo.aceito:
                return {"aviso": "primeira_vez"}  # 200: pedir o aceite não é erro
            s.memory.meta_set(CHAVE_AVISO, "1")
        dados, mime = achou
        try:
            texto = await asyncio.to_thread(vision.describe, dados, mime, corpo.pergunta)
        except VisionError as e:
            raise HTTPException(502, str(e)) from None
        s.ops.audit_add(
            {
                "tool": "ponte_explicar",
                "action": "allow",
                "risk": "egress",
                "reason": f"imagem de {len(dados)} bytes ao provedor de visão",
            }
        )
        return {"texto": texto}

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

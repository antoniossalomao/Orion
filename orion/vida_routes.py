"""E10: rotas de finanças, contas, preços e simulação. Regra 61: tudo local, nada sai daqui."""

from __future__ import annotations

from datetime import date, datetime
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, Field

from .vida import VidaError, parse_gasto, parse_valor, simular


class PedidoGasto(BaseModel):
    texto: str = Field(min_length=1, max_length=300)


class PedidoConta(BaseModel):
    descricao: str = Field(min_length=1, max_length=200)
    reais: float = Field(gt=0, le=10_000_000)
    vencimento: date


class PedidoPreco(BaseModel):
    item: str = Field(min_length=1, max_length=120)
    consulta: str = Field(min_length=1, max_length=200)
    alvo_reais: float | None = Field(default=None, gt=0, le=10_000_000)


class PedidoSimulacao(BaseModel):
    saldo_reais: float = Field(ge=-10_000_000, le=10_000_000)
    extra_mensal_reais: float = Field(default=0, ge=0, le=10_000_000)
    meses: int = Field(ge=1, le=120)


def router(require_auth: Any) -> Any:
    api = APIRouter(dependencies=[Depends(require_auth)])

    def _financas(request: Request) -> Any:
        s = request.app.state.orion
        if s.financas is None:
            raise HTTPException(409, "finanças indisponíveis neste servidor")
        return s.financas

    @api.get("/financas/resumo")
    def resumo(request: Request, ano: int | None = None, mes: int | None = None) -> dict[str, Any]:
        hoje = datetime.now()
        try:
            return _financas(request).resumo_mensal(ano or hoje.year, mes or hoje.month)
        except (ValueError, OverflowError):
            raise HTTPException(422, "mês inválido") from None

    @api.post("/financas/gastos")
    def lancar_gasto(corpo: PedidoGasto, request: Request) -> dict[str, Any]:
        g = parse_gasto(corpo.texto)
        if g is None:
            raise HTTPException(422, "não achei o valor (ex.: 'gastei 30 no mercado')")
        return _financas(request).registrar_gasto(g["cents"], g["category"], g["description"])

    @api.get("/financas/contas")
    def contas(request: Request) -> dict[str, Any]:
        return {"contas": _financas(request).contas_abertas()}

    @api.post("/financas/contas")
    def cadastrar_conta(corpo: PedidoConta, request: Request) -> dict[str, Any]:
        try:
            cents = parse_valor(str(corpo.reais))
            return _financas(request).cadastrar_conta(corpo.descricao, cents, corpo.vencimento)
        except VidaError as e:
            raise HTTPException(422, str(e)) from None

    @api.post("/financas/contas/{rid}/pagar")
    def pagar(rid: int, request: Request) -> dict[str, Any]:
        try:
            return _financas(request).pagar_conta(rid)
        except VidaError as e:
            raise HTTPException(409 if "já paga" in str(e) else 404, str(e)) from None

    @api.get("/precos")
    def precos(request: Request) -> dict[str, Any]:
        return {"itens": _precos(request).listar()}

    @api.post("/precos")
    def acompanhar(corpo: PedidoPreco, request: Request) -> dict[str, Any]:
        alvo = None if corpo.alvo_reais is None else parse_valor(str(corpo.alvo_reais))
        return _precos(request).adicionar(corpo.item, corpo.consulta, alvo)

    @api.post("/vida/simular")
    def simulacao(corpo: PedidoSimulacao, request: Request) -> dict[str, Any]:
        f = _financas(request)
        historico = f.historico_mensal()
        try:
            faixas = simular(
                round(corpo.saldo_reais * 100),
                historico,
                round(corpo.extra_mensal_reais * 100),
                corpo.meses,
            )
        except VidaError as e:
            raise HTTPException(409, str(e)) from None
        return {
            "meses": corpo.meses,
            "base_meses": len(historico),
            "faixas_cents": faixas,
            "aviso": "faixas de cenário, não previsão",
        }

    def _precos(request: Request) -> Any:
        s = request.app.state.orion
        if s.precos is None:
            raise HTTPException(409, "monitor de preço indisponível")
        return s.precos

    return api

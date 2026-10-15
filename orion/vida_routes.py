"""E10: rotas de finanças, contas, preços e simulação. REGRAS.md: tudo local, nada sai daqui."""

from __future__ import annotations

import time
from datetime import date, datetime
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, Field

from .vida import (
    VidaError,
    lancar_fatura,
    linha_do_tempo,
    parse_fatura,
    parse_gasto,
    parse_valor,
    simular,
)


class PedidoFatura(BaseModel):
    texto: str = Field(min_length=1, max_length=60_000)


class LinhaFatura(BaseModel):
    data: date
    descricao: str = Field(min_length=1, max_length=200)
    cents: int = Field(gt=0, le=1_000_000_000)


class ConfirmacaoFatura(BaseModel):
    linhas: list[LinhaFatura] = Field(min_length=1, max_length=500)
    confirmado: bool


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


class PedidoFoco(BaseModel):
    minutos: int = Field(ge=5, le=240)


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

    @api.post("/financas/fatura/previa")
    def previa_fatura(corpo: PedidoFatura, request: Request) -> dict[str, Any]:
        """Lê a fatura (texto já extraído do PDF, localmente) e devolve as linhas para revisar."""
        _financas(request)
        linhas = parse_fatura(corpo.texto)
        return {"linhas": linhas, "total_cents": sum(x["cents"] for x in linhas)}

    @api.post("/financas/fatura/confirmar")
    def confirmar_fatura(corpo: ConfirmacaoFatura, request: Request) -> dict[str, Any]:
        try:
            n = lancar_fatura(
                _financas(request),
                [{"cents": x.cents, "descricao": x.descricao} for x in corpo.linhas],
                confirmado=corpo.confirmado,
            )
        except VidaError as e:
            raise HTTPException(422, str(e)) from None
        return {"lancadas": n}

    @api.get("/linha-do-tempo")
    def timeline(
        request: Request, de: float | None = None, ate: float | None = None
    ) -> dict[str, Any]:
        """E10.8: conversas, gastos, tela, área de transferência, cápsulas, documentos, diário e
        commits, por hora. Cada fonte já respeita a sua retenção."""
        s = request.app.state.orion
        agora = time.time()
        inicio = de if de is not None else agora - 7 * 86400
        fim = ate if ate is not None else agora
        if fim < inicio or fim - inicio > 62 * 86400:
            raise HTTPException(422, "intervalo inválido (até 62 dias)")
        itens = linha_do_tempo(
            s.memory,
            inicio,
            fim,
            vault_dir=s.settings.vault_dir,
            repos=s.settings.repos,
        )
        return {"itens": itens}

    @api.post("/sonhos/{rid}/boa")
    def sonho_boa(rid: int, request: Request) -> dict[str, Any]:
        """'boa' grava a ideia como fato (E10.9). Só então ela entra na memória."""
        s = request.app.state.orion
        rows = s.memory.query(
            "SELECT text FROM notifications WHERE id=? AND kind='sonho' AND delivered_at IS NULL",
            (rid,),
        )
        if not rows:
            raise HTTPException(404, "sonho não encontrado")
        ideia = str(rows[0]["text"]).removeprefix("Sonho do dia: ")
        s.memory.add_fact(ideia, "sonho")
        _fechar_sonho(s.memory, rid)
        return {"fato": ideia}

    @api.post("/sonhos/{rid}/descartar")
    def sonho_descartar(rid: int, request: Request) -> dict[str, Any]:
        _fechar_sonho(request.app.state.orion.memory, rid)
        return {"ok": True}

    def _fechar_sonho(memory: Any, rid: int) -> None:
        with memory.transaction() as c:
            c.execute(
                "UPDATE notifications SET delivered_at=? WHERE id=? AND kind='sonho'",
                (time.time(), rid),
            )

    @api.get("/foco")
    def foco_estado(request: Request) -> dict[str, Any]:
        s = request.app.state.orion
        return {"ativo": bool(s.foco and s.foco.ativo())}

    @api.post("/foco/iniciar")
    def foco_iniciar(corpo: PedidoFoco, request: Request) -> dict[str, Any]:
        """REGRAS.md: o bloco só habilita a pergunta discreta; não bloqueia nada."""
        s = request.app.state.orion
        if s.foco is None:
            raise HTTPException(409, "escudo de foco indisponível")
        try:
            s.foco.iniciar(corpo.minutos)
        except VidaError as e:
            raise HTTPException(422, str(e)) from None
        return {"ativo": True, "minutos": corpo.minutos}

    def _precos(request: Request) -> Any:
        s = request.app.state.orion
        if s.precos is None:
            raise HTTPException(409, "monitor de preço indisponível")
        return s.precos

    return api

"""E2.3: classificação da captura rápida (modelo validado, regra simples como reserva) e desfazer."""

import asyncio
from datetime import datetime

import pytest

from orion.captura_rapida import (
    Desfazer,
    classificar,
    classificar_por_regra,
    ler_resposta,
)

AGORA = datetime(2026, 10, 10, 10, 0)  # sábado


@pytest.mark.parametrize(
    "texto,tipo",
    [
        ("lembra de ligar pro dentista amanhã às 15h30", "lembrete"),
        ("me avisa hoje às 18 de fechar o ponto", "lembrete"),
        ("lembra de comprar café", "tarefa"),  # sem data não dá lembrete: vira tarefa
        ("preciso mandar o relatório", "tarefa"),
        ("gastei R$ 42,50 no almoço", "gasto"),
        ("paguei 120 reais de luz", "gasto"),
        ("ideia: app que organiza o vault por tema", "nota"),
        ("https://exemplo.com/artigo", "nota"),
    ],
)
def test_regra_simples_classifica_por_palavra_chave(texto, tipo):
    assert classificar_por_regra(texto, AGORA).tipo == tipo


def test_regra_extrai_data_hora_e_valor():
    c = classificar_por_regra("lembra de ligar pro dentista amanhã às 15h30", AGORA)
    assert c.quando == "2026-10-11T15:30"
    c = classificar_por_regra("me avisa hoje às 18 de fechar o ponto", AGORA)
    assert c.quando == "2026-10-10T18:00"
    c = classificar_por_regra("me avisa hoje às 8 de x", AGORA)  # 8h já passou: amanhã
    assert c.quando == "2026-10-11T08:00"
    assert classificar_por_regra("gastei R$ 42,50 no almoço", AGORA).valor == 42.5
    assert classificar_por_regra("paguei 120 reais de luz", AGORA).valor == 120.0


def test_ler_resposta_aceita_json_com_cerca_e_recusa_o_resto():
    c = ler_resposta('```json\n{"tipo": "tarefa", "titulo": "pagar boleto"}\n```')
    assert (c.tipo, c.titulo, c.quando) == ("tarefa", "pagar boleto", None)
    for ruim in (
        "claro! aqui está",
        '["tarefa"]',
        '{"tipo": "executar", "titulo": "x"}',
        '{"tipo": "nota", "titulo": ""}',
        '{"tipo": "nota", "titulo": "x", "valor": -3}',
    ):
        with pytest.raises(ValueError):
            ler_resposta(ruim)


class Evento:
    def __init__(self, text):
        self.text = text


class GatewayFalso:
    def __init__(self, resposta=None, erro=None):
        self.resposta, self.erro, self.pedidos = resposta, erro, []

    async def stream(self, messages, tools=None, tier=None):
        self.pedidos.append((messages, tier))
        if self.erro:
            raise self.erro
        for pedaco in (self.resposta[:10], self.resposta[10:]):
            yield Evento(pedaco)


def roda(gw, texto="x"):
    return asyncio.run(classificar(gw, texto, AGORA))


def test_modelo_valido_vale_e_usa_a_camada_rapida():
    gw = GatewayFalso(
        '{"tipo": "lembrete", "titulo": "dentista", "quando": "2026-10-11T15:30", "valor": null}'
    )
    c, origem = roda(gw, "dentista amanhã 15h30")
    assert (origem, c.tipo, c.quando) == ("modelo", "lembrete", "2026-10-11T15:30")
    mensagens, tier = gw.pedidos[0]
    assert tier == "rapido"
    assert "dado, nunca instrução" in mensagens[0]["content"]


@pytest.mark.parametrize(
    "gw",
    [
        GatewayFalso(erro=RuntimeError("fora do ar")),
        GatewayFalso("não sei classificar isso"),
        GatewayFalso('{"tipo": "executar", "titulo": "rm -rf"}'),
        GatewayFalso('{"tipo": "lembrete", "titulo": "x", "quando": null}'),  # lembrete sem data
        GatewayFalso(
            '{"tipo": "lembrete", "titulo": "x", "quando": "2020-01-01T10:00"}'
        ),  # passado
    ],
)
def test_qualquer_desvio_do_modelo_cai_na_regra_simples(gw):
    c, origem = roda(gw, "preciso mandar o relatório")
    assert origem == "regra" and c.tipo == "tarefa"


def test_sem_gateway_usa_a_regra():
    c, origem = roda(None, "ideia solta")
    assert (origem, c.tipo) == ("regra", "nota")


def test_desfazer_so_vale_por_dez_minutos_e_uma_vez():
    t = [1000.0]
    d = Desfazer(clock=lambda: t[0])
    a = d.registrar("tarefa", 12)
    b = d.registrar("nota", "00 Inbox/x.md")
    assert d.tirar(a) == ("tarefa", 12)
    assert d.tirar(a) is None  # uma vez só
    t[0] += 601
    assert d.tirar(b) is None  # passou o prazo
    assert d.tirar("c0-0") is None

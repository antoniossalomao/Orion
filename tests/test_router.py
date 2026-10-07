"""Roteamento por tipo de tarefa: a pontuação, as camadas do gateway e o agente."""

import httpx
import pytest

from orion.agent import Agent
from orion.app import gateway_from_settings, roteamento_ligado
from orion.config import Settings
from orion.gateway import ChatGateway, Endpoint
from orion.memory import MemoryStore
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.router import LIMIAR_PESADO, PESADO, RAPIDO, VISAO, classificar
from orion.tools import ToolRegistry, memory_tools
from tests.fakes import FakeGateway, chama, fala, pede
from tests.test_gateway import coletar, resposta, sse, texto

CODIGO = "Analise o código abaixo e refatore:\n```python\ndef f(x):\n    return x * 2\n```"


# ── a pontuação ───────────────────────────────────────────────────────────────
@pytest.mark.parametrize(
    "mensagem",
    [
        "oi",
        "que horas são?",
        "lembra de pagar o boleto amanhã às 9",
        "toca a próxima música",
        "qual a capital da França?",
        "obrigado!",
        "o que eu tenho para hoje?",
    ],
)
def test_conversa_curta_e_comando_de_ferramenta_vao_para_o_rapido(mensagem):
    assert classificar(mensagem).camada == RAPIDO


@pytest.mark.parametrize(
    "mensagem",
    [
        CODIGO,
        "Compare SQLite e Postgres para o meu caso, com prós e contras em detalhe",
        "Planeje a migração do banco de dados do legado, passo a passo",
        "Traceback (most recent call last):\n  File 'a.py', line 3\nValueError: x\nme ajuda a depurar",
        "x" * 1300,
    ],
)
def test_codigo_analise_e_texto_longo_vao_para_o_pesado(mensagem):
    assert classificar(mensagem).camada == PESADO


def test_o_motivo_mostra_os_sinais_que_pontuaram():
    r = classificar(CODIGO)
    assert r.camada == PESADO
    assert "código ou erro colado (+2)" in r.motivo and "analis" in r.motivo
    assert classificar("oi").motivo == "pergunta ou comando curto"


def test_um_sinal_so_nao_basta_e_o_limiar_e_o_documentado():
    assert LIMIAR_PESADO == 3
    assert classificar("explique o diagrama de classes").camada == RAPIDO  # só tema técnico (+1)
    assert classificar("analise isso" + " ." * 130).camada == PESADO  # pede (+2) + 261 chars (+1)


def test_acentos_e_maiusculas_nao_mudam_a_decisao():
    assert (
        classificar("COMPARE as OPÇÕES, PRÓS E CONTRAS").camada
        == classificar("compare as opcoes, pros e contras").camada
    )


def test_imagem_vai_para_a_visao_e_a_etiqueta_forca_a_camada():
    assert classificar("o que é isso?", imagens=1).camada == VISAO
    assert classificar("#pesado oi").camada == PESADO
    assert classificar("  #rapido " + CODIGO).camada == RAPIDO
    assert classificar("#visao descreva", imagens=0).camada == VISAO
    assert classificar("#pesado", imagens=2).camada == PESADO  # a etiqueta vence a imagem
    assert classificar("isto #pesado não é etiqueta").camada == RAPIDO  # só no começo
    assert classificar("#pesados").camada == RAPIDO


def test_e_deterministico():
    assert {classificar(CODIGO) for _ in range(5)} == {classificar(CODIGO)}


# ── camadas no gateway ────────────────────────────────────────────────────────
def gw(handler, *eps):
    return ChatGateway(
        list(eps),
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
        clock=lambda: 0.0,
    )


def eps():
    return [
        Endpoint("gateway", "http://g.test/v1", "padrao"),
        Endpoint("rapido", "http://g.test/v1", "leve", tier="rapido"),
        Endpoint("pesado", "http://g.test/v1", "forte", tier="pesado"),
    ]


async def test_a_camada_pedida_vai_primeiro_e_o_padrao_e_o_reserva():
    modelos = []

    def handler(req):
        import json

        modelos.append(json.loads(req.content)["model"])
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    g = gw(handler, *eps())
    await coletar(g, tier="pesado")
    await coletar(g, tier="rapido")
    await coletar(g)  # sem camada: só o padrão
    await coletar(g, tier="visao")  # camada sem endpoint próprio: cai no padrão
    assert modelos == ["forte", "leve", "padrao", "padrao"]


async def test_camada_que_falha_cai_no_padrao_e_a_outra_camada_nunca_entra():
    chamados = []

    def handler(req):
        import json

        modelo = json.loads(req.content)["model"]
        chamados.append(modelo)
        if modelo == "forte":
            return httpx.Response(503)
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    g = gw(handler, *eps())
    await coletar(g, tier="pesado")
    assert chamados == ["forte", "padrao"]  # o "leve" nunca é tentado para trabalho pesado


async def test_configuracao_so_com_camadas_ainda_responde():
    visto = []

    def handler(req):
        import json

        visto.append(json.loads(req.content)["model"])
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    g = gw(handler, Endpoint("so", "http://g.test/v1", "unico", tier="rapido"))
    await coletar(g)
    assert visto == ["unico"]


def test_estatisticas_dizem_a_camada_de_cada_endpoint():
    g = gw(lambda r: httpx.Response(200), *eps())
    assert [(e["nome"], e["camada"]) for e in g.stats()] == [
        ("gateway", "padrão"),
        ("rapido", "rapido"),
        ("pesado", "pesado"),
    ]


# ── agente ────────────────────────────────────────────────────────────────────
@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "r.db")
    yield s
    s.close()


@pytest.fixture
def policy(tmp_path):
    guard = PathGuard(protected_roots=(tmp_path / "p",), safe_roots=(tmp_path / "Documents",))
    return PolicyEngine(path_guard=guard, approvals=ApprovalStore())


def agente(store, policy, *roteiros, routing=True):
    g = FakeGateway(*roteiros)
    return Agent(
        gateway=g, tools=ToolRegistry(memory_tools(store)), policy=policy, memory=store,
        routing=routing,
    ), g  # fmt: skip


async def rodar(a, canal, msg, imagens=()):
    return [e async for e in a.run(canal, msg, imagens)]


async def test_agente_manda_a_camada_ao_gateway_e_guarda_o_motivo(store, policy):
    a, g = agente(store, policy, fala("oi!"), fala("feito."), fala("é um gato."))
    await rodar(a, "web", "oi")
    await rodar(a, "web", CODIGO)
    await rodar(a, "web", "o que é isso?", ["data:image/png;base64,AAAA"])
    assert g.camadas == [RAPIDO, PESADO, VISAO]
    assert dict(a.rotas) == {RAPIDO: 1, PESADO: 1, VISAO: 1}
    sessao = store.active_session("web")
    prov = [m.provenance for m in store.history(sessao.id) if m.role == "assistant"]
    assert [p["roteamento"]["camada"] for p in prov] == [RAPIDO, PESADO, VISAO]
    assert "código" in prov[1]["roteamento"]["motivo"]


async def test_retomada_depois_da_aprovacao_continua_na_camada_do_pedido(store, policy):
    a, g = agente(
        store, policy,
        pede(chama("esquecer_fato", id=1)), fala("Aguardando."), fala("Esqueci."),
    )  # fmt: skip
    store.add_fact("fato a esquecer", "teste")
    eventos = await rodar(a, "web", CODIGO)
    (cartao,) = [e.data for e in eventos if e.kind == "approval"]
    policy.approvals.decide(cartao["id"], True, channel="web", actor="antonio")
    async for _ in a.resume("web", cartao["id"]):
        pass
    assert g.camadas == [PESADO, PESADO, PESADO]  # as duas voltas do pedido e a retomada


async def test_sem_roteamento_o_gateway_nao_recebe_camada(store, policy):
    a, g = agente(store, policy, fala("oi!"), routing=False)
    await rodar(a, "web", CODIGO)
    assert g.camadas == [None] and dict(a.rotas) == {}
    prov = [m.provenance for m in store.history(store.active_session("web").id)]
    assert all("roteamento" not in (p or {}) for p in prov)


# ── configuração ──────────────────────────────────────────────────────────────
def cfg(tmp_path, **kw):
    base = {"gateway_url": "http://127.0.0.1:20128/v1", "gateway_model": "padrao"}
    return Settings(_env_file=None, data_dir=tmp_path, **{**base, **kw})


def test_so_as_camadas_com_modelo_proprio_viram_endpoints(tmp_path):
    g = gateway_from_settings(cfg(tmp_path))
    assert g is not None and [(e.name, e.tier) for e in g.endpoints] == [("gateway", "")]
    g = gateway_from_settings(
        cfg(tmp_path, gateway_model_fast="leve", gateway_model_heavy="forte", vision_model="olho")
    )
    assert g is not None
    assert [(e.name, e.model, e.tier) for e in g.endpoints] == [
        ("gateway", "padrao", ""),
        ("rapido", "leve", "rapido"),
        ("pesado", "forte", "pesado"),
        ("visao", "olho", "visao"),
    ]
    assert [e.timeout_s for e in g.endpoints] == [60.0, 60.0, 120.0, 90.0]
    # modelo igual ao padrão não vira endpoint duplicado
    g = gateway_from_settings(cfg(tmp_path, gateway_model_fast="padrao"))
    assert g is not None and len(g.endpoints) == 1


def test_roteamento_so_liga_quando_ha_o_que_escolher(tmp_path):
    assert roteamento_ligado(cfg(tmp_path)) is False
    assert roteamento_ligado(cfg(tmp_path, gateway_model_heavy="forte")) is True
    assert roteamento_ligado(cfg(tmp_path, vision_model="olho")) is True

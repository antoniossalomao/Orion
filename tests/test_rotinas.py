"""E9.1 (REGRAS.md: automação sua vale pelo arquivo concedido): rotinas suas, concedidas pelo hash, com gatilhos e aprovação onde precisa."""

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.palmas import acao_das_palmas
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.ponte.config import TeclasInvalidas, mapa_de_teclas
from orion.ponte.rotas import rota_permitida
from orion.rotinas import (
    EXEMPLO_BOM_DIA,
    Executor,
    RotinaStore,
    normalizar_frase,
)
from orion.tools import Tool, ToolRegistry
from tests.fakes import FakeGateway, fala
from tests.test_ponte import montar as montar_ponte
from tests.test_telegram import (  # noqa: F401 — fixtures e ajudantes do canal de mentira
    USER,
    clique,
    montar,
    msg,
    policy,
    store,
    tg,
)

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}

BOM_DIA = """
descricao: Bom dia
gatilhos:
  - {tipo: frase, valor: "Bom dia, Orion!"}
  - {tipo: telegram}
liberar: [abrir_app]
passos:
  - briefing: true
  - ferramenta: controlar_midia
    argumentos: {acao: tocar_pausar}
  - ferramenta: abrir_app
    argumentos: {nome: orion}
  - falar: "Tudo pronto."
"""


@pytest.fixture
def mem(tmp_path):
    m = MemoryStore(tmp_path / "r.db")
    yield m
    m.close()


class Modos:
    def __init__(self, panico=False, dnd=False):
        self._p, self._d = panico, dnd

    def panico(self):
        return self._p

    def nao_perturbe(self):
        return self._d


def montar_executor(mem, tmp_path, modos=None, **extra):
    chamadas = []

    def rec(nome, resultado=None):
        def fn(**kw):
            chamadas.append((nome, kw))
            return resultado or {"ok": True}

        return Tool(nome, nome, {"type": "object", "properties": {}}, fn)

    ferramentas = [
        rec("controlar_midia"),
        rec("abrir_app"),
        rec("listar_numeros"),
        rec("gerenciar_lembretes"),
        rec("controlar_janela", {"ok": False, "erro": "sem janela"}),
        *extra.get("extras", []),
    ]
    politica = PolicyEngine(
        path_guard=PathGuard(protected_roots=(), safe_roots=(tmp_path,), system_roots=()),
        approvals=ApprovalStore(),
    )
    pasta = tmp_path / "rotinas"
    pasta.mkdir(exist_ok=True)
    rs = RotinaStore(pasta, mem)
    ops = Operations(mem)
    ex = Executor(
        store=rs,
        tools=ToolRegistry(ferramentas),
        policy=politica,
        memory=mem,
        ops=ops,
        briefing=lambda: "Bom dia! Hoje: nada.",
        modos=modos,
    )
    return ex, rs, ops, chamadas, pasta, politica


def escrever(pasta, nome, texto):
    (pasta / f"{nome}.yaml").write_text(texto, encoding="utf-8")


# ── arquivo e concessão ───────────────────────────────────────────────────────
def test_frase_ignora_acento_caixa_e_pontuacao():
    assert normalizar_frase("  Bom  dia, ÓRION!! ") == "bom dia orion"
    assert normalizar_frase("???") == ""


def test_arquivo_invalido_vira_erro_legivel_sem_derrubar_a_lista(mem, tmp_path):
    pasta = tmp_path / "rotinas"
    pasta.mkdir()
    escrever(pasta, "boa", BOM_DIA)
    escrever(pasta, "quebrada", "passos: [")
    escrever(pasta, "sem-passo", "descricao: x\npassos: []")
    escrever(pasta, "tipo-duplo", "passos:\n  - {falar: oi, briefing: true}")
    escrever(pasta, "liberar-ruim", "liberar: [apagar_arquivo]\npassos: [{falar: oi}]")
    escrever(pasta, "campo-extra", "passos: [{falar: oi}]\nsegredo: 1")
    (pasta / "grande.yaml").write_text("passos: [{falar: oi}]\n" + "# " + "x" * 20000)
    escrever(pasta, "Maiuscula", "passos: [{falar: oi}]")  # nome inválido: nem aparece
    lista = {c.nome: c for c in RotinaStore(pasta, mem).listar()}
    assert set(lista) == {
        "boa",
        "quebrada",
        "sem-passo",
        "tipo-duplo",
        "liberar-ruim",
        "campo-extra",
        "grande",
    }
    assert lista["boa"].erro is None and lista["boa"].rotina is not None
    for ruim in set(lista) - {"boa"}:
        assert lista[ruim].rotina is None and lista[ruim].erro, ruim
    assert "liberar" in lista["liberar-ruim"].erro


def test_concessao_cai_quando_o_arquivo_muda(mem, tmp_path):
    pasta = tmp_path / "rotinas"
    pasta.mkdir()
    escrever(pasta, "boa", BOM_DIA)
    s = RotinaStore(pasta, mem)
    assert s.obter("boa").estado == "sem_concessao"
    assert s.conceder("boa").estado == "concedida"
    assert s.por_frase("bom dia orion").nome == "boa"
    escrever(pasta, "boa", BOM_DIA + "\n# editado por alguém\n")
    assert s.obter("boa").estado == "mudou"
    assert s.por_frase("bom dia orion") is None  # não concedida não responde à frase
    assert s.conceder("boa").estado == "concedida"
    assert s.revogar("boa") is True and s.revogar("boa") is False
    assert s.obter("boa").estado == "sem_concessao"
    with pytest.raises(KeyError):
        s.conceder("nao-existe")
    escrever(pasta, "ruim", "passos: [")
    with pytest.raises(ValueError, match="tem erro"):
        s.conceder("ruim")
    assert s.obter("../fora") is None


def test_exemplo_do_bom_dia_e_valido(mem, tmp_path):
    pasta = tmp_path / "rotinas"
    pasta.mkdir()
    escrever(pasta, "bom-dia", EXEMPLO_BOM_DIA)
    c = RotinaStore(pasta, mem).obter("bom-dia")
    assert c.erro is None and len(c.rotina.passos) == 4 and c.rotina.liberar == ["abrir_app"]


# ── executor ──────────────────────────────────────────────────────────────────
async def test_rotina_concedida_roda_briefing_midia_app_e_fala(mem, tmp_path):
    ex, rs, ops, chamadas, pasta, _ = montar_executor(mem, tmp_path)
    escrever(pasta, "bom-dia", BOM_DIA)
    rs.conceder("bom-dia")
    r = await ex.executar("bom-dia", "manual")
    assert r.status == "concluida" and [p["estado"] for p in r.passos] == ["ok"] * 4
    assert chamadas == [
        ("controlar_midia", {"acao": "tocar_pausar"}),
        ("abrir_app", {"nome": "orion"}),
    ]
    textos = [n["text"] for n in ops.pending_notifications()]
    assert textos == ["Bom dia! Hoje: nada.", "Tudo pronto."]
    trilha = mem.query("SELECT tool, action, reason FROM audit ORDER BY id")
    assert [t["tool"] for t in trilha] == [
        "rotina_texto", "rotina_controlar_midia", "rotina_abrir_app", "rotina_texto", "rotina"
    ]  # fmt: skip
    assert "rotina=bom-dia" in trilha[1]["reason"]


async def test_nao_concedida_ou_alterada_nao_roda(mem, tmp_path):
    ex, rs, _, chamadas, pasta, _ = montar_executor(mem, tmp_path)
    escrever(pasta, "r", BOM_DIA)
    assert (await ex.executar("r")).status == "nao_concedida"
    rs.conceder("r")
    escrever(pasta, "r", BOM_DIA + "# mudou")
    r = await ex.executar("r")
    assert r.status == "nao_concedida" and "mudou" in r.motivo and chamadas == []
    assert (await ex.executar("nao-existe")).status == "falhou"


async def test_abrir_app_sem_liberar_pede_aprovacao_e_a_rotina_para_ali(mem, tmp_path):
    ex, rs, ops, chamadas, pasta, politica = montar_executor(mem, tmp_path)
    escrever(
        pasta,
        "r",
        "passos:\n  - {ferramenta: controlar_midia, argumentos: {acao: proxima}}\n"
        "  - {ferramenta: abrir_app, argumentos: {nome: calc}}\n  - {falar: nunca chega}",
    )
    rs.conceder("r")
    r = await ex.executar("r", "manual")
    assert r.status == "aguardando_aprovacao" and r.aprovacao_id
    assert [p["estado"] for p in r.passos] == ["ok", "aguardando_aprovacao"]
    assert [c[0] for c in chamadas] == ["controlar_midia"]  # abrir_app NÃO rodou
    assert ops.pending_notifications() == []  # o passo seguinte não rodou
    a = politica.approvals.get(r.aprovacao_id)
    assert a.tool == "abrir_app" and a.args == {"nome": "calc"} and a.status.value == "pending"


async def test_escrita_comum_tambem_pede_aprovacao_mesmo_que_a_politica_deixasse(mem, tmp_path):
    ex, rs, _, chamadas, pasta, politica = montar_executor(mem, tmp_path)
    escrever(
        pasta, "r", "passos:\n  - {ferramenta: gerenciar_lembretes, argumentos: {acao: criar}}"
    )
    rs.conceder("r")
    r = await ex.executar("r")
    assert r.status == "aguardando_aprovacao" and chamadas == []
    assert politica.approvals.get(r.aprovacao_id).tool == "gerenciar_lembretes"


async def test_leitura_roda_direto_e_ferramenta_desconhecida_ou_com_falha_para(mem, tmp_path):
    ex, rs, _, chamadas, pasta, _ = montar_executor(mem, tmp_path)
    escrever(pasta, "le", "passos:\n  - {ferramenta: listar_numeros}\n  - {falar: ok}")
    rs.conceder("le")
    assert (await ex.executar("le")).status == "concluida" and chamadas == [("listar_numeros", {})]
    escrever(pasta, "some", "passos:\n  - {ferramenta: ferramenta_que_nao_existe}")
    rs.conceder("some")
    r = await ex.executar("some")
    assert r.status == "falhou" and "não existe" in r.motivo
    escrever(
        pasta,
        "janela",
        "passos:\n  - {ferramenta: controlar_janela, argumentos: {acao: listar}}\n  - {falar: nunca}",
    )
    rs.conceder("janela")
    # controlar_janela é EXEC: pede aprovação (não é leitura nem mídia)
    assert (await ex.executar("janela")).status == "aguardando_aprovacao"


async def test_passo_direto_que_falha_para_a_rotina(mem, tmp_path):
    falha = Tool(
        "controlar_midia",
        "x",
        {"type": "object", "properties": {}},
        lambda **k: {"ok": False, "erro": "sem player"},
    )
    chamadas = []
    ex, rs, _, _, pasta, _ = montar_executor(mem, tmp_path)
    ex.tools = ToolRegistry(
        [
            falha,
            Tool(
                "abrir_app",
                "x",
                {"type": "object", "properties": {}},
                lambda **k: chamadas.append(k) or {"ok": True},
            ),
        ]
    )
    escrever(
        pasta,
        "r",
        "liberar: [abrir_app]\npassos:\n  - {ferramenta: controlar_midia, argumentos: {acao: mudo}}\n  - {ferramenta: abrir_app, argumentos: {nome: x}}",
    )
    rs.conceder("r")
    r = await ex.executar("r")
    assert r.status == "falhou" and "sem player" in r.motivo and chamadas == []


async def test_panico_nao_perturbe_e_gatilho_exigido(mem, tmp_path):
    ex, rs, _, chamadas, pasta, _ = montar_executor(mem, tmp_path, modos=Modos(panico=True))
    escrever(pasta, "r", BOM_DIA)
    rs.conceder("r")
    r = await ex.executar("r", "manual")
    assert r.status == "recusada" and "pânico" in r.motivo and chamadas == []
    ex._modos = Modos(dnd=True)
    assert (await ex.executar("r", "palmas")).status == "recusada"  # gatilho automático
    assert (await ex.executar("r", "tecla")).status == "recusada"
    assert (await ex.executar("r", "manual")).status == "concluida"  # você pediu: vale
    assert (await ex.executar("r", "telegram")).status == "concluida"
    escrever(pasta, "sem-gatilho", "passos: [{falar: oi}]")
    rs.conceder("sem-gatilho")
    r = await ex.executar("sem-gatilho", "frase")
    assert r.status == "recusada" and "gatilho 'frase'" in r.motivo
    assert (await ex.executar("sem-gatilho", "telegram")).status == "recusada"
    ex._modos = None
    assert (await ex.executar("sem-gatilho", "palmas")).status == "concluida"  # a config escolheu


async def test_aprovacao_de_rotina_do_telegram_fica_no_canal_telegram(mem, tmp_path):
    ex, rs, _, _, pasta, _ = montar_executor(mem, tmp_path)
    escrever(
        pasta,
        "r",
        "gatilhos: [{tipo: telegram}]\npassos: [{ferramenta: gerenciar_lembretes, argumentos: {acao: listar}}]",
    )
    rs.conceder("r")
    r = await ex.executar("r", "telegram")
    sessao = mem.get_session(ex.policy.approvals.get(r.aprovacao_id).session_id)
    assert sessao.channel == "telegram" and sessao.title == "Rotina: r"


# ── gatilhos ──────────────────────────────────────────────────────────────────
def test_palmas_com_rotina_so_acionam_a_rotina_e_respeitam_o_bloqueio():
    chamadas = []
    bloqueio = [""]
    palma = acao_das_palmas(
        "rotina:bom-dia",
        abrir_ponte=lambda: chamadas.append("ponte") or True,
        abrir_navegador=lambda: chamadas.append("navegador"),
        bloqueado=lambda: bloqueio[0],
        rotina=lambda nome: chamadas.append(f"rotina:{nome}"),
    )
    assert palma() is False and chamadas == ["rotina:bom-dia"]  # não abre nada por conta própria
    bloqueio[0] = "pânico"
    assert palma() is None and chamadas == ["rotina:bom-dia"]
    sem = acao_das_palmas(  # sem o executor: cai no comportamento de sempre (abrir)
        "rotina:x", abrir_ponte=lambda: True, abrir_navegador=lambda: None, bloqueado=lambda: ""
    )
    assert sem() is False


def test_tecla_de_rotina_na_ponte_so_chama_a_rota_da_rotina():
    pedidos = []

    def handler(req):
        pedidos.append((req.method, req.url.path, dict(req.url.params)))
        import httpx

        return httpx.Response(200, json={"status": "concluida"})

    ponte, _, _ = montar_ponte(handler)
    ponte.teclas = mapa_de_teclas('{"rotina:bom-dia": "ctrl+alt+b"}')
    acoes = ponte.acoes_das_teclas()
    assert "<ctrl>+<alt>+b" in acoes
    acoes["<ctrl>+<alt>+b"]()
    assert pedidos == [("POST", "/rotinas/bom-dia/executar", {"origem": "tecla"})]
    assert rota_permitida("POST", "/rotinas/bom-dia/executar")
    assert not rota_permitida("POST", "/rotinas/bom-dia/conceder")
    assert not rota_permitida("GET", "/rotinas")
    assert not rota_permitida("POST", "/rotinas/../x/executar")
    for ruim in ('{"rotina:Maiúscula": "ctrl+alt+b"}', '{"rotina:a": 5}', '{"rotina:a": "b"}'):
        with pytest.raises(TeclasInvalidas):
            mapa_de_teclas(ruim)
    with pytest.raises(TeclasInvalidas):
        mapa_de_teclas('{"rotina:a": "ctrl+alt+b", "rotina:b": "ctrl+alt+b"}')


# ── API ───────────────────────────────────────────────────────────────────────
def _app(tmp_path, gw=None):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    (tmp_path / "d" / "rotinas").mkdir(parents=True)
    (tmp_path / "d" / "rotinas" / "bom-dia.yaml").write_text(
        "gatilhos: [{tipo: frase, valor: bom dia}]\npassos:\n  - {falar: Olá!}\n", encoding="utf-8"
    )
    return create_app(settings, gateway_factory=lambda _: gw or FakeGateway(fala("não devia")))


def test_rotas_da_api_e_frase_no_chat(tmp_path):
    with TestClient(_app(tmp_path), base_url="http://127.0.0.1") as c:
        assert c.get("/rotinas").status_code == 401
        lista = c.get("/rotinas", headers=AUTH).json()
        assert lista["total"] == 1 and lista["rotinas"][0]["estado"] == "sem_concessao"
        r = c.post("/rotinas/bom-dia/executar", headers=AUTH).json()
        assert r["status"] == "nao_concedida"
        assert (
            c.post("/rotinas/por-frase", json={"texto": "bom dia"}, headers=AUTH).status_code == 404
        )
        assert c.post("/rotinas/bom-dia/conceder", headers=AUTH).json()["estado"] == "concedida"
        assert c.post("/rotinas/nada/conceder", headers=AUTH).status_code == 404
        assert c.post("/rotinas/bom-dia/executar?origem=hacker", headers=AUTH).status_code == 422
        assert c.post("/rotinas/nada/executar", headers=AUTH).status_code == 404
        # a frase exata, pelo chat, roda a rotina em vez de ir ao modelo
        sse = c.post("/chat", json={"texto": "Bom dia!"}, headers=AUTH).text
        assert "Rotina bom-dia: concluida" in sse and "não devia" not in sse
        assert c.app.state.orion.ops.pending_notifications()[0]["text"] == "Olá!"
        # frase parecida mas diferente vai ao modelo
        assert (
            "não devia" in c.post("/chat", json={"texto": "bom dia, tudo bem?"}, headers=AUTH).text
        )
        assert c.post("/rotinas/bom-dia/revogar", headers=AUTH).json() == {"ok": True}
        assert (
            c.post("/rotinas/por-frase", json={"texto": "bom dia"}, headers=AUTH).status_code == 404
        )


def test_frase_no_chat_com_passo_que_pede_aprovacao_mostra_o_cartao(tmp_path):
    app = _app(tmp_path)
    (tmp_path / "d" / "rotinas" / "bom-dia.yaml").write_text(
        "gatilhos: [{tipo: frase, valor: bom dia}]\npassos:\n  - {ferramenta: gerenciar_lembretes, argumentos: {acao: listar}}\n",
        encoding="utf-8",
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        c.post("/rotinas/bom-dia/conceder", headers=AUTH)
        sse = c.post("/chat", json={"texto": "bom dia"}, headers=AUTH).text
        assert (
            "aguardando aprovacao" in sse and '"approval"' in sse and "gerenciar_lembretes" in sse
        )


# ── Telegram ──────────────────────────────────────────────────────────────────
async def test_telegram_lista_roda_por_comando_e_por_frase(store, policy, tg, mem, tmp_path):  # noqa: F811
    canal, gw, ops = montar(store, policy, tg, fala("do modelo"))
    pasta = tmp_path / "rotinas"
    pasta.mkdir()
    escrever(
        pasta,
        "bom-dia",
        BOM_DIA.replace("liberar: [abrir_app]\n", "")
        .replace("ferramenta: abrir_app", "ferramenta: listar_numeros")
        .replace("    argumentos: {nome: orion}\n", ""),
    )
    escrever(
        pasta,
        "aprova",
        "gatilhos: [{tipo: telegram}]\n"
        "passos: [{ferramenta: gerenciar_lembretes, argumentos: {acao: listar}}]",
    )
    rotinas = RotinaStore(pasta, store)
    ex = Executor(
        store=rotinas,
        tools=ToolRegistry(
            [
                Tool(
                    "controlar_midia",
                    "x",
                    {"type": "object", "properties": {}},
                    lambda **k: {"ok": True},
                ),
                Tool(
                    "listar_numeros",
                    "x",
                    {"type": "object", "properties": {}},
                    lambda **k: {"ok": True},
                ),
                Tool(
                    "gerenciar_lembretes",
                    "x",
                    {"type": "object", "properties": {}},
                    lambda **k: {"ok": True},
                ),
            ]
        ),
        policy=policy,
        memory=store,
        ops=ops,
    )
    canal.rotinas, canal.executar_rotina = rotinas, ex.executar
    await canal.handle_update(msg("/rotina"))
    assert (
        "bom-dia — sem concessao" in tg.textos()[-1].replace("_", "")
        or "bom-dia — sem concessao" in tg.textos()[-1]
    )
    rotinas.conceder("bom-dia")
    rotinas.conceder("aprova")
    await canal.handle_update(msg("/rotina bom-dia"))
    assert tg.textos()[-1].startswith("✅ Rotina bom-dia: concluida")
    await canal.handle_update(msg("Bom dia, Orion!"))  # frase exata: roda, não vai ao modelo
    assert tg.textos()[-1].startswith("✅ Rotina bom-dia") and gw.chamadas == []
    await canal.handle_update(msg("/rotina aprova"))
    assert "aguardando aprovacao" in tg.textos()[-2]
    cartao = tg.enviadas()[-1]
    assert "gerenciar_lembretes" in cartao["text"] and cartao["reply_markup"]["inline_keyboard"]
    await canal.handle_update(msg("/rotina nada"))
    assert "não existe" in tg.textos()[-1]

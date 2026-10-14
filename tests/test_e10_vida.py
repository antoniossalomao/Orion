"""E10: vida pessoal: finanças locais (regra 61), escudo de foco (regra 62), estado pela voz
(regra 63), contas, preços, cápsula, diário, revisão e cenários."""

from datetime import date, datetime
from types import SimpleNamespace

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.vida import (
    EstadoVoz,
    Financas,
    Foco,
    Precos,
    VidaError,
    VidaJobs,
    avaliar_preco,
    capsula,
    categoria_de,
    e_distracao,
    escrever_no_vault,
    extrair_preco,
    montar_diario,
    montar_revisao,
    parse_gasto,
    parse_linha_digitavel,
    parse_valor,
    simular,
)
from tests.fakes import FakeGateway, fala

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
# 2026-10-14 12:00 local, em epoch
AGORA = datetime(2026, 10, 14, 12, 0).timestamp()


@pytest.fixture
def memoria(tmp_path):
    m = MemoryStore(tmp_path / "m.db")
    yield m
    m.close()


def test_valor_aceita_formatos_brasileiros_e_recusa_zero_e_lixo():
    assert parse_valor("30") == 3000
    assert parse_valor("30,5") == 3050
    assert parse_valor("R$ 1.234,56") == 123456
    assert parse_valor("12.90") == 1290
    assert parse_valor("1.500") == 150000
    for ruim in ("0", "-3", "abc"):
        with pytest.raises(VidaError):
            parse_valor(ruim)


def test_gasto_por_frase_com_categoria_por_palavra():
    g = parse_gasto("gastei 30 no mercado")
    assert g == {"cents": 3000, "category": "mercado", "description": "mercado"}
    assert parse_gasto("paguei R$ 12,50 na padaria")["category"] == "mercado"
    assert parse_gasto("gastei 20 com uber")["category"] == "transporte"
    assert parse_gasto("gastei 9 numa coisa qualquer")["category"] == "outros"
    assert parse_gasto("gastei muito") is None
    assert parse_gasto("ontem almocei") is None
    assert categoria_de("Almoço no Café") == "alimentação"


def test_linha_digitavel_de_boleto_traz_valor_e_vencimento():
    # fator 1000 a partir de 07/10/1997 = 03/07/2000; valor 123,45
    digitos = "0" * 32 + "9" + "1000" + "0000012345"
    assert len(digitos) == 47
    linha = f"{digitos[:5]}.{digitos[5:10]} {digitos[10:15]}.{digitos[15:21]} "
    linha += f"{digitos[21:26]}.{digitos[26:32]} {digitos[32]} {digitos[33:]}"
    b = parse_linha_digitavel(f"vence logo: {linha}")
    assert b["cents"] == 12345 and b["due"] == date(2000, 7, 3)
    assert parse_linha_digitavel("sem boleto aqui 123") is None


def test_preco_e_comparacao():
    assert extrair_preco("Notebook por R$ 1.299,90 à vista") == 129990
    assert extrair_preco("sem preço") is None
    assert avaliar_preco(None, 1000, 900) is None
    assert avaliar_preco(1000, 880, 900) == "alvo"
    assert avaliar_preco(1000, 940, None) == "queda"  # 6% abaixo
    assert avaliar_preco(1000, 980, None) is None  # 2%: ruído
    assert avaliar_preco(1000, 880, 900) == "alvo"  # cruzou o alvo


def test_gastos_resumo_e_desfazer(memoria):
    f = Financas(memoria, clock=lambda: AGORA)
    f.registrar_gasto(3000, "mercado", "mercado")
    f.registrar_gasto(2000, "transporte", "uber", source="captura")
    r = f.resumo_mensal(2026, 10)
    assert r == {
        "mes": "2026-10",
        "total_cents": 5000,
        "por_categoria": {"mercado": 3000, "transporte": 2000},
    }
    assert f.resumo_mensal(2026, 9)["total_cents"] == 0
    f.apagar_gasto(2)
    assert f.resumo_mensal(2026, 10)["total_cents"] == 3000
    with pytest.raises(VidaError):
        f.registrar_gasto(0, "outros", "x")


def test_conta_avisa_3_e_1_dia_antes_uma_vez_e_some_ao_pagar(memoria):
    f = Financas(memoria, clock=lambda: AGORA)
    conta = f.cadastrar_conta("Luz", 15000, date(2026, 10, 17))  # restam 3 dias
    avisos = []

    def notificar(kind, texto, ref):
        avisos.append((kind, texto, ref))

    assert f.avisos_de_contas(AGORA, notificar) == 1
    assert f.avisos_de_contas(AGORA, notificar) == 0  # mesmo dia: não repete
    assert avisos[0][0] == "conta" and "vence em 3 dia(s)" in avisos[0][1]
    amanha = AGORA + 86400 * 2  # 16/10: restava 1 dia
    assert f.avisos_de_contas(amanha, notificar) == 1
    assert len(avisos) == 2
    assert f.contas_abertas()[0]["id"] == conta["id"]
    f.pagar_conta(conta["id"])
    assert f.contas_abertas() == []
    assert f.resumo_mensal(2026, 10)["por_categoria"]["contas"] == 15000
    with pytest.raises(VidaError, match="já paga"):
        f.pagar_conta(conta["id"])


def test_monitor_de_preco_consulta_uma_vez_por_dia(memoria):
    p = Precos(memoria, clock=lambda: AGORA)
    item = p.adicionar("Mouse", "mouse sem fio logitech preço", 5000)
    respostas = iter(["R$ 60,00", "R$ 49,90", "sem preço"])
    chamadas = []

    def pesquisar(consulta):
        chamadas.append(consulta)
        return next(respostas)

    assert p.verificar(pesquisar, agora=AGORA) == []  # 60,00: sem alerta
    assert p.verificar(pesquisar, agora=AGORA) == []  # mesmo dia: não consulta de novo
    assert len(chamadas) == 1
    alertas = p.verificar(pesquisar, agora=AGORA + 86400)  # 49,90 cruza o alvo de 50
    assert alertas == [{"id": item["id"], "item": "Mouse", "motivo": "alvo", "cents": 4990}]
    assert p.listar()[0]["ultimo_cents"] == 4990


def test_capsula_vira_lembrete_do_tipo_capsula(memoria):
    r = capsula(memoria, "cobrar o empréstimo do Carlos", 180, "conversa sobre o carro")
    linha = memoria.query("SELECT kind, title, note FROM reminders WHERE id=?", (r["id"],))[0]
    assert linha["kind"] == "capsula" and linha["title"].startswith("cobrar")
    with pytest.raises(VidaError):
        capsula(memoria, "x", 0)


def test_cenarios_sao_faixas_e_respeitam_o_historico():
    faixas = simular(100_000, [50_000, 60_000], 0, 6, rodadas=2000, seed=1)
    assert faixas["p10"] <= faixas["p50"] <= faixas["p90"]
    # 6 meses de 50 mil a 60 mil: o saldo final fica entre 100 mil - 360 mil e 100 mil - 300 mil
    assert 100_000 - 360_000 <= faixas["p10"] and faixas["p90"] <= 100_000 - 300_000
    assert simular(100_000, [50_000], 0, 6, seed=1) == simular(100_000, [50_000], 0, 6, seed=1)
    with pytest.raises(VidaError):
        simular(100_000, [], 0, 6)


def test_diario_tem_frontmatter_e_fica_dentro_do_vault(tmp_path):
    texto = montar_diario(
        date(2026, 10, 14),
        commits=["orion: feat x", "vault: nota"],
        gastos_cents=3000,
        gastos_por_categoria={"mercado": 3000},
    )
    assert texto.startswith("---\ntype: diario\ndate: 2026-10-14")
    assert "- orion: feat x" in texto and "Total do dia: R$ 30,00" in texto
    vault = tmp_path / "vault"
    vault.mkdir()
    arq = escrever_no_vault(vault, "Diário", "2026-10-14", texto)
    assert arq == vault / "Diário" / "2026-10-14.md" and arq.read_text(encoding="utf-8") == texto
    for pasta in ("../fora", "/abs"):
        with pytest.raises(VidaError):
            escrever_no_vault(vault, pasta, "x", "x")
    with pytest.raises(VidaError):
        escrever_no_vault(vault, "Diário", "../../x", "x")


def test_jobs_de_vida_escrevem_diario_e_revisao_uma_vez(memoria, tmp_path):
    vault = tmp_path / "vault"
    vault.mkdir()
    relogio = {"t": datetime(2026, 10, 11, 23, 30).timestamp()}  # domingo
    ops = SimpleNamespace(
        notify=lambda *a, **k: 1,
        list_tasks=lambda: [{"title": "tarefa A", "status": "pendente"}],
    )
    financas = Financas(memoria, clock=lambda: relogio["t"])
    financas.registrar_gasto(2500, "lazer", "cinema")
    vida = VidaJobs(
        memoria,
        financas,
        ops=ops,
        vault_dir=vault,
        diary_at="23:00",
        weekly_at="18:00",
        clock=lambda: relogio["t"],
    )
    primeiro = vida.tick()
    assert primeiro["diario"] and primeiro["revisao"]
    assert (vault / "Diário" / "2026-10-11.md").read_text(encoding="utf-8").count("R$ 25,00") >= 1
    assert "tarefa A" in next((vault / "Revisão").glob("*.md")).read_text(encoding="utf-8")
    assert vida.tick() == {
        "avisos": 0,
        "diario": None,
        "revisao": None,
        "precos": [],
    }  # idempotente
    segunda = datetime(2026, 10, 12, 23, 30).timestamp()
    relogio["t"] = segunda
    assert vida.tick()["revisao"] is None  # revisão é só no domingo


def test_revisao_tem_os_cinco_passos():
    texto = montar_revisao(["a"], [{"nome": "Orion", "proxima": "E10"}])
    for passo in ("1. Tarefas", "2. Escolher", "3. Blocos", "4. Ideias", "5. Bloqueios"):
        assert passo in texto
    assert "**Orion**: próxima ação: E10" in texto


def test_escudo_de_foco_so_pergunta_uma_vez_e_so_no_bloco():
    relogio = {"t": AGORA}
    foco = Foco(["youtube", "Netflix"], clock=lambda: relogio["t"])
    assert not foco.pergunta_permitida("youtube.com")  # sem bloco ativo
    foco.iniciar(30)
    assert e_distracao("YouTube - Vídeo", ["youtube"])
    assert foco.pergunta_permitida("youtube.com") is True
    assert foco.pergunta_permitida("netflix.com") is False  # uma pergunta por bloco
    assert not foco.pergunta_permitida("Visual Studio Code")  # não é distração
    relogio["t"] = AGORA + 31 * 60
    assert not foco.ativo()
    assert not hasattr(foco, "bloquear") and not hasattr(foco, "fechar")  # regra 62


def test_estado_pela_voz_so_muda_o_estilo_depois_de_uma_base():
    voz = EstadoVoz()
    for _ in range(5):
        assert voz.observar(30, 10, 0.5) == "normal"  # 180 palavras/min, energia normal
    assert voz.observar(60, 10, 0.5) == "curto"  # 360 palavras/min: pressa
    assert voz.observar(30, 10, 0.1) == "curto"  # energia caiu
    assert voz.observar(0, 0, 0) == "normal"


def test_api_financas_e_captura_de_gasto(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    (tmp_path / "d").mkdir()
    gw = FakeGateway(fala("oi"))
    with TestClient(
        create_app(settings, gateway_factory=lambda _: gw), base_url="http://127.0.0.1"
    ) as c:
        assert c.post("/financas/gastos", json={"texto": "oi"}, headers=AUTH).status_code == 422
        g = c.post("/financas/gastos", json={"texto": "gastei 30 no mercado"}, headers=AUTH)
        assert g.json()["cents"] == 3000
        resumo = c.get("/financas/resumo", headers=AUTH).json()
        assert resumo["total_cents"] >= 3000
        cap = c.post("/captura", json={"texto": "gastei 12 no uber"}, headers=AUTH).json()
        assert cap["tipo"] == "gasto", cap
        assert c.delete(f"/captura/{cap['id']}", headers=AUTH).json() == {"ok": True}
        assert c.post("/financas/contas", json={"descricao": "Luz", "reais": 150,
                                                 "vencimento": "2026-12-01"},
                      headers=AUTH).status_code == 200  # fmt: skip
        conta = c.get("/financas/contas", headers=AUTH).json()["contas"][0]
        assert c.post(f"/financas/contas/{conta['id']}/pagar", headers=AUTH).json()["paga"]
        assert c.post(f"/financas/contas/{conta['id']}/pagar", headers=AUTH).status_code == 409
        assert c.post("/precos", json={"item": "Mouse", "consulta": "mouse preço",
                                        "alvo_reais": 50}, headers=AUTH).status_code == 200  # fmt: skip
        assert len(c.get("/precos", headers=AUTH).json()["itens"]) == 1
        sim = c.post("/vida/simular", json={"saldo_reais": 1000, "meses": 3}, headers=AUTH)
        assert sim.status_code == 200 and sim.json()["base_meses"] >= 1
        assert c.get("/financas/resumo", headers=AUTH).status_code == 200
        assert c.get("/financas/resumo").status_code == 401

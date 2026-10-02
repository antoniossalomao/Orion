import sys

import pytest

from orion.delegate import CliAgent, Delegator
from orion.memory import MemoryStore


def fake(nome: str, codigo: str, limite: int = 2) -> CliAgent:
    """CLI falsa: um Python que recebe o prompt como argv[1]."""
    return CliAgent(nome, (sys.executable, "-c", codigo, "{prompt}"), limite)


OK = "import sys; print('feito:' + sys.argv[1])"
COTA = "import sys; sys.stderr.write('Error 429: usage limit reached'); sys.exit(1)"
FALHA = "import sys; sys.stderr.write('SyntaxError no projeto'); sys.exit(2)"


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "d.db")
    yield s
    s.close()


@pytest.fixture
def pasta(tmp_path):
    p = tmp_path / "projeto"
    p.mkdir()
    return p


def delegador(store, *agentes, **kw):
    return Delegator(store, agents=agentes, which=lambda _: "/bin/x", **kw)


def test_executa_na_pasta_e_devolve_a_saida(store, pasta):
    d = delegador(store, fake("claude", "import os, sys; print(os.getcwd() + '|' + sys.argv[1])"))
    r = d.delegate("refatore o módulo", str(pasta))
    assert r["ok"] and r["agente"] == "claude"
    assert r["saida"] == f"{pasta}|Tarefa: refatore o módulo"


def test_prompt_com_hifen_e_com_metacaracteres_vai_como_um_argumento(store, pasta):
    d = delegador(store, fake("claude", OK))
    perigo = "--dangerously-skip-permissions; rm -rf / && $(whoami) `id`"
    r = d.delegate(perigo, str(pasta))
    assert r["saida"] == f"feito:Tarefa: {perigo}"  # nada foi interpretado


def test_preferido_primeiro_e_cota_esgotada_cai_para_o_proximo(store, pasta):
    d = delegador(store, fake("claude", COTA), fake("codex", OK), fake("gemini", OK))
    r = d.delegate("x", str(pasta))
    assert r["agente"] == "codex" and r["tentativas"] == [
        {"agente": "claude", "motivo": "cota esgotada"}
    ]
    # claude ficou marcado como esgotado: a próxima chamada nem tenta
    r2 = d.delegate("y", str(pasta))
    assert r2["tentativas"] == [{"agente": "claude", "motivo": "limite diário atingido"}]
    # pedir um agente específico começa por ele
    assert d.delegate("z", str(pasta), agente="gemini")["agente"] == "gemini"


def test_contador_diario_limita_e_zera_no_dia_seguinte(store, pasta):
    t = {"agora": 1_700_000_000.0}
    d = delegador(store, fake("claude", OK, limite=2), clock=lambda: t["agora"])
    assert d.delegate("a", str(pasta))["ok"] and d.delegate("b", str(pasta))["ok"]
    r = d.delegate("c", str(pasta))
    assert not r["ok"] and r["tentativas"][0]["motivo"] == "limite diário atingido"
    t["agora"] += 86_400
    assert d.delegate("d", str(pasta))["ok"]


def test_cli_nao_instalada_e_pulada(store, pasta):
    d = Delegator(
        store,
        agents=(fake("claude", OK), fake("codex", OK)),
        which=lambda _: None,
    )
    assert d.delegate("x", str(pasta)) == {
        "ok": False,
        "erro": "nenhum agente disponível",
        "tentativas": [
            {"agente": "claude", "motivo": "CLI não instalada"},
            {"agente": "codex", "motivo": "CLI não instalada"},
        ],
    }


def test_falha_real_nao_tenta_outro_agente(store, pasta):
    d = delegador(store, fake("claude", FALHA), fake("codex", OK))
    r = d.delegate("x", str(pasta))
    assert not r["ok"] and r["agente"] == "claude" and "SyntaxError" in r["erro"]


def test_tempo_limite(store, pasta):
    d = delegador(store, fake("claude", "import time; time.sleep(5)"), timeout_s=0.3)
    r = d.delegate("x", str(pasta))
    assert not r["ok"] and "tempo-limite" in r["erro"]


def test_entradas_invalidas(store, pasta, tmp_path):
    d = delegador(store, fake("claude", OK))
    assert "tarefa vazia" in d.delegate("  ", str(pasta))["erro"]
    assert "inexistente" in d.delegate("x", str(tmp_path / "nao"))["erro"]
    assert "desconhecido" in d.delegate("x", str(pasta), agente="gpt")["erro"]


def test_saida_truncada(store, pasta):
    d = delegador(store, fake("claude", "print('x' * 500)"), max_output=100)
    r = d.delegate("x", str(pasta))
    assert len(r["saida"]) == 100 and r["truncado"]


def test_segredo_do_orion_nao_chega_ao_processo_filho(store, pasta, monkeypatch):
    monkeypatch.setenv("ORION_ADMIN_TOKEN", "segredo")
    monkeypatch.setenv("PATH_DA_CLI", "visivel")
    d = delegador(
        store,
        fake(
            "claude",
            "import os; print(os.environ.get('ORION_ADMIN_TOKEN'), os.environ.get('PATH_DA_CLI'))",
        ),
    )
    assert d.delegate("x", str(pasta))["saida"] == "None visivel"


def test_processo_filho_nao_le_o_teclado(store, pasta):
    d = delegador(store, fake("claude", "import sys; print(repr(sys.stdin.read()))"))
    assert d.delegate("x", str(pasta))["saida"] == "''"


def test_erro_do_so_ao_executar(store, pasta):
    def run(*a, **k):
        raise PermissionError("negado")

    d = Delegator(
        store, agents=(fake("claude", OK), fake("codex", OK)), which=lambda _: "/x", run=run
    )
    r = d.delegate("x", str(pasta))
    assert [t["motivo"].split(":")[0] for t in r["tentativas"]] == ["não executou"] * 2


def test_contadores_do_store(store):
    assert store.counter_get("a") == 0
    assert store.counter_incr("a") == 1 and store.counter_incr("a", 4) == 5
    store.counter_set("a", 2)
    assert store.counter_get("a") == 2 and store.counter_get("b") == 0

import json
import sys
import time

import pytest

from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.tools.processes import ProcessManager, process_tools

POSIX = pytest.mark.skipif(sys.platform == "win32", reason="comandos reais de sh")


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "m.db")
    yield s
    s.close()


@pytest.fixture
def ops(store):
    return Operations(store)


def esperar(proc_manager, pid, timeout=10):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if proc_manager.status(pid)["status"] != "rodando":
            return proc_manager.status(pid)
        time.sleep(0.05)
    raise AssertionError("o processo não terminou")


@POSIX
def test_processo_roda_em_segundo_plano_com_log_e_codigo(tmp_path):
    m = ProcessManager(tmp_path / "logs")
    r = m.start("eco", "echo ola; echo erro >&2; exit 3")
    assert r["ok"] and r["processo"]["status"] in ("rodando", "erro")
    st = esperar(m, r["processo"]["id"])
    assert st["status"] == "erro" and st["codigo"] == 3
    assert "ola" in st["log"] and "erro" in st["log"]  # stdout e stderr no mesmo log
    ok = m.start("ok", "true")
    assert esperar(m, ok["processo"]["id"])["status"] == "concluido"


@POSIX
def test_log_so_do_dono_e_nome_do_arquivo_saneado(tmp_path):
    m = ProcessManager(tmp_path / "logs")
    m.start("../../etc/passwd; rm -rf", "true")
    (log,) = list((tmp_path / "logs").glob("*.log"))
    assert ".." not in log.name and "/" not in log.name and log.parent == tmp_path / "logs"
    assert oct(log.stat().st_mode & 0o777) == "0o600"


@POSIX
def test_processo_nao_herda_orion_nem_segredos(tmp_path, monkeypatch):
    monkeypatch.setenv("ORION_ADMIN_TOKEN", "segredo-do-orion-123456")
    monkeypatch.setenv("MINHA_API_KEY", "sk-nao-pode-vazar")
    m = ProcessManager(tmp_path / "logs")
    r = m.start("env", "env")
    log = esperar(m, r["processo"]["id"])["log"]
    assert "ORION_ADMIN_TOKEN" not in log and "MINHA_API_KEY" not in log and "PATH=" in log


@POSIX
def test_limite_de_processos_ativos_e_listagem(tmp_path):
    m = ProcessManager(tmp_path / "logs", max_ativos=2)
    a, b = m.start("a", "sleep 5"), m.start("b", "sleep 5")
    assert a["ok"] and b["ok"]
    assert "já há 2" in m.start("c", "sleep 5")["erro"]
    assert m.listar()["total"] == 2 and m.listar(somente_ativos=False)["total"] == 2
    for pid in (a, b):  # limpeza
        m._procs[pid["processo"]["id"]].popen.kill()


def test_status_de_processo_desconhecido_e_comando_exibido_sem_segredo(tmp_path):
    m = ProcessManager(
        tmp_path / "logs",
        launcher=lambda *a, **k: type("P", (), {"pid": 1, "poll": lambda s: None})(),
    )
    assert "desconhecido" in m.status(99)["erro"]
    r = m.start("x", "curl -H 'Authorization: Bearer gsk_" + "a" * 30 + "' https://x")
    assert "gsk_" not in json.dumps(r["processo"]["comando"])


def test_sem_shell_e_falha_ao_iniciar_viram_erro(tmp_path, monkeypatch):
    monkeypatch.setattr("shutil.which", lambda n: None)
    assert "nenhum shell" in ProcessManager(tmp_path / "l").start("x", "ls")["erro"]
    monkeypatch.undo()

    def quebra(*a, **k):
        raise PermissionError

    assert (
        "não foi possível"
        in ProcessManager(tmp_path / "l", launcher=quebra).start("x", "ls")["erro"]
    )


def test_logs_velhos_sao_podados(tmp_path):
    logs = tmp_path / "logs"
    logs.mkdir()
    velho, novo = logs / "1_1_velho.log", logs / "2_2_novo.log"
    velho.write_text("x")
    novo.write_text("y")
    import os

    antigo = time.time() - 10 * 86400
    os.utime(velho, (antigo, antigo))
    ProcessManager(logs).start("x", "true")
    assert not velho.exists() and novo.exists()


@POSIX
def test_job_avisa_uma_vez_quando_o_processo_termina(tmp_path, store, ops):
    m = ProcessManager(tmp_path / "logs")
    ok, falha, mudo = (
        m.start("bom", "true"),
        m.start("ruim", "exit 2"),
        m.start("quieto", "true", False),
    )
    for r in (ok, falha, mudo):
        esperar(m, r["processo"]["id"])
    jobs = JobRunner(store, ops, processes=m)
    import asyncio

    r1 = asyncio.run(jobs.tick())
    assert r1.processos == 2  # o "quieto" pediu para não avisar
    textos = sorted(n["text"] for n in ops.pending_notifications())
    assert "Processo 'bom' terminou." in textos
    assert any("erro (código 2)" in t for t in textos)
    assert asyncio.run(jobs.tick()).processos == 0  # só uma vez


# ── ferramentas e vigilância de pastas ────────────────────────────────────────
@POSIX
def test_ferramentas_de_processo(tmp_path, ops):
    m = ProcessManager(tmp_path / "logs")
    ts = {t.name: t for t in process_tools(m, ops)}
    r = json.loads(ts["iniciar_processo_bg"].run({"nome": "t", "comando": "echo oi"}))
    pid = r["processo"]["id"]
    esperar(m, pid)
    assert "oi" in json.loads(ts["status_processo_bg"].run({"processo_id": pid}))["log"]
    assert json.loads(ts["listar_processos_bg"].run({"somente_ativos": False}))["total"] == 1


def test_vigilancia_avisa_arquivo_novo_ignora_parciais_e_sobrevive_a_reinicio(tmp_path, store, ops):
    pasta = tmp_path / "downloads"
    pasta.mkdir()
    (pasta / "velho.txt").write_text("x")
    ts = {t.name: t for t in process_tools(ProcessManager(tmp_path / "l"), ops)}
    assert json.loads(ts["iniciar_vigilancia_pasta"].run({"pasta": str(pasta)}))["ok"]
    assert ops.watch_poll() == 0  # nada novo desde a foto
    (pasta / "novo.pdf").write_text("x")
    (pasta / "baixando.crdownload").write_text("x")
    assert ops.watch_poll() == 1
    (n,) = ops.pending_notifications()
    assert "novo.pdf" in n["text"] and "crdownload" not in n["text"]
    assert ops.watch_poll() == 0  # já avisado
    (pasta / "baixando.crdownload").rename(pasta / "terminou.zip")  # o download acabou
    assert ops.watch_poll() == 1
    outra = Operations(store)  # a lista vive no banco: outro objeto enxerga a mesma vigilância
    assert [w["pasta"] for w in outra.watch_list()] == [str(pasta.resolve())]
    assert json.loads(ts["listar_vigilancias"].run({}))["total"] == 1
    assert json.loads(ts["parar_vigilancia_pasta"].run({"pasta": str(pasta)}))["ok"] is True
    assert ops.watch_list() == [] and ops.watch_poll() == 0


def test_vigilancia_lista_longa_pasta_inexistente_e_pasta_que_some(tmp_path, ops):
    assert (
        "não é uma pasta"
        in json.loads(
            {t.name: t for t in process_tools(ProcessManager(tmp_path / "l"), ops)}[
                "iniciar_vigilancia_pasta"
            ].run({"pasta": str(tmp_path / "x")})
        )["erro"]
    )
    pastas = []
    for i in range(20):
        p = tmp_path / f"p{i}"
        p.mkdir()
        ops.watch_add(str(p))
        pastas.append(p)
    extra = tmp_path / "extra"
    extra.mkdir()
    with pytest.raises(ValueError, match="limite"):
        ops.watch_add(str(extra))
    ops.watch_add(str(pastas[0]))  # refazer a mesma não conta como nova
    pastas[3].rmdir()
    assert ops.watch_poll() == 1
    assert "sumiu" in ops.pending_notifications()[0]["text"] and len(ops.watch_list()) == 19

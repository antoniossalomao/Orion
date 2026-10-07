"""Agenda no briefing (regra 37): leitura direta, sem modelo, só se o mcp.json diz `read`."""

from datetime import datetime

import pytest

from orion.agenda import MAX_LINHAS, AgendaDoDia, agenda_do_briefing, formatar
from orion.briefing import build_briefing
from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.policy.classes import Risk, ToolSpec

AGORA = datetime(2026, 10, 5, 7, 30).timestamp()
TOOL = "google__get_events"


class FalsoMcp:
    def __init__(self, resposta=None, erro=None):
        self.resposta, self.erro, self.chamadas = resposta, erro, []

    def __call__(self, nome, args):
        self.chamadas.append((nome, args))
        if self.erro:
            raise self.erro
        return self.resposta


def spec(risk):
    return {TOOL: ToolSpec(TOOL, risk, external=True)}


@pytest.fixture
def ops(tmp_path):
    s = MemoryStore(tmp_path / "a.db", clock=lambda: AGORA)
    yield Operations(s)
    s.close()


def test_so_liga_com_email_e_ferramenta_de_leitura():
    mcp = FalsoMcp({"texto": "x"})
    ok = agenda_do_briefing(tool=TOOL, email="a@b.c", specs=spec(Risk.READ), call=mcp)
    assert isinstance(ok, AgendaDoDia)
    assert agenda_do_briefing(tool=TOOL, email="", specs=spec(Risk.READ), call=mcp) is None
    assert agenda_do_briefing(tool=TOOL, email="a@b.c", specs={}, call=mcp) is None
    for risco in (Risk.WRITE, Risk.EXEC, Risk.DESTRUCTIVE):  # a classe vem do mcp.json
        assert agenda_do_briefing(tool=TOOL, email="a@b.c", specs=spec(risco), call=mcp) is None
    assert mcp.chamadas == []  # nada foi chamado só por montar


def test_chama_a_ferramenta_fixa_com_argumentos_do_codigo_e_o_dia_de_hoje():
    mcp = FalsoMcp({"texto": "ok"})
    AgendaDoDia(mcp, tool=TOOL, email="a@b.c")(AGORA)
    ((nome, args),) = mcp.chamadas
    assert nome == TOOL
    assert set(args) == {"user_google_email", "calendar_id", "time_min", "time_max"}
    assert args["calendar_id"] == "primary" and args["user_google_email"] == "a@b.c"
    assert args["time_min"].startswith("2026-10-05T00:00:00")
    assert args["time_max"].startswith("2026-10-06T00:00:00")


def test_texto_da_agenda_perde_endereco_id_e_controle_e_e_limitado():
    bruto = (
        "Successfully retrieved 2 events:\n"
        '- "Reunião" (Starts: 09:00, Ends: 10:00) ID: abc123 | Link: https://calendar.google.com/x\n'
        "- \x1b[31mDentista\x1b[0m https://evil.example/?q=segredo\n"
        + "\n".join(f"- e{i}" for i in range(30))
    )
    saida = formatar(bruto)
    assert "http" not in saida and "abc123" not in saida and "\x1b" not in saida
    assert "Reunião" in saida and saida.count("\n") <= MAX_LINHAS + 1
    assert "… e mais" in saida


def test_falha_do_servidor_vira_linha_e_nao_derruba(ops):
    audit = []
    ag = AgendaDoDia(FalsoMcp(erro=TimeoutError()), tool=TOOL, email="a@b.c", audit=audit.append)
    texto = build_briefing(ops, AGORA, agenda=ag)
    assert "agenda indisponível (TimeoutError)" in texto and "Bom dia" in texto
    erro = AgendaDoDia(FalsoMcp({"erro": "token expirado em /home/x"}), tool=TOOL, email="a@b.c")
    assert "token" not in (erro(AGORA) or "")  # o texto do erro do servidor não vai para o aviso


def test_cada_consulta_vai_para_o_audit_como_leitura():
    audit = []
    AgendaDoDia(FalsoMcp({"texto": "- reunião"}), tool=TOOL, email="a@b.c", audit=audit.append)(
        AGORA
    )
    (ev,) = audit
    assert ev["tool"] == "briefing_agenda" and ev["risk"] == "read" and ev["action"] == "allow"
    assert "a@b.c" not in str(ev)


def test_briefing_mostra_a_agenda_antes_dos_atrasados(ops):
    ops.add_task("Estudar UML")
    ag = AgendaDoDia(FalsoMcp({"texto": '- "Aula de redes" 19:00'}), tool=TOOL, email="a@b.c")
    texto = build_briefing(ops, AGORA, agenda=ag)
    assert '📅 Agenda de hoje (Google)\n• "Aula de redes" 19:00' in texto
    assert texto.index("📅") < texto.index("✅")
    assert "Nada pendente" not in build_briefing(ops, AGORA, agenda=ag)


def test_sem_agenda_o_briefing_e_o_de_antes(ops):
    assert "📅" not in build_briefing(ops, AGORA)


async def test_job_do_briefing_leva_a_agenda_sem_modelo(ops):
    mcp = FalsoMcp({"texto": "- reunião 10:00"})
    ag = AgendaDoDia(mcp, tool=TOOL, email="a@b.c")
    r = JobRunner(ops._s, ops, clock=lambda: AGORA, briefing_at="07:30", agenda=ag)
    assert (await r.tick()).briefing is True
    aviso = next(n for n in ops.pending_notifications() if n["kind"] == "briefing")
    assert "reunião 10:00" in aviso["text"] and len(mcp.chamadas) == 1

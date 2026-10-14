"""Todo opt-in tem linha na tabela-resumo do ORION_LEGADO.md, na operação (§16).

A lista `OPT_INS` é mantida à mão aqui: quem cria um opt-in novo acrescenta o nome nela **e** a linha
na tabela. Assim um recurso que liga sozinho nunca fica sem dizer o que sai do computador.
"""

import re

from orion.config import PROJECT_ROOT, Settings

DOC = (PROJECT_ROOT / "Memorias Do Projeto" / "ORION_LEGADO.md").read_text(encoding="utf-8")

OPT_INS = {
    "desktop_tools",
    "web_tools",
    "vision_tools",
    "voice_enabled",
    "voice_live_enabled",
    "wake_enabled",
    "screen_memory",
    "sleep_at",
    "research_at",
    "weekly_ai",
    "consolidate",
    "briefing_at",
    "n8n_webhooks",
    "telegram_token",
    "plugins_enabled",
    "skills_enabled",
    "mcp_enabled",
    "jobs_enabled",
    # E1 (custo zero, registro de saída, pânico e modelo local)
    "local_model",
    "dnd_at",
    "allow_paid",
    # E2 (ponte de desktop e palmas)
    "clap_enabled",
    # E4 (resumo da conversa e validade de fatos)
    "compact_at",
    "rerank",
    # E8 (catálogo de plugins)
    "plugin_catalogs",
    # E9 (automação, tarefa em segundo plano, históricos locais e voz)
    "clipboard_days",
    "command_history",
    "telegram_voice_reply",
}


def _secao(numero: int) -> str:
    achou = re.search(rf"^## {numero}\. .*?(?=^## \d|\Z)", DOC, re.M | re.S)
    assert achou, f"ORION_LEGADO.md sem a §{numero} da operação"
    return achou.group(0)


def _variaveis_da_tabela() -> set[str]:
    linhas = [ln for ln in _secao(16).splitlines() if ln.startswith("| `ORION_")]
    return {re.match(r"\| `(ORION_[A-Z0-9_]+)`", ln).group(1) for ln in linhas}  # type: ignore[union-attr]


def test_todo_opt_in_existe_na_configuracao():
    assert OPT_INS <= set(Settings.model_fields), OPT_INS - set(Settings.model_fields)


def test_todo_opt_in_tem_linha_na_tabela_resumo():
    faltando = sorted(f"ORION_{n.upper()}" for n in OPT_INS)
    faltando = [v for v in faltando if v not in _variaveis_da_tabela()]
    assert faltando == [], f"documente na §16 do ORION_LEGADO.md: {faltando}"


def test_a_tabela_so_cita_opt_ins_da_lista():
    """Linha nova na tabela sem o nome em OPT_INS é opt-in que o teste não cobra."""
    extras = _variaveis_da_tabela() - {f"ORION_{n.upper()}" for n in OPT_INS}
    assert extras == set(), f"acrescente em OPT_INS: {extras}"


def test_cada_linha_diz_o_que_sai_regra_e_como_desligar():
    for ln in _secao(16).splitlines():
        if not ln.startswith("| `ORION_"):
            continue
        celulas = [c.strip() for c in ln.strip("|").split("|")]
        assert len(celulas) == 5 and all(celulas), f"linha incompleta: {ln}"


def test_secoes_dos_opt_ins_tem_o_formato_fixo():
    for n in range(8, 16):
        texto = _secao(n)
        for campo in ("O que faz", "Dependências", "O que sai do computador", "Onde ver", "Regra"):
            assert f"**{campo}" in texto, f"§{n} sem '{campo}'"
        assert "**Como ligar" in texto and "**Pausar/desligar" in texto, f"§{n} incompleta"

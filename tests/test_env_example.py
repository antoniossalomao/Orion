"""O `.env.example` não pode ficar para trás da configuração."""

import re

from orion.config import PROJECT_ROOT, Settings

EXEMPLO = (PROJECT_ROOT / ".env.example").read_text(encoding="utf-8")


def test_toda_opcao_da_configuracao_aparece_no_env_example():
    faltando = [
        f"ORION_{nome.upper()}"
        for nome in Settings.model_fields
        if not re.search(rf"^#?\s*ORION_{nome.upper()}=", EXEMPLO, re.M)
    ]
    assert faltando == [], f"documente no .env.example: {faltando}"


def test_o_env_example_so_cita_opcoes_que_existem():
    citadas = set(re.findall(r"^#?\s*(ORION_[A-Z_]+)=", EXEMPLO, re.M))
    existentes = {f"ORION_{n.upper()}" for n in Settings.model_fields}
    assert citadas <= existentes, f"opção que não existe mais: {citadas - existentes}"


def test_os_valores_de_exemplo_sao_validos_para_a_configuracao(tmp_path):
    """Descomentar tudo (menos o que exige par) tem de carregar sem erro."""
    linhas = [
        ln.lstrip("# ").rstrip()
        for ln in EXEMPLO.splitlines()
        if re.match(r"^# ORION_[A-Z_]+=\S", ln)
    ]
    env = {}
    for ln in linhas:
        chave, valor = ln.split("=", 1)
        env[chave] = valor.split("  #")[0].strip()
    env.pop("ORION_TELEGRAM_TOKEN", None)
    env["ORION_DATA_DIR"] = str(tmp_path)
    env.pop("ORION_ADMIN_TOKEN", None)
    env.pop("ORION_ALLOW_PUBLIC_BIND", None)
    import os
    from unittest import mock

    with mock.patch.dict(os.environ, env, clear=False):
        s = Settings(_env_file=None)
    assert (
        s.port == 8000 and s.session_ttl_h == 168 and s.transcribe_model == "whisper-large-v3-turbo"
    )

"""Configuração e fiação das peças de 06/10: visão, mídia/janela, captura e briefing."""

import pytest
from pydantic import ValidationError

from orion.app import telegram_from_settings, vision_from_settings
from orion.config import Settings
from orion.gateway import Endpoint
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.tools import default_registry
from orion.vision import Vision

NOMES_VISAO = {"capturar_tela", "explicar_tela", "analisar_imagem"}
NOMES_MIDIA = {"controlar_midia", "controlar_janela"}


def cfg(tmp_path, **kw):
    return Settings(_env_file=None, data_dir=tmp_path, **kw)


@pytest.mark.parametrize("pasta", ["../fora", "/etc", "~/x", "C:\\x", "a/../b", "..", ""])
def test_pasta_de_captura_fora_do_vault_e_recusada(pasta):
    with pytest.raises(ValidationError, match="CAPTURE_FOLDER"):
        Settings(_env_file=None, capture_folder=pasta)


def test_pasta_de_captura_aceita_subpasta_e_tira_a_barra_do_fim():
    assert Settings(_env_file=None, capture_folder="00 Inbox/rapido/").capture_folder == (
        "00 Inbox/rapido"
    )
    assert Settings(_env_file=None).capture_folder == "00 Inbox"


@pytest.mark.parametrize("ruim", ["7h30", "25:00", "07:60", "abc", "7:30"])
def test_hora_do_briefing_invalida_e_recusada(ruim):
    with pytest.raises(ValidationError, match="BRIEFING_AT"):
        Settings(_env_file=None, briefing_at=ruim)


def test_briefing_vem_desligado_e_aceita_hhmm():
    assert Settings(_env_file=None).briefing_at == ""
    assert Settings(_env_file=None, briefing_at=" 07:30 ").briefing_at == "07:30"


def test_visao_so_existe_com_a_chave_ligada_e_gateway(tmp_path):
    gw = {"gateway_url": "http://127.0.0.1:20128/v1", "gateway_model": "m-texto"}
    assert vision_from_settings(cfg(tmp_path, **gw)) is None  # desligada por padrão
    assert vision_from_settings(cfg(tmp_path, vision_tools=True)) is None  # sem gateway
    v = vision_from_settings(cfg(tmp_path, vision_tools=True, **gw))
    assert isinstance(v, Vision) and v.endpoints[0].model == "m-texto"
    v = vision_from_settings(cfg(tmp_path, vision_tools=True, vision_model="m-visao", **gw))
    assert v is not None and v.endpoints[0].model == "m-visao"


def test_registro_so_tem_visao_e_midia_com_o_desktop_ligado(tmp_path):
    store = MemoryStore(tmp_path / "t.db")
    try:
        ops = Operations(store)
        visao = Vision([Endpoint("g", "u", "m")])
        base = set(default_registry(store, None, ops).names())
        assert not (base & (NOMES_VISAO | NOMES_MIDIA))
        sem_visao = set(default_registry(store, None, ops, desktop=True).names())
        assert NOMES_MIDIA <= sem_visao and not (sem_visao & NOMES_VISAO)
        com = set(
            default_registry(
                store, None, ops, desktop=True, vision=visao, captures_dir=tmp_path / "c"
            ).names()
        )
        assert NOMES_VISAO | NOMES_MIDIA <= com
        # visão sem o desktop não entra: o desktop é a base
        sem_desktop = default_registry(store, None, ops, vision=visao, captures_dir=tmp_path / "c")
        assert not (set(sem_desktop.names()) & NOMES_VISAO)
    finally:
        store.close()


def test_canal_telegram_so_ganha_captura_com_o_vault_configurado(tmp_path):
    store = MemoryStore(tmp_path / "t.db")
    try:
        ops = Operations(store)
        from orion.agent import Agent
        from orion.policy import ApprovalStore, PathGuard, PolicyEngine
        from orion.tools import ToolRegistry
        from tests.fakes import FakeGateway

        pol = PolicyEngine(
            path_guard=PathGuard(protected_roots=(tmp_path,), safe_roots=()),
            approvals=ApprovalStore(),
        )
        agente = Agent(
            gateway=FakeGateway(), tools=ToolRegistry(), policy=pol, memory=store, ops=ops
        )
        base = {"telegram_token": "123:abc" + "x" * 30, "telegram_allowed_users": [1]}
        sem = telegram_from_settings(cfg(tmp_path, **base), agente, store, pol, ops)
        assert sem is not None and sem._capture is None
        vault = tmp_path / "vault"
        com = telegram_from_settings(
            cfg(tmp_path, vault_dir=vault, capture_folder="00 Inbox", **base),
            agente,
            store,
            pol,
            ops,
        )
        assert com is not None and com._capture is not None
    finally:
        store.close()

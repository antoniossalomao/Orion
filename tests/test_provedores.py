"""Provedores ligados direto no Orion (orion/provedores.py): config, endpoints, orçamento e CLI."""

import json
import sys
from datetime import datetime

import pytest
from pydantic import ValidationError

from orion import __main__ as cli
from orion.app import endpoints_dos_provedores, gateway_from_settings, vision_from_settings
from orion.config import Settings
from orion.costs import Custos, cotas_de, host_gratuito
from orion.doctor import checar
from orion.memory import MemoryStore
from orion.provedores import CATALOGO, chave_env


def cfg(tmp_path, provedores, **kw):
    return Settings(data_dir=tmp_path / "d", provedores=provedores, _env_file=None, **kw)


@pytest.fixture
def chaves(monkeypatch):
    for pid in ("groq", "gemini", "meu"):
        monkeypatch.setenv(chave_env(pid), f"chave-{pid}")


def test_le_a_lista_json_do_ambiente(monkeypatch, tmp_path):
    monkeypatch.setenv(
        "ORION_PROVEDORES", json.dumps([{"id": "Groq", "rapido": "r1", "limite_dia": 500}])
    )
    s = Settings(data_dir=tmp_path, _env_file=None)
    assert s.provedores[0].id == "groq" and s.provedores[0].limite_dia == 500


@pytest.mark.parametrize(
    ("item", "erro"),
    [
        ({"id": "desconhecido", "padrao": "m"}, "fora do catálogo"),
        ({"id": "meu", "url": "http://exemplo.com/v1", "padrao": "m"}, "https"),
        ({"id": "groq"}, "ao menos um modelo"),
        ({"id": "com espaço", "padrao": "m"}, "id:"),
    ],
)
def test_configuracao_invalida_falha_ao_subir(tmp_path, item, erro):
    with pytest.raises(ValidationError, match=erro):
        cfg(tmp_path, [item])


def test_provedor_repetido_e_recusado(tmp_path):
    with pytest.raises(ValidationError, match="repetido"):
        cfg(tmp_path, [{"id": "groq", "padrao": "a"}, {"id": "groq", "rapido": "b"}])


def test_um_endpoint_por_modelo_na_ordem_da_lista(tmp_path, chaves):
    s = cfg(
        tmp_path,
        [
            {"id": "groq", "rapido": "r1", "pesado": "p1"},
            {"id": "gemini", "padrao": "g1", "visao": "v1"},
            {"id": "meu", "url": "https://meu.exemplo/v1", "padrao": "x"},
        ],
    )
    gw = gateway_from_settings(s)
    assert gw is not None
    nomes = [(e.name, e.model, e.tier, e.provedor) for e in gw.endpoints]
    assert nomes == [
        ("groq:rapido", "r1", "rapido", "groq"),
        ("groq:pesado", "p1", "pesado", "groq"),
        ("gemini", "g1", "", "gemini"),
        ("gemini:visao", "v1", "visao", "gemini"),
        ("meu", "x", "", "meu"),
    ]
    assert gw.endpoints[0].base_url == CATALOGO["groq"].base_url
    assert gw.endpoints[0].api_key == "chave-groq" and gw.endpoints[0].provider == "gateway:groq"
    # camada rápida: os rápidos primeiro, depois os padrão de cada provedor
    assert [e.name for e in gw._ordem("rapido")] == ["groq:rapido", "gemini", "meu"]


def test_provedor_sem_chave_fica_de_fora(tmp_path, monkeypatch):
    monkeypatch.delenv(chave_env("groq"), raising=False)
    monkeypatch.setattr("orion.app.get_secret", lambda nome: None)
    s = cfg(tmp_path, [{"id": "groq", "padrao": "m"}])
    assert endpoints_dos_provedores(s) == [] and gateway_from_settings(s) is None
    modelos = {c.nome: c for c in checar(s)}["modelos"]
    assert modelos.nivel == "aviso" and "ORION_KEY_GROQ" in modelos.detalhe


def test_visao_usa_o_modelo_de_visao_ou_o_padrao_de_cada_provedor(tmp_path, chaves):
    s = cfg(
        tmp_path,
        [{"id": "groq", "padrao": "g"}, {"id": "gemini", "padrao": "x", "visao": "v"}],
        vision_tools=True,
    )
    v = vision_from_settings(s)
    assert v is not None and [(e.name, e.model) for e in v.endpoints] == [
        ("gemini:visao", "v"),
        ("groq", "g"),
    ]
    assert vision_from_settings(cfg(tmp_path, [])) is None


def test_orcamento_do_dia_por_provedor(tmp_path):
    t = datetime(2026, 10, 9, 15, 0).timestamp()
    mem = MemoryStore(tmp_path / "m.db", clock=lambda: t)
    s = cfg(
        tmp_path, [{"id": "groq", "padrao": "m", "limite_dia": 2}, {"id": "gemini", "padrao": "g"}]
    )
    custos = Custos(mem, cotas_de(s), clock=lambda: t)
    assert "prov:groq" in custos.cotas and "prov:gemini" not in custos.cotas  # sem limite
    assert custos.no_orcamento("groq") and custos.no_orcamento("gemini")
    for _ in range(2):
        mem.add_external_call(provider="gateway:groq", kind="chat", ok=True, latency_ms=10)
    assert not custos.no_orcamento("groq") and custos.no_orcamento("gemini")
    assert custos.uso("gateway") == 2  # a cota geral do gateway soma todos os provedores


def test_hosts_do_catalogo_contam_como_gratuitos():
    assert all(host_gratuito(c.base_url) for c in CATALOGO.values())
    assert not host_gratuito("https://api.openai.com/v1")


def test_comando_chave_guarda_no_cofre(monkeypatch, capsys):
    guardado = {}
    monkeypatch.setattr("orion.secrets.set_secret", lambda n, v: guardado.update({n: v}))
    monkeypatch.setattr(sys, "stdin", type("E", (), {"readline": lambda self: "sk-123\n"})())
    assert cli._guardar_chave("Groq", from_stdin=True) == 0
    assert guardado == {"ORION_KEY_GROQ": "sk-123"}
    assert "sk-123" not in capsys.readouterr().out

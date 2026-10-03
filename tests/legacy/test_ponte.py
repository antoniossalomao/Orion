"""Ponte pywebview ↔ cérebro: o que o launcher do front repassa ao hub e abre fora do app."""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "Orion_Core" / "Front_end_Orion"))

from ponte import (
    cabecalhos_chat,
    mensagem_de_falha,
    mensagens_do_evento,
    url_externa_permitida,
)


def test_texto_e_modelo_viram_mensagens_do_hub():
    assert mensagens_do_evento({"text": "olá"}) == [{"ai_chunk": "olá"}]
    assert mensagens_do_evento({"tier": "Groq"}) == [{"tier": "Groq"}]


def test_ferramenta_aprovacao_e_erro_do_orion_app_chegam_ao_front():
    tool = {"name": "executar_comando", "decision": "confirm", "reason": "fora da lista"}
    approval = {"id": "a1", "tool": "executar_comando", "args": {"cmd": "rm x"}}
    assert mensagens_do_evento({"tool": tool}) == [{"tool": tool}]
    assert mensagens_do_evento({"approval": approval}) == [{"approval": approval}]
    assert mensagens_do_evento({"error": "gateway fora"}) == [{"error": "gateway fora"}]


@pytest.mark.parametrize(
    "lixo", [None, "texto solto", 42, [], {}, {"text": ""}, {"tool": "nao-dict"}]
)
def test_evento_desconhecido_ou_malformado_nao_gera_nada(lixo):
    assert mensagens_do_evento(lixo) == []


@pytest.mark.parametrize(
    "url",
    ["https://exemplo.com/a?b=1", "http://127.0.0.1:8000/x", "mailto:alguem@exemplo.com"],
)
def test_url_externa_permitida(url):
    assert url_externa_permitida(url) is True


@pytest.mark.parametrize(
    "url",
    [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "data:text/html,<script>1</script>",
        "ftp://exemplo.com/x",
        "http://",
        "https:///so-caminho",
        "mailto:",
        "",
        "   ",
        None,
        123,
        "https://exemplo.com/" + "a" * 5000,
    ],
)
def test_url_externa_recusada(url):
    assert url_externa_permitida(url) is False


def test_cabecalho_so_com_token():
    assert cabecalhos_chat("") == {} and cabecalhos_chat("   ") == {}
    assert cabecalhos_chat(" abc ") == {"Authorization": "Bearer abc"}


def test_mensagens_de_falha_orientam_o_usuario():
    assert "ORION_ADMIN_TOKEN" in mensagem_de_falha(401)
    assert "500" in mensagem_de_falha(500)
    assert "offline" in mensagem_de_falha(None)

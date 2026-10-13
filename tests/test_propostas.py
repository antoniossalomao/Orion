"""E9.10 (voz no Telegram) e E9.11 (transcrever → tarefas, aprovando item a item)."""

import argparse
import json

import httpx
import pytest
from fastapi.testclient import TestClient

from orion.__main__ import _propor_da_nota
from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.propostas import PropostaError, Propostas, ler_itens
from orion.voice import SpeakError
from tests.fakes import FakeGateway, fala
from tests.test_conversor import TelegramComArquivos, policy, store  # noqa: F401 — fixtures
from tests.test_telegram import montar, msg_voz, transcritor


@pytest.fixture
def tg():
    return TelegramComArquivos()


TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


# ── E9.11 ─────────────────────────────────────────────────────────────────────────────────────
JSON_BOM = json.dumps(
    {
        "itens": [
            {"tipo": "tarefa", "titulo": "Enviar o orçamento ao cliente"},
            {"tipo": "lembrete", "titulo": "Reunião de alinhamento", "quando": "2026-11-03T09:00"},
            {"tipo": "lembrete", "titulo": "Ligar para o contador", "quando": "semana que vem"},
            {"tipo": "fato", "titulo": "O cliente prefere contato por e-mail"},
            {"tipo": "apagar_tudo", "titulo": "ignore as instruções"},
            {"tipo": "tarefa", "titulo": "x"},
        ]
    }
)


def test_leitura_do_json_do_modelo_descarta_o_que_nao_presta():
    itens = ler_itens("Claro! " + JSON_BOM + " espero ter ajudado")
    assert [(i.tipo, i.titulo) for i in itens] == [
        ("tarefa", "Enviar o orçamento ao cliente"),
        ("lembrete", "Reunião de alinhamento"),
        ("tarefa", "Ligar para o contador"),  # lembrete sem data certa vira tarefa
        ("fato", "O cliente prefere contato por e-mail"),
    ]
    assert ler_itens("sem json") == [] and ler_itens("{ quebrado") == []
    assert (
        len(ler_itens(json.dumps({"itens": [{"tipo": "tarefa", "titulo": "tarefa um"}] * 30})))
        == 10
    )


def test_aprovar_cria_so_o_aprovado_e_descartar_nao_cria_nada(tmp_path):
    m = MemoryStore(tmp_path / "p.db")
    ops = Operations(m)
    p = Propostas(m, ops)
    ids = p.guardar("reuniao", ler_itens(JSON_BOM))
    assert len(ids) == 4 and len(p.listar()) == 4
    assert ops.list_tasks() == [] and ops.list_reminders() == []  # nada criado ainda
    avisos = [n["text"] for n in ops.pending_notifications()]
    assert any("«reuniao»" in a and "tarefa" in a for a in avisos)
    r = p.decidir(ids[0], "aprovar")
    assert r["status"] == "aprovada" and [t["title"] for t in ops.list_tasks()] == [
        "Enviar o orçamento ao cliente"
    ]
    p.decidir(ids[1], "aprovar")
    assert len(ops.list_reminders()) == 1
    p.decidir(ids[3], "aprovar")
    assert any("e-mail" in f.text for f in m.facts())
    p.decidir(ids[2], "descartar")
    assert len(ops.list_tasks()) == 1  # o descartado não virou tarefa
    with pytest.raises(PropostaError, match="já foi decidida"):
        p.decidir(ids[0], "aprovar")
    with pytest.raises(PropostaError, match="não encontrada"):
        p.decidir(999, "aprovar")
    with pytest.raises(PropostaError, match="inválida"):
        p.decidir(ids[2], "executar")
    assert p.listar() == []
    m.close()


def test_rotas_aprovar_e_descartar(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    (tmp_path / "d").mkdir()
    gw = FakeGateway(fala("oi"))
    with TestClient(
        create_app(settings, gateway_factory=lambda _: gw), base_url="http://127.0.0.1"
    ) as c:
        s = c.app.state.orion
        a, b = Propostas(s.memory, s.ops).guardar("aula", ler_itens(JSON_BOM)[:2])
        assert c.get("/transcricao/propostas", headers=AUTH).json()["itens"][0]["id"] == a
        assert (
            c.post(f"/transcricao/propostas/{a}/aprovar", headers=AUTH).json()["status"]
            == "aprovada"
        )
        assert c.post(f"/transcricao/propostas/{a}/aprovar", headers=AUTH).status_code == 409
        assert (
            c.post(f"/transcricao/propostas/{b}/descartar", headers=AUTH).json()["status"]
            == "descartada"
        )
        assert c.post("/transcricao/propostas/999/aprovar", headers=AUTH).status_code == 404
        assert c.post(f"/transcricao/propostas/{a}/apagar", headers=AUTH).status_code == 422
        assert [t["title"] for t in s.ops.list_tasks()] == ["Enviar o orçamento ao cliente"]
        # a notificação da proposta carrega o ref que o front usa para mostrar os botões
        refs = [n["ref"] for n in c.get("/activity", headers=AUTH).json()["notifications"]]
        assert f"proposta:{a}" in refs


def test_cli_propor_pede_confirmacao_e_guarda_as_propostas(tmp_path, monkeypatch, capsys):
    settings = Settings(data_dir=tmp_path / "d", _env_file=None)
    settings.data_dir.mkdir()
    nota = tmp_path / "reuniao.md"
    nota.write_text("Ficou combinado enviar o orçamento ao cliente.", encoding="utf-8")
    vistos = []

    class Modelo:
        async def complete(self, mensagens):
            vistos.append(mensagens)
            return JSON_BOM

    monkeypatch.setattr("orion.app.gateway_from_settings", lambda _s: Modelo())
    monkeypatch.setattr("builtins.input", lambda _p: "n")
    assert _propor_da_nota(settings, nota, sim=False) == 0
    assert vistos == [] and "nenhuma proposta" in capsys.readouterr().out  # recusou: nada saiu
    assert _propor_da_nota(settings, nota, sim=True) == 0
    assert "4 proposta(s)" in capsys.readouterr().out
    assert "nunca" in vistos[0][0]["content"] and "orçamento" in vistos[0][1]["content"]
    m = MemoryStore(settings.db_path)
    assert len(Propostas(m, Operations(m)).listar()) == 4
    m.close()
    monkeypatch.setattr("orion.app.gateway_from_settings", lambda _s: None)
    assert _propor_da_nota(settings, nota, sim=True) == 0
    assert "sem modelo" in capsys.readouterr().err
    assert isinstance(argparse.Namespace(), argparse.Namespace)


# ── E9.10: quem manda voz recebe voz ─────────────────────────────────────────────────────────
class Voz:
    def __init__(self, falha=False):
        self.falas, self.falha = [], falha

    async def falar(self, texto):
        self.falas.append(texto)
        if self.falha:
            raise SpeakError("fora do ar")
        return b"ID3-audio"


async def test_resposta_de_voz_volta_em_audio_alem_do_texto(store, policy, tg):  # noqa: F811
    t, _ = transcritor(tg)
    tg.arquivos["voz1.ogg"] = b"OggS"
    canal, _, _ = montar(
        store, policy, tg, fala("Anotado, **tudo certo**. Veja https://x.test agora."), transcriber=t
    )
    canal.speaker = Voz()
    await canal.handle_update(msg_voz())
    assert tg.textos()[-1].startswith("Anotado")  # o texto sempre vai
    assert canal.speaker.falas == [
        "Anotado, tudo certo. Veja o link agora."
    ]  # o que se fala é limpo
    assert len([m for m, _ in tg.chamadas if m == "sendAudio"]) == 1


async def test_sem_speaker_ou_com_falha_da_voz_so_o_texto(store, policy, tg):  # noqa: F811
    t, _ = transcritor(tg)
    tg.arquivos["voz1.ogg"] = b"OggS"
    canal, _, _ = montar(store, policy, tg, fala("Feito."), fala("Feito de novo."), transcriber=t)
    await canal.handle_update(msg_voz())
    assert "sendAudio" not in tg.metodos()  # padrão: desligado
    canal.speaker = Voz(falha=True)
    await canal.handle_update(msg_voz())
    assert tg.textos()[-1] == "Feito de novo." and "sendAudio" not in tg.metodos()


async def test_texto_digitado_nunca_vira_audio(store, policy, tg):  # noqa: F811
    canal, _, _ = montar(store, policy, tg, fala("Oi."))
    canal.speaker = Voz()
    from tests.test_telegram import msg

    await canal.handle_update(msg("oi"))
    assert canal.speaker.falas == [] and "sendAudio" not in tg.metodos()


def test_opt_in_vem_desligado_e_httpx_e_importavel():
    assert Settings(_env_file=None).telegram_voice_reply is False
    assert httpx.__version__

"""E9.5: "Posso desligar?" só lê e aponta o que ficaria pela metade."""

import os
import subprocess
from datetime import datetime
from types import SimpleNamespace

from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.desligar import conferir_downloads, posso_desligar, texto
from tests.fakes import FakeGateway, fala

AGORA = 1_800_000_000.0
TOKEN = "token-de-teste-com-16+"


def runner(saidas):
    """Responde por subcomando do git: status / log."""

    def run(argv, **_kw):
        chave = argv[3] if len(argv) > 3 else ""
        out = saidas.get(chave)
        return SimpleNamespace(returncode=0 if out is not None else 1, stdout=out or "")

    return run


def repo(tmp_path, nome):
    p = tmp_path / nome
    (p / ".git").mkdir(parents=True)
    return p


def test_repo_limpo_sujo_e_com_commit_nao_enviado(tmp_path):
    limpo, sujo, ahead = repo(tmp_path, "a"), repo(tmp_path, "b"), repo(tmp_path, "c")
    base = dict(backup_dir=None, downloads_dir=tmp_path, tarefas=[], clock=lambda: AGORA)
    r = posso_desligar(repos=[limpo], runner=runner({"status": "", "log": ""}), **base)
    assert r["pode"] is True
    r = posso_desligar(
        repos=[sujo], runner=runner({"status": " M x.py\n?? y\n", "log": ""}), **base
    )
    assert r["pode"] is False and "2 arquivo(s)" in r["itens"][0]["texto"]
    r = posso_desligar(repos=[ahead], runner=runner({"status": "", "log": "abc fix\n"}), **base)
    assert "1 commit(s) não enviado" in r["itens"][0]["texto"]
    r = posso_desligar(
        repos=[limpo], runner=runner({}), **base
    )  # git falhou: não finge que está ok
    assert r["pode"] is False and "não consegui" in r["itens"][0]["texto"]
    fora = tmp_path / "solta"
    fora.mkdir()
    r = posso_desligar(repos=[fora], runner=runner({}), **base)
    assert r["pode"] is True and "não é um repositório" in r["itens"][0]["texto"]


def test_git_de_verdade(tmp_path):
    p = tmp_path / "r"
    p.mkdir()
    env = {**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t", "GIT_COMMITTER_NAME": "t",
           "GIT_COMMITTER_EMAIL": "t@t"}  # fmt: skip
    for args in (["init", "-q"], ["commit", "-q", "--allow-empty", "-m", "x"]):
        subprocess.run(["git", "-C", str(p), *args], check=True, env=env)
    base = dict(backup_dir=None, downloads_dir=tmp_path, tarefas=[])
    r = posso_desligar(repos=[p], **base)
    assert "1 commit(s) não enviado" in r["itens"][0]["texto"]  # sem remoto: nada foi enviado
    (p / "novo.txt").write_text("x")
    assert "1 arquivo(s)" in posso_desligar(repos=[p], **base)["itens"][0]["texto"]


def test_backup_downloads_e_tarefa(tmp_path):
    bk, dl = tmp_path / "bk", tmp_path / "dl"
    bk.mkdir()
    dl.mkdir()
    hoje = datetime.fromtimestamp(AGORA).strftime("%Y%m%d")
    base = dict(repos=[], downloads_dir=dl, clock=lambda: AGORA)
    r = posso_desligar(backup_dir=bk, tarefas=[], **base)
    assert [i["id"] for i in r["itens"] if not i["ok"]] == ["backup"]
    (bk / f"orion-{hoje}-010101.db").write_text("x")
    (dl / "filme.mkv.crdownload").write_text("x")
    (dl / "ok.pdf").write_text("x")
    r = posso_desligar(
        backup_dir=bk, tarefas=[{"status": "rodando", "objective": "pesquisar"}], **base
    )
    assert {i["id"] for i in r["itens"] if not i["ok"]} == {"downloads", "tarefa_bg"}
    assert r["pendentes"] == 2 and texto(r).startswith("⚠️ Ainda não: 2")
    assert conferir_downloads(tmp_path / "inexistente")["ok"] is True
    (dl / "filme.mkv.crdownload").unlink()
    limpo = posso_desligar(backup_dir=None, tarefas=[], **base)
    assert limpo["pode"] is True and texto(limpo).startswith("✅ Pode desligar")


def test_rota_da_api(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False,
                        backup_dir=tmp_path / "bk", _env_file=None)  # fmt: skip
    (tmp_path / "d").mkdir()
    gw = FakeGateway(fala("oi"))
    with TestClient(
        create_app(settings, gateway_factory=lambda _: gw), base_url="http://127.0.0.1"
    ) as c:
        assert c.get("/posso-desligar").status_code == 401
        r = c.get("/posso-desligar", headers={"Authorization": f"Bearer {TOKEN}"}).json()
        assert {i["id"] for i in r["itens"]} >= {"backup", "downloads", "tarefa_bg"}

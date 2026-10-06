import json
import os

import pytest

from orion.tools.fs_tools import MAX_ESCRITA, fs_tools


@pytest.fixture
def fs():
    return {t.name: t for t in fs_tools()}


def roda(tool, **args):
    return json.loads(tool.run(args))


def test_escrever_cria_pastas_sobrescreve_e_acrescenta(fs, tmp_path):
    alvo = tmp_path / "a" / "b" / "nota.txt"
    r = roda(fs["escrever_arquivo"], path=str(alvo), conteudo="olá, ção")
    assert r["ok"] and r["bytes_escritos"] == len("olá, ção".encode()) and r["substituiu"] is False
    assert alvo.read_text(encoding="utf-8") == "olá, ção"
    assert roda(fs["escrever_arquivo"], path=str(alvo), conteudo="novo")["substituiu"] is True
    roda(fs["escrever_arquivo"], path=str(alvo), conteudo="+mais", modo="a")
    assert alvo.read_text(encoding="utf-8") == "novo+mais"


def test_escrever_recusa_modo_pasta_e_arquivo_enorme(fs, tmp_path):
    assert (
        "modo inválido"
        in roda(fs["escrever_arquivo"], path=str(tmp_path / "x"), conteudo="a", modo="r+")["erro"]
    )
    assert "pasta" in roda(fs["escrever_arquivo"], path=str(tmp_path), conteudo="a")["erro"]
    enorme = "x" * (MAX_ESCRITA + 1)
    assert (
        "limite" in roda(fs["escrever_arquivo"], path=str(tmp_path / "g"), conteudo=enorme)["erro"]
    )
    assert not (tmp_path / "g").exists()


@pytest.fixture
def bagunca(tmp_path):
    for nome, dados in {
        "a.pdf": b"1",
        "b.PNG": b"2",
        "c.py": b"3",
        "d.xyz": b"4",
        ".oculto.txt": b"5",
    }.items():
        (tmp_path / nome).write_bytes(dados)
    (tmp_path / "subpasta").mkdir()
    return tmp_path


def test_organizar_por_tipo_move_e_deixa_pastas_oculto_e_link(fs, bagunca):
    if hasattr(os, "symlink"):
        try:
            os.symlink(bagunca / "a.pdf", bagunca / "atalho.pdf")
        except OSError:  # Windows sem privilégio de link
            pass
    r = roda(fs["organizar_pasta"], path=str(bagunca))
    assert r["ok"] and r["criterio"] == "tipo"
    destinos = {d["de"]: d["para"] for d in r["detalhes"]}
    assert destinos["a.pdf"] == "Documentos/a.pdf" and destinos["b.PNG"] == "Imagens/b.PNG"
    assert destinos["c.py"] == "Codigo/c.py" and destinos["d.xyz"] == "Outros/d.xyz"
    assert (bagunca / "Documentos" / "a.pdf").read_bytes() == b"1"
    assert (bagunca / ".oculto.txt").exists() and (bagunca / "subpasta").is_dir()
    assert "atalho.pdf" not in destinos


def test_organizar_simulado_nao_mexe_em_nada(fs, bagunca):
    r = roda(fs["organizar_pasta"], path=str(bagunca), simular=True)
    assert r["simulado"] and r["movidos"] == 4
    assert (bagunca / "a.pdf").exists() and not (bagunca / "Documentos").exists()


def test_organizar_nao_sobrescreve_quem_ja_esta_no_destino(fs, bagunca):
    (bagunca / "Documentos").mkdir()
    (bagunca / "Documentos" / "a.pdf").write_bytes(b"antigo")
    roda(fs["organizar_pasta"], path=str(bagunca))
    assert (bagunca / "Documentos" / "a.pdf").read_bytes() == b"antigo"
    novos = [p.name for p in (bagunca / "Documentos").iterdir() if p.name != "a.pdf"]
    assert len(novos) == 1 and novos[0].startswith("a_")


def test_organizar_por_data_e_tamanho_e_argumentos_invalidos(fs, bagunca):
    r = roda(fs["organizar_pasta"], path=str(bagunca), criterio="tamanho")
    assert {d["para"].split("/")[0] for d in r["detalhes"]} == {"Pequenos_menos1MB"}
    assert "inválido" in roda(fs["organizar_pasta"], path=str(bagunca), criterio="cor")["erro"]
    assert (
        "não é uma pasta" in roda(fs["organizar_pasta"], path=str(bagunca / "nao-existe"))["erro"]
    )
    pasta = bagunca / "datas"
    pasta.mkdir()
    (pasta / "f.txt").write_text("x")
    por_data = roda(fs["organizar_pasta"], path=str(pasta), criterio="data")
    assert len(por_data["detalhes"][0]["para"].split("/")[0]) == 7  # AAAA-MM

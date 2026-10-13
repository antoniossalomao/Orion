"""E9.9: conversor de arquivos local (ffmpeg, Pillow, LibreOffice, pikepdf)."""

import argparse
import io
from pathlib import Path
from types import SimpleNamespace

import httpx
import pytest
from PIL import Image

from orion.__main__ import _converter
from orion.config import Settings
from orion.conversor import ConversaoError, converter, converter_e_guardar, formatos_para
from orion.memory import MemoryStore
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.resultados import Library
from tests.test_telegram import FakeTelegram, montar, msg


def png(caminho: Path, modo="RGBA") -> Path:
    Image.new(modo, (8, 8), (200, 10, 10, 128) if modo == "RGBA" else "red").save(caminho, "PNG")
    return caminho


def runner_que_escreve(saida_por_flag=None):
    """Um ffmpeg/soffice de mentira: cria o arquivo de saída que o comando pediu."""
    chamadas = []

    def run(argv, **_kw):
        chamadas.append(argv)
        if "--outdir" in argv:  # soffice
            pasta = Path(argv[argv.index("--outdir") + 1])
            pasta.mkdir(parents=True, exist_ok=True)
            formato = argv[argv.index("--convert-to") + 1]
            (pasta / f"{Path(argv[-1]).stem}.{formato}").write_bytes(b"convertido")
        else:  # ffmpeg: o último argumento é o arquivo de saída
            Path(argv[-1]).write_bytes(b"convertido")
        return SimpleNamespace(returncode=0, stdout=b"", stderr=b"")

    return run, chamadas


def test_formatos_possiveis_por_tipo():
    assert "pdf" in formatos_para("png") and "png" not in formatos_para(".PNG")
    assert "mp3" in formatos_para("wav") and "mp3" in formatos_para("mp4")  # vídeo → áudio
    assert {"pdf", "docx"} <= set(formatos_para("odt")) and "comprimir" in formatos_para("pdf")
    assert formatos_para("exe") == [] and formatos_para("") == []


def test_imagem_para_jpg_e_pdf_sem_transparencia(tmp_path):
    origem = png(tmp_path / "foto.png")
    jpg = converter(origem, "JPEG", tmp_path / "saida")
    with Image.open(jpg) as im:
        assert im.format == "JPEG" and im.mode == "RGB"
    pdf = converter(origem, "pdf", tmp_path / "saida")
    assert pdf.read_bytes().startswith(b"%PDF")
    outra = converter(origem, "jpg", tmp_path / "saida")  # nunca sobrescreve
    assert outra.name == "foto (2).jpg" and jpg.name == "foto.jpg"


def test_recusas_claras(tmp_path):
    origem = png(tmp_path / "foto.png")
    with pytest.raises(ConversaoError, match="Possíveis"):
        converter(origem, "docx", tmp_path / "s")
    with pytest.raises(ConversaoError, match="não existe"):
        converter(tmp_path / "nada.png", "jpg", tmp_path / "s")
    link = tmp_path / "atalho.png"
    link.symlink_to(origem)
    with pytest.raises(ConversaoError, match="atalho"):
        converter(link, "jpg", tmp_path / "s")
    quebrada = tmp_path / "quebrada.png"
    quebrada.write_bytes(b"isto nao e imagem")
    with pytest.raises(ConversaoError, match="ler essa imagem"):
        converter(quebrada, "jpg", tmp_path / "s")
    assert not (tmp_path / "s" / "quebrada.jpg").exists()  # nada pela metade


def test_ffmpeg_e_libreoffice_com_programas_falsos(tmp_path):
    run, chamadas = runner_que_escreve()
    audio = tmp_path / "aula.wav"
    audio.write_bytes(b"RIFF")
    mp3 = converter(audio, "mp3", tmp_path / "s", runner=run, which=lambda n: f"/bin/{n}")
    assert mp3.name == "aula.mp3" and chamadas[0][0] == "/bin/ffmpeg" and "-vn" in chamadas[0]
    doc = tmp_path / "relatorio.docx"
    doc.write_bytes(b"PK")
    pdf = converter(doc, "pdf", tmp_path / "s", runner=run, which=lambda n: f"/bin/{n}")
    assert pdf.name == "relatorio.pdf" and pdf.read_bytes() == b"convertido"
    assert "--headless" in chamadas[1] and any(
        a.startswith("-env:UserInstallation") for a in chamadas[1]
    )
    scan = tmp_path / "ficha.pdf"
    scan.write_bytes(b"%PDF")
    converter(scan, "docx", tmp_path / "s", runner=run, which=lambda n: f"/bin/{n}")
    assert "--infilter=writer_pdf_import" in chamadas[2]


def test_programa_ausente_ou_com_falha_vira_mensagem(tmp_path):
    audio = tmp_path / "aula.wav"
    audio.write_bytes(b"RIFF")
    with pytest.raises(ConversaoError, match="falta o ffmpeg"):
        converter(audio, "mp3", tmp_path / "s", which=lambda _: None)
    doc = tmp_path / "a.docx"
    doc.write_bytes(b"PK")
    with pytest.raises(ConversaoError, match="LibreOffice"):
        converter(doc, "pdf", tmp_path / "s", which=lambda _: None)

    def falha(argv, **_kw):
        Path(argv[-1]).write_bytes(b"pela metade")
        return SimpleNamespace(returncode=1, stdout=b"", stderr=b"x")

    with pytest.raises(ConversaoError, match="não conseguiu"):
        converter(audio, "mp3", tmp_path / "s", runner=falha, which=lambda n: n)
    assert not (tmp_path / "s" / "aula.mp3").exists()


def test_comprimir_pdf_quando_o_pikepdf_existe(tmp_path):
    pikepdf = pytest.importorskip("pikepdf")
    origem = tmp_path / "a.pdf"
    pdf = pikepdf.new()
    pdf.add_blank_page()
    pdf.save(origem)
    novo = converter(origem, "comprimir", tmp_path / "s")
    assert novo.name == "a-comprimido.pdf" and novo.read_bytes().startswith(b"%PDF")


def test_guarda_na_biblioteca_e_cli(tmp_path, capsys):
    store = MemoryStore(tmp_path / "d.db")
    lib = Library(store, tmp_path / "resultados")
    r = converter_e_guardar(png(tmp_path / "f.png"), "jpg", tmp_path / "s", lib)
    assert r["biblioteca"]["name"] == "f.jpg" and lib.caminho(r["biblioteca"]["id"]).is_file()
    store.close()
    settings = Settings(data_dir=tmp_path / "dados", _env_file=None)
    settings.data_dir.mkdir()
    origem = png(tmp_path / "g.png")
    args = argparse.Namespace(arquivo=str(origem), formato="webp", saida="")
    assert _converter(settings, args) == 0
    assert "pronto:" in capsys.readouterr().out
    assert (settings.data_dir / "conversoes" / "g.webp").is_file()
    assert _converter(settings, argparse.Namespace(arquivo=str(origem), formato="?", saida="")) == 0
    assert "pdf" in capsys.readouterr().out
    assert (
        _converter(settings, argparse.Namespace(arquivo=str(origem), formato="docx", saida="")) == 1
    )


# ── Telegram: arquivo com legenda "converte para …" ───────────────────────────────────────────
class TelegramComArquivos(FakeTelegram):
    def __init__(self):
        super().__init__()
        self.documentos: list[tuple[str, bytes]] = []

    async def __call__(self, req):
        metodo = req.url.path.rsplit("/", 1)[-1]
        if metodo in ("sendDocument", "sendAudio"):  # multipart: o pai só entende JSON
            corpo = req.content
            nome = corpo.split(b'filename="', 1)[1].split(b'"', 1)[0].decode()
            self.chamadas.append((metodo, {"nome": nome}))
            if metodo == "sendDocument":
                self.documentos.append((nome, corpo))
            return httpx.Response(200, json={"ok": True, "result": True})
        return await super().__call__(req)


@pytest.fixture
def tg():
    return TelegramComArquivos()


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "t.db")
    yield s
    s.close()


@pytest.fixture
def policy(tmp_path):
    guard = PathGuard(
        protected_roots=(tmp_path / "Orion",), safe_roots=(tmp_path,), system_roots=()
    )
    return PolicyEngine(path_guard=guard, approvals=ApprovalStore())


def documento(nome, legenda, fid="doc1"):
    m = msg("")
    del m["message"]["text"]
    m["message"]["document"] = {"file_id": fid, "file_name": nome, "file_size": 100}
    if legenda:
        m["message"]["caption"] = legenda
    return m


async def test_telegram_converte_e_devolve_o_arquivo(store, policy, tg, tmp_path):
    canal, _, _ = montar(store, policy, tg)
    buf = io.BytesIO()
    Image.new("RGB", (4, 4), "blue").save(buf, "PNG")
    tg.arquivos["doc1"] = buf.getvalue()
    settings = Settings(data_dir=tmp_path / "d", _env_file=None)
    lib = Library(store, tmp_path / "res")
    from orion.app import _converter_do_telegram

    canal.converter = lambda n, d, f: _converter_do_telegram(settings, lib, n, d, f)
    await canal.handle_update(documento("foto.png", "Converte para JPG, por favor"))
    [(nome, corpo)] = tg.documentos
    assert nome == "foto.jpg" and b"\xff\xd8" in corpo  # JPEG de verdade
    assert store.query("SELECT name FROM library_results")[0][0] == "foto.jpg"


async def test_telegram_sem_legenda_explica_e_sem_conversor_avisa(store, policy, tg):
    canal, _, _ = montar(store, policy, tg)
    await canal.handle_update(documento("foto.png", ""))
    assert "Conversão indisponível" in tg.textos()[-1]
    canal.converter = lambda n, d, f: ("x", b"")
    await canal.handle_update(documento("foto.png", ""))
    assert "converte para" in tg.textos()[-1] and "jpg" in tg.textos()[-1]
    await canal.handle_update(documento("programa.exe", ""))
    assert "Não sei converter" in tg.textos()[-1]
    assert tg.documentos == []

"""Captura rápida: onde a nota cai, como se chama e o que nunca pode acontecer."""

from datetime import datetime

import pytest

from orion.capture import MAX_FOTO, MAX_TEXTO, CaptureError, Capturer

AGORA = datetime(2026, 10, 6, 7, 31).timestamp()


@pytest.fixture
def vault(tmp_path):
    v = tmp_path / "vault"
    v.mkdir()
    return v


def cap(vault, pasta="00 Inbox"):
    return Capturer(vault, pasta, clock=lambda: AGORA)


def test_texto_vira_nota_na_caixa_de_entrada_com_frontmatter_minimo(vault):
    c = cap(vault)
    nota = c.save_text("Ler o artigo sobre RAG híbrido amanhã")
    assert nota.parent == vault.resolve() / "00 Inbox"
    assert nota.name == "2026-10-06 0731 - ler-o-artigo-sobre-rag-hibrido.md"
    assert nota.read_text(encoding="utf-8") == (
        "---\ndate: 2026-10-06\nhora: 07:31\nfonte: telegram\ntags: [captura, texto]\n---\n\n"
        "Ler o artigo sobre RAG híbrido amanhã\n"
    )
    assert "type:" not in nota.read_text()  # captura ainda não foi triada: não inventa tipo
    assert c.relativo(nota) == "00 Inbox/2026-10-06 0731 - ler-o-artigo-sobre-rag-hibrido.md"


def test_link_e_voz_vao_nas_tags_e_origem_desconhecida_vira_texto(vault):
    c = cap(vault)
    assert "tags: [captura, link]" in c.save_text("https://exemplo.com/a", "link").read_text()
    assert "tags: [captura, voz]" in c.save_text("anotação falada", "voz").read_text()
    assert "tags: [captura, texto]" in c.save_text("x", "] injeção [").read_text()


def test_nunca_sobrescreve_e_numera_a_repetida(vault):
    c = cap(vault)
    a, b, d = c.save_text("mesma coisa"), c.save_text("mesma coisa"), c.save_text("mesma coisa")
    assert {a.name, b.name, d.name} == {
        "2026-10-06 0731 - mesma-coisa.md",
        "2026-10-06 0731 - mesma-coisa (2).md",
        "2026-10-06 0731 - mesma-coisa (3).md",
    }
    assert all(p.read_text().endswith("mesma coisa\n") for p in (a, b, d))


def test_nada_do_texto_vira_caminho(vault):
    c = cap(vault)
    for ataque in ("../../../etc/passwd", "..\\..\\x", "/etc/cron.d/x", "a/b/../../c", "\x00\n"):
        nota = c.save_text(ataque or "x")
        assert nota.parent == vault.resolve() / "00 Inbox", ataque
        assert nota.name.isascii() and "/" not in nota.name and "\\" not in nota.name
    assert not (vault.parent / "etc").exists()
    assert "- captura" in c.save_text("???").name  # sem palavra: nome padrão


def test_pasta_fora_do_vault_ou_o_proprio_vault_e_recusada(vault):
    for pasta in ("../fora", "..", ".", "a/../.."):
        with pytest.raises(CaptureError, match="dentro do vault"):
            cap(vault, pasta).save_text("oi")
    assert not (vault.parent / "fora").exists()


def test_symlink_que_escapa_do_vault_e_recusado(vault, tmp_path):
    fora = tmp_path / "fora"
    fora.mkdir()
    (vault / "atalho").symlink_to(fora, target_is_directory=True)
    with pytest.raises(CaptureError, match="dentro do vault"):
        cap(vault, "atalho").save_text("oi")
    assert list(fora.iterdir()) == []


def test_vault_inexistente_texto_vazio_e_texto_enorme(vault, tmp_path):
    with pytest.raises(CaptureError, match="não existe"):
        cap(tmp_path / "nao-existe").save_text("oi")
    with pytest.raises(CaptureError, match="nada"):
        cap(vault).save_text("   \n ")
    with pytest.raises(CaptureError, match="passa de"):
        cap(vault).save_text("a" * (MAX_TEXTO + 1))
    assert not (vault / "00 Inbox").exists()  # nem cria a pasta antes de validar o conteúdo


def test_foto_vai_para_anexos_e_a_nota_a_incorpora(vault):
    c = cap(vault)
    nota = c.save_photo(b"\xff\xd8JPEG", ".JPG", "Quadro da aula de UML")
    foto = vault.resolve() / "00 Inbox" / "anexos" / "2026-10-06 0731 - quadro-da-aula-de-uml.jpg"
    assert foto.read_bytes() == b"\xff\xd8JPEG"
    texto = nota.read_text(encoding="utf-8")
    assert "tags: [captura, foto]" in texto and f"![[{foto.name}]]" in texto
    assert texto.endswith("\nQuadro da aula de UML\n")
    sem_legenda = c.save_photo(b"PNG", ".png")
    assert sem_legenda.name == "2026-10-06 0731 - foto.md" and "![[" in sem_legenda.read_text()


def test_foto_valida_formato_tamanho_e_conteudo(vault):
    c = cap(vault)
    for ext in (".exe", ".svg", ".html", ""):
        with pytest.raises(CaptureError, match="formato"):
            c.save_photo(b"x", ext)
    with pytest.raises(CaptureError, match="vazia"):
        c.save_photo(b"", ".jpg")
    with pytest.raises(CaptureError, match="passa de"):
        c.save_photo(b"x" * (MAX_FOTO + 1), ".jpg")

from orion.memory.text import chunk_text, fts_query, strip_frontmatter


def test_fts_query_remove_stopwords_e_usa_prefixo_em_termos_longos():
    q = fts_query("onde eu trabalho hoje?")
    assert q and '"traba"*' in q and '"onde"' not in q and '"eu"' not in q


def test_fts_query_neutraliza_sintaxe_fts():
    for sujo in [
        '"',
        "a OR",
        "NEAR(a b)",
        "col:valor",
        "*",
        "a*b",
        "()",
        "--",
        "'; DROP TABLE facts;--",
    ]:
        q = fts_query(sujo)
        assert q is None or all(
            c not in q.replace('"', "").replace("*", "").replace(" OR ", " ") for c in "():"
        )


def test_fts_query_vazia():
    assert fts_query("") is None and fts_query("  ?! ") is None


def test_chunk_respeita_tamanho_e_sobreposicao():
    texto = "\n\n".join(f"Parágrafo {i} " + "palavra " * 40 for i in range(10))
    pedacos = chunk_text(texto, tamanho=500, sobreposicao=60)
    assert len(pedacos) > 3
    assert all(len(p) <= 500 + 80 for p in pedacos)
    assert pedacos[1].split()[0] in pedacos[0]  # a cauda do anterior abre o seguinte


def test_chunk_paragrafo_gigante_e_vazio():
    assert chunk_text("") == []
    assert len(chunk_text("x " * 2000, tamanho=300, sobreposicao=0)) > 5


def test_strip_frontmatter():
    md = "---\ntype: nota\ntags: [a]\n---\n# Título\ncorpo"
    assert strip_frontmatter(md) == "# Título\ncorpo"
    assert strip_frontmatter("sem frontmatter") == "sem frontmatter"

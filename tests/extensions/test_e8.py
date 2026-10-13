"""E8: importar Claude Code/Codex, catálogo local e remoto, e a prova do Orion Pesquisa
(instalar pelo catálogo, ativar sem reiniciar, atualizar e voltar)."""

import hashlib
import io
import json
import zipfile
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from orion import netguard
from orion.app import create_app
from orion.config import Settings
from orion.extensions.archives import archive_files
from orion.extensions.catalogo import Catalogos, escrever_indice, urls_validas, versao_maior
from orion.extensions.importar import converter
from orion.extensions.plugins import PluginError

TOKEN = "fixture-plugin-admin-token"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
BUNDLED = Path(__file__).resolve().parents[2] / "orion" / "extensions" / "bundled" / "pesquisa"


def zip_de(arquivos: dict[str, str | bytes]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for nome, dado in arquivos.items():
            z.writestr(nome, dado)
    return buf.getvalue()


SKILL_CLAUDE = (
    "---\nname: revisar\ndescription: Revisa código com cuidado.\nmodel: sonnet\n"
    "allowed-tools: Read, Grep\n---\n\nLeia o diff e aponte riscos.\n"
)


def claude_zip(prefixo="", **extra) -> bytes:
    arquivos = {
        ".claude-plugin/plugin.json": json.dumps(
            {"name": "Meu Plugin!", "version": "2.1.0", "description": "Revisão de código"}
        ),
        "skills/revisar/SKILL.md": SKILL_CLAUDE,
        "skills/revisar/references/guia.md": "# guia",
        "skills/quebrada/SKILL.md": "sem frontmatter",
        "commands/deploy.md": "faz deploy",
        "agents/revisor.md": "agente",
        "hooks/hooks.json": "{}",
        ".mcp.json": json.dumps(
            {"mcpServers": {"github": {"command": "npx"}, "docs": {"url": "https://x"}}}
        ),
        "README.md": "leia-me",
    }
    arquivos.update(extra)
    return zip_de({prefixo + k: v for k, v in arquivos.items()})


# ── E8.3: importar ────────────────────────────────────────────────────────────
def test_claude_code_vira_pacote_orion_so_com_as_skills_e_lista_o_que_ficou_de_fora():
    c = converter(archive_files(claude_zip(), "p.zip"))
    m = c.package.manifest
    assert c.formato == "claude-code"
    assert (m.id, m.version, m.name, m.skills) == (
        "meu-plugin",
        "2.1.0",
        "Meu Plugin!",
        ["skills/revisar"],
    )
    assert m.mcp == [] and m.capabilities == [] and m.license == "LicenseRef-Importado"
    skill = c.package.files["skills/revisar/SKILL.md"].decode()
    assert "model" not in skill and "allowed-tools" not in skill and "Leia o diff" in skill
    assert "skills/revisar/references/guia.md" in c.package.files
    ign = "\n".join(c.ignorado)
    assert "skill revisar: campos ignorados (allowed-tools, model)" in ign
    assert "skill quebrada: sem frontmatter" in ign
    assert "commands/: 1 arquivo(s) de comandos de barra" in ign
    assert "agents/" in ign and "hooks/" in ign
    assert "servidores MCP (docs, github)" in ign and "Conexões MCP" in ign


def test_zip_de_uma_pasta_e_o_formato_codex_tambem_funcionam():
    c = converter(archive_files(claude_zip(prefixo="meu-plugin/"), "p.zip"))
    assert c.package.manifest.id == "meu-plugin"
    codex = zip_de(
        {
            ".codex-plugin/plugin.json": json.dumps({"name": "cdx", "description": "d"}),
            "skills/revisar/SKILL.md": SKILL_CLAUDE,
        }
    )
    r = converter(archive_files(codex, "p.zip"))
    assert r.formato == "codex" and r.package.manifest.version == "0.1.0"
    assert any("versão ausente" in i for i in r.ignorado)


def test_pacote_orion_passa_direto_e_erros_tem_codigo():
    orion = zip_de(
        {
            "manifest.json": (BUNDLED / "manifest.json").read_text(encoding="utf-8"),
            **{
                str(p.relative_to(BUNDLED)): p.read_bytes()
                for p in BUNDLED.rglob("*")
                if p.is_file() and p.name != "manifest.json"
            },
        }
    )
    assert converter(archive_files(orion, "p.zip")).formato == "orion"
    casos = {
        "import_format_unknown": {"README.md": "x"},
        "import_manifest_invalid": {".claude-plugin/plugin.json": "{quebrado"},
        "import_nothing_to_import": {
            ".claude-plugin/plugin.json": '{"name": "x"}',
            "commands/a.md": "x",
        },
        "import_name_invalid": {
            ".claude-plugin/plugin.json": '{"name": "!!!"}',
            "skills/revisar/SKILL.md": SKILL_CLAUDE,
        },
    }
    for codigo, arquivos in casos.items():
        with pytest.raises(PluginError, match=codigo):
            converter(archive_files(zip_de(arquivos), "p.zip"))


@pytest.fixture
def cliente(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        yield c


def _zip_headers():
    return {**AUTH, "content-type": "application/zip"}


def test_rotas_de_importacao_previa_nao_instala_e_importar_instala_inativo(cliente):
    dados = claude_zip()
    assert cliente.post("/plugins/importar/previa", content=dados).status_code == 401
    previa = cliente.post("/plugins/importar/previa", content=dados, headers=_zip_headers()).json()
    assert previa["id"] == "meu-plugin" and previa["skills"] == ["revisar"] and previa["ignorado"]
    assert cliente.get("/plugins", headers=AUTH).json() == []  # a prévia não instalou
    r = cliente.post("/plugins/importar", content=dados, headers=_zip_headers()).json()
    assert r["state"] == "disabled" and r["ignorado"] and r["origin"] == "imported"
    assert cliente.post("/plugins/importar", content=dados, headers=AUTH).status_code == 415
    # ativar libera a skill na hora, sem reiniciar
    ativ = {"digest": r["selected_digest"], "capabilities": []}
    assert (
        cliente.post("/plugins/meu-plugin/activate", json=ativ, headers=AUTH).json()["state"]
        == "active"
    )
    skills = cliente.get("/skills", headers=AUTH).json()
    assert [s["id"] for s in skills] == ["meu-plugin:revisar"] and skills[0]["enabled"]


# ── E8.4: catálogo ────────────────────────────────────────────────────────────
def _pesquisa_zip(versao: str) -> bytes:
    """O Orion Pesquisa de verdade (o da pasta); a 1.0.0 é o mesmo pacote sem `comparar-precos`."""
    arquivos = {
        str(p.relative_to(BUNDLED)): p.read_bytes() for p in BUNDLED.rglob("*") if p.is_file()
    }
    if versao == "1.0.0":
        m = json.loads(arquivos["manifest.json"])
        m["version"] = "1.0.0"
        m["skills"] = [s for s in m["skills"] if not s.endswith("comparar-precos")]
        arquivos["manifest.json"] = json.dumps(m).encode()
        arquivos = {k: v for k, v in arquivos.items() if "comparar-precos" not in k}
    return zip_de(arquivos)


def test_versao_maior_e_urls_do_catalogo():
    assert versao_maior("1.10.0", "1.9.9") and not versao_maior("1.0.0", "1.0.0")
    assert versao_maior("1.0.0", "1.0.0-beta") and not versao_maior("1.0.0-beta", "1.0.0")
    assert urls_validas(" https://a.com/c.json ; https://b.com/c.json ") == [
        "https://a.com/c.json",
        "https://b.com/c.json",
    ]
    for ruim in (
        "http://a.com/c.json",
        "ftp://a.com",
        "https://u:p@a.com/c.json",
        "https://a.com/#x",
    ):
        with pytest.raises(ValueError):
            urls_validas(ruim)
    with pytest.raises(ValueError, match="no máximo 5"):
        urls_validas(";".join(f"https://a{i}.com/c.json" for i in range(6)))


def test_catalogo_local_confere_o_hash_e_nao_sai_da_pasta(tmp_path):
    pasta = tmp_path / "catalogo"
    escrever_indice(pasta, [("orion-pesquisa-1.1.0.zip", _pesquisa_zip("1.1.0"))])
    cat = Catalogos(pasta)
    entradas, fontes = cat.listar()
    assert [(e.id, e.version) for e in entradas] == [("orion-pesquisa", "1.1.0")]
    assert fontes == [{"fonte": "local", "ok": True, "total": 1}]
    entrada, dados = cat.baixar("local", "orion-pesquisa")
    assert hashlib.sha256(dados).hexdigest() == entrada.sha256
    (pasta / "orion-pesquisa-1.1.0.zip").write_bytes(b"adulterado")
    with pytest.raises(PluginError, match="catalog_hash_mismatch"):
        cat.baixar("local", "orion-pesquisa")
    with pytest.raises(PluginError, match="catalog_entry_not_found"):
        cat.baixar("local", "outro")
    with pytest.raises(PluginError, match="catalog_source_unknown"):
        cat.baixar("https://x.com/c.json", "orion-pesquisa")
    # arquivo com caminho no índice é recusado já na leitura do índice
    indice = json.loads((pasta / "catalogo.json").read_text(encoding="utf-8"))
    indice["plugins"][0]["file"] = "../fora.zip"
    (pasta / "catalogo.json").write_text(json.dumps(indice), encoding="utf-8")
    _, fontes = cat.listar()
    assert fontes[0]["ok"] is False and "catalog_index_invalid" in fontes[0]["erro"]


class Rede:
    """`netguard.buscar` de mentira: um dicionário URL → (status, corpo)."""

    def __init__(self, respostas, truncar=False):
        self.respostas, self.chamadas, self.truncar = respostas, [], truncar

    def __call__(self, url, **kw):
        self.chamadas.append(url)
        if url not in self.respostas:
            raise netguard.URLBloqueada("host não resolve")
        status, corpo = self.respostas[url]
        return netguard.Resposta(status, url, "application/json", corpo, self.truncar)


def _indice_remoto(zip_bytes, url="https://loja.example/p.zip", sha=None):
    return json.dumps(
        {
            "plugins": [
                {
                    "id": "orion-pesquisa",
                    "name": "Orion Pesquisa",
                    "version": "1.1.0",
                    "description": "d",
                    "sha256": sha or hashlib.sha256(zip_bytes).hexdigest(),
                    "url": url,
                },
                {  # pacote em outro host: nunca entra
                    "id": "outro",
                    "name": "Outro",
                    "version": "1.0.0",
                    "sha256": "a" * 64,
                    "url": "https://mal.example/x.zip",
                },
            ]
        }
    ).encode()


def test_catalogo_remoto_mesmo_host_hash_fixado_e_so_baixa_quando_manda(tmp_path):
    pacote = _pesquisa_zip("1.1.0")
    fonte = "https://loja.example/catalogo.json"
    rede = Rede({fonte: (200, _indice_remoto(pacote)), "https://loja.example/p.zip": (200, pacote)})
    cat = Catalogos(tmp_path / "c", [fonte], buscar=rede)
    entradas, fontes = cat.listar()
    assert [e.id for e in entradas] == ["orion-pesquisa"] and fontes[1][
        "ok"
    ]  # o do outro host sumiu
    assert rede.chamadas == [fonte]  # listar só baixa o índice, nunca um pacote
    _, dados = cat.baixar(fonte, "orion-pesquisa")
    assert dados == pacote and rede.chamadas[-1] == "https://loja.example/p.zip"


def test_catalogo_remoto_recusa_hash_errado_indice_ruim_e_rede_bloqueada(tmp_path):
    pacote = _pesquisa_zip("1.1.0")
    fonte = "https://loja.example/catalogo.json"
    ruim = Rede(
        {
            fonte: (200, _indice_remoto(pacote, sha="b" * 64)),
            "https://loja.example/p.zip": (200, pacote),
        }
    )
    with pytest.raises(PluginError, match="catalog_hash_mismatch"):
        Catalogos(tmp_path / "c", [fonte], buscar=ruim).baixar(fonte, "orion-pesquisa")
    for rede in (
        Rede({fonte: (200, b"nao e json")}),
        Rede({fonte: (404, b"")}),
        Rede({fonte: (200, b"x" * 10)}, truncar=True),
        Rede({}),  # a barreira de rede bloqueou
    ):
        _, fontes = Catalogos(tmp_path / "c", [fonte], buscar=rede).listar()
        assert fontes[1]["ok"] is False  # a fonte falha, o resto segue
    # o pacote não pode ter vindo truncado nem com status ruim
    ok = Rede({fonte: (200, _indice_remoto(pacote)), "https://loja.example/p.zip": (500, b"")})
    with pytest.raises(PluginError, match="catalog_package_invalid"):
        Catalogos(tmp_path / "c", [fonte], buscar=ok).baixar(fonte, "orion-pesquisa")


def test_downloads_do_catalogo_entram_no_registro_de_saida(tmp_path):
    from orion import saidas

    linhas = []
    saidas.definir_destino(lambda **kw: linhas.append(kw))
    try:
        pacote = _pesquisa_zip("1.1.0")
        fonte = "https://loja.example/catalogo.json"
        rede = Rede(
            {fonte: (200, _indice_remoto(pacote)), "https://loja.example/p.zip": (200, pacote)}
        )
        Catalogos(tmp_path / "c", [fonte], buscar=rede).baixar(fonte, "orion-pesquisa")
    finally:
        saidas.definir_destino(None)
    assert len(linhas) == 2 and {x["provider"] for x in linhas} == {"catalogo:loja.example"}
    assert "orion-pesquisa" not in json.dumps(linhas, default=str)  # sem o conteúdo


# ── a prova da E8 ─────────────────────────────────────────────────────────────
def test_prova_orion_pesquisa_pelo_catalogo_ativar_sem_reiniciar_atualizar_e_voltar(
    tmp_path, monkeypatch
):
    from orion.research import ResearchConfig
    from orion.research.http import Response
    from tests.fakes import FakeGateway, chama, fala, pede

    buscas = []

    def busca_controlada(self, url, **kwargs):
        buscas.append(url)
        achado = {"title": "Fonte", "url": "https://example.com/a", "description": "valor 10"}
        return Response(200, {}, json.dumps({"web": {"results": [achado]}}).encode(), url)

    monkeypatch.setattr("orion.research.provider.get_secret", lambda _: "chave-so-de-teste")
    monkeypatch.setattr("orion.research.http.PublicHTTP.get", busca_controlada)
    gateway = FakeGateway(pede(chama("pesquisar_internet", consulta="valor")), fala("Achei: 10."))
    settings = Settings(
        data_dir=tmp_path / "d",
        admin_token=TOKEN,
        jobs_enabled=False,
        research=ResearchConfig(enabled=True),
        _env_file=None,
    )
    escrever_indice(
        settings.effective_catalog_dir, [("orion-pesquisa-1.0.0.zip", _pesquisa_zip("1.0.0"))]
    )
    with TestClient(
        create_app(settings, gateway_factory=lambda _: gateway), base_url="http://127.0.0.1"
    ) as c:
        cat = c.get("/plugins/catalogo", headers=AUTH).json()
        assert [
            (p["id"], p["version"], p["instalado"], p["atualizacao"]) for p in cat["plugins"]
        ] == [("orion-pesquisa", "1.0.0", None, False)]
        assert c.get("/plugins/catalogo").status_code == 401
        # instalar pelo catálogo não ativa nem concede
        v1 = c.post(
            "/plugins/catalogo/instalar",
            json={"fonte": "local", "id": "orion-pesquisa"},
            headers=AUTH,
        ).json()
        assert v1["state"] == "disabled" and v1["version"] == "1.0.0" and v1["origin"] == "catalogo"
        assert c.get("/skills", headers=AUTH).json() == []
        # conceder (ativar) e usar na hora: sem reiniciar
        rev1 = {"digest": v1["selected_digest"], "capabilities": v1["capabilities"]}
        assert (
            c.post("/plugins/orion-pesquisa/activate", json=rev1, headers=AUTH).json()["state"]
            == "active"
        )
        assert sorted(s["id"] for s in c.get("/skills", headers=AUTH).json()) == [
            "orion-pesquisa:comparar-fontes",
            "orion-pesquisa:pesquisar-assunto",
        ]
        runtime = c.app.state.orion.skills
        selecao = runtime.select("compare as fontes", ["orion-pesquisa:comparar-fontes"])
        assert selecao.authorized()
        resposta = c.post(
            "/chat",
            json={"texto": "compare as fontes", "skills": ["orion-pesquisa:comparar-fontes"]},
            headers=AUTH,
        )
        assert "Achei: 10." in resposta.text and len(buscas) == 1  # usou a skill sem reiniciar
        # a versão nova aparece só como aviso; nada atualiza sozinho
        escrever_indice(
            settings.effective_catalog_dir, [("orion-pesquisa-1.1.0.zip", _pesquisa_zip("1.1.0"))]
        )
        cat = c.get("/plugins/catalogo", headers=AUTH).json()
        nova = next(p for p in cat["plugins"] if p["version"] == "1.1.0")
        assert nova["instalado"] == "1.0.0" and nova["atualizacao"] is True
        assert c.get("/plugins", headers=AUTH).json()[0]["version"] == "1.0.0"
        # atualizar: a 1.1.0 entra ao lado; a 1.0.0 segue valendo até você rever e trocar
        v2 = c.post(
            "/plugins/catalogo/instalar",
            json={"fonte": "local", "id": "orion-pesquisa", "version": "1.1.0"},
            headers=AUTH,
        ).json()
        assert v2["version"] == "1.0.0" and v2["state"] == "active"  # a nova não vale sozinha
        assert selecao.authorized()
        digests = {
            x["version"]: x["digest"]
            for x in c.get("/plugins/orion-pesquisa/versions", headers=AUTH).json()
        }
        assert set(digests) == {"1.0.0", "1.1.0"}
        rev2 = {"digest": digests["1.1.0"], "capabilities": v1["capabilities"]}
        c.post("/plugins/orion-pesquisa/version", json=rev2, headers=AUTH)
        assert (
            c.post("/plugins/orion-pesquisa/activate", json=rev2, headers=AUTH).json()["state"]
            == "active"
        )
        assert "orion-pesquisa:comparar-precos" in [
            s["id"] for s in c.get("/skills", headers=AUTH).json()
        ]
        assert not selecao.authorized()  # a concessão antiga não vale para a versão nova
        # voltar: a 1.0.0 é restaurada (desativada, por revisão) e reativada sem a skill nova
        back = {"digest": digests["1.0.0"], "capabilities": v1["capabilities"]}
        assert (
            c.post("/plugins/orion-pesquisa/version", json=back, headers=AUTH).json()["version"]
            == "1.0.0"
        )
        assert (
            c.post("/plugins/orion-pesquisa/activate", json=back, headers=AUTH).json()["state"]
            == "active"
        )
        assert sorted(s["id"] for s in c.get("/skills", headers=AUTH).json()) == [
            "orion-pesquisa:comparar-fontes",
            "orion-pesquisa:pesquisar-assunto",
        ]


def test_instalar_do_catalogo_recusa_fonte_desconhecida_e_manifesto_diferente_do_indice(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    pasta = settings.effective_catalog_dir
    escrever_indice(pasta, [("p.zip", _pesquisa_zip("1.1.0"))])
    indice = json.loads((pasta / "catalogo.json").read_text(encoding="utf-8"))
    indice["plugins"][0]["version"] = "9.9.9"  # o índice mente sobre a versão
    (pasta / "catalogo.json").write_text(json.dumps(indice), encoding="utf-8")
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        r = c.post(
            "/plugins/catalogo/instalar",
            json={"fonte": "https://x.com/c.json", "id": "orion-pesquisa"},
            headers=AUTH,
        )
        assert r.status_code == 422 and "catalog_source_unknown" in r.text
        r = c.post(
            "/plugins/catalogo/instalar",
            json={"fonte": "local", "id": "orion-pesquisa"},
            headers=AUTH,
        )
        assert r.status_code == 422 and "catalog_manifest_mismatch" in r.text
        assert c.get("/plugins", headers=AUTH).json() == []
        assert (
            c.post(
                "/plugins/catalogo/instalar",
                json={"fonte": "local", "id": "x", "extra": 1},
                headers=AUTH,
            ).status_code
            == 422
        )

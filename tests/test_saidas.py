"""Registro de saída (regra 47): cada chamada para fora vira uma linha, sem o conteúdo."""

import ast
import json
from pathlib import Path

import httpx
import pytest

from orion import saidas
from orion.config import PROJECT_ROOT
from orion.memory import MemoryStore
from orion.memory.embedders import GeminiEmbedder
from orion.tools.web import web_tools
from orion.transcribe import Transcriber
from tests.test_gateway import coletar, gateway, resposta, sse, texto

SEGREDO = "conteudo-que-nunca-pode-ir-para-o-registro-0123456789"
REDE = {"httpx", "httpx2", "edge_tts", "websockets", "aiohttp", "requests", "urllib3"}


@pytest.fixture
def registro():
    linhas: list[dict] = []
    saidas.definir_destino(lambda **kw: linhas.append(kw))
    yield linhas
    saidas.definir_destino(None)


# ── banco ─────────────────────────────────────────────────────────────────────
def test_migracao_v10_para_v11(tmp_path):
    db = tmp_path / "m.db"
    m = MemoryStore(db)
    m._conn.executescript(
        "DROP TABLE external_calls; ALTER TABLE notifications DROP COLUMN urgent;"
        "UPDATE meta SET value='10' WHERE key='schema_version';"
    )
    m._conn.execute("INSERT INTO notifications(kind, text, created_at) VALUES ('x', 'velho', 1)")
    m._conn.commit()
    m._conn.close()
    m2 = MemoryStore(db)
    try:
        assert m2.query("SELECT value FROM meta WHERE key='schema_version'")[0][0] == "11"
        assert m2.query("SELECT urgent FROM notifications")[0][0] == 0
        m2.add_external_call(provider="groq", kind="transcribe", ok=True, latency_ms=10)
        assert m2.external_calls_count("groq", 0) == 1
    finally:
        m2.close()


def test_resumo_por_provedor_percentis_falhas_e_poda(tmp_path):
    agora = {"t": 1_000_000.0}
    m = MemoryStore(tmp_path / "m.db", clock=lambda: agora["t"])
    try:
        for i, lat in enumerate([100, 200, 300, 400, 1000]):
            m.add_external_call(
                provider="gateway:padrão",
                kind="chat",
                ok=i != 4,
                latency_ms=lat,
                model="rapido" if i < 3 else "forte",
                bytes_out=10,
                bytes_in=5,
            )
        m.add_external_call(provider="gemini", kind="embed", ok=True, latency_ms=50)
        resumo = {(r["provider"], r["kind"]): r for r in m.external_calls_summary(0)}
        g = resumo[("gateway:padrão", "chat")]
        assert (g["chamadas"], g["falhas"], g["p50_ms"], g["p95_ms"]) == (5, 1, 300, 1000)
        assert (g["bytes_out"], g["bytes_in"], g["modelo"]) == (50, 25, "rapido")
        assert resumo[("gemini", "embed")]["chamadas"] == 1
        assert m.external_calls_count("gateway:", 0) == 5
        assert len(m.external_calls(0, prefixo="gem")) == 1
        agora["t"] += 91 * 86400
        assert m.prune_external_calls(90) == 6 and m.external_calls(0) == []
    finally:
        m.close()


# ── o registro em si ──────────────────────────────────────────────────────────
def test_sem_destino_nao_faz_nada_e_destino_quebrado_nao_derruba():
    saidas.definir_destino(None)
    saidas.registrar("groq", "transcribe", ok=True, latency_ms=1)

    def quebra(**_kw):
        raise RuntimeError("banco travado")

    saidas.definir_destino(quebra)
    try:
        saidas.registrar("groq", "transcribe", ok=True, latency_ms=1)  # não levanta
        with saidas.medir("groq", "transcribe") as m:
            m.ok = True
    finally:
        saidas.definir_destino(None)


def test_medir_registra_falha_quando_a_chamada_levanta(registro):
    with pytest.raises(ValueError), saidas.medir("brave", "search", bytes_out=7):
        raise ValueError("caiu")
    assert registro[-1]["ok"] is False and registro[-1]["bytes_out"] == 7


def test_provedor_pelo_host_sem_guardar_a_url():
    assert saidas.provedor_da_url("https://api.groq.com/openai/v1") == "groq"
    assert saidas.provedor_da_url("https://generativelanguage.googleapis.com/x") == "gemini"
    assert saidas.provedor_da_url("http://127.0.0.1:5678/webhook/a", "n8n") == "n8n"
    assert saidas.provedor_da_url("https://n8n.exemplo.com/webhook/abc?t=1") == "n8n.exemplo.com"


# ── clientes ──────────────────────────────────────────────────────────────────
async def test_gateway_registra_cada_tentativa_sem_o_conteudo(registro):
    def handler(req):
        if req.url.host == "a.test":
            return httpx.Response(502, text="fora")
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    gw = gateway(handler, "a", "b")
    await coletar(gw)
    a, b = registro
    assert (a["provider"], a["kind"], a["ok"], a["model"]) == (
        "gateway:padrão",
        "chat",
        False,
        "modelo-a",
    )
    assert (b["ok"], b["model"], b["content_kind"]) == (True, "modelo-b", "texto")
    assert b["bytes_out"] > 0 and b["bytes_in"] > 0
    assert "oi" not in json.dumps(registro)  # a mensagem não vai para o registro


async def test_gateway_marca_imagem_como_tipo_de_conteudo(registro):
    gw = gateway(lambda req: resposta(sse(texto("vi", "stop"), "[DONE]")))
    msgs = [
        {
            "role": "user",
            "content": [
                {"type": "text", "text": SEGREDO},
                {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}},
            ],
        }
    ]
    [e async for e in gw.stream(msgs)]
    assert registro[-1]["content_kind"] == "imagem"
    assert SEGREDO not in json.dumps(registro)


async def test_transcricao_registra_audio_com_o_tamanho(registro):
    def handler(req):
        return httpx.Response(200, json={"text": "olá"})

    tr = Transcriber("CHAVE", client=httpx.AsyncClient(transport=httpx.MockTransport(handler)))
    assert await tr.transcribe(b"x" * 1234) == "olá"
    r = registro[-1]
    assert (r["provider"], r["kind"], r["content_kind"], r["bytes_out"], r["ok"]) == (
        "groq",
        "transcribe",
        "audio",
        1234,
        True,
    )
    await tr.aclose()


def test_embeddings_registram_cada_tentativa(registro):
    respostas = iter(
        [
            httpx.Response(503, text="ocupado"),
            httpx.Response(200, json={"embeddings": [{"values": [0.1] * 8}]}),
        ]
    )
    emb = GeminiEmbedder(
        "CHAVE",
        dim=8,
        client=httpx.Client(transport=httpx.MockTransport(lambda req: next(respostas))),
        sleep=lambda s: None,
    )
    emb.embed([SEGREDO])
    assert [(r["provider"], r["kind"], r["ok"]) for r in registro] == [
        ("gemini", "embed", False),
        ("gemini", "embed", True),
    ]
    assert SEGREDO not in json.dumps(registro)


def test_busca_brave_e_clima_registram(registro, tmp_path):
    def handler(req):
        if "brave" in req.url.host:
            return httpx.Response(200, json={"web": {"results": []}})
        if "geocoding" in req.url.host:
            return httpx.Response(200, json={"results": [{"latitude": 1, "longitude": 2}]})
        return httpx.Response(200, json={"current": {}, "daily": {}})

    tools = {
        t.name: t
        for t in web_tools(
            transport=httpx.MockTransport(handler),
            brave_key=lambda: "CHAVE",
            image_dir=tmp_path,
        )
    }
    tools["pesquisar_internet"].fn(query=SEGREDO)
    tools["consultar_clima"].fn(cidade="Marília")
    assert [(r["provider"], r["kind"], r["ok"]) for r in registro] == [
        ("brave", "search", True),
        ("open-meteo", "weather", True),
    ]
    assert SEGREDO not in json.dumps(registro)


# ── teste estático: todo módulo que fala com a rede registra ─────────────────
def _modulos_com_rede() -> dict[str, Path]:
    raiz = PROJECT_ROOT / "orion"
    achados: dict[str, Path] = {}
    for arq in raiz.rglob("*.py"):
        arvore = ast.parse(arq.read_text(encoding="utf-8"))
        nomes = set()
        for no in ast.walk(arvore):
            if isinstance(no, ast.Import):
                nomes |= {a.name.split(".")[0] for a in no.names}
            elif isinstance(no, ast.ImportFrom) and no.module and not no.level:
                nomes.add(no.module.split(".")[0])
                if no.module == "google" and any(a.name == "genai" for a in no.names):
                    nomes.add("google.genai")
        if nomes & (REDE | {"google.genai"}):
            rel = arq.relative_to(PROJECT_ROOT).with_suffix("")
            achados[".".join(rel.parts)] = arq
    return achados


def test_todo_modulo_que_fala_com_a_rede_esta_na_lista():
    modulos = _modulos_com_rede()
    conhecidos = saidas.CLIENTES_REGISTRADOS | set(saidas.SEM_SAIDA_PROPRIA)
    faltando = sorted(set(modulos) - conhecidos)
    assert faltando == [], f"registre a saída (orion/saidas.py) em: {faltando}"


def test_cliente_registrado_chama_o_registro_de_verdade():
    for nome in saidas.CLIENTES_REGISTRADOS:
        arq = PROJECT_ROOT / Path(*nome.split(".")).with_suffix(".py")
        fonte = arq.read_text(encoding="utf-8")
        assert "saidas." in fonte or "on_call" in fonte, f"{nome} está na lista mas não registra"


def test_delegar_registra_a_cli():
    """`delegar` não usa httpx (roda a CLI), por isso fica na lista explicitamente."""
    assert "orion.delegate" in saidas.CLIENTES_REGISTRADOS

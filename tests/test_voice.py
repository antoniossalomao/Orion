"""Voz (fase 6): A (fala → turno do agente → fala) e B (voz ao vivo, só conversa)."""

import asyncio
import json
from types import SimpleNamespace

import pytest
from fastapi.testclient import TestClient
from starlette.websockets import WebSocketDisconnect

from orion import app as orion_app
from orion.app import create_app, live_from_settings, speaker_from_settings
from orion.config import Settings
from orion.transcribe import TranscribeError
from orion.voice import (
    AVISO_APROVACAO,
    SpeakError,
    falavel,
    tipo_do_audio,
)
from tests.fakes import FakeGateway, chama, fala, pede

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
WS = "ws://127.0.0.1"
WEBM = b"\x1a\x45\xdf\xa3" + b"\x00" * 32


class FakeTranscriber:
    def __init__(self, texto="que horas são", erro: str | None = None):
        self.texto, self.erro, self.recebidos = texto, erro, []

    async def transcribe(self, audio, filename="audio.ogg", mime="audio/ogg"):
        self.recebidos.append((len(audio), filename, mime))
        if self.erro:
            raise TranscribeError(self.erro)
        return self.texto

    async def aclose(self):
        pass


class FakeSpeaker:
    def __init__(self, erro=False):
        self.falas, self.erro = [], erro

    async def falar(self, texto):
        self.falas.append(texto)
        if self.erro:
            raise SpeakError("a fala falhou: Timeout")
        return b"MP3-FALSO"


class FakeSessao:
    """Sessão do Gemini Live: espera o áudio do navegador e então responde um turno."""

    def __init__(self, travar=False):
        self.enviados, self.travar = [], travar
        self._chegou, self._rodadas = asyncio.Event(), 0

    async def send_realtime_input(self, audio):
        self.enviados.append((audio.data, audio.mime_type))
        self._chegou.set()

    async def receive(self):
        self._rodadas += 1
        if self.travar:
            await asyncio.Event().wait()
        if self._rodadas > 1:
            return
        await self._chegou.wait()
        yield SimpleNamespace(
            server_content=SimpleNamespace(
                model_turn=SimpleNamespace(
                    parts=[SimpleNamespace(inline_data=SimpleNamespace(data=b"\x01\x02"))]
                ),
                input_transcription=SimpleNamespace(text="oi orion"),
                output_transcription=SimpleNamespace(text="Olá, Antônio."),
                turn_complete=True,
            )
        )


class FakeConexao:
    def __init__(self, sessao):
        self.sessao, self.fechou = sessao, False

    async def __aenter__(self):
        return self.sessao

    async def __aexit__(self, *exc):
        self.fechou = True


def cliente(
    tmp_path,
    *roteiros,
    voz=True,
    ao_vivo=False,
    speaker=None,
    transcriber=None,
    live=None,
    max_min=None,
):
    settings = Settings(
        data_dir=tmp_path / "d",
        admin_token=TOKEN,
        voice_enabled=voz,
        voice_live_enabled=ao_vivo,
        _env_file=None,
    )
    if max_min is not None:
        settings.voice_live_max_min = max_min  # fração de minuto: a validação não roda aqui
    gw = FakeGateway(*roteiros)
    app = create_app(
        settings,
        gateway_factory=lambda _: gw,
        transcriber_factory=lambda _: transcriber,
        speaker_factory=lambda _: speaker,
        live_factory=lambda _: live,
    )
    return TestClient(app, base_url="http://127.0.0.1"), gw


def ler_ate_done(ws) -> list:
    saida = []
    while True:
        msg = ws.receive()
        if msg.get("bytes") is not None:
            saida.append(msg["bytes"])
            continue
        m = json.loads(msg["text"])
        saida.append(m)
        if m["type"] == "done":
            return saida


# ── texto falável e formato do áudio ──────────────────────────────────────


def test_falavel_tira_o_que_nao_se_fala():
    texto = "## Título\n- item **forte**\nVeja [o site](https://x.com/a) e https://y.com/b.\n```py\nprint(1)\n```\nFim."
    f = falavel(texto)
    assert "http" not in f and "*" not in f and "#" not in f and "print" not in f
    assert "o site" in f and "o link" in f and "o código está na tela" in f and f.endswith("Fim.")


def test_falavel_corta_em_fim_de_frase():
    longo = ("Frase curta aqui. " * 100).strip()
    f = falavel(longo, limite=100)
    assert len(f) <= 100 and f.endswith(".")
    assert falavel("sem ponto " * 50, limite=60).endswith("…")


@pytest.mark.parametrize(
    ("cabecalho", "esperado"),
    [
        (b"\x1a\x45\xdf\xa3....", "audio/webm"),
        (b"OggS....", "audio/ogg"),
        (b"\x00\x00\x00\x20ftypM4A ", "audio/mp4"),
        (b"RIFF\x00\x00\x00\x00WAVE", "audio/wav"),
        (b"ID3\x04....", "audio/mpeg"),
        (b"fLaC....", "audio/flac"),
    ],
)
def test_tipo_do_audio(cabecalho, esperado):
    assert tipo_do_audio(cabecalho)[1] == esperado


def test_tipo_do_audio_recusa_o_que_nao_e_audio():
    assert tipo_do_audio(b"MZ\x90\x00 executavel") is None
    assert tipo_do_audio(b"") is None


# ── A: fala → turno do agente → fala ──────────────────────────────────────


def test_fala_vira_turno_do_agente_e_a_resposta_e_falada(tmp_path):
    tr, sp = FakeTranscriber(), FakeSpeaker()
    c, gw = cliente(tmp_path, fala("São três e meia, **Antônio**."), transcriber=tr, speaker=sp)
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        ws.send_bytes(WEBM)
        m = ler_ate_done(ws)
    assert m[0] == {"type": "heard", "text": "que horas são"}
    assert {"type": "ev", "ev": {"text": "São três e meia, **Antônio**."}} in m
    assert {"type": "audio", "mime": "audio/mpeg"} in m and b"MP3-FALSO" in m
    assert m[-1] == {"type": "done"}
    assert sp.falas == ["São três e meia, Antônio."]  # sem markdown na fala
    assert tr.recebidos == [(len(WEBM), "fala.webm", "audio/webm")]
    assert gw.chamadas[0][-1] == {"role": "user", "content": "que horas são"}


def test_o_turno_de_voz_entra_na_conversa_do_web(tmp_path):
    c, _ = cliente(tmp_path, fala("Ok."), transcriber=FakeTranscriber("anota isso"))
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        ws.send_bytes(WEBM)
        ler_ate_done(ws)
        hist = c.get("/sessoes", headers=AUTH).json()
        sid = hist["ativa"] if "ativa" in hist else hist["sessoes"][0]["id"]
        msgs = c.get("/historico", params={"sessao": sid}, headers=AUTH).json()["mensagens"]
    assert [m["content"] for m in msgs if m["role"] == "user"] == ["anota isso"]


def test_sem_fala_configurada_responde_so_em_texto(tmp_path):
    c, _ = cliente(tmp_path, fala("Oi."), transcriber=FakeTranscriber(), speaker=None)
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        ws.send_bytes(WEBM)
        m = ler_ate_done(ws)
    assert not any(isinstance(x, bytes) for x in m) and m[-1] == {"type": "done"}


def test_falha_da_fala_nao_perde_o_texto(tmp_path):
    c, _ = cliente(
        tmp_path, fala("Oi."), transcriber=FakeTranscriber(), speaker=FakeSpeaker(erro=True)
    )
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        ws.send_bytes(WEBM)
        m = ler_ate_done(ws)
    assert {"type": "ev", "ev": {"text": "Oi."}} in m
    assert m[-2:] == [{"type": "error", "msg": "a fala falhou: Timeout"}, {"type": "done"}]


def test_erro_de_transcricao_chega_ao_navegador_sem_rodar_o_agente(tmp_path):
    c, gw = cliente(tmp_path, transcriber=FakeTranscriber(erro="não entendi nada neste áudio"))
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        ws.send_bytes(WEBM)
        m = ler_ate_done(ws)
        ws.send_bytes(b"MZ nao e audio")  # a conexão segue de pé
        m2 = ler_ate_done(ws)
    assert m == [{"type": "error", "msg": "não entendi nada neste áudio"}, {"type": "done"}]
    assert m2 == [{"type": "error", "msg": "formato de áudio não reconhecido"}, {"type": "done"}]
    assert gw.chamadas == []


def test_acao_que_pede_aprovacao_so_e_avisada_na_voz_nunca_decidida(tmp_path):
    """Regra 2: a aprovação é por botão. A voz avisa e o cartão aparece; nada executa."""
    sp = FakeSpeaker()
    c, _ = cliente(
        tmp_path,
        pede(chama("esquecer_fato", id=1)),
        fala("Aguardando você."),
        transcriber=FakeTranscriber("esqueça esse fato, aprovado"),
        speaker=sp,
    )
    with c:
        c.app.state.orion.memory.add_fact("Antônio gosta de café", "manual")
        with c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
            ws.send_bytes(WEBM)
            m = ler_ate_done(ws)
        assert any(
            x.get("type") == "ev" and "approval" in x["ev"] for x in m if isinstance(x, dict)
        )
        assert len(c.app.state.orion.memory.facts()) == 1
        assert len(c.app.state.orion.policy.approvals.pending()) == 1
    assert sp.falas[0].endswith(AVISO_APROVACAO)


def test_um_turno_por_vez_e_cancelar(tmp_path):
    class Lento(FakeTranscriber):
        async def transcribe(self, *a, **k):
            await asyncio.sleep(30)

    c, _ = cliente(tmp_path, transcriber=Lento())
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        ws.send_bytes(WEBM)
        ws.send_bytes(WEBM)
        assert ws.receive_json() == {"type": "error", "msg": "ainda estou no turno anterior"}
        ws.send_text(json.dumps({"cmd": "cancel"}))
        assert ws.receive_json() == {"type": "done"}
        ws.send_bytes(WEBM)  # depois de cancelar, aceita outro
        ws.send_text(json.dumps({"cmd": "cancel"}))
        assert ws.receive_json() == {"type": "done"}


def test_falha_interna_do_agente_nao_derruba_a_conexao(tmp_path):
    c, _ = cliente(tmp_path, RuntimeError("quebrou"), transcriber=FakeTranscriber())
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        ws.send_bytes(WEBM)
        m = ler_ate_done(ws)
    assert m[-2:] == [{"type": "error", "msg": "falha interna no turno"}, {"type": "done"}]


def test_fala_grande_demais_e_recusada(tmp_path, monkeypatch):
    monkeypatch.setattr(orion_app, "MAX_AUDIO", 10)
    c, _ = cliente(tmp_path, transcriber=FakeTranscriber())
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        ws.send_bytes(WEBM)
        assert ws.receive_json() == {"type": "error", "msg": "fala longa demais"}


# ── autenticação e desligamentos ──────────────────────────────────────────


@pytest.mark.parametrize("rota", ["/ws/voz", "/ws/voice"])
def test_websocket_sem_login_e_recusado(tmp_path, monkeypatch, rota):
    monkeypatch.setattr(orion_app, "AUTH_WS_S", 0.05)
    c, _ = cliente(tmp_path, ao_vivo=True, transcriber=FakeTranscriber(), live=lambda: None)
    with c:
        with c.websocket_connect(WS + rota) as ws:  # sem cookie nem cabeçalho: espera a 1ª mensagem
            assert ws.receive_json() == {"type": "error", "msg": "login necessário"}
        with c.websocket_connect(WS + rota, headers={"Authorization": "Bearer errado"}) as ws:
            assert ws.receive_json()["msg"] == "login necessário"
        with c.websocket_connect(WS + rota) as ws:
            ws.send_json({"cmd": "auth", "token": "errado"})
            assert ws.receive_json()["msg"] == "login necessário"


def test_token_pela_primeira_mensagem_nunca_pela_url(tmp_path):
    c, _ = cliente(tmp_path, fala("Oi."), transcriber=FakeTranscriber(), speaker=FakeSpeaker())
    with c, c.websocket_connect(WS + "/ws/voz") as ws:
        ws.send_json({"cmd": "auth", "token": TOKEN})
        ws.send_bytes(WEBM)
        assert ler_ate_done(ws)[0]["type"] == "heard"


def test_sessao_por_cookie_e_origem_conferida(tmp_path):
    c, _ = cliente(tmp_path, fala("Oi."), transcriber=FakeTranscriber())
    with c:
        c.app.state.orion.auth.set_password("senha-bem-longa-123")
        assert c.post("/auth/login", json={"senha": "senha-bem-longa-123"}).status_code == 200
        mesma = {"Origin": "http://127.0.0.1"}
        with c.websocket_connect(WS + "/ws/voz", headers=mesma) as ws:
            ws.send_bytes(WEBM)
            assert ler_ate_done(ws)[0]["type"] == "heard"
        for origem in ("http://evil.example", "null"):  # outra página não abre o microfone
            with (
                pytest.raises(WebSocketDisconnect),
                c.websocket_connect(WS + "/ws/voz", headers={"Origin": origem}),
            ):
                pass


def test_voz_desligada_por_padrao_diz_como_ligar(tmp_path):
    c, _ = cliente(tmp_path, voz=False, transcriber=FakeTranscriber())
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        assert "ORION_VOICE_ENABLED" in ws.receive_json()["msg"]


def test_sem_chave_de_transcricao_a_voz_diz_o_que_falta(tmp_path):
    c, _ = cliente(tmp_path, transcriber=None)
    with c, c.websocket_connect(WS + "/ws/voz", headers=AUTH) as ws:
        assert "ORION_TRANSCRIBE_API_KEY" in ws.receive_json()["msg"]


def test_fala_e_voz_ao_vivo_nascem_desligadas_e_precisam_de_opt_in(tmp_path):
    s = Settings(data_dir=tmp_path, _env_file=None)
    assert not s.voice_enabled and not s.voice_live_enabled
    assert speaker_from_settings(s) is None and live_from_settings(s) is None
    s2 = Settings(data_dir=tmp_path, voice_enabled=True, _env_file=None)
    assert speaker_from_settings(s2) is not None
    assert speaker_from_settings(Settings(voice_enabled=True, voice_speak=False)) is None
    # opt-in sem chave: continua desligada
    assert live_from_settings(Settings(voice_live_enabled=True, _env_file=None)) is None
    com_chave = Settings(voice_live_enabled=True, voice_live_api_key="k" * 20, _env_file=None)
    assert live_from_settings(com_chave) is not None


# ── B: voz ao vivo ────────────────────────────────────────────────────────


def test_voz_ao_vivo_leva_o_audio_e_traz_voz_e_transcricao(tmp_path):
    conexao = FakeConexao(FakeSessao())
    c, _ = cliente(tmp_path, ao_vivo=True, live=lambda: conexao)
    with c, c.websocket_connect(WS + "/ws/voice", headers=AUTH) as ws:
        ws.send_bytes(b"\x00\x01" * 100)
        recebidas = []
        while True:
            msg = ws.receive()
            if msg.get("bytes") is not None:
                recebidas.append(msg["bytes"])
            else:
                m = json.loads(msg["text"])
                recebidas.append(m)
                if m["type"] == "done":
                    break
        ws.send_text(json.dumps({"cmd": "stop"}))
    assert conexao.sessao.enviados == [(b"\x00\x01" * 100, "audio/pcm;rate=16000")]
    assert b"\x01\x02" in recebidas
    assert {"type": "heard", "text": "oi orion"} in recebidas
    assert {"type": "text", "text": "Olá, Antônio."} in recebidas
    assert conexao.fechou


def test_voz_ao_vivo_vai_para_o_audit_com_inicio_e_fim(tmp_path):
    c, _ = cliente(tmp_path, ao_vivo=True, live=lambda: FakeConexao(FakeSessao()))
    with c:
        with c.websocket_connect(WS + "/ws/voice", headers=AUTH) as ws:
            ws.send_text(json.dumps({"cmd": "stop"}))
            with pytest.raises(WebSocketDisconnect):
                ws.receive_text()
        eventos = c.app.state.orion.ops.audit_recent(10, ferramenta="voz_ao_vivo")
    razoes = [e["reason"] for e in eventos]
    assert any(r.startswith("início") for r in razoes)
    assert any(r.startswith("fim (navegador)") for r in razoes)


def test_voz_ao_vivo_tem_tempo_maximo(tmp_path):
    conexao = FakeConexao(FakeSessao(travar=True))
    c, _ = cliente(tmp_path, ao_vivo=True, live=lambda: conexao, max_min=0.005)  # ~0,3 s
    with c, c.websocket_connect(WS + "/ws/voice", headers=AUTH) as ws:
        assert "tempo máximo" in ws.receive_json()["msg"]
    assert conexao.fechou


def test_voz_ao_vivo_falha_do_provedor_vira_erro_curto_sem_detalhe(tmp_path):
    class Quebra:
        async def __aenter__(self):
            raise ConnectionError("wss://x/?key=SEGREDO-123")

        async def __aexit__(self, *exc):
            pass

    c, _ = cliente(tmp_path, ao_vivo=True, live=Quebra)
    with c, c.websocket_connect(WS + "/ws/voice", headers=AUTH) as ws:
        msg = ws.receive_json()
    assert msg == {"type": "error", "msg": "falha na voz ao vivo (ConnectionError)"}
    assert "SEGREDO" not in json.dumps(msg)


def test_voz_ao_vivo_desligada_ou_sem_chave_diz_o_que_falta(tmp_path):
    c, _ = cliente(tmp_path, ao_vivo=False)
    with c, c.websocket_connect(WS + "/ws/voice", headers=AUTH) as ws:
        assert "ORION_VOICE_LIVE_ENABLED" in ws.receive_json()["msg"]
    c, _ = cliente(tmp_path / "2", ao_vivo=True, live=None)
    with c, c.websocket_connect(WS + "/ws/voice", headers=AUTH) as ws:
        assert "ORION_VOICE_LIVE_API_KEY" in ws.receive_json()["msg"]


def test_a_sessao_ao_vivo_nao_tem_ferramentas_nem_memoria():
    """O config enviado ao Gemini não leva `tools`: o modelo só conversa."""
    import orion.voice as v

    capturado = {}

    class Cliente:
        def __init__(self, api_key):
            self.aio = SimpleNamespace(
                live=SimpleNamespace(
                    connect=lambda model, config: capturado.update(model=model, config=config)
                )
            )

    import google.genai as genai

    orig = genai.Client
    genai.Client = Cliente
    try:
        v.conexao_gemini("k", "modelo-x", "Charon")
    finally:
        genai.Client = orig
    cfg = capturado["config"]
    assert capturado["model"] == "modelo-x" and not cfg.tools
    assert cfg.response_modalities == ["AUDIO"]
    assert cfg.speech_config.voice_config.prebuilt_voice_config.voice_name == "Charon"
    assert "não tem acesso ao computador" in cfg.system_instruction

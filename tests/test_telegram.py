import asyncio
import json
import logging

import httpx
import pytest
from fastapi.testclient import TestClient
from pydantic import ValidationError

from orion.agent import Agent
from orion.app import create_app, telegram_from_settings
from orion.channels import TelegramChannel, TelegramError
from orion.channels.telegram import BOAS_VINDAS, _dividir
from orion.config import Settings
from orion.log import setup_logging
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.policy import ApprovalStore, PathGuard, PolicyEngine, Status
from orion.tools import ToolRegistry, memory_tools
from tests.fakes import FakeGateway, chama, fala, pede

TOKEN = "123456789:AAH-token_falso_com_tamanho_suficiente_0123456789"
USER, OUTRO = 111, 222


class FakeTelegram:
    """API do Telegram de mentira: guarda o que o bot enviou e devolve lotes de atualizações."""

    def __init__(self):
        self.chamadas: list[tuple[str, dict]] = []
        self.lotes: list[list[dict]] = []
        self.falhas: dict[str, list] = {}  # método -> fila de (status, descrição) ou "rede"
        self._id = 100
        self.arquivos: dict[str, bytes] = {}  # file_id -> conteúdo (getFile + download)

    async def __call__(self, req: httpx.Request) -> httpx.Response:
        assert f"/bot{TOKEN}/" in req.url.path
        if req.url.path.startswith("/file/"):  # download: /file/bot<TOKEN>/<file_path>
            caminho = req.url.path.split(f"/bot{TOKEN}/", 1)[1]
            self.chamadas.append(("download", {"file_path": caminho}))
            for fid, conteudo in self.arquivos.items():
                if caminho == f"arquivos/{fid}":
                    return httpx.Response(200, content=conteudo)
            return httpx.Response(404)
        metodo = req.url.path.rsplit("/", 1)[-1]
        corpo = json.loads(req.content) if req.content else {}
        self.chamadas.append((metodo, corpo))
        if self.falhas.get(metodo):
            falha = self.falhas[metodo].pop(0)
            if falha == "rede":  # a mensagem de erro do httpx carrega a URL (e o token)
                raise httpx.ConnectError(f"falhou em {req.url}")
            status, descricao = falha
            return httpx.Response(status, json={"ok": False, "description": descricao})
        if metodo == "getFile":
            fid = corpo["file_id"]
            if fid not in self.arquivos:
                return httpx.Response(400, json={"ok": False, "description": "file not found"})
            return httpx.Response(
                200,
                json={
                    "ok": True,
                    "result": {
                        "file_path": f"arquivos/{fid}",
                        "file_size": len(self.arquivos[fid]),
                    },
                },
            )
        if metodo == "getMe":
            return httpx.Response(200, json={"ok": True, "result": {"username": "orion_bot"}})
        if metodo == "getUpdates":
            lote = self.lotes.pop(0) if self.lotes else []
            if not lote:
                await asyncio.sleep(
                    0.002
                )  # long polling de verdade demora; sem isto o laço não cede
            return httpx.Response(200, json={"ok": True, "result": lote})
        if metodo == "sendMessage":
            self._id += 1
            return httpx.Response(200, json={"ok": True, "result": {"message_id": self._id}})
        return httpx.Response(200, json={"ok": True, "result": True})

    def enviadas(self) -> list[dict]:
        return [c for m, c in self.chamadas if m == "sendMessage"]

    def textos(self) -> list[str]:
        return [c["text"] for c in self.enviadas()]

    def metodos(self) -> list[str]:
        return [m for m, _ in self.chamadas]


def msg(texto, uid=USER, tipo="private", update_id=1):
    return {
        "update_id": update_id,
        "message": {
            "message_id": 1,
            "from": {"id": uid},
            "chat": {"id": uid, "type": tipo},
            "text": texto,
        },
    }


def clique(data, uid=USER, update_id=2):
    return {
        "update_id": update_id,
        "callback_query": {
            "id": "cb1",
            "from": {"id": uid},
            "data": data,
            "message": {"message_id": 50, "chat": {"id": uid, "type": "private"}},
        },
    }


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "t.db")
    yield s
    s.close()


@pytest.fixture
def policy(tmp_path):
    guard = PathGuard(
        protected_roots=(tmp_path / "Orion",), safe_roots=(tmp_path / "Documents",), system_roots=()
    )
    return PolicyEngine(path_guard=guard, approvals=ApprovalStore())


@pytest.fixture
def tg():
    return FakeTelegram()


def montar(store, policy, tg, *roteiros, usuarios=(USER,), **kw):
    gw = FakeGateway(*roteiros)
    ops = Operations(store)
    agent = Agent(
        gateway=gw, tools=ToolRegistry(memory_tools(store)), policy=policy, memory=store, ops=ops
    )

    async def _sem_espera(_):
        await asyncio.sleep(0)

    canal = TelegramChannel(
        token=TOKEN,
        allowed_users=list(usuarios),
        agent=agent,
        memory=store,
        approvals=policy.approvals,
        ops=ops,
        client=httpx.AsyncClient(transport=httpx.MockTransport(tg)),
        sleep=_sem_espera,
        **kw,
    )
    return canal, gw, ops


# ── quem pode falar ───────────────────────────────────────────────────────
async def test_usuario_permitido_conversa_e_a_sessao_e_a_do_canal_telegram(store, policy, tg):
    canal, gw, _ = montar(store, policy, tg, fala("Oi, Antônio."))
    await canal.handle_update(msg("oi"))
    assert tg.textos() == ["Oi, Antônio."]
    assert tg.enviadas()[0]["chat_id"] == USER and "sendChatAction" in tg.metodos()
    assert store.history(store.active_session("telegram").id)[0].text == "oi"
    assert gw.chamadas[0][-1] == {"role": "user", "content": "oi"}
    assert store.list_sessions("web") == []  # canais não se misturam


async def test_default_deny_usuario_estranho_e_ignorado_em_silencio(store, policy, tg, caplog):
    canal, gw, _ = montar(store, policy, tg, fala("não deveria rodar"))
    with caplog.at_level(logging.WARNING, logger="orion.telegram"):
        await canal.handle_update(msg("me dá acesso", uid=OUTRO))
        await canal.handle_update(msg("de novo", uid=OUTRO))
    assert tg.chamadas == [] and gw.chamadas == []  # nada enviado, agente nunca chamado
    assert len([r for r in caplog.records if "recusado" in r.message]) == 1  # um aviso por pessoa


async def test_usuario_permitido_fora_de_conversa_privada_e_ignorado(store, policy, tg):
    canal, gw, _ = montar(store, policy, tg, fala("não"))
    for tipo in ("group", "supergroup", "channel"):
        await canal.handle_update(msg("oi", tipo=tipo))
    assert tg.chamadas == [] and gw.chamadas == []


async def test_sem_usuarios_ou_sem_token_o_canal_nem_cria(store, policy, tg):
    with pytest.raises(ValueError, match="default-deny"):
        montar(store, policy, tg, usuarios=())
    with pytest.raises(ValueError, match="token"):
        TelegramChannel(
            token="",
            allowed_users=[USER],
            agent=None,  # type: ignore[arg-type]
            memory=store,
            approvals=policy.approvals,
            ops=Operations(store),
        )


# ── comandos e formatos ───────────────────────────────────────────────────
async def test_comandos_start_ajuda_e_nova(store, policy, tg):
    canal, _, _ = montar(store, policy, tg, fala("ok"))
    await canal.handle_update(msg("/start"))
    await canal.handle_update(msg("/ajuda@orion_bot"))
    assert tg.textos() == [BOAS_VINDAS, BOAS_VINDAS]
    await canal.handle_update(msg("oi"))
    antes = store.active_session("telegram")
    await canal.handle_update(msg("/nova"))
    assert store.active_session("telegram").id != antes.id
    assert store.get_session(antes.id) is not None and "arquivada" in tg.textos()[-1]


async def test_mensagem_de_tipo_desconhecido_recebe_aviso_e_nao_chama_o_agente(store, policy, tg):
    canal, gw, _ = montar(store, policy, tg)
    figurinha = msg("x")
    del figurinha["message"]["text"]
    figurinha["message"]["sticker"] = {"file_id": "s"}
    await canal.handle_update(figurinha)
    assert tg.textos() == ["Entendo texto, voz e foto por aqui."] and gw.chamadas == []


# ── voz ───────────────────────────────────────────────────────────────────
def msg_voz(file_id="voz1.ogg", duracao=5, uid=USER):
    m = msg("x", uid=uid)
    del m["message"]["text"]
    m["message"]["voice"] = {"file_id": file_id, "duration": duracao, "mime_type": "audio/ogg"}
    return m


def transcritor(tg_voz, texto="marque reunião amanhã às 9", status=200):
    """Transcritor real (`orion.transcribe`) falando com uma API de mentira."""
    from orion.transcribe import Transcriber

    vistos = []

    def api(req: httpx.Request):
        vistos.append(req)
        if status != 200:
            return httpx.Response(status, text="chave KEY-SECRETA recusada")
        return httpx.Response(200, json={"text": texto})

    t = Transcriber("KEY-SECRETA", client=httpx.AsyncClient(transport=httpx.MockTransport(api)))
    return t, vistos


async def test_voz_e_transcrita_mostrada_e_so_entao_vai_ao_agente(store, policy, tg):
    t, vistos = transcritor(tg)
    tg.arquivos["voz1.ogg"] = b"OggS-audio"
    canal, gw, _ = montar(store, policy, tg, fala("Anotado."), transcriber=t)
    await canal.handle_update(msg_voz())
    assert tg.textos() == ["🎙️ Entendi: marque reunião amanhã às 9", "Anotado."]
    assert gw.chamadas[0][-1] == {"role": "user", "content": "marque reunião amanhã às 9"}
    (req,) = vistos
    assert req.headers["authorization"] == "Bearer KEY-SECRETA" and "KEY-SECRETA" not in str(
        req.url
    )
    assert b"OggS-audio" in req.content and b"whisper-large-v3-turbo" in req.content
    assert b'name="language"' in req.content and b"pt" in req.content


async def test_voz_sem_transcritor_longa_demais_ou_com_falha_avisa_sem_chamar_o_agente(
    store, policy, tg
):
    canal, gw, _ = montar(store, policy, tg)
    await canal.handle_update(msg_voz())
    assert "ORION_TRANSCRIBE_API_KEY" in tg.textos()[0]

    t, _ = transcritor(tg)
    tg.arquivos["voz1.ogg"] = b"x"
    canal, gw, _ = montar(store, policy, tg, transcriber=t)
    await canal.handle_update(msg_voz(duracao=301))
    assert "longo demais" in tg.textos()[-1]

    ruim, _ = transcritor(tg, status=429)
    canal, gw, _ = montar(store, policy, tg, transcriber=ruim)
    await canal.handle_update(msg_voz())
    assert "Não consegui entender o áudio" in tg.textos()[-1] and "HTTP 429" in tg.textos()[-1]
    assert "KEY-SECRETA" not in " ".join(tg.textos())  # nem a chave nem a URL vazam
    assert gw.chamadas == []


async def test_voz_de_usuario_estranho_nem_baixa_o_arquivo(store, policy, tg):
    t, vistos = transcritor(tg)
    canal, _, _ = montar(store, policy, tg, transcriber=t)
    await canal.handle_update(msg_voz(uid=OUTRO))
    assert tg.chamadas == [] and vistos == []


async def test_voz_que_o_telegram_nao_acha_vira_aviso(store, policy, tg):
    t, _ = transcritor(tg)
    canal, gw, _ = montar(store, policy, tg, transcriber=t)
    await canal.handle_update(msg_voz("voz-inexistente.ogg"))
    assert "Não consegui entender o áudio" in tg.textos()[-1] and gw.chamadas == []


# ── foto ──────────────────────────────────────────────────────────────────
def msg_foto(tamanhos, legenda=None):
    m = msg("x")
    del m["message"]["text"]
    m["message"]["photo"] = tamanhos
    if legenda:
        m["message"]["caption"] = legenda
    return m


async def test_foto_vai_ao_modelo_como_imagem_so_naquele_turno(store, policy, tg):
    tg.arquivos["pequena.jpg"], tg.arquivos["grande.png"] = b"JPEGPEQ", b"PNGGRANDE"
    canal, gw, _ = montar(store, policy, tg, fala("É um gato."), fala("Segue o papo."))
    await canal.handle_update(
        msg_foto(
            [{"file_id": "pequena.jpg", "file_size": 7}, {"file_id": "grande.png", "file_size": 9}],
            legenda="o que é isso?",
        )
    )
    assert tg.textos() == ["É um gato."]
    ultima = gw.chamadas[0][-1]
    assert ultima["role"] == "user" and ultima["content"][0] == {
        "type": "text",
        "text": ultima["content"][0]["text"],
    }
    assert ultima["content"][0]["text"].startswith("o que é isso?")
    import base64

    assert (
        ultima["content"][1]["image_url"]["url"]
        == "data:image/png;base64," + base64.b64encode(b"PNGGRANDE").decode()
    )
    # o histórico guarda o aviso, nunca a imagem
    guardado = store.history(store.active_session("telegram").id)[0].text
    assert "imagem" in guardado and "PNG" not in guardado
    await canal.handle_update(msg("e agora?"))
    assert gw.chamadas[1][-1] == {"role": "user", "content": "e agora?"}  # a imagem não volta
    assert all(isinstance(m["content"], str) for m in gw.chamadas[1])


async def test_foto_sem_legenda_pergunta_o_que_ve_e_foto_grande_escolhe_o_menor(store, policy, tg):
    tg.arquivos["a.jpg"], tg.arquivos["b.jpg"] = b"A", b"B" * 10
    canal, gw, _ = montar(store, policy, tg, fala("ok"))
    await canal.handle_update(
        msg_foto(
            [{"file_id": "a.jpg", "file_size": 1}, {"file_id": "b.jpg", "file_size": 50_000_000}]
        )
    )
    assert gw.chamadas[0][-1]["content"][0]["text"].startswith("O que você vê nesta imagem?")
    assert ("download", {"file_path": "arquivos/a.jpg"}) in tg.chamadas  # a de 50 MB nem é baixada


async def test_foto_que_nao_baixa_avisa_sem_chamar_o_agente(store, policy, tg):
    canal, gw, _ = montar(store, policy, tg)
    await canal.handle_update(msg_foto([{"file_id": "some.jpg", "file_size": 1}]))
    assert "Não consegui baixar a foto" in tg.textos()[-1] and gw.chamadas == []
    await canal.handle_update(msg_foto([{"width": 1}]))
    assert tg.textos()[-1] == "Não consegui ler essa foto."


async def test_resposta_longa_e_dividida_sem_passar_do_limite(store, policy, tg):
    longa = "\n".join(f"linha {i} " + "x" * 90 for i in range(120))  # ~11 mil caracteres
    canal, _, _ = montar(store, policy, tg, fala(longa))
    await canal.handle_update(msg("conta tudo"))
    assert len(tg.textos()) >= 3 and all(len(t) <= 4000 for t in tg.textos())
    assert "\n".join(tg.textos()) == longa  # nada perdido nem repetido
    assert _dividir("a" * 9000) == ["a" * 4000, "a" * 4000, "a" * 1000]
    assert _dividir("   ") == []


async def test_resposta_vazia_e_erro_do_modelo_viram_mensagem(store, policy, tg):
    from orion.gateway import GatewayError

    canal, _, _ = montar(store, policy, tg, fala("  "), GatewayError("tudo fora"))
    await canal.handle_update(msg("oi"))
    await canal.handle_update(msg("oi de novo"))
    assert tg.textos()[0] == "(sem resposta)"
    assert tg.textos()[1].startswith("⚠️ nenhum modelo respondeu")


# ── aprovação por botão (regra 2) ─────────────────────────────────────────
async def _pedir_aprovacao(canal, store, tg):
    fato = store.add_fact("dado a apagar", "manual")
    await canal.handle_update(msg("apague aquele fato"))
    cartao = tg.enviadas()[-1]
    botoes = cartao["reply_markup"]["inline_keyboard"][0]
    assert "Aprovar esta ação" in cartao["text"] and "esquecer_fato" in cartao["text"]
    return fato, botoes


async def test_botao_aprovar_executa_uma_unica_vez_e_tira_os_botoes(store, policy, tg):
    canal, gw, _ = montar(
        store,
        policy,
        tg,
        pede(chama("esquecer_fato", id=1)),
        fala("Aguardando sua aprovação."),
        fala("Pronto, apaguei."),
    )
    _, (aprovar, _negar) = await _pedir_aprovacao(canal, store, tg)
    assert aprovar["callback_data"].startswith("ap:") and len(aprovar["callback_data"]) <= 64
    assert len(store.facts()) == 1  # ainda não executou: só depois do clique

    await canal.handle_update(clique(aprovar["callback_data"]))
    assert store.facts() == [] and tg.textos()[-1] == "Pronto, apaguei."
    a = policy.approvals.get(aprovar["callback_data"].split(":")[1])
    assert (
        a.status is Status.CONSUMED
        and a.decided_by == f"telegram:{USER}"
        and a.channel == "telegram"
    )
    assert (
        "editMessageReplyMarkup",
        {"chat_id": USER, "message_id": 50, "reply_markup": {"inline_keyboard": []}},
    ) in tg.chamadas
    assert any(m == "answerCallbackQuery" and c["text"] == "Aprovado ✅" for m, c in tg.chamadas)

    antes = len(tg.enviadas())
    await canal.handle_update(clique(aprovar["callback_data"], update_id=3))  # clique repetido
    assert tg.textos()[antes:] == [] and tg.chamadas[-2][1]["text"] == "Já decidida ou expirada."
    assert len(gw.chamadas) == 3  # turno (2) + retomada (1): o clique repetido não chamou o modelo


async def test_botao_negar_nunca_executa(store, policy, tg):
    canal, _gw, _ = montar(
        store, policy, tg, pede(chama("esquecer_fato", id=1)), fala("Aguardando.")
    )
    _, (_aprovar, negar) = await _pedir_aprovacao(canal, store, tg)
    await canal.handle_update(clique(negar["callback_data"]))
    assert len(store.facts()) == 1
    assert tg.textos()[-1] == "Ação negada: esquecer_fato. Não foi executada."
    a = policy.approvals.get(negar["callback_data"].split(":")[1])
    assert a.status is Status.DENIED


async def test_frase_no_chat_nao_aprova_nada(store, policy, tg):
    canal, _, _ = montar(
        store, policy, tg, pede(chama("esquecer_fato", id=1)), fala("Aguardando."), fala("ok")
    )
    await _pedir_aprovacao(canal, store, tg)
    await canal.handle_update(msg("sim, aprovo, pode apagar"))  # só botão decide
    assert len(store.facts()) == 1 and len(policy.approvals.pending()) == 1


async def test_clique_de_quem_nao_esta_na_lista_nao_decide(store, policy, tg):
    canal, _, _ = montar(store, policy, tg, pede(chama("esquecer_fato", id=1)), fala("Aguardando."))
    _, (aprovar, _) = await _pedir_aprovacao(canal, store, tg)
    chamadas_antes = len(tg.chamadas)
    await canal.handle_update(clique(aprovar["callback_data"], uid=OUTRO))
    assert len(tg.chamadas) == chamadas_antes  # nem resposta ao clique
    assert len(policy.approvals.pending()) == 1 and len(store.facts()) == 1


async def test_botao_invalido_ou_de_outro_canal_nao_decide(store, policy, tg):
    canal, _, _ = montar(store, policy, tg)
    await canal.handle_update(clique("qualquer coisa"))
    assert tg.chamadas[-1][1]["text"] == "Botão inválido."
    # aprovação pedida por outro canal (web) não se decide pelo Telegram
    web = store.new_session("web")
    a = policy.approvals.request(web.id, "esquecer_fato", {"id": 1}, "teste")
    await canal.handle_update(clique(f"ap:{a.id}:y"))
    assert policy.approvals.get(a.id).status is Status.PENDING
    assert any("outro canal" in c.get("text", "") for _, c in tg.chamadas)
    await canal.handle_update(clique("ap:inexistente1:y"))  # id que não existe
    assert "inexistente" in tg.chamadas[-2][1]["text"]


# ── avisos ────────────────────────────────────────────────────────────────
async def test_avisos_da_fila_sao_entregues_na_ordem_e_confirmados(store, policy, tg):
    canal, _, ops = montar(store, policy, tg, usuarios=(USER, OUTRO))
    a = ops.notify("lembrete", "Pagar boleto")
    ops.notify("agendamento", "Briefing")
    assert await canal.deliver_notifications() == 2
    assert tg.textos() == ["🔔 Pagar boleto", "🔔 Briefing"]
    assert {c["chat_id"] for c in tg.enviadas()} == {USER}  # só o primeiro da lista recebe avisos
    assert ops.pending_notifications() == [] and not ops.ack_notification(a)
    assert await canal.deliver_notifications() == 0


async def test_aviso_nao_enviado_nao_e_confirmado_e_sai_na_proxima(store, policy, tg):
    canal, _, ops = montar(store, policy, tg)
    ops.notify("lembrete", "Pagar boleto")
    tg.falhas["sendMessage"] = [(500, "Internal Server Error")]
    assert await canal.deliver_notifications() == 0
    assert len(ops.pending_notifications()) == 1  # continua na fila
    assert await canal.deliver_notifications() == 1 and ops.pending_notifications() == []


# ── polling e laço ────────────────────────────────────────────────────────
async def test_poll_avanca_o_offset_e_trata_cada_atualizacao_em_tarefa_propria(store, policy, tg):
    canal, _, _ = montar(store, policy, tg, fala("um"), fala("dois"))
    tg.lotes = [[msg("a", update_id=10), msg("b", update_id=11)]]
    assert await canal.poll_once() == 2
    await asyncio.gather(*canal._tarefas)
    assert sorted(tg.textos()) == ["dois", "um"]
    assert await canal.poll_once() == 0
    ofertas = [c["offset"] for m, c in tg.chamadas if m == "getUpdates"]
    assert ofertas == [0, 12]  # confirma o que já recebeu
    assert tg.chamadas[0][1]["allowed_updates"] == ["message", "callback_query"]


async def test_laco_roda_trata_mensagens_e_cancela_limpo(store, policy, tg):
    canal, _, _ = montar(store, policy, tg, fala("oi!"))
    tg.lotes = [[msg("oi", update_id=1)]]
    tarefa = asyncio.create_task(canal.run())
    for _ in range(200):
        if tg.textos():
            break
        await asyncio.sleep(0.01)
    assert tg.textos() == ["oi!"] and tg.metodos()[0] == "getMe"
    tarefa.cancel()
    with pytest.raises(asyncio.CancelledError):
        await tarefa
    assert canal._tarefas == set()


async def test_token_recusado_encerra_o_canal_sem_insistir(store, policy, tg, caplog):
    canal, _, _ = montar(store, policy, tg)
    tg.falhas["getMe"] = [(401, "Unauthorized")]
    with caplog.at_level(logging.ERROR, logger="orion.telegram"):
        await asyncio.wait_for(canal.run(), timeout=2)  # volta sozinho
    assert tg.metodos() == ["getMe"] and "não iniciou" in caplog.text


async def test_erros_de_rede_e_conflito_recuam_e_o_laco_segue(store, policy, tg, caplog):
    canal, _, _ = montar(store, policy, tg, fala("voltei"))
    tg.falhas["getUpdates"] = ["rede", (409, "Conflict: terminated by other getUpdates request")]
    tg.lotes = [[msg("oi", update_id=1)]]
    with caplog.at_level(logging.WARNING, logger="orion.telegram"):
        tarefa = asyncio.create_task(canal.run())
        for _ in range(300):
            if tg.textos():
                break
            await asyncio.sleep(0.01)
        tarefa.cancel()
        await asyncio.gather(tarefa, return_exceptions=True)
    assert tg.textos() == ["voltei"]
    assert "pare o bot do legado" in caplog.text


# ── segredo ───────────────────────────────────────────────────────────────
async def test_token_nunca_aparece_em_erro_nem_em_log(store, policy, tg, caplog):
    canal, _, _ = montar(store, policy, tg)
    tg.falhas["getUpdates"] = ["rede"]
    with caplog.at_level(logging.DEBUG), pytest.raises(TelegramError) as e:
        await canal.poll_once()
    assert TOKEN not in str(e.value) and "ConnectError" in str(e.value)
    assert e.value.__cause__ is None  # a exceção original carrega a URL: não encadeia
    assert TOKEN not in caplog.text
    tg.falhas["sendMessage"] = [(400, "Bad Request: chat not found")]
    with pytest.raises(TelegramError, match="chat not found") as e2:
        await canal._enviar(USER, "x")
    assert TOKEN not in str(e2.value)


def test_logger_do_httpx_nao_registra_url_com_token():
    setup_logging("INFO", json_logs=False)
    try:
        assert logging.getLogger("httpx").level >= logging.WARNING
        assert logging.getLogger("httpcore").level >= logging.WARNING
    finally:
        logging.getLogger().handlers.clear()


# ── configuração e ligação ao app ─────────────────────────────────────────
def test_token_sem_lista_de_usuarios_nao_sobe_e_a_lista_aceita_dois_formatos(tmp_path):
    with pytest.raises(ValidationError, match="default-deny"):
        Settings(telegram_token=TOKEN, _env_file=None)
    s = Settings(telegram_token=TOKEN, telegram_allowed_users="123, 456", _env_file=None)
    assert s.telegram_allowed_users == [123, 456]
    assert Settings(telegram_allowed_users="[7, 8]", _env_file=None).telegram_allowed_users == [
        7,
        8,
    ]
    assert Settings(telegram_allowed_users="", _env_file=None).telegram_allowed_users == []
    with pytest.raises(ValidationError):
        Settings(telegram_allowed_users="abc", _env_file=None)


def test_lista_de_usuarios_vem_de_variavel_de_ambiente(monkeypatch):
    monkeypatch.setenv("ORION_TELEGRAM_TOKEN", TOKEN)
    monkeypatch.setenv("ORION_TELEGRAM_ALLOWED_USERS", "111,222")
    s = Settings(_env_file=None)
    assert s.telegram_token == TOKEN and s.telegram_allowed_users == [111, 222]


def test_fabrica_so_cria_o_canal_com_token_usuarios_e_gateway(tmp_path, store, policy, monkeypatch):
    monkeypatch.setattr("orion.app.get_secret", lambda nome: None)
    ops = Operations(store)
    agent = object()  # a fábrica só repassa
    sem_token = Settings(data_dir=tmp_path, _env_file=None)
    assert telegram_from_settings(sem_token, agent, store, policy, ops) is None  # type: ignore[arg-type]
    pronto = Settings(
        data_dir=tmp_path, telegram_token=TOKEN, telegram_allowed_users=[USER], _env_file=None
    )
    assert telegram_from_settings(pronto, None, store, policy, ops) is None  # sem gateway
    assert isinstance(telegram_from_settings(pronto, agent, store, policy, ops), TelegramChannel)  # type: ignore[arg-type]
    # token só no cofre do SO e sem lista de usuários: não sobe aberto
    monkeypatch.setattr("orion.app.get_secret", lambda nome: TOKEN)
    assert telegram_from_settings(sem_token, agent, store, policy, ops) is None  # type: ignore[arg-type]


def test_app_sobe_o_canal_no_lifespan_e_derruba_no_fim(tmp_path, tg):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token="token-de-teste-com-16+", _env_file=None
    )
    criados = []

    def fabrica(s, agent, memory, policy, ops):
        canal = TelegramChannel(
            token=TOKEN,
            allowed_users=[USER],
            agent=agent,
            memory=memory,
            approvals=policy.approvals,
            ops=ops,
            client=httpx.AsyncClient(transport=httpx.MockTransport(tg)),
        )
        criados.append(canal)
        return canal

    app = create_app(
        settings, gateway_factory=lambda _: FakeGateway(fala("x")), telegram_factory=fabrica
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        saude = c.get("/health", headers={"Authorization": "Bearer token-de-teste-com-16+"})
        assert saude.json()["components"]["telegram"] is True
        for _ in range(100):  # o getMe roda na tarefa de fundo
            if "getMe" in tg.metodos():
                break
            asyncio.run(asyncio.sleep(0.01))
        assert "getMe" in tg.metodos()
    assert len(criados) == 1


def test_sem_canal_o_health_diz_false(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token="token-de-teste-com-16+", _env_file=None
    )
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        saude = c.get("/health", headers={"Authorization": "Bearer token-de-teste-com-16+"})
        assert saude.json()["components"]["telegram"] is False


# ── argumentos que o cartão não consegue mostrar inteiros ─────────────────
CMD_LONGO = "echo ok " + "x" * 2500 + " ; rm -rf ~"  # o perigo está depois do corte de exibição


async def test_argumento_cortado_nao_pode_ser_aprovado_so_negado(store, policy, tg):
    canal, _, _ = montar(
        store, policy, tg, pede(chama("executar_comando", cmd=CMD_LONGO)), fala("Aguardando.")
    )
    await canal.handle_update(msg("rode isto"))
    cartao = tg.enviadas()[-1]
    botoes = cartao["reply_markup"]["inline_keyboard"][0]
    assert [b["text"] for b in botoes] == ["❌ Negar"]  # sem botão de aprovar
    assert "só dá para negar" in cartao["text"] and "rm -rf" not in cartao["text"]
    assert len(cartao["text"]) < 4000  # cabe numa mensagem do Telegram

    # clique forjado em "aprovar" (a API aceitaria qualquer callback_data): o canal recusa
    aprov_id = botoes[0]["callback_data"].split(":")[1]
    await canal.handle_update(clique(f"ap:{aprov_id}:y"))
    assert policy.approvals.get(aprov_id).status is Status.PENDING
    assert tg.chamadas[-2][1]["text"] == "Argumentos grandes demais: só dá para negar."

    await canal.handle_update(clique(botoes[0]["callback_data"], update_id=3))
    assert policy.approvals.get(aprov_id).status is Status.DENIED


async def test_argumento_curto_mantem_os_dois_botoes(store, policy, tg):
    canal, _, _ = montar(
        store,
        policy,
        tg,
        pede(chama("executar_comando", cmd="rm -rf ./build")),
        fala("Aguardando."),
    )
    await canal.handle_update(msg("limpe o build"))
    botoes = tg.enviadas()[-1]["reply_markup"]["inline_keyboard"][0]
    assert [b["text"] for b in botoes] == ["✅ Aprovar", "❌ Negar"]


# ── captura rápida e briefing ─────────────────────────────────────────────
@pytest.fixture
def vault(tmp_path):
    v = tmp_path / "vault"
    v.mkdir()
    return v


def com_captura(store, policy, tg, vault, *roteiros, **kw):
    from orion.capture import Capturer

    return montar(store, policy, tg, *roteiros, capture=Capturer(vault, "00 Inbox"), **kw)


def notas(vault):
    return sorted((vault / "00 Inbox").glob("*.md")) if (vault / "00 Inbox").exists() else []


async def test_capturar_com_texto_grava_a_nota_sem_chamar_o_modelo(store, policy, tg, vault):
    canal, gw, _ = com_captura(store, policy, tg, vault)
    await canal.handle_update(msg("/capturar Comprar cabo HDMI para o notebook"))
    (nota,) = notas(vault)
    assert "Comprar cabo HDMI para o notebook" in nota.read_text(encoding="utf-8")
    assert tg.textos() == [f"📝 Guardado no vault: 00 Inbox/{nota.name}"]
    assert gw.chamadas == [] and store.list_sessions("telegram") == []  # nada foi ao modelo
    assert str(vault) not in tg.textos()[0]  # nunca o caminho absoluto


async def test_capturar_sozinho_guarda_so_a_proxima_mensagem(store, policy, tg, vault):
    canal, gw, _ = com_captura(store, policy, tg, vault, fala("Oi!"))
    await canal.handle_update(msg("/capturar"))
    assert "próxima mensagem vira nota" in tg.textos()[0]
    await canal.handle_update(msg("https://exemplo.com/artigo"))
    (nota,) = notas(vault)
    assert "tags: [captura, link]" in nota.read_text() and gw.chamadas == []
    await canal.handle_update(msg("oi"))  # a captura foi de uso único: volta a conversar
    assert tg.textos()[-1] == "Oi!" and len(notas(vault)) == 1


async def test_captura_expira_e_qualquer_comando_cancela(store, policy, tg, vault):
    relogio = [1000.0]
    canal, _, _ = com_captura(
        store, policy, tg, vault, fala("respondi 1"), fala("respondi 2"), clock=lambda: relogio[0]
    )
    await canal.handle_update(msg("/capturar"))
    relogio[0] += 301  # passou dos 5 minutos
    await canal.handle_update(msg("isto é conversa"))
    assert notas(vault) == [] and tg.textos()[-1] == "respondi 1"
    await canal.handle_update(msg("/capturar"))
    await canal.handle_update(msg("/nova"))  # outro comando cancela
    await canal.handle_update(msg("isto também é conversa"))
    assert notas(vault) == [] and tg.textos()[-1] == "respondi 2"


async def test_capturar_sem_vault_configurado_avisa(store, policy, tg):
    canal, gw, _ = montar(store, policy, tg)
    await canal.handle_update(msg("/capturar algo"))
    assert "ORION_VAULT_DIR" in tg.textos()[0] and gw.chamadas == []


async def test_captura_de_estranho_nao_grava_nada(store, policy, tg, vault):
    canal, _, _ = com_captura(store, policy, tg, vault)
    await canal.handle_update(msg("/capturar plantar uma nota", uid=OUTRO))
    assert tg.chamadas == [] and notas(vault) == []


async def test_captura_por_voz_mostra_o_que_entendeu_e_guarda_o_texto(store, policy, tg, vault):
    t, _ = transcritor(tg, texto="ideia: automatizar o relatório semanal")
    tg.arquivos["voz1.ogg"] = b"OggS"
    canal, gw, _ = com_captura(store, policy, tg, vault, transcriber=t)
    await canal.handle_update(msg("/capturar"))
    await canal.handle_update(msg_voz())
    assert tg.textos()[1] == "🎙️ Entendi: ideia: automatizar o relatório semanal"
    assert tg.textos()[2].startswith("📝 Guardado no vault: 00 Inbox/")
    (nota,) = notas(vault)
    assert "tags: [captura, voz]" in nota.read_text() and gw.chamadas == []


async def test_captura_de_foto_guarda_em_anexos_com_a_legenda(store, policy, tg, vault):
    tg.arquivos["f1.jpg"] = b"\xff\xd8JPEGDATA"
    canal, gw, _ = com_captura(store, policy, tg, vault)
    await canal.handle_update(msg("/capturar"))
    await canal.handle_update(msg_foto([{"file_id": "f1.jpg", "file_size": 11}], legenda="quadro"))
    (nota,) = notas(vault)
    (foto,) = list((vault / "00 Inbox" / "anexos").glob("*.jpg"))
    assert foto.read_bytes() == b"\xff\xd8JPEGDATA" and f"![[{foto.name}]]" in nota.read_text()
    assert "quadro" in nota.read_text() and gw.chamadas == []


async def test_captura_que_falha_avisa_sem_derrubar_o_canal(store, policy, tg, tmp_path):
    from orion.capture import Capturer

    canal, _, _ = montar(store, policy, tg, capture=Capturer(tmp_path / "nao-existe"))
    await canal.handle_update(msg("/capturar algo"))
    assert "Não guardei" in tg.textos()[0] and TOKEN not in tg.textos()[0]


async def test_captura_de_tipo_nao_suportado_nao_grava(store, policy, tg, vault):
    canal, *_ = com_captura(store, policy, tg, vault)
    figurinha = msg("x")
    del figurinha["message"]["text"]
    figurinha["message"]["sticker"] = {"file_id": "s"}
    await canal.handle_update(msg("/capturar"))
    await canal.handle_update(figurinha)
    assert "Só guardo texto" in tg.textos()[-1] and notas(vault) == []


async def test_briefing_responde_na_hora_sem_chamar_o_modelo(store, policy, tg):
    canal, gw, ops = montar(store, policy, tg)
    ops.add_task("Estudar UML")
    await canal.handle_update(msg("/briefing"))
    assert "Bom dia, Antônio" in tg.textos()[0] and "Estudar UML" in tg.textos()[0]
    assert gw.chamadas == []


async def test_ajuda_lista_os_comandos_novos(store, policy, tg):
    assert "/capturar" in BOAS_VINDAS and "/briefing" in BOAS_VINDAS


async def test_painel_manda_o_texto_do_app_e_sem_ele_avisa(store, policy, tg):
    canal, gw, _ = montar(store, policy, tg)
    await canal.handle_update(msg("/painel"))
    assert "indisponível" in tg.textos()[0]
    canal.painel = lambda: "📊 Painel do Orion · teste"
    await canal.handle_update(msg("/painel"))
    assert tg.textos()[1] == "📊 Painel do Orion · teste" and gw.chamadas == []
    await canal.handle_update(msg("/painel", uid=OUTRO))  # estranho: nada
    assert len(tg.textos()) == 2


async def test_ajuda_lista_o_painel(store, policy, tg):
    assert "/painel" in BOAS_VINDAS


def test_app_liga_o_painel_ao_canal_telegram(tmp_path, tg):
    from fastapi.testclient import TestClient

    from orion.app import create_app

    settings = Settings(
        data_dir=tmp_path / "d", admin_token="token-de-teste-com-16+", _env_file=None
    )
    canais = []

    def fabrica(_s, agent, memory, policy, ops):
        c = TelegramChannel(
            token=TOKEN,
            allowed_users=[USER],
            agent=agent,  # type: ignore[arg-type]
            memory=memory,
            approvals=policy.approvals,
            ops=ops,
            client=httpx.AsyncClient(transport=httpx.MockTransport(tg)),
        )
        canais.append(c)
        return c

    with TestClient(
        create_app(settings, telegram_factory=fabrica, gateway_factory=lambda _: None),
        base_url="http://127.0.0.1",
    ):
        assert canais[0].painel is not None
        assert "Painel do Orion" in canais[0].painel()


async def test_tela_passa_o_argumento_ao_app_e_sem_ele_avisa(store, policy, tg):
    canal, gw, _ = montar(store, policy, tg)
    await canal.handle_update(msg("/tela"))
    assert "indisponível" in tg.textos()[0]
    recebidos = []
    canal.tela = lambda acao: recebidos.append(acao) or f"ok:{acao}"
    await canal.handle_update(msg("/tela pausar"))
    await canal.handle_update(msg("/tela"))
    assert recebidos == ["pausar", ""] and tg.textos()[1:] == ["ok:pausar", "ok:"]
    await canal.handle_update(msg("/tela limpar", uid=OUTRO))  # estranho: nada
    assert len(tg.textos()) == 3 and gw.chamadas == [] and "/tela" in BOAS_VINDAS

"""Ponte de desktop (E2.1, regra 50): núcleo com adaptadores falsos e as rotas do servidor."""

from __future__ import annotations

import asyncio
import json
import threading
from contextlib import asynccontextmanager

import httpx
import pytest
from fastapi.testclient import TestClient
from starlette.websockets import WebSocketDisconnect

from orion.ponte.config import PADRAO, TeclasInvalidas, mapa_de_teclas, normalizar, para_pynput
from orion.ponte.hub import ComandoInvalido, PonteHub, validar
from orion.ponte.nucleo import Adaptadores, Ponte
from tests.test_app_sessions import AUTH, app_at

# ── teclas ────────────────────────────────────────────────────────────────


def test_mapa_padrao_e_troca_por_json():
    assert mapa_de_teclas("") == PADRAO
    troca = mapa_de_teclas('{"captura": "Ctrl+Shift+K"}')
    assert troca["captura"] == "ctrl+shift+k" and troca["ocr"] == PADRAO["ocr"]
    assert para_pynput("ctrl+alt+space") == "<ctrl>+<alt>+<space>"
    assert para_pynput("ctrl+shift+k") == "<ctrl>+<shift>+k"


@pytest.mark.parametrize(
    "bruto",
    [
        "não é json",
        "[1]",
        '{"voar": "ctrl+alt+v"}',
        '{"captura": "k"}',  # sem modificador: capturaria a digitação toda
        '{"captura": "ctrl+alt"}',  # sem tecla
        '{"captura": "ctrl+a+b"}',  # duas teclas
        '{"captura": 3}',
        '{"captura": "ctrl+alt+t"}',  # repete a combinação do OCR
    ],
)
def test_teclas_invalidas_sao_recusadas(bruto):
    with pytest.raises(TeclasInvalidas):
        mapa_de_teclas(bruto)


def test_normalizar_ordem_e_duplicatas():
    assert normalizar("ALT + ctrl + alt + F5") == "alt+ctrl+f5"


# ── comandos ──────────────────────────────────────────────────────────────


def test_validar_aceita_so_abrir_e_colar():
    assert validar({"cmd": "abrir", "rota": "#/chat"}) == {"cmd": "abrir", "rota": "#/chat"}
    assert validar({"cmd": "abrir"}) == {"cmd": "abrir", "rota": "#/"}
    assert validar({"cmd": "colar", "texto": "oi", "extra": 1}) == {"cmd": "colar", "texto": "oi"}
    for ruim in (
        {"cmd": "executar", "texto": "rm -rf /"},
        {"cmd": "abrir", "rota": "javascript:alert(1)"},
        {"cmd": "abrir", "rota": "#/../x y"},
        {"cmd": "colar", "texto": "   "},
        {"cmd": "colar", "texto": "x" * 8001},
        {"cmd": "colar", "texto": 5},
        {},
    ):
        with pytest.raises(ComandoInvalido):
            validar(ruim)


# ── núcleo com falsos ─────────────────────────────────────────────────────


class Registro:
    def __init__(self):
        self.eventos: list[tuple] = []

    def __call__(self, *a):
        self.eventos.append(a)


class TecladoFalso:
    def __init__(self, r):
        self.r, self.mapa = r, None

    def iniciar(self, mapa):
        self.mapa = mapa
        self.r("teclado:iniciar", sorted(mapa))

    def parar(self):
        self.r("teclado:parar")


class BandejaFalsa:
    def __init__(self, r):
        self.r, self.itens = r, None

    def iniciar(self, itens):
        self.itens = dict(itens)
        self.r("bandeja:iniciar", [t for t, _ in itens])

    def parar(self):
        self.r("bandeja:parar")


class JanelaFalsa:
    def __init__(self, r):
        self.r = r

    def abrir(self, url, titulo, largura, altura):
        self.r("janela", url, titulo, largura, altura)


class AreaFalsa:
    def __init__(self, r, conteudo="anterior"):
        self.r, self.conteudo = r, conteudo

    def ler(self):
        return self.conteudo

    def gravar(self, texto):
        self.conteudo = texto
        self.r("area:gravar", texto)


class ColadorFalso:
    def __init__(self, r, area):
        self.r, self.area = r, area

    def colar(self):
        self.r("colar", self.area.conteudo)  # o que estava na área NA HORA do Ctrl+V


def montar(handler=None, **extra):
    r = Registro()
    area = AreaFalsa(r)
    a = Adaptadores(
        teclado=TecladoFalso(r),
        bandeja=BandejaFalsa(r),
        janela=JanelaFalsa(r),
        area=area,
        colador=ColadorFalso(r, area),
        navegador=lambda url: r("navegador", url),
        notificar=lambda t, x: r("notificar", t, x),
        **extra,
    )
    http = httpx.Client(
        base_url="http://127.0.0.1:8000",
        transport=httpx.MockTransport(handler or (lambda req: httpx.Response(200, json={}))),
    )
    return (
        Ponte(
            "http://127.0.0.1:8000/", "tok-da-ponte", a, http=http, dormir=lambda s: r("dormir", s)
        ),
        r,
        a,
    )


def test_captura_rapida_abre_janela_com_token_no_fragmento():
    ponte, r, _ = montar()
    ponte.captura_rapida()
    [(_, url, titulo, w, h)] = r.eventos
    assert url == "http://127.0.0.1:8000/ui/ponte.html?modo=rapido#t=tok-da-ponte"
    assert "tok-da-ponte" not in url.split("#")[0]  # o token nunca vai na parte que o servidor vê
    assert (titulo, w, h) == ("Captura rápida", 560, 190)


def test_colar_poe_cola_e_devolve_a_area_de_transferencia():
    ponte, r, a = montar()
    ponte.colar("texto aprovado")
    assert [e[0] for e in r.eventos] == ["area:gravar", "colar", "dormir", "area:gravar"]
    assert ("colar", "texto aprovado") in r.eventos  # no Ctrl+V a área tinha o texto
    assert a.area.conteudo == "anterior"  # e depois voltou ao que havia


def test_comando_do_servidor_e_validado_de_novo_no_cliente():
    ponte, r, _ = montar()
    ponte.tratar({"cmd": "executar", "texto": "calc.exe"})
    ponte.tratar({"cmd": "abrir", "rota": "http://evil.test"})
    assert r.eventos == []
    ponte.tratar({"cmd": "abrir", "rota": "#/memoria"})
    assert r.eventos == [("navegador", "http://127.0.0.1:8000/ui/#/memoria")]


def test_comando_colar_roda_em_thread_sem_travar_o_laco():
    ponte, r, _ = montar()
    ponte.tratar({"cmd": "colar", "texto": "oi"})
    for t in threading.enumerate():
        if t is not threading.current_thread() and t.daemon:
            t.join(timeout=2)
    assert ("colar", "oi") in r.eventos


def test_panico_pela_tecla_chama_o_servidor_e_avisa():
    chamadas = []

    def handler(req):
        chamadas.append(
            (req.method, req.url.path, json.loads(req.content), req.headers.get("authorization"))
        )
        return httpx.Response(200, json={"panico": True})

    ponte, r, _ = montar(handler)
    ponte.panico()
    assert chamadas == [("POST", "/modo/panico", {"ativo": True}, None)] or chamadas[0][:3] == (
        "POST",
        "/modo/panico",
        {"ativo": True},
    )
    assert r.eventos[-1][0] == "notificar" and "pânico" in r.eventos[-1][2].lower()


def test_panico_com_servidor_fora_do_ar_avisa_em_vez_de_calar():
    def cai(req):
        raise httpx.ConnectError("sem rede")

    ponte, r, _ = montar(cai)
    ponte.panico()
    assert r.eventos[-1][0] == "notificar" and "não consegui" in r.eventos[-1][2].lower()


def test_teclas_e_bandeja_so_registram_o_que_a_ponte_sabe_fazer():
    ponte, _, _ = montar()
    acoes = ponte.acoes_das_teclas()
    assert set(acoes) == {
        "<ctrl>+<alt>+<space>",
        "<ctrl>+<alt>+shift+p".replace("shift", "<shift>"),
    }
    assert [t for t, _ in ponte.itens_da_bandeja()] == [
        "Abrir Orion",
        "Captura rápida",
        "Modo pânico",
        "Sair",
    ]


# ── laço do WebSocket ─────────────────────────────────────────────────────


class WsFalso:
    def __init__(self, mensagens, erro=None):
        self.mensagens, self.erro = list(mensagens), erro

    def __aiter__(self):
        return self

    async def __anext__(self):
        if self.mensagens:
            return self.mensagens.pop(0)
        if self.erro:
            raise self.erro
        raise StopAsyncIteration


def conector(*sessoes, vistas=None):
    fila = list(sessoes)

    @asynccontextmanager
    async def conectar(url, cabecalhos):
        if vistas is not None:
            vistas.append((url, cabecalhos))
        sessao = fila.pop(0)
        if isinstance(sessao, Exception):
            raise sessao
        yield sessao

    return conectar


def test_escutar_trata_comandos_reconecta_e_manda_o_token_no_cabecalho():
    ponte, r, _ = montar()
    vistas: list = []
    esperas: list[float] = []

    async def dormir(s):
        esperas.append(s)
        await asyncio.sleep(0)  # cede a vez: sem isso a tarefa que manda parar nunca roda

    class Cai(Exception):
        pass

    conectar = conector(
        WsFalso([json.dumps({"cmd": "abrir", "rota": "#/chat"}), "lixo não-json"]),
        Cai("caiu"),
        WsFalso([json.dumps({"cmd": "abrir", "rota": "#/painel"})]),
        vistas=vistas,
    )

    async def vai():
        dois = asyncio.Event()
        abrir = ponte.a.navegador

        def navegador(url):
            abrir(url)
            if len([e for e in r.eventos if e[0] == "navegador"]) >= 2:
                dois.set()

        ponte.a.navegador = navegador

        async def parar_depois():
            await dois.wait()
            ponte.sair()

        t = asyncio.create_task(parar_depois())
        motivo = await ponte.escutar(conectar, dormir_async=dormir)
        await t
        return motivo

    assert asyncio.run(vai()) == "sair"
    urls = [e[1] for e in r.eventos if e[0] == "navegador"]
    assert urls == ["http://127.0.0.1:8000/ui/#/chat", "http://127.0.0.1:8000/ui/#/painel"]
    assert vistas[0] == ("ws://127.0.0.1:8000/ws/ponte", {"Authorization": "Bearer tok-da-ponte"})
    assert esperas[:2] == [1.0, 1.0]  # conectou: a espera volta a 1 s


def test_escutar_para_quando_o_servidor_recusa_o_token():
    ponte, r, _ = montar()

    class Recusado(Exception):
        response = type("R", (), {"status_code": 403})()

    motivo = asyncio.run(
        ponte.escutar(conector(Recusado()), dormir_async=lambda s: asyncio.sleep(0))
    )
    assert motivo == "pareamento"
    assert r.eventos[-1][0] == "notificar" and "--parear" in r.eventos[-1][2]


def test_escutar_avisa_so_depois_de_tres_falhas_e_a_espera_cresce_ate_o_teto():
    ponte, r, _ = montar()
    esperas: list[float] = []

    async def dormir(s):
        esperas.append(s)
        if len(esperas) >= 7:
            ponte.sair()

    sessoes = [OSError("sem rede")] * 8
    asyncio.run(ponte.escutar(conector(*sessoes), dormir_async=dormir))
    assert esperas == [1.0, 2.0, 4.0, 8.0, 16.0, 30.0, 30.0]
    avisos = [e for e in r.eventos if e[0] == "notificar"]
    assert len(avisos) == 1


def test_rodar_sobe_e_desce_teclado_e_bandeja():
    ponte, r, _ = montar()

    @asynccontextmanager
    async def conectar(url, cabecalhos):
        ponte.sair()
        yield WsFalso([])

    assert ponte.rodar(conectar) == "sair"
    ordem = [e[0] for e in r.eventos]
    assert ordem[:2] == ["teclado:iniciar", "bandeja:iniciar"] and ordem[-2:] == [
        "teclado:parar",
        "bandeja:parar",
    ]


# ── servidor: token de escopo estreito, WebSocket, comandos ───────────────

HOST = "http://127.0.0.1"
WS = "ws://127.0.0.1"


def cliente(tmp_path):
    return TestClient(app_at(tmp_path), base_url=HOST)


def parear(c) -> str:
    return c.app.state.orion.auth.create_device_token("ponte", "teste")


def test_token_da_ponte_alcanca_so_as_rotas_da_ponte(tmp_path):
    with cliente(tmp_path) as c:
        h = {"Authorization": f"Bearer {parear(c)}"}
        for metodo, caminho in (
            ("get", "/sessoes"),
            ("post", "/chat"),
            ("get", "/painel"),
            ("get", "/facts"),
            ("post", "/ponte/comando"),
            ("get", "/ponte/estado"),
            ("post", "/modo/nao-perturbe"),
            ("get", "/capabilities/details"),
        ):
            r = getattr(c, metodo)(caminho, headers=h, **({"json": {}} if metodo == "post" else {}))
            assert r.status_code == 403, (metodo, caminho, r.status_code)
        # entrar no pânico é permitido (cortar é sempre seguro); sair não: precisa da senha
        assert c.post("/modo/panico", headers=h, json={"ativo": True}).status_code == 200
        assert (
            c.post("/modo/panico", headers=h, json={"ativo": False, "senha": "x"}).status_code
            == 403
        )
        # e o admin de sempre continua igual
        assert c.get("/sessoes", headers=AUTH).status_code == 200


def test_token_invalido_ou_revogado_nao_entra(tmp_path):
    with cliente(tmp_path) as c:
        auth = c.app.state.orion.auth
        token = parear(c)
        assert (
            c.get("/sessoes", headers={"Authorization": "Bearer " + token + "x"}).status_code == 401
        )
        assert auth.device_scope(token) == "ponte"
        novo = parear(c)  # parear de novo desfaz o anterior
        assert auth.device_scope(token) is None and auth.device_scope(novo) == "ponte"
        auth.set_password("uma-senha-bem-longa-123")  # trocar a senha desfaz os pareamentos
        assert auth.device_scope(novo) is None


def test_websocket_exige_token_da_ponte_e_entrega_comandos(tmp_path):
    with cliente(tmp_path) as c:
        hub: PonteHub = c.app.state.orion.ponte
        for headers in ({}, AUTH, {"Authorization": "Bearer errado"}):
            with pytest.raises(WebSocketDisconnect):
                with c.websocket_connect(WS + "/ws/ponte", headers=headers):
                    pass
        assert not hub.conectada
        r = c.post("/ponte/comando", headers=AUTH, json={"cmd": "abrir", "rota": "#/chat"})
        assert r.status_code == 503  # sem ponte conectada
        with c.websocket_connect(
            WS + "/ws/ponte", headers={"Authorization": f"Bearer {parear(c)}"}
        ) as ws:
            assert c.get("/ponte/estado", headers=AUTH).json()["conectada"] is True
            ok = c.post("/ponte/comando", headers=AUTH, json={"cmd": "abrir", "rota": "#/painel"})
            assert ok.status_code == 200
            assert json.loads(ws.receive_text()) == {"cmd": "abrir", "rota": "#/painel"}
            c.post("/ponte/comando", headers=AUTH, json={"cmd": "colar", "texto": "olá"})
            assert json.loads(ws.receive_text()) == {"cmd": "colar", "texto": "olá"}
            # comando fora da lista nem chega
            assert (
                c.post("/ponte/comando", headers=AUTH, json={"cmd": "executar"}).status_code == 422
            )
            assert (
                c.post(
                    "/ponte/comando", headers=AUTH, json={"cmd": "abrir", "rota": "x"}
                ).status_code
                == 422
            )
            assert c.post("/ponte/comando", json={"cmd": "abrir"}).status_code == 401
        assert not hub.conectada


def test_nova_ponte_derruba_a_antiga(tmp_path):
    with cliente(tmp_path) as c:
        token = parear(c)
        h = {"Authorization": f"Bearer {token}"}
        with c.websocket_connect(WS + "/ws/ponte", headers=h) as antiga:
            with c.websocket_connect(WS + "/ws/ponte", headers=h) as nova:
                with pytest.raises(WebSocketDisconnect):
                    antiga.receive_text()
                c.post("/ponte/comando", headers=AUTH, json={"cmd": "abrir"})
                assert json.loads(nova.receive_text())["cmd"] == "abrir"


def test_panico_derruba_a_ponte_e_recusa_nova_conexao(tmp_path, monkeypatch):
    monkeypatch.setattr("orion.ponte.rotas.VIGIA_S", 0.05)
    with cliente(tmp_path) as c:
        h = {"Authorization": f"Bearer {parear(c)}"}
        with c.websocket_connect(WS + "/ws/ponte", headers=h) as ws:
            c.post("/modo/panico", headers=AUTH, json={"ativo": True})
            with pytest.raises(WebSocketDisconnect):
                ws.receive_text()
        with pytest.raises(WebSocketDisconnect):
            with c.websocket_connect(WS + "/ws/ponte", headers=h):
                pass
        assert c.post("/ponte/comando", headers=AUTH, json={"cmd": "abrir"}).status_code == 409

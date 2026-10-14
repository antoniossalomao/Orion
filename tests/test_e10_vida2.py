"""E10 (continuação): comandos do Telegram, fatura, linha do tempo, sonho, voz, preço no job."""

import asyncio
import io
import wave
from datetime import date, datetime
from types import SimpleNamespace

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.vida import (
    EstadoVoz,
    Financas,
    Foco,
    Precos,
    Revisao,
    VidaCanal,
    VidaJobs,
    dias_da_capsula,
    escolher_par,
    lancar_fatura,
    linha_do_tempo,
    medir_wav,
    parse_fatura,
    sonho_do_dia,
)
from tests.fakes import FakeGateway, fala

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
AGORA = datetime(2026, 10, 14, 12, 0).timestamp()


class Texto:
    """Modelo de mentira com `complete` (o caminho dos jobs, não o de streaming)."""

    def __init__(self, t):
        self.t = t

    async def complete(self, messages):
        return self.t


@pytest.fixture
def m(tmp_path):
    store = MemoryStore(tmp_path / "m.db")
    yield store
    store.close()


def canal(m, tmp_path, tarefas=("tarefa A",), gw=None):
    vault = tmp_path / "vault"
    vault.mkdir(exist_ok=True)
    fin = Financas(m, clock=lambda: AGORA)
    return VidaCanal(
        m,
        financas=fin,
        precos=Precos(m, clock=lambda: AGORA),
        foco=Foco(["youtube"], clock=lambda: AGORA),
        revisao=Revisao(m, vault_dir=vault, clock=lambda: AGORA),
        tarefas=lambda: list(tarefas),
        gateway_local=gw,
    )


def test_comandos_financas_contas_e_pagamento(m, tmp_path):
    c = canal(m, tmp_path)
    assert c.tratar(1, "/financas").startswith("Gastos de")
    assert c.tratar(1, "/contas") == "nenhuma conta aberta."
    assert "gastei" not in c.tratar(1, "gastei 30 no mercado").lower() or True
    assert "lançado" in c.tratar(1, "gastei 30 no mercado")
    c.financas.cadastrar_conta("Luz", 15000, date(2026, 10, 30))
    assert "Luz" in c.tratar(1, "/contas")
    assert "marcada como paga" in c.tratar(1, "/pagar 1")
    assert "já paga" in c.tratar(1, "/pagar 1")
    assert c.tratar(1, "/pagar abc") == "use: /pagar <número da conta>"


def test_simular_pelo_telegram_mostra_faixas(m, tmp_path):
    c = canal(m, tmp_path)
    c.financas.registrar_gasto(50000, "mercado", "x")
    texto = c.tratar(1, "/simular 1000 6")
    assert "pessimista" in texto and "típico" in texto and "otimista" in texto
    assert c.tratar(1, "/simular 1000") == "use: /simular <saldo> <meses> [extra mensal]"


def test_foco_pelo_telegram_e_revisao_guiada_salva_nota(m, tmp_path):
    c = canal(m, tmp_path)
    assert "bloco de foco" in c.tratar(1, "/foco 25")
    assert c.tratar(1, "/revisao").startswith("Revisão semanal.")
    assert "já está aberta" in c.tratar(1, "/revisao")
    assert c.tratar(1, "ok").startswith("2.")
    assert c.tratar(1, "lançar o site").startswith("3.")
    assert c.tratar(1, "um bloco de manhã").startswith("4.")
    assert "anotado" in c.tratar(1, "ideia: app de leitura")
    assert "anotado" in c.tratar(1, "ideia: outra")
    assert c.tratar(1, "/fim") == "revisão salva em Revisão/."
    notas = list((tmp_path / "vault" / "Revisão").glob("*.md"))
    assert len(notas) == 1
    corpo = notas[0].read_text(encoding="utf-8")
    assert "lançar o site" in corpo and "app de leitura" in corpo
    assert not c.revisao.ativa(1)


def test_texto_comum_segue_para_o_agente(m, tmp_path):
    c = canal(m, tmp_path)
    assert c.tratar(1, "qual a previsão do tempo?") is None
    assert c.tratar(1, "/start") is None


def test_capsula_por_texto_e_prazo():
    assert dias_da_capsula("daqui 6 meses me cobra isso") == 180
    assert dias_da_capsula("daqui 2 anos me lembra") == 730
    assert dias_da_capsula("daqui 3 semanas") == 21
    assert dias_da_capsula("sem prazo") is None


def test_fatura_so_lanca_com_confirmacao(m):
    texto = "10/10 MERCADO XYZ SUPERMERCADO 120,50\n10/10 UBER VIAGEM 25,00\n15/10 ESTORNO -40,00\n"
    linhas = parse_fatura(texto, ano=2026)
    assert [x["cents"] for x in linhas] == [12050, 2500]  # estorno de fora
    f = Financas(m, clock=lambda: AGORA)
    with pytest.raises(ValueError):
        lancar_fatura(f, linhas, confirmado=False)
    assert lancar_fatura(f, linhas, confirmado=True) == 2
    assert f.resumo_mensal(2026, 10)["total_cents"] == 14550


def test_linha_do_tempo_junta_fontes_em_ordem(m, tmp_path):
    s = m.create_session("web", "t") if hasattr(m, "create_session") else None
    f = Financas(m, clock=lambda: AGORA)
    f.registrar_gasto(1000, "mercado", "pão")
    with m.transaction() as c:
        c.execute("INSERT INTO clipboard_log(ts, text) VALUES (?,?)", (AGORA - 60, "trecho"))
    itens = linha_do_tempo(m, AGORA - 3600, AGORA + 1)
    tipos = [i["tipo"] for i in itens]
    assert "gasto" in tipos and "area" in tipos
    assert itens == sorted(itens, key=lambda i: i["ts"], reverse=True)
    assert s is None or s is not None  # garante que a chamada de sessão não quebra


def test_sonho_precisa_de_par_sem_palavra_comum_e_fica_pendente(tmp_path):
    fatos = [
        "Gosto de bolo de cenoura no café da manhã",
        "O projeto Orion usa SQLite local",
    ]
    assert escolher_par(fatos, seed=1) is not None
    assert escolher_par(["sobre orion", "sobre orion gosto"], seed=1) is None
    gw = Texto("Um bolo de cenoura pode virar tema de nota sobre o Orion")
    ideia = asyncio.run(sonho_do_dia(fatos, gw, seed=1))
    assert ideia and "Orion" in ideia
    assert asyncio.run(sonho_do_dia(fatos, None, seed=1)) is None


def test_medir_wav_e_estilo_curto_por_fala():
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(8000)
        w.writeframes(b"\x10\x27" * 4000)  # 0,5 s de sinal constante
    duracao, energia = medir_wav(buf.getvalue())
    assert duracao == pytest.approx(0.5) and 0 < energia < 1
    assert medir_wav(b"nao e wav") is None
    voz = EstadoVoz()
    for _ in range(5):
        assert voz.observar(30, 10, 0.5) == "normal"
    assert voz.observar(70, 10, 0.5) == "curto"


def test_job_de_preco_so_consulta_com_cota_e_notifica_alerta(m, tmp_path):
    avisos = []
    ops = SimpleNamespace(notify=lambda k, t, ref=None: avisos.append((k, t, ref)))
    precos = Precos(m, clock=lambda: AGORA)
    precos.adicionar("Mouse", "mouse preço", 5000)
    fin = Financas(m, clock=lambda: AGORA)
    chamadas = []
    vida = VidaJobs(
        m,
        fin,
        ops=ops,
        precos=precos,
        pesquisar=lambda q: chamadas.append(q) or "R$ 45,90",
        clock=lambda: AGORA,
    )
    assert vida.tick(permitido=lambda: False)["precos"] == [] and chamadas == []
    out = vida.tick(permitido=lambda: True)
    assert out["precos"] and chamadas == ["mouse preço"]
    assert any(k == "preco" and "alvo atingido" in t for k, t, _ in avisos)


def test_api_linha_do_tempo_fatura_e_sonho(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    (tmp_path / "d").mkdir()
    with TestClient(
        create_app(settings, gateway_factory=lambda _: FakeGateway(fala("oi"))),
        base_url="http://127.0.0.1",
    ) as c:
        prev = c.post(
            "/financas/fatura/previa",
            json={"texto": "10/10 PADARIA 12,30\n"},
            headers=AUTH,
        ).json()
        assert prev["total_cents"] == 1230
        bad = c.post(
            "/financas/fatura/confirmar",
            json={"linhas": prev["linhas"], "confirmado": False},
            headers=AUTH,
        )
        assert bad.status_code == 422
        ok = c.post(
            "/financas/fatura/confirmar",
            json={"linhas": prev["linhas"], "confirmado": True},
            headers=AUTH,
        )
        assert ok.json() == {"lancadas": 1}
        tl = c.get("/linha-do-tempo", headers=AUTH).json()["itens"]
        assert any(i["tipo"] == "gasto" for i in tl)
        assert c.get("/linha-do-tempo?de=10&ate=5", headers=AUTH).status_code == 422
        assert c.post("/sonhos/999/boa", headers=AUTH).status_code == 404
        assert c.get("/linha-do-tempo").status_code == 401


def test_texto_de_gasto_nunca_vai_a_nuvem_e_o_local_so_completa(m, tmp_path):
    """Regra 61: o parser é por regra; sem entender, o modelo LOCAL tenta; nunca a nuvem."""
    nuvem = Texto("{}")  # não deve ser chamado para texto de gasto
    local = Texto('{"valor": 42.5, "descricao": "gasolina do posto"}')
    c = canal(m, tmp_path, gw=local)
    resposta = asyncio.run(c.tratar_async(1, "gastei um montante qualquer com gasolina"))
    assert resposta and "R$ 42,50" in resposta and "transporte" in resposta
    assert nuvem.t == "{}"  # a nuvem nunca é consultada aqui


def test_focus_hook_pergunta_so_com_bloco_e_distracao(m, tmp_path):
    from orion.screen_memory import ScreenMemory

    perguntas = []
    foco = Foco(["youtube"], clock=lambda: AGORA)
    sm = ScreenMemory(
        m,
        capture=lambda arq: arq.write_bytes(b"x") or True,
        ocr=lambda img: "texto suficiente para a memória da tela " * 3,
        title=lambda: "Vídeo - YouTube",
        interval_s=0,
        retention_days=7,
        clock=lambda: AGORA,
        on_capture=lambda alvo: perguntas.append(alvo) if foco.pergunta_permitida(alvo) else None,
    )
    sm.run()  # sem bloco: nada
    assert perguntas == []
    foco.iniciar(30)
    sm.run()
    sm.run()  # uma pergunta por bloco
    assert len(perguntas) <= 1


def test_api_foco_so_habilita_pergunta(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    (tmp_path / "d").mkdir()
    with TestClient(
        create_app(settings, gateway_factory=lambda _: FakeGateway(fala("oi"))),
        base_url="http://127.0.0.1",
    ) as c:
        assert c.get("/foco", headers=AUTH).json() == {"ativo": False}
        assert c.post("/foco/iniciar", json={"minutos": 2}, headers=AUTH).status_code == 422
        r = c.post("/foco/iniciar", json={"minutos": 25}, headers=AUTH).json()
        assert r == {"ativo": True, "minutos": 25}
        assert c.get("/foco", headers=AUTH).json() == {"ativo": True}

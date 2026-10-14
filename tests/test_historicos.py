"""E9.6/E9.7 (REGRAS.md: históricos locais: opt-in, só texto, só neste computador): histórico da área de transferência e dos comandos, só local e opt-in."""

import httpx
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.historicos import Historicos, analisar, ferramentas, parece_segredo
from orion.memory import MemoryStore
from orion.policy.classes import Risk
from tests.fakes import FakeGateway, fala
from tests.test_ponte import montar

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
DIA = 86400.0
AGORA = 1_800_000_000.0


def hist(tmp_path, relogio=None):
    m = MemoryStore(tmp_path / "h.db")
    t = [AGORA]
    return Historicos(m, clock=relogio or (lambda: t[0])), m, t


def test_area_filtra_segredo_repetido_e_curto(tmp_path):
    h, m, _ = hist(tmp_path)
    assert h.guardar_area("ab") == "curto"
    assert h.guardar_area("minha senha é 12345") == "segredo"
    assert h.guardar_area("token: abc123") == "segredo"
    assert h.guardar_area("CPF 123.456.789-09") == "segredo"
    assert h.guardar_area("4111 1111 1111 1111") == "segredo"
    assert h.guardar_area("x" * 5000) == "longo"
    assert h.guardar_area("receita de bolo de cenoura") == "guardado"
    assert h.guardar_area("receita de bolo de cenoura") == "repetido"
    assert [x["texto"] for x in h.recentes_area()] == ["receita de bolo de cenoura"]
    m.close()


def test_area_busca_poda_e_limpa(tmp_path):
    h, m, t = hist(tmp_path)
    h.guardar_area("endereço do cliente: rua das flores 10")
    t[0] += 8 * DIA
    h.guardar_area("receita de bolo de cenoura")
    assert [x["texto"] for x in h.buscar_area("bolo", dias=7)] == ["receita de bolo de cenoura"]
    assert h.buscar_area("flores", dias=7) == []  # fora da janela de 7 dias
    assert len(h.buscar_area("flores", dias=30)) == 1
    assert h.podar_area(7) == 1 and len(h.recentes_area()) == 1
    assert h.limpar_area() == 1 and h.recentes_area() == []
    assert h.buscar_area("") == []
    m.close()


def test_comandos_descartam_segredos():
    for ruim in (
        "mysql -u root -p hunter2",
        "curl --token abc123 https://x.test",
        "git clone https://user:segredo@github.com/a/b.git",
        "export API_KEY=abc123",
        "$env:SENHA = 'x'",
        "gbak -user SYSDBA -password masterkey a.fdb b.fbk",
    ):
        assert parece_segredo(ruim), ruim
    for bom in ("gbak -b -v -user SYSDBA banco.fdb banco.fbk", "git status", "ls -la"):
        assert not parece_segredo(bom), bom


def test_formatos_de_historico():
    assert analisar("zsh", [": 1700000000:0;git status", ": 1700000100:0;ls"], 5.0) == [
        (1700000000.0, "git status"),
        (1700000100.0, "ls"),
    ]
    assert analisar("bash", ["#1700000000", "make test", "", "ls"], 5.0) == [
        (1700000000.0, "make test"),
        (1700000000.0, "ls"),
    ]
    assert analisar("powershell", ["Get-Process", "  ", "dir"], 7.0) == [
        (7.0, "Get-Process"),
        (7.0, "dir"),
    ]


def test_importa_so_o_novo_e_acha_o_gbak_de_ontem(tmp_path):
    h, m, _t = hist(tmp_path)
    home = tmp_path / "home"
    home.mkdir()
    arq = home / ".bash_history"
    ontem = AGORA - DIA
    arq.write_text(
        f"#{int(ontem)}\ngbak -b -v banco.fdb banco.fbk\n#{int(ontem) + 5}\nmysql -p hunter2\n",
        encoding="utf-8",
    )
    assert h.importar(home=home, appdata=tmp_path / "nada") == 1  # o segredo ficou de fora
    assert h.importar(home=home, appdata=tmp_path / "nada") == 0  # nada novo: não duplica
    with arq.open("a", encoding="utf-8") as f:
        f.write(f"#{int(AGORA)}\ngit push\n")
    assert h.importar(home=home, appdata=tmp_path / "nada") == 1
    [achado] = h.buscar_comandos("gbak")
    assert achado["comando"].startswith("gbak -b") and achado["ts"] == float(int(ontem))
    assert h.buscar_comandos("hunter2") == []
    arq.write_text("ls\n", encoding="utf-8")  # arquivo trocado: começa de novo
    assert h.importar(home=home, appdata=tmp_path / "nada") == 1
    assert h.limpar_comandos() >= 2 and h.buscar_comandos("gbak") == []
    m.close()


def test_ferramentas_sao_leitura_externa_e_so_existem_ligadas(tmp_path):
    h, m, _ = hist(tmp_path)
    specs = {}
    assert ferramentas(h, specs, area=False, comandos=False) == []
    tools = ferramentas(h, specs, area=True, comandos=True)
    assert {t.name for t in tools} == {"buscar_area_transferencia", "buscar_comandos"}
    assert all(s.risk is Risk.READ and s.external for s in specs.values())
    h.guardar_area("receita de bolo de cenoura")
    t = next(t for t in tools if t.name == "buscar_area_transferencia")
    [r] = t.fn("bolo")["resultados"]
    assert "cenoura" in r["texto"]
    m.close()


def settings(tmp_path, **kw):
    (tmp_path / "d").mkdir(exist_ok=True)
    return Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None, **kw
    )


def cliente(tmp_path, **kw):
    gw = FakeGateway(fala("oi"))
    return TestClient(
        create_app(settings(tmp_path, **kw), gateway_factory=lambda _: gw),
        base_url="http://127.0.0.1",
    )


def test_api_desligada_por_padrao(tmp_path):
    with cliente(tmp_path) as c:
        assert (
            c.post("/area-transferencia", json={"texto": "oi tudo bem"}, headers=AUTH).status_code
            == 404
        )
        assert c.get("/comandos", headers=AUTH).status_code == 404
        assert c.get("/busca?q=bolo", headers=AUTH).status_code == 200


def test_api_ligada_guarda_busca_e_apaga(tmp_path):
    with cliente(tmp_path, clipboard_days=7, command_history=True) as c:
        r = c.post(
            "/area-transferencia", json={"texto": "receita de bolo de cenoura"}, headers=AUTH
        )
        assert r.json() == {"resultado": "guardado"}
        assert c.post("/area-transferencia", json={"texto": "senha: 123"}, headers=AUTH).json() == {
            "resultado": "segredo"
        }
        itens = c.get("/area-transferencia?q=bolo", headers=AUTH).json()["itens"]
        assert len(itens) == 1
        grupos = {g["fonte"] for g in c.get("/busca?q=bolo cenoura", headers=AUTH).json()["grupos"]}
        assert "area" in grupos
        c.app.state.orion.historicos.guardar_comando("bash", AGORA, "gbak -b banco.fdb banco.fbk")
        assert c.get("/comandos?q=gbak", headers=AUTH).json()["itens"][0]["shell"] == "bash"
        grupos = {g["fonte"] for g in c.get("/busca?q=gbak banco", headers=AUTH).json()["grupos"]}
        assert "comandos" in grupos
        assert c.delete("/area-transferencia", headers=AUTH).json() == {"apagados": 1}
        assert c.get("/area-transferencia", headers=AUTH).json()["itens"] == []
        c.post("/modo/panico", json={"ativo": True}, headers=AUTH)
        assert (
            c.post(
                "/area-transferencia", json={"texto": "outro texto qualquer"}, headers=AUTH
            ).status_code
            == 409
        )


# ── ponte: observa a área, sem tratar a própria colagem como cópia ─────────────────────────────
def test_ponte_envia_texto_novo_uma_vez_e_ignora_a_propria_colagem():
    enviados = []

    def handler(req):
        enviados.append(req.content)
        return httpx.Response(200, json={"resultado": "guardado"})

    ponte, _, a = montar(handler)
    ponte._observar_area = True
    a.area.conteudo = "primeiro texto copiado"
    ultimo = ponte.vigiar_area_uma_vez(None)
    assert ultimo == "primeiro texto copiado" and len(enviados) == 1
    assert ponte.vigiar_area_uma_vez(ultimo) == ultimo and len(enviados) == 1  # igual: não reenvia
    ponte.colar("texto aprovado do modelo")  # a ponte põe e devolve: nada disso é "cópia"
    a.area.conteudo = "texto aprovado do modelo"
    ultimo = ponte.vigiar_area_uma_vez(ultimo)
    assert len(enviados) == 1
    a.area.conteudo = ""
    assert ponte.vigiar_area_uma_vez(ultimo) == ultimo


def test_ponte_para_de_observar_quando_o_servidor_responde_404():
    ponte, _, a = montar(lambda req: httpx.Response(404, json={"detail": "desligado"}))
    ponte._observar_area = True
    a.area.conteudo = "alguma coisa copiada"
    ponte.vigiar_area_uma_vez(None)
    assert ponte._observar_area is False


def test_ponte_so_registra_a_tecla_da_area_quando_observa():
    ponte, _, _ = montar()
    assert not any("v" == k[-1] for k in ponte.acoes_das_teclas())
    ponte._observar_area = True
    assert "<ctrl>+<alt>+v" in ponte.acoes_das_teclas()
    ponte.abrir_area()

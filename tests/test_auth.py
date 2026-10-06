"""Login (fase 5): senha com PBKDF2, sessão por cookie opaco, bloqueio e CSRF."""

import sqlite3

import pytest
from fastapi.testclient import TestClient

from orion.app import COOKIE_SESSAO, create_app
from orion.auth import (
    AuthService,
    BadCredentials,
    LockedOut,
    LoginThrottle,
    NotConfigured,
    WeakPassword,
    hash_password,
    verify_password,
)
from orion.config import Settings

SENHA = "uma-senha-bem-longa-123"
TOKEN = "token-de-maquina-16+chars"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
IT = 1000  # iterações baixas: nos testes o custo do PBKDF2 não importa


class Relogio:
    def __init__(self, t=1000.0):
        self.t = t

    def __call__(self):
        return self.t


@pytest.fixture
def relogio():
    return Relogio()


@pytest.fixture
def auth(tmp_path, relogio):
    a = AuthService(
        tmp_path / "auth.db",
        iteracoes=IT,
        clock=relogio,
        throttle=LoginThrottle(clock=relogio),
    )
    yield a
    a.close()


# ── hash ──────────────────────────────────────────────────────────────────────
def test_hash_tem_sal_proprio_e_so_confere_a_senha_certa():
    a, b = hash_password("senha-correta-123", IT), hash_password("senha-correta-123", IT)
    assert a != b  # sal diferente
    assert a.startswith("pbkdf2_sha256$1000$")
    assert verify_password("senha-correta-123", a) and not verify_password("outra-senha-123", a)


@pytest.mark.parametrize("lixo", ["", "x", "pbkdf2_sha256$1000$a$b", "md5$1$a$b", "a$b$c$d$e"])
def test_hash_corrompido_nunca_confere(lixo):
    assert verify_password("qualquer", lixo) is False


# ── serviço ───────────────────────────────────────────────────────────────────
def test_sem_senha_o_login_diz_que_nao_esta_configurado(auth):
    assert auth.has_password() is False
    with pytest.raises(NotConfigured):
        auth.login(SENHA)


@pytest.mark.parametrize("fraca", ["curta", "x" * 11, "antonio" * 40])
def test_senha_fraca_e_recusada(auth, fraca):
    with pytest.raises(WeakPassword):
        auth.set_password(fraca)
    assert auth.has_password() is False


def test_senha_igual_ao_usuario_e_recusada(tmp_path):
    a = AuthService(tmp_path / "a.db", user="antonio-salomao", iteracoes=IT)
    with pytest.raises(WeakPassword):
        a.set_password("Antonio-Salomao")
    a.close()


def test_login_valida_logout_revoga(auth):
    auth.set_password(SENHA)
    token = auth.login(SENHA, "1.2.3.4")
    assert auth.validate(token) == "admin"
    assert auth.validate(token + "x") is None and auth.validate(None) is None
    auth.logout(token)
    assert auth.validate(token) is None


def test_o_banco_guarda_so_o_hash_do_token_e_da_senha(auth, tmp_path):
    auth.set_password(SENHA)
    token = auth.login(SENHA)
    bruto = sqlite3.connect(tmp_path / "auth.db")
    tudo = "".join(
        str(linha)
        for t in ("credential", "sessions")
        for linha in bruto.execute(f"SELECT * FROM {t}")
    )
    bruto.close()
    assert token not in tudo and SENHA not in tudo


def test_sessao_expira(auth, relogio):
    auth.set_password(SENHA)
    token = auth.login(SENHA)
    relogio.t += 7 * 86400 - 1
    assert auth.validate(token) == "admin"
    relogio.t += 2
    assert auth.validate(token) is None


def test_trocar_a_senha_derruba_todas_as_sessoes(auth):
    auth.set_password(SENHA)
    t1, t2 = auth.login(SENHA), auth.login(SENHA)
    assert auth.active_sessions() == 2
    auth.set_password("outra-senha-ainda-maior")
    assert auth.validate(t1) is None and auth.validate(t2) is None
    with pytest.raises(BadCredentials):
        auth.login(SENHA)
    assert auth.validate(auth.login("outra-senha-ainda-maior")) == "admin"


def test_senha_errada_trava_o_cliente_com_tempo_crescente(auth, relogio):
    auth.set_password(SENHA)
    for _ in range(5):
        with pytest.raises(BadCredentials):
            auth.login("errada-errada-errada", "9.9.9.9")
    with pytest.raises(LockedOut) as e:  # a senha CERTA também fica travada durante a espera
        auth.login(SENHA, "9.9.9.9")
    assert 1 <= e.value.retry_after <= 60
    assert auth.validate(auth.login(SENHA, "outro-cliente")) == "admin"  # outro cliente segue

    relogio.t += 61
    for _ in range(5):  # segunda rodada de erros: a trava dobra
        with pytest.raises(BadCredentials):
            auth.login("errada-errada-errada", "9.9.9.9")
    with pytest.raises(LockedOut) as e:
        auth.login(SENHA, "9.9.9.9")
    assert 60 < e.value.retry_after <= 120

    relogio.t += 121
    assert auth.validate(auth.login(SENHA, "9.9.9.9")) == "admin"  # destravou e zerou


def test_teto_global_barra_quem_troca_de_ip(auth):
    auth.set_password(SENHA)
    for i in range(30):
        with pytest.raises((BadCredentials, LockedOut)):
            auth.login("errada-errada-errada", f"10.0.0.{i}")
    with pytest.raises(LockedOut):
        auth.login(SENHA, "10.0.0.200")


# ── rotas ─────────────────────────────────────────────────────────────────────
@pytest.fixture
def settings(tmp_path):
    return Settings(data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None)


@pytest.fixture
def client(settings):
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        c.app.state.orion.auth.set_password(SENHA)
        yield c


def entrar(c, senha=SENHA, **kw):
    return c.post("/auth/login", json={"senha": senha}, **kw)


def test_api_sem_credencial_da_401_e_o_status_publico_informa(client):
    assert client.get("/approvals").status_code == 401
    assert client.get("/auth/status").json() == {
        "configured": True,
        "authenticated": False,
        "token_auth": True,
    }


def test_login_abre_sessao_com_cookie_endurecido(client):
    r = entrar(client)
    assert r.status_code == 200
    cookie = r.headers["set-cookie"].lower()
    assert COOKIE_SESSAO in cookie
    assert "httponly" in cookie and "samesite=strict" in cookie and "path=/" in cookie
    assert "secure" not in cookie  # http local: o navegador descartaria o cookie
    assert client.get("/approvals").json() == []
    assert client.get("/auth/status").json()["authenticated"] is True


def test_cookie_ganha_secure_em_https_ou_por_configuracao(settings):
    with TestClient(create_app(settings), base_url="https://127.0.0.1") as c:
        c.app.state.orion.auth.set_password(SENHA)
        assert "secure" in entrar(c).headers["set-cookie"].lower()
    sec = settings.model_copy(update={"cookie_secure": True})
    with TestClient(create_app(sec), base_url="http://127.0.0.1") as c:
        c.app.state.orion.auth.set_password(SENHA)
        assert "secure" in entrar(c).headers["set-cookie"].lower()


def test_senha_errada_da_401_sem_dizer_o_que_faltou(client):
    r = entrar(client, "senha-errada-qualquer")
    assert r.status_code == 401 and r.json()["detail"] == "usuário ou senha incorretos"
    assert COOKIE_SESSAO not in r.headers.get("set-cookie", "")


def test_usuario_errado_falha_igual_a_senha_errada_e_conta_para_o_bloqueio(client):
    r = client.post("/auth/login", json={"usuario": "intruso", "senha": SENHA})
    assert r.status_code == 401 and r.json()["detail"] == "usuário ou senha incorretos"
    assert COOKIE_SESSAO not in r.headers.get("set-cookie", "")
    # o usuário certo (sem diferenciar maiúsculas) entra; sem usuário vale o único
    ok = client.post("/auth/login", json={"usuario": " Admin ", "senha": SENHA})
    assert ok.status_code == 200
    assert client.post("/auth/login", json={"senha": SENHA}).status_code == 200
    for _ in range(5):  # chutar usuário também esgota as tentativas
        client.post("/auth/login", json={"usuario": "x", "senha": SENHA})
    assert client.post("/auth/login", json={"senha": SENHA}).status_code == 429


def test_bloqueio_vira_429_com_retry_after(client):
    for _ in range(5):
        assert entrar(client, "senha-errada-qualquer").status_code == 401
    r = entrar(client)
    assert r.status_code == 429 and int(r.headers["retry-after"]) >= 1


def test_sem_senha_definida_o_login_da_503(tmp_path):
    s = Settings(data_dir=tmp_path / "x", admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        assert entrar(c).status_code == 503
        assert c.get("/auth/status").json()["configured"] is False


def test_logout_encerra_a_sessao(client):
    entrar(client)
    assert client.post("/auth/logout").status_code == 200
    assert client.get("/approvals").status_code == 401


def test_token_de_admin_continua_valendo_e_cabecalho_errado_nao_cai_no_cookie(client):
    assert client.get("/approvals", headers=AUTH).status_code == 200
    entrar(client)  # tem cookie válido...
    errado = {"Authorization": "Bearer token-errado-errado"}
    assert client.get("/approvals", headers=errado).status_code == 401  # ...mas o erro vale


def test_so_com_senha_o_token_de_admin_nao_existe(tmp_path):
    s = Settings(data_dir=tmp_path / "y", jobs_enabled=False, _env_file=None)
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        c.app.state.orion.auth.set_password(SENHA)
        assert c.get("/approvals", headers=AUTH).status_code == 401
        assert entrar(c).status_code == 200
        assert c.get("/approvals").status_code == 200


def test_csrf_post_com_cookie_de_outra_origem_e_recusado(client):
    entrar(client)
    corpo = {"approved": True}
    rotas = "/approvals/x/decide"
    assert (
        client.post(rotas, json=corpo, headers={"Origin": "http://evil.example"}).status_code == 403
    )
    assert (
        client.post(rotas, json=corpo, headers={"Sec-Fetch-Site": "cross-site"}).status_code == 403
    )
    ok = client.post(rotas, json=corpo, headers={"Origin": "http://127.0.0.1"})
    assert ok.status_code == 404  # passou pela auth; a aprovação é que não existe
    assert client.post(rotas, json=corpo).status_code == 404  # cliente sem Origin (curl, app)
    # o token de admin não é cookie: não sofre CSRF e não precisa de Origin
    assert (
        client.post(
            rotas, json=corpo, headers={**AUTH, "Origin": "http://evil.example"}
        ).status_code
        == 404
    )


def test_login_de_outra_origem_e_recusado(client):
    assert entrar(client, headers={"Origin": "http://evil.example"}).status_code == 403


def test_health_sem_login_so_diz_o_minimo(client):
    assert set(client.get("/health").json()) == {"status", "version"}
    entrar(client)
    assert "components" in client.get("/health").json()


def test_a_documentacao_da_api_nao_e_publica(client):
    for caminho in ("/docs", "/redoc", "/openapi.json"):
        assert client.get(caminho).status_code == 404


def test_trocar_a_senha_pede_a_atual_e_mantem_quem_trocou_logado(client):
    entrar(client)
    nova = "senha-nova-bem-comprida-9"
    assert (
        client.post("/auth/password", json={"senha_atual": "errada", "nova": nova}).status_code
        == 401
    )
    assert (
        client.post("/auth/password", json={"senha_atual": SENHA, "nova": "curta"}).status_code
        == 422
    )
    assert (
        client.post("/auth/password", json={"senha_atual": SENHA, "nova": nova}).status_code == 200
    )
    assert client.get("/approvals").status_code == 200  # a sessão de quem trocou foi renovada
    outro = TestClient(client.app, base_url="http://127.0.0.1")
    assert entrar(outro, SENHA).status_code == 401 and entrar(outro, nova).status_code == 200


def test_o_token_de_admin_define_a_primeira_senha(tmp_path):
    s = Settings(data_dir=tmp_path / "z", admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        r = c.post("/auth/password", headers=AUTH, json={"nova": SENHA})
        assert r.status_code == 200
        assert entrar(c).status_code == 200


def test_auth_db_fica_fora_do_banco_da_memoria_e_do_backup(client, settings):
    assert settings.auth_db_path.exists() and settings.auth_db_path != settings.db_path
    store = client.app.state.orion.memory
    destino = settings.data_dir / "bk.db"
    store.backup_to(destino)
    tabelas = {r[0] for r in sqlite3.connect(destino).execute("SELECT name FROM sqlite_master")}
    assert "credential" not in tabelas


# ── CLI ───────────────────────────────────────────────────────────────────────
def test_cli_set_password_grava_e_recusa_senha_fraca(tmp_path, monkeypatch, capsys):
    import io

    from orion.__main__ import main

    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "dados"))
    monkeypatch.setattr("sys.stdin", io.StringIO("curta\n"))
    assert main(["set-password", "--stdin"]) == 1
    assert "pelo menos" in capsys.readouterr().err

    monkeypatch.setattr("sys.stdin", io.StringIO(SENHA + "\n"))
    assert main(["set-password", "--stdin"]) == 0
    a = AuthService(tmp_path / "dados" / "auth.db")
    assert a.check_password(SENHA) and not a.check_password("outra-senha-qualquer")
    a.close()


# ── senha de fábrica (só o .exe liga; ver orion/auth.py) ──────────────────────
def test_seed_cria_admin_com_senha_de_fabrica_uma_vez_e_nunca_sobrescreve(auth):
    from orion.auth import SENHA_PADRAO, USUARIO_PADRAO

    assert (auth.user, SENHA_PADRAO, USUARIO_PADRAO) == ("admin", "261210@", "admin")
    assert auth.has_password() is False and auth.uses_default_password() is False
    assert auth.seed_default() is True
    assert auth.has_password() and auth.uses_default_password()
    assert auth.validate(auth.login(SENHA_PADRAO, usuario="admin")) == "admin"
    assert auth.seed_default() is False  # já há senha
    auth.set_password(SENHA)  # trocar tira a marca de fábrica e vale a nova
    assert auth.uses_default_password() is False
    assert auth.seed_default() is False
    with pytest.raises(BadCredentials):
        auth.login(SENHA_PADRAO)


def test_auth_db_antigo_sem_a_coluna_sobe_sozinho(tmp_path):
    import sqlite3

    caminho = tmp_path / "velho.db"
    c = sqlite3.connect(caminho)
    c.executescript(
        "CREATE TABLE credential (user TEXT PRIMARY KEY, pw_hash TEXT NOT NULL, updated_at REAL NOT NULL);"
        "CREATE TABLE sessions (token_hash TEXT PRIMARY KEY, user TEXT NOT NULL, created_at REAL NOT NULL, expires_at REAL NOT NULL);"
    )
    c.commit()
    c.close()
    a = AuthService(caminho, iteracoes=IT)
    a.set_password(SENHA)
    assert a.has_password() and not a.uses_default_password()
    a.close()


def test_app_com_seed_deixa_entrar_com_admin_e_so_diz_que_e_de_fabrica_depois_do_login(tmp_path):
    s = Settings(
        data_dir=tmp_path / "f", seed_default_password=True, jobs_enabled=False, _env_file=None
    )
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        antes = c.get("/auth/status").json()
        assert antes == {
            "configured": True,
            "authenticated": False,
            "token_auth": False,
        }  # não anuncia
        assert (
            c.post("/auth/login", json={"usuario": "admin", "senha": "261210@"}).status_code == 200
        )
        assert c.get("/auth/status").json()["default_password"] is True
        r = c.post("/auth/password", json={"senha_atual": "261210@", "nova": SENHA})
        assert r.status_code == 200
        assert c.get("/auth/status").json()["default_password"] is False


def test_sem_o_seed_nao_ha_senha_de_fabrica(tmp_path):
    s = Settings(data_dir=tmp_path / "g", jobs_enabled=False, _env_file=None)
    assert s.seed_default_password is False and s.auth_user == "admin"
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        assert c.get("/auth/status").json()["configured"] is False
        assert (
            c.post("/auth/login", json={"usuario": "admin", "senha": "261210@"}).status_code == 503
        )


def test_host_de_fora_com_senha_de_fabrica_ainda_ativa_recusa_subir(tmp_path):
    tailnet = ["127.0.0.1", "localhost", "orion.tail1234.ts.net"]
    s = Settings(
        data_dir=tmp_path / "h", seed_default_password=True, allowed_hosts=tailnet,
        admin_token=TOKEN, jobs_enabled=False, _env_file=None,
    )  # fmt: skip
    with pytest.raises(RuntimeError, match="senha de fábrica"):
        with TestClient(create_app(s), base_url="http://orion.tail1234.ts.net"):
            pass
    # trocando a senha antes, sobe
    AuthService(tmp_path / "h" / "auth.db").set_password(SENHA)
    with TestClient(create_app(s), base_url="http://orion.tail1234.ts.net") as c:
        assert c.get("/health").status_code == 200

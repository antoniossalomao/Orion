//! Paridade com `tests/test_auth.py` (Python): senha com PBKDF2, sessão opaca, bloqueio por
//! cliente, senha de fábrica e migração do `auth.db` antigo.
//!
//! Fora daqui: as rotas (`/auth/login`, cookie, CSRF, 429, `/auth/password`) e a recusa de subir
//! com host de fora e senha de fábrica, que são do `orion-server`; e a segunda rodada do
//! bloqueio (a trava dobra), porque o `LoginThrottle` do Rust mede em tempo real e a trava mínima
//! é de 60 s, sem relógio injetável.

use orion_core::auth::{hash_password, verify_password, AuthError, AuthService, SENHA_PADRAO, USUARIO_PADRAO};
use std::path::Path;

const SENHA: &str = "uma-senha-bem-longa-123";
const IT: u32 = 1000; // iterações baixas: nos testes o custo do PBKDF2 não importa

fn auth(d: &Path) -> AuthService {
    AuthService::open_with(&d.join("auth.db"), "admin", 7.0 * 86400.0, IT).unwrap()
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn hash_tem_sal_proprio_e_so_confere_a_senha_certa() {
    let (a, b) = (hash_password("senha-correta-123", IT), hash_password("senha-correta-123", IT));
    assert_ne!(a, b); // sal diferente
    assert!(a.starts_with("pbkdf2_sha256$1000$"));
    assert!(verify_password("senha-correta-123", &a) && !verify_password("outra-senha-123", &a));
}

#[test]
fn hash_corrompido_nunca_confere() {
    for lixo in ["", "x", "pbkdf2_sha256$1000$a$b", "md5$1$a$b", "a$b$c$d$e"] {
        assert!(!verify_password("qualquer", lixo), "{lixo:?}");
    }
}

#[test]
fn sem_senha_o_login_diz_que_nao_esta_configurado() {
    let d = tmp();
    let a = auth(d.path());
    assert!(!a.has_password());
    assert!(matches!(a.login(SENHA, "c", None), Err(AuthError::NotConfigured(_))));
}

#[test]
fn senha_fraca_e_recusada() {
    let d = tmp();
    let a = auth(d.path());
    for fraca in ["curta".to_string(), "x".repeat(11), "antonio".repeat(40)] {
        assert!(matches!(a.set_password(&fraca), Err(AuthError::WeakPassword(_))), "{fraca}");
    }
    assert!(!a.has_password());
}

#[test]
fn senha_igual_ao_usuario_e_recusada() {
    let d = tmp();
    let a = AuthService::open_with(&d.path().join("a.db"), "antonio-salomao", 3600.0, IT).unwrap();
    assert!(matches!(a.set_password("Antonio-Salomao"), Err(AuthError::WeakPassword(_))));
}

#[test]
fn login_valida_logout_revoga() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password(SENHA).unwrap();
    let token = a.login(SENHA, "1.2.3.4", None).unwrap();
    assert_eq!(a.validate(Some(&token)).as_deref(), Some("admin"));
    assert!(a.validate(Some(&format!("{token}x"))).is_none() && a.validate(None).is_none());
    a.logout(Some(&token));
    assert!(a.validate(Some(&token)).is_none());
}

#[test]
fn o_banco_guarda_so_o_hash_do_token_e_da_senha() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password(SENHA).unwrap();
    let token = a.login(SENHA, "c", None).unwrap();
    let bruto = rusqlite::Connection::open(d.path().join("auth.db")).unwrap();
    let mut tudo = String::new();
    for t in ["credential", "sessions"] {
        let mut st = bruto.prepare(&format!("SELECT * FROM {t}")).unwrap();
        let n = st.column_count();
        let mut rows = st.query([]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            for i in 0..n {
                tudo += &format!("{:?}", r.get_ref(i).unwrap());
            }
        }
    }
    assert!(!tudo.contains(&token) && !tudo.contains(SENHA));
}

#[test]
fn sessao_expira() {
    // o Python avança um relógio de mentira; aqui o prazo da sessão é curto de verdade
    let d = tmp();
    let a = AuthService::open_with(&d.path().join("auth.db"), "admin", 1.5, IT).unwrap();
    a.set_password(SENHA).unwrap();
    let token = a.login(SENHA, "c", None).unwrap();
    assert_eq!(a.validate(Some(&token)).as_deref(), Some("admin"));
    std::thread::sleep(std::time::Duration::from_millis(1700));
    assert!(a.validate(Some(&token)).is_none());
}

#[test]
fn trocar_a_senha_derruba_todas_as_sessoes() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password(SENHA).unwrap();
    let (t1, t2) = (a.login(SENHA, "c", None).unwrap(), a.login(SENHA, "c", None).unwrap());
    assert_eq!(a.active_sessions(), 2);
    a.set_password("outra-senha-ainda-maior").unwrap();
    assert!(a.validate(Some(&t1)).is_none() && a.validate(Some(&t2)).is_none());
    assert!(matches!(a.login(SENHA, "c", None), Err(AuthError::BadCredentials(_))));
    let novo = a.login("outra-senha-ainda-maior", "c", None).unwrap();
    assert_eq!(a.validate(Some(&novo)).as_deref(), Some("admin"));
}

#[test]
fn senha_errada_trava_o_cliente() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password(SENHA).unwrap();
    for _ in 0..5 {
        assert!(matches!(a.login("errada-errada-errada", "9.9.9.9", None), Err(AuthError::BadCredentials(_))));
    }
    match a.login(SENHA, "9.9.9.9", None) {
        // a senha CERTA também fica travada durante a espera
        Err(AuthError::LockedOut(s)) => assert!((1..=60).contains(&s), "{s}"),
        o => panic!("{o:?}"),
    }
    let outro = a.login(SENHA, "outro-cliente", None).unwrap(); // outro cliente segue
    assert_eq!(a.validate(Some(&outro)).as_deref(), Some("admin"));
}

#[test]
fn usuario_errado_falha_igual_a_senha_errada_e_conta_para_o_bloqueio() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password(SENHA).unwrap();
    let e = a.login(SENHA, "c", Some("intruso")).unwrap_err();
    assert_eq!(e.to_string(), "usuário ou senha incorretos");
    // o usuário certo (sem diferenciar maiúsculas) entra; sem usuário vale o único
    assert!(a.login(SENHA, "c", Some(" Admin ")).is_ok() && a.login(SENHA, "c", None).is_ok());
    for _ in 0..5 {
        let _ = a.login(SENHA, "d", Some("x")); // chutar usuário também esgota as tentativas
    }
    assert!(matches!(a.login(SENHA, "d", None), Err(AuthError::LockedOut(_))));
}

#[test]
fn teto_global_barra_quem_troca_de_ip() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password(SENHA).unwrap();
    for i in 0..30 {
        assert!(a.login("errada-errada-errada", &format!("10.0.0.{i}"), None).is_err());
    }
    assert!(matches!(a.login(SENHA, "10.0.0.200", None), Err(AuthError::LockedOut(_))));
}

#[test]
fn seed_cria_admin_com_senha_de_fabrica_uma_vez_e_nunca_sobrescreve() {
    let d = tmp();
    let a = auth(d.path());
    assert_eq!((a.user.as_str(), SENHA_PADRAO, USUARIO_PADRAO), ("admin", "261210@", "admin"));
    assert!(!a.has_password() && !a.uses_default_password());
    assert!(a.seed_default().unwrap());
    assert!(a.has_password() && a.uses_default_password());
    let t = a.login(SENHA_PADRAO, "c", Some("admin")).unwrap();
    assert_eq!(a.validate(Some(&t)).as_deref(), Some("admin"));
    assert!(!a.seed_default().unwrap()); // já há senha
    a.set_password(SENHA).unwrap(); // trocar tira a marca de fábrica e vale a nova
    assert!(!a.uses_default_password() && !a.seed_default().unwrap());
    assert!(matches!(a.login(SENHA_PADRAO, "c2", None), Err(AuthError::BadCredentials(_))));
}

#[test]
fn auth_db_antigo_sem_a_coluna_sobe_sozinho() {
    let d = tmp();
    let caminho = d.path().join("velho.db");
    let c = rusqlite::Connection::open(&caminho).unwrap();
    c.execute_batch(
        "CREATE TABLE credential (user TEXT PRIMARY KEY, pw_hash TEXT NOT NULL, updated_at REAL NOT NULL);
         CREATE TABLE sessions (token_hash TEXT PRIMARY KEY, user TEXT NOT NULL, created_at REAL NOT NULL, expires_at REAL NOT NULL);",
    )
    .unwrap();
    drop(c);
    let a = AuthService::open_with(&caminho, "admin", 3600.0, IT).unwrap();
    a.set_password(SENHA).unwrap();
    assert!(a.has_password() && !a.uses_default_password());
}

#[test]
fn check_password_nao_cria_sessao_nem_conta_tentativa() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password(SENHA).unwrap();
    for _ in 0..10 {
        assert!(!a.check_password("errada-errada-errada"));
    }
    assert!(a.check_password(SENHA) && a.active_sessions() == 0);
    assert!(a.login(SENHA, "c", None).is_ok()); // as conferências não travaram ninguém
}

#[test]
fn tokens_de_dispositivo_sao_revogados_ao_trocar_a_senha_e_ao_parear_de_novo() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password(SENHA).unwrap();
    let t1 = a.create_device_token("ponte", "PC").unwrap();
    assert_eq!(a.device_scope(Some(&t1)).as_deref(), Some("ponte"));
    let t2 = a.create_device_token("ponte", "PC novo").unwrap(); // parear de novo revoga o anterior
    assert!(a.device_scope(Some(&t1)).is_none() && a.device_scope(Some(&t2)).is_some());
    a.set_password("outra-senha-ainda-maior").unwrap(); // trocar a senha desfaz o pareamento
    assert!(a.device_scope(Some(&t2)).is_none());
}

#[test]
fn senha_nova_e_gravada_em_argon2id_e_confere() {
    let d = tmp();
    let a = auth(d.path());
    a.set_password("senha-correta-123").unwrap();
    let hash: String = rusqlite::Connection::open(d.path().join("auth.db"))
        .unwrap()
        .query_row("SELECT pw_hash FROM credential", [], |r| r.get(0))
        .unwrap();
    assert!(hash.starts_with("$argon2id$"), "{hash}");
    assert!(a.check_password("senha-correta-123") && !a.check_password("outra-senha-123"));
}

#[test]
fn hash_pbkdf2_legado_confere_e_vira_argon2id_no_primeiro_login() {
    let d = tmp();
    let a = auth(d.path());
    let legado = hash_password("senha-correta-123", IT);
    let c = rusqlite::Connection::open(d.path().join("auth.db")).unwrap();
    c.execute("INSERT INTO credential(user, pw_hash, updated_at, is_default) VALUES ('admin', ?, 0, 0)", [&legado]).unwrap();
    assert!(!a.check_password("senha-errada-123"));
    let ainda: String = c.query_row("SELECT pw_hash FROM credential", [], |r| r.get(0)).unwrap();
    assert_eq!(ainda, legado, "senha errada não regrava o hash");
    assert!(a.check_password("senha-correta-123"));
    let novo: String = c.query_row("SELECT pw_hash FROM credential", [], |r| r.get(0)).unwrap();
    assert!(novo.starts_with("$argon2id$"));
    assert!(a.check_password("senha-correta-123") && !a.check_password("senha-errada-123"));
}

#[test]
fn argon2id_corrompido_nunca_confere() {
    for lixo in ["$argon2id$", "$argon2id$v=19$m=8,t=1,p=1$x$y", "$argon2i$v=19$m=8,t=1,p=1$c2Fs$aGFzaA"] {
        assert!(!verify_password("qualquer", lixo), "{lixo:?}");
    }
}

#[test]
fn chave_do_banco_depende_da_senha_e_do_sal() {
    use orion_core::auth::derive_db_key;
    let sal = [1u8; 16];
    let a = derive_db_key("senha-correta-123", &sal).unwrap();
    assert_eq!(*a, *derive_db_key("senha-correta-123", &sal).unwrap());
    assert_ne!(*a, *derive_db_key("outra-senha-123", &sal).unwrap());
    assert_ne!(*a, *derive_db_key("senha-correta-123", &[2u8; 16]).unwrap());
    assert!(derive_db_key("senha-correta-123", &[1u8; 8]).is_err());
}

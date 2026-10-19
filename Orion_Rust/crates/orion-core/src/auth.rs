//! Login do Orion: senha com Argon2id e sessão por cookie opaco e revogável
//! (REGRAS.md: login com senha e sessão revogável).
//!
//! O token da sessão é aleatório (256 bits); o banco guarda só o SHA-256 dele. Os dados ficam em
//! `auth.db`, fora do banco da memória: o backup não leva o hash da senha. O formato do hash é o
//! mesmo do Python (`pbkdf2_sha256$iter$sal$hash`, base64 padrão) só para ler hashes antigos: toda
//! senha nova é gravada em Argon2id (PHC) e um hash PBKDF2 correto é regravado em Argon2id no
//! primeiro login. Depois disso o Python de referência deixa de ler o `auth.db`.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use orion_policy::approvals::token_urlsafe;
use parking_lot::Mutex;
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::time::Instant;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const ALGORITMO: &str = "pbkdf2_sha256";
/// Prefixo do hash PHC do Argon2id.
pub const ALGORITMO_ARGON2: &str = "$argon2id$";
/// OWASP 2023 para Argon2id: 19 MiB, 2 passes, 1 faixa.
const ARGON2_MEM_KIB: u32 = 19 * 1024;
const ARGON2_PASSES: u32 = 2;
/// OWASP 2023 para PBKDF2-HMAC-SHA256.
pub const ITERACOES: u32 = 600_000;
pub const SENHA_MIN: usize = 12;
pub const SENHA_MAX: usize = 256;
pub const USUARIO_PADRAO: &str = "admin";
/// Senha de fábrica do executável. Só existe com `ORION_SEED_DEFAULT_PASSWORD`; o app recusa subir
/// exposto na rede enquanto ela não for trocada.
pub const SENHA_PADRAO: &str = "261210@";

const DDL: &str = "
CREATE TABLE IF NOT EXISTS credential (user TEXT PRIMARY KEY, pw_hash TEXT NOT NULL, updated_at REAL NOT NULL, is_default INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS sessions (token_hash TEXT PRIMARY KEY, user TEXT NOT NULL, created_at REAL NOT NULL, expires_at REAL NOT NULL);
CREATE TABLE IF NOT EXISTS device_tokens (token_hash TEXT PRIMARY KEY, scope TEXT NOT NULL, name TEXT NOT NULL, created_at REAL NOT NULL);
";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("{0}")]
    NotConfigured(String),
    #[error("{0}")]
    BadCredentials(String),
    #[error("muitas tentativas; tente de novo em {0}s")]
    LockedOut(u64),
    #[error("{0}")]
    WeakPassword(String),
    #[error("banco de login: {0}")]
    Db(#[from] rusqlite::Error),
}

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn unb64(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

fn pbkdf2(senha: &str, sal: &[u8], it: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(senha.as_bytes(), sal, it, &mut out);
    out
}

pub fn hash_password(senha: &str, it: u32) -> String {
    let mut sal = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut sal);
    format!("{ALGORITMO}${it}${}${}", b64(&sal), b64(&pbkdf2(senha, &sal, it)))
}

/// `custo_pleno` falso só nos testes, para não gastar 19 MiB por hash.
fn argon2(custo_pleno: bool) -> Argon2<'static> {
    let (m, t) = if custo_pleno { (ARGON2_MEM_KIB, ARGON2_PASSES) } else { (8, 1) };
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::new(m, t, 1, None).expect("parâmetros fixos válidos"))
}

/// Hash Argon2id (PHC) com sal aleatório de 128 bits.
pub fn hash_password_argon2id(senha: &str, custo_pleno: bool) -> String {
    let mut sal = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut sal);
    let sal = SaltString::encode_b64(&sal).expect("sal de 16 bytes");
    argon2(custo_pleno).hash_password(senha.as_bytes(), &sal).expect("Argon2id com parâmetros fixos").to_string()
}

/// Confere a senha contra Argon2id (PHC) ou PBKDF2 legado.
pub fn verify_password(senha: &str, armazenado: &str) -> bool {
    if armazenado.starts_with(ALGORITMO_ARGON2) {
        let Ok(h) = PasswordHash::new(armazenado) else { return false };
        return Argon2::default().verify_password(senha.as_bytes(), &h).is_ok();
    }
    let p: Vec<&str> = armazenado.split('$').collect();
    if p.len() != 4 || p[0] != ALGORITMO {
        return false;
    }
    let (Ok(it), Some(sal), Some(esperado)) = (p[1].parse::<u32>(), unb64(p[2]), unb64(p[3])) else { return false };
    let h = pbkdf2(senha, &sal, it);
    h.ct_eq(esperado.as_slice()).into()
}

/// Chave de 256 bits do banco a partir da senha e de um sal próprio (16 bytes ou mais), por
/// Argon2id com o custo pleno. Não é o hash de login: sal e uso são outros, e a chave não é gravada.
pub fn derive_db_key(senha: &str, sal: &[u8]) -> Result<Zeroizing<[u8; 32]>, AuthError> {
    if sal.len() < 16 {
        return Err(AuthError::WeakPassword("sal da chave do banco com menos de 16 bytes".into()));
    }
    let mut key = Zeroizing::new([0u8; 32]);
    argon2(true)
        .hash_password_into(senha.as_bytes(), sal, key.as_mut())
        .map_err(|e| AuthError::WeakPassword(format!("não consegui derivar a chave do banco: {e}")))?;
    Ok(key)
}

/// O hash guardado é o legado (PBKDF2) e deve virar Argon2id no próximo login certo?
pub fn precisa_rehash(armazenado: &str) -> bool {
    !armazenado.starts_with(ALGORITMO_ARGON2)
}

fn digest(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn agora() -> f64 {
    crate::util::agora()
}

/// Trava quem erra a senha: `max_falhas` na janela travam o cliente por `trava_s`, que dobra a
/// cada nova rodada (até `trava_max_s`). Há também um teto global.
pub struct LoginThrottle {
    max: usize,
    janela: f64,
    trava: f64,
    trava_max: f64,
    max_glob: usize,
    inicio: Instant,
    estado: Mutex<Throttle>,
}

#[derive(Default)]
struct Throttle {
    falhas: HashMap<String, VecDeque<f64>>,
    globais: VecDeque<f64>,
    ate: HashMap<String, f64>,
    rodadas: HashMap<String, i32>,
}

impl Default for LoginThrottle {
    fn default() -> Self {
        LoginThrottle { max: 5, janela: 900.0, trava: 60.0, trava_max: 3600.0, max_glob: 30, inicio: Instant::now(), estado: Mutex::new(Throttle::default()) }
    }
}

impl LoginThrottle {
    fn agora(&self) -> f64 {
        self.inicio.elapsed().as_secs_f64()
    }

    fn podar(fila: &mut VecDeque<f64>, agora: f64, janela: f64) {
        while fila.front().is_some_and(|t| agora - t > janela) {
            fila.pop_front();
        }
    }

    /// Segundos até poder tentar de novo (0 = liberado).
    pub fn espera(&self, cliente: &str) -> u64 {
        let agora = self.agora();
        let mut e = self.estado.lock();
        Self::podar(&mut e.globais, agora, self.janela);
        if e.globais.len() >= self.max_glob {
            return (self.janela - (agora - e.globais[0])).max(1.0) as u64;
        }
        (e.ate.get(cliente).copied().unwrap_or(0.0) - agora + 0.999).max(0.0) as u64
    }

    pub fn falhou(&self, cliente: &str) {
        let agora = self.agora();
        let mut e = self.estado.lock();
        Self::podar(&mut e.globais, agora, self.janela);
        e.globais.push_back(agora);
        let fila = e.falhas.entry(cliente.into()).or_default();
        Self::podar(fila, agora, self.janela);
        fila.push_back(agora);
        if fila.len() >= self.max {
            fila.clear();
            let r = e.rodadas.entry(cliente.into()).or_insert(0);
            *r += 1;
            let trava = (self.trava * 2f64.powi(*r - 1)).min(self.trava_max);
            e.ate.insert(cliente.into(), agora + trava);
        }
    }

    pub fn acertou(&self, cliente: &str) {
        let mut e = self.estado.lock();
        e.falhas.remove(cliente);
        e.ate.remove(cliente);
        e.rodadas.remove(cliente);
    }
}

pub struct AuthService {
    pub user: String,
    ttl: f64,
    it: u32,
    pleno: bool,
    pub throttle: LoginThrottle,
    conn: Mutex<Connection>,
    falso: String,
}

impl AuthService {
    pub fn open(path: &Path, user: &str, ttl_s: f64) -> Result<Self, AuthError> {
        Self::open_with(path, user, ttl_s, ITERACOES)
    }

    pub fn open_with(path: &Path, user: &str, ttl_s: f64, it: u32) -> Result<Self, AuthError> {
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(DDL)?;
        let tem_default: bool = conn.prepare("PRAGMA table_info(credential)")?.query_map([], |r| r.get::<_, String>(1))?.flatten().any(|c| c == "is_default");
        if !tem_default {
            // auth.db criado antes da senha de fábrica
            conn.execute("ALTER TABLE credential ADD COLUMN is_default INTEGER NOT NULL DEFAULT 0", [])?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        // hash falso para igualar o tempo de "usuário sem senha" ao de "senha errada"
        let pleno = it >= ITERACOES;
        let falso = hash_password_argon2id(&token_urlsafe(16), pleno);
        Ok(AuthService { user: user.into(), ttl: ttl_s, it, pleno, throttle: LoginThrottle::default(), conn: Mutex::new(conn), falso })
    }

    pub fn has_password(&self) -> bool {
        self.conn.lock().query_row("SELECT 1 FROM credential WHERE user=?", [&self.user], |r| r.get::<_, i64>(0)).optional().ok().flatten().is_some()
    }

    /// A senha ainda é a de fábrica?
    pub fn uses_default_password(&self) -> bool {
        self.conn
            .lock()
            .query_row("SELECT is_default FROM credential WHERE user=?", [&self.user], |r| r.get::<_, i64>(0))
            .optional()
            .ok()
            .flatten()
            .unwrap_or(0)
            != 0
    }

    /// Cria o usuário com a senha de fábrica **se ainda não há senha**. Nunca sobrescreve.
    pub fn seed_default(&self) -> Result<bool, AuthError> {
        if self.has_password() {
            return Ok(false);
        }
        self.conn.lock().execute(
            "INSERT INTO credential(user, pw_hash, updated_at, is_default) VALUES (?,?,?,1)",
            params![self.user, hash_password_argon2id(SENHA_PADRAO, self.pleno), agora()],
        )?;
        Ok(true)
    }

    /// Define ou troca a senha e **revoga todas as sessões** e os pareamentos de dispositivo.
    pub fn set_password(&self, senha: &str) -> Result<(), AuthError> {
        let n = senha.chars().count();
        if n < SENHA_MIN {
            return Err(AuthError::WeakPassword(format!("a senha precisa de pelo menos {SENHA_MIN} caracteres")));
        }
        if n > SENHA_MAX {
            return Err(AuthError::WeakPassword(format!("a senha passa de {SENHA_MAX} caracteres")));
        }
        if senha.to_lowercase() == self.user.to_lowercase() {
            return Err(AuthError::WeakPassword("a senha não pode ser o nome de usuário".into()));
        }
        let novo = hash_password_argon2id(senha, self.pleno);
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO credential(user, pw_hash, updated_at, is_default) VALUES (?,?,?,0)
             ON CONFLICT(user) DO UPDATE SET pw_hash=excluded.pw_hash, updated_at=excluded.updated_at, is_default=0",
            params![self.user, novo, agora()],
        )?;
        tx.execute("DELETE FROM sessions", [])?;
        tx.execute("DELETE FROM device_tokens", [])?; // trocar a senha também desfaz o pareamento
        tx.commit()?;
        Ok(())
    }

    /// Confere a senha sem criar sessão nem contar tentativa (use `login`).
    pub fn check_password(&self, senha: &str) -> bool {
        // cópia da senha apagada da memória ao sair
        let senha = Zeroizing::new(senha.to_string());
        let r: Option<String> = self.conn.lock().query_row("SELECT pw_hash FROM credential WHERE user=?", [&self.user], |r| r.get(0)).optional().ok().flatten();
        let ok = verify_password(&senha, r.as_deref().unwrap_or(&self.falso)) && r.is_some();
        if ok && r.as_deref().is_some_and(precisa_rehash) {
            // hash PBKDF2 antigo: regrava em Argon2id agora que a senha em claro está à mão
            let novo = hash_password_argon2id(&senha, self.pleno);
            let _ = self.conn.lock().execute("UPDATE credential SET pw_hash=? WHERE user=?", params![novo, self.user]);
        }
        ok
    }

    /// Token da sessão (vai no cookie). Usuário errado falha igual a senha errada.
    pub fn verify_credentials(&self, senha: &str, cliente: &str, usuario: Option<&str>) -> Result<(), AuthError> {
        if !self.has_password() {
            return Err(AuthError::NotConfigured("defina a senha com: orion set-password".into()));
        }
        let espera = self.throttle.espera(cliente);
        if espera > 0 {
            return Err(AuthError::LockedOut(espera));
        }
        let usuario_ok = usuario.is_none_or(|u| u.trim().to_lowercase().as_bytes().ct_eq(self.user.to_lowercase().as_bytes()).into());
        let senha_ok = senha.chars().count() <= SENHA_MAX && self.check_password(senha);
        if !(usuario_ok && senha_ok) {
            self.throttle.falhou(cliente);
            return Err(AuthError::BadCredentials("usuário ou senha incorretos".into()));
        }
        self.throttle.acertou(cliente);
        Ok(())
    }

    pub fn login(&self, senha: &str, cliente: &str, usuario: Option<&str>) -> Result<String, AuthError> {
        self.verify_credentials(senha, cliente, usuario)?;
        self.create_session()
    }

    /// Só chamar depois de conferir todos os fatores (REGRAS.md: login com senha e sessão revogável).
    pub fn create_session(&self) -> Result<String, AuthError> {
        let token = token_urlsafe(32);
        let t = agora();
        let c = self.conn.lock();
        c.execute("DELETE FROM sessions WHERE expires_at < ?", [t])?;
        c.execute("INSERT INTO sessions(token_hash, user, created_at, expires_at) VALUES (?,?,?,?)", params![digest(&token), self.user, t, t + self.ttl])?;
        Ok(token)
    }

    /// Usuário dono do token, ou `None` se inexistente, revogado ou expirado.
    pub fn validate(&self, token: Option<&str>) -> Option<String> {
        let token = token.filter(|t| !t.is_empty() && t.len() <= 200)?;
        let r: Option<(String, f64)> = self
            .conn
            .lock()
            .query_row("SELECT user, expires_at FROM sessions WHERE token_hash=?", [digest(token)], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .ok()
            .flatten();
        r.filter(|(_, exp)| *exp >= agora()).map(|(u, _)| u)
    }

    /// Token novo para um dispositivo (ex.: `ponte`). Parear de novo revoga o anterior.
    pub fn create_device_token(&self, scope: &str, name: &str) -> Result<String, AuthError> {
        let token = token_urlsafe(32);
        let c = self.conn.lock();
        c.execute("DELETE FROM device_tokens WHERE scope=?", [scope])?;
        c.execute(
            "INSERT INTO device_tokens(token_hash, scope, name, created_at) VALUES (?,?,?,?)",
            params![digest(&token), scope, name.chars().take(60).collect::<String>(), agora()],
        )?;
        Ok(token)
    }

    pub fn device_scope(&self, token: Option<&str>) -> Option<String> {
        let token = token.filter(|t| !t.is_empty() && t.len() <= 200)?;
        self.conn.lock().query_row("SELECT scope FROM device_tokens WHERE token_hash=?", [digest(token)], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn revoke_device_tokens(&self, scope: Option<&str>) -> usize {
        let c = self.conn.lock();
        match scope {
            None => c.execute("DELETE FROM device_tokens", []),
            Some(s) => c.execute("DELETE FROM device_tokens WHERE scope=?", [s]),
        }
        .unwrap_or(0)
    }

    pub fn logout(&self, token: Option<&str>) {
        if let Some(t) = token {
            let _ = self.conn.lock().execute("DELETE FROM sessions WHERE token_hash=?", [digest(t)]);
        }
    }

    pub fn active_sessions(&self) -> i64 {
        self.conn.lock().query_row("SELECT COUNT(*) FROM sessions WHERE expires_at >= ?", [agora()], |r| r.get(0)).unwrap_or(0)
    }

    pub fn revoke_all(&self) -> usize {
        self.conn.lock().execute("DELETE FROM sessions", []).unwrap_or(0)
    }
}

#[cfg(test)]
mod factor_tests {
    use super::*;
    #[test]
    fn conferir_senha_nao_cria_sessao() {
        let dir = tempfile::tempdir().unwrap();
        let auth = AuthService::open_with(&dir.path().join("auth.db"), "admin", 3600.0, 10).unwrap();
        auth.set_password("senha-de-teste-comprida").unwrap();
        auth.verify_credentials("senha-de-teste-comprida", "pc", None).unwrap();
        assert_eq!(auth.active_sessions(), 0);
        let token = auth.create_session().unwrap();
        assert!(auth.validate(Some(&token)).is_some());
        auth.logout(Some(&token));
        assert!(auth.validate(Some(&token)).is_none());
    }
}

//! O app sobe o `orion serve` sozinho (processo filho, só em 127.0.0.1, porta livre e token aleatório)
//! e o encerra ao sair. Com `ORION_URL` definido, nada é iniciado: usa o servidor que você já tem.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static FILHO: Mutex<Option<Child>> = Mutex::new(None);

/// `ORION_BIN`, depois o `orion` ao lado do executável do app, depois o do PATH.
pub fn binario() -> PathBuf {
    if let Some(b) = std::env::var_os("ORION_BIN") {
        return b.into();
    }
    let nome = if cfg!(windows) { "orion.exe" } else { "orion" };
    std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join(nome))).filter(|p| p.exists()).unwrap_or_else(|| nome.into())
}

fn token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut s = String::new();
    for _ in 0..4 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.write_u32(std::process::id());
        s.push_str(&format!("{:016x}", h.finish()));
    }
    s
}

/// Sobe o servidor e devolve `(url, token)`. Erro em português se o binário não existir.
pub fn iniciar() -> Result<(String, String), String> {
    let porta = std::net::TcpListener::bind(("127.0.0.1", 0)).and_then(|l| l.local_addr()).map_err(|e| format!("Sem porta livre no computador: {e}"))?.port();
    let tk = token();
    let bin = binario();
    let mut c = Command::new(&bin);
    c.arg("serve").env("ORION_HOST", "127.0.0.1").env("ORION_PORT", porta.to_string()).env("ORION_ADMIN_TOKEN", &tk).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let filho = c.spawn().map_err(|e| format!("Não consegui iniciar o Orion ({}): {e}. Instale o Orion completo ou defina ORION_URL para usar um servidor já rodando.", bin.display()))?;
    *FILHO.lock().unwrap() = Some(filho);
    Ok((format!("http://127.0.0.1:{porta}"), tk))
}

pub fn ativo() -> bool {
    FILHO.lock().map(|g| g.is_some()).unwrap_or(false)
}

/// Se o servidor filho já morreu (ex.: banco cifrado sem senha), diz por quê.
pub fn morreu() -> Option<String> {
    let mut g = FILHO.lock().ok()?;
    let f = g.as_mut()?;
    let st = f.try_wait().ok()??;
    Some(format!("O servidor do Orion encerrou ao iniciar ({st}). Se o banco é cifrado, defina ORION_DB_PASSWORD ou rode \"orion serve\" num terminal e abra o app com ORION_URL."))
}

pub fn encerrar() {
    if let Some(mut f) = FILHO.lock().ok().and_then(|mut g| g.take()) {
        let _ = f.kill();
        let _ = f.wait();
    }
}

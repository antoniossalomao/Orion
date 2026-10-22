//! Logs em JSON por linha, com segredos mascarados (REGRAS.md: segredos nunca em log, audit ou
//! processo filho).

use orion_core::logcrypt::EncryptedLog;
use std::io::Write;
use std::sync::OnceLock;
use tracing_subscriber::fmt::MakeWriter;

/// Destino cifrado em arquivo, quando o banco é cifrado.
static CIFRADO: OnceLock<EncryptedLog> = OnceLock::new();

/// Escreve no stderr mascarando padrões de token conhecidos em cada linha.
pub struct Mascarado;

pub struct Escritor(Vec<u8>);

impl Write for Escritor {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for Escritor {
    fn drop(&mut self) {
        let texto = String::from_utf8_lossy(&self.0);
        // segredos e dados pessoais mascarados antes de qualquer destino
        let limpo = orion_policy::audit::mask_pii(&orion_policy::audit::mask_tokens(&texto));
        let _ = std::io::stderr().write_all(limpo.as_bytes());
        if let Some(c) = CIFRADO.get() {
            let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            let _ = c.write_line(limpo.trim_end_matches('\n'), agora);
        }
    }
}

impl<'a> MakeWriter<'a> for Mascarado {
    type Writer = Escritor;
    fn make_writer(&'a self) -> Self::Writer {
        Escritor(Vec::new())
    }
}

pub fn setup(nivel: &str, json: bool, cifrado: Option<EncryptedLog>) {
    if let Some(c) = cifrado {
        let _ = CIFRADO.set(c);
    }
    let nivel = match nivel {
        "DEBUG" => "debug",
        "WARNING" => "warn",
        "ERROR" | "CRITICAL" => "error",
        _ => "info",
    };
    // o cliente HTTP nunca loga a URL com token (o do Telegram vai na URL)
    let filtro = tracing_subscriber::EnvFilter::new(format!("{nivel},hyper=warn,reqwest=warn,h2=warn"));
    let b = tracing_subscriber::fmt().with_env_filter(filtro).with_writer(Mascarado);
    if json {
        b.json().init();
    } else {
        b.init();
    }
}

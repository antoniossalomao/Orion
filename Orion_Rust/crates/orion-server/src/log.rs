//! Logs em JSON por linha, com segredos mascarados (REGRAS.md: segredos nunca em log, audit ou
//! processo filho).

use std::io::Write;
use tracing_subscriber::fmt::MakeWriter;

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
        let _ = std::io::stderr().write_all(orion_policy::audit::mask_tokens(&texto).as_bytes());
    }
}

impl<'a> MakeWriter<'a> for Mascarado {
    type Writer = Escritor;
    fn make_writer(&'a self) -> Self::Writer {
        Escritor(Vec::new())
    }
}

pub fn setup(nivel: &str, json: bool) {
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

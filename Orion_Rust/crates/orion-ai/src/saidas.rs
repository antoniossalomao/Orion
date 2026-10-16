//! Registro de saída (REGRAS.md: toda saída de dado fica registrada, sem conteúdo): toda chamada
//! que leva dado para fora do computador vira uma linha.
//!
//! Provedor, tipo da chamada, modelo, se deu certo, latência, tamanhos de ida e volta e o tipo do
//! conteúdo. Nunca o conteúdo, a URL nem a chave. O destino (a tabela `external_calls`) é ligado
//! pelo app; sem destino o registro não faz nada. Registrar nunca derruba a chamada.

use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Instant;

/// Provedores que rodam neste computador: entram no registro, mas não são "saída".
pub const LOCAIS: [&str; 2] = ["ollama", "local"];

const HOSTS: [(&str, &str); 5] = [
    ("groq.com", "groq"),
    ("googleapis.com", "gemini"),
    ("brave.com", "brave"),
    ("telegram.org", "telegram"),
    ("open-meteo.com", "open-meteo"),
];

#[derive(Debug, Clone, Default)]
pub struct Chamada {
    pub provider: String,
    pub kind: String,
    pub ok: bool,
    pub latency_ms: i64,
    pub model: String,
    pub bytes_out: i64,
    pub bytes_in: i64,
    pub content_kind: String,
}

pub type Destino = Arc<dyn Fn(&Chamada) -> anyhow::Result<()> + Send + Sync>;

static DESTINO: RwLock<Option<Destino>> = RwLock::new(None);

/// O app liga (`store.add_external_call`) ao subir e desliga (`None`) ao parar.
pub fn definir_destino(fn_: Option<Destino>) {
    *DESTINO.write() = fn_;
}

/// Destino que grava no banco.
pub fn destino_store(store: Arc<orion_store::Store>) -> Destino {
    Arc::new(move |c: &Chamada| {
        store.add_external_call(&orion_store::library::ExternalCall {
            provider: &c.provider,
            kind: &c.kind,
            ok: c.ok,
            latency_ms: c.latency_ms,
            model: &c.model,
            bytes_out: c.bytes_out,
            bytes_in: c.bytes_in,
            content_kind: &c.content_kind,
            ts: None,
        })?;
        Ok(())
    })
}

/// Nome curto do provedor pelo host (sem guardar a URL).
pub fn provedor_da_url(url: &str, padrao: &str) -> String {
    let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(|h| h.trim_matches(['[', ']']).to_lowercase()));
    let host = host.unwrap_or_default();
    if matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1") {
        return if padrao.is_empty() { "local".into() } else { padrao.into() };
    }
    for (sufixo, nome) in HOSTS {
        if host == sufixo || host.ends_with(&format!(".{sufixo}")) {
            return nome.into();
        }
    }
    if !padrao.is_empty() {
        return padrao.into();
    }
    if host.is_empty() { "desconhecido".into() } else { host.chars().take(60).collect() }
}

/// Uma linha em `external_calls`. Nunca falha: é telemetria.
#[allow(clippy::too_many_arguments)]
pub fn registrar(provider: &str, kind: &str, ok: bool, latency_ms: f64, model: &str, bytes_out: usize, bytes_in: usize, content_kind: &str) {
    let destino = DESTINO.read().clone();
    let Some(fn_) = destino else { return };
    let c = Chamada {
        provider: provider.into(),
        kind: kind.into(),
        ok,
        latency_ms: latency_ms.round() as i64,
        model: model.into(),
        bytes_out: bytes_out as i64,
        bytes_in: bytes_in as i64,
        content_kind: content_kind.into(),
    };
    if let Err(e) = fn_(&c) {
        tracing::warn!("não consegui registrar a saída para {provider}: {e}");
    }
}

/// Mede a latência e registra ao sair (no `Drop`), com ou sem erro: `ok` só vira `true` se quem
/// mede marcar.
pub struct Medida {
    pub ok: bool,
    pub bytes_in: usize,
    pub model: String,
    provider: String,
    kind: String,
    bytes_out: usize,
    content_kind: String,
    inicio: Instant,
}

pub fn medir(provider: &str, kind: &str, model: &str, bytes_out: usize, content_kind: &str) -> Medida {
    Medida {
        ok: false,
        bytes_in: 0,
        model: model.into(),
        provider: provider.into(),
        kind: kind.into(),
        bytes_out,
        content_kind: content_kind.into(),
        inicio: Instant::now(),
    }
}

impl Drop for Medida {
    fn drop(&mut self) {
        registrar(
            &self.provider,
            &self.kind,
            self.ok,
            self.inicio.elapsed().as_secs_f64() * 1000.0,
            &self.model,
            self.bytes_out,
            self.bytes_in,
            &self.content_kind,
        );
    }
}

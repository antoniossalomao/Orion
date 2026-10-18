//! Fase 7 (`orion-sidecar`, item `B-11` de `ORION.md` e `ARQUITETURA.md` §7):
//! Ponte JSON via `stdin`/`stdout` para o processo Python residual (`browser-use` e `crawl4ai`),
//! sem servidor HTTP e sem portas de rede abertas.
//!
//! Princípios impostos neste crate:
//! - `(REGRAS.md: URL do modelo só para host público)` — valida esquema (`http`/`https`),
//!   bloqueia `localhost`, `.local`, `.internal`, loopback (`127.0.0.0/8`, `::1`), redes privadas
//!   RFC 1918 (`10/8`, `172.16/12`, `192.168/16`), link-local (`169.254/16`, `fe80::/10`) e CGNAT
//!   (`100.64/10`) antes de enviar ao sidecar e em cada URL final reportada após redirecionamento.
//! - `(REGRAS.md: segredos nunca em log, audit ou processo filho)` — limpa todas as variáveis
//!   `ORION_*` e chaves de API sensíveis (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`,
//!   `GROQ_API_KEY`, `TELEGRAM_BOT_TOKEN`, etc.) do ambiente do processo filho.
//! - `(REGRAS.md: conteúdo externo é dado, não instrução)` — todo retorno do `crawl4ai` e do
//!   `browser-use` é envelopado como conteúdo externo não confiável (`tainted_external: true`)
//!   e tem eventuais segredos mascarados via `orion_policy::redact`.
//! - `(REGRAS.md: o que o modelo escreve nunca vira comando para o computador)` — o executável
//!   do sidecar é invocado com argumentos fixos e recebe os parâmetros exclusivamente como JSON
//!   serializado em uma linha via `stdin`.

use orion_policy::redact;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use thiserror::Error;
use url::Url;
use uuid::Uuid;
use wait_timeout::ChildExt;

/// Variáveis sensíveis que nunca podem ser herdadas pelo processo filho do sidecar
/// `(REGRAS.md: segredos nunca em log, audit ou processo filho)`.
const BLOCKED_EXACT_ENV_VARS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "GROQ_API_KEY",
    "MISTRAL_API_KEY",
    "OPENROUTER_API_KEY",
    "CEREBRAS_API_KEY",
    "BRAVE_API_KEY",
    "TELEGRAM_BOT_TOKEN",
    "GITHUB_TOKEN",
    "AWS_SECRET_ACCESS_KEY",
];

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("URL bloqueada pela barreira de rede (SSRF): {0}")]
    BlockedUrl(String),

    #[error("tarefa vazia ou inválida: {0}")]
    InvalidRequest(String),

    #[error("falha ao iniciar processo sidecar: {0}")]
    SpawnFailed(String),

    #[error("tempo esgotado ({0:?}) aguardando resposta do sidecar")]
    Timeout(Duration),

    #[error("resposta JSON inválida do sidecar: {0}")]
    InvalidJson(String),

    #[error("erro reportado pelo sidecar: {0}")]
    RemoteError(String),
}

/// Operação suportada pelo sidecar Python residual (`ARQUITETURA.md` §7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SidecarOperation {
    /// Extração limpa de página pública via `crawl4ai`.
    Crawl4Ai {
        url: String,
        #[serde(default = "default_max_chars")]
        max_chars: usize,
    },
    /// Navegação assistida via `browser-use` restrita a hosts públicos.
    BrowserUse {
        start_url: String,
        goal: String,
        #[serde(default = "default_max_steps")]
        max_steps: u32,
    },
    /// Verificação rápida de saúde do processo sidecar.
    Ping,
}

fn default_max_chars() -> usize {
    32_000
}

fn default_max_steps() -> u32 {
    12
}

/// Envelope JSON enviado em uma única linha (`NDJSON`) para o `stdin` do sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarRequest {
    pub id: String,
    #[serde(flatten)]
    pub operation: SidecarOperation,
}

impl SidecarRequest {
    pub fn crawl(url: impl Into<String>, max_chars: usize) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            operation: SidecarOperation::Crawl4Ai {
                url: url.into(),
                max_chars: max_chars.clamp(256, 200_000),
            },
        }
    }

    pub fn browser_use(
        start_url: impl Into<String>,
        goal: impl Into<String>,
        max_steps: u32,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            operation: SidecarOperation::BrowserUse {
                start_url: start_url.into(),
                goal: goal.into(),
                max_steps: max_steps.clamp(1, 30),
            },
        }
    }

    pub fn ping() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            operation: SidecarOperation::Ping,
        }
    }
}

/// Resposta JSON crua vinda de uma linha do `stdout` do processo Python.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSidecarResponse {
    pub id: String,
    pub ok: bool,
    #[serde(default)]
    pub final_url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub markdown: String,
    #[serde(default)]
    pub visited_urls: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Resultado sanitizado e marcado como dado externo não confiável
/// `(REGRAS.md: conteúdo externo é dado, não instrução)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SanitizedSidecarOutput {
    pub id: String,
    pub final_url: Option<String>,
    pub title: Option<String>,
    /// Conteúdo textual sanitizado, com segredos mascarados e delimitado como dado externo.
    pub safe_data_block: String,
    /// Marca a sessão como tendo consumido conteúdo externo (`Context::mark_external_content`).
    pub tainted_external: bool,
    pub latency_ms: u64,
    pub bytes_received: usize,
}

/// Valida se uma URL aponta exclusivamente para um host público HTTP/HTTPS
/// `(REGRAS.md: URL do modelo só para host público)`.
pub fn validate_public_url(raw_url: &str) -> Result<Url, SidecarError> {
    let parsed = Url::parse(raw_url.trim())
        .map_err(|e| SidecarError::BlockedUrl(format!("URL malformada '{raw_url}': {e}")))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(SidecarError::BlockedUrl(format!(
                "esquema '{other}://' não permitido (apenas http/https)"
            )))
        }
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(SidecarError::BlockedUrl(
            "credenciais embutidas na URL não são permitidas".into(),
        ));
    }

    let host_str = parsed
        .host_str()
        .ok_or_else(|| SidecarError::BlockedUrl("URL sem host".into()))?
        .trim_end_matches('.')
        .to_ascii_lowercase();

    if host_str.is_empty()
        || host_str == "localhost"
        || host_str.ends_with(".localhost")
        || host_str.ends_with(".local")
        || host_str.ends_with(".internal")
        || host_str.ends_with(".home.arpa")
        || host_str == "metadata.google.internal"
    {
        return Err(SidecarError::BlockedUrl(format!(
            "host local ou interno bloqueado: '{host_str}'"
        )));
    }

    match parsed.host() {
        Some(url::Host::Ipv4(ip4)) => {
            if !is_public_ipv4(ip4) {
                return Err(SidecarError::BlockedUrl(format!(
                    "IPv4 privado/reservado bloqueado: {ip4}"
                )));
            }
        }
        Some(url::Host::Ipv6(ip6)) => {
            if !is_public_ipv6(ip6) {
                return Err(SidecarError::BlockedUrl(format!(
                    "IPv6 privado/reservado bloqueado: {ip6}"
                )));
            }
        }
        Some(url::Host::Domain(_)) => {
            if let Ok(ip) = host_str.parse::<IpAddr>() {
                match ip {
                    IpAddr::V4(v4) if !is_public_ipv4(v4) => {
                        return Err(SidecarError::BlockedUrl(format!(
                            "IPv4 privado/reservado bloqueado: {v4}"
                        )));
                    }
                    IpAddr::V6(v6) if !is_public_ipv6(v6) => {
                        return Err(SidecarError::BlockedUrl(format!(
                            "IPv6 privado/reservado bloqueado: {v6}"
                        )));
                    }
                    _ => {}
                }
            }
        }
        None => return Err(SidecarError::BlockedUrl("URL sem host válido".into())),
    }

    Ok(parsed)
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    let is_cgnat = o[0] == 100 && (64..=127).contains(&o[1]);
    let is_benchmark = o[0] == 198 && (o[1] == 18 || o[1] == 19);
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || o[0] == 0
        || o[0] >= 240
        || is_cgnat
        || is_benchmark)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4() {
        if ip.segments()[0..5] == [0, 0, 0, 0, 0] && ip.segments()[5] == 0xffff {
            return is_public_ipv4(v4);
        }
    }
    let seg0 = ip.segments()[0];
    let is_unique_local = (seg0 & 0xfe00) == 0xfc00;
    let is_link_local = (seg0 & 0xffc0) == 0xfe80;
    let is_doc = seg0 == 0x2001 && ip.segments()[1] == 0x0db8;
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_unique_local
        || is_link_local
        || is_doc)
}

fn redact_str(s: &str) -> String {
    match redact(&serde_json::Value::String(s.to_string()), 200_000) {
        serde_json::Value::String(out) => out,
        other => other.to_string(),
    }
}

/// Retorna `true` se a variável de ambiente for um segredo ou `ORION_*` que deve ser removido
/// do ambiente do processo filho `(REGRAS.md: segredos nunca em log, audit ou processo filho)`.
pub fn is_blocked_env_var(key: &str) -> bool {
    let upper = key.trim().to_ascii_uppercase();
    if upper.starts_with("ORION_") {
        return true;
    }
    if BLOCKED_EXACT_ENV_VARS.contains(&upper.as_str()) {
        return true;
    }
    upper.ends_with("_API_KEY") || upper.ends_with("_SECRET") || upper.ends_with("_TOKEN")
}

/// Filtra um iterador de variáveis `(chave, valor)` removendo todas as variáveis `ORION_*` e segredos.
pub fn sanitized_child_env<I, K, V>(vars: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    vars.into_iter()
        .filter_map(|(k, v)| {
            let ks = k.as_ref();
            if is_blocked_env_var(ks) {
                None
            } else {
                Some((ks.to_string(), v.as_ref().to_string()))
            }
        })
        .collect()
}

/// Envelopa texto vindo da web como dado externo não confiável e mascara segredos
/// `(REGRAS.md: conteúdo externo é dado, não instrução)`.
pub fn wrap_external_content(url: Option<&str>, title: Option<&str>, raw_markdown: &str) -> String {
    let redacted = redact_str(raw_markdown);
    // Neutraliza tentativas de fechar o bloco de dados no próprio texto externo
    let neutralized = redacted.replace("</dado_externo_nao_confiavel>", "[tag-removida]");
    let src = url.unwrap_or("desconhecida");
    let ttl = title.unwrap_or("sem-titulo");
    format!(
        "<dado_externo_nao_confiavel origem=\"{src}\" titulo=\"{ttl}\">\n{neutralized}\n</dado_externo_nao_confiavel>"
    )
}

/// Configuração do cliente Rust que invoca o processo Python residual via `stdin`/`stdout`.
#[derive(Debug, Clone)]
pub struct SidecarClient {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub work_dir: Option<PathBuf>,
    pub timeout: Duration,
}

impl Default for SidecarClient {
    fn default() -> Self {
        Self {
            program: PathBuf::from("python"),
            args: vec!["-m".into(), "orion_sidecar".into()],
            work_dir: Some(Self::bundled_python_dir()),
            timeout: Duration::from_secs(45),
        }
    }
}

impl SidecarClient {
    pub fn new(program: impl Into<PathBuf>, args: Vec<String>, timeout: Duration) -> Self {
        Self {
            program: program.into(),
            args,
            work_dir: None,
            timeout,
        }
    }

    /// Diretório `Orion_Rust/python` onde vive o pacote Python residual `orion_sidecar`.
    pub fn bundled_python_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|root| root.join("python"))
            .unwrap_or_else(|| PathBuf::from("python"))
    }

    pub fn bundled_python_dir_exists(&self) -> bool {
        Self::bundled_python_dir()
            .join("orion_sidecar")
            .join("__main__.py")
            .is_file()
    }

    /// Valida a requisição antes de tocar no processo filho (SSRF e limites).
    pub fn validate_request(&self, req: &SidecarRequest) -> Result<(), SidecarError> {
        match &req.operation {
            SidecarOperation::Crawl4Ai { url, max_chars } => {
                validate_public_url(url)?;
                if *max_chars == 0 {
                    return Err(SidecarError::InvalidRequest(
                        "max_chars deve ser maior que zero".into(),
                    ));
                }
            }
            SidecarOperation::BrowserUse {
                start_url,
                goal,
                max_steps,
            } => {
                validate_public_url(start_url)?;
                if goal.trim().is_empty() {
                    return Err(SidecarError::InvalidRequest(
                        "o objetivo (goal) do browser-use não pode ser vazio".into(),
                    ));
                }
                if *max_steps == 0 || *max_steps > 50 {
                    return Err(SidecarError::InvalidRequest(
                        "max_steps fora do intervalo permitido (1..=50)".into(),
                    ));
                }
            }
            SidecarOperation::Ping => {}
        }
        Ok(())
    }

    /// Executa uma chamada isolada via `stdin`/`stdout` ao processo Python residual,
    /// sem abrir porta de rede e sem vazar variáveis `ORION_*` ou segredos.
    pub fn execute(&self, req: &SidecarRequest) -> Result<SanitizedSidecarOutput, SidecarError> {
        self.validate_request(req)?;

        let started = Instant::now();
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .envs(sanitized_child_env(std::env::vars()))
            // Garante que o Python rode em modo UTF-8 e sem buffer de linha bloqueante
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUNBUFFERED", "1");

        if let Some(dir) = &self.work_dir {
            cmd.current_dir(dir);
        }

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| SidecarError::SpawnFailed(redact_str(&e.to_string())))?;

        // Escreve o JSON em uma única linha no stdin e fecha o pipe para sinalizar EOF
        let payload = serde_json::to_string(req)
            .map_err(|e| SidecarError::InvalidRequest(e.to_string()))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(payload.as_bytes())
                .and_then(|_| stdin.write_all(b"\n"))
                .and_then(|_| stdin.flush())
                .map_err(|e| SidecarError::SpawnFailed(redact_str(&e.to_string())))?;
        }

        // Aguarda até o tempo-limite; se exceder, mata o processo filho imediatamente
        match child.wait_timeout(self.timeout) {
            Ok(Some(_status)) => {}
            Ok(None) => {
                kill_child_quietly(&mut child);
                return Err(SidecarError::Timeout(self.timeout));
            }
            Err(e) => {
                kill_child_quietly(&mut child);
                return Err(SidecarError::SpawnFailed(redact_str(&e.to_string())));
            }
        }

        let latency_ms = started.elapsed().as_millis() as u64;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SidecarError::InvalidJson("stdout indisponível".into()))?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let bytes_received = reader
            .read_line(&mut line)
            .map_err(|e| SidecarError::InvalidJson(redact_str(&e.to_string())))?;

        if line.trim().is_empty() {
            return Err(SidecarError::InvalidJson(
                "sidecar encerrou sem escrever JSON no stdout".into(),
            ));
        }

        let raw: RawSidecarResponse = serde_json::from_str(line.trim())
            .map_err(|e| SidecarError::InvalidJson(redact_str(&e.to_string())))?;

        if !raw.ok {
            let err_msg = raw
                .error
                .unwrap_or_else(|| "falha desconhecida no sidecar".into());
            return Err(SidecarError::RemoteError(redact_str(&err_msg)));
        }

        // Revalida a URL final e todas as URLs visitadas contra redirecionamentos para IPs internos (SSRF)
        if let Some(final_u) = &raw.final_url {
            validate_public_url(final_u)?;
        }
        for visited in &raw.visited_urls {
            validate_public_url(visited)?;
        }

        let safe_data_block = wrap_external_content(
            raw.final_url.as_deref(),
            raw.title.as_deref(),
            &raw.markdown,
        );

        Ok(SanitizedSidecarOutput {
            id: raw.id,
            final_url: raw.final_url,
            title: raw.title,
            safe_data_block,
            tainted_external: true,
            latency_ms,
            bytes_received,
        })
    }
}

fn kill_child_quietly(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Código-fonte do módulo Python residual em `Orion_Rust/python/orion_sidecar/__main__.py`.
pub const PYTHON_SIDECAR_BOOTSTRAP: &str =
    include_str!("../../../python/orion_sidecar/__main__.py");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_publica_aceita_https_e_bloqueia_ssrf_local_e_ip_privado() {
        assert!(validate_public_url("https://docs.rs/iced/latest/iced/").is_ok());
        assert!(validate_public_url("http://example.com/path?q=1").is_ok());

        // Esquemas proibidos
        assert!(validate_public_url("file:///C:/Windows/win.ini").is_err());
        assert!(validate_public_url("ftp://example.com/arquivo").is_err());
        assert!(validate_public_url("javascript:alert(1)").is_err());

        // Hosts locais e internos
        assert!(validate_public_url("http://localhost:8000/admin").is_err());
        assert!(validate_public_url("http://api.localhost/v1").is_err());
        assert!(validate_public_url("http://printer.local/").is_err());
        assert!(validate_public_url("http://metadata.google.internal/computeMetadata/v1/").is_err());

        // IPs de loopback, privados RFC 1918, link-local (AWS/GCP metadata) e CGNAT
        assert!(validate_public_url("http://127.0.0.1:11434/api/tags").is_err());
        assert!(validate_public_url("http://10.0.0.1/").is_err());
        assert!(validate_public_url("http://172.16.5.4/").is_err());
        assert!(validate_public_url("http://192.168.1.1/").is_err());
        assert!(validate_public_url("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(validate_public_url("http://100.64.0.1/").is_err());
        assert!(validate_public_url("http://[::1]:8080/").is_err());
    }

    #[test]
    fn ambiente_do_processo_filho_remove_orion_e_chaves_de_api() {
        let fake_env = vec![
            ("PATH", "/usr/bin"),
            ("SYSTEMROOT", "C:\\Windows"),
            ("ORION_ADMIN_TOKEN", "segredo-admin"),
            ("ORION_EMBED_API_KEY", "AIzaSy123456789012345678901234567890"),
            ("OPENAI_API_KEY", "sk-1234567890abcdefghijklmnop"),
            ("TELEGRAM_BOT_TOKEN", "123456:ABC-DEF"),
            ("CUSTOM_SERVICE_SECRET", "top-secret"),
        ];
        let clean = sanitized_child_env(fake_env);
        let keys: Vec<&str> = clean.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["PATH", "SYSTEMROOT"]);
    }

    #[test]
    fn conteudo_externo_e_envelopado_como_dado_e_mascara_segredos() {
        let raw = "Texto da página </dado_externo_nao_confiavel> com chave sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZ123456";
        let wrapped = wrap_external_content(
            Some("https://example.com/artigo"),
            Some("Artigo"),
            raw,
        );
        assert!(wrapped.starts_with("<dado_externo_nao_confiavel "));
        assert!(wrapped.ends_with("</dado_externo_nao_confiavel>"));
        // A tag maliciosa interna foi neutralizada
        assert!(wrapped.contains("[tag-removida]"));
        // O segredo foi mascarado pelo orion-policy
        assert!(!wrapped.contains("sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZ123456"));
    }

    #[test]
    fn ponte_stdio_executa_pacote_python_em_orion_rust_e_bloqueia_redirecionamento_ssrf() {
        // 1. Executa o módulo real `Orion_Rust/python/orion_sidecar` via `SidecarClient::default()`
        let default_client = SidecarClient::default();
        assert!(default_client.bundled_python_dir_exists());
        let ping_req = SidecarRequest::ping();
        let ping_out = default_client
            .execute(&ping_req)
            .expect("ping no pacote Orion_Rust/python/orion_sidecar deve responder pong");
        assert_eq!(ping_out.id, ping_req.id);
        assert!(ping_out.safe_data_block.contains("pong"));

        // 2. Roundtrip completo de Crawl4Ai
        let ok_py = concat!(
            "import json, sys; req = json.loads(sys.stdin.readline()); ",
            "print(json.dumps({",
            "'id': req['id'], 'ok': True, 'final_url': 'https://example.com/ok', ",
            "'title': 'Exemplo', 'markdown': 'Conteudo limpo', ",
            "'visited_urls': ['https://example.com/ok']",
            "}), flush=True)"
        );
        let client = SidecarClient::new(
            "python",
            vec!["-c".into(), ok_py.into()],
            Duration::from_secs(5),
        );

        let req = SidecarRequest::crawl("https://example.com", 4096);
        let out = client.execute(&req).expect("roundtrip JSON deve funcionar");
        assert!(out.tainted_external);
        assert_eq!(out.id, req.id);
        assert_eq!(out.final_url.as_deref(), Some("https://example.com/ok"));
        assert!(out.safe_data_block.contains("Conteudo limpo"));

        // 3. Simula o sidecar sofrendo redirect HTTP 302 para 127.0.0.1 -> deve ser bloqueado na volta
        let redirect_py = concat!(
            "import json, sys; req = json.loads(sys.stdin.readline()); ",
            "print(json.dumps({",
            "'id': req['id'], 'ok': True, 'final_url': 'http://127.0.0.1:8080/internal', ",
            "'markdown': 'segredo'",
            "}), flush=True)"
        );
        let evil_redirect_client = SidecarClient::new(
            "python",
            vec!["-c".into(), redirect_py.into()],
            Duration::from_secs(5),
        );

        let err = evil_redirect_client
            .execute(&req)
            .expect_err("redirect para loopback deve ser bloqueado");
        assert!(matches!(err, SidecarError::BlockedUrl(_)));
    }
}

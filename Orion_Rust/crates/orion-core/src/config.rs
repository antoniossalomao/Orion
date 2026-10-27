//! Configuração do Orion: variáveis `ORION_*` e `.env` (o ambiente vence o `.env`).
//!
//! Os mesmos nomes, padrões e validações do Python (`orion/config.py`), para os dois lados lerem
//! o mesmo `.env` durante a transição.

use orion_ai::provedores::{parse_provedores, ProvedorConf};
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const PUBLICOS: [&str; 3] = ["0.0.0.0", "::", ""];
pub const LOCAIS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

static HHMM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([01]\d|2[0-3]):[0-5]\d$").unwrap());
static DND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([01]\d|2[0-3]):[0-5]\d-([01]\d|2[0-3]):[0-5]\d$").unwrap());
static CLAP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(abrir|abrir_e_ouvir|rotina:[a-z0-9_-]{1,32})$").unwrap());

#[derive(Debug, thiserror::Error)]
#[error("configuração inválida: {0}")]
pub struct ConfigError(pub String);

type R<T> = Result<T, ConfigError>;

fn erro<T>(m: impl Into<String>) -> R<T> {
    Err(ConfigError(m.into()))
}

/// Pasta de dados do usuário (`platformdirs.user_data_dir("orion", appauthor=False)`).
pub fn user_data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("orion")
}

/// Raiz do projeto (protegida pelo PathGuard): a pasta do repositório, quando rodando dele.
pub fn project_root() -> PathBuf {
    if let Ok(p) = std::env::var("ORION_PROJECT_ROOT") {
        return PathBuf::from(p);
    }
    // `Orion_Rust/crates/orion-core` → raiz do repositório
    let manifesto = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifesto.ancestors().nth(3).map(Path::to_path_buf).unwrap_or(manifesto)
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub host: String,
    pub port: u16,
    pub allow_public_bind: bool,
    pub data_dir: PathBuf,
    pub log_level: String,
    pub log_json: bool,
    /// Dias que o log cifrado fica em disco (ORION_LOG_RETENTION_DAYS).
    pub log_retention_days: u32,
    /// Bloqueio do Windows (Win+L) encerra as sessões e apaga a chave da memória (banco cifrado).
    pub lock_on_screen_lock: bool,
    /// Além disso, encerra o processo: o único jeito de o SQLCipher soltar a chave.
    pub lock_exit: bool,
    pub approval_ttl_s: u64,
    pub extra_safe_roots: Vec<PathBuf>,
    pub admin_token: String,
    pub auth_user: String,
    pub seed_default_password: bool,
    pub session_ttl_h: u64,
    pub cookie_secure: bool,
    pub audit_retention_days: i64,
    pub provedores: Vec<ProvedorConf>,
    pub providers_explicit: bool,
    pub gateway_url: String,
    pub gateway_model: String,
    pub gateway_model_fast: String,
    pub gateway_model_heavy: String,
    pub gateway_api_key: String,
    pub allowed_hosts: Vec<String>,
    pub skill_sources: Vec<Value>,
    pub mcp_connections: Vec<Value>,
    pub research: Value,
    pub desktop_tools: bool,
    pub web_tools: bool,
    /// ORION_NUVEM: tarefas na nuvem pelas CLIs oficiais (`/nuvem`). Opt-in.
    pub nuvem: bool,
    /// ORION_CODEX_ENV: ambiente do Codex na nuvem (`codex cloud`).
    pub codex_env: String,
    /// ORION_NUVEM_DIR: pasta (clone do repositório) de onde as tarefas na nuvem partem; vazio = pasta atual.
    pub nuvem_dir: String,
    pub vision_tools: bool,
    pub vision_model: String,
    pub camera: bool,
    pub camera_device: u32,
    pub camera_watch: bool,
    pub vision_order: String,
    pub object_model: Option<PathBuf>,
    pub face_model: Option<PathBuf>,
    pub translator_langs: String,
    pub presence_enabled: bool,
    pub presence_bt: String,
    pub presence_away_min: u32,
    pub presence_lock: bool,
    pub subtitles: bool,
    pub subtitle_model: Option<PathBuf>,
    pub subtitle_device: String,
    pub weather_city: String,
    pub search_api_key: String,
    pub search_model: String,
    pub brave_api_key: String,
    pub image_model: String,
    pub image_api_key: String,
    pub telegram_token: String,
    pub telegram_allowed_users: Vec<i64>,
    pub plugins_enabled: bool,
    pub plugins_dir: Option<PathBuf>,
    pub skills_enabled: bool,
    pub skills_dir: Option<PathBuf>,
    pub n8n_webhooks: String,
    pub mcp_enabled: bool,
    pub mcp_config: Option<PathBuf>,
    pub plugin_catalogs: String,
    pub mcp_index_at: usize,
    pub transcribe_api_key: String,
    pub transcribe_url: String,
    pub transcribe_model: String,
    pub voice_enabled: bool,
    pub local_stt_model: Option<PathBuf>,
    pub local_stt_gpu: bool,
    pub voice_barge_in: bool,
    pub voice_aec_delay_ms: i32,
    pub transcribe_groq_fallback: bool,
    pub telegram_2fa: bool,
    pub voice_speak: bool,
    pub voice_tts_voice: String,
    pub wake_enabled: bool,
    pub wake_engine: String,
    pub wake_model: Option<PathBuf>,
    pub wake_words: Vec<String>,
    pub wake_threshold: f64,
    pub wake_device: String,
    pub wake_beep: bool,
    pub wake_silence_s: f64,
    pub wake_max_s: f64,
    pub wake_max_per_hour: u32,
    pub clap_enabled: bool,
    pub clap_ratio: f64,
    pub clap_action: String,
    pub clap_max_per_hour: u32,
    pub voice_live_enabled: bool,
    pub voice_live_model: String,
    pub voice_live_voice: String,
    pub voice_live_api_key: String,
    pub voice_live_max_min: u32,
    pub jobs_enabled: bool,
    pub jobs_tick_s: f64,
    pub backup_dir: Option<PathBuf>,
    pub backup_keep: usize,
    pub vault_dir: Option<PathBuf>,
    /// ORION_VAULT_DIARIO: grava o diário de cada conversa no vault, sem commit nem push. Opt-in.
    pub vault_diario: bool,
    pub capture_folder: String,
    pub briefing_at: String,
    pub briefing_calendar_email: String,
    pub briefing_calendar_tool: String,
    pub repos: Vec<PathBuf>,
    pub clipboard_days: u32,
    pub command_history: bool,
    pub telegram_voice_reply: bool,
    pub screen_memory: bool,
    pub screen_interval_s: u64,
    pub screen_retention_days: i64,
    pub screen_exclude: String,
    pub screen_allow_unknown_title: bool,
    pub screen_ocr_langs: String,
    pub sleep_at: String,
    pub weekly_ai: bool,
    pub research_at: String,
    pub research_topics: String,
    pub consolidate: bool,
    pub quota_gateway_dia: i64,
    pub quota_groq_dia: i64,
    pub quota_gemini_dia: i64,
    pub quota_brave_mes: i64,
    pub allow_paid: bool,
    pub gateway_down_min: u32,
    pub dnd_at: String,
    pub diary_at: String,
    pub weekly_review_at: String,
    pub focus_distractions: String,
    pub hotkeys: String,
    pub local_model: String,
    pub local_model_path: Option<PathBuf>,
    pub local_model_gpu: bool,
    pub local_url: String,
    pub compact_at: usize,
    pub ocr_max_pages: u32,
    pub ocr_langs: String,
    pub rerank: bool,
    pub rerank_model_dir: String,
    pub screen_exclude_apps: String,
    pub embed_api_key: String,
    pub embed_model: String,
    pub embed_dim: usize,
}

/// Leitor das variáveis: o ambiente vence o `.env`.
struct Env(HashMap<String, String>);

impl Env {
    fn carregar(dotenv: Option<&Path>) -> Self {
        let mut m: HashMap<String, String> = HashMap::new();
        let arq = dotenv.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from(".env"));
        if let Ok(it) = dotenvy::from_path_iter(&arq) {
            for (k, v) in it.flatten() {
                m.insert(k.to_uppercase(), v);
            }
        }
        for (k, v) in std::env::vars() {
            m.insert(k.to_uppercase(), v);
        }
        // REGRAS.md: opt-in não liga sozinho. Só escolhas concluídas do orion-setup viram padrões.
        let installed = m.get("ORION_LOCAL_CONFIG").map(PathBuf::from).or_else(|| {
            let manifest = std::env::current_exe().ok()?.parent()?.join("orion-install.json");
            let raw = std::fs::read(manifest).ok()?;
            let value: Value = serde_json::from_slice(&raw).ok()?;
            Some(PathBuf::from(value["data_dir"].as_str()?).join("orion-local.json"))
        });
        if let Some(path) = installed {
            if let Ok(file) = std::fs::File::open(path) {
                use std::io::Read;
                let mut bytes = Vec::new();
                if file.take(65537).read_to_end(&mut bytes).is_ok() && bytes.len() <= 65536 {
                    if let Ok(Value::Object(values)) = serde_json::from_slice::<Value>(&bytes) {
                        for key in ["ORION_DATA_DIR", "ORION_LOCAL_MODEL_PATH", "ORION_LOCAL_MODEL_GPU", "ORION_LOCAL_STT_MODEL", "ORION_LOCAL_STT_GPU", "ORION_VOICE_ENABLED", "ORION_VOICE_BARGE_IN", "ORION_WAKE_ENABLED", "ORION_WAKE_MODEL", "ORION_WAKE_THRESHOLD"] {
                            if let Some(value) = values.get(key) {
                                let value = value.as_str().map(String::from).unwrap_or_else(|| value.to_string());
                                m.entry(key.into()).or_insert(value);
                            }
                        }
                    }
                }
            }
        }
        let dados = m.get("ORION_DATA_DIR").map(PathBuf::from).unwrap_or_else(user_data_dir);
        if let Some(p) = crate::onboarding::Profile::load(&dados).unwrap_or(None) {
            let mut values = vec![("ORION_AUTH_USER", p.user), ("ORION_SEED_DEFAULT_PASSWORD", "false".into()), ("ORION_BACKUP_KEEP", "7".into()), ("ORION_TELEGRAM_2FA", p.telegram_2fa.to_string()), ("ORION_TELEGRAM_ALLOWED_USERS", p.telegram_users.iter().map(i64::to_string).collect::<Vec<_>>().join(","))];
            for (key, path) in [("ORION_VAULT_DIR", p.vault), ("ORION_BACKUP_DIR", None::<PathBuf>)] {
                if let Some(path) = path { values.push((key, path.to_string_lossy().into_owned())); }
            }
            if let Some(path) = p.projects { values.push(("ORION_REPOS", serde_json::to_string(&vec![path]).unwrap())); }
            for (key, value) in values { m.entry(key.into()).or_insert(value); }
        }
        Env(m)
    }

    fn raw(&self, nome: &str) -> Option<&str> {
        self.0.get(&format!("ORION_{}", nome.to_uppercase())).map(String::as_str)
    }

    fn s(&self, nome: &str, padrao: &str) -> String {
        self.raw(nome).map(String::from).unwrap_or_else(|| padrao.into())
    }

    fn b(&self, nome: &str, padrao: bool) -> R<bool> {
        match self.raw(nome).map(|v| v.trim().to_lowercase()) {
            None => Ok(padrao),
            Some(v) => match v.as_str() {
                "1" | "true" | "t" | "yes" | "y" | "on" => Ok(true),
                "0" | "false" | "f" | "no" | "n" | "off" => Ok(false),
                _ => erro(format!("ORION_{}: booleano inválido", nome.to_uppercase())),
            },
        }
    }

    fn num<T: std::str::FromStr + PartialOrd + Copy + std::fmt::Display>(&self, nome: &str, padrao: T, min: Option<T>, max: Option<T>) -> R<T> {
        let v = match self.raw(nome) {
            None => padrao,
            Some(t) => t.trim().parse().map_err(|_| ConfigError(format!("ORION_{}: número inválido", nome.to_uppercase())))?,
        };
        if min.is_some_and(|m| v < m) || max.is_some_and(|m| v > m) {
            return erro(format!("ORION_{}: fora do intervalo", nome.to_uppercase()));
        }
        Ok(v)
    }

    /// Caminho opcional: vazio vira "não definido" (senão indexaria a pasta atual).
    fn path_opt(&self, nome: &str) -> Option<PathBuf> {
        self.raw(nome).map(str::trim).filter(|v| !v.is_empty()).map(PathBuf::from)
    }

    fn json_list(&self, nome: &str) -> R<Vec<Value>> {
        match self.raw(nome).map(str::trim).filter(|v| !v.is_empty()) {
            None => Ok(vec![]),
            Some(t) => serde_json::from_str(t).map_err(|e| ConfigError(format!("ORION_{}: {e}", nome.to_uppercase()))),
        }
    }

    fn str_list(&self, nome: &str, padrao: Vec<String>, sep: char) -> R<Vec<String>> {
        match self.raw(nome).map(str::trim) {
            None => Ok(padrao),
            Some(t) if t.starts_with('[') => serde_json::from_str(t).map_err(|e| ConfigError(format!("ORION_{}: {e}", nome.to_uppercase()))),
            Some(t) => Ok(t.split(sep).map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect()),
        }
    }
}

fn hora(nome: &str, v: String, msg: &str) -> R<String> {
    let v = v.trim().to_string();
    if !v.is_empty() && !HHMM.is_match(&v) {
        return erro(format!("{msg} ({nome})"));
    }
    Ok(v)
}

impl Settings {
    /// Lê o ambiente e o `.env` da pasta atual.
    pub fn load() -> R<Self> {
        Self::from_env(&Env::carregar(None))
    }

    pub fn load_from(dotenv: &Path) -> R<Self> {
        Self::from_env(&Env::carregar(Some(dotenv)))
    }

    fn from_env(e: &Env) -> R<Self> {
        let provedores = parse_provedores(&e.s("provedores", "")).map_err(ConfigError)?;
        let admin_token = e.s("admin_token", "");
        if !admin_token.is_empty() && admin_token.chars().count() < 16 {
            return erro("ORION_ADMIN_TOKEN precisa de 16+ caracteres");
        }
        let telegram_allowed_users: Vec<i64> = match e.raw("telegram_allowed_users").map(str::trim) {
            None | Some("") => vec![],
            Some(t) if t.starts_with('[') => serde_json::from_str(t).map_err(|e| ConfigError(format!("ORION_TELEGRAM_ALLOWED_USERS: {e}")))?,
            Some(t) => t
                .split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .map(|x| x.parse().map_err(|_| ConfigError("ORION_TELEGRAM_ALLOWED_USERS: id inválido".into())))
                .collect::<R<_>>()?,
        };
        let repos = match e.raw("repos").map(str::trim) {
            None | Some("") => vec![],
            Some(t) if t.starts_with('[') => {
                serde_json::from_str::<Vec<String>>(t).map_err(|e| ConfigError(format!("ORION_REPOS: {e}")))?.into_iter().map(PathBuf::from).collect()
            }
            Some(t) => t.replace('\n', ";").split(';').map(str::trim).filter(|x| !x.is_empty()).map(PathBuf::from).collect(),
        };
        let capture_folder = {
            let v = e.s("capture_folder", "00 Inbox").trim().to_string();
            let partes: Vec<&str> = v.split(['/', '\\']).collect();
            if v.is_empty() || v.starts_with(['/', '\\', '~']) || Path::new(&v).is_absolute() || partes.contains(&"..") || v.contains(':') {
                return erro("ORION_CAPTURE_FOLDER precisa ser uma pasta relativa do vault (ex.: 00 Inbox)");
            }
            v.trim_end_matches(['/', '\\']).to_string()
        };
        let dnd_at = e.s("dnd_at", "").trim().replace(' ', "");
        if !dnd_at.is_empty() && !DND.is_match(&dnd_at) {
            return erro("ORION_DND_AT precisa ser HH:MM-HH:MM (ex.: 22:30-07:00)");
        }
        let clap_action = e.s("clap_action", "abrir").trim().to_string();
        if !CLAP.is_match(&clap_action) {
            return erro("ORION_CLAP_ACTION: use abrir, abrir_e_ouvir ou rotina:<nome>");
        }
        let local_url = e.s("local_url", "http://127.0.0.1:11434/v1").trim().to_string();
        {
            // REGRAS.md: modelo local é reserva sem ferramentas; o endereço só aceita loopback
            let u = url::Url::parse(&local_url).map_err(|_| ConfigError("ORION_LOCAL_URL inválida".into()))?;
            let h = u.host_str().unwrap_or("").trim_matches(['[', ']']).to_string();
            if !matches!(u.scheme(), "http" | "https") || !LOCAIS.contains(&h.as_str()) {
                return erro("ORION_LOCAL_URL só aceita 127.0.0.1, localhost ou ::1 (o modelo local é deste computador)");
            }
        }
        let log_level = e.s("log_level", "WARNING").to_uppercase();
        if !["DEBUG", "INFO", "WARN", "WARNING", "ERROR", "CRITICAL"].contains(&log_level.as_str()) {
            return erro(format!("log_level inválido: {log_level}"));
        }
        let host = e.s("host", "127.0.0.1");
        let allow_public_bind = e.b("allow_public_bind", false)?;
        if PUBLICOS.contains(&host.as_str()) && !allow_public_bind {
            // REGRAS.md: bind só em 127.0.0.1
            return erro("host público recusado: o Orion só escuta em 127.0.0.1 (defina ORION_ALLOW_PUBLIC_BIND=true só se souber o que faz)");
        }
        let telegram_token = e.s("telegram_token", "");
        if !telegram_token.is_empty() && telegram_allowed_users.is_empty() {
            // REGRAS.md: Telegram com default-deny e aprovação só por botão
            return erro("ORION_TELEGRAM_TOKEN exige ORION_TELEGRAM_ALLOWED_USERS: sem lista de usuários o bot não sobe (default-deny)");
        }
        let wake_engine = e.s("wake_engine", "openwakeword");
        if !["openwakeword", "vosk"].contains(&wake_engine.as_str()) {
            return erro("ORION_WAKE_ENGINE: openwakeword ou vosk");
        }
        let research = match e.raw("research").map(str::trim).filter(|v| !v.is_empty()) {
            None => serde_json::json!({"enabled": false, "secret_ref": "ORION_RESEARCH_BRAVE_KEY"}),
            Some(t) => serde_json::from_str(t).map_err(|e| ConfigError(format!("ORION_RESEARCH: {e}")))?,
        };
        let data_dir = e.path_opt("data_dir").unwrap_or_else(user_data_dir);
        let auth_user = e.raw("auth_user").map(str::to_owned).or_else(|| {
            // Instalações anteriores preservam a conta existente (REGRAS.md: portabilidade).
            let conn = orion_store::rusqlite::Connection::open_with_flags(data_dir.join("auth.db"), orion_store::rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
            let mut statement = conn.prepare("SELECT user FROM credential LIMIT 2").ok()?;
            let users = statement.query_map([], |row| row.get::<_, String>(0)).ok()?.collect::<Result<Vec<_>, _>>().ok()?;
            (users.len() == 1).then(|| users[0].clone())
        }).unwrap_or_else(|| "admin".into());
        Ok(Settings {
            host,
            port: e.num("port", 8000u16, Some(1), None)?,
            allow_public_bind,
            data_dir,
            log_level,
            log_json: e.b("log_json", true)?,
            log_retention_days: e.num("log_retention_days", 30u32, Some(1), Some(3650))?,
            lock_on_screen_lock: e.b("lock_on_screen_lock", true)?,
            lock_exit: e.b("lock_exit", false)?,
            approval_ttl_s: e.num("approval_ttl_s", 600u64, Some(30), None)?,
            extra_safe_roots: e.str_list("extra_safe_roots", vec![], ',')?.into_iter().map(PathBuf::from).collect(),
            admin_token,
            auth_user,
            seed_default_password: e.b("seed_default_password", false)?,
            session_ttl_h: e.num("session_ttl_h", 168u64, Some(1), Some(24 * 90))?,
            cookie_secure: e.b("cookie_secure", false)?,
            audit_retention_days: e.num("audit_retention_days", 90i64, Some(1), None)?,
            providers_explicit: e.raw("provedores").is_some(),
            provedores,
            gateway_url: e.s("gateway_url", ""),
            gateway_model: e.s("gateway_model", ""),
            gateway_model_fast: e.s("gateway_model_fast", ""),
            gateway_model_heavy: e.s("gateway_model_heavy", ""),
            gateway_api_key: e.s("gateway_api_key", ""),
            allowed_hosts: e.str_list("allowed_hosts", vec!["127.0.0.1".into(), "localhost".into()], ',')?,
            skill_sources: e.json_list("skill_sources")?,
            mcp_connections: e.json_list("mcp_connections")?,
            research,
            desktop_tools: e.b("desktop_tools", false)?,
            web_tools: e.b("web_tools", false)?,
            nuvem: e.b("nuvem", false)?,
            codex_env: e.s("codex_env", ""),
            nuvem_dir: e.s("nuvem_dir", ""),
            vision_tools: e.b("vision_tools", false)?,
            vision_model: e.s("vision_model", ""),
            camera: e.b("camera", false)?,
            camera_device: e.num("camera_device", 0u32, Some(0), Some(16))?,
            camera_watch: e.b("camera_watch", false)?,
            vision_order: e.s("vision_order", "gemini,claude,codex,gemini_cli"),
            object_model: e.path_opt("object_model"),
            face_model: e.path_opt("face_model"),
            translator_langs: e.s("translator_langs", "por+eng+spa"),
            presence_enabled: e.b("presence_enabled", false)?,
            presence_bt: e.s("presence_bt", ""),
            presence_away_min: e.num("presence_away_min", 10u32, Some(1), Some(240))?,
            presence_lock: e.b("presence_lock", false)?,
            subtitles: e.b("subtitles", false)?,
            subtitle_model: e.path_opt("subtitle_model"),
            subtitle_device: e.s("subtitle_device", ""),
            weather_city: e.s("weather_city", "Marília"),
            search_api_key: e.s("search_api_key", ""),
            search_model: e.s("search_model", "gemini-2.5-flash"),
            brave_api_key: e.s("brave_api_key", ""),
            image_model: e.s("image_model", "gemini-2.5-flash-image"),
            image_api_key: e.s("image_api_key", ""),
            telegram_token,
            telegram_allowed_users,
            plugins_enabled: e.b("plugins_enabled", true)?,
            plugins_dir: e.path_opt("plugins_dir"),
            skills_enabled: e.b("skills_enabled", true)?,
            skills_dir: e.path_opt("skills_dir"),
            n8n_webhooks: e.s("n8n_webhooks", ""),
            mcp_enabled: e.b("mcp_enabled", true)?,
            mcp_config: e.path_opt("mcp_config"),
            plugin_catalogs: e.s("plugin_catalogs", ""),
            mcp_index_at: e.num("mcp_index_at", 30usize, Some(1), Some(1000))?,
            transcribe_api_key: e.s("transcribe_api_key", ""),
            transcribe_url: e.s("transcribe_url", "https://api.groq.com/openai/v1"),
            transcribe_model: e.s("transcribe_model", "whisper-large-v3-turbo"),
            voice_enabled: e.b("voice_enabled", false)?,
            local_stt_model: e.path_opt("local_stt_model"),
            local_stt_gpu: e.b("local_stt_gpu", false)?,
            voice_barge_in: e.b("voice_barge_in", false)?,
            voice_aec_delay_ms: e.num("voice_aec_delay_ms", 40i32, Some(0), Some(500))?,
            transcribe_groq_fallback: e.b("transcribe_groq_fallback", false)?,
            telegram_2fa: e.b("telegram_2fa", false)?,
            voice_speak: e.b("voice_speak", true)?,
            voice_tts_voice: e.s("voice_tts_voice", "pt-BR-AntonioNeural"),
            wake_enabled: e.b("wake_enabled", false)?,
            wake_engine,
            wake_model: e.path_opt("wake_model"),
            wake_words: e.str_list("wake_words", vec!["orion".into(), "órion".into()], ',')?,
            wake_threshold: e.num("wake_threshold", 0.5f64, Some(0.0001), Some(0.9999))?,
            wake_device: e.s("wake_device", ""),
            wake_beep: e.b("wake_beep", true)?,
            wake_silence_s: e.num("wake_silence_s", 1.0f64, Some(0.3), Some(5.0))?,
            wake_max_s: e.num("wake_max_s", 15.0f64, Some(3.0), Some(60.0))?,
            wake_max_per_hour: e.num("wake_max_per_hour", 30u32, Some(1), Some(600))?,
            clap_enabled: e.b("clap_enabled", false)?,
            clap_ratio: e.num("clap_ratio", 6.0f64, Some(2.0), Some(60.0))?,
            clap_action,
            clap_max_per_hour: e.num("clap_max_per_hour", 20u32, Some(1), Some(600))?,
            voice_live_enabled: e.b("voice_live_enabled", false)?,
            voice_live_model: e.s("voice_live_model", "gemini-2.5-flash-native-audio-latest"),
            voice_live_voice: e.s("voice_live_voice", "Charon"),
            voice_live_api_key: e.s("voice_live_api_key", ""),
            voice_live_max_min: e.num("voice_live_max_min", 20u32, Some(1), Some(60))?,
            jobs_enabled: e.b("jobs_enabled", true)?,
            jobs_tick_s: e.num("jobs_tick_s", 30.0f64, Some(1.0), None)?,
            backup_dir: e.path_opt("backup_dir"),
            backup_keep: e.num("backup_keep", 7usize, Some(1), None)?,
            vault_dir: e.path_opt("vault_dir"),
            vault_diario: e.b("vault_diario", false)?,
            capture_folder,
            briefing_at: hora("ORION_BRIEFING_AT", e.s("briefing_at", ""), "precisa estar no formato HH:MM (ex.: 07:30)")?,
            briefing_calendar_email: e.s("briefing_calendar_email", ""),
            briefing_calendar_tool: e.s("briefing_calendar_tool", "google__get_events"),
            repos,
            clipboard_days: e.num("clipboard_days", 0u32, Some(0), Some(90))?,
            command_history: e.b("command_history", false)?,
            telegram_voice_reply: e.b("telegram_voice_reply", false)?,
            screen_memory: e.b("screen_memory", false)?,
            screen_interval_s: e.num("screen_interval_s", 300u64, Some(60), Some(3600))?,
            screen_retention_days: e.num("screen_retention_days", 7i64, Some(1), Some(90))?,
            screen_exclude: e
                .s("screen_exclude", "senha;password;passwd;banco;bank;bitwarden;1password;keepass;lastpass;nubank;login;token;pix;cartão;cartao;cpf"),
            screen_allow_unknown_title: e.b("screen_allow_unknown_title", false)?,
            screen_ocr_langs: e.s("screen_ocr_langs", "por+eng"),
            sleep_at: e.s("sleep_at", ""),
            weekly_ai: e.b("weekly_ai", false)?,
            research_at: e.s("research_at", ""),
            research_topics: e.s(
                "research_topics",
                "MCP (Model Context Protocol) segurança e novidades; Pix e Open Finance (regras do Banco Central); LLMs locais e on-device",
            ),
            consolidate: e.b("consolidate", true)?,
            quota_gateway_dia: e.num("quota_gateway_dia", 1000i64, Some(0), None)?,
            quota_groq_dia: e.num("quota_groq_dia", 2000i64, Some(0), None)?,
            quota_gemini_dia: e.num("quota_gemini_dia", 1000i64, Some(0), None)?,
            quota_brave_mes: e.num("quota_brave_mes", 1000i64, Some(0), None)?,
            allow_paid: e.b("allow_paid", false)?,
            gateway_down_min: e.num("gateway_down_min", 10u32, Some(1), Some(1440))?,
            dnd_at,
            diary_at: hora("ORION_DIARY_AT", e.s("diary_at", ""), "ORION_DIARY_AT e ORION_WEEKLY_REVIEW_AT precisam ser HH:MM")?,
            weekly_review_at: hora("ORION_WEEKLY_REVIEW_AT", e.s("weekly_review_at", ""), "ORION_DIARY_AT e ORION_WEEKLY_REVIEW_AT precisam ser HH:MM")?,
            focus_distractions: e.s("focus_distractions", ""),
            hotkeys: e.s("hotkeys", "").trim().to_string(),
            local_model: e.s("local_model", ""),
            local_model_path: e.path_opt("local_model_path"),
            local_model_gpu: e.b("local_model_gpu", false)?,
            local_url,
            compact_at: e.num("compact_at", 40usize, Some(0), Some(1000))?,
            ocr_max_pages: e.num("ocr_max_pages", 50u32, Some(0), Some(200))?,
            ocr_langs: e.s("ocr_langs", "por+eng"),
            rerank: e.b("rerank", false)?,
            rerank_model_dir: e.s("rerank_model_dir", ""),
            screen_exclude_apps: e.s("screen_exclude_apps", "KeePassXC;1Password;Bitwarden;KeePass;LastPass;Authy"),
            embed_api_key: e.s("embed_api_key", ""),
            embed_model: e.s("embed_model", "gemini-embedding-001"),
            embed_dim: e.num("embed_dim", 768usize, Some(64), Some(3072))?,
        })
    }

    /// Hosts permitidos que não são locais (Tailscale, curinga): só com login
    /// (REGRAS.md: acesso de fora só com login).
    pub fn hosts_de_fora(&self) -> Vec<String> {
        self.allowed_hosts.iter().filter(|h| !LOCAIS.contains(&h.as_str())).cloned().collect()
    }

    /// Logs cifrados (só existem com o banco cifrado).
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("orion.db")
    }

    /// Fora do banco da memória: o backup vai para a nuvem e não leva o hash da senha.
    pub fn auth_db_path(&self) -> PathBuf {
        self.data_dir.join("auth.db")
    }

    pub fn effective_mcp_config(&self) -> PathBuf {
        self.mcp_config.clone().unwrap_or_else(|| self.data_dir.join("mcp.json"))
    }

    pub fn effective_plugins_dir(&self) -> PathBuf {
        self.plugins_dir.clone().unwrap_or_else(|| self.data_dir.join("plugins"))
    }

    pub fn effective_catalog_dir(&self) -> PathBuf {
        self.data_dir.join("catalogo")
    }

    pub fn effective_skills_dir(&self) -> PathBuf {
        self.skills_dir.clone().unwrap_or_else(|| self.data_dir.join("skills"))
    }

    pub fn effective_backup_dir(&self) -> PathBuf {
        self.backup_dir.clone().unwrap_or_else(|| self.data_dir.join("backups"))
    }

    pub fn quotas(&self) -> orion_ai::costs::Quotas {
        orion_ai::costs::Quotas {
            gateway_dia: self.quota_gateway_dia,
            groq_dia: self.quota_groq_dia,
            gemini_dia: self.quota_gemini_dia,
            brave_mes: self.quota_brave_mes,
        }
    }
}

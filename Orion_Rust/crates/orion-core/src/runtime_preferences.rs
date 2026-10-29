//! Configurações locais dos recursos nativos, sem segredos nem autorização do modelo.
//! REGRAS.md: opt-in não liga sozinho; portabilidade; toda rota da API exige login.
use crate::config::Settings;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub const FILE: &str = "runtime-preferences.json";
/// F07.03: foto dos valores da primeira execução, usada por "Restaurar padrão".
pub const DEFAULTS_FILE: &str = "runtime-defaults.json";

macro_rules! preferences {
    ($($field:ident: $kind:ty),* $(,)?) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(deny_unknown_fields)]
        pub struct RuntimePreferences { $(pub $field: $kind,)* }
        impl RuntimePreferences {
            pub fn from_settings(settings: &Settings) -> Self { Self { $($field: settings.$field.clone(),)* } }
            pub fn apply(&self, settings: &mut Settings) { $(settings.$field = self.$field.clone();)* }
        }
    }
}
preferences! {
    voice_enabled: bool, local_stt_model: Option<PathBuf>, local_stt_gpu: bool,
    voice_speak: bool, voice_tts_voice: String, voice_barge_in: bool,
    wake_enabled: bool, wake_model: Option<PathBuf>, wake_device: String, wake_threshold: f64,
    wake_max_s: f64, wake_silence_s: f64, wake_max_per_hour: u32,
    clap_enabled: bool, clap_ratio: f64, clap_action: String,
    voice_live_enabled: bool, voice_live_max_min: u32,
    vision_tools: bool, camera: bool, camera_device: u32, camera_watch: bool,
    presence_enabled: bool, presence_bt: String, presence_away_min: u32, presence_lock: bool,
    object_model: Option<PathBuf>, face_model: Option<PathBuf>,
    subtitles: bool, subtitle_model: Option<PathBuf>, subtitle_device: String,
    screen_memory: bool, screen_interval_s: u64, screen_retention_days: i64,
    screen_exclude: String, screen_exclude_apps: String, screen_ocr_langs: String,
    clipboard_days: u32, command_history: bool,
    jobs_enabled: bool, briefing_at: String, research_at: String, research_topics: String,
    sleep_at: String, weekly_ai: bool, consolidate: bool, diary_at: String, weekly_review_at: String,
    nuvem: bool, codex_env: String, nuvem_dir: String,
    skills_enabled: bool, skills_dir: Option<PathBuf>, plugins_enabled: bool, plugins_dir: Option<PathBuf>,
    mcp_enabled: bool, mcp_config: Option<PathBuf>,
    vault_dir: Option<PathBuf>, backup_dir: Option<PathBuf>, backup_keep: usize,
    rerank: bool, rerank_model_dir: String, extra_safe_roots: Vec<PathBuf>, repos: Vec<PathBuf>,
    hotkeys: String,
}

impl RuntimePreferences {
    pub fn load(data: &Path, settings: &Settings) -> anyhow::Result<Self> {
        let base = Self::from_settings(settings);
        match std::fs::read(data.join(FILE)) {
            Ok(bytes) => {
                anyhow::ensure!(
                    bytes.len() <= 65536,
                    "configuração de recursos muito grande"
                );
                base.updated(serde_json::from_slice(&bytes)?)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(base),
            Err(e) => Err(e.into()),
        }
    }

    pub fn updated(&self, changes: Value) -> anyhow::Result<Self> {
        let mut value = serde_json::to_value(self)?;
        let map = value.as_object_mut().expect("preferências são objeto");
        let changes = changes
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("configuração precisa ser um objeto"))?;
        for (key, value) in changes {
            anyhow::ensure!(map.contains_key(key), "configuração desconhecida: {key}");
            map.insert(key.clone(), value.clone());
        }
        let result: Self = serde_json::from_value(value)?;
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let value = serde_json::to_value(self)?;
        for (key, value) in value.as_object().unwrap() {
            if let Some(text) = value.as_str() {
                anyhow::ensure!(
                    text.len() <= 4096 && !text.contains('\0'),
                    "campo inválido: {key}"
                );
            }
            if let Some(items) = value.as_array() {
                anyhow::ensure!(
                    items.len() <= 50
                        && items
                            .iter()
                            .all(|v| v.as_str().is_some_and(|s| s.len() <= 4096)),
                    "lista inválida: {key}"
                );
            }
        }
        for (value, min, max, name) in [
            (self.wake_threshold, 0.0001, 0.9999, "wake_threshold"),
            (self.wake_max_s, 3.0, 60.0, "wake_max_s"),
            (self.wake_silence_s, 0.3, 5.0, "wake_silence_s"),
            (
                self.wake_max_per_hour as f64,
                1.0,
                600.0,
                "wake_max_per_hour",
            ),
            (self.clap_ratio, 2.0, 60.0, "clap_ratio"),
            (
                self.voice_live_max_min as f64,
                1.0,
                60.0,
                "voice_live_max_min",
            ),
            (self.camera_device as f64, 0.0, 16.0, "camera_device"),
            (
                self.presence_away_min as f64,
                1.0,
                240.0,
                "presence_away_min",
            ),
            (
                self.screen_interval_s as f64,
                60.0,
                3600.0,
                "screen_interval_s",
            ),
            (
                self.screen_retention_days as f64,
                1.0,
                90.0,
                "screen_retention_days",
            ),
            (self.clipboard_days as f64, 0.0, 90.0, "clipboard_days"),
            (self.backup_keep as f64, 1.0, 100.0, "backup_keep"),
        ] {
            anyhow::ensure!(
                value.is_finite() && (min..=max).contains(&value),
                "{name}: use um valor entre {min} e {max}"
            );
        }
        for time in [
            &self.briefing_at,
            &self.research_at,
            &self.sleep_at,
            &self.diary_at,
            &self.weekly_review_at,
        ] {
            if time.is_empty() {
                continue;
            }
            let parts: Vec<_> = time.split(':').collect();
            anyhow::ensure!(
                time.len() == 5
                    && parts.len() == 2
                    && parts[0].parse::<u8>().is_ok_and(|v| v < 24)
                    && parts[1].parse::<u8>().is_ok_and(|v| v < 60),
                "horário inválido: use HH:MM ou deixe vazio"
            );
        }
        anyhow::ensure!(
            self.clap_action == "abrir"
                || self.clap_action == "abrir_e_ouvir"
                || self
                    .clap_action
                    .strip_prefix("rotina:")
                    .is_some_and(|n| !n.is_empty()
                        && n.len() <= 32
                        && n.chars().all(|c| c.is_ascii_lowercase()
                            || c.is_ascii_digit()
                            || "_-".contains(c))),
            "ação de palmas inválida"
        );
        anyhow::ensure!(
            serde_json::to_vec_pretty(self)?.len() <= 64 * 1024,
            "configuração de recursos muito grande"
        );
        Ok(())
    }

    pub fn save(&self, data: &Path) -> anyhow::Result<()> {
        self.validate()?;
        let mut out = tempfile::NamedTempFile::new_in(data)?;
        out.write_all(&serde_json::to_vec_pretty(self)?)?;
        out.as_file().sync_all()?;
        out.persist(data.join(FILE)).map_err(|e| e.error)?;
        Ok(())
    }
}

/// Grava a foto dos valores da primeira execução; depois disso não mexe mais no arquivo.
pub fn save_defaults_once(data: &Path, prefs: &RuntimePreferences) -> anyhow::Result<()> {
    if data.join(DEFAULTS_FILE).exists() {
        return Ok(());
    }
    std::fs::create_dir_all(data)?;
    let mut out = tempfile::NamedTempFile::new_in(data)?;
    out.write_all(&serde_json::to_vec_pretty(prefs)?)?;
    out.as_file().sync_all()?;
    out.persist(data.join(DEFAULTS_FILE)).map_err(|e| e.error)?;
    Ok(())
}

/// Valores da foto da primeira execução, como JSON (mesmos campos de `RuntimePreferences`).
pub fn load_defaults(data: &Path) -> anyhow::Result<Value> {
    Ok(serde_json::from_slice(&std::fs::read(data.join(DEFAULTS_FILE))?)?)
}

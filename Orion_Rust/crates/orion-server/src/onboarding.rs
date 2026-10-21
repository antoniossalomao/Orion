//! Primeiro uso exclusivamente pela ligação nativa, antes de ligar agentes/jobs/canais.
//! REGRAS.md: aprovação só por canal autenticado; segredos nunca em log, audit ou processo filho.
use orion_core::{
    auth::{AuthService, SENHA_MAX, SENHA_MIN},
    config::Settings,
    dbkey,
    onboarding::{Profile, FILE},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

pub static BACKUP_KEY: parking_lot::Mutex<Option<dbkey::MasterKey>> = parking_lot::Mutex::new(None);

const PENDING: &str = "onboarding.pending.json";

/// Ausência de marcador nunca permite sobrescrever uma instalação existente.
pub fn needed(s: &Settings) -> anyhow::Result<bool> {
    if Profile::load(&s.data_dir)?.is_some() {
        return Ok(false);
    }
    if s.data_dir.join(PENDING).exists() {
        return Ok(true);
    }
    Ok(!s.db_path().exists() && !s.auth_db_path().exists() && !orion_core::dbopen::key_path(&s.db_path()).exists())
}

pub fn defaults(s: &Settings) -> Value {
    let vault = s.vault_dir.clone().or_else(orion_vault::detector::detect_vault_path);
    json!({"user":s.auth_user,"vault":vault,"encrypted_available":cfg!(feature="sqlcipher"),"hello_available":crate::onboarding_hello::available(),"backup":s.data_dir.join("backups"),"recovery":s.data_dir.join("Orion-recuperacao.txt"),"detected":crate::onboarding_imports::detected(),"google_client_id":std::env::var("ORION_GOOGLE_CLIENT_ID").unwrap_or_default()})
}

#[derive(Deserialize)]
pub struct Setup {
    pub user: String,
    pub name: String,
    pub language: String,
    pub password: String,
    pub confirmation: String,
    pub encrypted: bool,
    pub recovery: PathBuf,
    pub vault: String,
    pub projects: String,
    pub backup: String,
    #[serde(default)]
    pub telegram_users: String,
    #[serde(default)]
    pub telegram_2fa: bool,
    #[serde(default)]
    pub drive_backup: bool,
    #[serde(default)]
    pub hello: bool,
    #[serde(default)]
    pub imports: Vec<Value>,
    #[serde(default)]
    pub secrets: BTreeMap<String, String>,
}
impl Drop for Setup {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
        self.confirmation.zeroize();
        for value in self.secrets.values_mut() {
            value.zeroize();
        }
    }
}

const SECRET_NAMES: &[&str] = &[
    "ORION_KEY_GEMINI",
    "ORION_KEY_GROQ",
    "ORION_KEY_CEREBRAS",
    "ORION_KEY_OPENROUTER",
    "ORION_KEY_MISTRAL",
    "ORION_KEY_GITHUB",
    "ORION_KEY_NVIDIA",
    "ORION_KEY_ZAI",
    "ORION_BRAVE_API_KEY",
    "ORION_TELEGRAM_TOKEN",
    "ORION_GITHUB_TOKEN",
];

fn optional(s: &str) -> Option<PathBuf> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s.trim().into())
    }
}

impl Setup {
    fn profile(&self) -> anyhow::Result<Profile> {
        anyhow::ensure!(self.password == self.confirmation, "as senhas não coincidem");
        anyhow::ensure!(
            (SENHA_MIN..=SENHA_MAX).contains(&self.password.chars().count()),
            "a senha precisa ter entre {SENHA_MIN} e {SENHA_MAX} caracteres"
        );
        anyhow::ensure!(
            !self.encrypted || cfg!(feature = "sqlcipher"),
            "esta versão não tem SQLCipher; instale o pacote com cifra ou desmarque a opção explicitamente"
        );
        anyhow::ensure!(
            self.recovery.is_absolute() && self.recovery.file_name().is_some(),
            "escolha um arquivo de recuperação com caminho completo"
        );
        anyhow::ensure!(!self.recovery.exists(), "o arquivo de recuperação já existe; escolha outro nome para preservar o anterior");
        anyhow::ensure!(self.recovery.parent().is_some_and(Path::is_dir), "a pasta do arquivo de recuperação não existe");
        for (name, value) in &self.secrets {
            anyhow::ensure!(
                SECRET_NAMES.contains(&name.as_str()) && value.len() <= 16384 && !value.contains(['\r', '\n']),
                "chave de integração inválida"
            );
        }
        let ids: Result<Vec<i64>, _> = self.telegram_users.split([',', ';', ' ']).filter(|s| !s.is_empty()).map(str::parse).collect();
        let p = Profile {
            version: 1,
            user: self.user.trim().into(),
            name: self.name.trim().into(),
            language: self.language.clone(),
            vault: optional(&self.vault),
            projects: optional(&self.projects),
            backup: optional(&self.backup),
            telegram_users: ids.map_err(|_| anyhow::anyhow!("IDs do Telegram inválidos"))?,
            telegram_2fa: self.telegram_2fa,
            drive_backup: self.drive_backup,
            hello: self.hello,
        };
        p.validate()?;
        let recovery_parent = self.recovery.parent().ok_or_else(|| anyhow::anyhow!("pasta de recuperação inválida"))?.canonicalize()?;
        for folder in [&p.vault, &p.projects].into_iter().flatten() {
            anyhow::ensure!(
                !recovery_parent.starts_with(folder.canonicalize()?),
                "salve a recuperação fora do vault e dos projetos para não indexar o código secreto"
            );
        }
        anyhow::ensure!(
            !self.drive_backup || orion_ai::secrets::get_secret("ORION_DRIVE_OAUTH").is_some(),
            "conecte o Google Drive antes de ativar o backup remoto"
        );
        anyhow::ensure!(
            !p.telegram_2fa || self.secrets.get("ORION_TELEGRAM_TOKEN").is_some_and(|v| !v.is_empty()) || orion_ai::secrets::get_secret("ORION_TELEGRAM_TOKEN").is_some(),
            "o 2FA precisa do token do bot e de um ID autorizado"
        );
        Ok(p)
    }
}

fn create_file(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

/// Só prepara arquivos numa pasta privada; marcador de conclusão é publicado por último.
/// Falha durante promoção deixa um diário que permite concluir sem substituir dados existentes.
pub fn finish(s: &Settings, request: Value) -> anyhow::Result<Value> {
    anyhow::ensure!(needed(s)?, "o primeiro uso já foi concluído ou esta instalação já tem dados");
    if s.data_dir.join(PENDING).exists() {
        return resume(s);
    }
    let form: Setup = serde_json::from_value(request).map_err(|_| anyhow::anyhow!("formulário de primeiro uso inválido"))?;
    let profile = form.profile()?;
    anyhow::ensure!(form.imports.len() <= 32, "importe no máximo 32 arquivos nesta abertura");
    for import in &form.imports {
        let preview = crate::onboarding_imports::preview(import)?;
        anyhow::ensure!(preview["digest"] == import["digest"], "arquivo alterado; confira a prévia novamente");
    }

    if form.hello {
        crate::onboarding_hello::verify().map_err(anyhow::Error::msg)?;
    }
    if let Some(path) = &profile.backup {
        std::fs::create_dir_all(path)?;
    }
    anyhow::ensure!(
        !s.db_path().exists() && !s.auth_db_path().exists(),
        "instalação já existente; nenhuma credencial foi substituída"
    );
    std::fs::create_dir_all(&s.data_dir)?;
    let stage = tempfile::Builder::new().prefix("onboarding-").tempdir_in(&s.data_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(stage.path(), std::fs::Permissions::from_mode(0o700))?;
    }
    let keys = dbkey::create(&form.password, dbkey::Kdf::PLENO)?;
    let key_name = if form.encrypted { dbkey::KEY_FILE } else { "orion-recovery.key" };
    keys.file.save(&stage.path().join(key_name))?;
    {
        let auth = AuthService::open(&stage.path().join("auth.db"), &profile.user, s.session_ttl_h as f64 * 3600.0)?;
        auth.set_password(&form.password)?;
    }
    {
        #[cfg(feature = "sqlcipher")]
        let store = if form.encrypted {
            orion_store::Store::open_encrypted(&stage.path().join("orion.db"), &keys.master, None, orion_policy::wall_clock())?
        } else {
            orion_store::Store::open(&stage.path().join("orion.db"))?
        };
        #[cfg(not(feature = "sqlcipher"))]
        let store = orion_store::Store::open(&stage.path().join("orion.db"))?;
        store.conn().execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    }
    create_file(&stage.path().join(FILE), &serde_json::to_vec_pretty(&profile)?)?;
    // Cofre nativo: se falhar, não existe perfil concluído nem senha substituída.
    for (key, value) in &form.secrets {
        if !value.is_empty() {
            orion_ai::secrets::set_secret(key, value).map_err(|_| anyhow::anyhow!("não foi possível salvar a chave no cofre do sistema; tente novamente"))?;
        }
    }
    if form.hello {
        let secret = zeroize::Zeroizing::new(json!({"user":profile.user,"password":form.password}).to_string());
        orion_ai::secrets::set_secret("ORION_HELLO_LOGIN", &secret).map_err(|_| anyhow::anyhow!("não foi possível guardar a credencial Hello no cofre"))?;
    }
    let mut extra = vec![];
    for import in &form.imports {
        extra.extend(crate::onboarding_imports::prepare(import, stage.path(), profile.vault.as_deref())?);
    }
    extra.sort();
    extra.dedup();
    let mut files = vec!["auth.db".to_string(), "orion.db".to_string(), key_name.to_string()];
    files.extend(extra);
    files.push(FILE.into());
    let recovery = zeroize::Zeroizing::new(format!(
        "ORION — RECUPERAÇÃO\nUsuário: {}\nCódigo: {}\n\nGuarde este arquivo fora do computador. Nunca envie o código a outra pessoa.\n",
        profile.user, &*keys.recovery_code
    ));
    create_file(&form.recovery, recovery.as_bytes())?;
    let journal = json!({"stage":stage.path(),"files":files});
    create_file(&s.data_dir.join(PENDING), &serde_json::to_vec(&journal)?)?;
    // A pasta passa a pertencer ao diário; uma interrupção não apaga a preparação.
    let _path = stage.keep();
    resume(s)
}

fn resume(s: &Settings) -> anyhow::Result<Value> {
    let journal: Value = serde_json::from_slice(&std::fs::read(s.data_dir.join(PENDING))?)?;
    let stage = PathBuf::from(journal["stage"].as_str().ok_or_else(|| anyhow::anyhow!("diário de primeiro uso inválido"))?);
    anyhow::ensure!(
        stage.parent() == Some(s.data_dir.as_path()) && stage.file_name().is_some_and(|n| n.to_string_lossy().starts_with("onboarding-")),
        "pasta de preparação inválida"
    );
    let files = journal["files"].as_array().ok_or_else(|| anyhow::anyhow!("diário de primeiro uso inválido"))?;
    for name in files {
        let name = name.as_str().ok_or_else(|| anyhow::anyhow!("arquivo de preparação inválido"))?;
        anyhow::ensure!(
            ["auth.db", "orion.db", "orion.key", "orion-recovery.key", "extensions", "mcp-importado.json", FILE].contains(&name),
            "arquivo de preparação inválido"
        );
        let (from, to) = (stage.join(name), s.data_dir.join(name));
        if from.exists() {
            anyhow::ensure!(!to.exists(), "arquivo de destino já existe; preserve os dados e verifique a preparação");
            std::fs::rename(from, to)?;
        } else {
            anyhow::ensure!(to.exists(), "arquivo de preparação ausente");
        }
    }
    anyhow::ensure!(Profile::load(&s.data_dir)?.is_some(), "perfil ainda incompleto");
    std::fs::remove_file(s.data_dir.join(PENDING))?;
    let _ = std::fs::remove_dir(stage);
    Ok(json!({"configured":true}))
}

/// Testes só consultam endpoints oficiais; não geram texto, não ativam ferramentas e não logam chaves.
pub async fn test_connection(body: Value) -> Result<Value, String> {
    if body["provider"] == "import" {
        return crate::onboarding_imports::preview(&body).map_err(|e| e.to_string());
    }
    if body["provider"] == "drive" {
        return crate::onboarding_drive::connect(body).await.map_err(|e| e.to_string());
    }
    let provider = body["provider"].as_str().unwrap_or("");
    let key = zeroize::Zeroizing::new(body["key"].as_str().unwrap_or("").to_string());
    if key.is_empty() || key.len() > 16384 || key.contains(['\r', '\n']) {
        return Err("informe uma chave válida".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "não foi possível iniciar a conexão")?;
    let req = match provider {
        "google" => client.get("https://generativelanguage.googleapis.com/v1beta/models").header("x-goog-api-key", &*key),
        "groq" => client.get("https://api.groq.com/openai/v1/models").bearer_auth(&*key),
        "cerebras" => client.get("https://api.cerebras.ai/v1/models").bearer_auth(&*key),
        "openrouter" => client.get("https://openrouter.ai/api/v1/key").bearer_auth(&*key),
        "mistral" => client.get("https://api.mistral.ai/v1/models").bearer_auth(&*key),
        "github" => client.get("https://api.github.com/user").header("User-Agent", "Orion").bearer_auth(&*key),
        "nvidia" => client.get("https://integrate.api.nvidia.com/v1/models").bearer_auth(&*key),
        "zai" => client.get("https://api.z.ai/api/paas/v4/models").bearer_auth(&*key),
        "brave" => client
            .get("https://api.search.brave.com/res/v1/web/search")
            .query(&[("q", "Orion"), ("count", "1")])
            .header("X-Subscription-Token", &*key),
        "telegram" => {
            if !key.chars().all(|c| c.is_ascii_alphanumeric() || ":_-".contains(c)) {
                return Err("token do bot inválido".into());
            }
            client.get(format!("https://api.telegram.org/bot{}/getMe", &*key))
        }
        _ => return Err("integração desconhecida".into()),
    };
    let resp = req.send().await.map_err(|_| "não foi possível conectar; confira a rede e tente novamente")?;
    if !resp.status().is_success() {
        return Err(format!("a conexão foi recusada (HTTP {}); confira a chave e as permissões", resp.status().as_u16()));
    }
    Ok(json!({"ok":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primeiro_uso_nunca_substitui_instalacao_existente() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::load().unwrap();
        s.data_dir = dir.path().into();
        assert!(needed(&s).unwrap());
        std::fs::write(s.auth_db_path(), b"existente").unwrap();
        assert!(!needed(&s).unwrap());
    }
    #[test]
    fn arquivo_de_recuperacao_nunca_sobrescreve() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("recuperacao.txt");
        create_file(&p, b"primeiro").unwrap();
        assert!(create_file(&p, b"segundo").is_err());
        assert_eq!(std::fs::read(p).unwrap(), b"primeiro");
    }
    #[test]
    fn cria_conta_recuperacao_e_perfil_sem_senha_em_json() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::load().unwrap();
        s.data_dir = dir.path().join("dados");
        std::fs::create_dir(&s.data_dir).unwrap();
        let recovery = dir.path().join("recuperacao.txt");
        let form = json!({"user":"novo_usuario","name":"Antônio","language":"pt-BR","password":"senha-de-teste-123","confirmation":"senha-de-teste-123","encrypted":false,"recovery":recovery,"vault":"","projects":"","backup":"","telegram_users":"","telegram_2fa":false,"secrets":{},"imports":[]});
        finish(&s, form.clone()).unwrap();
        assert!(!needed(&s).unwrap());
        assert!(recovery.is_file());
        let profile = Profile::load(&s.data_dir).unwrap().unwrap();
        assert_eq!(profile.user, "novo_usuario");
        let auth = AuthService::open(&s.auth_db_path(), &profile.user, 3600.0).unwrap();
        assert!(auth.check_password("senha-de-teste-123"));
        assert!(!auth.uses_default_password());
        assert!(!String::from_utf8_lossy(&std::fs::read(s.data_dir.join(FILE)).unwrap()).contains("senha-de-teste-123"));
        assert!(finish(&s, form).is_err());
    }
    #[test]
    fn erro_de_validacao_nao_cria_banco_ou_recuperacao() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::load().unwrap();
        s.data_dir = dir.path().join("dados");
        std::fs::create_dir(&s.data_dir).unwrap();
        let recovery = dir.path().join("recuperacao.txt");
        let form = json!({"user":"novo","name":"Antônio","language":"pt-BR","password":"senha-de-teste-123","confirmation":"diferente","encrypted":false,"recovery":recovery,"vault":"","projects":"","backup":"","telegram_users":"","telegram_2fa":false,"secrets":{},"imports":[]});
        assert!(finish(&s, form).is_err());
        assert!(!s.db_path().exists());
        assert!(!s.auth_db_path().exists());
        assert!(!recovery.exists());
    }
    #[tokio::test]
    async fn teste_recusa_provedor_arbitrario_sem_enviar_segredo() {
        assert!(test_connection(json!({"provider":"https://externo","key":"chave-de-teste"})).await.is_err());
    }
}

/// A senha abre a chave de backup também quando a pessoa desmarca a cifra do banco.
pub fn unlock(s: &Settings, password: &str) -> anyhow::Result<()> {
    if Profile::load(&s.data_dir)?.is_some() {
        let path = if orion_core::dbopen::is_encrypted(&s.db_path()) {
            s.data_dir.join(dbkey::KEY_FILE)
        } else {
            s.data_dir.join("orion-recovery.key")
        };
        let key = dbkey::KeyFile::load(&path)?.unlock_password(password)?;
        *BACKUP_KEY.lock() = Some(key);
    }
    if orion_core::dbopen::is_encrypted(&s.db_path()) {
        orion_core::dbopen::unlock_with_password(&s.db_path(), password)?;
    }
    Ok(())
}

pub fn start_backup(s: &Settings, store: std::sync::Arc<orion_store::Store>, modos: std::sync::Arc<orion_core::modos::Modos>) -> Option<tokio::task::JoinHandle<()>> {
    let profile = Profile::load(&s.data_dir).ok().flatten()?;
    let folder = profile.backup.clone().or_else(|| profile.drive_backup.then(|| s.data_dir.join("backups")))?;
    if BACKUP_KEY.lock().is_none() {
        return None;
    }
    let data = s.data_dir.clone();
    Some(tokio::spawn(async move {
        loop {
            if modos.panico() {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }
            let Some(key) = BACKUP_KEY.lock().clone() else { break };
            let (db, data, folder) = (store.clone(), data.clone(), folder.clone());
            match tokio::task::spawn_blocking(move || orion_core::onboarding_backup::save(&db, &data, &folder, &key)).await {
                Ok(Ok(path)) if profile.drive_backup && !modos.panico() => {
                    if crate::onboarding_drive::upload(&path).await.is_err() {
                        tracing::warn!("backup local preservado; conexão com Google Drive precisa ser verificada");
                    }
                }
                Ok(Ok(_)) => {}
                _ => tracing::warn!("não foi possível concluir o backup local"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
        }
    }))
}

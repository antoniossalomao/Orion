//! Estado do primeiro uso. Usa os mesmos tokens/componentes do app; rascunhos com senhas
//! vivem só em memória e não entram em Debug (REGRAS.md: segredos nunca em log).
use serde_json::{json, Value};
use std::collections::BTreeMap;
use zeroize::Zeroize;

pub const PROVIDERS: &[(&str, &str, &str, &str)] = &[
    ("google", "Google AI Studio", "ORION_KEY_GEMINI", "https://aistudio.google.com/apikey"),
    ("groq", "Groq", "ORION_KEY_GROQ", "https://console.groq.com/keys"),
    ("cerebras", "Cerebras", "ORION_KEY_CEREBRAS", "https://cloud.cerebras.ai/platform/api-keys"),
    ("openrouter", "OpenRouter", "ORION_KEY_OPENROUTER", "https://openrouter.ai/settings/keys"),
    ("mistral", "Mistral", "ORION_KEY_MISTRAL", "https://console.mistral.ai/api-keys"),
    ("github", "GitHub Models", "ORION_KEY_GITHUB", "https://github.com/settings/tokens"),
    ("nvidia", "NVIDIA NIM", "ORION_KEY_NVIDIA", "https://build.nvidia.com"),
    ("zai", "Z.ai", "ORION_KEY_ZAI", "https://open.bigmodel.cn/usercenter/proj-mgmt/apikeys"),
    ("brave", "Brave Search", "ORION_BRAVE_API_KEY", "https://api.search.brave.com/app/keys"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    User,
    Name,
    Language,
    Password,
    Confirmation,
    Recovery,
    Vault,
    Projects,
    Backup,
    TelegramUsers,
    DriveClient,
    DriveSecret,
    ImportPath,
    ImportKind,
    ImportSource,
    ImportProject,
    Secret(&'static str),
}
#[derive(Clone)]
pub enum Event {
    Input(Field, String),
    Encrypt(bool),
    Hello(bool),
    Telegram2fa(bool),
    Save,
    ConnectDrive,
    PreviewImport,
    UseFound(usize),
    RemoveImport(usize),
    DisconnectDrive,
    Saved(Result<Value, String>),
    Test(&'static str, &'static str),
    Tested(&'static str, Result<Value, String>),
    Link(&'static str),
    Choose(Field),
    Chosen(Field, Result<Option<String>, String>),
}
impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrimeiroUso([conteúdo oculto])")
    }
}

pub struct Form {
    pub user: String,
    pub name: String,
    pub language: String,
    pub password: String,
    pub confirmation: String,
    pub recovery: String,
    pub vault: String,
    pub projects: String,
    pub backup: String,
    pub telegram_users: String,
    pub drive_client: String,
    pub drive_secret: String,
    pub drive_connected: bool,
    pub import_path: String,
    pub import_kind: String,
    pub import_source: String,
    pub import_project: String,
    pub imports: Vec<Value>,
    pub detected: Vec<Value>,
    pub encrypted: bool,
    pub hello: bool,
    pub hello_available: bool,
    pub cipher_available: bool,
    pub telegram_2fa: bool,
    pub secrets: BTreeMap<String, String>,
    pub busy: bool,
    pub error: String,
    pub testing: Option<&'static str>,
    pub tests: BTreeMap<String, String>,
}
impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrimeiroUso([segredos ocultos])")
    }
}
impl Drop for Form {
    fn drop(&mut self) {
        self.drive_secret.zeroize();
        self.password.zeroize();
        self.confirmation.zeroize();
        for s in self.secrets.values_mut() {
            s.zeroize();
        }
    }
}
impl Form {
    pub fn new(v: Value) -> Self {
        let text = |k: &str| v[k].as_str().unwrap_or("").to_string();
        Self {
            user: text("user"),
            name: String::new(),
            language: "pt-BR".into(),
            password: String::new(),
            confirmation: String::new(),
            recovery: text("recovery"),
            vault: text("vault"),
            projects: String::new(),
            backup: text("backup"),
            telegram_users: String::new(),
            drive_client: text("google_client_id"),
            drive_secret: String::new(),
            drive_connected: false,
            import_path: String::new(),
            import_kind: "history".into(),
            import_source: "Claude".into(),
            import_project: "Orion".into(),
            imports: vec![],
            detected: v["detected"].as_array().cloned().unwrap_or_default(),
            encrypted: true,
            hello: false,
            hello_available: v["hello_available"] == true,
            cipher_available: v["encrypted_available"] == true,
            telegram_2fa: false,
            secrets: BTreeMap::new(),
            busy: false,
            error: String::new(),
            testing: None,
            tests: BTreeMap::new(),
        }
    }
    pub fn get(&self, f: Field) -> &str {
        match f {
            Field::User => &self.user,
            Field::Name => &self.name,
            Field::Language => &self.language,
            Field::Password => &self.password,
            Field::Confirmation => &self.confirmation,
            Field::Recovery => &self.recovery,
            Field::Vault => &self.vault,
            Field::Projects => &self.projects,
            Field::Backup => &self.backup,
            Field::TelegramUsers => &self.telegram_users,
            Field::DriveClient => &self.drive_client,
            Field::DriveSecret => &self.drive_secret,
            Field::ImportPath => &self.import_path,
            Field::ImportKind => &self.import_kind,
            Field::ImportSource => &self.import_source,
            Field::ImportProject => &self.import_project,
            Field::Secret(k) => self.secrets.get(k).map(String::as_str).unwrap_or(""),
        }
    }
    pub fn input(&mut self, f: Field, s: String) {
        self.error.clear();
        let slot = match f {
            Field::User => &mut self.user,
            Field::Name => &mut self.name,
            Field::Language => &mut self.language,
            Field::Password => &mut self.password,
            Field::Confirmation => &mut self.confirmation,
            Field::Recovery => &mut self.recovery,
            Field::Vault => &mut self.vault,
            Field::Projects => &mut self.projects,
            Field::Backup => &mut self.backup,
            Field::TelegramUsers => &mut self.telegram_users,
            Field::DriveClient => &mut self.drive_client,
            Field::DriveSecret => &mut self.drive_secret,
            Field::ImportPath => &mut self.import_path,
            Field::ImportKind => &mut self.import_kind,
            Field::ImportSource => &mut self.import_source,
            Field::ImportProject => &mut self.import_project,
            Field::Secret(k) => {
                self.tests.remove(k);
                self.secrets.entry(k.into()).or_default()
            }
        };
        slot.zeroize();
        *slot = s;
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.user.trim().is_empty() || self.name.trim().is_empty() {
            return Err("Preencha o usuário e como o Orion deve chamar você.".into());
        }
        if !(12..=256).contains(&self.password.chars().count()) {
            return Err("Use uma senha de 12 a 256 caracteres.".into());
        }
        if self.password != self.confirmation {
            return Err("As senhas não coincidem.".into());
        }
        if self.encrypted && !self.cipher_available {
            return Err("Esta versão não inclui cifra do banco. Instale o pacote completo ou desmarque a opção explicitamente.".into());
        }
        if self.recovery.trim().is_empty() {
            return Err("Escolha onde salvar o arquivo de recuperação.".into());
        }
        if self.telegram_2fa && (self.telegram_users.trim().is_empty() || self.get(Field::Secret("ORION_TELEGRAM_TOKEN")).is_empty()) {
            return Err("Para usar 2FA, preencha o token do bot e seu ID do Telegram.".into());
        }
        Ok(())
    }
    pub fn take_request(&mut self) -> Value {
        // Preserva o rascunho em memória se a gravação falhar. Drop limpa os segredos após concluir/cancelar.
        json!({"user":self.user,"name":self.name,"language":self.language,"password":self.password,"confirmation":self.confirmation,"encrypted":self.encrypted,"hello":self.hello,"recovery":self.recovery,"vault":self.vault,"projects":self.projects,"backup":self.backup,"telegram_users":self.telegram_users,"telegram_2fa":self.telegram_2fa,"drive_backup":self.drive_connected,"secrets":self.secrets,"imports":self.imports})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cifra_e_optins_tem_padrao_conservador() {
        let f = Form::new(json!({}));
        assert!(f.encrypted);
        assert!(!f.telegram_2fa);
        assert!(f.secrets.is_empty());
    }
    #[test]
    fn formulario_nao_envia_senhas_divergentes() {
        let mut f = Form::new(json!({"encrypted_available":true}));
        f.user = "antonio".into();
        f.name = "Antônio".into();
        f.password = "senha-forte-de-teste".into();
        f.recovery = "arquivo".into();
        assert!(f.validate().is_err());
        f.confirmation = f.password.clone();
        assert!(f.validate().is_ok());
        let v = f.take_request();
        assert_eq!(f.password, "senha-forte-de-teste");
        assert_eq!(v["name"], "Antônio");
    }
    #[test]
    fn debug_nunca_mostra_chave() {
        let e = Event::Input(Field::Password, "segredo-teste".into());
        assert!(!format!("{e:?}").contains("segredo-teste"));
    }
}

impl crate::app::App {
    pub fn update_onboarding(&mut self, event: Event) -> iced::Task<crate::app::Msg> {
        use crate::app::Msg;
        use iced::Task;
        let Some(form) = self.onboarding.as_mut() else { return Task::none() };
        match event {
            Event::Input(field, text) if !form.busy && form.testing.is_none() => form.input(field, text),
            Event::Encrypt(value) if !form.busy => form.encrypted = value,
            Event::Hello(value) if !form.busy && form.hello_available => form.hello = value,
            Event::Telegram2fa(value) if !form.busy => form.telegram_2fa = value,
            Event::UseFound(i) if !form.busy && form.testing.is_none() => {
                if let Some(v) = form.detected.get(i) {
                    form.import_path = v["path"].as_str().unwrap_or("").into();
                    form.import_kind = v["kind"].as_str().unwrap_or("").into();
                    form.import_source = v["source"].as_str().unwrap_or("").into();
                }
            }
            Event::PreviewImport if !form.busy && form.testing.is_none() => {
                let Some(api) = self.api.clone() else { return Task::none() };
                let body = json!({"provider":"import","path":form.import_path,"kind":form.import_kind});
                form.testing = Some("import");
                return Task::perform(async move { api.testar_config(body).await }, |r| Msg::Onboarding(Event::Tested("import", r)));
            }
            Event::RemoveImport(i) if !form.busy => {
                if i < form.imports.len() {
                    form.imports.remove(i);
                }
            }
            Event::ConnectDrive if !form.busy && form.testing.is_none() => {
                let Some(api) = self.api.clone() else { return Task::none() };
                let body = json!({"provider":"drive","client_id":form.drive_client,"client_secret":form.drive_secret});
                form.testing = Some("drive");
                form.tests
                    .insert("drive".into(), "Conclua a autorização no navegador. Você pode cancelar fechando a aba.".into());
                return Task::perform(async move { api.testar_config(body).await }, |r| Msg::Onboarding(Event::Tested("drive", r)));
            }
            Event::DisconnectDrive => form.drive_connected = false,
            Event::Save if !form.busy => {
                if let Err(e) = form.validate() {
                    form.error = e;
                    return Task::none();
                }
                let Some(api) = self.api.clone() else {
                    form.error = "A prévia não cria contas nem grava arquivos.".into();
                    return Task::none();
                };
                form.busy = true;
                form.error.clear();
                let body = form.take_request();
                return Task::perform(async move { api.configurar(body).await }, |r| Msg::Onboarding(Event::Saved(r)));
            }
            Event::Saved(Ok(_)) => {
                self.p.name = form.name.clone();
                self.p.call = form.name.clone();
                self.p.english = form.language == "en";
                self.user = form.user.clone();
                self.onboarding = None;
                self.logged = true;
                self.gate_err.clear();
                self.toast("Tudo pronto. Seu Orion está configurado.");
                if let Some(api) = self.api.clone() {
                    return crate::live::pull_basic(&api);
                }
            }
            Event::Saved(Err(e)) => {
                form.busy = false;
                form.error = e;
            }
            Event::Test(provider, key) if !form.busy && form.testing.is_none() => {
                let Some(api) = self.api.clone() else { return Task::none() };
                let value = form.get(Field::Secret(key)).to_string();
                form.testing = Some(key);
                form.tests.remove(key);
                return Task::perform(async move { api.testar_config(json!({"provider":provider,"key":value})).await }, move |r| {
                    Msg::Onboarding(Event::Tested(key, r))
                });
            }
            Event::Tested(key, result) => {
                if key == "import" {
                    if let Ok(v) = &result {
                        if !form.imports.iter().any(|i| i["digest"] == v["digest"] && i["kind"] == form.import_kind) {
                            form.imports.push(json!({"path":form.import_path,"kind":form.import_kind,"source":form.import_source,"project":form.import_project,"digest":v["digest"],"count":v["count"],"description":v["description"]}));
                        }
                    }
                }
                if key == "drive" {
                    form.drive_connected = result.is_ok();
                    if form.drive_connected {
                        form.drive_secret.zeroize();
                    }
                }
                form.testing = None;
                form.tests.insert(
                    key.into(),
                    match result {
                        Ok(_) => "Conexão verificada".into(),
                        Err(e) => e,
                    },
                );
            }
            Event::Choose(field) if !form.busy => {
                let selector = if field == Field::ImportPath && form.import_kind == "skills" {
                    Field::Vault
                } else {
                    field
                };
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || choose(selector))
                            .await
                            .map_err(|_| "Não foi possível abrir o seletor.".to_string())
                            .and_then(|r| r)
                    },
                    move |r| Msg::Onboarding(Event::Chosen(field, r)),
                );
            }
            Event::Chosen(field, Ok(Some(path))) => form.input(field, path),
            Event::Chosen(_, Err(e)) => form.error = e,
            Event::Link(url) => {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || open_link(url)).await.ok();
                    },
                    |_| Msg::Noop,
                );
            }
            _ => {}
        }
        Task::none()
    }
}

/// Processos de interface não recebem chaves ORION_* ou credenciais do ambiente.
pub fn system_command(program: &str) -> std::process::Command {
    let mut c = std::process::Command::new(program);
    c.env_clear();
    for key in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "HOME",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_RUNTIME_DIR",
        "LANG",
    ] {
        if let Some(value) = std::env::var_os(key) {
            c.env(key, value);
        }
    }
    c
}

fn open_link(url: &str) {
    #[cfg(windows)]
    let _ = system_command("rundll32.exe").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "linux")]
    let _ = system_command("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = system_command("open").arg(url).spawn();
}

fn choose(field: Field) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = if field == Field::ImportPath {
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='Exports e plugins (*.json;*.jsonl;*.zip;*.toml)|*.json;*.jsonl;*.zip;*.toml'; if($d.ShowDialog() -eq 'OK'){[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.FileName)}"
        } else if field == Field::Recovery {
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.SaveFileDialog; $d.Title='Salvar recuperação do Orion'; $d.Filter='Arquivo de texto (*.txt)|*.txt'; $d.FileName='Orion-recuperacao.txt'; if($d.ShowDialog() -eq 'OK'){[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.FileName)}"
        } else {
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description='Escolha uma pasta para o Orion'; if($d.ShowDialog() -eq 'OK'){[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.SelectedPath)}"
        };
        let output = system_command("powershell.exe")
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-STA", "-Command", script])
            .output()
            .map_err(|_| "Não foi possível abrir o seletor; digite o caminho completo.".to_string())?;
        if !output.status.success() {
            return Err("Não foi possível abrir o seletor; digite o caminho completo.".into());
        }
        let path = String::from_utf8_lossy(&output.stdout).trim().trim_start_matches('\u{feff}').to_string();
        Ok(if path.is_empty() { None } else { Some(path) })
    }
    #[cfg(not(windows))]
    {
        let _ = field;
        Err("Digite o caminho completo. O seletor nativo está disponível no Windows.".into())
    }
}

impl crate::app::App {
    pub fn sync_profile(&mut self) {
        if let Some(p) = self.api.as_ref().and_then(|a| a.perfil()) {
            self.p.call = p["name"].as_str().unwrap_or("").into();
            self.p.name = self.p.call.clone();
            self.p.english = p["language"] == "en";
            self.user = p["user"].as_str().unwrap_or("").into();
        }
    }
}

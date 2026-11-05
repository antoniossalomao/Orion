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

/// F02.26: política de treino dos provedores. Conferida em 29/10/2026 em fontes de terceiros;
/// confirmar no site oficial do provedor antes de confiar nela. "Não verificado" = sem fonte confiável ainda.
pub const TRAINING_POLICY: &[(&str, &str)] = &[
    ("google", "Plano gratuito: o Google usa os dados para melhorar produtos (documentação oficial). Plano pago: não."),
    ("groq", "A documentação oficial não fala de treino; diz só que não retém por padrão. Não confirmado."),
    ("mistral", "Vibe e modo gratuito do Studio treinam por padrão, com opt-out. Pay-as-you-go: não informado."),
    ("openrouter", "O OpenRouter só usa seus dados se você ativar. O treino depende do provedor roteado; controle de roteamento não confirmado."),
    ("cerebras", "Não confirmado: a política não cita a API diretamente."),
    ("github", "Não confirmado. A página oficial diz que o serviço foi descontinuado em 30/07/2026."),
    ("nvidia", "Não confirmado: não encontrei a política de dados nas páginas oficiais consultadas."),
    ("zai", "Não confirmado: não encontrei a política nas páginas oficiais consultadas."),
    ("brave", "Consultas não são usadas para treino. Resultados dependem do plano."),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    ResourceFile,
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
    Attachment,
    ImportKind,
    ImportSource,
    ImportProject,
    Secret(&'static str),
}

/// Etapas do assistente: conta, segurança, notas, chaves, Telegram e backup, importações.
pub const STEPS: usize = 6;
#[derive(Clone)]
pub enum Event {
    AuthorizeTools(bool),
    Input(Field, String),
    Encrypt(bool),
    Hello(bool),
    Autostart(bool),
    Telegram2fa(bool),
    Save,
    Next,
    Back,
    Finish,
    ConnectDrive,
    CancelDrive,
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

/// Quadro estático na resolução do painel com GPU; software usa resolução reduzida
/// (REGRAS.md: portabilidade). Só muda ao redimensionar ou trocar de etapa.
fn sky_px(win: iced::Size) -> (u32, u32) {
    let hardware = std::env::var("ICED_BACKEND").is_ok_and(|backend| backend == "wgpu");
    let scale = if hardware { 1.0 } else { 1.6 };
    let (w, h) = ((win.width - 761.0).max(200.0) / scale, win.height.max(300.0) / scale);
    let k = ((if hardware { 1600.0 } else { 800.0 }) / w.max(h)).min(1.0);
    ((w * k) as u32, (h * k) as u32)
}

pub struct Form {
    pub authorize_tools: bool,
    pub step: usize,
    pub step_at: std::time::Instant,
    /// Céu do instalador: uma estrela acende por tela (7 telas, 7 estrelas).
    pub sky: orion_sky::OrionSky,
    pub sky_img: iced::widget::image::Handle,
    pub sky_at: std::time::Instant,
    pub drive_abort: Option<iced::task::Handle>,
    /// Código de recuperação mostrado uma vez, depois do envio; vive só em memória.
    pub code: Option<String>,
    pub warnings: usize,
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
    pub autostart: bool,
    pub autostart0: bool,
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
        if let Some(c) = self.code.as_mut() {
            c.zeroize();
        }
        self.password.zeroize();
        self.confirmation.zeroize();
        for s in self.secrets.values_mut() {
            s.zeroize();
        }
    }
}
impl Form {
    /// Avança o céu até `now` (60 quadros por segundo com GPU, 30 na CPU) e acende a estrela da tela atual.
    pub fn tick_sky(&mut self, now: std::time::Instant, win: iced::Size) {
        let dt = now.saturating_duration_since(self.sky_at).as_secs_f32();
        // Com GPU o céu acompanha a tela (60 quadros); no renderizador por CPU fica em 30.
        let hardware = std::env::var("ICED_BACKEND").is_ok_and(|backend| backend == "wgpu");
        if dt < if hardware { 0.015 } else { 0.03 } {
            return;
        }
        self.sky_at = now;
        let (w, h) = sky_px(win);
        self.sky.set_size(w, h);
        self.sky.light_to(self.step + 1);
        self.sky.step_dt(dt, 0.0, 0.0);
        self.sky_img = self.sky.render_image_handle();
    }

    /// Para as capturas: deixa o céu já assentado na etapa `step`.
    pub fn settle_sky(&mut self, step: usize, win: iced::Size) {
        let (w, h) = sky_px(win);
        self.sky.set_size(w, h);
        self.sky.light_to(step + 1);
        for _ in 0..160 {
            self.sky.step_dt(0.05, 0.0, 0.0);
        }
        self.sky_img = self.sky.render_image_handle();
    }

    pub fn new(v: Value) -> Self {
        let text = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let mut sky = orion_sky::OrionSky::new();
        let (sw, sh) = sky_px(iced::Size::new(1440.0, 900.0));
        sky.set_size(sw, sh);
        sky.one_star_per_step();
        sky.compact_constellation(0.62);
        sky.light_to(1);
        sky.step_dt(0.016, 0.0, 0.0);
        let sky_img = sky.render_image_handle();
        Self {
            authorize_tools: false,
            step: 0,
            step_at: std::time::Instant::now(),
            sky,
            sky_img,
            sky_at: std::time::Instant::now(),
            drive_abort: None,
            code: None,
            warnings: 0,
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
            autostart: v["autostart"] == true,
            autostart0: v["autostart"] == true,
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
            Field::Attachment | Field::ResourceFile => "",
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
            Field::Attachment | Field::ResourceFile => return,
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
    /// Valida só a etapa pedida; o envio final confere todas.
    pub fn validate_step(&self, step: usize) -> Result<(), String> {
        match step {
            0 if self.user.trim().is_empty() || self.name.trim().is_empty() => {
                Err("Preencha o usuário e como o Orion deve chamar você.".into())
            }
            1 if !(12..=256).contains(&self.password.chars().count()) => Err("Use uma senha de 12 a 256 caracteres.".into()),
            1 if self.password != self.confirmation => Err("As senhas não coincidem.".into()),
            1 if self.encrypted && !self.cipher_available => {
                Err("Esta versão não inclui cifra do banco. Instale o pacote completo ou desmarque a opção explicitamente.".into())
            }
            4 if self.telegram_2fa
                && (self.telegram_users.trim().is_empty() || self.get(Field::Secret("ORION_TELEGRAM_TOKEN")).is_empty()) =>
            {
                Err("Para usar 2FA, preencha o token do bot e seu ID do Telegram.".into())
            }
            _ => Ok(()),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        (0..STEPS).try_for_each(|s| self.validate_step(s))
    }
    pub fn take_request(&mut self) -> Value {
        // Preserva o rascunho em memória se a gravação falhar. Drop limpa os segredos após concluir/cancelar.
        json!({"user":self.user,"name":self.name,"language":self.language,"password":self.password,"confirmation":self.confirmation,"encrypted":self.encrypted,"hello":self.hello,"recovery":self.recovery,"vault":self.vault,"projects":self.projects,"backup":self.backup,"telegram_users":self.telegram_users,"telegram_2fa":self.telegram_2fa,"drive_backup":self.drive_connected,"secrets":self.secrets,"imports":self.imports,"authorize_tools":self.authorize_tools})
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
    fn etapas_validam_na_ordem_e_o_envio_confere_tudo() {
        let mut f = Form::new(json!({"encrypted_available":true}));
        assert!(f.validate_step(0).is_err());
        f.user = "antonio".into();
        f.name = "Antônio".into();
        assert!(f.validate_step(0).is_ok());
        f.password = "senha-forte-de-teste".into();
        f.confirmation = "outra-senha-de-teste".into();
        assert!(f.validate_step(1).is_err());
        f.confirmation = f.password.clone();
        assert!(f.validate().is_ok());
        f.telegram_2fa = true;
        assert!(f.validate_step(4).is_err());
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
            Event::Next if !form.busy && form.testing.is_none() => match form.validate_step(form.step) {
                Ok(()) if form.step + 1 < STEPS => {
                    form.error.clear();
                    form.step += 1;
                    form.settle_sky(form.step, self.win);
                    form.step_at = std::time::Instant::now();
                }
                Ok(()) => {}
                Err(e) => form.error = e,
            },
            Event::Back if !form.busy && form.testing.is_none() && form.step > 0 => {
                form.error.clear();
                form.step -= 1;
                form.settle_sky(form.step, self.win);
                form.step_at = std::time::Instant::now();
            }
            Event::CancelDrive => {
                if let Some(h) = form.drive_abort.take() {
                    h.abort();
                }
                form.testing = None;
                form.tests.remove("drive");
            }
            Event::Input(field, text) if !form.busy && form.testing.is_none() => form.input(field, text),
            Event::Encrypt(value) if !form.busy => form.encrypted = value,
            Event::AuthorizeTools(value) if !form.busy => form.authorize_tools = value,
            Event::Autostart(value) if !form.busy => form.autostart = value,
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
                form.tests.remove("drive");
                let (task, handle) = Task::perform(async move { api.testar_config(body).await }, |r| Msg::Onboarding(Event::Tested("drive", r))).abortable();
                form.drive_abort = Some(handle);
                return task;
            }
            Event::DisconnectDrive => form.drive_connected = false,
            Event::Save if !form.busy && form.step + 1 == STEPS => {
                form.error.clear();
                if let Err(e) = form.validate() {
                    form.error = e;
                    return Task::none();
                }
                let Some(api) = self.api.clone() else {
                    form.error = "A prévia não cria contas nem grava arquivos.".into();
                    return Task::none();
                };
                form.busy = true;
                self.loading = Some("Criando sua conta…".into());
                form.error.clear();
                let body = form.take_request();
                return Task::perform(async move { api.configurar(body).await }, |r| Msg::Onboarding(Event::Saved(r)));
            }
            Event::Saved(Ok(done)) => {
                self.loading = None;
                form.busy = false;
                form.password.zeroize();
                form.confirmation.zeroize();
                form.code = done["recovery_code"].as_str().map(str::to_string);
                form.warnings = done["warnings"].as_array().map_or(0, Vec::len);
                form.step_at = std::time::Instant::now();
                form.step = STEPS;
                self.p.name = form.name.clone();
                self.p.call = form.name.clone();
                self.p.english = form.language == "en";
                self.user = form.user.clone();
            }
            Event::Finish if form.step == STEPS => {
                let avisos = form.warnings;
                if form.autostart != form.autostart0 {
                    if let Err(e)=set_autostart(form.autostart){self.gate_err=e;}
                    self.p.boot = form.autostart;
                }
                self.onboarding = None;
                self.logged = true;
                self.gate_err.clear();
                self.toast(if avisos == 0 { "Tudo pronto. Seu Orion está configurado." } else { "Orion pronto, mas algumas chaves precisam ser cadastradas em Configurações." });
                if let Some(api) = self.api.clone() {
                    return crate::live::pull_basic(&api);
                }
            }
            Event::Saved(Err(e)) => {
                self.loading = None;
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
                    form.drive_abort = None;
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

/// Liga ou desliga o início com o Windows (mesma chave Run do instalador); só age se a pessoa mudou a opção.
pub fn autostart()->bool{
    #[cfg(windows)]{let result=system_command("reg.exe").args(["query",r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run","/v","Orion"]).output();result.is_ok_and(|r|r.status.success())}
    #[cfg(not(windows))]{false}
}
pub fn set_autostart(on:bool)->Result<(),String>{
    #[cfg(windows)]{
        let key=r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        if !on && !autostart(){return Ok(());}
        let mut c=system_command("reg.exe");
        if on{let exe=std::env::current_exe().map_err(|_|"Não foi possível localizar o Orion.".to_string())?;c.args(["add",key,"/v","Orion","/t","REG_SZ","/d",&format!("\"{}\" --background",exe.display()),"/f"]);}else{c.args(["delete",key,"/v","Orion","/f"]);}
        if c.status().map_err(|_|"Não foi possível configurar o início com o Windows.".to_string())?.success(){Ok(())}else{Err("Não foi possível configurar o início com o Windows.".into())}
    }
    #[cfg(not(windows))]{let _=on;Err("Inicialização automática disponível no Windows.".into())}
}

/// Abre `arquivo[:linha]` no VS Code. O alvo já foi conferido (caminho absoluto de um arquivo que existe) e só tem
/// caracteres seguros de caminho, então não há o que injetar no `cmd`.
pub fn open_in_editor(target: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = system_command("cmd.exe").creation_flags(0x08000000).args(["/C", "code", "-g", target]).spawn();
    }
    #[cfg(not(windows))]
    let _ = system_command("code").args(["-g", target]).spawn();
}

pub fn open_link(url: &str) {
    #[cfg(windows)]
    let _ = system_command("rundll32.exe").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "linux")]
    let _ = system_command("xdg-open").arg(url).spawn();
    #[cfg(target_os = "macos")]
    let _ = system_command("open").arg(url).spawn();
}

/// Pergunta sim ou não numa janela do sistema. Fora do Windows, ou se a janela não abrir, é "não":
/// a ação que precisa da sua confirmação nunca segue por padrão.
pub fn confirm(text: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let text = text.replace('\'', "''");
        let script = format!("Add-Type -AssemblyName System.Windows.Forms; $r=[System.Windows.Forms.MessageBox]::Show('{text}','Orion','YesNo','Warning'); [Console]::Write($r)");
        return system_command("powershell.exe")
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-STA", "-Command", &script])
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "Yes")
            .unwrap_or(false);
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        false
    }
}

pub fn choose(field: Field) -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = if field == Field::ResourceFile {
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='Arquivos (*.*)|*.*'; if($d.ShowDialog() -eq 'OK'){[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.FileName)}"
        } else if field == Field::Attachment {
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Title='Anexar documento ao chat'; $d.Filter='Documentos (*.txt;*.md;*.pdf;*.png;*.jpg;*.webp;*.mp3;*.wav;*.m4a;*.ogg;*.opus;*.flac;*.webm)|*.txt;*.md;*.pdf;*.png;*.jpg;*.jpeg;*.webp;*.wav;*.mp3;*.m4a;*.ogg;*.opus;*.flac;*.webm'; if($d.ShowDialog() -eq 'OK'){[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.FileName)}"
        } else if field == Field::ImportPath {
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.OpenFileDialog; $d.Filter='Exports e plugins (*.json;*.jsonl;*.zip;*.toml)|*.json;*.jsonl;*.zip;*.toml'; if($d.ShowDialog() -eq 'OK'){[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.FileName)}"
        } else if field == Field::Recovery {
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.SaveFileDialog; $d.Title='Salvar recuperação do Orion'; $d.Filter='Arquivo de texto (*.txt)|*.txt'; $d.FileName='Orion-recuperacao.txt'; if($d.ShowDialog() -eq 'OK'){[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.FileName)}"
        } else {
            "Add-Type -AssemblyName System.Windows.Forms; $d=New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description='Escolha uma pasta para o Orion'; if($d.ShowDialog() -eq 'OK'){[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::Write($d.SelectedPath)}"
        };
        let script = ["Anexar documento ao chat", "Documentos", "Salvar recuperação do Orion", "Arquivo de texto", "Escolha uma pasta para o Orion", "Exports e plugins"].iter().fold(script.to_string(), |script, label| script.replace(label, crate::i18n::text(label)));
        let output = system_command("powershell.exe")
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-STA", "-Command", &script])
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

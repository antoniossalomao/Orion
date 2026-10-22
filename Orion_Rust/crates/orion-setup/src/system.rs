//! Checagem do PC, tabela dos 8 modelos locais e motor transacional de instalação com desfazer
//! (`ORION_INSTALADOR_ESPECIFICACAO.md`, §§ 6 e 10).

use crate::i18n::{format_gb, Language};
use crate::register::{self, InstallManifest, InstalledFile, RecordingRegistrar, Registrar, UninstallEntry, WindowsRegistrar};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sysinfo::{Disks, System};

/// Repositório dos releases e chave pública minisign (`release/minisign.pub`, gerada pelo dono do
/// projeto; o segredo fica só no GitHub Actions). Sem chave o instalador não baixa nada.
pub const REPO_RELEASES: &str = "antoniossalomao/Orion";
pub const CHAVE_PUBLICA: &str = include_str!("../release/minisign.pub");

pub fn feed_url(vulkan: bool) -> String {
    format!("https://github.com/{REPO_RELEASES}/releases/latest/download/latest-{}.json", if vulkan { "vulkan" } else { "cpu" })
}

/// Onde estão `orion.exe` e o `orion-llama.exe` (sidecar dos modelos locais): na pasta `cpu`/`vulkan` ao lado
/// do instalador (instalação offline) ou, não havendo, baixados do GitHub Releases com assinatura
/// minisign e SHA-256 conferidos. O segundo valor é a pasta temporária a apagar depois da cópia.
pub fn obter_pacote(ao_lado: &Path, vulkan: bool, fetch: &dyn orion_updater::Fetch) -> Result<(PathBuf, Option<PathBuf>), String> {
    let local = ao_lado.join(if vulkan { "vulkan" } else { "cpu" });
    if local.join("orion.exe").is_file() {
        return Ok((local, None));
    }
    let tmp = std::env::temp_dir().join(format!("orion-instalacao-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    orion_updater::baixar_release(&feed_url(vulkan), CHAVE_PUBLICA, fetch, &tmp, &|_, _| {}).map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp);
        format!("Não consegui baixar o Orion: {e}")
    })?;
    Ok((tmp.clone(), Some(tmp)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSpace {
    pub mount: String,
    pub free_gb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcInfo {
    pub os_label: String,
    pub is_windows_11: bool,
    pub ram_gb: u32,
    pub gpu_name: String,
    pub vram_gb: u32,
    pub gpu_api: String,
    pub disks: Vec<DiskSpace>,
    pub reduce_motion: bool,
}

impl PcInfo {
    /// Exemplo de referência da especificação (§6): Windows 11, 32 GB RAM, GPU 12 GB VRAM,
    /// C: 42 GB (âmbar), D: 610 GB. Usado em snapshots determinísticos e testes.
    pub fn prototype_reference() -> Self {
        Self {
            os_label: "Windows 11 Pro 24H2".into(),
            is_windows_11: true,
            ram_gb: 32,
            gpu_name: "RTX 4070".into(),
            vram_gb: 12,
            gpu_api: "CUDA · Vulkan".into(),
            disks: vec![
                DiskSpace { mount: "C:".into(), free_gb: 42 },
                DiskSpace { mount: "D:".into(), free_gb: 610 },
            ],
            reduce_motion: false,
        }
    }

    pub fn detect() -> Self {
        let mut sys = System::new();
        sys.refresh_memory();
        let ram_bytes = sys.total_memory();
        let ram_gb = (ram_bytes / (1024 * 1024 * 1024)) as u32;
        drop(sys);

        let os_ver = System::os_version().unwrap_or_else(|| "11".into());
        let build: u32 = System::kernel_version()
            .and_then(|k| k.split('.').next_back()?.parse().ok())
            .unwrap_or(22631);
        let is_windows_11 = !cfg!(windows) || build >= 22000;
        let os_label = if is_windows_11 {
            let release = if build >= 26100 {
                "24H2"
            } else if build >= 22631 {
                "23H2"
            } else {
                "22H2"
            };
            format!("Windows 11 {release} ({build})")
        } else {
            format!("Windows 10 ({os_ver})")
        };

        let mut disks_out = Vec::with_capacity(4);
        let disks = Disks::new_with_refreshed_list();
        for d in disks.list() {
            let mount = d.mount_point().to_string_lossy().trim_end_matches('\\').to_string();
            let free_gb = d.available_space() / (1024 * 1024 * 1024);
            if !mount.is_empty() && !disks_out.iter().any(|x: &DiskSpace| x.mount == mount) {
                disks_out.push(DiskSpace { mount, free_gb });
            }
        }
        drop(disks);
        if disks_out.is_empty() {
            disks_out.push(DiskSpace { mount: "C:".into(), free_gb: 120 });
        }
        disks_out.sort_by(|a, b| a.mount.cmp(&b.mount));

        let (gpu_name, vram_gb, gpu_api) = detect_gpu();

        Self {
            os_label,
            is_windows_11,
            ram_gb,
            gpu_name,
            vram_gb,
            gpu_api,
            disks: disks_out,
            reduce_motion: false,
        }
    }

    pub fn c_drive_low(&self) -> bool {
        self.disks
            .iter()
            .find(|d| d.mount.eq_ignore_ascii_case("C:"))
            .is_some_and(|d| d.free_gb < 60)
    }

    pub fn best_models_disk(&self) -> &DiskSpace {
        self.disks
            .iter()
            .max_by_key(|d| d.free_gb)
            .unwrap_or(&self.disks[0])
    }

    pub fn suggested_models_dir(&self) -> PathBuf {
        let best = self.best_models_disk();
        PathBuf::from(format!("{}\\Orion\\Modelos", best.mount))
    }

    pub fn disk_summary(&self) -> String {
        self.disks
            .iter()
            .take(3)
            .map(|d| format!("{} {} GB", d.mount, d.free_gb))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn amber_hint(&self, lang: Language) -> Option<String> {
        if self.c_drive_low() && self.disks.len() > 1 {
            let best = self.best_models_disk();
            if !best.mount.eq_ignore_ascii_case("C:") {
                return Some(match lang {
                    Language::PtBr => format!("Como C: tem pouco espaço, os modelos vão para {}.", best.mount),
                    Language::En => format!("Since C: is low on space, models will go to {}.", best.mount),
                });
            }
        }
        None
    }

    pub fn step2_models_hint(&self, lang: Language) -> String {
        let best = self.best_models_disk();
        match lang {
            Language::PtBr => format!("Sugerido: {} tem mais espaço livre ({} GB).", best.mount, best.free_gb),
            Language::En => format!("Suggested: {} has the most free space ({} GB).", best.mount, best.free_gb),
        }
    }

    pub fn step3_footer(&self, lang: Language) -> String {
        match lang {
            Language::PtBr => format!("Detectado: {} GB RAM, GPU {} GB", self.ram_gb, self.vram_gb),
            Language::En => format!("Detected: {} GB RAM, GPU {} GB", self.ram_gb, self.vram_gb),
        }
    }
}

fn detect_gpu() -> (String, u32, String) {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    if let Ok(out) = cmd.output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = s.lines().next() {
                let mut parts = line.split(',');
                let name = parts
                    .next()
                    .unwrap_or("NVIDIA GPU")
                    .trim()
                    .replace("NVIDIA GeForce ", "");
                let mem_mb: u32 = parts.next().unwrap_or("8192").trim().parse().unwrap_or(8192);
                let vram_gb = ((mem_mb as f32 / 1024.0).round() as u32).max(1);
                return (name, vram_gb, "CUDA".into());
            }
        }
    }
    ("Não identificada".into(), 0, "Verificar driver Vulkan".into())
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalModel {
    pub id: String,
    pub name: String,
    pub desc_pt: &'static str,
    pub desc_en: &'static str,
    pub size_gb: f32,
    pub ram_gb: u32,
    pub min_ram_gb: u32,
    pub min_vram_gb: u32,
    pub recommended: bool,
    pub selected: bool,
}

impl LocalModel {
    pub fn desc(&self, lang: Language) -> &'static str {
        match lang {
            Language::PtBr => self.desc_pt,
            Language::En => self.desc_en,
        }
    }

    pub fn meta_line(&self, lang: Language) -> String {
        format!("{} · RAM ~{} GB", format_gb(self.size_gb, lang), self.ram_gb)
    }
}

/// Os 8 modelos da Etapa 3 com regras de recomendação e pré-marcação por RAM e VRAM (§6 Etapa 3).
pub fn default_models(pc: &PcInfo) -> Vec<LocalModel> {
    let total = pc.ram_gb as u64 * 1024 * 1024 * 1024;
    let models = orion_local::catalog::all();
    let recommendation = models.iter().filter(|m| m.kind == "llm" && orion_local::admitted(m.bytes, total, total))
        .max_by_key(|m| m.bytes).map(|m| m.id.clone());
    models.into_iter().map(|m| {
        let llm = m.kind == "llm";
        LocalModel {
            id: m.id.clone(),
            name: m.name.clone(),
            desc_pt: if llm { "Reserva local, sem ferramentas. Teto de 30% da RAM." } else { "Transcrição local em português; CPU ou Vulkan." },
            desc_en: if llm { "Local fallback, no tools. Limited to 30% of RAM." } else { "Local Portuguese transcription; CPU or Vulkan." },
            size_gb: m.bytes as f32 / 1_000_000_000.0,
            ram_gb: (m.bytes.saturating_add(orion_local::RUNTIME_RESERVE) / (1024 * 1024 * 1024) + 1) as u32,
            min_ram_gb: if llm { ((m.bytes + orion_local::RUNTIME_RESERVE) * 100 / 30 / (1024 * 1024 * 1024) + 1) as u32 } else { 4 },
            // Estimativa: pesos do GGUF com folga de 20% para o cache de contexto. STT roda em CPU ou Vulkan, sem VRAM mínima.
            min_vram_gb: if llm { ((m.bytes as f64 * 1.2) / 1_000_000_000.0).ceil() as u32 } else { 0 },
            recommended: if llm { recommendation.as_deref() == Some(&m.id) } else { m.id == "whisper-small" },
            selected: false, // REGRAS.md: opt-in não liga sozinho; recomendações não iniciam download.
        }
    }).collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupChoices {
    pub language: Language,
    pub app_dir: PathBuf,
    pub models_dir: PathBuf,
    pub data_dir: PathBuf,
    pub models: Vec<LocalModel>,
    pub start_with_windows: bool,
    pub enable_voice: bool,
    pub use_vulkan: bool,
    pub wake_bundle: String,
    pub pin_to_taskbar: bool,
    pub send_crash_reports: bool,
}

impl SetupChoices {
    pub fn new(pc: &PcInfo) -> Self {
        Self {
            language: Language::PtBr,
            app_dir: PathBuf::from(r"C:\Program Files\Orion"),
            models_dir: pc.suggested_models_dir(),
            data_dir: PathBuf::from(r"C:\ProgramData\Orion"),
            models: default_models(pc),
            start_with_windows: false,
            enable_voice: false,
            use_vulkan: false,
            wake_bundle: String::new(),
            pin_to_taskbar: true,
            send_crash_reports: false,
        }
    }

    pub fn selected_models_count(&self) -> usize {
        self.models.iter().filter(|m| m.selected).count()
    }

    pub fn selected_models_gb(&self) -> f32 {
        self.models.iter().filter(|m| m.selected).map(|m| m.size_gb).sum()
    }

    pub fn total_line_left(&self) -> String {
        let n = self.selected_models_count();
        let gb = format_gb(self.selected_models_gb(), self.language);
        match self.language {
            Language::PtBr => format!("{n} selecionados · {gb}"),
            Language::En => format!("{n} selected · {gb}"),
        }
    }
}

/// Estado de cada um dos 6 passos da Etapa 5 (§6 Etapa 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StepItemStatus {
    Pending,
    Running,
    Done,
    Error,
}

/// Registro feito fora de arquivos, desfeito pelo `rollback`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Undo {
    Uninstall,
    Autostart,
    Identity,
    UpdateTask,
}

#[derive(Clone)]
pub struct InstallPlan {
    pub root_override: Option<PathBuf>,
    pub created_paths: Vec<PathBuf>,
    registrar: Arc<dyn Registrar>,
    undo: Vec<Undo>,
    /// Thumbprint do certificado criado neste plano, para o `rollback` retirá-lo.
    identity_thumbprint: Option<String>,
}

impl std::fmt::Debug for InstallPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallPlan").field("root_override", &self.root_override).field("created_paths", &self.created_paths).finish()
    }
}

impl InstallPlan {
    /// Com `root_override` (prévia e testes) nada é registrado no Windows.
    pub fn new(root_override: Option<PathBuf>) -> Self {
        let registrar: Arc<dyn Registrar> = if root_override.is_some() { Arc::new(RecordingRegistrar::default()) } else { Arc::new(WindowsRegistrar) };
        Self::with_registrar(root_override, registrar)
    }

    pub fn with_registrar(root_override: Option<PathBuf>, registrar: Arc<dyn Registrar>) -> Self {
        Self { root_override, created_paths: Vec::new(), registrar, undo: Vec::new(), identity_thumbprint: None }
    }

    fn real(&self) -> bool {
        self.root_override.is_none()
    }

    /// Relê o manifesto do app, aplica a mudança e grava de volta.
    fn update_manifest(&self, app_dir: &Path, change: impl FnOnce(&mut InstallManifest)) -> Result<(), String> {
        let mut m = InstallManifest::load(app_dir)?;
        change(&mut m);
        m.save(app_dir)
    }

    fn map_path(&self, original: &Path) -> PathBuf {
        if let Some(root) = &self.root_override {
            let clean: String = original
                .to_string_lossy()
                .chars()
                .map(|c| if c == ':' { '_' } else { c })
                .collect();
            root.join(clean.trim_start_matches(['\\', '/']))
        } else {
            original.to_path_buf()
        }
    }

    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
        if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
            let _ = std::fs::remove_file(path);
            return Err(error);
        }
        Ok(())
    }

    pub fn install_models(&mut self, choices: &SetupChoices, cancel: &std::sync::atomic::AtomicBool, progress: &dyn Fn(u64, u64)) -> Result<String, String> {
        let models_dir = self.map_path(&choices.models_dir);
        let data_dir = self.map_path(&choices.data_dir);
        let existed = models_dir.exists();
        std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
        if !existed { self.created_paths.push(models_dir.clone()); }
        let data_existed = data_dir.exists();
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        if !data_existed { self.created_paths.push(data_dir.clone()); }
        let selected: Vec<_> = choices.models.iter().filter(|m| m.selected).collect();
        let mut config = serde_json::json!({
            "ORION_DATA_DIR": choices.data_dir,
            "ORION_VOICE_ENABLED": choices.enable_voice,
            "ORION_VOICE_BARGE_IN": choices.enable_voice,
            "ORION_WAKE_ENABLED": false,
            "ORION_LOCAL_MODEL_GPU": choices.use_vulkan,
            "ORION_LOCAL_STT_GPU": choices.use_vulkan,
            "installed_models": [],
        });
        let mut llm = false;
        let mut stt = false;
        for model in selected {
            let artifact = orion_local::catalog::find(&model.id).ok_or("modelo sem artefato verificável")?;
            let path = if self.root_override.is_some() {
                // Prévia não baixa pesos nem finge que o modo local foi instalado.
                continue;
            } else {
                let (path, created) = orion_local::download::install(&model.id, &models_dir, cancel, progress).map_err(|e| e.to_string())?;
                if created { self.created_paths.push(path.clone()); }
                path
            };
            config["installed_models"].as_array_mut().unwrap().push(serde_json::json!({
                "id": artifact.id, "repository": artifact.repository, "revision": artifact.revision,
                "sha256": artifact.sha256, "bytes": artifact.bytes, "filename": artifact.filename,
            }));
            if artifact.kind == "llm" && !llm {
                config["ORION_LOCAL_MODEL_PATH"] = serde_json::json!(path);
                llm = true;
            } else if artifact.kind == "stt" && !stt {
                config["ORION_LOCAL_STT_MODEL"] = serde_json::json!(path);
                stt = true;
            }
        }
        if cancel.load(std::sync::atomic::Ordering::SeqCst) { return Err("instalação cancelada".into()); }
        if choices.enable_voice && !choices.wake_bundle.trim().is_empty() && self.root_override.is_none() {
            let source_dir = Path::new(choices.wake_bundle.trim());
            let (source, manifest) = orion_local::wake::load(source_dir, false).map_err(|e| e.to_string())?;
            let target_dir = models_dir.join("orion-wake");
            let existed = target_dir.exists();
            std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
            if !existed { self.created_paths.push(target_dir.clone()); }
            for (source, name) in [(source, "orion.onnx"), (source_dir.join("orion.json"), "orion.json")] {
                let target = target_dir.join(name);
                if source.canonicalize().ok() == target.canonicalize().ok() && target.exists() { continue; }
                self.write_new(&target, &std::fs::read(source).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                self.created_paths.push(target);
            }
            orion_local::wake::load(&target_dir, false).map_err(|e| e.to_string())?;
            if !stt { return Err("Escuta Orion exige selecionar também um modelo Whisper".into()); }
            config["ORION_WAKE_MODEL"] = serde_json::json!(target_dir.join("orion.onnx"));
            config["ORION_WAKE_THRESHOLD"] = serde_json::json!(manifest.threshold);
            config["ORION_WAKE_ENABLED"] = serde_json::json!(true);
        }
        let target = data_dir.join("orion-local.json");
        self.write_new(&target, &serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        self.created_paths.push(target);
        Ok(if self.root_override.is_some() { "Prévia concluída; nenhum modelo foi baixado" } else { "Modelos verificados e modo local configurado" }.into())
    }

    /// Executa o passo `index` (0..6) e registra os artefatos criados para desfazer se necessário (§10.9).
    pub fn execute_step(&mut self, index: usize, choices: &SetupChoices) -> Result<String, String> {
        let app_dir = self.map_path(&choices.app_dir);
        let data_dir = self.map_path(&choices.data_dir);

        match index {
            0 => {
                std::fs::create_dir_all(&app_dir).map_err(|e| format!("falha ao criar {}: {e}", app_dir.display()))?;
                if std::fs::read_dir(&app_dir).is_ok_and(|mut entries| entries.next().is_none()) { self.created_paths.push(app_dir.clone()); }
                let manifest = app_dir.join(register::MANIFEST);
                if manifest.exists() { return Err("Orion já instalado nesta pasta; escolha uma pasta nova para preservar a instalação existente".into()); }
                let info = InstallManifest {
                    version: env!("CARGO_PKG_VERSION").into(),
                    app_dir: choices.app_dir.clone(),
                    data_dir: choices.data_dir.clone(),
                    models_dir: choices.models_dir.clone(),
                    ..Default::default()
                };
                self.write_new(&manifest, &serde_json::to_vec_pretty(&info).unwrap_or_default())
                    .map_err(|e| format!("falha ao gravar manifesto: {e}"))?;
                self.created_paths.push(manifest);
                if self.real() {
                    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                    let (bundle, temporaria) = obter_pacote(executable.parent().ok_or("pasta do instalador ausente")?, choices.use_vulkan, &orion_updater::HttpFetch)?;
                    let mut copied = Vec::new();
                    for (name, source) in [
                        ("orion.exe", bundle.join("orion.exe")),
                        ("orion-llama.exe", bundle.join("orion-llama.exe")),
                        // o próprio instalador vira o desinstalador (Apps instalados chama `--uninstall`)
                        (register::UNINSTALLER, executable.clone()),
                    ] {
                        let mut input = std::fs::File::open(&source).map_err(|_| format!("O pacote precisa incluir {name}"))?;
                        let destination = app_dir.join(name);
                        let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&destination).map_err(|e| e.to_string())?;
                        self.created_paths.push(destination.clone());
                        std::io::copy(&mut input, &mut output).map_err(|e| e.to_string())?;
                        output.sync_all().map_err(|e| e.to_string())?;
                        copied.push(name);
                    }
                    let files: Vec<InstalledFile> = copied
                        .into_iter()
                        .map(|n| register::sha256_file(&app_dir.join(n)).map(|sha256| InstalledFile { name: n.into(), sha256 }).map_err(|e| e.to_string()))
                        .collect::<Result<_, _>>()?;
                    if let Some(t) = temporaria {
                        let _ = std::fs::remove_dir_all(t);
                    }
                    self.update_manifest(&app_dir, |m| m.files = files)?;
                }
                Ok(format!("App preparado em {}", app_dir.display()))
            }
            1 => {
                // Registro em Apps instalados: é o que habilita "Desinstalar" no Windows.
                let uninstaller = choices.app_dir.join(register::UNINSTALLER);
                let entry = UninstallEntry {
                    display_name: "Orion".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    publisher: "Antônio".into(),
                    install_location: choices.app_dir.clone(),
                    uninstall_command: format!("\"{}\" --uninstall \"{}\"", uninstaller.display(), choices.app_dir.display()),
                    icon: choices.app_dir.join("orion.exe"),
                };
                self.registrar.register_uninstall(&entry).map_err(|e| format!("falha ao registrar o app no Windows: {e}"))?;
                self.undo.push(Undo::Uninstall);
                self.update_manifest(&app_dir, |m| m.registered = true)?;
                Ok("App registrado em Apps instalados".into())
            }
            2 => {
                // Identidade de pacote esparso com certificado autoassinado. Sem SDK do Windows ou
                // sem administrador o passo é pulado e dito, nunca fingido.
                let identidade = match self.registrar.register_identity(&choices.app_dir, env!("CARGO_PKG_VERSION")) {
                    Ok(crate::identity::IdentityOutcome::Registered { thumbprint }) => {
                        self.identity_thumbprint = Some(thumbprint.clone());
                        self.undo.push(Undo::Identity);
                        self.update_manifest(&app_dir, |m| m.identity_thumbprint = Some(thumbprint))?;
                        "identidade de pacote registrada".to_string()
                    }
                    Ok(crate::identity::IdentityOutcome::Skipped(motivo)) => format!("identidade de pacote não registrada ({motivo})"),
                    Err(e) => format!("identidade de pacote não registrada ({e})"),
                };
                // Lista de SHA-256 dos arquivos copiados, para conferência.
                let m = InstallManifest::load(&app_dir)?;
                let lista: String = m.files.iter().map(|f| format!("{}  {}\n", f.sha256, f.name)).collect();
                let sums = app_dir.join("orion-files.sha256");
                self.write_new(&sums, lista.as_bytes()).map_err(|e| format!("falha ao gravar a lista de integridade: {e}"))?;
                self.created_paths.push(sums);
                Ok(format!("{identidade}; integridade registrada para {} arquivos", m.files.len()))
            }
            3 => {
                let lnk = if self.real() { register::start_menu_shortcut() } else { app_dir.join("Orion.lnk") };
                let orion = choices.app_dir.join("orion.exe");
                if lnk.exists() { return Err(format!("já existe um atalho em {}", lnk.display())); }
                self.registrar.create_shortcut(&lnk, &orion, "", &choices.app_dir).map_err(|e| format!("falha ao criar o atalho: {e}"))?;
                if lnk.exists() { self.created_paths.push(lnk.clone()); }
                let mut autostart = false;
                if choices.start_with_windows {
                    // opt-in (REGRAS.md): só com a caixa marcada
                    self.registrar.set_autostart(&format!("\"{}\" --background", orion.display())).map_err(|e| format!("falha ao ativar o início automático: {e}"))?;
                    self.undo.push(Undo::Autostart);
                    autostart = true;
                }
                self.update_manifest(&app_dir, |m| {
                    m.shortcut = Some(lnk);
                    m.autostart = autostart;
                })?;
                Ok(if choices.pin_to_taskbar { "Atalho no Menu Iniciar criado; fixar na barra exige sua confirmação no Windows" } else { "Atalho no Menu Iniciar criado" }.into())
            }
            4 => {
                // Preferências de atualização e tarefa diária. Sem chave pública nem feed o
                // atualizador não faz nada (fail-closed): quem publica releases preenche os dois.
                let updater = data_dir.join("update.json");
                let payload = serde_json::json!({
                    "channel": "stable",
                    "auto_update": false,
                    "crash_reports": choices.send_crash_reports,
                    "feed_url": if CHAVE_PUBLICA.trim().is_empty() { serde_json::Value::Null } else { serde_json::json!(feed_url(choices.use_vulkan)) },
                    "public_key": if CHAVE_PUBLICA.trim().is_empty() { serde_json::Value::Null } else { serde_json::json!(CHAVE_PUBLICA.trim()) },
                });
                std::fs::create_dir_all(&data_dir).map_err(|e| format!("falha ao criar {}: {e}", data_dir.display()))?;
                self.write_new(&updater, &serde_json::to_vec_pretty(&payload).unwrap_or_default())
                    .map_err(|e| format!("falha ao gravar preferências de atualização: {e}"))?;
                self.created_paths.push(updater);
                let tarefa = match self.registrar.install_update_task(&choices.app_dir.join("orion.exe")) {
                    Ok(()) => {
                        self.undo.push(Undo::UpdateTask);
                        self.update_manifest(&app_dir, |m| m.update_task = true)?;
                        "tarefa diária de atualização criada".to_string()
                    }
                    Err(e) => format!("tarefa diária não criada ({e})"),
                };
                Ok(format!("Preferências gravadas; {tarefa}"))
            }
            5 => self.install_models(choices, &std::sync::atomic::AtomicBool::new(false), &|_, _| {}),
            _ => Ok("Concluído".into()),
        }
    }

    /// Desfaz em ordem inversa tudo o que foi criado até aqui (§2.1, §6 Etapa 5 e §10.9).
    pub fn rollback(&mut self) {
        while let Some(undo) = self.undo.pop() {
            let _ = match undo {
                Undo::Autostart => self.registrar.remove_autostart(),
                Undo::Uninstall => self.registrar.unregister_uninstall(),
                Undo::Identity => self.identity_thumbprint.take().map_or(Ok(()), |t| self.registrar.unregister_identity(&t)),
                Undo::UpdateTask => self.registrar.remove_update_task(),
            };
        }
        while let Some(path) = self.created_paths.pop() {
            if path.is_file() {
                let _ = std::fs::remove_file(&path);
            } else if path.is_dir() {
                let _ = std::fs::remove_dir(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Falso(std::collections::HashMap<String, Vec<u8>>);
    impl orion_updater::Fetch for Falso {
        fn get(&self, url: &str, _max: u64) -> Result<Vec<u8>, orion_updater::UpdateError> {
            self.0.get(url).cloned().ok_or_else(|| orion_updater::UpdateError::Fetch(format!("404 {url}")))
        }
    }

    #[test]
    fn pacote_ao_lado_do_instalador_vale_e_nao_baixa() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("cpu")).unwrap();
        std::fs::write(d.path().join("cpu").join("orion.exe"), b"x").unwrap();
        let (pasta, tmp) = obter_pacote(d.path(), false, &Falso(Default::default())).unwrap();
        assert_eq!(pasta, d.path().join("cpu"));
        assert!(tmp.is_none());
    }

    #[test]
    fn sem_pacote_e_sem_chave_publica_nao_baixa_e_explica() {
        let d = tempfile::tempdir().unwrap();
        let erro = obter_pacote(d.path(), true, &Falso(Default::default())).unwrap_err();
        assert!(erro.contains("Não consegui baixar o Orion"), "{erro}");
    }

    #[test]
    fn modelos_so_baixam_apos_escolha_e_respeitam_o_teto() {
        let pc = PcInfo::prototype_reference();
        let models = default_models(&pc);
        assert_eq!(models.len(), 8);
        assert!(models.iter().all(|m| !m.selected));
        assert!(models.iter().any(|m| m.id == "qwen3-8b" && m.recommended));
        let choices = SetupChoices::new(&pc);
        assert!(!choices.enable_voice && !choices.start_with_windows && !choices.use_vulkan);
    }

    fn plan_de_teste(root: &Path) -> (InstallPlan, Arc<RecordingRegistrar>) {
        let rec = Arc::new(RecordingRegistrar::default());
        (InstallPlan::with_registrar(Some(root.to_path_buf()), rec.clone()), rec)
    }

    #[test]
    fn instalacao_e_rollback_desfazem_em_ordem_inversa() {
        let tmp = tempfile::tempdir().unwrap();
        let mut choices = SetupChoices::new(&PcInfo::prototype_reference());
        choices.start_with_windows = true;
        let (mut plan, rec) = plan_de_teste(tmp.path());
        for step in 0..5 {
            plan.execute_step(step, &choices).unwrap_or_else(|e| panic!("passo {step}: {e}"));
        }
        let calls = rec.calls();
        assert!(calls.iter().any(|c| c.starts_with("register_uninstall \"") && c.contains("--uninstall")));
        assert!(calls.iter().any(|c| c.starts_with("set_autostart") && c.contains("--background") && !c.contains("desktop")));
        assert!(calls.iter().any(|c| c.starts_with("register_identity")) && calls.iter().any(|c| c.starts_with("install_update_task")));
        let app = plan.map_path(&choices.app_dir);
        let m = InstallManifest::load(&app).unwrap();
        assert!(m.registered && m.autostart && m.shortcut.is_some());
        assert!(app.join("orion-files.sha256").exists());
        assert!(!plan.created_paths.is_empty());
        plan.rollback();
        assert!(plan.created_paths.is_empty());
        let calls = rec.calls();
        assert_eq!(&calls[calls.len() - 4..], ["remove_update_task", "remove_autostart", format!("unregister_identity {}", "A".repeat(40)).as_str(), "unregister_uninstall"]);
        assert!(!app.join(register::MANIFEST).exists());
    }

    #[test]
    fn inicio_automatico_so_com_a_caixa_marcada() {
        let tmp = tempfile::tempdir().unwrap();
        let choices = SetupChoices::new(&PcInfo::prototype_reference());
        assert!(!choices.start_with_windows);
        let (mut plan, rec) = plan_de_teste(tmp.path());
        for step in 0..4 {
            plan.execute_step(step, &choices).unwrap();
        }
        assert!(rec.calls().iter().all(|c| !c.contains("autostart")));
    }

    #[test]
    fn instalacao_completa_pode_ser_desinstalada() {
        let tmp = tempfile::tempdir().unwrap();
        let choices = SetupChoices::new(&PcInfo::prototype_reference());
        let (mut plan, rec) = plan_de_teste(tmp.path());
        for step in 0..5 {
            plan.execute_step(step, &choices).unwrap();
        }
        let app = plan.map_path(&choices.app_dir);
        let data = plan.map_path(&choices.data_dir);
        std::fs::write(data.join("orion.db"), b"dados do dono").unwrap();
        register::uninstall(&app, rec.as_ref(), register::UninstallOptions::default()).unwrap();
        assert!(!app.join(register::MANIFEST).exists());
        assert_eq!(std::fs::read(data.join("orion.db")).unwrap(), b"dados do dono");
    }

    #[test]
    fn rollback_preserva_dados_e_arquivos_preexistentes() {
        let tmp = tempfile::tempdir().unwrap();
        let choices = SetupChoices::new(&PcInfo::prototype_reference());
        let (mut plan, _) = plan_de_teste(tmp.path());
        let data = plan.map_path(&choices.data_dir);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("orion.db"), b"dados do dono").unwrap();
        plan.execute_step(0, &choices).unwrap();
        plan.execute_step(4, &choices).unwrap();
        assert!(data.join("update.json").exists());
        plan.rollback();
        assert_eq!(std::fs::read(data.join("orion.db")).unwrap(), b"dados do dono");
        assert!(!data.join("update.json").exists());
        std::fs::write(data.join("update.json"), b"config antiga").unwrap();
        plan.execute_step(0, &choices).unwrap();
        assert!(plan.execute_step(4, &choices).is_err());
        plan.rollback();
        assert_eq!(std::fs::read(data.join("update.json")).unwrap(), b"config antiga");
    }

    #[test]
    fn previa_nao_grava_caminho_de_modelo_inexistente() {
        let tmp = tempfile::tempdir().unwrap();
        let mut choices = SetupChoices::new(&PcInfo::prototype_reference());
        choices.models.iter_mut().find(|m| m.id == "qwen3-0.6b").unwrap().selected = true;
        let mut plan = InstallPlan::new(Some(tmp.path().to_path_buf()));
        plan.execute_step(5, &choices).unwrap();
        let config: serde_json::Value = serde_json::from_slice(&std::fs::read(plan.map_path(&choices.data_dir).join("orion-local.json")).unwrap()).unwrap();
        assert!(config.get("ORION_LOCAL_MODEL_PATH").is_none());
        assert_eq!(config["ORION_WAKE_ENABLED"], false);
        plan.rollback();
    }
}

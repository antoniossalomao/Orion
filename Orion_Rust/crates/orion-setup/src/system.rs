//! Checagem do PC, tabela dos 8 modelos locais e motor transacional de instalação com desfazer
//! (`ORION_INSTALADOR_ESPECIFICACAO.md`, §§ 6 e 10).

use crate::i18n::{format_gb, Language};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use sysinfo::{Disks, System};

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
            min_vram_gb: 0,
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

#[derive(Debug, Clone)]
pub struct InstallPlan {
    pub root_override: Option<PathBuf>,
    pub created_paths: Vec<PathBuf>,
}

impl InstallPlan {
    pub fn new(root_override: Option<PathBuf>) -> Self {
        Self { root_override, created_paths: Vec::new() }
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
                let manifest = app_dir.join("orion-install.json");
                if manifest.exists() { return Err("Orion já instalado nesta pasta; escolha uma pasta nova para preservar a instalação existente".into()); }
                let info = serde_json::json!({
                    "version": "0.1.0",
                    "app_dir": choices.app_dir,
                    "data_dir": choices.data_dir,
                    "models_dir": choices.models_dir,
                });
                self.write_new(&manifest, &serde_json::to_vec_pretty(&info).unwrap_or_default())
                    .map_err(|e| format!("falha ao gravar manifesto: {e}"))?;
                self.created_paths.push(manifest);
                if self.root_override.is_none() {
                    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                    let bundle = executable.parent().ok_or("pasta do instalador ausente")?.join(if choices.use_vulkan { "vulkan" } else { "cpu" });
                    for name in ["orion.exe", "orion-llama.exe"] {
                        let mut source = std::fs::File::open(bundle.join(name)).map_err(|_| format!("O pacote precisa incluir {name}"))?;
                        let destination = app_dir.join(name);
                        let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&destination).map_err(|e| e.to_string())?;
                        self.created_paths.push(destination);
                        std::io::copy(&mut source, &mut output).map_err(|e| e.to_string())?;
                    }
                }
                Ok(format!("App preparado em {}", app_dir.display()))
            }
            1 => {
                let data_existed = data_dir.exists();
                std::fs::create_dir_all(&data_dir).map_err(|e| format!("falha ao criar {}: {e}", data_dir.display()))?;
                if !data_existed { self.created_paths.push(data_dir.clone()); }
                let sparse = data_dir.join("AppxManifest.xml");
                self.write_new(
                    &sparse,
                    r#"<?xml version="1.0" encoding="utf-8"?><Package><Identity Name="Orion.Desktop" Version="0.1.0.0" /></Package>"#.as_bytes(),
                )
                .map_err(|e| format!("falha ao registrar identidade: {e}"))?;
                self.created_paths.push(sparse);
                Ok("Identidade de pacote esparso registrada".into())
            }
            2 => {
                let cert = data_dir.join("orion-selfsigned.cer");
                self.write_new(&cert, b"ORION-SELFSIGNED-CERT-V1")
                    .map_err(|e| format!("falha ao instalar certificado: {e}"))?;
                self.created_paths.push(cert);
                Ok("Certificado autoassinado registrado".into())
            }
            3 => {
                let shortcuts = data_dir.join("shortcuts.json");
                let payload = serde_json::json!({
                    "start_menu": true,
                    "pin_to_taskbar": choices.pin_to_taskbar,
                    "start_with_windows": choices.start_with_windows,
                });
                self.write_new(&shortcuts, &serde_json::to_vec_pretty(&payload).unwrap_or_default())
                    .map_err(|e| format!("falha ao criar atalhos: {e}"))?;
                self.created_paths.push(shortcuts);
                Ok("Atalhos no Menu Iniciar e barra configurados".into())
            }
            4 => {
                let updater = data_dir.join("updater-service.json");
                let payload = serde_json::json!({
                    "service": "OrionUpdater",
                    "channel": "stable",
                    "crash_reports": choices.send_crash_reports,
                });
                self.write_new(&updater, &serde_json::to_vec_pretty(&payload).unwrap_or_default())
                    .map_err(|e| format!("falha ao configurar serviço de atualização: {e}"))?;
                self.created_paths.push(updater);
                Ok("Serviço de atualização configurado".into())
            }
            5 => self.install_models(choices, &std::sync::atomic::AtomicBool::new(false), &|_, _| {}),
            _ => Ok("Concluído".into()),
        }
    }

    /// Desfaz em ordem inversa tudo o que foi criado até aqui (§2.1, §6 Etapa 5 e §10.9).
    pub fn rollback(&mut self) {
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

    #[test]
    fn instalacao_e_rollback_desfazem_em_ordem_inversa() {
        let tmp = tempfile::tempdir().unwrap();
        let pc = PcInfo::prototype_reference();
        let choices = SetupChoices::new(&pc);
        let mut plan = InstallPlan::new(Some(tmp.path().to_path_buf()));
        for step in 0..4 {
            assert!(plan.execute_step(step, &choices).is_ok());
        }
        assert!(!plan.created_paths.is_empty());
        plan.rollback();
        assert!(plan.created_paths.is_empty());
    }

    #[test]
    fn rollback_preserva_dados_e_arquivos_preexistentes() {
        let tmp = tempfile::tempdir().unwrap();
        let choices = SetupChoices::new(&PcInfo::prototype_reference());
        let mut plan = InstallPlan::new(Some(tmp.path().to_path_buf()));
        let data = plan.map_path(&choices.data_dir);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("orion.db"), b"dados do dono").unwrap();
        plan.execute_step(1, &choices).unwrap();
        plan.rollback();
        assert_eq!(std::fs::read(data.join("orion.db")).unwrap(), b"dados do dono");
        assert!(!data.join("AppxManifest.xml").exists());
        std::fs::write(data.join("shortcuts.json"), b"config antiga").unwrap();
        assert!(plan.execute_step(3, &choices).is_err());
        plan.rollback();
        assert_eq!(std::fs::read(data.join("shortcuts.json")).unwrap(), b"config antiga");
    }

    #[test]
    fn previa_nao_grava_caminho_de_modelo_inexistente() {
        let tmp = tempfile::tempdir().unwrap();
        let mut choices = SetupChoices::new(&PcInfo::prototype_reference());
        choices.models.iter_mut().find(|m| m.id == "qwen3-0.6b").unwrap().selected = true;
        let mut plan = InstallPlan::new(Some(tmp.path().to_path_buf()));
        plan.execute_step(1, &choices).unwrap();
        plan.execute_step(5, &choices).unwrap();
        let config: serde_json::Value = serde_json::from_slice(&std::fs::read(plan.map_path(&choices.data_dir).join("orion-local.json")).unwrap()).unwrap();
        assert!(config.get("ORION_LOCAL_MODEL_PATH").is_none());
        assert_eq!(config["ORION_WAKE_ENABLED"], false);
        plan.rollback();
    }
}

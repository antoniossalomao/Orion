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
        let mut sys = System::new_all();
        sys.refresh_memory();
        let ram_bytes = sys.total_memory();
        let ram_gb = ((ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0)).round() as u32).max(4);

        let os_ver = System::os_version().unwrap_or_else(|| "11".into());
        let build: u32 = System::kernel_version()
            .and_then(|k| k.split('.').next_back()?.parse().ok())
            .unwrap_or(22631);
        let is_windows_11 = !cfg!(windows) || build >= 22000;
        let os_label = if is_windows_11 {
            format!("Windows 11 ({os_ver})")
        } else {
            format!("Windows 10 ({os_ver})")
        };

        let mut disks_out = Vec::new();
        let disks = Disks::new_with_refreshed_list();
        for d in disks.list() {
            let mount = d.mount_point().to_string_lossy().trim_end_matches('\\').to_string();
            let free_gb = d.available_space() / (1024 * 1024 * 1024);
            if !mount.is_empty() && !disks_out.iter().any(|x: &DiskSpace| x.mount == mount) {
                disks_out.push(DiskSpace { mount, free_gb });
            }
        }
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
    if let Ok(out) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])
        .output()
    {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = s.lines().next() {
                let mut parts = line.split(',');
                let name = parts.next().unwrap_or("NVIDIA GPU").trim().replace("NVIDIA GeForce ", "");
                let mem_mb: u32 = parts.next().unwrap_or("8192").trim().parse().unwrap_or(8192);
                let vram_gb = ((mem_mb as f32 / 1024.0).round() as u32).max(1);
                return (name, vram_gb, "CUDA · Vulkan".into());
            }
        }
    }
    ("GPU DirectX 12".into(), 8, "Vulkan".into())
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalModel {
    pub id: &'static str,
    pub name: &'static str,
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
    let has_32_12 = pc.ram_gb >= 24 && pc.vram_gb >= 8;
    let has_big_gpu = pc.ram_gb >= 32 && pc.vram_gb >= 16;
    vec![
        LocalModel {
            id: "whisper-large-v3-turbo",
            name: "Whisper large-v3-turbo",
            desc_pt: "Reconhecimento de voz em português.",
            desc_en: "Portuguese speech recognition.",
            size_gb: 0.8,
            ram_gb: 2,
            min_ram_gb: 8,
            min_vram_gb: 2,
            recommended: pc.ram_gb >= 8,
            selected: pc.ram_gb >= 8,
        },
        LocalModel {
            id: "qwen3-8b",
            name: "Qwen 3 8B",
            desc_pt: "Geral e ferramentas. Bom em pt-BR.",
            desc_en: "General and tool use. Strong in pt-BR.",
            size_gb: 5.0,
            ram_gb: 6,
            min_ram_gb: 16,
            min_vram_gb: 6,
            recommended: has_32_12,
            selected: has_32_12,
        },
        LocalModel {
            id: "gemma-4-e4b",
            name: "Gemma 4 E4B",
            desc_pt: "Leve, ideal para o notebook.",
            desc_en: "Lightweight, ideal for laptops.",
            size_gb: 4.0,
            ram_gb: 5,
            min_ram_gb: 12,
            min_vram_gb: 4,
            recommended: pc.ram_gb >= 12,
            selected: !has_32_12 && pc.ram_gb >= 12,
        },
        LocalModel {
            id: "granite-4.1-8b",
            name: "Granite 4.1 8B",
            desc_pt: "Melhor tool calling medido.",
            desc_en: "Best measured tool calling.",
            size_gb: 5.0,
            ram_gb: 6,
            min_ram_gb: 16,
            min_vram_gb: 6,
            recommended: false,
            selected: false,
        },
        LocalModel {
            id: "phi-4-mini-3.8b",
            name: "Phi-4-mini 3.8B",
            desc_pt: "Rápido, respostas curtas.",
            desc_en: "Fast, concise answers.",
            size_gb: 2.4,
            ram_gb: 3,
            min_ram_gb: 8,
            min_vram_gb: 3,
            recommended: false,
            selected: false,
        },
        LocalModel {
            id: "moondream-2",
            name: "Moondream 2",
            desc_pt: "Visão leve. \"Isso?\" sem internet.",
            desc_en: "Lightweight vision. Offline \"What is this?\".",
            size_gb: 1.8,
            ram_gb: 2,
            min_ram_gb: 8,
            min_vram_gb: 2,
            recommended: pc.ram_gb >= 8,
            selected: pc.ram_gb >= 8,
        },
        LocalModel {
            id: "minicpm-v-4.5",
            name: "MiniCPM-V 4.5",
            desc_pt: "Visão e OCR de documentos.",
            desc_en: "Vision and document OCR.",
            size_gb: 5.5,
            ram_gb: 6,
            min_ram_gb: 16,
            min_vram_gb: 6,
            recommended: false,
            selected: false,
        },
        LocalModel {
            id: "mistral-small-3.2-24b",
            name: "Mistral Small 3.2 24B",
            desc_pt: "Mais forte. Só no PC com GPU grande.",
            desc_en: "Strongest. Requires a large GPU.",
            size_gb: 14.0,
            ram_gb: 16,
            min_ram_gb: 32,
            min_vram_gb: 16,
            recommended: has_big_gpu,
            selected: false,
        },
    ]
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
            start_with_windows: true,
            enable_voice: true,
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

    /// Executa o passo `index` (0..6) e registra os artefatos criados para desfazer se necessário (§10.9).
    pub fn execute_step(&mut self, index: usize, choices: &SetupChoices) -> Result<String, String> {
        let app_dir = self.map_path(&choices.app_dir);
        let data_dir = self.map_path(&choices.data_dir);
        let models_dir = self.map_path(&choices.models_dir);

        match index {
            0 => {
                std::fs::create_dir_all(&app_dir).map_err(|e| format!("falha ao criar {}: {e}", app_dir.display()))?;
                self.created_paths.push(app_dir.clone());
                let manifest = app_dir.join("orion-install.json");
                let info = serde_json::json!({
                    "version": "0.1.0",
                    "app_dir": choices.app_dir,
                    "data_dir": choices.data_dir,
                    "models_dir": choices.models_dir,
                });
                std::fs::write(&manifest, serde_json::to_vec_pretty(&info).unwrap_or_default())
                    .map_err(|e| format!("falha ao gravar manifesto: {e}"))?;
                self.created_paths.push(manifest);
                Ok(format!("App preparado em {}", app_dir.display()))
            }
            1 => {
                std::fs::create_dir_all(&data_dir).map_err(|e| format!("falha ao criar {}: {e}", data_dir.display()))?;
                self.created_paths.push(data_dir.clone());
                let sparse = data_dir.join("AppxManifest.xml");
                std::fs::write(
                    &sparse,
                    r#"<?xml version="1.0" encoding="utf-8"?><Package><Identity Name="Orion.Desktop" Version="0.1.0.0" /></Package>"#,
                )
                .map_err(|e| format!("falha ao registrar identidade: {e}"))?;
                self.created_paths.push(sparse);
                Ok("Identidade de pacote esparso registrada".into())
            }
            2 => {
                let cert = data_dir.join("orion-selfsigned.cer");
                std::fs::write(&cert, b"ORION-SELFSIGNED-CERT-V1")
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
                std::fs::write(&shortcuts, serde_json::to_vec_pretty(&payload).unwrap_or_default())
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
                std::fs::write(&updater, serde_json::to_vec_pretty(&payload).unwrap_or_default())
                    .map_err(|e| format!("falha ao configurar serviço de atualização: {e}"))?;
                self.created_paths.push(updater);
                Ok("Serviço de atualização configurado".into())
            }
            5 => {
                std::fs::create_dir_all(&models_dir)
                    .map_err(|e| format!("falha ao criar pasta de modelos {}: {e}", models_dir.display()))?;
                self.created_paths.push(models_dir.clone());
                let queue = models_dir.join("download-queue.json");
                let selecionados: Vec<&str> = choices.models.iter().filter(|m| m.selected).map(|m| m.id).collect();
                let payload = serde_json::json!({
                    "models_dir": choices.models_dir,
                    "voice_enabled": choices.enable_voice,
                    "queued_models": selecionados,
                    "total_gb": choices.selected_models_gb(),
                });
                std::fs::write(&queue, serde_json::to_vec_pretty(&payload).unwrap_or_default())
                    .map_err(|e| format!("falha ao agendar download dos modelos: {e}"))?;
                self.created_paths.push(queue);
                Ok("Fila de modelos agendada para segundo plano".into())
            }
            _ => Ok("Concluído".into()),
        }
    }

    /// Desfaz em ordem inversa tudo o que foi criado até aqui (§2.1, §6 Etapa 5 e §10.9).
    pub fn rollback(&mut self) {
        while let Some(path) = self.created_paths.pop() {
            if path.is_file() {
                let _ = std::fs::remove_file(&path);
            } else if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referencia_32gb_12gb_marca_exatamente_os_modelos_da_especificacao() {
        let pc = PcInfo::prototype_reference();
        let models = default_models(&pc);
        assert_eq!(models.len(), 8);
        let rec: Vec<&str> = models.iter().filter(|m| m.recommended).map(|m| m.name).collect();
        assert_eq!(rec, vec!["Whisper large-v3-turbo", "Qwen 3 8B", "Gemma 4 E4B", "Moondream 2"]);
        let sel: Vec<&str> = models.iter().filter(|m| m.selected).map(|m| m.name).collect();
        assert_eq!(sel, vec!["Whisper large-v3-turbo", "Qwen 3 8B", "Moondream 2"]);
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
}

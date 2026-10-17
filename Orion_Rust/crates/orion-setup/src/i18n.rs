//! Textos das 5 etapas do `Orion Setup.exe` em pt-BR e Inglês
//! (`ORION_INSTALADOR_ESPECIFICACAO.md`, §6).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Language {
    #[default]
    PtBr,
    En,
}

impl Language {
    pub const ALL: [Language; 2] = [Language::PtBr, Language::En];

    pub fn label(self) -> &'static str {
        match self {
            Self::PtBr => "Português (Brasil)",
            Self::En => "English",
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Nome da estrela exibida no canto inferior direito do céu por etapa (§6 e §8.11).
pub fn star_name(step: u8, lang: Language) -> &'static str {
    match (step, lang) {
        (1, _) => "Betelgeuse",
        (2, _) => "Bellatrix",
        (3, Language::PtBr) => "Cinturão",
        (3, Language::En) => "Orion's Belt",
        (4, _) => "Saiph",
        _ => "Rigel",
    }
}

pub fn step_counter(step: u8, lang: Language) -> String {
    match lang {
        Language::PtBr => format!("Etapa {step} de 5"),
        Language::En => format!("Step {step} of 5"),
    }
}

pub struct Texts {
    pub back: &'static str,
    pub next: &'static str,
    pub install: &'static str,
    pub open_orion: &'static str,
    pub change: &'static str,
    pub retry: &'static str,
    pub view_details: &'static str,
    pub cancel_title: &'static str,
    pub cancel_confirm: &'static str,
    pub cancel_keep: &'static str,
    // Etapa 1
    pub s1_eyebrow: &'static str,
    pub s1_title: &'static str,
    pub s1_lead: &'static str,
    pub s1_lang_label: &'static str,
    pub s1_pc_eyebrow: &'static str,
    pub s1_sys_label: &'static str,
    pub s1_ram_label: &'static str,
    pub s1_gpu_label: &'static str,
    pub s1_disk_label: &'static str,
    pub s1_footer: &'static str,
    pub s1_win10_block: &'static str,
    // Etapa 2
    pub s2_eyebrow: &'static str,
    pub s2_title: &'static str,
    pub s2_app_label: &'static str,
    pub s2_app_hint: &'static str,
    pub s2_models_label: &'static str,
    pub s2_data_label: &'static str,
    pub s2_data_hint: &'static str,
    pub s2_footer: &'static str,
    // Etapa 3
    pub s3_eyebrow: &'static str,
    pub s3_title: &'static str,
    pub s3_lead: &'static str,
    pub s3_rec_badge: &'static str,
    pub s3_bg_note: &'static str,
    // Etapa 4
    pub s4_eyebrow: &'static str,
    pub s4_title: &'static str,
    pub s4_opt_autostart: (&'static str, &'static str),
    pub s4_opt_voice: (&'static str, &'static str),
    pub s4_opt_taskbar: (&'static str, &'static str),
    pub s4_opt_crash: (&'static str, &'static str),
    pub s4_footer: &'static str,
    // Etapa 5
    pub s5_eyebrow: &'static str,
    pub s5_title_installing: &'static str,
    pub s5_title_done: &'static str,
    pub s5_title_error: &'static str,
    pub s5_footer_installing: &'static str,
    pub s5_footer_done: &'static str,
}

pub const PT_BR: Texts = Texts {
    back: "Voltar",
    next: "Continuar",
    install: "Instalar",
    open_orion: "Abrir Orion",
    change: "Alterar",
    retry: "Tentar de novo",
    view_details: "Ver detalhes",
    cancel_title: "Cancelar a instalação?",
    cancel_confirm: "Cancelar",
    cancel_keep: "Continuar instalando",
    s1_eyebrow: "BOAS-VINDAS",
    s1_title: "Vamos instalar o Orion",
    s1_lead: "Seu assistente de desktop. Leva uns 2 minutos. Os modelos locais baixam depois, em segundo plano, e o Orion já abre usando as APIs enquanto isso.",
    s1_lang_label: "Idioma",
    s1_pc_eyebrow: "SEU PC",
    s1_sys_label: "Sistema",
    s1_ram_label: "Memória",
    s1_gpu_label: "GPU",
    s1_disk_label: "Espaço livre",
    s1_footer: "Instalação para todos os usuários",
    s1_win10_block: "O Orion precisa do Windows 11.",
    s2_eyebrow: "ONDE INSTALAR",
    s2_title: "Escolha as pastas",
    s2_app_label: "App",
    s2_app_hint: "~150 MB",
    s2_models_label: "Modelos locais",
    s2_data_label: "Dados (banco cifrado e configurações)",
    s2_data_hint: "O vault do Obsidian é escolhido no primeiro uso.",
    s2_footer: "Pede permissão de administrador uma vez",
    s3_eyebrow: "MODELOS LOCAIS",
    s3_title: "O que roda no seu PC",
    s3_lead: "Todos ficam descarregados e só ocupam RAM quando são chamados. Dá para mudar depois em Configurações › Modelos locais.",
    s3_rec_badge: "recomendado p/ seu PC",
    s3_bg_note: "baixa depois, em segundo plano",
    s4_eyebrow: "OPÇÕES",
    s4_title: "Últimos ajustes",
    s4_opt_autostart: ("Iniciar com o Windows", "Abre minimizado na bandeja"),
    s4_opt_voice: ("Ativar voz", "Palavra \"Orion\" e duas palmas"),
    s4_opt_taskbar: ("Fixar na barra de tarefas", "O atalho no Menu Iniciar é sempre criado"),
    s4_opt_crash: ("Enviar relatório de falhas", "Só para a automelhoria do Orion"),
    s4_footer: "Tudo pode ser mudado depois",
    s5_eyebrow: "INSTALANDO",
    s5_title_installing: "Formando a constelação…",
    s5_title_done: "Pronto. O Orion está no céu.",
    s5_title_error: "Algo deu errado",
    s5_footer_installing: "Instalando componentes do sistema…",
    s5_footer_done: "Os modelos continuam baixando no app",
};

pub const EN: Texts = Texts {
    back: "Back",
    next: "Continue",
    install: "Install",
    open_orion: "Open Orion",
    change: "Change",
    retry: "Try again",
    view_details: "View details",
    cancel_title: "Cancel installation?",
    cancel_confirm: "Cancel",
    cancel_keep: "Keep installing",
    s1_eyebrow: "WELCOME",
    s1_title: "Let's install Orion",
    s1_lead: "Your desktop assistant. Takes about 2 minutes. Local models download later in the background, and Orion opens right away using APIs in the meantime.",
    s1_lang_label: "Language",
    s1_pc_eyebrow: "YOUR PC",
    s1_sys_label: "System",
    s1_ram_label: "Memory",
    s1_gpu_label: "GPU",
    s1_disk_label: "Free space",
    s1_footer: "Installation for all users",
    s1_win10_block: "Orion requires Windows 11.",
    s2_eyebrow: "WHERE TO INSTALL",
    s2_title: "Choose folders",
    s2_app_label: "App",
    s2_app_hint: "~150 MB",
    s2_models_label: "Local models",
    s2_data_label: "Data (encrypted database and settings)",
    s2_data_hint: "Your Obsidian vault is chosen on first launch.",
    s2_footer: "Requests administrator permission once",
    s3_eyebrow: "LOCAL MODELS",
    s3_title: "What runs on your PC",
    s3_lead: "All models stay unloaded and only use RAM when called. You can change them later in Settings › Local models.",
    s3_rec_badge: "recommended for your PC",
    s3_bg_note: "downloads later in background",
    s4_eyebrow: "OPTIONS",
    s4_title: "Final adjustments",
    s4_opt_autostart: ("Start with Windows", "Opens minimized to the system tray"),
    s4_opt_voice: ("Enable voice", "Wake word \"Orion\" and double clap"),
    s4_opt_taskbar: ("Pin to taskbar", "Start Menu shortcut is always created"),
    s4_opt_crash: ("Send crash reports", "Only for Orion self-improvement"),
    s4_footer: "Everything can be changed later",
    s5_eyebrow: "INSTALLING",
    s5_title_installing: "Forming the constellation…",
    s5_title_done: "Ready. Orion is in the sky.",
    s5_title_error: "Something went wrong",
    s5_footer_installing: "Installing system components…",
    s5_footer_done: "Models continue downloading in the app",
};

pub fn texts(lang: Language) -> &'static Texts {
    match lang {
        Language::PtBr => &PT_BR,
        Language::En => &EN,
    }
}

pub fn install_step_texts(total_gb: f32, lang: Language) -> [String; 6] {
    let gb_str = format_gb(total_gb, lang);
    match lang {
        Language::PtBr => [
            "Copiando o Orion para Arquivos de Programas".into(),
            "Registrando a identidade do app (OCR e avisos)".into(),
            "Instalando o certificado do Orion".into(),
            "Criando atalhos no Menu Iniciar e na barra".into(),
            "Configurando o serviço de atualização".into(),
            format!("Agendando o download dos modelos ({gb_str})"),
        ],
        Language::En => [
            "Copying Orion to Program Files".into(),
            "Registering package identity (OCR and notifications)".into(),
            "Installing Orion certificate".into(),
            "Creating Start Menu and taskbar shortcuts".into(),
            "Configuring update service".into(),
            format!("Scheduling model downloads ({gb_str})"),
        ],
    }
}

pub fn format_gb(gb: f32, lang: Language) -> String {
    let rounded = (gb * 10.0).round() / 10.0;
    let raw = if (rounded - rounded.round()).abs() < 0.05 {
        format!("{:.0} GB", rounded)
    } else {
        format!("{:.1} GB", rounded)
    };
    match lang {
        Language::PtBr => raw.replace('.', ","),
        Language::En => raw,
    }
}

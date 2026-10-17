//! Início automático do Orion no login (fase 5: "autostart por SO").
//!
//! Gera o arquivo certo para cada sistema; NÃO executa nada do sistema (nem `launchctl`, nem
//! `systemctl`, nem `schtasks`): imprime o comando de ativação para o Antônio rodar. O serviço sobe
//! com este executável e com a pasta do projeto como diretório de trabalho (é de lá que o `.env` é
//! lido).

use std::path::{Path, PathBuf};

pub const PLATAFORMAS: [&str; 3] = ["windows", "macos", "linux"];
const ROTULO: &str = "com.orion.assistente";
const ROTULO_PONTE: &str = "com.orion.ponte";

#[derive(Debug, Clone)]
pub struct Autostart {
    pub plataforma: String,
    /// Onde o sistema procura.
    pub arquivo: PathBuf,
    pub conteudo: String,
    /// Comando (ou passo) para ligar.
    pub ativar: String,
    pub desativar: String,
}

pub fn detectar() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn escape_xml(t: &str) -> String {
    t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// `ponte=true` gera o início da ponte de desktop (`orion ponte`), um arquivo à parte: ela precisa de
/// sessão gráfica, o servidor não.
pub fn render(plataforma: &str, exe: &Path, projeto: &Path, logs: &Path, home: &Path, ponte: bool) -> Result<Autostart, String> {
    let comando = if ponte {
        "ponte"
    } else if plataforma == "windows" {
        "desktop --background"
    } else {
        "serve"
    };
    let nome = if ponte { "orion-ponte" } else { "orion" };
    let rotulo = if ponte { ROTULO_PONTE } else { ROTULO };
    let exe_s = exe.to_string_lossy();
    let proj = projeto.to_string_lossy();
    match plataforma {
        "windows" => {
            let appdata = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| home.join("AppData").join("Roaming"));
            let arquivo = appdata.join("Microsoft").join("Windows").join("Start Menu").join("Programs").join("Startup").join(format!("{nome}.cmd"));
            let conteudo = format!(
                "@echo off\r\nrem Gerado por `orion autostart`. Apague este arquivo para desligar o inicio.\r\ncd /d \"{}\"\r\nstart \"Orion\" /min \"{}\" {comando}\r\n",
                proj.replace('%', "%%"),
                exe_s.replace('%', "%%")
            );
            let del = format!("del \"{}\"", arquivo.display());
            Ok(Autostart {
                plataforma: plataforma.into(),
                arquivo,
                conteudo,
                ativar: "Sai da sessão e entra de novo, ou rode o arquivo uma vez para testar.".into(),
                desativar: del,
            })
        }
        "macos" => {
            let arquivo = home.join("Library").join("LaunchAgents").join(format!("{rotulo}.plist"));
            let args: String = [exe_s.as_ref(), comando].iter().map(|a| format!("\n    <string>{}</string>", escape_xml(a))).collect();
            let saida = if ponte { "orion-ponte" } else { "orion" };
            let conteudo = format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key><string>{rotulo}</string>\n  <key>ProgramArguments</key>\n  <array>{args}\n  </array>\n  <key>WorkingDirectory</key><string>{}</string>\n  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><true/>\n  <key>StandardOutPath</key><string>{}</string>\n  <key>StandardErrorPath</key><string>{}</string>\n</dict>\n</plist>\n",
                escape_xml(&proj),
                escape_xml(&logs.join(format!("{saida}.out.log")).to_string_lossy()),
                escape_xml(&logs.join(format!("{saida}.err.log")).to_string_lossy())
            );
            let (a, d) = (
                format!("launchctl bootstrap \"gui/$(id -u)\" \"{}\"", arquivo.display()),
                format!("launchctl bootout \"gui/$(id -u)\" \"{0}\" && rm \"{0}\"", arquivo.display()),
            );
            Ok(Autostart { plataforma: plataforma.into(), arquivo, conteudo, ativar: a, desativar: d })
        }
        "linux" => {
            let arquivo = home.join(".config").join("systemd").join("user").join(format!("{nome}.service"));
            let conteudo = format!(
                "[Unit]\nDescription=Orion ({})\nAfter={}\n\n[Service]\nType=simple\nWorkingDirectory={}\nExecStart=\"{}\" {comando}\nRestart=on-failure\nRestartSec=5\n\n[Install]\nWantedBy={}\n",
                if ponte { "ponte de desktop" } else { "assistente pessoal" },
                if ponte { "graphical-session.target" } else { "network-online.target" },
                proj.replace('%', "%%"),
                exe_s.replace('%', "%%"),
                if ponte { "graphical-session.target" } else { "default.target" }
            );
            let (a, d) = (
                format!("systemctl --user daemon-reload && systemctl --user enable --now {nome}.service"),
                format!("systemctl --user disable --now {nome}.service && rm \"{}\"", arquivo.display()),
            );
            Ok(Autostart { plataforma: plataforma.into(), arquivo, conteudo, ativar: a, desativar: d })
        }
        o => Err(format!("plataforma desconhecida: {o}. Use: {}", PLATAFORMAS.join(", "))),
    }
}

/// Grava o arquivo (em `destino`, se dado). Não sobrescreve sem `force`.
pub fn instalar(a: &Autostart, destino: Option<&Path>, force: bool) -> Result<PathBuf, String> {
    let alvo = match destino {
        Some(d) => d.join(a.arquivo.file_name().unwrap_or_default()),
        None => a.arquivo.clone(),
    };
    if alvo.exists() && !force {
        return Err(format!("{} já existe (use --force para substituir)", alvo.display()));
    }
    if let Some(p) = alvo.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(&alvo, &a.conteudo).map_err(|e| e.to_string())?;
    Ok(alvo)
}

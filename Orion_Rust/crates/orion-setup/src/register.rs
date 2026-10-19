//! Registro do Orion no Windows (Apps instalados, Menu Iniciar, início automático) e remoção.
//!
//! Usa só ferramentas do próprio sistema (`reg.exe` e PowerShell/WScript.Shell), sempre com
//! argumentos separados ou variáveis de ambiente, nunca montando comando em texto. Em prévia e em
//! teste entra o [`RecordingRegistrar`], que só anota o que faria.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Orion";
pub const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
pub const AUTOSTART_NAME: &str = "Orion";
pub const MANIFEST: &str = "orion-install.json";
pub const UNINSTALLER: &str = "Orion Setup.exe";

#[derive(Debug, Clone)]
pub struct UninstallEntry {
    pub display_name: String,
    pub version: String,
    pub publisher: String,
    pub install_location: PathBuf,
    pub uninstall_command: String,
    pub icon: PathBuf,
}

pub trait Registrar: Send + Sync {
    fn register_uninstall(&self, entry: &UninstallEntry) -> Result<(), String>;
    fn unregister_uninstall(&self) -> Result<(), String>;
    fn create_shortcut(&self, lnk: &Path, target: &Path, args: &str, workdir: &Path) -> Result<(), String>;
    fn set_autostart(&self, command: &str) -> Result<(), String>;
    fn remove_autostart(&self) -> Result<(), String>;
}

/// Registrar real: só funciona no Windows; nos outros sistemas recusa em vez de fingir.
pub struct WindowsRegistrar;

#[cfg(windows)]
fn run(cmd: &mut std::process::Command) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&[out.stderr, out.stdout].concat()).trim().to_string())
    }
}

#[cfg(windows)]
fn reg_set(key: &str, name: &str, kind: &str, value: &str) -> Result<(), String> {
    run(std::process::Command::new("reg").args(["add", key, "/v", name, "/t", kind, "/d", value, "/f"]))
}

#[cfg(windows)]
fn reg_delete(key: &str, name: Option<&str>) -> Result<(), String> {
    let mut c = std::process::Command::new("reg");
    c.args(["delete", key]);
    match name {
        Some(n) => c.args(["/v", n, "/f"]),
        None => c.arg("/f"),
    };
    run(&mut c)
}

impl Registrar for WindowsRegistrar {
    #[cfg(windows)]
    fn register_uninstall(&self, e: &UninstallEntry) -> Result<(), String> {
        let loc = e.install_location.to_string_lossy();
        for (n, k, v) in [
            ("DisplayName", "REG_SZ", e.display_name.as_str()),
            ("DisplayVersion", "REG_SZ", e.version.as_str()),
            ("Publisher", "REG_SZ", e.publisher.as_str()),
            ("InstallLocation", "REG_SZ", &loc),
            ("UninstallString", "REG_SZ", e.uninstall_command.as_str()),
            ("DisplayIcon", "REG_SZ", &e.icon.to_string_lossy()),
            ("NoModify", "REG_DWORD", "1"),
            ("NoRepair", "REG_DWORD", "1"),
        ] {
            reg_set(UNINSTALL_KEY, n, k, v)?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    fn register_uninstall(&self, _: &UninstallEntry) -> Result<(), String> {
        Err("registro de aplicativos só existe no Windows".into())
    }

    #[cfg(windows)]
    fn unregister_uninstall(&self) -> Result<(), String> {
        reg_delete(UNINSTALL_KEY, None)
    }
    #[cfg(not(windows))]
    fn unregister_uninstall(&self) -> Result<(), String> {
        Err("registro de aplicativos só existe no Windows".into())
    }

    #[cfg(windows)]
    fn create_shortcut(&self, lnk: &Path, target: &Path, args: &str, workdir: &Path) -> Result<(), String> {
        if let Some(p) = lnk.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        // Caminhos vão por variável de ambiente: nada do usuário entra no texto do script.
        run(std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg("$s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:ORION_LNK);$s.TargetPath=$env:ORION_TARGET;$s.Arguments=$env:ORION_ARGS;$s.WorkingDirectory=$env:ORION_WORKDIR;$s.Description='Orion';$s.Save()")
            .env("ORION_LNK", lnk)
            .env("ORION_TARGET", target)
            .env("ORION_ARGS", args)
            .env("ORION_WORKDIR", workdir))
    }
    #[cfg(not(windows))]
    fn create_shortcut(&self, _: &Path, _: &Path, _: &str, _: &Path) -> Result<(), String> {
        Err("atalhos .lnk só existem no Windows".into())
    }

    #[cfg(windows)]
    fn set_autostart(&self, command: &str) -> Result<(), String> {
        reg_set(RUN_KEY, AUTOSTART_NAME, "REG_SZ", command)
    }
    #[cfg(not(windows))]
    fn set_autostart(&self, _: &str) -> Result<(), String> {
        Err("início automático só existe no Windows".into())
    }

    #[cfg(windows)]
    fn remove_autostart(&self) -> Result<(), String> {
        reg_delete(RUN_KEY, Some(AUTOSTART_NAME))
    }
    #[cfg(not(windows))]
    fn remove_autostart(&self) -> Result<(), String> {
        Err("início automático só existe no Windows".into())
    }
}

/// Registrar de prévia e de teste: guarda as chamadas e não toca no sistema.
#[derive(Default)]
pub struct RecordingRegistrar {
    pub calls: Mutex<Vec<String>>,
}

impl RecordingRegistrar {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().clone()
    }
}

impl Registrar for RecordingRegistrar {
    fn register_uninstall(&self, e: &UninstallEntry) -> Result<(), String> {
        self.calls.lock().push(format!("register_uninstall {}", e.uninstall_command));
        Ok(())
    }
    fn unregister_uninstall(&self) -> Result<(), String> {
        self.calls.lock().push("unregister_uninstall".into());
        Ok(())
    }
    fn create_shortcut(&self, lnk: &Path, target: &Path, args: &str, _: &Path) -> Result<(), String> {
        self.calls.lock().push(format!("create_shortcut {} -> {} {args}", lnk.display(), target.display()));
        Ok(())
    }
    fn set_autostart(&self, command: &str) -> Result<(), String> {
        self.calls.lock().push(format!("set_autostart {command}"));
        Ok(())
    }
    fn remove_autostart(&self) -> Result<(), String> {
        self.calls.lock().push("remove_autostart".into());
        Ok(())
    }
}

/// Atalho do Menu Iniciar do usuário (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Orion.lnk`).
pub fn start_menu_shortcut() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\Default\AppData\Roaming"));
    base.join(r"Microsoft\Windows\Start Menu\Programs\Orion.lnk")
}

/// O que a instalação deixou, lido pela desinstalação. Só o que está aqui é removido.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstallManifest {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub app_dir: PathBuf,
    #[serde(default)]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub models_dir: PathBuf,
    /// Arquivos do app com o SHA-256 de quando foram copiados.
    #[serde(default)]
    pub files: Vec<InstalledFile>,
    #[serde(default)]
    pub shortcut: Option<PathBuf>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub registered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledFile {
    pub name: String,
    pub sha256: String,
}

impl InstallManifest {
    pub fn load(app_dir: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(app_dir.join(MANIFEST)).map_err(|e| format!("não achei {MANIFEST} em {}: {e}", app_dir.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{MANIFEST} inválido: {e}"))
    }

    pub fn save(&self, app_dir: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(app_dir.join(MANIFEST), bytes).map_err(|e| e.to_string())
    }
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct UninstallReport {
    pub removed_files: usize,
    /// Arquivos que mudaram depois da instalação e foram mantidos.
    pub kept_modified: Vec<String>,
    pub removed_data: bool,
}

/// Remove o que a instalação registrou. Dados e modelos só saem com `remove_data`; arquivo do app
/// alterado depois da instalação fica no lugar.
pub fn uninstall(app_dir: &Path, registrar: &dyn Registrar, remove_data: bool) -> Result<UninstallReport, String> {
    let manifest = InstallManifest::load(app_dir)?;
    let mut report = UninstallReport::default();
    if manifest.autostart {
        let _ = registrar.remove_autostart();
    }
    if let Some(lnk) = &manifest.shortcut {
        let _ = std::fs::remove_file(lnk);
    }
    if manifest.registered {
        registrar.unregister_uninstall().map_err(|e| format!("falha ao tirar o Orion de Apps instalados: {e}"))?;
    }
    for f in &manifest.files {
        // nome vindo do JSON: nunca sai da pasta do app
        if Path::new(&f.name).components().count() != 1 || f.name == ".." {
            continue;
        }
        let path = app_dir.join(&f.name);
        if !path.is_file() {
            continue;
        }
        // O desinstalador em execução (cópia em %TEMP%) não é este arquivo; o do app pode sair.
        if sha256_file(&path).ok().as_deref() != Some(f.sha256.as_str()) && f.name != UNINSTALLER {
            report.kept_modified.push(f.name.clone());
            continue;
        }
        if std::fs::remove_file(&path).is_ok() {
            report.removed_files += 1;
        }
    }
    let _ = std::fs::remove_file(app_dir.join(MANIFEST));
    let _ = std::fs::remove_file(app_dir.join("orion-files.sha256"));
    let _ = std::fs::remove_dir(app_dir); // só some se ficou vazia
    if remove_data && manifest.data_dir.is_dir() {
        report.removed_data = std::fs::remove_dir_all(&manifest.data_dir).is_ok();
    }
    Ok(report)
}

/// Trata `--uninstall <pasta> [--remove-data]` (chamado por Apps instalados). Devolve o código de
/// saída, ou `None` se os argumentos não pedem desinstalação.
///
/// Um .exe em execução não se apaga: se estamos dentro da pasta do app, o desinstalador copia a si
/// mesmo para %TEMP%, reabre dali com `--relocated` e sai.
pub fn handle_uninstall_args(args: &[String]) -> Option<i32> {
    let pos = args.iter().position(|a| a == "--uninstall")?;
    let dir = PathBuf::from(args.get(pos + 1)?);
    let remove_data = args.iter().any(|a| a == "--remove-data");
    let relocated = args.iter().any(|a| a == "--relocated");
    let log = |msg: &str| {
        let _ = std::fs::write(std::env::temp_dir().join("orion-uninstall.log"), msg);
    };
    if !relocated {
        let me = std::env::current_exe().ok()?;
        let inside = me.parent().and_then(|p| p.canonicalize().ok()) == dir.canonicalize().ok();
        if inside {
            let copy = std::env::temp_dir().join(format!("orion-uninstall-{}.exe", std::process::id()));
            if let Err(e) = std::fs::copy(&me, &copy) {
                log(&format!("não consegui copiar o desinstalador: {e}"));
                return Some(1);
            }
            let mut cmd = std::process::Command::new(&copy);
            cmd.arg("--uninstall").arg(&dir).arg("--relocated");
            if remove_data {
                cmd.arg("--remove-data");
            }
            return Some(match cmd.spawn() {
                Ok(_) => 0,
                Err(e) => {
                    log(&format!("não consegui reabrir o desinstalador: {e}"));
                    1
                }
            });
        }
    }
    Some(match uninstall(&dir, &WindowsRegistrar, remove_data) {
        Ok(r) => {
            log(&format!("removidos {} arquivos; mantidos por alteração: {:?}; dados removidos: {}", r.removed_files, r.kept_modified, r.removed_data));
            0
        }
        Err(e) => {
            log(&e);
            1
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instalado(dir: &Path, autostart: bool) -> InstallManifest {
        let app = dir.join("app");
        let data = dir.join("data");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(app.join("orion.exe"), b"exe").unwrap();
        std::fs::write(app.join(UNINSTALLER), b"setup").unwrap();
        std::fs::write(data.join("orion.db"), b"dados").unwrap();
        let lnk = dir.join("Orion.lnk");
        std::fs::write(&lnk, b"lnk").unwrap();
        let m = InstallManifest {
            version: "0.1.0".into(),
            app_dir: app.clone(),
            data_dir: data,
            files: ["orion.exe", UNINSTALLER].iter().map(|n| InstalledFile { name: n.to_string(), sha256: sha256_file(&app.join(n)).unwrap() }).collect(),
            shortcut: Some(lnk),
            autostart,
            registered: true,
            ..Default::default()
        };
        m.save(&app).unwrap();
        m
    }

    #[test]
    fn desinstalar_remove_o_app_e_preserva_os_dados() {
        let tmp = tempfile::tempdir().unwrap();
        let m = instalado(tmp.path(), true);
        let reg = RecordingRegistrar::default();
        let r = uninstall(&m.app_dir, &reg, false).unwrap();
        assert_eq!(r.removed_files, 2);
        assert!(!m.app_dir.exists(), "pasta vazia some");
        assert!(m.data_dir.join("orion.db").exists());
        assert!(!m.shortcut.unwrap().exists());
        assert_eq!(reg.calls(), ["remove_autostart", "unregister_uninstall"]);
    }

    #[test]
    fn desinstalar_pode_apagar_os_dados_quando_pedido() {
        let tmp = tempfile::tempdir().unwrap();
        let m = instalado(tmp.path(), false);
        let r = uninstall(&m.app_dir, &RecordingRegistrar::default(), true).unwrap();
        assert!(r.removed_data && !m.data_dir.exists());
    }

    #[test]
    fn arquivo_do_app_alterado_depois_da_instalacao_fica() {
        let tmp = tempfile::tempdir().unwrap();
        let m = instalado(tmp.path(), false);
        std::fs::write(m.app_dir.join("orion.exe"), b"outra versao").unwrap();
        let r = uninstall(&m.app_dir, &RecordingRegistrar::default(), false).unwrap();
        assert_eq!(r.kept_modified, ["orion.exe"]);
        assert!(m.app_dir.join("orion.exe").exists());
    }

    #[test]
    fn manifesto_com_nome_fora_da_pasta_nao_apaga_nada_la() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = instalado(tmp.path(), false);
        let fora = tmp.path().join("fora.txt");
        std::fs::write(&fora, b"x").unwrap();
        m.files.push(InstalledFile { name: "../fora.txt".into(), sha256: sha256_file(&fora).unwrap() });
        m.save(&m.app_dir).unwrap();
        uninstall(&m.app_dir, &RecordingRegistrar::default(), false).unwrap();
        assert!(fora.exists());
    }

    #[test]
    fn sem_manifesto_nao_remove_nada() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("importante.txt"), b"x").unwrap();
        assert!(uninstall(tmp.path(), &RecordingRegistrar::default(), true).is_err());
        assert!(tmp.path().join("importante.txt").exists());
    }

    #[cfg(not(windows))]
    #[test]
    fn registrar_real_recusa_fora_do_windows() {
        assert!(WindowsRegistrar.set_autostart("x").is_err());
    }
}

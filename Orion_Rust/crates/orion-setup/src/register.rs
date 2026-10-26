//! Registro do Orion no Windows (Apps instalados, Menu Iniciar, início automático) e remoção.
//!
//! Usa só ferramentas do próprio sistema (`reg.exe` e PowerShell/WScript.Shell), sempre com
//! argumentos separados ou variáveis de ambiente, nunca montando comando em texto. Em prévia e em
//! teste entra o [`RecordingRegistrar`], que só anota o que faria.

#[cfg_attr(not(windows), allow(unused_imports))]
use crate::identity::{self, IdentityOutcome};
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
    /// Identidade de pacote esparso com certificado autoassinado (ver `identity`).
    fn register_identity(&self, app_dir: &Path, version: &str) -> Result<IdentityOutcome, String>;
    fn unregister_identity(&self, thumbprint: &str) -> Result<(), String>;
    /// Tarefa diária (SYSTEM) que roda `orion.exe update --check` sem pedir UAC a cada versão.
    fn install_update_task(&self, orion: &Path) -> Result<(), String>;
    fn remove_update_task(&self) -> Result<(), String>;
}

pub const UPDATE_TASK: &str = "OrionUpdater";

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
fn run_output(cmd: &mut std::process::Command) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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

    #[cfg(windows)]
    fn register_identity(&self, app_dir: &Path, version: &str) -> Result<IdentityOutcome, String> {
        use rand::RngCore;
        let work = std::env::temp_dir().join(format!("orion-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        identity::write_package_dir(&work.join("pkg"), version).map_err(|e| e.to_string())?;
        let mut pw = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut pw);
        let out = run_output(
            std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", identity::REGISTER_SCRIPT])
                .env("ORION_PKGDIR", work.join("pkg"))
                .env("ORION_MSIX", work.join("Orion.msix"))
                .env("ORION_PFX", work.join("orion.pfx"))
                .env("ORION_CER", work.join("orion.cer"))
                .env("ORION_PFX_PASSWORD", hex::encode(pw))
                .env("ORION_APPDIR", app_dir),
        );
        // a chave privada do certificado não fica em disco em nenhum caso
        let _ = std::fs::remove_dir_all(&work);
        identity::parse_script_output(&out?)
    }
    #[cfg(not(windows))]
    fn register_identity(&self, _: &Path, _: &str) -> Result<IdentityOutcome, String> {
        Err("identidade de pacote só existe no Windows".into())
    }

    #[cfg(windows)]
    fn unregister_identity(&self, thumbprint: &str) -> Result<(), String> {
        if !identity::valid_thumbprint(thumbprint) {
            return Err(format!("thumbprint inválido: {thumbprint}"));
        }
        run_output(std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", identity::UNREGISTER_SCRIPT]).env("ORION_THUMBPRINT", thumbprint)).map(|_| ())
    }
    #[cfg(not(windows))]
    fn unregister_identity(&self, _: &str) -> Result<(), String> {
        Err("identidade de pacote só existe no Windows".into())
    }

    #[cfg(windows)]
    fn install_update_task(&self, orion: &Path) -> Result<(), String> {
        let tr = format!("\"{}\" update --check", orion.display());
        run(std::process::Command::new("schtasks").args(["/Create", "/TN", UPDATE_TASK, "/SC", "DAILY", "/ST", "12:30", "/RU", "SYSTEM", "/RL", "HIGHEST", "/TR", &tr, "/F"]))
    }
    #[cfg(not(windows))]
    fn install_update_task(&self, _: &Path) -> Result<(), String> {
        Err("tarefa agendada só existe no Windows".into())
    }

    #[cfg(windows)]
    fn remove_update_task(&self) -> Result<(), String> {
        run(std::process::Command::new("schtasks").args(["/Delete", "/TN", UPDATE_TASK, "/F"]))
    }
    #[cfg(not(windows))]
    fn remove_update_task(&self) -> Result<(), String> {
        Err("tarefa agendada só existe no Windows".into())
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
    fn register_identity(&self, _: &Path, version: &str) -> Result<IdentityOutcome, String> {
        self.calls.lock().push(format!("register_identity {version}"));
        Ok(IdentityOutcome::Registered { thumbprint: "A".repeat(40) })
    }
    fn unregister_identity(&self, thumbprint: &str) -> Result<(), String> {
        self.calls.lock().push(format!("unregister_identity {thumbprint}"));
        Ok(())
    }
    fn install_update_task(&self, orion: &Path) -> Result<(), String> {
        self.calls.lock().push(format!("install_update_task {}", orion.display()));
        Ok(())
    }
    fn remove_update_task(&self) -> Result<(), String> {
        self.calls.lock().push("remove_update_task".into());
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
    /// Este `Orion Setup.exe` entende `--whats-new`; instalações antigas não têm a marca e o app não o chama.
    #[serde(default)]
    pub whats_new: bool,
    /// Arquivos do app com o SHA-256 de quando foram copiados.
    #[serde(default)]
    pub files: Vec<InstalledFile>,
    #[serde(default)]
    pub shortcut: Option<PathBuf>,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub registered: bool,
    /// Thumbprint do certificado autoassinado da identidade de pacote, se foi registrada.
    #[serde(default)]
    pub identity_thumbprint: Option<String>,
    #[serde(default)]
    pub update_task: bool,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct UninstallOptions {
    /// Modelos baixados pelo instalador (só os arquivos registrados em `orion-local.json`).
    pub remove_models: bool,
    /// Pasta de dados do Orion (banco, configurações). O vault nunca está aqui nem é tocado.
    pub remove_data: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct UninstallReport {
    pub removed_files: usize,
    /// Arquivos que mudaram depois da instalação e foram mantidos.
    pub kept_modified: Vec<String>,
    pub removed_models: usize,
    pub removed_data: bool,
}

/// Só apaga pasta com cara de pasta do Orion e com profundidade mínima, para um caminho digitado
/// errado (`C:\`, `C:\Users`) nunca virar `remove_dir_all`.
fn pasta_do_orion(p: &Path) -> bool {
    let normais = p.components().filter(|c| matches!(c, std::path::Component::Normal(_))).count();
    normais >= 2 && p.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().contains("orion"))
}

/// Remove os modelos que o instalador registrou. Arquivos que não estão em `orion-local.json`
/// (por exemplo outros GGUF na mesma pasta) ficam.
fn remover_modelos(manifest: &InstallManifest) -> usize {
    let Ok(bytes) = std::fs::read(manifest.data_dir.join("orion-local.json")) else { return 0 };
    let Ok(cfg) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return 0 };
    let mut removidos = 0;
    let nomes = cfg["installed_models"].as_array().into_iter().flatten().filter_map(|m| m["filename"].as_str());
    for nome in nomes {
        if Path::new(nome).components().count() == 1 && std::fs::remove_file(manifest.models_dir.join(nome)).is_ok() {
            removidos += 1;
        }
    }
    for extra in ["orion-wake/orion.onnx", "orion-wake/orion.json"] {
        if std::fs::remove_file(manifest.models_dir.join(extra)).is_ok() {
            removidos += 1;
        }
    }
    let _ = std::fs::remove_dir(manifest.models_dir.join("orion-wake"));
    let _ = std::fs::remove_dir(&manifest.models_dir); // só some se ficou vazia
    removidos
}

/// Remove o que a instalação registrou. Arquivo do app alterado depois da instalação fica no lugar;
/// modelos e dados só saem se pedidos.
pub fn uninstall(app_dir: &Path, registrar: &dyn Registrar, options: UninstallOptions) -> Result<UninstallReport, String> {
    let manifest = InstallManifest::load(app_dir)?;
    let mut report = UninstallReport::default();
    if manifest.autostart {
        let _ = registrar.remove_autostart();
    }
    if let Some(lnk) = &manifest.shortcut {
        let _ = std::fs::remove_file(lnk);
    }
    if let Some(t) = &manifest.identity_thumbprint {
        let _ = registrar.unregister_identity(t);
    }
    if manifest.update_task {
        let _ = registrar.remove_update_task();
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
    if options.remove_models {
        report.removed_models = remover_modelos(&manifest);
    }
    if options.remove_data && manifest.data_dir.is_dir() && pasta_do_orion(&manifest.data_dir) {
        report.removed_data = std::fs::remove_dir_all(&manifest.data_dir).is_ok();
    }
    Ok(report)
}

/// O que o processo deve fazer depois de ler `--uninstall`.
#[derive(Debug, PartialEq, Eq)]
pub enum UninstallAction {
    /// Terminou (ou falhou): sai com este código.
    Exit(i32),
    /// Abrir a janela com as três caixas para esta pasta.
    Ui(PathBuf),
}

/// Trata `--uninstall <pasta> [--silent] [--remove-models] [--remove-data]` (chamado por Apps
/// instalados). Devolve `None` se os argumentos não pedem desinstalação. Sem `--silent`, mostra a
/// janela com as caixas; com `--silent`, remove só o app, mais o que os `--remove-*` pedirem.
///
/// Um .exe em execução não se apaga: se estamos dentro da pasta do app, o desinstalador copia a si
/// mesmo para %TEMP%, reabre dali com `--relocated` e sai.
pub fn handle_uninstall_args(args: &[String]) -> Option<UninstallAction> {
    let pos = args.iter().position(|a| a == "--uninstall")?;
    let dir = PathBuf::from(args.get(pos + 1)?);
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let options = UninstallOptions { remove_models: has("--remove-models"), remove_data: has("--remove-data") };
    let log = |msg: &str| {
        let _ = std::fs::write(std::env::temp_dir().join("orion-uninstall.log"), msg);
    };
    if !has("--relocated") {
        let me = std::env::current_exe().ok()?;
        let inside = me.parent().and_then(|p| p.canonicalize().ok()) == dir.canonicalize().ok();
        if inside {
            let copy = std::env::temp_dir().join(format!("orion-uninstall-{}.exe", std::process::id()));
            if let Err(e) = std::fs::copy(&me, &copy) {
                log(&format!("não consegui copiar o desinstalador: {e}"));
                return Some(UninstallAction::Exit(1));
            }
            let mut cmd = std::process::Command::new(&copy);
            cmd.args(&args[pos..]).arg("--relocated");
            return Some(UninstallAction::Exit(match cmd.spawn() {
                Ok(_) => 0,
                Err(e) => {
                    log(&format!("não consegui reabrir o desinstalador: {e}"));
                    1
                }
            }));
        }
    }
    if !has("--silent") && !options.remove_models && !options.remove_data {
        return Some(UninstallAction::Ui(dir));
    }
    Some(UninstallAction::Exit(match uninstall(&dir, &WindowsRegistrar, options) {
        Ok(r) => {
            log(&format!("{r:?}"));
            0
        }
        Err(e) => {
            log(&e);
            1
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instalado(dir: &Path, autostart: bool) -> InstallManifest {
        let app = dir.join("app");
        let data = dir.join("ProgramData").join("Orion");
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
        let r = uninstall(&m.app_dir, &reg, UninstallOptions::default()).unwrap();
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
        let r = uninstall(&m.app_dir, &RecordingRegistrar::default(), UninstallOptions { remove_data: true, ..Default::default() }).unwrap();
        assert!(r.removed_data && !m.data_dir.exists());
    }

    #[test]
    fn arquivo_do_app_alterado_depois_da_instalacao_fica() {
        let tmp = tempfile::tempdir().unwrap();
        let m = instalado(tmp.path(), false);
        std::fs::write(m.app_dir.join("orion.exe"), b"outra versao").unwrap();
        let r = uninstall(&m.app_dir, &RecordingRegistrar::default(), UninstallOptions::default()).unwrap();
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
        uninstall(&m.app_dir, &RecordingRegistrar::default(), UninstallOptions::default()).unwrap();
        assert!(fora.exists());
    }

    #[test]
    fn sem_manifesto_nao_remove_nada() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("importante.txt"), b"x").unwrap();
        assert!(uninstall(tmp.path(), &RecordingRegistrar::default(), UninstallOptions { remove_data: true, remove_models: true }).is_err());
        assert!(tmp.path().join("importante.txt").exists());
    }

    #[test]
    fn modelos_registrados_saem_e_arquivo_alheio_fica() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = instalado(tmp.path(), false);
        m.models_dir = tmp.path().join("Modelos");
        m.save(&m.app_dir).unwrap();
        std::fs::create_dir_all(&m.models_dir).unwrap();
        std::fs::write(m.models_dir.join("meu.gguf"), b"registrado").unwrap();
        std::fs::write(m.models_dir.join("outro.gguf"), b"de outra coisa").unwrap();
        std::fs::write(m.data_dir.join("orion-local.json"), br#"{"installed_models":[{"filename":"meu.gguf"},{"filename":"../orion.db"}]}"#).unwrap();
        let r = uninstall(&m.app_dir, &RecordingRegistrar::default(), UninstallOptions { remove_models: true, remove_data: false }).unwrap();
        assert_eq!(r.removed_models, 1);
        assert!(!m.models_dir.join("meu.gguf").exists());
        assert!(m.models_dir.join("outro.gguf").exists());
        assert!(m.data_dir.join("orion.db").exists());
    }

    #[test]
    fn dados_so_saem_de_pasta_com_cara_de_orion() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = instalado(tmp.path(), false);
        let generica = tmp.path().join("Documentos");
        std::fs::create_dir_all(&generica).unwrap();
        std::fs::write(generica.join("minha-tese.docx"), b"x").unwrap();
        m.data_dir = generica.clone();
        m.save(&m.app_dir).unwrap();
        let r = uninstall(&m.app_dir, &RecordingRegistrar::default(), UninstallOptions { remove_data: true, remove_models: false }).unwrap();
        assert!(!r.removed_data);
        assert!(generica.join("minha-tese.docx").exists());
        assert!(!pasta_do_orion(Path::new("/")) && !pasta_do_orion(Path::new("/orion")));
    }

    #[cfg(not(windows))]
    #[test]
    fn registrar_real_recusa_fora_do_windows() {
        assert!(WindowsRegistrar.set_autostart("x").is_err());
    }
}

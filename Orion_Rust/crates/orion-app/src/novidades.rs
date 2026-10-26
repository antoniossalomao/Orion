//! Aviso discreto de novidades (F01.26 e F10.25 do board): uma vez por versão, na primeira abertura visível depois de atualizar.
//! Usa o toast já existente; não abre janela nem interrompe a conversa.

use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Arquivo que guarda a última versão vista, ao lado dos dados do app.
fn state_file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("Orion").join("ultima_versao.txt"))
}

/// Há novidade a mostrar? Na primeira instalação grava a versão e não avisa, para não anunciar a própria instalação.
pub fn pending() -> bool {
    let Some(path) = state_file() else { return false };
    match std::fs::read_to_string(&path) {
        Ok(seen) => seen.trim() != VERSION,
        Err(_) => {
            mark_seen();
            false
        }
    }
}

/// Registra a versão atual como vista.
pub fn mark_seen() {
    let Some(path) = state_file() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, VERSION);
}

/// Inicialização em segundo plano (atalho de login): a janela fica escondida, então o aviso espera a próxima abertura visível.
pub fn started_in_background() -> bool {
    std::env::args().any(|a| a == "--background")
}

pub fn text(english: bool) -> String {
    if english {
        format!("Orion updated to {VERSION}. See Settings › System › About.")
    } else {
        format!("Orion atualizado para {VERSION}. Veja em Configurações › Sistema › Sobre.")
    }
}

/// Abre a mini tela "O que mudou" (do `Orion Setup.exe`, ao lado do app) se o atualizador deixou notas; sem elas, não faz nada.
pub fn open_whats_new() {
    let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) else { return };
    let setup = dir.join("Orion Setup.exe");
    if dir.join("whats-new.json").exists() && setup.exists() {
        let _ = std::process::Command::new(setup).arg("--whats-new").arg(&dir).spawn();
    }
}

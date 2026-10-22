//! Aviso discreto de novidades (F01.26 e F10.25 do board): uma vez por versão, na primeira abertura depois de atualizar.
//! Usa o toast já existente; não abre janela nem interrompe a conversa.

use std::path::PathBuf;

const VERSAO: &str = env!("CARGO_PKG_VERSION");

/// Arquivo que guarda a última versão aberta, ao lado dos dados do app.
fn arquivo() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("Orion").join("ultima_versao.txt"))
}

/// Devolve o texto do aviso quando a versão mudou desde a última abertura.
/// Na primeira execução grava a versão e não avisa, para não anunciar a própria instalação.
pub fn aviso() -> Option<String> {
    let caminho = arquivo()?;
    let visto = std::fs::read_to_string(&caminho).ok();
    if visto.as_deref().map(str::trim) == Some(VERSAO) {
        return None;
    }
    if let Some(pasta) = caminho.parent() {
        let _ = std::fs::create_dir_all(pasta);
    }
    let _ = std::fs::write(&caminho, VERSAO);
    visto.map(|_| format!("Orion atualizado para {VERSAO}. Veja em Configurações › Sistema › Sobre."))
}

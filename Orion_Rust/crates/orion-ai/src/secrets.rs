//! Segredos: variável de ambiente primeiro, cofre do SO depois (Credential Manager no Windows,
//! Keychain no macOS). Nunca registrar o valor em log
//! (REGRAS.md: segredos nunca em log, audit ou processo filho).

pub const SERVICO: &str = "orion";

pub fn get_secret(nome: &str) -> Option<String> {
    if let Ok(v) = std::env::var(nome) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    // sem backend de cofre (CI, servidor): segue sem segredo
    keyring::Entry::new(SERVICO, nome).ok()?.get_password().ok()
}

pub fn set_secret(nome: &str, valor: &str) -> anyhow::Result<()> {
    keyring::Entry::new(SERVICO, nome)?.set_password(valor)?;
    Ok(())
}

pub fn delete_secret(nome: &str) -> anyhow::Result<()> {
    keyring::Entry::new(SERVICO, nome)?.delete_credential()?;
    Ok(())
}

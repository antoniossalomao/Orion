//! Concessão de automações suas (REGRAS.md: automação sua vale pelo arquivo concedido): rotinas e
//! regras de Downloads.
//!
//! Uma automação é um arquivo que você escreveu. Ela só roda depois de `conceder`, que guarda o
//! SHA-256 do arquivo inteiro: qualquer edição derruba a concessão até você conceder de novo. A
//! concessão mora no banco (`meta`), fora do arquivo, para o arquivo não poder se conceder sozinho.

use orion_store::Store;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;

pub const PREFIXO: &str = "automacao:";

pub fn hash_do_arquivo(arquivo: &Path) -> std::io::Result<String> {
    Ok(hex::encode(Sha256::digest(std::fs::read(arquivo)?)))
}

/// YAML → JSON, para validar com `serde`. Chaves que não são texto viram texto.
pub fn yaml_para_json(texto: &str) -> Result<serde_json::Value, String> {
    let y: serde_yaml::Value = serde_yaml::from_str(texto).map_err(|e| e.to_string().lines().next().unwrap_or("YAML inválido").to_string())?;
    serde_json::to_value(y).map_err(|e| e.to_string())
}

#[derive(Clone)]
pub struct Concessoes {
    m: Arc<Store>,
}

impl Concessoes {
    pub fn new(m: Arc<Store>) -> Self {
        Concessoes { m }
    }

    fn chave(tipo: &str, nome: &str) -> String {
        format!("{PREFIXO}{tipo}:{nome}")
    }

    pub fn conceder(&self, tipo: &str, nome: &str, hash: &str) -> anyhow::Result<()> {
        Ok(self.m.meta_set(&Self::chave(tipo, nome), Some(hash))?)
    }

    pub fn revogar(&self, tipo: &str, nome: &str) -> anyhow::Result<bool> {
        let k = Self::chave(tipo, nome);
        let existia = self.m.meta_get(&k).is_some();
        self.m.meta_set(&k, None)?;
        Ok(existia)
    }

    /// `concedida`, `mudou` (já foi concedida, o arquivo mudou) ou `sem_concessao`.
    pub fn estado(&self, tipo: &str, nome: &str, hash_atual: &str) -> &'static str {
        match self.m.meta_get(&Self::chave(tipo, nome)) {
            None => "sem_concessao",
            Some(g) if g == hash_atual => "concedida",
            Some(_) => "mudou",
        }
    }
}

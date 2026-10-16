//! Provedores de modelo ligados direto no Orion (sem gateway externo).
//!
//! Cada provedor do catálogo fala a API compatível com a da OpenAI. Quem você liga e em que ordem
//! é `ORION_PROVEDORES` (lista JSON; a ordem é a prioridade); a chave vem do cofre do SO ou do
//! ambiente, em `ORION_KEY_<ID>`. Modelos e limites ficam na configuração, não no código.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

pub struct Catalogado {
    pub nome: &'static str,
    pub base_url: &'static str,
}

/// Endereços conferidos na documentação de cada provedor.
pub const CATALOGO: &[(&str, Catalogado)] = &[
    ("gemini", Catalogado { nome: "Google AI Studio (Gemini)", base_url: "https://generativelanguage.googleapis.com/v1beta/openai" }),
    ("groq", Catalogado { nome: "Groq", base_url: "https://api.groq.com/openai/v1" }),
    ("cerebras", Catalogado { nome: "Cerebras", base_url: "https://api.cerebras.ai/v1" }),
    ("openrouter", Catalogado { nome: "OpenRouter", base_url: "https://openrouter.ai/api/v1" }),
    ("mistral", Catalogado { nome: "Mistral", base_url: "https://api.mistral.ai/v1" }),
    ("github", Catalogado { nome: "GitHub Models", base_url: "https://models.github.ai/inference" }),
    ("nvidia", Catalogado { nome: "NVIDIA NIM", base_url: "https://integrate.api.nvidia.com/v1" }),
    ("zai", Catalogado { nome: "Z.ai (GLM)", base_url: "https://api.z.ai/api/paas/v4" }),
];

pub fn catalogado(id: &str) -> Option<&'static Catalogado> {
    CATALOGO.iter().find(|(k, _)| *k == id).map(|(_, c)| c)
}

pub const CAMADAS_CONF: [&str; 4] = ["padrao", "rapido", "pesado", "visao"];
static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,23}$").unwrap());

/// Um item de `ORION_PROVEDORES`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ProvedorConf {
    pub id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub padrao: String,
    #[serde(default)]
    pub rapido: String,
    #[serde(default)]
    pub pesado: String,
    #[serde(default)]
    pub visao: String,
    /// Chamadas por dia que o Orion se permite; 0 = sem teto.
    #[serde(default)]
    pub limite_dia: u64,
}

impl ProvedorConf {
    /// Normaliza e confere o item (equivale aos validadores do pydantic).
    pub fn validar(mut self) -> Result<Self, String> {
        self.id = self.id.trim().to_lowercase();
        if !ID.is_match(&self.id) {
            return Err("id: letras minúsculas, números, '-' ou '_' (até 24)".into());
        }
        if self.url.is_empty() && catalogado(&self.id).is_none() {
            return Err(format!("provedor '{}' fora do catálogo: informe 'url'", self.id));
        }
        if !self.url.is_empty()
            && !["https://", "http://127.0.0.1", "http://localhost"].iter().any(|p| self.url.starts_with(p))
        {
            return Err(format!("provedor '{}': url precisa ser https (ou local)", self.id));
        }
        if self.modelos().is_empty() {
            return Err(format!("provedor '{}': configure ao menos um modelo", self.id));
        }
        Ok(self)
    }

    pub fn base_url(&self) -> String {
        if !self.url.is_empty() {
            return self.url.clone();
        }
        catalogado(&self.id).map(|c| c.base_url.to_string()).unwrap_or_default()
    }

    /// (camada, modelo) configurados; camada "" é o padrão.
    pub fn modelos(&self) -> Vec<(String, String)> {
        [("", &self.padrao), ("rapido", &self.rapido), ("pesado", &self.pesado), ("visao", &self.visao)]
            .into_iter()
            .filter(|(_, m)| !m.is_empty())
            .map(|(c, m)| (c.to_string(), m.clone()))
            .collect()
    }
}

/// Lê `ORION_PROVEDORES` (lista JSON).
pub fn parse_provedores(json: &str) -> Result<Vec<ProvedorConf>, String> {
    if json.trim().is_empty() {
        return Ok(vec![]);
    }
    let lista: Vec<ProvedorConf> = serde_json::from_str(json).map_err(|e| format!("ORION_PROVEDORES: {e}"))?;
    let mut vistos = std::collections::HashSet::new();
    let mut out = vec![];
    for p in lista {
        let p = p.validar()?;
        if !vistos.insert(p.id.clone()) {
            return Err(format!("provedor repetido: {}", p.id));
        }
        out.push(p);
    }
    Ok(out)
}

pub fn chave_env(provedor_id: &str) -> String {
    format!("ORION_KEY_{}", provedor_id.to_uppercase().replace('-', "_"))
}

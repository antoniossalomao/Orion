//! Fontes escolhidas na interface são filtros reais, inclusive na busca vetorial.
//! REGRAS.md: conteúdo externo é dado, não instrução; o que é de um projeto fica no projeto.
use crate::{Result, Store};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const KEY: &str = "sources:preferences";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SourcePreferences {
    pub vault: bool,
    pub repo: bool,
    pub calendar: bool,
    pub email: bool,
    pub web: bool,
    pub facts: bool,
    pub conversations: bool,
}
impl Default for SourcePreferences {
    fn default() -> Self {
        Self {
            vault: true,
            repo: true,
            calendar: true,
            email: true,
            web: true,
            facts: true,
            conversations: true,
        }
    }
}
impl SourcePreferences {
    pub fn tool_allowed(&self, name: &str) -> bool {
        let name = name.to_lowercase();
        if !self.facts && name == "listar_fatos" {
            return false;
        }
        if !self.web
            && [
                "pesquisar_internet",
                "pesquisar_com_ia",
                "consultar_clima",
                "buscar_url",
                "navegador_ler",
                "navegador_agir",
                "gerar_imagem",
            ]
            .contains(&name.as_str())
        {
            return false;
        }
        if !self.repo && ["consultar_git", "consultar_git_projeto"].contains(&name.as_str()) {
            return false;
        }
        if !self.calendar
            && [
                "calendar",
                "agenda",
                "calendario",
                "calendário",
                "disponibilidade",
                "list-events",
                "get-freebusy",
            ]
            .iter()
            .any(|part| name.contains(part))
        {
            return false;
        }
        if !self.email
            && ["gmail", "email", "e_mail", "outlook", "mail__"]
                .iter()
                .any(|part| name.contains(part))
        {
            return false;
        }
        true
    }

    pub fn allows(&self, kind: crate::search::Kind, source: &str, repos: &[String]) -> bool {
        use crate::search::Kind;
        match kind {
            Kind::Fact => self.facts,
            Kind::Message => self.conversations,
            Kind::Chunk => {
                let normalized = source.replace('\\', "/").to_lowercase();
                let source = if normalized.starts_with("project:") {
                    normalized
                        .split_once('/')
                        .map(|(_, s)| s)
                        .unwrap_or(&normalized)
                } else {
                    &normalized
                };
                if source.starts_with("http://")
                    || source.starts_with("https://")
                    || source.starts_with("web:")
                {
                    return self.web;
                }
                if source.starts_with("calendar:") || source.starts_with("agenda:") {
                    return self.calendar;
                }
                if source.starts_with("gmail:")
                    || source.starts_with("email:")
                    || source.starts_with("mail:")
                {
                    return self.email;
                }
                if source.starts_with("repo:")
                    || repos.iter().any(|root| {
                        let root = root.replace('\\', "/").trim_end_matches('/').to_lowercase();
                        !root.is_empty() && (source == root || source.starts_with(&(root + "/")))
                    })
                {
                    return self.repo;
                }
                self.vault
            }
        }
    }

    pub fn updated(&self, patch: Value) -> Result<Self> {
        let mut value =
            serde_json::to_value(self).map_err(|e| crate::StoreError::Other(e.to_string()))?;
        let Some(changes) = patch.as_object() else {
            return crate::invalid("fontes precisam ser um objeto");
        };
        for (key, item) in changes {
            if value.get(key).is_none() || !item.is_boolean() {
                return crate::invalid("fonte desconhecida ou valor inválido");
            }
            value[key] = item.clone();
        }
        serde_json::from_value(value).map_err(|e| crate::StoreError::Other(e.to_string()))
    }
}
impl Store {
    pub fn source_preferences(&self) -> Result<SourcePreferences> {
        let value: Option<String> = self
            .conn()
            .query_row(
                "SELECT value FROM meta WHERE key=?",
                [format!("valor:{KEY}")],
                |r| r.get(0),
            )
            .optional()?;
        match value {
            Some(value) => {
                serde_json::from_str(&value).map_err(|e| crate::StoreError::Other(e.to_string()))
            }
            None => Ok(Default::default()),
        }
    }
    pub fn save_source_preferences(&self, value: &SourcePreferences) -> Result<()> {
        let encoded =
            serde_json::to_string(value).map_err(|e| crate::StoreError::Other(e.to_string()))?;
        self.meta_set(KEY, Some(&encoded))
    }
}

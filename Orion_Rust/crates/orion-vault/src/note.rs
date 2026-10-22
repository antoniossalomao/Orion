use serde::{Deserialize, Serialize};

/// Frontmatter de uma nota do Vault Obsidian (F06.03 / F06.10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteFrontmatter {
    #[serde(rename = "type")]
    pub note_type: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hora: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonte: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projeto: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resumo: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agente: Option<String>,
}

impl Default for NoteFrontmatter {
    fn default() -> Self {
        Self {
            note_type: "recurso".to_string(),
            status: None,
            date: None,
            hora: None,
            fonte: None,
            projeto: None,
            tags: Vec::new(),
            resumo: None,
            agente: None,
        }
    }
}

/// Metadados extraídos de uma nota completa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteMetadata {
    pub title: String,
    pub rel_path: String,
    pub frontmatter: NoteFrontmatter,
    pub wikilinks: Vec<String>,
    pub tags: Vec<String>,
    pub is_private: bool,
    pub is_archive: bool,
    pub char_count: usize,
    pub last_modified_epoch: Option<i64>,
}

/// Representação estruturada de uma nota de conversa pronta para gravação em `03 Recursos/Orion/Memória/`.
#[derive(Debug, Clone)]
pub struct ConversationNote {
    pub date_str: String,
    pub time_str: String,
    pub project_name: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub dialogue_markdown: String,
    pub decisions: Vec<String>,
}

impl ConversationNote {
    /// Gera o nome do arquivo sanitizado: `AAAA-MM-DD - orion - <titulo>.md`
    pub fn file_name(&self) -> String {
        let clean_title = sanitize_filename(&self.title);
        // Reserva espaço para o marcador de conflito e mantém compatibilidade com NTFS.
        let mut end = clean_title.len().min(120);
        while !clean_title.is_char_boundary(end) {
            end -= 1;
        }
        let clean_title = clean_title[..end].trim_end_matches([' ', '.']);
        format!("{} - orion - {}.md", self.date_str, clean_title)
    }

    /// Gera o conteúdo final em Markdown com frontmatter canônico e diálogo formatado.
    pub fn to_markdown(&self) -> String {
        let fm = NoteFrontmatter {
            note_type: "conversa".to_string(),
            status: None,
            date: Some(self.date_str.clone()),
            hora: Some(self.time_str.clone()),
            fonte: Some("orion".to_string()),
            projeto: Some(format!("[[{}]]", self.project_name)),
            tags: self.tags.clone(),
            resumo: Some(self.summary.clone()),
            agente: Some("orion".to_string()),
        };

        let yaml = serde_yaml::to_string(&fm).unwrap_or_else(|_| "type: conversa\n".to_string());

        let mut out = format!("---\n{}---\n\n", yaml);
        out.push_str(&format!("# {} — {}\n\n", self.date_str, self.title));
        out.push_str("## Resumo Executivo\n\n");
        out.push_str(&self.summary);
        out.push_str("\n\n");

        if !self.decisions.is_empty() {
            out.push_str("## Decisões Registradas\n\n");
            for d in &self.decisions {
                out.push_str(&format!("- {}\n", d));
            }
            out.push_str("\n");
        }

        out.push_str("## Diálogo\n\n");
        out.push_str(&self.dialogue_markdown);
        out.push('\n');

        out
    }
}

/// Remove caracteres inválidos no Windows e no Vault (`: < > " | ? * \ /`).
pub fn sanitize_filename(name: &str) -> String {
    let forbidden = [':', '<', '>', '"', '|', '?', '*', '/', '\\'];
    let mut clean = String::with_capacity(name.len());
    for c in name.chars() {
        if forbidden.contains(&c) {
            clean.push('-');
        } else {
            clean.push(c);
        }
    }
    // Remove espaços extras ou traços consecutivos
    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_note_formatting() {
        let note = ConversationNote {
            date_str: "2026-10-17".to_string(),
            time_str: "03:40".to_string(),
            project_name: "Orion".to_string(),
            title: "Implementação da F06: Vault Nativo".to_string(),
            summary: "Implementado o crate orion-vault em Rust puro com git e parser.".to_string(),
            tags: vec!["orion".to_string(), "rust".to_string(), "vault".to_string()],
            dialogue_markdown: "### 👤 Antonio\n\nFaz a F06.\n\n### 🤖 Orion\n\nFeito.".to_string(),
            decisions: vec!["Crate orion-vault criado".to_string()],
        };

        let md = note.to_markdown();
        println!("GERADO:\n{}", md);
        assert!(md.starts_with("---\n"));
        assert!(md.contains("fonte: orion"));
        assert!(
            md.contains("projeto: '[[Orion]]'")
                || md.contains("projeto: \"[[Orion]]\"")
                || md.contains("projeto: '[[Orion]]'")
        );
        assert!(md.contains("## Resumo Executivo"));
        assert!(md.contains("## Decisões Registradas"));
        assert_eq!(
            note.file_name(),
            "2026-10-17 - orion - Implementação da F06- Vault Nativo.md"
        );
    }
}

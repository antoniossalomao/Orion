use crate::note::{NoteFrontmatter, NoteMetadata};
use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

static WIKILINK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]").unwrap()
});

static TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\s)#([a-zA-Z0-9_\-]+)").unwrap()
});

/// Representação de uma nota Markdown lida e parseada do Vault.
#[derive(Debug, Clone)]
pub struct ParsedNote {
    pub metadata: NoteMetadata,
    pub raw_content: String,
    pub body_content: String,
}

pub struct VaultParser;

impl VaultParser {
    /// Faz o parse completo de um arquivo Markdown do Vault.
    pub fn parse(rel_path: &str, content: &str) -> ParsedNote {
        let (frontmatter, body) = Self::extract_frontmatter(content);

        // Extrai título: primeiro tenta h1 (# Titulo), senão nome do arquivo sem extensão
        let title = Self::extract_title(&body).unwrap_or_else(|| {
            std::path::Path::new(rel_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(rel_path)
                .to_string()
        });

        // Extrai wikilinks
        let mut links_set = HashSet::new();
        for cap in WIKILINK_RE.captures_iter(&body) {
            if let Some(target) = cap.get(1) {
                let clean = target.as_str().trim();
                if !clean.is_empty() {
                    links_set.insert(clean.to_string());
                }
            }
        }
        let mut wikilinks: Vec<_> = links_set.into_iter().collect();
        wikilinks.sort();

        // Extrai tags do body e junta com as do frontmatter
        let mut tags_set = HashSet::new();
        for t in &frontmatter.tags {
            tags_set.insert(t.trim_start_matches('#').to_string());
        }
        for cap in TAG_RE.captures_iter(&body) {
            if let Some(tag_match) = cap.get(1) {
                tags_set.insert(tag_match.as_str().to_string());
            }
        }
        let mut tags: Vec<_> = tags_set.into_iter().collect();
        tags.sort();

        // Verifica se a nota é privada
        let is_private = tags.iter().any(|t| t == "privado" || t == "private")
            || rel_path.starts_with(".private")
            || rel_path.contains("Pessoal");

        // Verifica se a nota está arquivada (04 Arquivo)
        let is_archive = rel_path.starts_with("04 Arquivo");

        let metadata = NoteMetadata {
            title,
            rel_path: rel_path.replace('\\', "/"),
            frontmatter,
            wikilinks,
            tags,
            is_private,
            is_archive,
            char_count: body.chars().count(),
            last_modified_epoch: None,
        };

        ParsedNote {
            metadata,
            raw_content: content.to_string(),
            body_content: body,
        }
    }

    /// Separa o frontmatter YAML do corpo Markdown.
    fn extract_frontmatter(content: &str) -> (NoteFrontmatter, String) {
        let trimmed = content.trim_start();
        if trimmed.starts_with("---") {
            let after_first = &trimmed[3..];
            if let Some(end_idx) = after_first.find("\n---") {
                let yaml_str = &after_first[..end_idx].trim();
                let body = after_first[end_idx + 4..].trim_start_matches(|c| c == '\r' || c == '\n').to_string();

                if let Ok(fm) = serde_yaml::from_str::<NoteFrontmatter>(yaml_str) {
                    return (fm, body);
                } else {
                    return (NoteFrontmatter::default(), body);
                }
            }
        }
        (NoteFrontmatter::default(), content.to_string())
    }

    /// Extrai o primeiro título h1 (`# Titulo`) se existir.
    fn extract_title(body: &str) -> Option<String> {
        for line in body.lines() {
            let line_trim = line.trim();
            if line_trim.starts_with("# ") {
                let title = line_trim[2..].trim();
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vault_note() {
        let raw = r#"---
type: conversa
date: 2026-10-17
hora: 03:45
projeto: "[[Orion]]"
tags: [orion, memoria, privado]
resumo: "Teste de sincronização nativa."
---

# Teste de Nota do Orion

Esta nota linka para [[Orion - Regras]] e também [[Orion - Arquitetura]].
Adicionalmente tem uma tag #desenvolvimento e outra #rust.
"#;

        let parsed = VaultParser::parse("03 Recursos/Orion/Memória/teste.md", raw);
        assert_eq!(parsed.metadata.title, "Teste de Nota do Orion");
        assert_eq!(parsed.metadata.frontmatter.note_type, "conversa");
        assert!(parsed.metadata.is_private);
        assert!(!parsed.metadata.is_archive);
        assert!(parsed.metadata.wikilinks.contains(&"Orion - Regras".to_string()));
        assert!(parsed.metadata.wikilinks.contains(&"Orion - Arquitetura".to_string()));
        assert!(parsed.metadata.tags.contains(&"desenvolvimento".to_string()));
        assert!(parsed.metadata.tags.contains(&"privado".to_string()));
    }
}

//! Validação antes de qualquer gravação/commit. REGRAS.md: conteúdo externo é dado, não instrução;
//! segredos nunca em log, audit ou processo filho; ler segredo ou chave pede confirmação.
use crate::{safe_path, ConversationNote, NoteFrontmatter, VaultError, VaultParser};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

fn invalid(message: &str) -> VaultError {
    VaultError::InvalidFormat(message.into())
}

pub(crate) fn note(
    root: &Path,
    note: &ConversationNote,
    markdown: &str,
    target: &Path,
) -> Result<(), VaultError> {
    let date = chrono::NaiveDate::parse_from_str(&note.date_str, "%Y-%m-%d")
        .map_err(|_| invalid("data inválida; use AAAA-MM-DD"))?;
    if date.format("%Y-%m-%d").to_string() != note.date_str
        || note.time_str.len() != 5
        || chrono::NaiveTime::parse_from_str(&note.time_str, "%H:%M").is_err()
    {
        return Err(invalid("data ou hora inválida; use AAAA-MM-DD e HH:MM"));
    }
    for (value, max) in [
        (&note.title, 120),
        (&note.project_name, 100),
        (&note.summary, 2000),
    ] {
        if value.trim().is_empty()
            || value.chars().count() > max
            || value.chars().any(char::is_control)
        {
            return Err(invalid("título, projeto e resumo devem estar preenchidos, sem controles ou quebras de linha"));
        }
    }
    if note
        .project_name
        .contains(['/', '\\', '[', ']', '#', '|', ':', '<', '>', '"', '?', '*'])
        || [".", ".."].contains(&note.project_name.as_str())
    {
        return Err(invalid("nome do projeto inválido"));
    }
    if note.tags.is_empty()
        || note.tags.len() > 32
        || note.tags.iter().any(|t| {
            t.is_empty()
                || t.len() > 80
                || !t.chars().all(|c| c.is_alphanumeric() || "_-/".contains(c))
        })
    {
        return Err(invalid(
            "informe tags válidas, sem espaços ou delimitadores YAML",
        ));
    }
    if markdown.len() as u64 > safe_path::NOTE_LIMIT {
        return Err(invalid("nota maior que 2 MiB"));
    }
    let yaml = markdown
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---\n"))
        .ok_or_else(|| invalid("frontmatter ausente ou incompleto"))?
        .0;
    let fm: NoteFrontmatter = serde_yaml::from_str(yaml)?;
    if fm.note_type != "conversa"
        || fm.fonte.as_deref() != Some("orion")
        || fm.date.as_deref() != Some(&note.date_str)
        || fm.hora.as_deref() != Some(&note.time_str)
        || fm.projeto.as_deref() != Some(&format!("[[{}]]", note.project_name))
        || fm.resumo.as_deref() != Some(&note.summary)
        || fm.tags != note.tags
    {
        return Err(invalid(
            "frontmatter de conversa não corresponde aos dados validados",
        ));
    }
    let project = ["01 Projetos", "02 Areas"].into_iter().any(|folder| {
        let rel = Path::new(folder)
            .join(&note.project_name)
            .join(format!("{}.md", note.project_name));
        safe_path::checked(root, &rel, false).is_ok_and(|p| p.is_file())
    });
    if !project {
        return Err(invalid(
            "vincule a nota a um Projeto ou Área existente no vault",
        ));
    }
    let links = VaultParser::parse("nota.md", markdown).metadata.wikilinks;
    if links.is_empty() {
        return Ok(());
    }
    let mut available = HashMap::<String, Vec<String>>::new();
    collect_notes(root, root, &mut available)?;
    let self_path = target.to_string_lossy().replace('\\', "/");
    let self_stem = target.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    for link in links {
        let link = link.strip_suffix(".md").unwrap_or(&link);
        let p = PathBuf::from(link);
        safe_path::relative(&p)?;
        let found = if link.contains('/') {
            let rel = PathBuf::from(format!("{link}.md"));
            safe_path::checked(root, &rel, false)?.is_file()
                || rel.to_string_lossy().replace('\\', "/") == self_path
        } else {
            let count = available.get(link).map_or(0, Vec::len);
            count == 1 || (count == 0 && link == self_stem)
        };
        if !found {
            return Err(invalid(
                "a nota tem wikilink ausente ou ambíguo; confira os destinos antes de salvar",
            ));
        }
    }
    Ok(())
}

fn collect_notes(
    root: &Path,
    dir: &Path,
    available: &mut HashMap<String, Vec<String>>,
) -> Result<(), VaultError> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|_| invalid("caminho fora do vault"))?;
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        let Ok(path) = safe_path::checked(root, rel, false) else {
            continue;
        };
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            collect_notes(root, &path, available)?;
        } else if meta.is_file() && path.extension().is_some_and(|s| s == "md") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                available
                    .entry(stem.to_string())
                    .or_default()
                    .push(rel.to_string_lossy().into());
            }
        }
    }
    Ok(())
}

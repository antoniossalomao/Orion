use crate::detector::{detect_vault_path, VaultConfig};
use crate::git::GitSync;
use crate::note::ConversationNote;
use crate::project::ProjectNote;
use crate::parser::{ParsedNote, VaultParser};
use crate::scanner::SecretScanner;
use crate::VaultError;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Estatísticas de notas do Vault.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_notes: usize,
    pub private_notes: usize,
    pub archive_notes: usize,
    pub total_links: usize,
    pub total_tags: usize,
    pub last_commit: Option<String>,
}

/// Resultado de gravação de nota de conversa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteConflict {
    pub original_path: String,
    pub proposed_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub file_path: String,
    pub commit_hash: Option<String>,
    pub pushed: bool,
    pub warning: Option<String>,
    pub conflict: Option<NoteConflict>,
    pub unchanged: bool,
    /// SHA-256 do texto que o Orion pretendia gravar; o diário o guarda para saber se a nota
    /// continua intacta antes de atualizá-la (REGRAS.md: você é o administrador).
    #[serde(default)]
    pub content_sha256: String,
}

/// Sugestão de link para uma nota nova (projeto, área ou hub de conceito).
#[derive(Debug, Clone, serde::Serialize)]
pub struct LinkSuggestion {
    pub link: String,
    pub kind: String,
    pub rel_path: String,
    pub reason: String,
}

/// Gerenciador central de sincronização e memória do Vault (F06).
pub struct VaultManager {
    config: VaultConfig,
    git: Option<GitSync>,
    notes_cache: Arc<RwLock<HashMap<String, ParsedNote>>>,
}

static FULL_SYNC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Liga a sincronização completa antes de cada nota gravada (o servidor liga conforme a configuração;
/// biblioteca e testes ficam com ela desligada).
pub fn set_full_sync(on: bool) {
    FULL_SYNC.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn full_sync_enabled() -> bool {
    FULL_SYNC.load(std::sync::atomic::Ordering::Relaxed)
}

impl VaultManager {
    /// Sincronização completa com o GitHub (commit do que mudou, junção e envio). `None` se o vault
    /// não é um repositório Git.
    pub fn sync_all(&self) -> Result<Option<crate::FullSyncReport>, VaultError> {
        match &self.git {
            Some(git) => Ok(Some(git.sync_all()?)),
            None => Ok(None),
        }
    }

    /// Inicializa o VaultManager com auto-detecção ou caminho configurado.
    pub fn new(custom_path: Option<PathBuf>) -> Result<Self, VaultError> {
        let path = custom_path.or_else(detect_vault_path).ok_or_else(|| {
            VaultError::NotFound("Nenhum caminho de Vault encontrado".to_string())
        })?;

        if !path.is_dir() {
            return Err(VaultError::NotFound(path.display().to_string()));
        }

        let path = path.canonicalize()?;
        let git = match GitSync::new(&path) {
            Ok(git) => Some(git),
            Err(crate::GitError::NotAGitRepo) => None,
            Err(e) => return Err(e.into()),
        };
        let config = VaultConfig {
            path,
            remote_url: None,
            branch: "main".to_string(),
            auto_pull: true,
            auto_push: true,
        };

        Ok(Self {
            config,
            git,
            notes_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Só grava o arquivo, sem commit nem push: o diário não polui o histórico do seu vault.
    pub fn without_git(mut self) -> Self {
        self.git = None;
        self
    }

    pub fn config(&self) -> &VaultConfig {
        &self.config
    }

    /// Pull só por fast-forward, sem rebase, stash ou descarte de edições locais.
    pub fn pull(&self) -> Result<Option<String>, VaultError> {
        match &self.git {
            Some(git) if self.config.auto_pull => Ok(Some(git.pull(&self.config.branch)?)),
            _ => Ok(None),
        }
    }

    /// Nunca substitui um arquivo existente com conteúdo diferente (REGRAS.md: você é o administrador).
    /// Diferenças viram uma proposta separada, marcada como conflito, sem commit/push automático.
    pub fn save_conversation(&self, note: &ConversationNote) -> Result<SyncResult, VaultError> {
        self.save_note(note, None)
    }

    /// Diário de sessão: igual a `save_conversation`, mas atualiza a nota no lugar quando o arquivo
    /// ainda é exatamente a versão que o Orion gravou antes (`previous_sha256`). Se você editou a
    /// nota, ela é preservada e a versão nova vira proposta de conflito.
    pub fn save_diary(&self, note: &ConversationNote, previous_sha256: Option<&str>) -> Result<SyncResult, VaultError> {
        self.save_note(note, previous_sha256)
    }

    fn save_note(&self, note: &ConversationNote, replace_if: Option<&str>) -> Result<SyncResult, VaultError> {
        // Antes de gravar, o vault local alcança o GitHub; se falhar, a nota é gravada do mesmo jeito
        // e o aviso de push pendente cobre o resto.
        if full_sync_enabled() {
            if let Some(git) = &self.git {
                if let Err(e) = git.sync_all() {
                    tracing::warn!("vault: sincronização antes da nota falhou: {e}");
                }
            }
        }
        let md = note.to_markdown();
        let md_sha = format!("{:x}", Sha256::digest(md.as_bytes()));
        SecretScanner::scan(&md).map_err(|e| VaultError::SecretDetected(e.to_string()))?;
        let rel = Path::new("03 Recursos")
            .join("Orion")
            .join("Memória")
            .join(note.file_name());
        crate::validate::note(&self.config.path, note, &md, &rel)?;
        let target = crate::safe_path::checked(&self.config.path, &rel, true)?;
        let unchanged = match publish_new(&target, &md) {
            Ok(()) => false,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = crate::safe_path::read_note(&crate::safe_path::checked(
                    &self.config.path,
                    &rel,
                    false,
                )?)?;
                let own_previous = replace_if.is_some_and(|prev| prev == format!("{:x}", Sha256::digest(existing.as_bytes())));
                if existing != md && own_previous {
                    replace_note(&target, &md)?;
                    false
                } else if existing != md {
                    let proposal = format!("{md}\n> Conflito de edição. A versão anterior foi preservada. Revise as duas versões antes de publicar.\n");
                    let digest = format!("{:x}", Sha256::digest(proposal.as_bytes()));
                    let stem = rel
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .ok_or_else(|| VaultError::InvalidFormat("nome de nota inválido".into()))?;
                    let proposed_rel =
                        rel.with_file_name(format!("{stem} - conflito Orion {}.md", &digest[..20]));
                    let proposed =
                        crate::safe_path::checked(&self.config.path, &proposed_rel, false)?;
                    match publish_new(&proposed, &proposal) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                            if crate::safe_path::read_note(&proposed)? != proposal {
                                return Err(VaultError::InvalidFormat("a proposta de conflito também foi editada; nenhum arquivo foi substituído".into()));
                            }
                        }
                        Err(e) => return Err(e.into()),
                    }
                    let original_path = rel.to_string_lossy().replace('\\', "/");
                    let proposed_path = proposed_rel.to_string_lossy().replace('\\', "/");
                    return Ok(SyncResult {
                        file_path: original_path.clone(), commit_hash: None, pushed: false, unchanged: false, content_sha256: md_sha,
                        warning: Some("Conflito de edição: as duas versões foram preservadas. Revise a original e a proposta; nenhuma foi enviada ao Git.".into()),
                        conflict: Some(NoteConflict { original_path, proposed_path }),
                    });
                } else {
                    true
                }
            }
            Err(e) => return Err(e.into()),
        };
        let path = rel.to_string_lossy().replace('\\', "/");
        let parsed = VaultParser::parse(&path, &md);
        self.notes_cache.write().insert(path.clone(), parsed);
        let mut result = SyncResult {
            file_path: path,
            commit_hash: None,
            pushed: false,
            warning: None,
            conflict: None,
            unchanged,
            content_sha256: md_sha,
        };
        self.commit_and_push(&rel, &md, &note.summary, &mut result);
        Ok(result)
    }

    /// Commit isolado da nota (índice do usuário preservado) e push só se o pai já estiver no remoto.
    fn commit_and_push(&self, rel: &Path, md: &str, message: &str, result: &mut SyncResult) {
        let Some(git) = &self.git else { return };
        match git.commit_note(rel, md, message) {
            Ok(commit) => {
                result.commit_hash = Some(commit.hash.clone());
                if commit.created && self.config.auto_push {
                    match git.push_note(&commit) {
                        Ok(_) => result.pushed = true,
                        Err(e) => result.warning = Some(format!("Nota commitada localmente; push pendente: {e}")),
                    }
                }
            }
            Err(e) => result.warning = Some(format!("Nota preservada localmente; commit pendente: {e}")),
        }
    }

    /// F08.06: cria `01 Projetos/<Nome>/<Nome>.md` ao criar o projeto. Nunca sobrescreve: se a nota
    /// já existe (mesmo editada por você), ela fica como está e o resultado vem com `unchanged`.
    pub fn create_project_note(&self, note: &ProjectNote) -> Result<SyncResult, VaultError> {
        let md = note.to_markdown()?;
        SecretScanner::scan(&md).map_err(|e| VaultError::SecretDetected(e.to_string()))?;
        let rel = note.rel_path();
        let target = crate::safe_path::checked(&self.config.path, &rel, true)?;
        let unchanged = match publish_new(&target, &md) {
            Ok(()) => false,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => true,
            Err(e) => return Err(e.into()),
        };
        let path = rel.to_string_lossy().replace('\\', "/");
        self.notes_cache.write().insert(path.clone(), VaultParser::parse(&path, &md));
        let mut result = SyncResult {
            file_path: path,
            commit_hash: None,
            pushed: false,
            warning: None,
            conflict: None,
            unchanged,
            content_sha256: format!("{:x}", Sha256::digest(md.as_bytes())),
        };
        if !unchanged {
            self.commit_and_push(&rel, &md, &format!("Orion: nota do projeto {}", note.name.trim()), &mut result);
        }
        Ok(result)
    }

    /// F09.05: exporta o "Você fez" do dia para `03 Recursos/Orion/Memória/Diário/<data> - Você fez[ - <projeto>].md`.
    /// Nunca sobrescreve (se já existe, devolve `unchanged`) e não faz commit nem push.
    /// F06.15: grava (ou substitui) um relatório em `03 Recursos/Relatórios de Mudanças`. Devolve o caminho.
    pub fn publish_report(&self, name: &str, markdown: &str) -> Result<String, VaultError> {
        let rel = Path::new("03 Recursos").join("Relatórios de Mudanças").join(format!("{}.md", crate::note::sanitize_filename(name)));
        let target = crate::safe_path::checked(&self.config.path, &rel, true)?;
        replace_note(&target, markdown)?;
        Ok(rel.to_string_lossy().replace('\\', "/"))
    }

    /// F06.15: acrescenta uma linha ao relatório da sessão da conversa; cria com `header` se não existir.
    pub fn append_report(&self, name: &str, header: &str, line: &str) -> Result<String, VaultError> {
        let rel = Path::new("03 Recursos").join("Relatórios de Mudanças").join(format!("{}.md", crate::note::sanitize_filename(name)));
        let target = crate::safe_path::checked(&self.config.path, &rel, true)?;
        let current = std::fs::read_to_string(&target).unwrap_or_else(|_| header.to_string());
        replace_note(&target, &format!("{current}{line}\n"))?;
        Ok(rel.to_string_lossy().replace('\\', "/"))
    }

    pub fn save_daily_report(&self, date_str: &str, hora: &str, scope: Option<&str>, markdown: &str) -> Result<SyncResult, VaultError> {
        chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|_| VaultError::InvalidFormat("data inválida; use AAAA-MM-DD".into()))?;
        if chrono::NaiveTime::parse_from_str(hora, "%H:%M").is_err() {
            return Err(VaultError::InvalidFormat("hora inválida; use HH:MM".into()));
        }
        let md = format!(
            "---\ntype: conversa\ndate: {date_str}\nhora: \"{hora}\"\nfonte: orion\nprojeto: \"[[Orion]]\"\ntags: [orion, diario]\nresumo: Relatório diário Você fez\n---\n\n{markdown}"
        );
        SecretScanner::scan(&md).map_err(|e| VaultError::SecretDetected(e.to_string()))?;
        // Um arquivo por dia e por escopo: o relatório de um projeto não pode ser confundido com o pessoal.
        let name = match scope.map(crate::note::sanitize_filename).filter(|n| !n.is_empty()) {
            Some(n) => format!("{date_str} - Você fez - {n}.md"),
            None => format!("{date_str} - Você fez.md"),
        };
        let rel = Path::new("03 Recursos").join("Orion").join("Memória").join("Diário").join(name);
        let target = crate::safe_path::checked(&self.config.path, &rel, true)?;
        let unchanged = match publish_new(&target, &md) {
            Ok(()) => false,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => true,
            Err(e) => return Err(e.into()),
        };
        Ok(SyncResult {
            file_path: rel.to_string_lossy().replace('\\', "/"),
            commit_hash: None,
            pushed: false,
            warning: None,
            conflict: None,
            unchanged,
            content_sha256: format!("{:x}", Sha256::digest(md.as_bytes())),
        })
    }

    /// Resumo semanal: uma nota por semana ISO em `Memória/Diário`, nunca sobrescreve uma existente.
    pub fn save_weekly_report(&self, monday: &str, hora: &str, markdown: &str) -> Result<SyncResult, VaultError> {
        use chrono::Datelike;
        let day = chrono::NaiveDate::parse_from_str(monday, "%Y-%m-%d").map_err(|_| VaultError::InvalidFormat("data inválida; use AAAA-MM-DD".into()))?;
        if chrono::NaiveTime::parse_from_str(hora, "%H:%M").is_err() {
            return Err(VaultError::InvalidFormat("hora inválida; use HH:MM".into()));
        }
        let iso = day.iso_week();
        let md = format!(
            "---\ntype: conversa\ndate: {monday}\nhora: \"{hora}\"\nfonte: orion\nprojeto: \"[[Orion]]\"\ntags: [orion, diario, semanal]\nresumo: Resumo semanal {}-W{:02}\n---\n\n{markdown}",
            iso.year(), iso.week()
        );
        SecretScanner::scan(&md).map_err(|e| VaultError::SecretDetected(e.to_string()))?;
        let name = format!("{}-W{:02} - Resumo semanal.md", iso.year(), iso.week());
        let rel = Path::new("03 Recursos").join("Orion").join("Memória").join("Diário").join(name);
        let target = crate::safe_path::checked(&self.config.path, &rel, true)?;
        let unchanged = match publish_new(&target, &md) {
            Ok(()) => false,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => true,
            Err(e) => return Err(e.into()),
        };
        Ok(SyncResult {
            file_path: rel.to_string_lossy().replace('\\', "/"),
            commit_hash: None,
            pushed: false,
            warning: None,
            conflict: None,
            unchanged,
            content_sha256: format!("{:x}", Sha256::digest(md.as_bytes())),
        })
    }

    /// Faz uma indexação rápida de todas as notas `.md` do Vault.
    pub fn index_all(&self) -> Result<VaultStats, VaultError> {
        let mut cache = self.notes_cache.write();
        cache.clear();

        let mut total_notes = 0;
        let mut private_notes = 0;
        let mut archive_notes = 0;
        let mut total_links = 0;
        let mut all_tags = std::collections::HashSet::new();

        self.scan_dir(&self.config.path, &mut |rel_path, content| {
            let parsed = VaultParser::parse(&rel_path, &content);
            if parsed.metadata.is_private {
                private_notes += 1;
            }
            if parsed.metadata.is_archive {
                archive_notes += 1;
            }
            total_links += parsed.metadata.wikilinks.len();
            for t in &parsed.metadata.tags {
                all_tags.insert(t.clone());
            }
            total_notes += 1;
            cache.insert(rel_path, parsed);
        })?;

        let last_commit = self.git.as_ref().and_then(|g| g.head_commit_hash().ok());

        Ok(VaultStats {
            total_notes,
            private_notes,
            archive_notes,
            total_links,
            total_tags: all_tags.len(),
            last_commit,
        })
    }

    /// Busca textual rápida no cache de notas parseadas.
    pub fn search(&self, query: &str) -> Vec<ParsedNote> {
        let q = query.to_lowercase();
        let cache = self.notes_cache.read();

        let mut matches = Vec::new();
        for note in cache.values() {
            if note.metadata.title.to_lowercase().contains(&q)
                || note
                    .metadata
                    .tags
                    .iter()
                    .any(|t| t.to_lowercase().contains(&q))
                || note
                    .metadata
                    .wikilinks
                    .iter()
                    .any(|l| l.to_lowercase().contains(&q))
                || note.body_content.to_lowercase().contains(&q)
            {
                matches.push(note.clone());
            }
        }

        // Ordena: primeiro não-arquivados, depois por tamanho
        matches.sort_by(|a, b| {
            a.metadata
                .is_archive
                .cmp(&b.metadata.is_archive)
                .then(b.metadata.char_count.cmp(&a.metadata.char_count))
        });

        matches
    }

    /// Sugere onde ligar uma nota nova (regra "não criar ilhas" do vault): projetos, áreas e hubs de
    /// conceito cujo nome, tags ou título aparecem no texto. Só leitura do índice; não grava nada.
    pub fn suggest_links(&self, title: &str, text: &str, tags: &[String], limit: usize) -> Vec<LinkSuggestion> {
        fn tokens(s: &str) -> std::collections::HashSet<String> {
            s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|t| t.chars().count() >= 3).map(String::from).collect()
        }
        let mut entrada = tokens(title);
        entrada.extend(tokens(text));
        entrada.extend(tags.iter().flat_map(|t| tokens(t)));
        let texto_baixo = format!("{title} {text}").to_lowercase();
        let cache = self.notes_cache.read();
        let mut achados: Vec<(u32, LinkSuggestion)> = vec![];
        for note in cache.values() {
            let rel = note.metadata.rel_path.replace('\\', "/");
            let tipo = if rel.starts_with("01 Projetos/") {
                "projeto"
            } else if rel.starts_with("02 Areas/") {
                "area"
            } else if rel.starts_with("03 Recursos/Conceitos/") {
                "hub"
            } else {
                continue;
            };
            let Some(stem) = Path::new(&rel).file_stem().and_then(|s| s.to_str()).map(String::from) else { continue };
            // Projeto/área: só a nota raiz (`<Nome>/<Nome>.md`), não as notas internas.
            if tipo != "hub" && !rel.ends_with(&format!("/{stem}/{stem}.md")) {
                continue;
            }
            if note.metadata.is_archive || rel.starts_with("04 Arquivo/") {
                continue;
            }
            let mut pontos = 0u32;
            let mut motivos: Vec<String> = vec![];
            if texto_baixo.contains(&stem.to_lowercase()) {
                pontos += 6;
                motivos.push(format!("o texto cita \"{stem}\""));
            }
            let nome = tokens(&stem);
            let em_comum = nome.intersection(&entrada).count() as u32;
            if em_comum > 0 {
                pontos += 3 * em_comum;
                motivos.push("palavras do nome aparecem na nota".into());
            }
            let tags_em_comum: Vec<&String> = note.metadata.tags.iter().filter(|t| entrada.contains(&t.to_lowercase())).collect();
            if !tags_em_comum.is_empty() {
                pontos += 2 * tags_em_comum.len() as u32;
                motivos.push(format!("tags em comum: {}", tags_em_comum.iter().take(3).map(|t| t.as_str()).collect::<Vec<_>>().join(", ")));
            }
            if pontos > 0 {
                achados.push((pontos, LinkSuggestion { link: format!("[[{stem}]]"), kind: tipo.into(), rel_path: rel, reason: motivos.join("; ") }));
            }
        }
        achados.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.link.cmp(&b.1.link)));
        achados.into_iter().take(limit.clamp(1, 20)).map(|a| a.1).collect()
    }

    /// Notas-ilha: sem link para fora e sem link vindo de outra nota. Ignora modelos, Home e arquivo morto.
    pub fn orphans(&self) -> Vec<String> {
        let cache = self.notes_cache.read();
        let mut recebidos: std::collections::HashSet<String> = std::collections::HashSet::new();
        for note in cache.values() {
            for l in note.metadata.wikilinks.iter().cloned().chain(frontmatter_links(&note.raw_content)) {
                recebidos.insert(l.split(['|', '#']).next().unwrap_or(&l).trim().to_lowercase());
            }
        }
        let mut ilhas: Vec<String> = cache
            .values()
            .filter(|n| {
                let rel = n.metadata.rel_path.replace('\\', "/");
                !(rel.starts_with("Templates/") || rel.starts_with("04 Arquivo/") || rel.starts_with("scripts/") || rel == "Home.md" || n.metadata.is_archive)
            })
            .filter(|n| {
                let stem = Path::new(&n.metadata.rel_path).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
                // Link com pasta ([[Pasta/Nota]]) conta pelo caminho relativo sem a extensão.
                let caminho = n.metadata.rel_path.replace('\\', "/").trim_end_matches(".md").to_lowercase();
                n.metadata.wikilinks.is_empty() && frontmatter_links(&n.raw_content).is_empty() && !recebidos.contains(&stem) && !recebidos.contains(&n.metadata.title.to_lowercase()) && !recebidos.contains(&caminho)
            })
            .map(|n| n.metadata.rel_path.replace('\\', "/"))
            .collect();
        ilhas.sort();
        ilhas.truncate(200);
        ilhas
    }

    fn scan_dir<F>(&self, root: &Path, callback: &mut F) -> Result<(), VaultError>
    where
        F: FnMut(String, String),
    {
        for entry in walk_md_files(root)? {
            if let Ok(rel) = entry.strip_prefix(root) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                // Pular diretórios internos do git ou lixeira
                if rel_str.starts_with(".git") || rel_str.starts_with(".obsidian") {
                    continue;
                }
                if let Ok(content) = crate::safe_path::read_note(&entry) {
                    callback(rel_str, content);
                }
            }
        }
        Ok(())
    }
}

// Publicação atômica sem substituir destino: até uma edição concorrente fica preservada.
fn publish_new(path: &Path, content: &str) -> Result<(), std::io::Error> {
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| std::io::Error::other("pasta inválida"))?,
    )?;
    file.write_all(content.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path).map_err(|e| e.error)?;
    Ok(())
}

/// Troca atômica de uma nota que o Orion mesmo gravou e que continua intacta.
fn replace_note(path: &Path, content: &str) -> Result<(), std::io::Error> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or_else(|| std::io::Error::other("pasta inválida"))?)?;
    file.write_all(content.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

fn walk_md_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            let meta = std::fs::symlink_metadata(&p)?;
            if meta.file_type().is_symlink() {
                continue;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    continue;
                }
            }
            if meta.is_dir() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') {
                    files.extend(walk_md_files(&p)?);
                }
            } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                files.push(p);
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_vault_manager_save_and_scan() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path().to_path_buf();

        std::fs::create_dir_all(vault_path.join("01 Projetos/Orion")).unwrap();
        std::fs::write(
            vault_path.join("01 Projetos/Orion/Orion.md"),
            "---\ntype: projeto\n---\n# Orion",
        )
        .unwrap();
        let mgr = VaultManager::new(Some(vault_path.clone())).unwrap();

        let note = ConversationNote {
            date_str: "2026-10-17".to_string(),
            time_str: "03:50".to_string(),
            project_name: "Orion".to_string(),
            title: "Teste F06".to_string(),
            summary: "Resumo do teste de sincronização nativa.".to_string(),
            tags: vec!["orion".to_string(), "teste".to_string()],
            dialogue_markdown: "Olá\n\nOi".to_string(),
            decisions: vec![],
        };

        let res = mgr.save_conversation(&note).unwrap();
        assert!(vault_path.join(&res.file_path).exists());

        let stats = mgr.index_all().unwrap();
        assert_eq!(stats.total_notes, 2);

        let results = mgr.search("sincronização");
        assert_eq!(results.len(), 1);
    }
}


/// Links `[[Nota]]` escritos no frontmatter (propriedades como `projeto: "[[Orion]]"`). O parser só
/// extrai os do corpo, mas uma nota ligada por propriedade não é uma ilha.
fn frontmatter_links(raw: &str) -> Vec<String> {
    let Some(rest) = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n")) else { return vec![] };
    let Some(end) = rest.find("\n---") else { return vec![] };
    let mut links = vec![];
    let mut texto = &rest[..end];
    while let Some(i) = texto.find("[[") {
        let Some(j) = texto[i + 2..].find("]]") else { break };
        let alvo = texto[i + 2..i + 2 + j].trim();
        if !alvo.is_empty() {
            links.push(alvo.to_string());
        }
        texto = &texto[i + 2 + j + 2..];
    }
    links
}

//! Auditoria das notas que agentes externos (Codex, Gemini, Claude e outros) escrevem no vault (F06.16).
//!
//! Esses agentes escrevem direto nos arquivos, como o `claude.md` do vault manda; o Orion não é o caminho
//! de escrita deles (REGRAS.md: o Orion como servidor MCP só lê). Depois de cada rodada de sincronização, o
//! Orion confere as notas que mudaram desde a última vez e que não são dele: segredo, frontmatter e links
//! quebrados. O que fica no log do Orion (tabela `audit`) é só o caminho, o agente e os achados, nunca o
//! conteúdo. A primeira rodada só guarda a foto do vault (não acusa o que já estava lá).

use orion_store::ops::Operations;
use orion_store::Store;
use orion_vault::{ScanError, SecretScanner, VaultParser};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::Arc;

const FOTO: &str = "vault_auditoria:foto";
const MAX_NOTAS: usize = 20_000;
const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Pasta das notas do próprio Orion: não é escrita de agente externo.
const PASTA_DO_ORION: &str = "03 Recursos/Orion/";

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resumo {
    pub conferidas: usize,
    pub com_problema: usize,
}

fn listar(raiz: &Path, pasta: &Path, out: &mut Vec<(String, std::path::PathBuf)>) {
    let Ok(dir) = std::fs::read_dir(pasta) else { return };
    for e in dir.flatten() {
        let nome = e.file_name().to_string_lossy().into_owned();
        if nome.starts_with('.') || out.len() >= MAX_NOTAS {
            continue;
        }
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            listar(raiz, &p, out);
        } else if t.is_file() && nome.ends_with(".md") {
            if let Ok(rel) = p.strip_prefix(raiz) {
                out.push((rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"), p));
            }
        }
    }
}

/// Confere o vault. Seguro de chamar a cada rodada: sem mudança externa, não registra nada.
pub fn auditar(vault: &Path, memory: &Arc<Store>) -> Resumo {
    let mut arquivos = vec![];
    listar(vault, vault, &mut arquivos);
    arquivos.sort();
    let antiga: Option<BTreeMap<String, String>> = memory.meta_get(FOTO).and_then(|v| serde_json::from_str(&v).ok());
    let stems: HashSet<String> = arquivos.iter().filter_map(|(_, p)| p.file_stem().map(|s| s.to_string_lossy().to_lowercase())).collect();
    let ops = Operations::new(memory.clone());
    let mut nova = BTreeMap::new();
    let mut resumo = Resumo::default();
    for (rel, arq) in &arquivos {
        if std::fs::metadata(arq).map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
            continue;
        }
        let Ok(texto) = std::fs::read_to_string(arq) else { continue };
        let sha: String = Sha256::digest(texto.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        let mudou = antiga.as_ref().is_some_and(|a| a.get(rel) != Some(&sha));
        nova.insert(rel.clone(), sha);
        if !mudou || rel.starts_with(PASTA_DO_ORION) {
            continue;
        }
        let meta = VaultParser::parse(rel, &texto).metadata;
        let agente = meta.frontmatter.agente.clone().or(meta.frontmatter.fonte.clone()).map(|a| a.trim().to_lowercase()).unwrap_or_default();
        if agente == "orion" {
            continue;
        }
        resumo.conferidas += 1;
        let achados = conferir(&texto, &meta, &stems);
        if !achados.is_empty() {
            resumo.com_problema += 1;
        }
        registrar(&ops, rel, if agente.is_empty() { "desconhecido" } else { &agente }, &achados);
    }
    if let Ok(foto) = serde_json::to_string(&nova) {
        let _ = memory.meta_set(FOTO, Some(&foto));
    }
    resumo
}

/// Só nomes dos problemas, nunca trechos do conteúdo.
fn conferir(texto: &str, meta: &orion_vault::NoteMetadata, stems: &HashSet<String>) -> Vec<String> {
    let mut achados = vec![];
    if let Err(ScanError::SecretFound(tipo)) = SecretScanner::scan(texto) {
        achados.push(format!("segredo ({tipo})"));
    }
    let tem_bloco = texto.trim_start().starts_with("---");
    let valido = tem_bloco && (meta.frontmatter != orion_vault::NoteFrontmatter::default() || texto.contains("type:"));
    if !valido {
        achados.push("frontmatter ausente ou inválido".into());
    }
    let quebrados = meta
        .wikilinks
        .iter()
        .filter(|l| {
            let alvo = l.split(['|', '#']).next().unwrap_or("").trim().rsplit('/').next().unwrap_or("").trim().to_lowercase();
            !alvo.is_empty() && !stems.contains(&alvo)
        })
        .count();
    if quebrados > 0 {
        achados.push(format!("{quebrados} link(s) quebrado(s)"));
    }
    achados
}

fn registrar(ops: &Operations, rel: &str, agente: &str, achados: &[String]) {
    let motivo = if achados.is_empty() { "sem problemas".to_string() } else { achados.join("; ") };
    let evento: Value = json!({
        "tool": "vault_escrita_externa",
        "action": if achados.is_empty() { "ok" } else { "alerta" },
        "reason": motivo,
        "args": {"path": rel, "agente": agente},
    });
    if let Err(e) = ops.audit_add(&evento) {
        tracing::warn!("auditoria de escrita externa no vault não gravada: {e}");
    }
    tracing::info!(target: "orion.audit", "vault_escrita_externa agente={agente} nota={rel} achados={}", achados.len());
    if !achados.is_empty() {
        let _ = ops.notify("vault_externo", &format!("Nota escrita por {agente} com problema: {rel} ({motivo})."), Some(&format!("vault:{rel}")), false);
    }
}

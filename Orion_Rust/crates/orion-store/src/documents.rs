//! Documentos (vault do Obsidian, uploads), trechos e vetores.

use crate::scope::current;
use crate::text::{chunk_text, like, strip_frontmatter, wikilinks};
use crate::vectors::para_blob;
use crate::{query_maps, Result, Store};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// Anexo ou export gigante não é nota.
pub const MAX_NOTA_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexResult {
    New,
    Updated,
    Same,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct VaultCount {
    pub new: usize,
    pub updated: usize,
    pub same: usize,
    pub removed: usize,
}

impl Store {
    /// Indexa (ou reindexa) um documento. Idempotente por hash de conteúdo.
    pub fn index_document(&self, source: &str, title: &str, texto: &str) -> Result<IndexResult> {
        let projeto = current().project_id;
        let source = match &projeto {
            Some(p) => format!("project:{p}/{source}"),
            None => source.to_string(),
        };
        let h = hex::encode(Sha256::digest(texto.as_bytes()));
        let corpo = strip_frontmatter(texto);
        let r = self.tx(|c| {
            let r: Option<(i64, String)> = c
                .query_row(
                    "SELECT id, content_hash FROM documents WHERE source=? AND project_id IS ?",
                    params![source, projeto],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            if let Some((_, hash)) = &r {
                if *hash == h {
                    return Ok(IndexResult::Same);
                }
            }
            if let Some((id, _)) = &r {
                self.delete_document_in(c, *id)?;
            }
            c.execute(
                "INSERT INTO documents(source, title, content_hash, indexed_at, project_id) VALUES (?,?,?,?,?)",
                params![source, title, h, self.now(), projeto],
            )?;
            let did = c.last_insert_rowid();
            for (i, trecho) in chunk_text(&corpo, 800, 100).iter().enumerate() {
                c.execute(
                    "INSERT INTO chunks(document_id, ord, text) VALUES (?,?,?)",
                    params![did, i as i64, format!("{title}\n\n{trecho}")],
                )?;
            }
            Ok(if r.is_some() { IndexResult::Updated } else { IndexResult::New })
        })?;
        if r != IndexResult::Same {
            self.try_embed();
        }
        Ok(r)
    }

    pub(crate) fn delete_document_in(&self, c: &Connection, doc_id: i64) -> Result<()> {
        let ids: Vec<i64> = {
            let mut st = c.prepare("SELECT id FROM chunks WHERE document_id=?")?;
            let rows = st.query_map([doc_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        self.drop_vec(c, "chunk", &ids)?;
        c.execute("DELETE FROM documents WHERE id=?", [doc_id])?;
        Ok(())
    }

    /// Documentos cuja origem começa com `prefix` (os enviados pela interface), com o número
    /// de trechos. O texto em si não vai na lista.
    pub fn list_documents(&self, prefix: &str) -> Result<Vec<Map<String, Value>>> {
        let padrao = format!("{}%", like(prefix));
        query_maps(
            &self.conn(),
            "SELECT d.id, d.source, d.title, d.indexed_at, d.project_id,
             (SELECT COUNT(*) FROM chunks c WHERE c.document_id=d.id) AS trechos
             FROM documents d WHERE d.source LIKE ? ESCAPE '\\' ORDER BY d.indexed_at DESC",
            &[&padrao],
        )
    }

    /// Só apaga o que veio de upload; nota do vault não é daqui.
    pub fn remove_document_id(&self, doc_id: i64, prefix: &str) -> Result<bool> {
        self.tx(|c| {
            let r: Option<(i64, String)> = c
                .query_row("SELECT id, source FROM documents WHERE id=?", [doc_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            match r {
                Some((id, source)) if source.starts_with(prefix) => {
                    self.delete_document_in(c, id)?;
                    Ok(true)
                }
                _ => Ok(false),
            }
        })
    }

    pub fn remove_document(&self, source: &str) -> Result<bool> {
        let projeto = current().project_id;
        let source = match &projeto {
            Some(p) => format!("project:{p}/{source}"),
            None => source.to_string(),
        };
        self.tx(|c| {
            let r: Option<i64> = c
                .query_row(
                    "SELECT id FROM documents WHERE source=? AND project_id IS ?",
                    params![source, projeto],
                    |r| r.get(0),
                )
                .optional()?;
            match r {
                Some(id) => {
                    self.delete_document_in(c, id)?;
                    Ok(true)
                }
                None => Ok(false),
            }
        })
    }

    /// Indexa os `.md` da pasta (ignora pastas com ponto, como `.obsidian`) e remove os que
    /// sumiram.
    pub fn index_vault(&self, pasta: &Path) -> Result<VaultCount> {
        let mut contagem = VaultCount::default();
        let mut arquivos = vec![];
        listar_md(pasta, pasta, &mut arquivos);
        arquivos.sort();
        let mut vistos: HashSet<String> = HashSet::new();
        let mut links: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        for (rel, arq) in arquivos {
            if std::fs::metadata(&arq).map(|m| m.len()).unwrap_or(0) > MAX_NOTA_BYTES {
                continue;
            }
            let Ok(texto) = std::fs::read_to_string(&arq) else { continue };
            vistos.insert(rel.clone());
            links.insert(rel.clone(), wikilinks(&texto));
            let titulo = arq.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            match self.index_document(&rel, &titulo, &texto)? {
                IndexResult::New => contagem.new += 1,
                IndexResult::Updated => contagem.updated += 1,
                IndexResult::Same => contagem.same += 1,
            }
        }
        let projeto = current().project_id;
        let prefixo = projeto.as_ref().map(|p| format!("project:{p}/")).unwrap_or_default();
        let antigos: Vec<String> = {
            let c = self.conn();
            let mut st = c.prepare("SELECT source FROM documents WHERE project_id IS ?")?;
            let rows = st.query_map([&projeto], |r| r.get::<_, String>(0))?;
            rows.filter_map(|r| r.ok()).map(|s| s.get(prefixo.len()..).unwrap_or("").to_string()).collect()
        };
        for fonte in antigos {
            if !vistos.contains(&fonte) && self.remove_document(&fonte)? {
                contagem.removed += 1;
            }
        }
        if projeto.is_none() {
            // o vault inteiro é da memória pessoal
            self.gravar_wikilinks(&links)?;
        }
        Ok(contagem)
    }

    /// Arestas `doc:<nota>` —liga→ `doc:<nota>` (kind `vault`), refeitas a cada indexação.
    fn gravar_wikilinks(&self, links: &BTreeMap<String, HashSet<String>>) -> Result<()> {
        let mut por_nome: HashMap<String, String> = HashMap::new();
        // a mais rasa (ordem do caminho) vence um nome repetido
        for fonte in links.keys() {
            let stem = Path::new(fonte).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
            por_nome.entry(stem).or_insert_with(|| fonte.clone());
        }
        let agora = self.now();
        let mut arestas: HashSet<(String, String)> = HashSet::new();
        for (origem, nomes) in links {
            for nome in nomes {
                if let Some(alvo) = por_nome.get(&nome.to_lowercase()) {
                    if alvo != origem {
                        arestas.insert((format!("doc:{origem}"), format!("doc:{alvo}")));
                    }
                }
            }
        }
        self.tx(|c| {
            c.execute("DELETE FROM edges WHERE kind='vault'", [])?;
            let mut st = c.prepare(
                "INSERT OR IGNORE INTO edges(src, rel, dst, weight, kind, created_at) VALUES (?,'liga',?,1,'vault',?)",
            )?;
            for (a, b) in &arestas {
                st.execute(params![a, b, agora])?;
            }
            Ok(())
        })
    }

    // ── vetores ───────────────────────────────────────────────────────────
    pub(crate) fn drop_vec(&self, c: &Connection, kind: &str, ids: &[i64]) -> Result<()> {
        let mut st = c.prepare("DELETE FROM vectors WHERE kind=? AND ref_id=?")?;
        for i in ids {
            st.execute(params![kind, i])?;
        }
        self.index.lock().invalidar(Some(kind));
        Ok(())
    }

    /// API de embedding fora do ar não derruba a escrita: o que faltou fica na fila.
    pub(crate) fn try_embed(&self) {
        if let Err(e) = self.embed_pending(32) {
            tracing::warn!("embedding adiado (fica em fila): {e}");
        }
    }

    /// Gera vetores dos itens ainda sem vetor. Devolve quantos foram feitos.
    pub fn embed_pending(&self, lote: i64) -> Result<usize> {
        let Some(emb) = self.embedder.clone() else { return Ok(0) };
        let mut feitos = 0;
        for (tabela, kind) in [("facts", "fact"), ("chunks", "chunk")] {
            loop {
                let rows: Vec<(i64, String)> = {
                    let c = self.conn();
                    let mut st = c.prepare(&format!("SELECT id, text FROM {tabela} WHERE embedded=0 LIMIT ?"))?;
                    let rows = st.query_map([lote], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    rows.collect::<rusqlite::Result<_>>()?
                };
                if rows.is_empty() {
                    break;
                }
                let textos: Vec<String> = rows.iter().map(|r| r.1.clone()).collect();
                let vetores = emb.embed(&textos).map_err(|e| crate::StoreError::Other(e.to_string()))?;
                if vetores.len() != rows.len() {
                    return Err(crate::StoreError::Other("embedder devolveu número errado de vetores".into()));
                }
                self.tx(|c| {
                    for ((id, _), v) in rows.iter().zip(&vetores) {
                        c.execute(
                            "INSERT OR REPLACE INTO vectors(kind, ref_id, embedding) VALUES (?,?,?)",
                            params![kind, id, para_blob(v)],
                        )?;
                        c.execute(&format!("UPDATE {tabela} SET embedded=1 WHERE id=?"), [id])?;
                    }
                    Ok(())
                })?;
                self.index.lock().invalidar(Some(kind));
                feitos += rows.len();
            }
        }
        Ok(feitos)
    }
}

fn listar_md(raiz: &Path, pasta: &Path, out: &mut Vec<(String, std::path::PathBuf)>) {
    let Ok(dir) = std::fs::read_dir(pasta) else { return };
    for e in dir.flatten() {
        let nome = e.file_name().to_string_lossy().into_owned();
        if nome.starts_with('.') {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            listar_md(raiz, &p, out);
        } else if nome.ends_with(".md") {
            if let Ok(rel) = p.strip_prefix(raiz) {
                let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                out.push((rel, p));
            }
        }
    }
}

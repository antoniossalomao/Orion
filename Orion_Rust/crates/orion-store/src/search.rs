//! Busca híbrida: FTS5 + vetores, fundidos por RRF, com reranker opcional.

use crate::facts::PESO_FATO_ANTIGO;
use crate::scope::{clause, current};
use crate::text::fts_query;
use crate::{Result, Store};
use rusqlite::params;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};

pub const RRF_K: f64 = 60.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Fact,
    Chunk,
    Message,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Fact => "fact",
            Kind::Chunk => "chunk",
            Kind::Message => "message",
        }
    }

    fn weight(self) -> f64 {
        match self {
            Kind::Fact => 1.25,
            Kind::Chunk => 1.0,
            Kind::Message => 0.8,
        }
    }

    pub const ALL: [Kind; 3] = [Kind::Fact, Kind::Chunk, Kind::Message];
}

#[derive(Debug, Clone, Serialize)]
pub struct Hit {
    pub kind: Kind,
    pub id: i64,
    pub text: String,
    pub source: String,
    pub score: f64,
    /// "fts", "vec" ou "fts+vec".
    pub via: String,
}

type Lista = (Kind, &'static str, Vec<(i64, String, String)>);

fn sql_fts(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Fact => (
            "SELECT t.id, t.text, t.source FROM facts_fts JOIN facts t ON t.id=facts_fts.rowid
             WHERE facts_fts MATCH ? AND {P} ORDER BY bm25(facts_fts) LIMIT ?",
            "t.project_id",
        ),
        Kind::Chunk => (
            "SELECT t.id, t.text, d.source FROM chunks_fts JOIN chunks t ON t.id=chunks_fts.rowid
             JOIN documents d ON d.id=t.document_id
             WHERE chunks_fts MATCH ? AND {P} ORDER BY bm25(chunks_fts) LIMIT ?",
            "d.project_id",
        ),
        Kind::Message => (
            "SELECT t.id, t.text, 'sessão ' || s.channel AS source FROM messages_fts
             JOIN messages t ON t.id=messages_fts.rowid JOIN sessions s ON s.id=t.session_id
             WHERE messages_fts MATCH ? AND t.superseded=0 AND t.role IN ('user','assistant')
             AND {P} ORDER BY bm25(messages_fts) LIMIT ?",
            "s.project_id",
        ),
    }
}

impl Store {
    pub fn search(&self, query: &str, k: usize, kinds: &[Kind]) -> Result<Vec<Hit>> {
        let fq = fts_query(query);
        let amplo = (k * 3).max(20);
        let mut listas: Vec<Lista> = vec![];
        {
            let c = self.conn();
            for &kind in kinds {
                let (sql, coluna) = sql_fts(kind);
                let (pred, (p, inc)) = clause(coluna);
                let itens = match &fq {
                    Some(q) => {
                        let mut st = c.prepare(&sql.replace("{P}", &pred))?;
                        let rows = st.query_map(params![q, p, inc, amplo as i64], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                        })?;
                        rows.collect::<rusqlite::Result<_>>()?
                    }
                    None => vec![],
                };
                listas.push((kind, "fts", itens));
            }
        }
        let vec_kinds: Vec<Kind> = kinds.iter().copied().filter(|k| *k != Kind::Message).collect();
        if self.embedder.is_some() && !vec_kinds.is_empty() {
            listas.extend(self.vec_lists(query, &vec_kinds, amplo)?);
        }

        let mut placar: HashMap<(Kind, i64), f64> = HashMap::new();
        let mut textos: HashMap<(Kind, i64), (String, String)> = HashMap::new();
        let mut vias: HashMap<(Kind, i64), BTreeSet<&'static str>> = HashMap::new();
        let mut ordem_insercao: Vec<(Kind, i64)> = vec![];
        for (kind, via, itens) in listas {
            for (rank, (iid, texto, fonte)) in itens.into_iter().enumerate() {
                let chave = (kind, iid);
                if !placar.contains_key(&chave) {
                    ordem_insercao.push(chave);
                }
                *placar.entry(chave).or_insert(0.0) += kind.weight() / (RRF_K + rank as f64 + 1.0);
                textos.insert(chave, (texto, fonte));
                vias.entry(chave).or_default().insert(via);
            }
        }
        let fatos: Vec<i64> = ordem_insercao.iter().filter(|(k, _)| *k == Kind::Fact).map(|(_, i)| *i).collect();
        let antigos = self.fatos_antigos(&fatos)?;
        for i in &antigos {
            // REGRAS.md: o Orion não decide sozinho qual fato vale (pesa 0,3, mas não some)
            if let Some(v) = placar.get_mut(&(Kind::Fact, *i)) {
                *v *= PESO_FATO_ANTIGO;
            }
        }
        let mut ordem = ordem_insercao;
        ordem.sort_by(|a, b| placar[b].total_cmp(&placar[a]));
        let reranker = self.reranker.lock().clone();
        if let Some(r) = reranker {
            if ordem.len() > 1 {
                ordem.truncate((k * 3).max(20));
                let trechos: Vec<String> = ordem.iter().map(|c| textos[c].0.clone()).collect();
                match r.score(query, &trechos) {
                    Ok(notas) if notas.len() == ordem.len() => {
                        for (c, n) in ordem.iter().zip(notas) {
                            let peso = if c.0 == Kind::Fact && antigos.contains(&c.1) { PESO_FATO_ANTIGO } else { 1.0 };
                            placar.insert(*c, n as f64 * peso);
                        }
                        ordem.sort_by(|a, b| placar[b].total_cmp(&placar[a]));
                    }
                    Ok(_) => tracing::warn!("reranker devolveu notas a menos; mantendo a ordem do RRF"),
                    Err(e) => tracing::warn!("reranker falhou, mantendo a ordem do RRF: {e}"),
                }
            }
        }
        ordem.truncate(k);
        Ok(ordem
            .into_iter()
            .map(|c| {
                let (texto, fonte) = textos[&c].clone();
                let marca = if c.0 == Kind::Fact && antigos.contains(&c.1) { "(antigo) " } else { "" };
                Hit {
                    kind: c.0,
                    id: c.1,
                    text: format!("{marca}{texto}"),
                    source: fonte,
                    score: placar[&c],
                    via: vias[&c].iter().copied().collect::<Vec<_>>().join("+"),
                }
            })
            .collect())
    }

    fn fatos_antigos(&self, ids: &[i64]) -> Result<HashSet<i64>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let marcas = vec!["?"; ids.len()].join(",");
        let c = self.conn();
        let mut st = c.prepare(&format!(
            "SELECT id FROM facts WHERE id IN ({marcas}) AND (superseded_by IS NOT NULL
             OR (valid_until IS NOT NULL AND valid_until<?))"
        ))?;
        let mut p: Vec<rusqlite::types::Value> = ids.iter().map(|i| (*i).into()).collect();
        p.push(self.now().into());
        let rows = st.query_map(rusqlite::params_from_iter(p), |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn vec_lists(&self, query: &str, kinds: &[Kind], amplo: usize) -> Result<Vec<Lista>> {
        let Some(emb) = self.embedder.clone() else { return Ok(vec![]) };
        let consulta = match emb.embed_query(query) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("busca vetorial indisponível, usando só palavra-chave: {e}");
                return Ok(vec![]);
            }
        };
        let scope = current();
        let mut saida = vec![];
        for &kind in kinds {
            let cache_key = format!(
                "{}@{}:{}",
                kind.as_str(),
                scope.project_id.as_deref().unwrap_or("None"),
                scope.include_personal as i32
            );
            let (coluna, join) = if kind == Kind::Fact {
                ("t.project_id", " JOIN facts t ON t.id=v.ref_id")
            } else {
                ("d.project_id", " JOIN chunks t ON t.id=v.ref_id JOIN documents d ON d.id=t.document_id")
            };
            let (pred, (p, inc)) = clause(coluna);
            let ids = {
                let c = self.conn();
                let carregar = || -> Vec<(i64, Vec<u8>)> {
                    let sql = format!("SELECT v.ref_id,v.embedding FROM vectors v{join} WHERE v.kind=? AND {pred}");
                    let Ok(mut st) = c.prepare(&sql) else { return vec![] };
                    st.query_map(params![kind.as_str(), p, inc], |r| Ok((r.get(0)?, r.get(1)?)))
                        .map(|rows| rows.filter_map(|r| r.ok()).collect())
                        .unwrap_or_default()
                };
                self.index.lock().topk(&cache_key, &consulta, amplo, carregar)
            };
            let c = self.conn();
            let mut itens = vec![];
            let sql = if kind == Kind::Fact {
                let (pred, _) = clause("t.project_id");
                format!("SELECT t.id, t.text, t.source FROM facts t WHERE t.id=? AND {pred}")
            } else {
                let (pred, _) = clause("d.project_id");
                format!(
                    "SELECT t.id, t.text, d.source FROM chunks t JOIN documents d ON d.id=t.document_id WHERE t.id=? AND {pred}"
                )
            };
            let mut st = c.prepare(&sql)?;
            for i in ids {
                if let Ok(r) = st.query_row(params![i, p, inc], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))) {
                    itens.push(r);
                }
            }
            saida.push((kind, "vec", itens));
        }
        Ok(saida)
    }
}

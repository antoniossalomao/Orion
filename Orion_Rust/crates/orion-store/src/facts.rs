//! Fatos sobre o Antônio: com fonte e data, auditáveis, editáveis
//! (REGRAS.md: o Orion não decide sozinho qual fato vale).

use crate::scope::{clause, current};
use crate::text::{colapsar_espacos, fts_query, like};
use crate::{invalid, not_found, Result, Store, StoreError};
use regex::Regex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

/// Peso de fato vencido ou substituído na busca.
pub const PESO_FATO_ANTIGO: f64 = 0.3;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Fact {
    pub id: i64,
    pub text: String,
    pub source: String,
    pub created_at: f64,
    pub updated_at: f64,
    pub project_id: Option<String>,
    /// E4.5: depois disto o fato pesa menos na busca.
    pub valid_until: Option<f64>,
    /// E4.5: id do fato que o substituiu (o Antônio decidiu).
    pub superseded_by: Option<i64>,
    /// Vencido ou substituído: sai do topo da busca e leva a marca "(antigo)".
    pub old: bool,
}

static PALAVRA_ASCII: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-z0-9]+").unwrap());

fn norm(t: &str) -> String {
    colapsar_espacos(t).to_lowercase()
}

/// Palavras de mais de 2 letras, sem acento nem caixa.
pub fn palavras(t: &str) -> HashSet<String> {
    let sem: String = t.to_lowercase().nfkd().filter(|c| c.is_ascii()).collect();
    PALAVRA_ASCII.find_iter(&sem).map(|m| m.as_str().to_string()).filter(|w| w.len() > 2).collect()
}

impl Store {
    fn fato(&self, r: &Row) -> rusqlite::Result<Fact> {
        let vence: Option<f64> = r.get("valid_until")?;
        let sup: Option<i64> = r.get("superseded_by")?;
        Ok(Fact {
            id: r.get("id")?,
            text: r.get("text")?,
            source: r.get("source")?,
            created_at: r.get("created_at")?,
            updated_at: r.get("updated_at")?,
            project_id: r.get("project_id")?,
            valid_until: vence,
            superseded_by: sup,
            old: sup.is_some() || vence.is_some_and(|v| v < self.now()),
        })
    }

    fn fact_in(&self, c: &Connection, fid: i64) -> Result<Fact> {
        Ok(c.query_row("SELECT * FROM facts WHERE id=?", [fid], |r| self.fato(r))?)
    }

    fn fato_do_escopo(c: &Connection, fid: i64) -> Result<bool> {
        let p = current().project_id;
        Ok(c.query_row("SELECT 1 FROM facts WHERE id=? AND project_id IS ?", params![fid, p], |r| r.get::<_, i64>(0))
            .optional()?
            .is_some())
    }

    /// Registra um fato. Fato idêntico (ignorando caixa e espaços) só renova a data.
    pub fn add_fact(&self, text: &str, source: &str, valid_until: Option<f64>) -> Result<Fact> {
        let texto = colapsar_espacos(text);
        if texto.is_empty() {
            return invalid("fato vazio");
        }
        let agora = self.now();
        let projeto = current().project_id;
        let fato = self.tx(|c| {
            let existentes: Vec<(i64, String)> = {
                let mut st = c.prepare("SELECT id,text FROM facts WHERE project_id IS ?")?;
                let rows = st.query_map([&projeto], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            let alvo = norm(&texto);
            for (id, t) in existentes {
                if norm(&t) == alvo {
                    c.execute("UPDATE facts SET updated_at=? WHERE id=?", params![agora, id])?;
                    return Ok((self.fact_in(c, id)?, false));
                }
            }
            c.execute(
                "INSERT INTO facts(text, source, created_at, updated_at, project_id, valid_until) VALUES (?,?,?,?,?,?)",
                params![texto, source, agora, agora, projeto, valid_until],
            )?;
            Ok((self.fact_in(c, c.last_insert_rowid())?, true))
        })?;
        if fato.1 {
            self.try_embed();
        }
        Ok(fato.0)
    }

    pub fn update_fact(&self, fact_id: i64, text: &str, source: Option<&str>) -> Result<Fact> {
        let texto = colapsar_espacos(text);
        if texto.is_empty() {
            return invalid("fato vazio");
        }
        let f = self.tx(|c| {
            if !Self::fato_do_escopo(c, fact_id)? {
                return not_found(fact_id);
            }
            c.execute(
                "UPDATE facts SET text=?, source=COALESCE(?, source), updated_at=?, embedded=0 WHERE id=?",
                params![texto, source, self.now(), fact_id],
            )?;
            self.drop_vec(c, "fact", &[fact_id])?;
            self.fact_in(c, fact_id)
        })?;
        self.try_embed();
        Ok(f)
    }

    /// Esquece de verdade: apaga o texto, o índice FTS e o vetor.
    pub fn forget_fact(&self, fact_id: i64) -> Result<bool> {
        self.tx(|c| {
            if !Self::fato_do_escopo(c, fact_id)? {
                return Ok(false);
            }
            self.drop_vec(c, "fact", &[fact_id])?;
            Ok(c.execute("DELETE FROM facts WHERE id=?", [fact_id])? > 0)
        })
    }

    /// Tudo o que o Orion sabe sobre o Antônio (no escopo atual), do mais recente ao mais antigo.
    pub fn facts(&self) -> Result<Vec<Fact>> {
        let (pred, (p, inc)) = clause("project_id");
        let c = self.conn();
        let mut st = c.prepare(&format!("SELECT * FROM facts WHERE {pred} ORDER BY updated_at DESC,id DESC"))?;
        let rows = st.query_map(params![p, inc], |r| self.fato(r))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_fact(&self, fact_id: i64) -> Result<Option<Fact>> {
        let c = self.conn();
        Ok(c.query_row("SELECT * FROM facts WHERE id=?", [fact_id], |r| self.fato(r)).optional()?)
    }

    /// Fatos que casam com a consulta (FTS) ou que a contêm como trecho literal, só do escopo
    /// atual. Fato vencido ou substituído vem depois dos vigentes.
    pub fn search_facts(&self, query: &str, limit: i64) -> Result<Vec<Fact>> {
        let limit = limit.clamp(1, 100);
        let (pred, (p, inc)) = clause("project_id");
        let pred_fts = pred.replace("project_id", "f.project_id");
        let mut ids: Vec<i64> = vec![];
        {
            let c = self.conn();
            if let Some(q) = fts_query(query) {
                let mut st = c.prepare(&format!(
                    "SELECT f.id FROM facts_fts JOIN facts f ON f.id=facts_fts.rowid
                     WHERE facts_fts MATCH ? AND {pred_fts} ORDER BY rank LIMIT ?"
                ))?;
                ids = st.query_map(params![q, p, inc, limit], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            }
            let mut st = c.prepare(&format!(
                "SELECT id FROM facts WHERE lower(text) LIKE ? ESCAPE '\\' AND {pred} LIMIT ?"
            ))?;
            let padrao = format!("%{}%", like(&query.trim().to_lowercase()));
            for id in st.query_map(params![padrao, p, inc, limit], |r| r.get::<_, i64>(0))? {
                let id = id?;
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        let mut fatos: Vec<Fact> = ids
            .into_iter()
            .take(limit as usize)
            .filter_map(|i| self.get_fact(i).ok().flatten())
            .collect();
        fatos.sort_by_key(|f| f.old); // estável: mantém a ordem da busca
        Ok(fatos)
    }

    /// Muda o escopo do fato (`None` = pessoal). Quem chama já conferiu o projeto de destino.
    /// Ligações de substituição ou conflito com fatos do escopo antigo são desfeitas.
    pub fn move_fact(&self, fact_id: i64, project_id: Option<&str>) -> Result<Fact> {
        let f = self.tx(|c| {
            if c.query_row("SELECT 1 FROM facts WHERE id=?", [fact_id], |r| r.get::<_, i64>(0)).optional()?.is_none() {
                return not_found(fact_id);
            }
            c.execute("UPDATE facts SET project_id=? WHERE id=?", params![project_id, fact_id])?;
            c.execute("UPDATE facts SET superseded_by=NULL WHERE superseded_by=?", [fact_id])?;
            c.execute("UPDATE facts SET superseded_by=NULL WHERE id=?", [fact_id])?;
            c.execute("DELETE FROM fact_conflicts WHERE older_id=?1 OR newer_id=?1", [fact_id])?;
            self.fact_in(c, fact_id)
        })?;
        self.index.lock().invalidar(Some("fact"));
        Ok(f)
    }

    /// Data em que o fato deixa de valer (`None` = sem prazo). Só o Antônio define.
    pub fn set_fact_validity(&self, fact_id: i64, valid_until: Option<f64>) -> Result<Fact> {
        self.tx(|c| {
            if !Self::fato_do_escopo(c, fact_id)? {
                return not_found(fact_id);
            }
            c.execute("UPDATE facts SET valid_until=? WHERE id=?", params![valid_until, fact_id])?;
            self.fact_in(c, fact_id)
        })
    }

    /// O fato `new_id` vale no lugar de `old_id`. Os dois ficam no banco; o antigo pesa 0,3 na
    /// busca e leva a marca "(antigo)". Só no mesmo escopo.
    pub fn supersede_fact(&self, old_id: i64, new_id: i64) -> Result<Fact> {
        if old_id == new_id {
            return invalid("um fato não substitui a si mesmo");
        }
        self.tx(|c| {
            for fid in [old_id, new_id] {
                if !Self::fato_do_escopo(c, fid)? {
                    return not_found(fid);
                }
            }
            c.execute("UPDATE facts SET superseded_by=? WHERE id=?", params![new_id, old_id])?;
            self.fact_in(c, old_id)
        })
    }

    /// Registra o par para decisão. `false` se o par já foi visto (decidido ou não).
    pub fn record_conflict(&self, older_id: i64, newer_id: i64) -> Result<bool> {
        self.tx(|c| {
            Ok(c.execute(
                "INSERT OR IGNORE INTO fact_conflicts(older_id, newer_id, created_at) VALUES (?,?,?)",
                params![older_id, newer_id, self.now()],
            )? > 0)
        })
    }

    /// Pares esperando decisão, só do escopo atual.
    pub fn pending_conflicts(&self) -> Result<Vec<(Fact, Fact)>> {
        let (pred, (p, inc)) = clause("a.project_id");
        let pares: Vec<(i64, i64)> = {
            let c = self.conn();
            let mut st = c.prepare(&format!(
                "SELECT fc.older_id, fc.newer_id FROM fact_conflicts fc
                 JOIN facts a ON a.id=fc.older_id JOIN facts b ON b.id=fc.newer_id
                 WHERE fc.status='pendente' AND {pred} AND a.project_id IS b.project_id
                 ORDER BY fc.created_at, fc.older_id"
            ))?;
            let rows = st.query_map(params![p, inc], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        Ok(pares
            .into_iter()
            .filter_map(|(a, b)| Some((self.get_fact(a).ok()??, self.get_fact(b).ok()??)))
            .collect())
    }

    /// `novo`: o mais novo vale (marca `superseded_by`); `ambos`: os dois valem.
    pub fn resolve_conflict(&self, older_id: i64, newer_id: i64, decisao: &str) -> Result<()> {
        if decisao != "novo" && decisao != "ambos" {
            return invalid("decisão inválida");
        }
        let existe = self
            .conn()
            .query_row(
                "SELECT 1 FROM fact_conflicts WHERE older_id=? AND newer_id=? AND status='pendente'",
                [older_id, newer_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        if existe.is_none() {
            return not_found(format!("({older_id}, {newer_id})"));
        }
        if decisao == "novo" {
            self.supersede_fact(older_id, newer_id)?;
        }
        self.tx(|c| {
            c.execute(
                "UPDATE fact_conflicts SET status=? WHERE older_id=? AND newer_id=?",
                params![decisao, older_id, newer_id],
            )?;
            Ok(())
        })
    }

    /// Pares de fatos quase iguais (Jaccard das palavras). Só sugere: quem decide é o Antônio.
    pub fn duplicate_facts(&self, limiar: f64) -> Result<Vec<(Fact, Fact, f64)>> {
        let fatos = self.facts()?;
        let conj: HashMap<i64, HashSet<String>> = fatos.iter().map(|f| (f.id, palavras(&f.text))).collect();
        let mut achados = vec![];
        for (i, a) in fatos.iter().enumerate() {
            for b in &fatos[i + 1..] {
                let (ca, cb) = (&conj[&a.id], &conj[&b.id]);
                let u = ca.union(cb).count();
                if u == 0 {
                    continue;
                }
                let j = ca.intersection(cb).count() as f64 / u as f64;
                if j >= limiar {
                    achados.push((a.clone(), b.clone(), j));
                }
            }
        }
        achados.sort_by(|x, y| y.2.total_cmp(&x.2));
        Ok(achados)
    }

    /// Pares (mais antigo, mais novo) do mesmo assunto com informação diferente. Fato já
    /// substituído ou gerado pelo sono não entra. Só sugere.
    pub fn conflicting_facts(&self, limite: usize) -> Result<Vec<(Fact, Fact)>> {
        let fatos: Vec<Fact> = self
            .facts()?
            .into_iter()
            .take(limite)
            .filter(|f| f.superseded_by.is_none() && !f.source.starts_with("sono:"))
            .collect();
        let conj: HashMap<i64, HashSet<String>> = fatos.iter().map(|f| (f.id, palavras(&f.text))).collect();
        let mut achados = vec![];
        for (i, a) in fatos.iter().enumerate() {
            for b in &fatos[i + 1..] {
                let (ca, cb) = (&conj[&a.id], &conj[&b.id]);
                let comum = ca.intersection(cb).count();
                let menor = ca.len().min(cb.len());
                if menor == 0 || comum < 2 || (comum as f64) / (menor as f64) < 0.5 {
                    continue;
                }
                let uniao = ca.union(cb).count();
                if comum as f64 / uniao as f64 >= 0.75 || ca.is_subset(cb) || cb.is_subset(ca) {
                    continue;
                }
                let (velho, novo) = if (a.created_at, a.id) <= (b.created_at, b.id) { (a, b) } else { (b, a) };
                achados.push((velho.clone(), novo.clone()));
            }
        }
        Ok(achados)
    }

    /// Nota para o vault do Obsidian (a pessoa confere e corrige em texto).
    pub fn facts_markdown(&self) -> Result<String> {
        let local = |t: f64| {
            chrono::DateTime::from_timestamp(t as i64, 0).unwrap_or_default().with_timezone(&chrono::Local)
        };
        let agora = local(self.now());
        let mut linhas = vec![
            "---".to_string(),
            "type: fatos-orion".into(),
            format!("gerado: {}", agora.format("%Y-%m-%dT%H:%M:%S%:z")),
            "fonte: orion".into(),
            "---".into(),
            String::new(),
            "# O que o Orion sabe sobre o Antônio".into(),
            String::new(),
        ];
        for f in self.facts()? {
            let data = local(f.updated_at).format("%Y-%m-%d");
            linhas.push(format!("- {} _(fonte: {}, {data}, id {})_", f.text, f.source, f.id));
        }
        Ok(linhas.join("\n") + "\n")
    }

    pub fn export_facts(&self, pasta: &Path, nome: &str) -> Result<PathBuf> {
        let destino = pasta.join(nome);
        let io = |e: std::io::Error| StoreError::Other(e.to_string());
        std::fs::create_dir_all(pasta).map_err(io)?;
        std::fs::write(&destino, self.facts_markdown()?).map_err(io)?;
        Ok(destino)
    }
}

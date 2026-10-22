//! Registro de saídas (REGRAS.md: toda saída de dado fica registrada, sem conteúdo), biblioteca
//! de resultados e memória da tela (REGRAS.md: memória da tela: só texto, local e com opt-in).

use crate::text::{fts_query, like};
use crate::{query_map_one, query_maps, Result, Store};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Percentil pelo vizinho mais próximo (lista já ordenada); 0 se vazia.
pub fn percentil(ordenados: &[i64], p: i64) -> i64 {
    if ordenados.is_empty() {
        return 0;
    }
    let n = ordenados.len() as i64;
    let i = ((p * n + 99) / 100 - 1).clamp(0, n - 1);
    ordenados[i as usize]
}

fn corta(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

#[derive(Debug, Clone, Default)]
pub struct ExternalCall<'a> {
    pub provider: &'a str,
    pub kind: &'a str,
    pub ok: bool,
    pub latency_ms: i64,
    pub model: &'a str,
    pub bytes_out: i64,
    pub bytes_in: i64,
    pub content_kind: &'a str,
    pub ts: Option<f64>,
}

impl Store {
    pub fn add_external_call(&self, e: &ExternalCall) -> Result<i64> {
        self.tx(|c| {
            c.execute(
                "INSERT INTO external_calls(ts, provider, kind, model, ok, latency_ms, bytes_out, bytes_in, content_kind)
                 VALUES (?,?,?,?,?,?,?,?,?)",
                params![
                    e.ts.unwrap_or_else(|| self.now()),
                    corta(e.provider, 60),
                    corta(e.kind, 30),
                    corta(e.model, 120),
                    e.ok as i64,
                    e.latency_ms.max(0),
                    e.bytes_out.max(0),
                    e.bytes_in.max(0),
                    corta(e.content_kind, 20),
                ],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    /// Linhas desde `desde` (mais novas primeiro); `prefixo` filtra o provedor.
    pub fn external_calls(&self, desde: f64, prefixo: Option<&str>, limite: i64) -> Result<Vec<Map<String, Value>>> {
        let lim = limite.clamp(1, 50_000);
        let c = self.conn();
        match prefixo.filter(|p| !p.is_empty()) {
            Some(p) => query_maps(
                &c,
                "SELECT * FROM external_calls WHERE ts>=? AND provider LIKE ? ESCAPE '\\' ORDER BY ts DESC, id DESC LIMIT ?",
                &[&desde, &format!("{}%", like(p)), &lim],
            ),
            None => query_maps(&c, "SELECT * FROM external_calls WHERE ts>=? ORDER BY ts DESC, id DESC LIMIT ?", &[&desde, &lim]),
        }
    }

    /// Chamadas de um provedor (ou família, `gateway:`) desde `desde`: a base da cota.
    pub fn external_calls_count(&self, provider_prefix: &str, desde: f64) -> i64 {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM external_calls WHERE ts>=? AND provider LIKE ? ESCAPE '\\'",
                params![desde, format!("{}%", like(provider_prefix))],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }

    /// Por provedor e tipo: chamadas, falhas, p50/p95 da latência, bytes e o modelo mais usado.
    pub fn external_calls_summary(&self, desde: f64) -> Result<Vec<Value>> {
        let linhas = query_maps(
            &self.conn(),
            "SELECT provider, kind, model, ok, latency_ms, bytes_out, bytes_in FROM external_calls WHERE ts>=? ORDER BY ts",
            &[&desde],
        )?;
        let mut grupos: BTreeMap<(String, String), Vec<Map<String, Value>>> = BTreeMap::new();
        for r in linhas {
            let k = (r["provider"].as_str().unwrap_or("").to_string(), r["kind"].as_str().unwrap_or("").to_string());
            grupos.entry(k).or_default().push(r);
        }
        let i = |r: &Map<String, Value>, k: &str| r.get(k).and_then(Value::as_i64).unwrap_or(0);
        Ok(grupos
            .into_iter()
            .map(|((provider, kind), linhas)| {
                let mut lat: Vec<i64> = linhas.iter().map(|r| i(r, "latency_ms")).collect();
                lat.sort();
                let mut modelos: Vec<(String, usize)> = vec![];
                for r in &linhas {
                    let m = r["model"].as_str().unwrap_or("");
                    if !m.is_empty() {
                        match modelos.iter_mut().find(|(x, _)| x == m) {
                            Some(e) => e.1 += 1,
                            None => modelos.push((m.to_string(), 1)),
                        }
                    }
                }
                // max() do Python devolve o primeiro em caso de empate
                let modelo = modelos
                    .iter()
                    .fold(None::<&(String, usize)>, |acc, x| match acc {
                        Some(a) if a.1 >= x.1 => Some(a),
                        _ => Some(x),
                    })
                    .map(|x| x.0.clone())
                    .unwrap_or_default();
                json!({
                    "provider": provider,
                    "kind": kind,
                    "chamadas": linhas.len(),
                    "falhas": linhas.iter().filter(|r| i(r, "ok") == 0).count(),
                    "p50_ms": percentil(&lat, 50),
                    "p95_ms": percentil(&lat, 95),
                    "bytes_out": linhas.iter().map(|r| i(r, "bytes_out")).sum::<i64>(),
                    "bytes_in": linhas.iter().map(|r| i(r, "bytes_in")).sum::<i64>(),
                    "modelo": modelo,
                })
            })
            .collect())
    }

    pub fn prune_external_calls(&self, dias: i64) -> Result<usize> {
        let limite = self.now() - (dias * 86400) as f64;
        self.tx(|c| Ok(c.execute("DELETE FROM external_calls WHERE ts < ?", [limite])?))
    }

    // ── biblioteca de resultados (C34) ────────────────────────────────────
    /// Registra um resultado; mesmo nome de novo vira a versão seguinte (a antiga fica).
    pub fn add_artifact(&self, session_id: Option<&str>, kind: &str, name: &str, stored: &str, size: i64, tool: &str) -> Result<Map<String, Value>> {
        self.tx(|c| {
            let ant: Option<(i64, i64)> = c
                .query_row(
                    "SELECT id, version FROM library_results WHERE name=? AND kind=? ORDER BY version DESC LIMIT 1",
                    params![name, kind],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let projeto: Option<String> = match session_id {
                Some(s) => c.query_row("SELECT project_id FROM sessions WHERE id=?", [s], |r| r.get(0)).optional()?.flatten(),
                None => None,
            };
            c.execute(
                "INSERT INTO library_results(session_id, project_id, kind, name, stored, bytes, tool, version, parent_id, created_at)
                 VALUES (?,?,?,?,?,?,?,?,?,?)",
                params![
                    session_id,
                    projeto,
                    kind,
                    name,
                    stored,
                    size,
                    tool,
                    ant.map(|a| a.1 + 1).unwrap_or(1),
                    ant.map(|a| a.0),
                    self.now()
                ],
            )?;
            let id = c.last_insert_rowid();
            Ok(query_map_one(c, "SELECT * FROM library_results WHERE id=?", &[&id])?.unwrap_or_default())
        })
    }

    pub fn get_artifact(&self, aid: i64) -> Result<Option<Map<String, Value>>> {
        query_map_one(&self.conn(), "SELECT * FROM library_results WHERE id=?", &[&aid])
    }

    pub fn list_artifacts(&self, project_id: Option<&str>, limit: i64) -> Result<Vec<Map<String, Value>>> {
        query_maps(
            &self.conn(),
            "SELECT a.*, s.title AS sessao_titulo FROM library_results a LEFT JOIN sessions s ON s.id=a.session_id
             WHERE (?1 IS NULL OR a.project_id=?1) ORDER BY a.created_at DESC, a.id DESC LIMIT ?2",
            &[&project_id, &limit.clamp(1, 500)],
        )
    }

    /// Versões além das `manter` mais novas de cada (nome, tipo); só `nome`, se dado (E6.3).
    pub fn surplus_artifacts(&self, manter: usize, nome: Option<&str>) -> Result<Vec<Map<String, Value>>> {
        let rows = query_maps(
            &self.conn(),
            "SELECT id, name, kind, version, stored, bytes FROM library_results
             WHERE (?1 IS NULL OR name=?1) ORDER BY name, kind, version DESC, id DESC",
            &[&nome],
        )?;
        let mut vistos: BTreeMap<(String, String), usize> = BTreeMap::new();
        Ok(rows
            .into_iter()
            .filter(|r| {
                let k = (r["name"].to_string(), r["kind"].to_string());
                let n = vistos.entry(k).or_insert(0);
                *n += 1;
                *n > manter
            })
            .collect())
    }

    /// Apaga o registro e devolve o nome do arquivo guardado (para o chamador apagar).
    pub fn delete_artifact(&self, aid: i64) -> Result<Option<String>> {
        self.tx(|c| {
            let r: Option<String> =
                c.query_row("SELECT stored FROM library_results WHERE id=?", [aid], |r| r.get(0)).optional()?;
            if r.is_some() {
                c.execute("DELETE FROM library_results WHERE id=?", [aid])?;
            }
            Ok(r)
        })
    }

    // ── memória da tela (D3): texto de OCR, retenção curta, nunca imagem ───
    pub fn add_screen(&self, texto: &str, title: &str, app: &str) -> Result<i64> {
        let titulo = corta(&title.split_whitespace().collect::<Vec<_>>().join(" "), 200);
        self.tx(|c| {
            c.execute(
                "INSERT INTO screen_log(ts, title, text, app) VALUES (?,?,?,?)",
                params![self.now(), titulo, texto, corta(app, 80)],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    pub fn last_screen_text(&self) -> Option<String> {
        self.conn().query_row("SELECT text FROM screen_log ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).ok()
    }

    pub fn search_screen(&self, query: &str, limit: i64, dias: i64) -> Result<Vec<Value>> {
        let Some(q) = fts_query(query) else { return Ok(vec![]) };
        let desde = self.now() - (dias * 86400) as f64;
        let rows = query_maps(
            &self.conn(),
            "SELECT s.id, s.ts, s.title, s.app, snippet(screen_log_fts, 0, '[', ']', '…', 24) AS trecho
             FROM screen_log_fts f JOIN screen_log s ON s.id=f.rowid
             WHERE screen_log_fts MATCH ? AND s.ts>=? ORDER BY f.rank LIMIT ?",
            &[&q, &desde, &limit.clamp(1, 20)],
        )?;
        Ok(rows
            .into_iter()
            .map(|r| json!({"id": r["id"], "ts": r["ts"], "titulo": r["title"], "app": r["app"], "trecho": r["trecho"]}))
            .collect())
    }

    pub fn prune_screen(&self, dias: i64) -> Result<usize> {
        let limite = self.now() - (dias * 86400) as f64;
        self.tx(|c| Ok(c.execute("DELETE FROM screen_log WHERE ts<?", [limite])?))
    }

    pub fn clear_screen(&self) -> Result<usize> {
        self.tx(|c| Ok(c.execute("DELETE FROM screen_log", [])?))
    }

    pub fn screen_count(&self, desde: f64) -> i64 {
        self.conn().query_row("SELECT COUNT(*) FROM screen_log WHERE ts>=?", [desde], |r| r.get(0)).unwrap_or(0)
    }
}

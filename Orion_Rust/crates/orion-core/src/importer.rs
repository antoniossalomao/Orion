//! Importa o export do SurrealDB do legado (fase 0 → fase 3 do NUCLEO).
//!
//! O formato é o que a ferramenta `backup_memoria` do legado já grava: um `<tabela>.json` por
//! tabela, cada um uma lista de registros. Conversas: `sessao` → sessões arquivadas (canal
//! "legado") e `evento` → mensagens com a data original, pesquisáveis por FTS. Operação:
//! `lembrete`, `agendamento`, `tarefa`, `numero` e `prompt` viram as tabelas de `Operations`.
//! Grafo: as relações `precedeu`, `sobre` e `conecta` viram arestas. Idempotente: rodar de novo não
//! duplica.

use chrono::{Local, NaiveDateTime, TimeZone};
use orion_store::ops::{next_run, TIPOS_AGENDAMENTO};
use orion_store::rusqlite::params;
use orion_store::Store;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub const CANAL_LEGADO: &str = "legado";
pub const SEM_SESSAO: &str = "pre-sessoes";
pub const OPERACIONAIS: [&str; 5] = ["lembrete", "agendamento", "tarefa", "numero", "prompt"];
pub const RELACOES: [&str; 3] = ["precedeu", "sobre", "conecta"];
const USUARIO: [&str; 2] = ["antônio", "antonio"];
/// Pendência vencida há mais que isso entra já "avisada": sem rajada de avisos velhos.
const VENCIDO_HA: f64 = 86_400.0;

#[derive(Debug, Default, Clone)]
pub struct ImportReport {
    pub sessoes: usize,
    pub mensagens: usize,
    pub ja_importadas: usize,
    pub invalidas: usize,
    pub invalidas_por_tabela: BTreeMap<String, usize>,
    /// Atores de evento que não são o Antônio nem o assistente informado: viram papel "system".
    pub atores_como_system: BTreeMap<String, usize>,
    pub operacionais: BTreeMap<String, usize>,
    pub operacionais_repetidos: BTreeMap<String, usize>,
    pub arestas: usize,
    pub arestas_repetidas: usize,
    /// Ponta que não virou mensagem/tópico.
    pub arestas_sem_no: usize,
}

impl ImportReport {
    fn invalida(&mut self, tabela: &str) {
        self.invalidas += 1;
        *self.invalidas_por_tabela.entry(tabela.into()).or_default() += 1;
    }

    pub fn resumo(&self) -> String {
        let ops = if self.operacionais.is_empty() { "nenhuma".to_string() } else { self.operacionais.iter().map(|(t, n)| format!("{t}={n}")).collect::<Vec<_>>().join(", ") };
        let mut linha = format!(
            "sessões novas={} mensagens novas={} já importadas={} inválidas={} operação nova: {ops} · arestas novas={} repetidas={} sem nó={}",
            self.sessoes, self.mensagens, self.ja_importadas, self.invalidas, self.arestas, self.arestas_repetidas, self.arestas_sem_no
        );
        if !self.atores_como_system.is_empty() {
            let quem = self.atores_como_system.iter().map(|(a, n)| format!("{a}={n}")).collect::<Vec<_>>().join(", ");
            linha += &format!("\nATENÇÃO: atores tratados como 'system' ({quem}). Se algum é o nome antigo do assistente, rode de novo com --assistente <nome>.");
        }
        linha
    }
}

/// "sessao:⟨uuid⟩" / "sessao:`uuid`" / "uuid" → "uuid".
fn id_limpo(rid: &Value) -> String {
    if rid.is_null() {
        // registro sem `id`: o Python lê `r.get("id", "")` e descarta como inválido
        return String::new();
    }
    let s = rid.as_str().map(String::from).unwrap_or_else(|| rid.to_string());
    let fim = s.rsplit_once(':').map(|x| x.1).unwrap_or(&s);
    fim.trim_matches(|c: char| "⟨⟩`'\" ".contains(c)).to_string()
}

fn ts(v: &Value) -> f64 {
    let Some(s) = v.as_str() else { return 0.0 };
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return d.timestamp_micros() as f64 / 1e6;
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d"] {
        let n = NaiveDateTime::parse_from_str(s, f).or_else(|_| chrono::NaiveDate::parse_from_str(s, f).map(|d| d.and_hms_opt(0, 0, 0).unwrap()));
        if let Ok(n) = n {
            return Local.from_local_datetime(&n).earliest().map(|d| d.timestamp_micros() as f64 / 1e6).unwrap_or(0.0);
        }
    }
    0.0
}

fn texto(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.trim().to_string(),
        o => o.to_string().trim().to_string(),
    }
}

pub fn ler(pasta: &Path, tabela: &str) -> Result<Option<Vec<Map<String, Value>>>, String> {
    let arq = pasta.join(format!("{tabela}.json"));
    if !arq.exists() {
        return Ok(None);
    }
    let dados: Value = serde_json::from_str(&std::fs::read_to_string(&arq).map_err(|e| e.to_string())?).map_err(|e| format!("{tabela}.json: {e}"))?;
    let Some(lista) = dados.as_array() else { return Err(format!("{tabela}.json: esperava uma lista de registros")) };
    Ok(Some(lista.iter().filter_map(|d| d.as_object().cloned()).collect()))
}

pub fn import_surreal_export(store: &Store, pasta: &Path, assistentes: &[String]) -> Result<ImportReport, String> {
    let eventos = ler(pasta, "evento")?.ok_or_else(|| format!("{} não existe (rode backup_memoria no legado)", pasta.join("evento.json").display()))?;
    let mut rel = ImportReport::default();
    let nomes: Vec<String> = assistentes.iter().map(|a| a.trim().to_lowercase()).filter(|a| !a.is_empty()).collect();
    let mut sessoes: HashMap<String, String> = HashMap::new();
    for s in ler(pasta, "sessao")?.unwrap_or_default() {
        let ext = id_limpo(s.get("id").unwrap_or(&Value::Null));
        if ext.is_empty() {
            rel.invalida("sessao");
            continue;
        }
        let titulo = s.get("titulo").and_then(Value::as_str).filter(|t| !t.is_empty()).unwrap_or("conversa sem título");
        let (id, nova) = store.import_session(&ext, CANAL_LEGADO, Some(titulo), ts(s.get("criada").unwrap_or(&Value::Null))).map_err(|e| e.to_string())?;
        sessoes.insert(ext, id);
        rel.sessoes += nova as usize;
    }
    let mut ordenados = eventos;
    ordenados.sort_by(|a, b| ts(a.get("timestamp").unwrap_or(&Value::Null)).partial_cmp(&ts(b.get("timestamp").unwrap_or(&Value::Null))).unwrap_or(std::cmp::Ordering::Equal));
    for ev in &ordenados {
        let t = texto(ev.get("texto").unwrap_or(&Value::Null));
        let ext = id_limpo(ev.get("id").unwrap_or(&Value::Null));
        if t.is_empty() || ext.is_empty() {
            rel.invalida("evento");
            continue;
        }
        let ator = texto(ev.get("ator").unwrap_or(&Value::Null)).to_lowercase();
        let papel = if nomes.contains(&ator) { "assistant" } else if USUARIO.contains(&ator.as_str()) { "user" } else { "system" };
        if papel == "system" && !["sistema", "system"].contains(&ator.as_str()) {
            *rel.atores_como_system.entry(if ator.is_empty() { "(sem ator)".into() } else { ator.clone() }).or_default() += 1;
        }
        let mut prov = json!({"importado": "surrealdb", "fonte": ev.get("fonte").cloned().unwrap_or(Value::Null)});
        for k in ["intencao", "fontes_rag"] {
            if ev.get(k).is_some_and(|v| !v.is_null() && v != &json!("") && v != &json!([])) {
                prov[k] = ev[k].clone();
            }
        }
        let chave = ev.get("sessao_id").filter(|v| !v.is_null() && **v != json!("")).map(id_limpo);
        let sk = chave.clone().unwrap_or_else(|| SEM_SESSAO.to_string());
        if !sessoes.contains_key(&sk) {
            let titulo = if chave.is_none() { "conversas antigas (pré-sessões)".to_string() } else { format!("sessão {}", sk.chars().take(8).collect::<String>()) };
            let (id, nova) = store.import_session(&sk, CANAL_LEGADO, Some(&titulo), 0.0).map_err(|e| e.to_string())?;
            sessoes.insert(sk.clone(), id);
            rel.sessoes += nova as usize;
        }
        let sid = &sessoes[&sk];
        if store.import_message(sid, &ext, papel, &t, ts(ev.get("timestamp").unwrap_or(&Value::Null)), Some(&prov)).map_err(|e| e.to_string())? {
            rel.mensagens += 1;
        } else {
            rel.ja_importadas += 1;
        }
    }
    importar_operacao(store, pasta, &mut rel)?;
    importar_arestas(store, pasta, &mut rel)?;
    tracing::info!("importação: {}", rel.resumo());
    Ok(rel)
}

type Linha = (&'static str, Vec<orion_store::rusqlite::types::Value>);

fn sv(x: &str) -> orion_store::rusqlite::types::Value {
    orion_store::rusqlite::types::Value::Text(x.into())
}
fn fv(x: f64) -> orion_store::rusqlite::types::Value {
    orion_store::rusqlite::types::Value::Real(x)
}
fn iv(x: i64) -> orion_store::rusqlite::types::Value {
    orion_store::rusqlite::types::Value::Integer(x)
}
fn nulo() -> orion_store::rusqlite::types::Value {
    orion_store::rusqlite::types::Value::Null
}

fn g<'a>(r: &'a Map<String, Value>, k: &str) -> &'a Value {
    r.get(k).unwrap_or(&Value::Null)
}

fn lembrete(r: &Map<String, Value>, agora: f64) -> Result<Linha, String> {
    let (titulo, quando) = (texto(g(r, "titulo")), ts(g(r, "quando")));
    if titulo.is_empty() || quando <= 0.0 {
        return Err("lembrete sem título ou sem data".into());
    }
    let feito = g(r, "concluido").as_bool().unwrap_or(false);
    let avisado = feito || quando < agora - VENCIDO_HA;
    let criado = ts(g(r, "criado"));
    Ok(("INSERT INTO reminders(title, due_at, note, done, notified, created_at) VALUES (?,?,?,?,?,?)", vec![sv(&titulo), fv(quando), sv(&texto(g(r, "nota"))), iv(feito as i64), iv(avisado as i64), fv(if criado > 0.0 { criado } else { agora })]))
}

fn agendamento(r: &Map<String, Value>, agora: f64) -> Result<Linha, String> {
    let titulo = texto(g(r, "titulo"));
    let tipo = Some(texto(g(r, "tipo"))).filter(|t| !t.is_empty()).unwrap_or_else(|| "unico".into());
    if titulo.is_empty() || !TIPOS_AGENDAMENTO.contains(&tipo.as_str()) {
        return Err("agendamento sem título ou com tipo desconhecido".into());
    }
    let horario = texto(g(r, "horario"));
    let intervalo = g(r, "intervalo_min").as_i64().unwrap_or(0);
    let mut ativo = g(r, "ativo") != &json!(false);
    let mut prox = Some(ts(g(r, "proxima_execucao"))).filter(|p| *p > 0.0);
    if tipo == "unico" {
        if prox.is_none_or(|p| p < agora - VENCIDO_HA) {
            // nunca disparou e já passou: não dispara agora
            prox = None;
            ativo = false;
        }
    } else if prox.is_none_or(|p| p <= agora) {
        // recorrente atrasado: próximo horário a partir de agora
        prox = next_run(&tipo, agora, None, &horario, intervalo).ok();
    }
    let params_json = serde_json::from_str::<Value>(&{
        let t = texto(g(r, "parametros"));
        if t.is_empty() { "{}".into() } else { t }
    })
    .ok()
    .filter(Value::is_object)
    .unwrap_or_else(|| json!({}));
    let ultima = Some(ts(g(r, "ultima_execucao"))).filter(|p| *p > 0.0);
    let criado = ts(g(r, "criado"));
    Ok((
        "INSERT INTO schedules(title, kind, time_of_day, interval_min, tool, params, next_run, last_run, active, created_at) VALUES (?,?,?,?,?,?,?,?,?,?)",
        vec![sv(&titulo), sv(&tipo), sv(&horario), iv(intervalo), sv(&texto(g(r, "ferramenta"))), sv(&orion_policy::pyjson::dumps(&params_json, false)), prox.map(fv).unwrap_or_else(nulo), ultima.map(fv).unwrap_or_else(nulo), iv((ativo && prox.is_some()) as i64), fv(if criado > 0.0 { criado } else { agora })],
    ))
}

fn tarefa(r: &Map<String, Value>, agora: f64) -> Result<Linha, String> {
    let titulo = texto(g(r, "titulo"));
    if titulo.is_empty() {
        return Err("tarefa sem título".into());
    }
    let status = texto(g(r, "status"));
    let status = if ["pendente", "em_andamento", "concluida"].contains(&status.as_str()) { status } else { "pendente".into() };
    let c = ts(g(r, "criado"));
    let criado = if c > 0.0 { c } else { agora };
    Ok(("INSERT INTO tasks(title, status, created_at, updated_at) VALUES (?,?,?,?)", vec![sv(&titulo), sv(&status), fv(criado), fv(criado)]))
}

fn numero(r: &Map<String, Value>, agora: f64) -> Result<Linha, String> {
    let alvo = texto(g(r, "alvo"));
    if alvo.is_empty() {
        return Err("número sem alvo".into());
    }
    // `float(r.get("score") or 0.0)`: texto que não é número invalida o registro
    let score = match g(r, "score") {
        Value::Null | Value::Bool(false) => 0.0,
        Value::Bool(true) => 1.0,
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(t) if t.is_empty() => 0.0,
        Value::String(t) => t.trim().parse::<f64>().map_err(|_| format!("score inválido: {t:?}"))?,
        _ => return Err("score inválido".into()),
    }
    .clamp(0.0, 1.0);
    let fonte = Some(texto(g(r, "fonte"))).filter(|f| !f.is_empty()).unwrap_or_else(|| "orion".into());
    let c = ts(g(r, "criado"));
    Ok(("INSERT INTO numbers(target, score, reason, source, notified, created_at) VALUES (?,?,?,?,?,?)", vec![sv(&alvo), fv(score), sv(&texto(g(r, "motivo"))), sv(&fonte), iv(g(r, "notificado").as_bool().unwrap_or(false) as i64), fv(if c > 0.0 { c } else { agora })]))
}

fn prompt(r: &Map<String, Value>, agora: f64) -> Result<Linha, String> {
    let (titulo, conteudo) = (texto(g(r, "titulo")), texto(g(r, "conteudo")));
    if titulo.is_empty() || conteudo.is_empty() {
        return Err("prompt sem título ou conteúdo".into());
    }
    let c = ts(g(r, "criado_em"));
    Ok(("INSERT INTO prompts(title, command, content, created_at) VALUES (?,?,?,?)", vec![sv(&titulo), sv(&texto(g(r, "comando"))), sv(&conteudo), fv(if c > 0.0 { c } else { agora })]))
}

fn importar_operacao(store: &Store, pasta: &Path, rel: &mut ImportReport) -> Result<(), String> {
    let agora = store.now();
    for tabela in OPERACIONAIS {
        for r in ler(pasta, tabela)?.unwrap_or_default() {
            let ext = id_limpo(g(&r, "id"));
            if ext.is_empty() {
                rel.invalida(tabela);
                continue;
            }
            let mapa = match tabela {
                "lembrete" => lembrete(&r, agora),
                "agendamento" => agendamento(&r, agora),
                "tarefa" => tarefa(&r, agora),
                "numero" => numero(&r, agora),
                _ => prompt(&r, agora),
            };
            let (sql, ps) = match mapa {
                Ok(x) => x,
                Err(e) => {
                    tracing::warn!("{tabela} {ext} ignorado: {e}");
                    rel.invalida(tabela);
                    continue;
                }
            };
            let novo = store
                .tx(|c| {
                    if c.execute("INSERT OR IGNORE INTO imported(kind, external_id) VALUES (?,?)", params![tabela, ext])? == 0 {
                        return Ok(false);
                    }
                    c.execute(sql, orion_store::rusqlite::params_from_iter(ps.iter()))?;
                    let rid = c.last_insert_rowid();
                    c.execute("UPDATE imported SET ref=? WHERE kind=? AND external_id=?", params![rid.to_string(), tabela, ext])?;
                    Ok(true)
                })
                .map_err(|e| e.to_string())?;
            let alvo = if novo { &mut rel.operacionais } else { &mut rel.operacionais_repetidos };
            *alvo.entry(tabela.into()).or_default() += 1;
        }
    }
    Ok(())
}

/// "evento:⟨id⟩" → "message:<id interno>"; "topico:⟨nome⟩" → "topic:<nome>".
fn no(r: &Value, mensagens: &HashMap<String, String>) -> Option<String> {
    let s = r.as_str().map(String::from).unwrap_or_else(|| r.to_string());
    let tabela = s.split(':').next().unwrap_or("").to_string();
    let ident = id_limpo(r);
    if ident.is_empty() {
        return None;
    }
    match tabela.as_str() {
        "evento" => mensagens.get(&ident).map(|m| format!("message:{m}")),
        "topico" => Some(format!("topic:{ident}")),
        _ => None,
    }
}

fn importar_arestas(store: &Store, pasta: &Path, rel: &mut ImportReport) -> Result<(), String> {
    let mensagens: HashMap<String, String> = store
        .query("SELECT external_id, ref FROM imported WHERE kind='mensagem'", &[])
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|r| Some((r["external_id"].as_str()?.to_string(), r["ref"].as_str()?.to_string())))
        .collect();
    let agora = store.now();
    for relacao in RELACOES {
        let mut linhas: Vec<(String, String, f64, String)> = vec![];
        for r in ler(pasta, relacao)?.unwrap_or_default() {
            let (origem, destino) = (no(g(&r, "in"), &mensagens), no(g(&r, "out"), &mensagens));
            let (Some(o), Some(d)) = (origem, destino) else {
                rel.arestas_sem_no += 1;
                continue;
            };
            let peso = match g(&r, "peso") {
                Value::Null => 1.0,
                v => v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(1.0),
            };
            linhas.push((o, d, peso, texto(g(&r, "tipo"))));
        }
        if linhas.is_empty() {
            continue;
        }
        let total = linhas.len();
        let novas = store
            .tx(|c| {
                let mut n = 0;
                for (o, d, p, k) in &linhas {
                    n += c.execute("INSERT OR IGNORE INTO edges(src, rel, dst, weight, kind, created_at) VALUES (?,?,?,?,?,?)", params![o, relacao, d, p, k, agora])?;
                }
                Ok(n)
            })
            .map_err(|e| e.to_string())?;
        rel.arestas += novas;
        rel.arestas_repetidas += total - novas;
    }
    Ok(())
}

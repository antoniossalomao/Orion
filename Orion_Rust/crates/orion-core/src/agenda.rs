//! Agenda do Google no briefing matinal (REGRAS.md: agenda do briefing é leitura fixa, sem
//! modelo).
//!
//! Sem modelo; uma ferramenta fixa com argumentos montados aqui; só vale se o `mcp.json` a
//! classifica como `read`; o resultado vai só para o aviso, sem URL nem ID; cada consulta vai
//! para o audit.

use orion_policy::{Risk, ToolSpec};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

pub const MAX_LINHAS: usize = 10;
pub const MAX_LINHA: usize = 160;
pub const MAX_TOTAL: usize = 1500;
static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://\S+").unwrap());
static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(?:ID|Id|id)\s*:\s*\S+").unwrap());
static CONTROLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\x00-\x08\x0b-\x1f\x7f]").unwrap());
static SOBRA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*[|·]\s*([|·]|$)").unwrap());
static ESPACOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Linhas do que a agenda devolveu, sem endereço, sem ID e sem caractere de controle.
pub fn limpar(texto: &str) -> Vec<String> {
    texto
        .lines()
        .filter_map(|bruta| {
            let ln = CONTROLE.replace_all(bruta, "");
            let ln = ID.replace_all(&URL.replace_all(&ln, ""), "").into_owned();
            // `(?=[|·]|$)` do Python: tira o separador sobrando, mantém o que vem depois
            let mut ln2 = ln.clone();
            loop {
                let novo = SOBRA.replace_all(&ln2, "$1").into_owned();
                if novo == ln2 {
                    break;
                }
                ln2 = novo;
            }
            let ln = ESPACOS.replace_all(&ln2, " ").trim_matches([' ', '-', '|', '·', '\t']).to_string();
            (!ln.is_empty()).then(|| ln.chars().take(MAX_LINHA).collect())
        })
        .collect()
}

pub fn formatar(texto: &str) -> String {
    let linhas = limpar(texto);
    let mut corpo: Vec<String> = linhas.iter().take(MAX_LINHAS).cloned().collect();
    if linhas.len() > MAX_LINHAS {
        corpo.push(format!("… e mais {} linhas", linhas.len() - MAX_LINHAS));
    }
    corpo.iter().map(|l| format!("• {l}")).collect::<Vec<_>>().join("\n").chars().take(MAX_TOTAL).collect()
}

pub type Chamada = Arc<dyn Fn(&str, Map<String, Value>) -> anyhow::Result<Value> + Send + Sync>;
pub type Audit = Arc<dyn Fn(&Value) -> anyhow::Result<()> + Send + Sync>;

pub struct AgendaDoDia {
    call: Chamada,
    tool: String,
    email: String,
    audit: Option<Audit>,
    cal: String,
}

impl AgendaDoDia {
    pub fn consultar(&self, agora: f64) -> Option<String> {
        let dt = crate::util::local(agora);
        let inicio = dt.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let fim = inicio + chrono::Duration::days(1);
        let tz = |n: chrono::NaiveDateTime| {
            chrono::TimeZone::from_local_datetime(&chrono::Local, &n).earliest().map(|d| d.format("%Y-%m-%dT%H:%M:%S%:z").to_string()).unwrap_or_default()
        };
        let mut args = Map::new();
        args.insert("user_google_email".into(), json!(self.email));
        args.insert("calendar_id".into(), json!(self.cal));
        args.insert("time_min".into(), json!(tz(inicio)));
        args.insert("time_max".into(), json!(tz(fim)));
        let res = match (self.call)(&self.tool, args) {
            Ok(r) => r,
            Err(e) => return Some(self.falha(&e.to_string().split(':').next().unwrap_or("Exception").to_string())),
        };
        if res.get("erro").is_some() {
            return Some(self.falha("erro do servidor MCP"));
        }
        let texto = formatar(res.get("texto").and_then(Value::as_str).unwrap_or(""));
        self.registrar("ok");
        (!texto.is_empty()).then_some(texto)
    }

    fn falha(&self, motivo: &str) -> String {
        tracing::warn!("agenda do briefing falhou: {motivo}");
        self.registrar(&format!("falhou: {motivo}"));
        format!("agenda indisponível ({motivo})")
    }

    fn registrar(&self, motivo: &str) {
        if let Some(a) = &self.audit {
            let e = json!({"tool": "briefing_agenda", "action": "allow", "risk": Risk::Read.as_str(),
                           "reason": format!("briefing sem supervisão: {} ({motivo})", self.tool)});
            if let Err(err) = a(&e) {
                tracing::error!("audit da agenda do briefing falhou: {err}");
            }
        }
    }
}

/// A agenda do briefing, ou `None` (com o motivo no log) se não está pronta ou não é segura.
pub fn agenda_do_briefing(tool: &str, email: &str, specs: &HashMap<String, ToolSpec>, call: Chamada, audit: Option<Audit>) -> Option<AgendaDoDia> {
    if email.trim().is_empty() {
        return None;
    }
    let Some(spec) = specs.get(tool) else {
        tracing::warn!("agenda do briefing: a ferramenta {tool} não está no mcp.json (ou o servidor caiu)");
        return None;
    };
    if spec.risk != Some(Risk::Read) {
        tracing::warn!("agenda do briefing recusada: {tool} é '{}' no mcp.json; sem supervisão só roda leitura", spec.risk());
        return None;
    }
    Some(AgendaDoDia { call, tool: tool.into(), email: email.trim().into(), audit, cal: "primary".into() })
}

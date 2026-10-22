//! Painel único: o estado do Orion numa tela só (e num comando do Telegram).
//!
//! Só leitura e **sem segredo** (REGRAS.md: painel só lê e nunca mostra segredo): nome de
//! endpoint, modelo, contagens e o tipo do último erro; aprovações e decisões sem argumentos.

use crate::agent::Agent;
use crate::modos::Modos;
use chrono::Duration;
use orion_ai::costs::Custos;
use orion_ai::delegate::Delegator;
use orion_policy::PolicyEngine;
use orion_store::ops::Operations;
use orion_store::Store;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const JANELA_H: i64 = 24;
pub const RECENTES: usize = 8;

pub type Fonte = Arc<dyn Fn() -> Value + Send + Sync>;

pub struct Painel {
    pub started_at: f64,
    pub memory: Arc<Store>,
    pub ops: Arc<Operations>,
    pub policy: Arc<PolicyEngine>,
    pub agent: Option<Arc<Agent>>,
    pub delegator: Option<Arc<Delegator>>,
    pub modos: Option<Arc<Modos>>,
    pub custos: Option<Arc<Custos>>,
    /// Estado dos jobs (`ativo`, `ultima_rodada`, `erros`).
    pub jobs: Option<Fonte>,
    pub mcp: Option<Fonte>,
    pub telegram_ativo: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    pub tela: Option<Fonte>,
    pub voz: Option<Fonte>,
}

impl Painel {
    pub fn montar(&self) -> Value {
        let agora = crate::util::agora();
        let endpoints = self.agent.as_ref().map(|a| a.gateway.stats()).unwrap_or_default();
        let pendentes = self.policy.approvals.pending(None, false);
        let jobs = self.jobs.as_ref().map(|f| f()).unwrap_or(json!({"ativo": false, "ultima_rodada": null, "erros": []}));
        json!({
            "gerado_em": agora,
            "uptime_s": (agora - self.started_at).round() as i64,
            "modelos": {"configurado": self.agent.is_some(), "endpoints": endpoints},
            "semana": self.semana(agora),
            "roteamento": {
                "ativo": self.agent.as_ref().is_some_and(|a| a.routing()),
                "contagem": self.agent.as_ref().map(|a| json!(a.rotas.lock().clone())).unwrap_or(json!({})),
            },
            "clis": self.delegator.as_ref().map(|d| d.status()).unwrap_or_default(),
            "aprovacoes": {
                "pendentes": pendentes.len(),
                "itens": pendentes.iter().take(10).map(|a| json!({
                    "id": a.id, "ferramenta": a.tool, "motivo": a.reason.chars().take(200).collect::<String>(),
                    "idade_s": (agora - a.created_at).max(0.0).round() as i64,
                    "expira_em_s": (a.expires_at - agora).max(0.0).round() as i64,
                })).collect::<Vec<_>>(),
            },
            "decisoes": self.decisoes(agora),
            "avisos": {"pendentes": self.ops.pending_notifications(1000, true).map(|v| v.len()).unwrap_or(0)},
            "jobs": jobs,
            "memoria": {"ok": self.memory.ping(), "vetores": self.memory.vectors_available},
            "canais": {"telegram": self.telegram_ativo.as_ref().is_some_and(|f| f())},
            "tela": self.tela.as_ref().map(|f| f()),
            "voz": self.voz.as_ref().map(|f| f()),
            "ferramentas": self.agent.as_ref().map(|a| a.tools.read().names().len()).unwrap_or(0),
            "mcp": self.mcp.as_ref().map(|f| f()).unwrap_or(json!({})),
            "modos": self.modos.as_ref().map(|m| m.estado()),
            "cota": self.custos.as_ref().map(|c| c.resumo()).unwrap_or_default(),
            // o que o Orion contou por provedor na semana (REGRAS.md: toda saída de dado fica registrada)
            "provedores": self.memory.external_calls_summary(agora - 7.0 * 86400.0).unwrap_or_default(),
        })
    }

    /// Respostas dos últimos 7 dias por endpoint (contadas pelo agente, persistem no banco).
    fn semana(&self, agora: f64) -> Vec<Value> {
        let usos = self.memory.counters_with_prefix("uso:").unwrap_or_default();
        let hoje = crate::util::local(agora);
        (0..7)
            .rev()
            .map(|i| {
                let d = (hoje - Duration::days(i)).format("%Y%m%d").to_string();
                let mut por = Map::new();
                let mut erros = 0;
                for (chave, n) in &usos {
                    let partes: Vec<&str> = chave.splitn(3, ':').collect();
                    if partes.len() != 3 || partes[1] != d {
                        continue;
                    }
                    if partes[2] == "__erro" {
                        erros += n;
                    } else {
                        por.insert(partes[2].to_string(), json!(n));
                    }
                }
                let total: i64 = por.values().filter_map(Value::as_i64).sum();
                json!({"dia": d, "total": total, "erros": erros, "por_endpoint": por})
            })
            .collect()
    }

    fn decisoes(&self, agora: f64) -> Value {
        let linhas = self.ops.audit_recent(500, None, None, Some(agora - (JANELA_H * 3600) as f64)).unwrap_or_default();
        let mut por_acao: BTreeMap<String, i64> = BTreeMap::new();
        let mut por_ferramenta: Vec<(String, i64)> = vec![];
        for r in &linhas {
            *por_acao.entry(r["action"].as_str().unwrap_or("").into()).or_insert(0) += 1;
            let t = r["tool"].as_str().unwrap_or("").to_string();
            match por_ferramenta.iter_mut().find(|(n, _)| *n == t) {
                Some(e) => e.1 += 1,
                None => por_ferramenta.push((t, 1)),
            }
        }
        // `Counter.most_common`: mais usadas primeiro; empate fica na ordem em que apareceu
        por_ferramenta.sort_by(|a, b| b.1.cmp(&a.1));
        json!({
            "janela_h": JANELA_H,
            "total": linhas.len(),
            "truncado": linhas.len() >= 500,
            "por_acao": {"allow": por_acao.get("allow").unwrap_or(&0), "confirm": por_acao.get("confirm").unwrap_or(&0), "deny": por_acao.get("deny").unwrap_or(&0)},
            "mais_usadas": por_ferramenta.iter().take(5).map(|(t, n)| json!({"ferramenta": t, "n": n})).collect::<Vec<_>>(),
            "recentes": linhas.iter().take(RECENTES).map(|r| json!({
                "ts": r["ts"], "ferramenta": r["tool"], "acao": r["action"], "risco": r["risk"],
                "motivo": r["reason"].as_str().unwrap_or("").chars().take(160).collect::<String>(),
            })).collect::<Vec<_>>(),
        })
    }
}

fn dur(segundos: i64) -> String {
    let s = segundos;
    if s < 90 {
        format!("{s}s")
    } else if s < 5400 {
        format!("{} min", s / 60)
    } else if s < 172_800 {
        format!("{} h", s / 3600)
    } else {
        format!("{} d", s / 86400)
    }
}

fn i(v: &Value) -> i64 {
    v.as_i64().unwrap_or(0)
}

fn s(v: &Value) -> String {
    match v {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        o => o.to_string(),
    }
}

fn verdade(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(x) => !x.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// O painel em texto puro (Telegram): uma seção por assunto, só o que importa.
pub fn texto_do_painel(p: &Value) -> String {
    let mut l = vec![format!("📊 Painel do Orion · no ar há {}", dur(i(&p["uptime_s"])))];
    let modos = &p["modos"];
    if verdade(&modos["panico"]) {
        l.push("🚨 MODO PÂNICO: rede, execução, tela e escuta cortadas (/panico sair)".into());
    }
    if verdade(&modos["nao_perturbe"]) {
        l.push("🌙 Não perturbe: avisos não urgentes esperando na fila".into());
    }
    let m = &p["modelos"];
    if !verdade(&m["configurado"]) {
        l.push("\n🧠 Modelos: gateway não configurado.".into());
    } else {
        l.push("\n🧠 Modelos (desde que o Orion subiu)".into());
        for e in m["endpoints"].as_array().cloned().unwrap_or_default() {
            let camada = if s(&e["camada"]) != "padrão" && !s(&e["camada"]).is_empty() { format!(", camada {}", s(&e["camada"])) } else { String::new() };
            let pausa = if s(&e["motivo"]) == "falhas" { "pausa (falhas seguidas)" } else { "quarentena" };
            let estado = if i(&e["quarentena_s"]) > 0 { format!("⛔ {pausa} {}", dur(i(&e["quarentena_s"]))) } else { "ok".into() };
            let extra = if i(&e["limitada"]) > 0 { format!(", {}× cota", i(&e["limitada"])) } else { String::new() };
            let erro = if verdade(&e["ultimo_erro"]) { format!(" · último erro: {}", s(&e["ultimo_erro"])) } else { String::new() };
            l.push(format!("• {} ({}{camada}): {} ok, {} falha(s){extra} · {estado}{erro}", s(&e["nome"]), s(&e["modelo"]), i(&e["ok"]), i(&e["falhas"])));
            if i(&e["orcamento"]) > 0 {
                l.push(format!("   limite do dia gasto: {}× para o fim da fila", i(&e["orcamento"])));
            }
        }
    }
    let semana = p["semana"].as_array().cloned().unwrap_or_default();
    if semana.iter().any(|d| i(&d["total"]) > 0 || i(&d["erros"]) > 0) {
        let total: i64 = semana.iter().map(|d| i(&d["total"])).sum();
        let erros: i64 = semana.iter().map(|d| i(&d["erros"])).sum();
        l.push(format!("\n📈 Últimos 7 dias: {total} resposta(s), {erros} falha(s) sem resposta"));
        l.push(format!(
            "• por dia: {}",
            semana.iter().map(|d| { let x = s(&d["dia"]); format!("{}/{} {}", &x[6..], &x[4..6], i(&d["total"])) }).collect::<Vec<_>>().join(" · ")
        ));
    }
    let cota: Vec<Value> = p["cota"].as_array().cloned().unwrap_or_default().into_iter().filter(|c| verdade(&c["usado"]) || s(&c["estado"]) != "ok").collect();
    if !cota.is_empty() {
        l.push("\n🎟️ Cota gratuita (o que o Orion contou)".into());
        for c in cota {
            let limite = if i(&c["limite"]) > 0 { format!("/{}", i(&c["limite"])) } else { String::new() };
            let quando = if s(&c["periodo"]) == "dia" { "hoje" } else { "no mês" };
            let alerta = if matches!(s(&c["estado"]).as_str(), "alta" | "estourada") { " ⚠️" } else { "" };
            l.push(format!("• {}: {}{limite} {quando}{alerta}", s(&c["nome"]), i(&c["usado"])));
        }
    }
    let clis = p["clis"].as_array().cloned().unwrap_or_default();
    if !clis.is_empty() {
        l.push("\n💻 CLIs oficiais (hoje)".into());
        for c in clis {
            if !verdade(&c["instalada"]) {
                l.push(format!("• {}: não instalada", s(&c["nome"])));
            } else {
                l.push(format!("• {}: {}/{} usadas", s(&c["nome"]), i(&c["usadas_hoje"]), i(&c["limite_diario"])));
            }
        }
    }
    let r = &p["roteamento"];
    if verdade(&r["ativo"]) {
        let c = &r["contagem"];
        l.push(format!("• roteamento: {} rápidas, {} pesadas, {} com imagem", i(&c["rapido"]), i(&c["pesado"]), i(&c["visao"])));
    }
    let a = &p["aprovacoes"];
    l.push(format!("\n✋ Aprovações pendentes: {}", i(&a["pendentes"])));
    for it in a["itens"].as_array().cloned().unwrap_or_default().iter().take(5) {
        l.push(format!("• {} (expira em {})", s(&it["ferramenta"]), dur(i(&it["expira_em_s"]))));
    }
    let d = &p["decisoes"];
    let ac = &d["por_acao"];
    l.push(format!(
        "\n🛡️ Política, últimas {} h: {} decisões ({} liberadas, {} pediram aval, {} negadas)",
        i(&d["janela_h"]),
        i(&d["total"]),
        i(&ac["allow"]),
        i(&ac["confirm"]),
        i(&ac["deny"])
    ));
    let mais = d["mais_usadas"].as_array().cloned().unwrap_or_default();
    if !mais.is_empty() {
        l.push(format!("• mais usadas: {}", mais.iter().map(|x| format!("{} ×{}", s(&x["ferramenta"]), i(&x["n"]))).collect::<Vec<_>>().join(", ")));
    }
    let j = &p["jobs"];
    if !verdade(&j["ativo"]) {
        l.push("\n⏱️ Jobs: desligados".into());
    } else {
        let erros: Vec<String> = j["erros"].as_array().cloned().unwrap_or_default().iter().map(s).collect();
        l.push(format!("\n⏱️ Jobs: {}", if erros.is_empty() { "ok".to_string() } else { format!("com erro — {}", erros.join("; ")) }));
    }
    l.push(format!(
        "🔔 Avisos na fila: {} · 🧰 Ferramentas: {} · 💾 Memória: {}",
        i(&p["avisos"]["pendentes"]),
        i(&p["ferramentas"]),
        if verdade(&p["memoria"]["ok"]) { "ok" } else { "ERRO" }
    ));
    let v = &p["voz"];
    if verdade(&v["escuta"]["pedida"]) {
        let e = &v["escuta"];
        let estado = if !verdade(&e["ouvindo"]) { "não subiu" } else if verdade(&e["pausada"]) { "pausada" } else { "ouvindo" };
        let motivo = if verdade(&e["ultimo_erro"]) { format!(" ({})", s(&e["ultimo_erro"])) } else { String::new() };
        l.push(format!("🎧 Palavra de ativação: {estado}, {} ativação(ões){motivo}", i(&e["ativacoes"])));
    }
    if v.is_object() && (verdade(&v["clique"]["ligada"]) || verdade(&v["ao_vivo"]["ligada"])) {
        let mut partes = vec![];
        if verdade(&v["clique"]["ligada"]) {
            let fala = if verdade(&v["clique"]["fala"]) { "com fala" } else { "só texto" };
            partes.push(format!("clique {} turno(s), {fala}", i(&v["clique"]["turnos"])));
        }
        if verdade(&v["ao_vivo"]["ligada"]) {
            partes.push(format!("ao vivo {} sessão(ões), {} min", i(&v["ao_vivo"]["sessoes"]), s(&v["ao_vivo"]["minutos"])));
        }
        let erro = if i(&v["falhas"]) > 0 { format!(" · falhas: {} (última: {})", i(&v["falhas"]), s(&v["ultimo_erro"])) } else { String::new() };
        l.push(format!("🎙️ Voz: {}{erro}", partes.join("; ")));
    }
    let t = &p["tela"];
    if verdade(&t["ligada"]) {
        let estado = if verdade(&t["pausada"]) { "pausada" } else { "capturando" };
        l.push(format!("🖥️ Memória da tela: {estado}, {} registro(s) guardado(s), {} nesta sessão (só texto, OCR local)", i(&t["registros"]), i(&t["gravadas"])));
    }
    if let Some(mcp) = p["mcp"].as_object().filter(|o| !o.is_empty()) {
        l.push(format!("🔌 MCP: {}", mcp.iter().map(|(k, v)| format!("{k} {}", s(v))).collect::<Vec<_>>().join(", ")));
    }
    l.join("\n")
}

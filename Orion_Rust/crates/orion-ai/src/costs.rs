//! Custo zero e cota gratuita (REGRAS.md: custo zero (sem API paga)).
//!
//! Conta quanto da cota gratuita **o próprio Orion** já usou, a partir do registro de saída. Job
//! opcional não roda com 90% usado (e o aviso sai uma vez por dia); o chat nunca é bloqueado.

use crate::provedores::{ProvedorConf, CATALOGO};
use chrono::{Datelike, Local, TimeZone};
use orion_store::ops::Operations;
use orion_store::Store;
use serde_json::{json, Value};
use std::sync::Arc;

pub const LIMIAR_OPCIONAL: f64 = 0.9;
/// A partir daqui sai um aviso de cota alta, uma vez por período (F13.05).
pub const LIMIAR_ALERTA: f64 = 0.8;

/// Hosts que contam como gratuitos (local é gratuito por definição).
pub fn provedores_gratuitos() -> Vec<String> {
    let mut v: Vec<String> = ["127.0.0.1", "localhost", "::1", "api.groq.com", "generativelanguage.googleapis.com", "api.search.brave.com"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for (_, c) in CATALOGO {
        if let Some(h) = url::Url::parse(c.base_url).ok().and_then(|u| u.host_str().map(String::from)) {
            v.push(h);
        }
    }
    v
}

pub fn host_gratuito(u: &str) -> bool {
    let host = url::Url::parse(u)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.trim_matches(['[', ']']).to_lowercase()))
        .unwrap_or_default();
    provedores_gratuitos().contains(&host)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cota {
    /// Prefixo no registro de saída: "gateway:" pega todas as camadas.
    pub provedor: String,
    pub nome: String,
    /// 0 = sem limite.
    pub limite: i64,
    /// "dia" ou "mes".
    pub periodo: &'static str,
}

pub struct Quotas {
    pub gateway_dia: i64,
    pub groq_dia: i64,
    pub gemini_dia: i64,
    pub brave_mes: i64,
}

pub fn cotas_de(q: &Quotas, provedores: &[ProvedorConf]) -> Vec<(String, Cota)> {
    let c = |p: &str, n: &str, l: i64, per: &'static str| Cota { provedor: p.into(), nome: n.into(), limite: l, periodo: per };
    let mut v = vec![
        ("gateway".to_string(), c("gateway:", "Gateway de modelos", q.gateway_dia, "dia")),
        ("groq".to_string(), c("groq", "Groq (transcrição)", q.groq_dia, "dia")),
        ("gemini".to_string(), c("gemini", "Gemini (busca, embeddings, imagem)", q.gemini_dia, "dia")),
        ("brave".to_string(), c("brave", "Brave Search", q.brave_mes, "mes")),
    ];
    for p in provedores.iter().filter(|p| p.limite_dia > 0) {
        v.push((format!("prov:{}", p.id), c(&format!("gateway:{}", p.id), &format!("Modelos · {}", p.id), p.limite_dia as i64, "dia")));
    }
    v
}

pub struct Custos {
    pub memory: Arc<Store>,
    pub cotas: parking_lot::RwLock<Vec<(String, Cota)>>,
    ops: Option<Arc<Operations>>,
}

impl Custos {
    pub fn new(memory: Arc<Store>, cotas: Vec<(String, Cota)>, ops: Option<Arc<Operations>>) -> Self {
        Custos { memory, cotas: parking_lot::RwLock::new(cotas), ops }
    }

    /// Atualiza os limites sem apagar o uso registrado (REGRAS.md: custo zero (sem API paga)).
    pub fn configure(&self, quotas: Vec<(String, Cota)>) {
        *self.cotas.write() = quotas;
    }

    fn cota(&self, k: &str) -> Option<Cota> {
        self.cotas.read().iter().find(|(c, _)| c == k).map(|(_, c)| c.clone())
    }

    fn inicio(&self, periodo: &str) -> f64 {
        let dt = chrono::DateTime::from_timestamp(self.memory.now() as i64, 0).unwrap_or_default().with_timezone(&Local);
        let dia = if periodo == "mes" { dt.date_naive().with_day(1).unwrap_or(dt.date_naive()) } else { dt.date_naive() };
        Local
            .from_local_datetime(&dia.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|d| d.timestamp() as f64)
            .unwrap_or(0.0)
    }

    /// Chamadas no período corrente (hoje, ou este mês para o Brave).
    pub fn uso(&self, provedor: &str) -> i64 {
        let Some(c) = self.cota(provedor) else { return 0 };
        self.memory.external_calls_count(&c.provedor, self.inicio(c.periodo))
    }

    pub fn fracao(&self, provedor: &str) -> f64 {
        match self.cota(provedor) {
            Some(c) if c.limite > 0 => self.uso(provedor) as f64 / c.limite as f64,
            _ => 0.0,
        }
    }

    /// Gateway: o provedor ainda está dentro do `limite_dia` hoje? Sem limite, sempre.
    pub fn no_orcamento(&self, provedor_id: &str) -> bool {
        let chave = format!("prov:{provedor_id}");
        match self.cota(&chave) {
            None => true,
            Some(c) => self.uso(&chave) < c.limite,
        }
    }

    /// O chat (`opcional=false`) sempre pode. Job opcional para com 90% e avisa uma vez por dia.
    pub fn pode_usar(&self, provedor: &str, opcional: bool, quem: &str) -> bool {
        match self.cota(provedor) {
            Some(c) if opcional && c.limite > 0 => {
                if self.fracao(provedor) < LIMIAR_OPCIONAL {
                    return true;
                }
                self.avisar(provedor, quem);
                false
            }
            _ => true,
        }
    }

    fn avisar(&self, provedor: &str, quem: &str) {
        let Some(ops) = &self.ops else { return };
        let dia = chrono::DateTime::from_timestamp(self.memory.now() as i64, 0)
            .unwrap_or_default()
            .with_timezone(&Local)
            .format("%Y%m%d")
            .to_string();
        let chave = format!("cota:aviso:{provedor}:{dia}");
        // o aviso de limite de `alertas` já disse o mesmo neste período: um aviso só
        if self.memory.counter_get(&chave) != 0 || self.memory.counter_get(&self.marca_alerta("limite", provedor)) != 0 {
            return;
        }
        let _ = self.memory.counter_set(&chave, 1);
        let Some(c) = self.cota(provedor) else { return };
        let periodo = if c.periodo == "dia" { "hoje" } else { "neste mês" };
        let oque = if quem.is_empty() { "os jobs opcionais param".to_string() } else { format!("{quem} ficou para depois") };
        let _ = ops.notify(
            "cota",
            &format!(
                "Cota gratuita de {} quase no fim ({}/{} {periodo}): {oque}. O chat continua.",
                c.nome,
                self.uso(provedor),
                c.limite
            ),
            Some(&format!("cota:{provedor}:{dia}")),
            false,
        );
        tracing::warn!("cota de {provedor} em {:.0}%: job opcional adiado", self.fracao(provedor) * 100.0);
    }

    fn marca_alerta(&self, nivel: &str, chave: &str) -> String {
        let dt = chrono::DateTime::from_timestamp(self.memory.now() as i64, 0).unwrap_or_default().with_timezone(&Local);
        let periodo = match self.cota(chave) {
            Some(c) if c.periodo == "mes" => dt.format("%Y%m").to_string(),
            _ => dt.format("%Y%m%d").to_string(),
        };
        format!("cota:alerta:{nivel}:{chave}:{periodo}")
    }

    /// Avisa uma vez por período quando uma cota passa de 80% e outra quando chega ao limite
    /// (F13.05). Só avisa: quem decide se o chat segue é o gateway, e o chat nunca é bloqueado.
    /// Devolve quantos avisos novos saíram.
    pub fn alertas(&self) -> usize {
        let Some(ops) = &self.ops else { return 0 };
        let quotas = self.cotas.read().clone();
        let mut n = 0;
        for (chave, c) in quotas.iter().filter(|(_, c)| c.limite > 0) {
            let usado = self.uso(chave);
            let frac = usado as f64 / c.limite as f64;
            let (nivel, texto) = if usado >= c.limite {
                ("limite", format!("Cota gratuita de {} no limite ({usado}/{}): o que for opcional para até virar o período.", c.nome, c.limite))
            } else if frac >= LIMIAR_ALERTA {
                ("80", format!("Cota gratuita de {} em {:.0}% ({usado}/{}).", c.nome, frac * 100.0, c.limite))
            } else {
                continue;
            };
            let marca = self.marca_alerta(nivel, chave);
            let dia = chrono::DateTime::from_timestamp(self.memory.now() as i64, 0).unwrap_or_default().with_timezone(&Local).format("%Y%m%d");
            // o job opcional adiado já avisou a cota alta hoje: não repete
            if self.memory.counter_get(&marca) != 0 || self.memory.counter_get(&format!("cota:aviso:{chave}:{dia}")) != 0 {
                continue;
            }
            let _ = self.memory.counter_set(&marca, 1);
            if ops.notify("cota", &texto, Some(&marca), false).is_ok() {
                n += 1;
            }
        }
        n
    }

    /// Para o card "Cota de hoje": usado, limite e o estado de cada provedor.
    pub fn resumo(&self) -> Vec<Value> {
        let quotas = self.cotas.read().clone();
        quotas
            .iter()
            .map(|(chave, c)| {
                let usado = self.uso(chave);
                let pct = if c.limite > 0 { (100.0 * usado as f64 / c.limite as f64).round() as i64 } else { 0 };
                let estado = if c.limite == 0 {
                    "sem_limite"
                } else if usado >= c.limite {
                    "estourada"
                } else if pct as f64 >= LIMIAR_OPCIONAL * 100.0 {
                    "alta"
                } else {
                    "ok"
                };
                json!({"provedor": chave, "nome": c.nome, "usado": usado, "limite": c.limite,
                       "periodo": c.periodo, "pct": pct, "estado": estado})
            })
            .collect()
    }
}

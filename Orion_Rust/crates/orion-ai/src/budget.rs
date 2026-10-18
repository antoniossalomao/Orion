//! Estimativa local em chamadas (REGRAS.md: custo zero (sem API paga)). Nunca representa uso oficial.
use orion_store::Store;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

const WEEK: i64 = 7 * 86400;
#[derive(Clone, Serialize, Deserialize)]
pub struct Quota {
    pub weekly_calls: i64,
    /// Referência UTC de um reset semanal (passado ou futuro).
    pub reset_at: i64,
    pub five_hour_calls: Option<i64>,
}
pub struct Budget {
    store: Arc<Store>,
    quotas: RwLock<BTreeMap<String, Quota>>,
}
impl Budget {
    pub fn new(store: Arc<Store>, quotas: BTreeMap<String, Quota>) -> anyhow::Result<Self> {
        Self::validate(&quotas)?;
        store.conn().execute_batch("CREATE TABLE IF NOT EXISTS usage_budget(provider TEXT NOT NULL, task TEXT NOT NULL, at INTEGER NOT NULL, amount INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS usage_budget_at ON usage_budget(provider, at);")?;
        Ok(Self { store, quotas: RwLock::new(quotas) })
    }
    pub fn load(store: Arc<Store>) -> anyhow::Result<Self> {
        let raw = store.meta_get("budget:quotas").or_else(|| std::env::var("ORION_WEEKLY_BUDGETS").ok()).unwrap_or_else(|| "{}".into());
        Self::new(store, serde_json::from_str(&raw)?)
    }
    fn validate(quotas: &BTreeMap<String, Quota>) -> anyhow::Result<()> {
        anyhow::ensure!(quotas.len() <= 64, "máximo de 64 provedores");
        for (provider, q) in quotas {
            anyhow::ensure!(
                !provider.is_empty()
                    && provider.len() <= 80
                    && q.weekly_calls >= 5
                    && q.weekly_calls <= 1_000_000_000
                    && q.five_hour_calls.is_none_or(|v| v > 0),
                "cota inválida"
            );
            anyhow::ensure!(q.reset_at.abs_diff(0) < 100_000_000_000, "reset inválido");
        }
        Ok(())
    }
    pub fn config(&self) -> BTreeMap<String, Quota> {
        self.quotas.read().clone()
    }
    pub fn configure(&self, quotas: BTreeMap<String, Quota>) -> anyhow::Result<()> {
        Self::validate(&quotas)?;
        let mut current = self.quotas.write();
        self.store.meta_set("budget:quotas", Some(&serde_json::to_string(&quotas)?))?;
        *current = quotas;
        Ok(())
    }
    fn start(q: &Quota, now: i64) -> i64 {
        q.reset_at + (now - q.reset_at).div_euclid(WEEK) * WEEK
    }
    pub fn reserve(&self, provider: &str, task: &str) -> anyhow::Result<()> {
        let quotas = self.quotas.read();
        let Some(q) = quotas.get(provider) else { return Ok(()) };
        let now = self.store.now() as i64;
        let start = Self::start(q, now);
        self.store.tx(|c| {
            let used: i64 =
                c.query_row("SELECT coalesce(sum(amount),0) FROM usage_budget WHERE provider=? AND at>=? AND at<=?", (provider, start, now), |r| r.get(0))?;
            let task_used: i64 = c.query_row(
                "SELECT coalesce(sum(amount),0) FROM usage_budget WHERE provider=? AND task=? AND at>=? AND at<=?",
                (provider, task, start, now),
                |r| r.get(0),
            )?;
            if used + 1 > q.weekly_calls * 95 / 100 {
                return orion_store::invalid("teto semanal de 95% atingido");
            }
            if task_used + 1 > q.weekly_calls / 5 {
                return orion_store::invalid("teto de 20% por tarefa atingido");
            }
            if let Some(limit) = q.five_hour_calls {
                let window: i64 = c.query_row(
                    "SELECT coalesce(sum(amount),0) FROM usage_budget WHERE provider=? AND at>=? AND at<=?",
                    (provider, now - 5 * 3600, now),
                    |r| r.get(0),
                )?;
                if window >= limit {
                    return orion_store::invalid("janela de cinco horas esgotada");
                }
            }
            // Reserva persistida antes da chamada; falha ou cancelamento também consome a tentativa.
            c.execute("INSERT INTO usage_budget(provider,task,at,amount) VALUES (?,?,?,1)", (provider, task, now))?;
            Ok(())
        })?;
        Ok(())
    }
    pub fn calibrate(&self, provider: &str, used: i64) -> anyhow::Result<()> {
        let quotas = self.quotas.read();
        let q = quotas.get(provider).ok_or_else(|| anyhow::anyhow!("provedor sem cota"))?;
        anyhow::ensure!(used >= 0 && used <= q.weekly_calls, "uso inválido");
        let now = self.store.now() as i64;
        let start = Self::start(q, now);
        self.store.tx(|c| {
            let counted: i64 =
                c.query_row("SELECT coalesce(sum(amount),0) FROM usage_budget WHERE provider=? AND at>=? AND at<=?", (provider, start, now), |r| r.get(0))?;
            c.execute("INSERT INTO usage_budget(provider,task,at,amount) VALUES (?,'calibragem',?,?)", (provider, now, used - counted))?;
            Ok(())
        })?;
        Ok(())
    }
    pub fn summary(&self) -> Vec<Value> {
        let now = self.store.now() as i64;
        self.quotas.read().iter().map(|(p,q)| {
            let start = Self::start(q,now);
            let used: i64 = self.store.conn().query_row("SELECT coalesce(sum(amount),0) FROM usage_budget WHERE provider=? AND at>=? AND at<=?", (p, start, now), |r| r.get(0)).unwrap_or(q.weekly_calls);
            let pct = used as f64 / q.weekly_calls as f64 * 100.0;
            let projection = used as f64 * WEEK as f64 / (now-start).max(1) as f64;
            json!({"provider":p,"used":used,"weekly_calls":q.weekly_calls,"percent":pct,"reset_at":start+WEEK,"task_cap":q.weekly_calls/5,"weekly_cap":q.weekly_calls*95/100,"five_hour_calls":q.five_hour_calls,"alert":pct>=80.0,"projected_weekly_calls":projection,"source":"estimativa_local_em_chamadas"})
        }).collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tetos_persistem_e_reset_semanal() {
        use std::sync::atomic::{AtomicI64, Ordering};
        let clock = Arc::new(AtomicI64::new(1000));
        let t = clock.clone();
        let store = Arc::new(Store::open_with(":memory:", None, Arc::new(move || t.load(Ordering::SeqCst) as f64)).unwrap());
        let budget = Budget::new(store, BTreeMap::from([("cli:codex".into(), Quota { weekly_calls: 100, reset_at: 1000, five_hour_calls: None })])).unwrap();
        for _ in 0..20 {
            budget.reserve("cli:codex", "a").unwrap();
        }
        assert!(budget.reserve("cli:codex", "a").is_err());
        for n in 20..95 {
            budget.reserve("cli:codex", &n.to_string()).unwrap();
        }
        assert!(budget.reserve("cli:codex", "novo").is_err());
        clock.store(1000 + WEEK, Ordering::SeqCst);
        budget.reserve("cli:codex", "a").unwrap();
        assert_eq!(budget.summary()[0]["used"], 1);
    }
    #[test]
    fn janela_menor_que_teto_e_calibragem() {
        let store = Arc::new(Store::open(":memory:").unwrap());
        let budget = Budget::new(store, BTreeMap::from([("groq".into(), Quota { weekly_calls: 100, reset_at: 0, five_hour_calls: Some(1) })])).unwrap();
        budget.reserve("groq", "a").unwrap();
        assert!(budget.reserve("groq", "b").is_err());
        budget.calibrate("groq", 80).unwrap();
        assert_eq!(budget.summary()[0]["alert"], true);
    }
    #[test]
    fn reservas_concorrentes_nao_ultrapassam_teto() {
        let store = Arc::new(Store::open(":memory:").unwrap());
        let budget =
            Arc::new(Budget::new(store, BTreeMap::from([("cli:codex".into(), Quota { weekly_calls: 100, reset_at: 0, five_hour_calls: None })])).unwrap());
        let threads: Vec<_> = (0..10)
            .map(|n| {
                let budget = budget.clone();
                std::thread::spawn(move || (0..20).filter(|i| budget.reserve("cli:codex", &format!("{n}:{i}")).is_ok()).count())
            })
            .collect();
        assert_eq!(threads.into_iter().map(|t| t.join().unwrap()).sum::<usize>(), 95);
        assert_eq!(budget.summary()[0]["used"], 95);
    }
}

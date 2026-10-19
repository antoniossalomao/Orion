//! Política de ferramentas do Orion.
//!
//! Porte de `orion/policy/` (Python). Núcleo imutável: muda só com aprovação explícita do
//! Antônio (REGRAS.md: código, persona e política imutáveis para as ferramentas).

pub mod approvals;
pub mod audit;
pub mod classes;
pub mod engine;
pub mod paths;
pub mod pyjson;
pub mod shell;

pub use approvals::{hash_call, Approval, ApprovalStore, Status};
pub use audit::{mask_pii, redact, redact_pii, RateLimiter, MASCARA};
pub use classes::{default_rate_limits, default_tools, Risk, ToolSpec};
pub use engine::{Action, AuditSink, Context, Decision, PolicyEngine, ToolCall};
pub use paths::PathGuard;
pub use shell::{classify_command, ShellVerdict};

/// Relógio injetável (segundos). Os testes usam um relógio de mentira.
pub type Clock = std::sync::Arc<dyn Fn() -> f64 + Send + Sync>;

/// Relógio de parede, em segundos desde a época (equivale a `time.time()`).
pub fn wall_clock() -> Clock {
    std::sync::Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    })
}

/// Relógio monotônico, em segundos desde a primeira chamada (equivale a `time.monotonic()`).
pub fn monotonic_clock() -> Clock {
    let inicio = std::time::Instant::now();
    std::sync::Arc::new(move || inicio.elapsed().as_secs_f64())
}

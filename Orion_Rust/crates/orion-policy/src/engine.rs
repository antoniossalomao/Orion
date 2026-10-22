//! Motor de política: decide allow, confirm ou deny para cada chamada de ferramenta.
//!
//! Ordem: ferramenta registrada → limite de uso → risco efetivo (classe + shell/caminho) →
//! escalada por conteúdo externo lido (escrita, execução e rede com destino escolhido pelo
//! modelo) → aprovação fora de banda → audit. Falha de audit em ação que não é leitura nega a
//! ação (REGRAS.md: falha de auditoria nega a ação).

use crate::approvals::ApprovalStore;
use crate::audit::{redact_pii, RateLimiter, MASCARA};
use crate::classes::{default_rate_limits, default_tools, Risk, ToolSpec};
use crate::paths::{realpath, PathGuard};
use crate::pyjson;
use crate::shell::classify_command;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Allow,
    Confirm,
    Deny,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Allow => "allow",
            Action::Confirm => "confirm",
            Action::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    pub args: Map<String, Value>,
}

impl ToolCall {
    pub fn new(name: impl Into<String>, args: Value) -> Self {
        let args = match args {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        ToolCall { name: name.into(), args }
    }

    pub fn arg_str(&self, k: &str) -> String {
        match self.args.get(k) {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(v) => v.to_string(),
        }
    }
}

/// Estado de uma sessão para a política. `tainted` vira `true` quando a sessão leu conteúdo
/// externo (web, e-mail, documento): escrita e execução seguintes passam a exigir confirmação
/// (REGRAS.md: conteúdo externo é dado, não instrução).
#[derive(Clone, Default)]
pub struct Context {
    pub session_id: String,
    pub tainted: bool,
    pub allowed_tools: Option<HashSet<String>>,
    pub authorities: Vec<String>,
    pub authorized: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    pub project_id: Option<String>,
    pub share_personal: bool,
    pub root: Option<String>,
    pub project_revision: Option<f64>,
}

impl Context {
    pub fn new(session_id: impl Into<String>) -> Self {
        Context { session_id: session_id.into(), ..Default::default() }
    }
}

#[derive(Debug, Clone)]
pub struct Decision {
    pub action: Action,
    pub risk: Option<Risk>,
    pub reason: String,
    pub approval_id: Option<String>,
}

impl Decision {
    fn new(action: Action, risk: Option<Risk>, reason: impl Into<String>) -> Self {
        Decision { action, risk, reason: reason.into(), approval_id: None }
    }
}

/// Destino do audit. Devolve erro se não conseguiu gravar.
pub type AuditSink = Arc<dyn Fn(&Value) -> Result<(), String> + Send + Sync>;

pub struct PolicyEngine {
    pub path_guard: PathGuard,
    pub approvals: Arc<ApprovalStore>,
    tools: RwLock<HashMap<String, ToolSpec>>,
    rate: RateLimiter,
    audit: Option<AuditSink>,
}

/// Ferramentas com caminho que, dentro de projeto, precisa ficar na raiz do projeto.
const CAMINHOS_DE_PROJETO: &[(&str, &str)] = &[
    ("ler_arquivo", "path"),
    ("listar_arquivos", "path"),
    ("escrever_arquivo", "path"),
    ("delegar", "pasta"),
];
/// Ferramentas que já respeitam o isolamento de projeto
/// (REGRAS.md: o que é de um projeto fica no projeto).
const ISOLADAS: &[&str] = &[
    "criar_evento_agenda", "propor_organizacao", "consultar_git", "consultar_git_projeto",
    "mcp_export_search", "mcp_export_facts", "mcp_export_sources", "mcp_export_artifacts",
    "aplicar_organizacao", "propor_evento_agenda", "consultar_agenda", "consultar_disponibilidade",
    "buscar_memoria", "salvar_memoria", "listar_fatos", "esquecer_fato", "editar_fato",
    "pesquisar_internet", "buscar_url", "navegador_ler", "navegador_agir",
];

impl PolicyEngine {
    pub fn new(path_guard: PathGuard) -> Self {
        PolicyEngine {
            path_guard,
            approvals: Arc::new(ApprovalStore::default()),
            tools: RwLock::new(default_tools()),
            rate: RateLimiter::new(default_rate_limits(), crate::monotonic_clock()),
            audit: None,
        }
    }

    pub fn with_approvals(mut self, a: Arc<ApprovalStore>) -> Self {
        self.approvals = a;
        self
    }

    pub fn with_tools(self, tools: HashMap<String, ToolSpec>) -> Self {
        *self.tools.write().unwrap() = tools;
        self
    }

    pub fn with_rate_limits(mut self, limites: HashMap<String, (u32, u64)>) -> Self {
        self.rate = RateLimiter::new(limites, crate::monotonic_clock());
        self
    }

    pub fn with_audit(mut self, audit: AuditSink) -> Self {
        self.audit = Some(audit);
        self
    }

    pub fn spec(&self, name: &str) -> Option<ToolSpec> {
        self.tools.read().unwrap().get(name).cloned()
    }

    pub fn tool_names(&self) -> Vec<String> {
        self.tools.read().unwrap().keys().cloned().collect()
    }

    pub fn evaluate(&self, call: &ToolCall, ctx: &Context) -> Decision {
        self.evaluate_opts(call, ctx, true)
    }

    pub fn evaluate_opts(&self, call: &ToolCall, ctx: &Context, consume_approval: bool) -> Decision {
        let d = self.decide(call, ctx, consume_approval);
        if !self.record(call, ctx, &d) && d.action == Action::Allow && d.risk != Some(Risk::Read) {
            return Decision::new(Action::Deny, d.risk, "audit indisponível (fail-closed)");
        }
        d
    }

    /// Registra uma ferramenta que só existe em tempo de execução (as de servidores MCP).
    /// Não sobrescreve uma já registrada: um servidor não troca a classe de risco de ninguém.
    pub fn register_tool(&self, spec: ToolSpec, rate: Option<(u32, u64)>) -> Result<(), String> {
        let mut tools = self.tools.write().unwrap();
        if tools.contains_key(&spec.name) {
            return Err(format!("ferramenta já registrada: {}", spec.name));
        }
        if let Some(r) = rate {
            self.rate.set_limit(&spec.name, r);
        }
        tools.insert(spec.name.clone(), spec);
        Ok(())
    }

    /// Remove uma ferramenta registrada em tempo de execução (servidor MCP desligado).
    pub fn unregister_tool(&self, name: &str) {
        self.tools.write().unwrap().remove(name);
    }

    /// O orquestrador chama depois de executar: ferramenta que devolve conteúdo externo
    /// contamina a sessão.
    pub fn note_result(&self, call: &ToolCall, ctx: &mut Context) {
        if self.spec(&call.name).is_some_and(|s| s.external) {
            ctx.tainted = true;
        }
    }

    fn decide(&self, call: &ToolCall, ctx: &Context, consume_approval: bool) -> Decision {
        let Some(spec) = self.spec(&call.name) else {
            return Decision::new(Action::Deny, None, format!("ferramenta '{}' não registrada", call.name));
        };
        let Some(risk) = spec.risk else {
            // REGRAS.md: ferramenta sem classe é negada.
            return Decision::new(Action::Deny, None, format!("ferramenta '{}' sem classe de risco", call.name));
        };
        let r = Some(risk);
        if ctx.authorized.as_ref().is_some_and(|f| !f()) {
            return Decision::new(Action::Deny, r, "concessão ou revisão revogada");
        }
        if ctx.allowed_tools.as_ref().is_some_and(|a| !a.contains(&call.name)) {
            return Decision::new(Action::Deny, r, "ferramenta fora do escopo da skill");
        }
        let scope = match &ctx.project_id {
            Some(p) => format!("project:{p}"),
            None => "personal".into(),
        };
        if spec.scope.as_ref().is_some_and(|s| *s != scope) {
            return Decision::new(Action::Deny, r, "ferramenta de outro escopo");
        }
        if ctx.project_id.is_some() && spec.scope.is_none() {
            if let Some((_, arg)) = CAMINHOS_DE_PROJETO.iter().find(|(n, _)| *n == call.name) {
                let caminho = call.arg_str(arg);
                let p = Path::new(&caminho);
                let ok = match &ctx.root {
                    Some(root) if p.is_absolute() => realpath(p).starts_with(realpath(Path::new(root))),
                    _ => false,
                };
                if !ok {
                    return Decision::new(Action::Deny, r, "caminho fora da raiz do projeto");
                }
            } else if !ISOLADAS.contains(&call.name.as_str()) {
                return Decision::new(Action::Deny, r, "ferramenta sem isolamento de projeto");
            }
        }
        if let Some(limite) = self.rate.check(&call.name) {
            return Decision::new(Action::Deny, r, limite);
        }
        let Some(motivo) = self.motivo_confirmacao(&spec, risk, call, ctx) else {
            return Decision::new(Action::Allow, r, "");
        };
        let binding = self.binding(&call.name, ctx);
        if consume_approval && self.approvals.consume(&ctx.session_id, &call.name, &call.args, &binding) {
            return Decision::new(Action::Allow, r, format!("aprovado fora de banda ({motivo})"));
        }
        let pedido = self.approvals.request(&ctx.session_id, &call.name, &call.args, &motivo, &binding);
        Decision { action: Action::Confirm, risk: r, reason: motivo, approval_id: Some(pedido.id) }
    }

    /// Liga a aprovação ao projeto, às autoridades e à revisão da ferramenta: mudar qualquer um
    /// deles invalida o que foi aprovado.
    pub fn binding(&self, name: &str, ctx: &Context) -> String {
        let spec = self.spec(name);
        if ctx.authorities.is_empty()
            && ctx.project_id.is_none()
            && !spec.as_ref().is_some_and(|s| s.revision.is_some())
        {
            return String::new();
        }
        let v = json!([
            ctx.project_id,
            ctx.project_revision,
            ctx.authorities,
            spec.as_ref().and_then(|s| s.origin.clone()),
            spec.as_ref().and_then(|s| s.revision.clone()),
        ]);
        pyjson::dumps_ascii(&v, true)
    }

    fn motivo_confirmacao(&self, spec: &ToolSpec, risk: Risk, call: &ToolCall, ctx: &Context) -> Option<String> {
        if risk == Risk::Destructive {
            return Some("ação destrutiva".into());
        }
        if spec.require_confirmation {
            return Some("ação externa exige confirmação para esta revisão".into());
        }
        let mut motivo: Option<String> = None;
        if risk == Risk::Exec {
            if let Some(arg) = &spec.cmd_arg {
                let v = classify_command(&call.arg_str(arg));
                if !v.read_only {
                    motivo = Some(format!("execução fora da lista de leitura segura: {}", v.reason));
                }
            } else {
                motivo = Some("execução/automação (agente, navegador ou UI)".into());
            }
        } else if let (Some(arg), Risk::Read) = (&spec.read_path_arg, risk) {
            motivo = self.path_guard.check_read(&call.arg_str(arg));
        } else if let (Some(arg), Risk::Write) = (&spec.path_arg, risk) {
            let caminho = call.arg_str(arg);
            motivo = if spec.organize {
                self.path_guard.check_organize(&caminho)
            } else {
                self.path_guard.check_write(&caminho)
            };
        }
        if motivo.is_none() && ctx.tainted && matches!(risk, Risk::Write | Risk::Exec) {
            motivo = Some(
                "a sessão leu conteúdo externo (web/e-mail/documento): possível prompt injection".into(),
            );
        }
        if motivo.is_none() && ctx.tainted && spec.egress {
            // leitura que escolhe o destino na rede: uma página injetada pode pedir
            // `https://dono-da-pagina/?d=<dados>`. O cartão mostra a URL inteira
            // (REGRAS.md: só se aprova o que se vê inteiro).
            motivo = Some(
                "a sessão leu conteúdo externo e esta ferramenta fala com um endereço escolhido \
                 pelo modelo: possível exfiltração de dados pela URL"
                    .into(),
            );
        }
        motivo
    }

    fn record(&self, call: &ToolCall, ctx: &Context, d: &Decision) -> bool {
        let Some(audit) = &self.audit else { return true };
        let spec = self.spec(&call.name);
        let args: Map<String, Value> = call
            .args
            .iter()
            .map(|(k, v)| {
                let mascarado = spec.as_ref().is_some_and(|s| s.masked_args.contains(k));
                (k.clone(), if mascarado { Value::String(MASCARA.into()) } else { v.clone() })
            })
            .collect();
        let evento = json!({
            "session_id": ctx.session_id,
            "tool": call.name,
            "origin": spec.as_ref().and_then(|s| s.origin.clone()),
            "revision": spec.as_ref().and_then(|s| s.revision.clone()),
            "args": redact_pii(&Value::Object(args), 500),
            "action": d.action.as_str(),
            "risk": d.risk.map(Risk::as_str),
            "reason": d.reason,
            "tainted": ctx.tainted,
        });
        match audit(&evento) {
            Ok(()) => true,
            Err(e) => {
                tracing::error!("falha ao gravar audit: {e}");
                false
            }
        }
    }
}

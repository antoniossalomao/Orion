//! Registro de ferramentas: nome (PT, contrato de function-calling), esquema e função.

use futures::future::BoxFuture;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use unicode_normalization::UnicodeNormalization;

/// Erro de uma ferramenta, no formato que o Python devolvia (`TipoDoErro: mensagem`).
#[derive(Debug, Clone)]
pub enum ToolError {
    /// Argumento ausente, de tipo errado ou desconhecido.
    Args(String),
    /// Falha da execução: (tipo, mensagem).
    Exec(String, String),
}

impl ToolError {
    pub fn exec(tipo: &str, msg: impl std::fmt::Display) -> Self {
        ToolError::Exec(tipo.into(), msg.to_string())
    }
    pub fn valor(msg: impl std::fmt::Display) -> Self {
        Self::exec("ValueError", msg)
    }
}

impl From<orion_store::StoreError> for ToolError {
    fn from(e: orion_store::StoreError) -> Self {
        match e {
            orion_store::StoreError::NotFound(m) => ToolError::exec("KeyError", format!("'{m}'")),
            orion_store::StoreError::Invalid(m) => ToolError::exec("ValueError", m),
            outro => ToolError::exec("RuntimeError", outro),
        }
    }
}

impl From<anyhow::Error> for ToolError {
    fn from(e: anyhow::Error) -> Self {
        ToolError::exec("RuntimeError", e)
    }
}

impl From<std::io::Error> for ToolError {
    fn from(e: std::io::Error) -> Self {
        ToolError::exec("OSError", e)
    }
}

pub type ToolResult = Result<Value, ToolError>;
pub type ToolFn = Arc<dyn Fn(Args) -> BoxFuture<'static, ToolResult> + Send + Sync>;

/// Argumentos de uma chamada, com conversões tolerantes (modelos mandam número como texto).
#[derive(Debug, Clone, Default)]
pub struct Args(pub Map<String, Value>);

impl Args {
    pub fn get(&self, k: &str) -> Option<&Value> {
        self.0.get(k).filter(|v| !v.is_null())
    }

    /// Texto obrigatório (a presença já foi conferida pelo `required`).
    pub fn s(&self, k: &str) -> Result<String, ToolError> {
        self.opt_s(k).ok_or_else(|| ToolError::Args(format!("argumento obrigatório ausente: '{k}'")))
    }

    pub fn opt_s(&self, k: &str) -> Option<String> {
        match self.get(k)? {
            Value::String(s) => Some(s.clone()),
            v => Some(v.to_string()),
        }
    }

    pub fn s_or(&self, k: &str, padrao: &str) -> String {
        self.opt_s(k).unwrap_or_else(|| padrao.into())
    }

    pub fn opt_i(&self, k: &str) -> Result<Option<i64>, ToolError> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::Number(n)) => n
                .as_i64()
                .or_else(|| n.as_f64().map(|f| f as i64))
                .map(Some)
                .ok_or_else(|| ToolError::Args(format!("'{k}' precisa ser inteiro"))),
            Some(Value::String(s)) => s
                .trim()
                .parse::<i64>()
                .map(Some)
                .map_err(|_| ToolError::exec("ValueError", format!("invalid literal for int() with base 10: '{s}'"))),
            Some(Value::Bool(b)) => Ok(Some(*b as i64)),
            Some(_) => Err(ToolError::Args(format!("'{k}' precisa ser inteiro"))),
        }
    }

    pub fn i(&self, k: &str) -> Result<i64, ToolError> {
        self.opt_i(k)?.ok_or_else(|| ToolError::Args(format!("argumento obrigatório ausente: '{k}'")))
    }

    pub fn i_or(&self, k: &str, padrao: i64) -> Result<i64, ToolError> {
        Ok(self.opt_i(k)?.unwrap_or(padrao))
    }

    pub fn opt_f(&self, k: &str) -> Result<Option<f64>, ToolError> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::Number(n)) => Ok(n.as_f64()),
            Some(Value::String(s)) => s
                .trim()
                .parse::<f64>()
                .map(Some)
                .map_err(|_| ToolError::exec("ValueError", format!("could not convert string to float: '{s}'"))),
            Some(_) => Err(ToolError::Args(format!("'{k}' precisa ser número"))),
        }
    }

    pub fn f_or(&self, k: &str, padrao: f64) -> Result<f64, ToolError> {
        Ok(self.opt_f(k)?.unwrap_or(padrao))
    }

    /// `bool(valor)` do Python.
    pub fn b_or(&self, k: &str, padrao: bool) -> bool {
        match self.get(k) {
            None => padrao,
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
            Some(Value::String(s)) => !s.is_empty(),
            Some(Value::Array(a)) => !a.is_empty(),
            Some(Value::Object(o)) => !o.is_empty(),
            Some(Value::Null) => false,
        }
    }

    pub fn list_s(&self, k: &str) -> Vec<String> {
        match self.get(k) {
            Some(Value::Array(a)) => a.iter().map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).collect(),
            Some(Value::String(s)) => vec![s.clone()],
            _ => vec![],
        }
    }
}

#[derive(Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// JSON Schema.
    pub parameters: Value,
    pub fn_: ToolFn,
    pub origin: Option<String>,
    /// Valida o esquema completo antes de executar (as de MCP e extensões sempre validam). As
    /// nativas só exigem os obrigatórios.
    pub validar: bool,
}

impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool").field("name", &self.name).field("origin", &self.origin).finish()
    }
}

fn erro(mensagem: &str, codigo: &str) -> String {
    json!({"ok": false, "erro": mensagem, "codigo": codigo}).to_string()
}

/// Serialização do resultado (igual ao `json.dumps(..., ensure_ascii=False)`).
pub fn resultado(v: &Value) -> String {
    orion_policy::pyjson::dumps(v, false)
}

impl Tool {
    /// Ferramenta assíncrona.
    pub fn new<F, Fut>(name: &str, description: &str, parameters: Value, f: F) -> Self
    where
        F: Fn(Args) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ToolResult> + Send + 'static,
    {
        Tool {
            name: name.into(),
            description: description.into(),
            parameters,
            fn_: Arc::new(move |a| Box::pin(f(a))),
            origin: None,
            validar: false,
        }
    }

    /// Ferramenta síncrona: roda fora do laço assíncrono, no escopo de dados de quem chamou.
    pub fn sync<F>(name: &str, description: &str, parameters: Value, f: F) -> Self
    where
        F: Fn(Args) -> ToolResult + Send + Sync + 'static,
    {
        let f = Arc::new(f);
        Tool::new(name, description, parameters, move |a| {
            let f = f.clone();
            let escopo = orion_store::current_scope();
            async move {
                tokio::task::spawn_blocking(move || orion_store::with_scope(escopo, || f(a)))
                    .await
                    .unwrap_or_else(|e| Err(ToolError::exec("RuntimeError", e)))
            }
        })
    }

    pub fn validando(mut self) -> Self {
        self.validar = true;
        self
    }

    pub fn com_origem(mut self, origem: &str) -> Self {
        self.origin = Some(origem.into());
        self
    }

    pub fn schema(&self) -> Value {
        json!({"type": "function", "function": {
            "name": self.name, "description": self.description, "parameters": self.parameters,
        }})
    }

    pub fn invalido(&self, args: &Map<String, Value>) -> Option<String> {
        let obrigatorios: Vec<&str> = self
            .parameters
            .get("required")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let faltam: Vec<&str> = obrigatorios.iter().copied().filter(|k| !args.contains_key(*k)).collect();
        if !(self.validar || self.origin.is_some()) {
            if !faltam.is_empty() {
                return Some(erro(&format!("argumentos obrigatórios ausentes: {}", faltam.join(", ")), "arguments_invalid"));
            }
            // como a chamada com kwargs do Python: argumento desconhecido é erro
            if let Some(props) = self.parameters.get("properties").and_then(Value::as_object) {
                let sobra = self.parameters.get("additionalProperties").is_none_or(|v| v != &Value::Bool(true));
                if let Some(k) = args.keys().find(|k| !props.contains_key(*k)).filter(|_| sobra) {
                    return Some(erro(
                        &format!("argumentos inválidos: {}() got an unexpected keyword argument '{k}'", self.name),
                        "arguments_invalid",
                    ));
                }
            }
            return None;
        }
        let validador = match jsonschema::validator_for(&self.parameters) {
            Ok(v) => v,
            Err(_) => return Some(erro("schema da ferramenta inválido", "schema_invalid")),
        };
        let instancia = Value::Object(args.clone());
        if let Some(e) = validador.iter_errors(&instancia).next() {
            let restricao = e.schema_path.as_str().rsplit('/').next().unwrap_or("").to_string();
            let msg = if restricao == "required" && !faltam.is_empty() {
                format!("argumentos obrigatórios ausentes: {}", faltam.join(", "))
            } else {
                let caminho: String = e
                    .instance_path
                    .as_str()
                    .split('/')
                    .filter(|p| !p.is_empty())
                    .map(|p| format!("[{p}]"))
                    .collect();
                format!("argumentos inválidos em ${caminho}: restrição {restricao}")
            };
            return Some(erro(&msg, "arguments_invalid"));
        }
        None
    }

    /// Mesmo contrato JSON do Python: resultado serializado, ou `{"ok": false, "erro": ...}`.
    pub async fn run(&self, args: Map<String, Value>) -> String {
        if let Some(inv) = self.invalido(&args) {
            return inv;
        }
        match (self.fn_)(Args(args)).await {
            Ok(v) => resultado(&v),
            Err(ToolError::Args(m)) => erro(&format!("argumentos inválidos: {m}"), "arguments_invalid"),
            Err(ToolError::Exec(t, m)) => erro(&format!("{t}: {m}"), "execution_error"),
        }
    }
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: Vec<Tool>,
    idx: HashMap<String, usize>,
}

const STOP: [&str; 10] = ["para", "como", "quero", "fazer", "uma", "que", "com", "por", "dos", "das"];

/// Palavras de 3+ letras, sem acento, sem as vazias.
pub fn words(text: &str) -> HashSet<String> {
    let plain: String = text
        .to_lowercase()
        .nfkd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect();
    let mut out = HashSet::new();
    let mut atual = String::new();
    for c in plain.chars().chain(std::iter::once(' ')) {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            atual.push(c);
        } else {
            if atual.len() >= 3 && !STOP.contains(&atual.as_str()) {
                out.insert(atual.clone());
            }
            atual.clear();
        }
    }
    out
}

impl ToolRegistry {
    pub fn new(tools: Vec<Tool>) -> anyhow::Result<Self> {
        let mut r = ToolRegistry::default();
        for t in tools {
            r.register(t)?;
        }
        Ok(r)
    }

    pub fn register(&mut self, tool: Tool) -> anyhow::Result<()> {
        anyhow::ensure!(!self.idx.contains_key(&tool.name), "ferramenta duplicada: {}", tool.name);
        if tool.validar || tool.origin.is_some() {
            jsonschema::validator_for(&tool.parameters).map_err(|e| anyhow::anyhow!("schema inválido em {}: {e}", tool.name))?;
        }
        self.idx.insert(tool.name.clone(), self.tools.len());
        self.tools.push(tool);
        Ok(())
    }

    pub fn unregister(&mut self, name: &str) {
        if self.idx.remove(name).is_some() {
            self.tools.retain(|t| t.name != name);
            self.idx = self.tools.iter().enumerate().map(|(i, t)| (t.name.clone(), i)).collect();
        }
    }

    pub fn get(&self, name: &str) -> Option<&Tool> {
        self.idx.get(name).map(|i| &self.tools[*i])
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name.clone()).collect()
    }

    pub fn catalog(&self, query: &str, limit: usize, allowed: Option<&HashSet<String>>) -> Vec<Value> {
        let ws = words(query);
        let mut ranked: Vec<(usize, &Tool)> = vec![];
        for t in &self.tools {
            if t.origin.is_none() || allowed.is_some_and(|a| !a.contains(&t.name)) {
                continue;
            }
            let score = ws.intersection(&words(&t.description)).count();
            if !ws.is_empty() && score == 0 {
                continue;
            }
            ranked.push((score, t));
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        ranked
            .into_iter()
            .take(limit.min(100))
            .map(|(_, t)| json!({"name": t.name, "description": t.description.chars().take(240).collect::<String>(), "origin": t.origin}))
            .collect()
    }

    /// Nativas sempre; esquemas externos entram só por escolha ou relevância, dentro do orçamento.
    pub fn schemas(&self, query: Option<&str>, selected: &[String], limit: usize, budget: usize, allowed: Option<&HashSet<String>>) -> Vec<Value> {
        let mut result: Vec<Value> = self
            .tools
            .iter()
            .filter(|t| t.origin.is_none() && allowed.is_none_or(|a| a.contains(&t.name)))
            .map(Tool::schema)
            .collect();
        let mut candidatos: Vec<String> = selected.to_vec();
        if let Some(q) = query {
            candidatos.extend(self.catalog(q, 50, allowed).iter().filter_map(|r| r["name"].as_str().map(String::from)));
        }
        let mut vistos = HashSet::new();
        let (mut usado, mut count) = (0usize, 0usize);
        for nome in candidatos {
            if !vistos.insert(nome.clone()) || allowed.is_some_and(|a| !a.contains(&nome)) {
                continue;
            }
            let Some(t) = self.get(&nome) else { continue };
            if t.origin.is_none() || count >= limit.min(32) {
                continue;
            }
            let s = t.schema();
            let tamanho = resultado(&s).len();
            if usado + tamanho > budget {
                continue;
            }
            result.push(s);
            usado += tamanho;
            count += 1;
        }
        result
    }

    pub fn all_schemas(&self) -> Vec<Value> {
        self.schemas(None, &[], 8, 24000, None)
    }

    pub fn context_usage(schemas: &[Value]) -> Value {
        let size = resultado(&Value::Array(schemas.to_vec())).len();
        json!({"schemas": schemas.len(), "bytes": size, "estimated_tokens": size.div_ceil(4)})
    }
}

/// Esquema de objeto com propriedades e obrigatórios.
pub fn obj(props: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": props, "required": required})
}

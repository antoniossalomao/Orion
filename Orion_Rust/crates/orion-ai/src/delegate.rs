//! Delegação de tarefa pesada às CLIs oficiais (`claude -p`, `codex exec`, `gemini -p`).
//!
//! Usa as assinaturas pelo caminho oficial, sem token extraído. Cada chamada tem pasta de
//! trabalho, tempo-limite e contador diário; se a cota de uma acabar, tenta a próxima. Nunca
//! passa por shell: o prompt vai como UM argumento. A CLI não herda `ORION_*`
//! (REGRAS.md: segredos nunca em log, audit ou processo filho).

use crate::saidas;
use orion_store::Store;
use regex::Regex;
use serde_json::{json, Value};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

static COTA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)quota|rate.?limit|usage limit|too many requests|limit reached|\b429\b|exceeded").unwrap());

#[derive(Debug, Clone)]
pub struct CliAgent {
    pub name: String,
    /// `{prompt}` vira o texto da tarefa.
    pub argv: Vec<String>,
    pub daily_limit: i64,
}

pub fn default_agents() -> Vec<CliAgent> {
    let a = |n: &str, argv: &[&str]| CliAgent { name: n.into(), argv: argv.iter().map(|s| s.to_string()).collect(), daily_limit: 20 };
    vec![a("claude", &["claude", "-p", "{prompt}"]), a("codex", &["codex", "exec", "{prompt}"]), a("gemini", &["gemini", "-p", "{prompt}"])]
}

/// `shutil.which`.
pub fn which(nome: &str) -> Option<PathBuf> {
    let p = PathBuf::from(nome);
    if p.components().count() > 1 {
        return p.is_file().then_some(p);
    }
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT").unwrap_or(".COM;.EXE;.BAT;.CMD".into()).split(';').map(|s| s.to_lowercase()).collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        for e in &exts {
            let c = dir.join(format!("{nome}{e}"));
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

pub struct Delegator {
    store: Arc<Store>,
    agents: Vec<CliAgent>,
    timeout: Duration,
    max_output: usize,
    pub weekly_budget: parking_lot::Mutex<Option<Arc<crate::budget::Budget>>>,
}

struct Saida {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Delegator {
    pub fn new(store: Arc<Store>, agents: Vec<CliAgent>, timeout_s: f64, max_output: usize) -> Self {
        Delegator { store, agents, timeout: Duration::from_secs_f64(timeout_s), max_output, weekly_budget: parking_lot::Mutex::new(None) }
    }

    fn chave(&self, nome: &str) -> String {
        let dia = chrono::DateTime::from_timestamp(self.store.now() as i64, 0).unwrap_or_default().with_timezone(&chrono::Local).format("%Y%m%d");
        format!("delegar:{nome}:{dia}")
    }

    /// Uso de hoje por CLI (para o painel).
    pub fn status(&self) -> Vec<Value> {
        self.agents
            .iter()
            .map(|ag| {
                let usadas = self.store.counter_get(&self.chave(&ag.name));
                json!({"nome": ag.name, "instalada": which(&ag.argv[0]).is_some(), "usadas_hoje": usadas,
                       "limite_diario": ag.daily_limit, "restante": (ag.daily_limit - usadas).max(0)})
            })
            .collect()
    }

    fn rodar(&self, argv: &[String], pasta: &PathBuf) -> std::io::Result<Option<Saida>> {
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]).current_dir(pasta).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.env_clear();
        for (k, v) in std::env::vars_os() {
            if !k.to_string_lossy().to_uppercase().starts_with("ORION_") {
                cmd.env(k, v);
            }
        }
        // Windows: CLI em Python escreve no pipe com a codepage local; força UTF-8.
        cmd.env("PYTHONUTF8", "1").env("PYTHONIOENCODING", "utf-8");
        let mut filho = cmd.spawn()?;
        let mut out = filho.stdout.take();
        let mut err = filho.stderr.take();
        let t_out = std::thread::spawn(move || {
            let mut b = vec![];
            if let Some(o) = out.as_mut() {
                let _ = o.read_to_end(&mut b);
            }
            b
        });
        let t_err = std::thread::spawn(move || {
            let mut b = vec![];
            if let Some(o) = err.as_mut() {
                let _ = o.read_to_end(&mut b);
            }
            b
        });
        match filho.wait_timeout(self.timeout)? {
            Some(st) => Ok(Some(Saida {
                code: st.code(),
                stdout: String::from_utf8_lossy(&t_out.join().unwrap_or_default()).into_owned(),
                stderr: String::from_utf8_lossy(&t_err.join().unwrap_or_default()).into_owned(),
            })),
            None => {
                let _ = filho.kill();
                let _ = filho.wait();
                Ok(None)
            }
        }
    }

    pub fn delegate(&self, tarefa: &str, pasta: &str, agente: Option<&str>) -> Value {
        if tarefa.trim().is_empty() {
            return json!({"ok": false, "erro": "tarefa vazia"});
        }
        let raiz = PathBuf::from(orion_policy::paths::norm(pasta));
        if !raiz.is_dir() {
            return json!({"ok": false, "erro": format!("pasta inexistente: {pasta}")});
        }
        let nomes: Vec<&str> = self.agents.iter().map(|a| a.name.as_str()).collect();
        if let Some(a) = agente {
            if !nomes.contains(&a) {
                return json!({"ok": false, "erro": format!("agente desconhecido: {a} (use {})", nomes.join(", "))});
            }
        }
        let mut ordem: Vec<&str> = agente.into_iter().collect();
        ordem.extend(nomes.iter().copied().filter(|n| Some(*n) != agente));
        let mut tentativas: Vec<Value> = vec![];
        for nome in ordem {
            let ag = self.agents.iter().find(|a| a.name == nome).unwrap();
            let Some(exe) = which(&ag.argv[0]) else {
                tentativas.push(json!({"agente": nome, "motivo": "CLI não instalada"}));
                continue;
            };
            let low = exe.to_string_lossy().to_lowercase();
            if low.ends_with(".cmd") || low.ends_with(".bat") {
                // Shims .cmd/.bat passam o argumento pelo cmd.exe: um prompt com aspas ou % vira
                // injeção de comando. Só binário de verdade.
                tentativas.push(json!({"agente": nome, "motivo": "shim .cmd/.bat recusado (use o .exe)"}));
                continue;
            }
            let chave = self.chave(nome);
            if self.store.counter_get(&chave) >= ag.daily_limit {
                tentativas.push(json!({"agente": nome, "motivo": "limite diário atingido"}));
                continue;
            }
            if let Some(budget) = self.weekly_budget.lock().clone() {
                if let Err(e) = budget.reserve(&format!("cli:{nome}"), &orion_store::uuid_hex()) {
                    tentativas.push(json!({"agente": nome, "motivo": e.to_string()}));
                    continue;
                }
            }
            let _ = self.store.counter_incr(&chave, 1);
            // "Tarefa:" evita que um prompt iniciado por "-" vire opção da CLI.
            let mut argv = vec![exe.to_string_lossy().into_owned()];
            argv.extend(ag.argv[1..].iter().map(|a| a.replace("{prompt}", &format!("Tarefa: {tarefa}"))));
            let inicio = Instant::now();
            let reg = |r: Option<&Saida>| {
                saidas::registrar(
                    &format!("cli:{nome}"),
                    "cli",
                    r.is_some_and(|r| r.code == Some(0)),
                    inicio.elapsed().as_secs_f64() * 1000.0,
                    "",
                    tarefa.len(),
                    r.map(|r| r.stdout.len()).unwrap_or(0),
                    "texto",
                )
            };
            let r = match self.rodar(&argv, &raiz) {
                Ok(Some(r)) => r,
                Ok(None) => {
                    reg(None);
                    return json!({"ok": false, "agente": nome, "erro": format!("tempo-limite de {:.0}s", self.timeout.as_secs_f64()), "tentativas": tentativas});
                }
                Err(e) => {
                    reg(None);
                    tentativas.push(json!({"agente": nome, "motivo": format!("não executou: {e}")}));
                    continue;
                }
            };
            reg(Some(&r));
            let saida = r.stdout.trim().to_string();
            if r.code == Some(0) {
                let truncado = saida.chars().count() > self.max_output;
                return json!({"ok": true, "agente": nome, "saida": saida.chars().take(self.max_output).collect::<String>(),
                              "truncado": truncado, "tentativas": tentativas});
            }
            let erro = if r.stderr.trim().is_empty() { saida.clone() } else { r.stderr.trim().to_string() };
            if COTA.is_match(&erro) {
                let _ = self.store.counter_set(&chave, ag.daily_limit); // esgotada até amanhã
                tentativas.push(json!({"agente": nome, "motivo": "cota esgotada"}));
                tracing::warn!("cota da CLI {nome} esgotada; tentando a próxima");
                continue;
            }
            let erro: String = erro.chars().take(2000).collect();
            let erro = if erro.is_empty() { format!("código {}", r.code.unwrap_or(-1)) } else { erro };
            return json!({"ok": false, "agente": nome, "erro": erro, "tentativas": tentativas});
        }
        json!({"ok": false, "erro": "nenhum agente disponível", "tentativas": tentativas})
    }
}

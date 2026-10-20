//! Nuvem (PLANO F12.15–F12.19): tarefas que rodam na nuvem pelas CLIs oficiais, na assinatura.
//!
//! `claude --cloud "<tarefa>"` cria uma sessão do Claude Code na nuvem; `codex cloud exec --env
//! <ENV> "<tarefa>"` cria uma tarefa do Codex. Só quando você pede (REGRAS.md: opt-in não liga
//! sozinho; `ORION_NUVEM=true`), só pelas CLIs oficiais, sem token extraído, sem shell e sem
//! herdar `ORION_*` (REGRAS.md: segredos nunca em log, audit ou processo filho). Nada é ferramenta
//! do modelo: o agente nunca manda código para a nuvem sozinho. Trazer o resultado muda a pasta
//! local e por isso só acontece por clique ou comando seu; o Codex falho nunca troca de serviço.

use orion_store::Store;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

const CHAVE: &str = "nuvem:tarefas";
const MAX_TAREFAS: usize = 50;
pub const MAX_TEXTO: usize = 4000;

static SESSAO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b((?:session|cse)_[A-Za-z0-9]{6,})\b").unwrap());
static TAREFA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(task_[A-Za-z0-9_]{4,})\b").unwrap());
static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https://[^\s"'<>)\]]+"#).unwrap());

#[derive(Debug, Clone, thiserror::Error)]
#[error("{mensagem}")]
pub struct NuvemError {
    pub mensagem: String,
    pub status: u16,
}

fn erro(status: u16, m: impl Into<String>) -> NuvemError {
    NuvemError { mensagem: m.into(), status }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TarefaNuvem {
    pub id: String,
    pub servico: String,
    pub titulo: String,
    pub url: String,
    pub estado: String,
    pub criada: f64,
    pub pasta: String,
}

pub struct Nuvem {
    store: Arc<Store>,
    pasta: PathBuf,
    codex_env: String,
    timeout: Duration,
}

struct Saida {
    code: Option<i32>,
    texto: String,
}

/// Servidor ou nome vindo da tela: só `claude` ou `codex`.
pub fn servico_valido(s: &str) -> Option<&'static str> {
    match s.trim().to_lowercase().as_str() {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        _ => None,
    }
}

/// Extrai id e link da saída da CLI.
pub fn extrair(servico: &str, saida: &str) -> (Option<String>, Option<String>) {
    let re = if servico == "claude" { &SESSAO } else { &TAREFA };
    let id = re.captures(saida).map(|c| c[1].to_string());
    let url = URL.find_iter(saida).map(|m| m.as_str().trim_end_matches(['.', ',']).to_string()).find(|u| id.as_deref().is_some_and(|i| u.contains(i)) || u.contains("claude.ai/code") || u.contains("codex"));
    (id, url)
}

fn titulo(tarefa: &str) -> String {
    let t: String = tarefa.split_whitespace().collect::<Vec<_>>().join(" ");
    t.chars().take(90).collect()
}

impl Nuvem {
    pub fn new(store: Arc<Store>, pasta: PathBuf, codex_env: String, timeout_s: f64) -> Self {
        Nuvem { store, pasta, codex_env, timeout: Duration::from_secs_f64(timeout_s) }
    }

    fn carregar(&self) -> Vec<TarefaNuvem> {
        self.store.meta_get(CHAVE).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    fn gravar(&self, v: &[TarefaNuvem]) {
        let _ = self.store.meta_set(CHAVE, Some(&serde_json::to_string(v).unwrap_or_else(|_| "[]".into())));
    }

    fn exe(nome: &str) -> Result<String, NuvemError> {
        let exe = orion_ai::delegate::which(nome).ok_or_else(|| erro(424, format!("a CLI `{nome}` não está instalada")))?;
        let low = exe.to_string_lossy().to_lowercase();
        if low.ends_with(".cmd") || low.ends_with(".bat") {
            // shim passa o argumento pelo cmd.exe: aspas e % virariam injeção de comando
            return Err(erro(422, format!("shim .cmd/.bat de `{nome}` recusado (use o .exe)")));
        }
        Ok(exe.to_string_lossy().into_owned())
    }

    /// Roda a CLI sem shell, sem `ORION_*`, na pasta do projeto, com tempo-limite.
    async fn rodar(&self, argv: &[String], pasta: &std::path::Path) -> Result<Option<Saida>, NuvemError> {
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]).current_dir(pasta).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        for (k, _) in std::env::vars() {
            if k.starts_with("ORION_") {
                cmd.env_remove(k);
            }
        }
        let mut filho = cmd.spawn().map_err(|e| erro(500, format!("não executou: {e}")))?;
        let (mut so, mut se) = (filho.stdout.take().unwrap(), filho.stderr.take().unwrap());
        let ler = async {
            let (mut a, mut b) = (Vec::new(), Vec::new());
            let _ = tokio::join!(so.read_to_end(&mut a), se.read_to_end(&mut b));
            a.extend_from_slice(&b);
            a
        };
        let inicio = Instant::now();
        let r = tokio::time::timeout(self.timeout, async {
            let bytes = ler.await;
            let st = filho.wait().await;
            (bytes, st)
        })
        .await;
        orion_ai::saidas::registrar("cli:nuvem", "cli", r.is_ok(), inicio.elapsed().as_secs_f64() * 1000.0, "", argv.iter().map(|a| a.len()).sum(), 0, "texto");
        match r {
            Ok((bytes, st)) => Ok(Some(Saida { code: st.ok().and_then(|s| s.code()), texto: String::from_utf8_lossy(&bytes).into_owned() })),
            Err(_) => Ok(None),
        }
    }

    /// `/nuvem claude|codex <tarefa>`: só quando você pede.
    pub async fn criar(&self, servico: &str, tarefa: &str, env: Option<&str>) -> Result<TarefaNuvem, NuvemError> {
        let servico = servico_valido(servico).ok_or_else(|| erro(422, "serviço desconhecido (use claude ou codex)"))?;
        let tarefa = tarefa.trim();
        if tarefa.is_empty() || tarefa.chars().count() > MAX_TEXTO {
            return Err(erro(422, format!("a tarefa precisa ter de 1 a {MAX_TEXTO} caracteres")));
        }
        if !self.pasta.is_dir() {
            return Err(erro(422, "a pasta do projeto não existe"));
        }
        let exe = Self::exe(servico)?;
        // "Tarefa:" evita que um pedido iniciado por "-" vire opção da CLI
        let pedido = format!("Tarefa: {tarefa}");
        let argv: Vec<String> = if servico == "claude" {
            vec![exe, "--cloud".into(), pedido]
        } else {
            let e = env.map(str::trim).filter(|e| !e.is_empty()).unwrap_or(self.codex_env.trim());
            if e.is_empty() {
                return Err(erro(422, "o Codex na nuvem precisa de um ambiente: defina ORION_CODEX_ENV (veja `codex cloud`)"));
            }
            if !e.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) {
                return Err(erro(422, "ambiente do Codex inválido"));
            }
            vec![exe, "cloud".into(), "exec".into(), "--env".into(), e.to_string(), pedido]
        };
        let Some(s) = self.rodar(&argv, &self.pasta).await? else {
            return Err(erro(504, format!("tempo-limite de {:.0}s ao criar a tarefa na nuvem", self.timeout.as_secs_f64())));
        };
        let (id, url) = extrair(servico, &s.texto);
        if s.code != Some(0) && id.is_none() {
            let fim: String = s.texto.trim().lines().rev().take(3).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" ");
            let fim: String = fim.chars().take(300).collect();
            return Err(erro(502, format!("a CLI recusou: {fim}")));
        }
        let t = TarefaNuvem {
            id: id.unwrap_or_else(|| format!("local_{}", orion_store::uuid_hex())),
            servico: servico.into(),
            titulo: titulo(tarefa),
            url: url.unwrap_or_default(),
            estado: "enviada".into(),
            criada: self.store.now(),
            pasta: self.pasta.to_string_lossy().into_owned(),
        };
        let mut v = self.carregar();
        v.insert(0, t.clone());
        v.truncate(MAX_TAREFAS);
        self.gravar(&v);
        Ok(t)
    }

    /// Lista as tarefas; as do Codex ganham o estado atual de `codex cloud list --json`.
    pub async fn listar(&self) -> Vec<TarefaNuvem> {
        let mut v = self.carregar();
        if v.iter().any(|t| t.servico == "codex") {
            if let Ok(exe) = Self::exe("codex") {
                let argv = vec![exe, "cloud".into(), "list".into(), "--json".into(), "--limit".into(), "20".into()];
                if let Ok(Some(s)) = self.rodar(&argv, &self.pasta).await {
                    if s.code == Some(0) {
                        if let Ok(j) = serde_json::from_str::<Value>(s.texto.trim()) {
                            atualizar_codex(&mut v, &j);
                            self.gravar(&v);
                        }
                    }
                }
            }
        }
        v
    }

    fn achar(&self, id: &str) -> Result<TarefaNuvem, NuvemError> {
        self.carregar().into_iter().find(|t| t.id == id).ok_or_else(|| erro(404, "tarefa não encontrada"))
    }

    /// Mensagem para uma sessão do Claude na nuvem (`claude -p "<msg>" --cloud <id>`).
    pub async fn mensagem(&self, id: &str, texto: &str) -> Result<Value, NuvemError> {
        let t = self.achar(id)?;
        if t.servico != "claude" {
            return Err(erro(422, "o Codex na nuvem não recebe mensagem pela CLI; use o link da tarefa"));
        }
        if !SESSAO.is_match(&t.id) {
            return Err(erro(422, "esta tarefa não tem o id da sessão"));
        }
        let texto = texto.trim();
        if texto.is_empty() || texto.chars().count() > MAX_TEXTO {
            return Err(erro(422, format!("a mensagem precisa ter de 1 a {MAX_TEXTO} caracteres")));
        }
        let argv = vec![Self::exe("claude")?, "-p".into(), texto.to_string(), "--cloud".into(), t.id.clone(), "--output-format".into(), "json".into()];
        let Some(s) = self.rodar(&argv, &PathBuf::from(&t.pasta)).await? else {
            return Err(erro(504, "tempo-limite ao enviar a mensagem"));
        };
        let j: Value = serde_json::from_str(s.texto.trim()).unwrap_or(Value::Null);
        if s.code == Some(0) && j.get("ok").and_then(Value::as_bool) != Some(false) {
            Ok(json!({"ok": true, "id": t.id, "url": j.get("url").cloned().unwrap_or(json!(t.url))}))
        } else {
            let e = j.get("error").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| s.texto.trim().chars().take(200).collect());
            Err(erro(502, format!("a sessão recusou a mensagem: {e}")))
        }
    }

    /// Trazer o resultado: Codex aplica o diff (`codex apply <id>`, muda a pasta local, só por clique);
    /// Claude devolve o comando de teleporte, que é interativo e roda no seu terminal.
    pub async fn trazer(&self, id: &str) -> Result<Value, NuvemError> {
        let t = self.achar(id)?;
        if t.servico == "claude" {
            return Ok(json!({"ok": true, "aplicado": false, "comando": format!("claude --teleport {}", t.id), "url": t.url,
                "aviso": "O teleporte é interativo: rode o comando num terminal, dentro de um clone do mesmo repositório."}));
        }
        if !TAREFA.is_match(&t.id) {
            return Err(erro(422, "esta tarefa não tem o id do Codex"));
        }
        let argv = vec![Self::exe("codex")?, "apply".into(), t.id.clone()];
        let Some(s) = self.rodar(&argv, &PathBuf::from(&t.pasta)).await? else {
            return Err(erro(504, "tempo-limite ao aplicar o diff"));
        };
        if s.code == Some(0) {
            Ok(json!({"ok": true, "aplicado": true, "saida": s.texto.trim().chars().take(1500).collect::<String>()}))
        } else {
            Err(erro(409, format!("o diff não aplicou: {}", s.texto.trim().chars().take(300).collect::<String>())))
        }
    }

    /// Tira da lista do Orion. As CLIs não têm cancelamento: se ainda roda, cancele pelo link.
    pub fn descartar(&self, id: &str) -> Result<Value, NuvemError> {
        let mut v = self.carregar();
        let n = v.len();
        let url = v.iter().find(|t| t.id == id).map(|t| t.url.clone()).ok_or_else(|| erro(404, "tarefa não encontrada"))?;
        v.retain(|t| t.id != id);
        self.gravar(&v);
        Ok(json!({"ok": true, "removidas": n - v.len(), "aviso": "Só saiu da lista do Orion; para parar a execução use o link da tarefa.", "url": url}))
    }
}

fn atualizar_codex(v: &mut [TarefaNuvem], j: &Value) {
    let Some(tasks) = j.get("tasks").and_then(Value::as_array) else { return };
    for t in v.iter_mut().filter(|t| t.servico == "codex") {
        if let Some(r) = tasks.iter().find(|x| x.get("id").and_then(Value::as_str) == Some(t.id.as_str())) {
            if let Some(s) = r.get("status").and_then(Value::as_str) {
                t.estado = s.to_lowercase();
            }
            if let Some(u) = r.get("url").and_then(Value::as_str) {
                t.url = u.to_string();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrai_id_e_link_do_claude_e_do_codex() {
        let (id, url) = extrair("claude", "Sent to cloud session.\nSession ID: session_01DiUkqY2kzbUbDmW1w96rfi\nView: https://claude.ai/code/session_01DiUkqY2kzbUbDmW1w96rfi?from=cli&m=0\n");
        assert_eq!(id.as_deref(), Some("session_01DiUkqY2kzbUbDmW1w96rfi"));
        assert!(url.unwrap().starts_with("https://claude.ai/code/session_01DiUkqY"));
        let (id, url) = extrair("codex", "Task submitted: https://chatgpt.com/codex/tasks/task_e_abc123def\n");
        assert_eq!(id.as_deref(), Some("task_e_abc123def"));
        assert!(url.unwrap().contains("task_e_abc123def"));
        assert_eq!(extrair("claude", "nada aqui"), (None, None));
    }

    #[test]
    fn so_aceita_claude_e_codex() {
        assert_eq!(servico_valido(" Claude "), Some("claude"));
        assert_eq!(servico_valido("gemini"), None);
    }

    #[test]
    fn estado_do_codex_vem_do_list_json() {
        let mut v = vec![TarefaNuvem { id: "task_e_1".into(), servico: "codex".into(), titulo: "x".into(), url: String::new(), estado: "enviada".into(), criada: 0.0, pasta: ".".into() }];
        atualizar_codex(&mut v, &json!({"tasks": [{"id": "task_e_1", "status": "READY", "url": "https://chatgpt.com/codex/tasks/task_e_1"}], "cursor": null}));
        assert_eq!(v[0].estado, "ready");
        assert!(v[0].url.ends_with("task_e_1"));
    }
}

//! Tarefa em segundo plano (E9.2; REGRAS.md: tarefa em segundo plano só lê e propõe): o Orion
//! trabalha num objetivo sem ninguém olhando.
//!
//! Roda o agente em modo só leitura numa conversa própria: o modelo só vê e só chama ferramentas
//! de leitura. O que ele julgar útil fazer vira uma proposta (texto, depois de `PROPOSTAS:`) que
//! você aceita ou descarta item a item; aceitar devolve o texto para o campo de mensagem, e aí vale
//! a política de sempre. Tetos: passos, minutos e cota gratuita (REGRAS.md: custo zero).

use crate::agent::{Agent, TurnRequest};
use crate::modos::Modos;
use futures::StreamExt;
use orion_ai::costs::Custos;
use orion_store::ops::Operations;
use orion_store::Store;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::task::JoinHandle;

pub const CANAL: &str = "tarefa-bg";
pub const MAX_OBJETIVO: usize = 500;
pub const MAX_PASSOS: i64 = 30;
pub const MAX_MINUTOS: i64 = 30;
pub const MAX_RELATORIO: usize = 8000;
pub const MAX_PROPOSTAS: usize = 10;
pub const MAX_PROPOSTA: usize = 240;
/// Tarefas rodando ao mesmo tempo; as outras esperam na fila, por ordem de chegada (F04.21).
pub const MAX_PARALELAS: usize = 3;

static PROPOSTAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*PROPOSTAS:[ \t]*$").unwrap());

fn prompt(objetivo: &str) -> String {
    format!(
        "Tarefa em segundo plano, sem ninguém acompanhando. Objetivo: {objetivo}\n\
         Use só ferramentas de leitura. NÃO escreva, apague, execute nem salve nada. Entregue um \
         relatório curto em português, com as fontes. Se achar que alguma ação (escrever, mudar, \
         executar) ajudaria, não a faça: no fim, depois de uma linha 'PROPOSTAS:', liste até \
         {MAX_PROPOSTAS} propostas, uma por linha, começando com '- ', cada uma como um pedido \
         completo que o Antônio poderá aceitar ou descartar."
    )
}

/// Pedido recusado (a mensagem é segura de mostrar).
#[derive(Debug, Clone, thiserror::Error)]
#[error("{mensagem}")]
pub struct TarefaBgError {
    pub mensagem: String,
    pub status: u16,
}

fn recusa<T>(m: impl Into<String>, status: u16) -> Result<T, TarefaBgError> {
    Err(TarefaBgError { mensagem: m.into(), status })
}

impl From<orion_store::StoreError> for TarefaBgError {
    fn from(e: orion_store::StoreError) -> Self {
        TarefaBgError { mensagem: e.to_string(), status: 500 }
    }
}

/// (relatório sem a seção, propostas). Só linhas '- ...'; texto é dado, nunca vira ação.
pub fn separar_propostas(texto: &str) -> (String, Vec<Value>) {
    let Some(m) = PROPOSTAS.find(texto) else { return (texto.trim().into(), vec![]) };
    let corpo = texto[..m.start()].trim().to_string();
    let itens: Vec<Value> = texto[m.end()..]
        .lines()
        .map(str::trim)
        .filter_map(|l| l.strip_prefix("- ").map(str::trim).filter(|r| !r.is_empty()))
        .map(|r| json!({"texto": r.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_PROPOSTA).collect::<String>(), "estado": "pendente"}))
        .take(MAX_PROPOSTAS)
        .collect();
    (corpo, itens)
}

pub struct TarefasBg {
    memory: Arc<Store>,
    ops: Arc<Operations>,
    agent: Arc<Agent>,
    custos: Option<Arc<Custos>>,
    modos: Option<Arc<Modos>>,
    tarefas: Mutex<HashMap<i64, JoinHandle<()>>>,
    drenagem: Mutex<()>,
}

fn linha(r: &serde_json::Map<String, Value>) -> Value {
    let mut d = r.clone();
    for k in ["budget", "proposals"] {
        let v = d.get(k).and_then(Value::as_str).and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(Value::Null);
        d.insert(k.into(), v);
    }
    Value::Object(d)
}

impl TarefasBg {
    pub fn new(memory: Arc<Store>, ops: Arc<Operations>, agent: Arc<Agent>, custos: Option<Arc<Custos>>, modos: Option<Arc<Modos>>) -> Self {
        TarefasBg { memory, ops, agent, custos, modos, tarefas: Mutex::new(HashMap::new()), drenagem: Mutex::new(()) }
    }

    pub fn criar(self: &Arc<Self>, objetivo: &str, max_passos: i64, max_min: i64) -> Result<Value, TarefaBgError> {
        let objetivo = objetivo.split_whitespace().collect::<Vec<_>>().join(" ");
        if objetivo.is_empty() || objetivo.chars().count() > MAX_OBJETIVO {
            return recusa(format!("o objetivo tem de ter de 1 a {MAX_OBJETIVO} caracteres"), 422);
        }
        if !(1..=MAX_PASSOS).contains(&max_passos) || !(1..=MAX_MINUTOS).contains(&max_min) {
            return recusa(format!("passos de 1 a {MAX_PASSOS} e minutos de 1 a {MAX_MINUTOS}"), 422);
        }
        if self.modos.as_ref().is_some_and(|m| m.panico()) {
            return recusa("modo pânico: a tarefa em segundo plano está cortada", 409);
        }
        if self.custos.as_ref().is_some_and(|c| !c.pode_usar("gateway", true, "a tarefa em segundo plano")) {
            return recusa("cota gratuita do dia quase no fim: tente amanhã", 429);
        }
        let orcamento = json!({"max_passos": max_passos, "max_min": max_min}).to_string();
        let agora = self.memory.now();
        // com as vagas cheias a tarefa entra na fila e começa quando outra terminar (F04.21)
        let tid = self.memory.tx(|c| {
            c.execute(
                "INSERT INTO background_tasks(objective, budget, status, created_at) VALUES (?,?,?,?)",
                orion_store::rusqlite::params![objetivo, orcamento, "na_fila", agora],
            )?;
            Ok(c.last_insert_rowid())
        })?;
        self.proxima();
        self.obter(tid)
    }

    /// Ao abrir o Orion: o que estava rodando quando ele fechou fica `interrompida` (não recomeça
    /// sozinho no meio) e a fila volta a andar.
    pub fn retomar_fila(self: &Arc<Self>) {
        let agora = self.memory.now();
        let r = self.memory.tx(|c| {
            c.execute("UPDATE background_tasks SET status='interrompida', finished_at=? WHERE status='rodando'", orion_store::rusqlite::params![agora])?;
            Ok(())
        });
        if let Err(e) = r {
            tracing::error!("fila de tarefas em segundo plano não retomou: {e}");
            return;
        }
        self.proxima();
    }

    fn rodando(&self) -> usize {
        self.tarefas.lock().values().filter(|t| !t.is_finished()).count()
    }

    /// Começa as tarefas da fila enquanto houver vaga, a mais antiga primeiro. Uma drenagem por vez
    /// (a vaga é conferida e ocupada sob a mesma trava) e nada sai em modo pânico.
    fn proxima(self: &Arc<Self>) {
        let _drenando = self.drenagem.lock();
        while self.rodando() < MAX_PARALELAS {
            if self.modos.as_ref().is_some_and(|m| m.panico()) {
                return;
            }
            // a cota pode ter subido desde que a tarefa entrou na fila: job opcional para em 90%
            if self.custos.as_ref().is_some_and(|c| !c.pode_usar("gateway", true, "a tarefa em segundo plano")) {
                return;
            }
            let Ok(rows) = self.memory.query("SELECT * FROM background_tasks WHERE status='na_fila' ORDER BY id LIMIT 1", &[]) else { return };
            let Some(r) = rows.first() else { return };
            let tid = r.get("id").and_then(Value::as_i64).unwrap_or(0);
            let objetivo = r.get("objective").and_then(Value::as_str).unwrap_or("").to_string();
            let orc: Value = r.get("budget").and_then(Value::as_str).and_then(|b| serde_json::from_str(b).ok()).unwrap_or_default();
            let (max_passos, max_min) = (orc["max_passos"].as_i64().unwrap_or(MAX_PASSOS), orc["max_min"].as_i64().unwrap_or(MAX_MINUTOS));
            let marcou = self.memory.tx(|c| Ok(c.execute("UPDATE background_tasks SET status='rodando' WHERE id=? AND status='na_fila'", orion_store::rusqlite::params![tid])?));
            if !matches!(marcou, Ok(1)) {
                return;
            }
            let me = self.clone();
            let h = tokio::spawn(async move {
                me.executar(tid, objetivo, max_passos, max_min).await;
                // libera a vaga antes de olhar a fila: o próprio handle ainda não terminou aqui
                me.tarefas.lock().remove(&tid);
                me.proxima();
            });
            self.tarefas.lock().insert(tid, h);
        }
    }

    async fn executar(&self, tid: i64, objetivo: String, max_passos: i64, max_min: i64) {
        let mut texto = String::new();
        let mut passos = 0i64;
        let mut status = "concluida";
        let consumir = async {
            // um canal por tarefa: com várias rodando juntas, cada uma tem a sua conversa
            let canal = format!("{CANAL}:{tid}");
            let _ = self.memory.new_session(&canal, Some(&format!("Tarefa: {objetivo}").chars().take(120).collect::<String>()), None);
            let req = TurnRequest { channel: canal, text: prompt(&objetivo), read_only: true, ..Default::default() };
            let mut eventos = self.agent.run(req);
            while let Some(ev) = eventos.next().await {
                match ev.kind {
                    "text" => texto += ev.data["text"].as_str().unwrap_or(""),
                    "tool" if matches!(ev.data.get("decision").and_then(Value::as_str), None | Some("allow")) => {
                        passos += 1;
                        if passos > max_passos {
                            status = "estourou_passos";
                            return;
                        }
                    }
                    "error" => {
                        texto += &format!("\n(erro: {})", ev.data["message"].as_str().unwrap_or("?"));
                        status = "falhou";
                    }
                    _ => {}
                }
            }
        };
        if tokio::time::timeout(std::time::Duration::from_secs((max_min * 60) as u64), consumir).await.is_err() {
            status = "estourou_tempo";
        }
        self.fechar(tid, &objetivo, &texto, status);
    }

    fn fechar(&self, tid: i64, objetivo: &str, texto: &str, status: &str) {
        let (relatorio, propostas) = separar_propostas(texto);
        let rel: String = relatorio.chars().take(MAX_RELATORIO).collect();
        let rel = if rel.is_empty() { "(sem relatório)".to_string() } else { rel };
        let agora = self.memory.now();
        let r = self.memory.tx(|c| {
            c.execute(
                "UPDATE background_tasks SET status=?, report=?, proposals=?, finished_at=? WHERE id=? AND status='rodando'",
                orion_store::rusqlite::params![status, rel, json!(propostas).to_string(), agora, tid],
            )?;
            Ok(())
        });
        if let Err(e) = r {
            tracing::error!("tarefa em segundo plano {tid} não fechou: {e}");
        }
        let _ = self.ops.notify(
            "tarefa_bg",
            &format!("Tarefa em segundo plano ({}): {}", status.replace('_', " "), objetivo.chars().take(80).collect::<String>()),
            Some(&format!("bg:{tid}")),
            false,
        );
    }

    pub fn obter(&self, tid: i64) -> Result<Value, TarefaBgError> {
        let rows = self.memory.query("SELECT * FROM background_tasks WHERE id=?", &[&tid])?;
        match rows.first() {
            Some(r) => Ok(linha(r)),
            None => recusa("tarefa não encontrada", 404),
        }
    }

    pub fn listar(&self, limite: i64) -> Result<Vec<Value>, TarefaBgError> {
        Ok(self.memory.query("SELECT * FROM background_tasks ORDER BY id DESC LIMIT ?", &[&limite.clamp(1, 100)])?.iter().map(linha).collect())
    }

    pub fn cancelar(self: &Arc<Self>, tid: i64) -> Result<Value, TarefaBgError> {
        let na_fila = self.obter(tid)?["status"] == "na_fila";
        if !na_fila {
            // abortada, a tarefa nunca chega ao fim que libera a vaga: tira o handle aqui mesmo
            let mut g = self.tarefas.lock();
            match g.remove(&tid) {
                Some(t) if !t.is_finished() => t.abort(),
                _ => return recusa("a tarefa não está rodando", 409),
            }
        }
        let agora = self.memory.now();
        self.memory.tx(|c| {
            c.execute("UPDATE background_tasks SET status='cancelada', finished_at=? WHERE id=? AND status IN ('rodando','na_fila')", orion_store::rusqlite::params![agora, tid])?;
            Ok(())
        })?;
        self.proxima();
        self.obter(tid)
    }

    /// `aceitar` devolve o texto para o campo de mensagem (a política vale lá); `descartar` só
    /// marca. Nada é executado aqui.
    pub fn decidir(&self, tid: i64, indice: i64, acao: &str) -> Result<Value, TarefaBgError> {
        if !["aceitar", "descartar"].contains(&acao) {
            return recusa("ação inválida", 422);
        }
        let tarefa = self.obter(tid)?;
        let mut propostas = tarefa["proposals"].as_array().cloned().unwrap_or_default();
        if indice < 0 || indice as usize >= propostas.len() {
            return recusa("proposta não encontrada", 404);
        }
        let i = indice as usize;
        if propostas[i]["estado"] != "pendente" {
            return recusa("essa proposta já foi decidida", 409);
        }
        propostas[i]["estado"] = json!(if acao == "aceitar" { "aceita" } else { "descartada" });
        let novo = json!(propostas).to_string();
        self.memory.tx(|c| {
            c.execute("UPDATE background_tasks SET proposals=? WHERE id=?", orion_store::rusqlite::params![novo, tid])?;
            Ok(())
        })?;
        Ok(json!({"ok": true, "texto": if acao == "aceitar" { propostas[i]["texto"].clone() } else { Value::Null }}))
    }

    pub fn encerrar(&self) {
        for t in self.tarefas.lock().values() {
            t.abort();
        }
    }
}

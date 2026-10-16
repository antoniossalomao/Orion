//! Pesquisa noturna supervisionada (REGRAS.md: pesquisa noturna só lê, sem aprovação): um
//! relatório por dia na caixa de entrada do vault.
//!
//! O Antônio escolhe os assuntos (`ORION_RESEARCH_TOPICS`) e a hora (`ORION_RESEARCH_AT`); o modelo
//! não escolhe nada disso. Cada assunto roda num turno SÓ LEITURA, numa conversa própria. O
//! resultado é texto de terceiros resumido por um modelo: vai marcado como não confiável e só para
//! a caixa de entrada do vault, que o Antônio tria. Nada é executado nem salvo na memória.

use crate::agent::{Agent, TurnRequest};
use crate::capture::Capturer;
use chrono::Timelike;
use futures::StreamExt;
use orion_store::Store;
use regex::Regex;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

pub const MAX_ASSUNTOS: usize = 5;
pub const MAX_ASSUNTO: usize = 200;
pub const JANELA_H: f64 = 6.0;
pub const TOPICS_KEY: &str = "research:topics";
static RELATORIO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2} \d{4} - pesquisa-noturna.*\.md$").unwrap());

fn prompt(assunto: &str) -> String {
    format!("Pesquise na internet sobre: {assunto}\nUse só ferramentas de leitura. Entregue um resumo de até 15 linhas, em português, com o que há de novo e os links das fontes. Não execute nada e não salve nada.")
}

pub fn parse_assuntos(bruto: &str) -> Vec<String> {
    bruto.split(';').map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_ASSUNTO).collect::<String>()).filter(|i| !i.is_empty()).take(MAX_ASSUNTOS).collect()
}

/// Assuntos vindos da interface: até 5, 1 a 200 caracteres cada, sem `;`. Nada é cortado em silêncio.
pub fn validar_assuntos(itens: &[String]) -> Result<Vec<String>, String> {
    let limpos: Vec<String> = itens.iter().map(|i| i.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    if limpos.len() > MAX_ASSUNTOS {
        return Err(format!("no máximo {MAX_ASSUNTOS} assuntos"));
    }
    for i in &limpos {
        if i.is_empty() {
            return Err("assunto vazio".into());
        }
        if i.chars().count() > MAX_ASSUNTO {
            return Err(format!("assunto passa de {MAX_ASSUNTO} caracteres"));
        }
        if i.contains(';') {
            return Err("assunto não pode ter ';'".into());
        }
    }
    let mut vistos = vec![];
    for i in limpos {
        if !vistos.contains(&i) {
            vistos.push(i);
        }
    }
    Ok(vistos)
}

/// Assuntos em vigor e de onde vieram: `interface` (guardados em `meta`) ou `padrao`.
pub fn assuntos_salvos(memory: &Store, padrao: &str) -> (Vec<String>, &'static str) {
    if let Some(bruto) = memory.meta_get(TOPICS_KEY) {
        if let Ok(lista) = serde_json::from_str::<Vec<String>>(&bruto) {
            if let Ok(v) = validar_assuntos(&lista) {
                return (v, "interface");
            }
        }
        tracing::warn!("assuntos da pesquisa guardados são inválidos; uso o padrão");
    }
    (parse_assuntos(padrao), "padrao")
}

pub fn salvar_assuntos(memory: &Store, itens: &[String]) -> Result<Vec<String>, String> {
    let limpos = validar_assuntos(itens)?;
    memory.meta_set(TOPICS_KEY, Some(&serde_json::to_string(&limpos).unwrap_or_default())).map_err(|e| e.to_string())?;
    Ok(limpos)
}

pub struct NightResearch {
    memory: Arc<Store>,
    agent: Arc<Agent>,
    cap: Capturer,
    topicos_padrao: String,
    hm: Option<(u32, u32)>,
    pub rodando: AtomicBool,
}

impl NightResearch {
    pub fn new(memory: Arc<Store>, agent: Arc<Agent>, cap: Capturer, topicos_padrao: &str, at: &str) -> Result<Self, String> {
        let hm = if at.trim().is_empty() {
            None // sem hora: só roda quando você manda (`rodar agora`)
        } else {
            let erro = || format!("hora da pesquisa inválida: {at:?} (use HH:MM)");
            let (h, m) = at.split_once(':').ok_or_else(erro)?;
            let (h, m): (u32, u32) = (h.trim().parse().map_err(|_| erro())?, m.trim().parse().map_err(|_| erro())?);
            if h >= 24 || m >= 60 {
                return Err(erro());
            }
            Some((h, m))
        };
        Ok(NightResearch { memory, agent, cap, topicos_padrao: topicos_padrao.into(), hm, rodando: AtomicBool::new(false) })
    }

    /// Lidos a cada rodada: o que você editou na interface vale na próxima.
    pub fn assuntos(&self) -> Vec<String> {
        assuntos_salvos(&self.memory, &self.topicos_padrao).0
    }

    pub fn esta_rodando(&self) -> bool {
        self.rodando.load(Ordering::SeqCst)
    }

    /// Relatórios da pesquisa na caixa de entrada do vault, do mais novo ao mais antigo.
    pub fn relatorios(&self, limite: usize) -> Vec<Value> {
        let Ok(pasta) = self.cap.pasta() else { return vec![] };
        let Ok(rd) = std::fs::read_dir(&pasta) else { return vec![] };
        let mut notas: Vec<_> = rd.flatten().map(|e| e.path()).filter(|p| p.is_file() && p.file_name().is_some_and(|n| RELATORIO.is_match(&n.to_string_lossy()))).collect();
        notas.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        notas
            .iter()
            .take(limite.clamp(1, 100))
            .map(|p| {
                let nome = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                json!({
                    "arquivo": self.cap.relativo(p), "nome": p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                    "data": nome.chars().take(10).collect::<String>(), "bytes": std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
                })
            })
            .collect()
    }

    pub fn devida(&self) -> bool {
        let Some((h, m)) = self.hm else { return false };
        let dt = crate::util::local(crate::util::agora());
        let Some(alvo) = dt.with_hour(h).and_then(|d| d.with_minute(m)).and_then(|d| d.with_second(0)) else { return false };
        if dt < alvo || (dt - alvo).num_seconds() as f64 > JANELA_H * 3600.0 {
            return false;
        }
        self.memory.counter_get("pesquisa:ultimo") < dt.format("%Y%m%d").to_string().parse::<i64>().unwrap_or(0)
    }

    /// Roda os assuntos e grava UM relatório. Devolve o caminho no vault. `marcar=false` (rodar
    /// agora, pela interface) não gasta a rodada da noite.
    pub async fn run(&self, marcar: bool) -> Option<String> {
        let assuntos = self.assuntos();
        if assuntos.is_empty() || self.rodando.swap(true, Ordering::SeqCst) {
            return None;
        }
        let r = self.rodar(&assuntos, marcar).await;
        self.rodando.store(false, Ordering::SeqCst);
        r
    }

    async fn rodar(&self, assuntos: &[String], marcar: bool) -> Option<String> {
        if marcar {
            // marca antes: se falhar no meio, não repete a rodada inteira a cada tick
            let hoje = crate::util::local(crate::util::agora()).format("%Y%m%d").to_string().parse().unwrap_or(0);
            let _ = self.memory.counter_set("pesquisa:ultimo", hoje);
        }
        let mut secoes = vec![];
        for assunto in assuntos {
            let _ = self.memory.new_session("pesquisa", Some(&format!("Pesquisa: {assunto}").chars().take(120).collect::<String>()), None);
            let mut texto = String::new();
            let mut s = self.agent.run(TurnRequest { channel: "pesquisa".into(), text: prompt(assunto), read_only: true, ..Default::default() });
            while let Some(ev) = s.next().await {
                match ev.kind {
                    "text" => texto += ev.data["text"].as_str().unwrap_or(""),
                    "error" => texto += &format!("\n(erro: {})", ev.data["message"].as_str().unwrap_or("?")),
                    _ => {}
                }
            }
            let t = texto.trim();
            secoes.push(format!("## {assunto}\n\n{}", if t.is_empty() { "(sem resposta)" } else { t }));
        }
        let corpo = format!("> [!warning] Conteúdo de fontes externas, resumido por um modelo: confira antes de confiar. Gerado pela pesquisa noturna do Orion (só leitura).\n\n{}", secoes.join("\n\n"));
        tracing::info!(target: "orion.audit", "pesquisa_noturna assuntos={} manual={}", assuntos.len(), !marcar);
        match self.cap.save_text(&format!("# Pesquisa noturna\n\n{corpo}"), "pesquisa", "orion", crate::capture::MAX_TEXTO * 10) {
            Ok(n) => Some(self.cap.relativo(&n)),
            Err(e) => {
                tracing::error!("pesquisa noturna não gravou: {e}");
                None
            }
        }
    }
}

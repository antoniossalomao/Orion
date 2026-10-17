//! Paridade com `tests/test_jobs.py` e a parte de jobs de `tests/test_costs.py` (Python): lembretes
//! e agendamentos viram aviso, backup, vault, embeddings, consolidação, cota e saúde do gateway.
//!
//! Sem equivalente: o passo que quebra por `monkeypatch` (sem ponto de injeção) e o laço
//! `run_forever` sobrevivendo a uma rodada que falha (só o cancelamento limpo é testado).

use chrono::TimeZone;
use futures::future::BoxFuture;
use orion_ai::costs::{Cota, Custos};
use orion_core::jobs::{JobRunner, JobsConfig};
use orion_core::memory::consolidate::Consolidator;
use orion_core::memory::Completer;
use orion_store::ops::{Operations, When};
use orion_store::search::Kind;
use orion_store::Store;
use orion_testkit::embed::FakeEmbedder;
use orion_testkit::Relogio;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

fn em(a: i32, m: u32, d: u32, h: u32, mi: u32) -> f64 {
    chrono::Local.with_ymd_and_hms(a, m, d, h, mi, 0).earliest().unwrap().timestamp() as f64
}

fn agora() -> f64 {
    em(2026, 10, 3, 12, 0)
}

struct Amb {
    store: Arc<Store>,
    ops: Arc<Operations>,
    rel: Relogio,
    d: tempfile::TempDir,
}

fn amb() -> Amb {
    let d = tempfile::tempdir().unwrap();
    let rel = Relogio::em(agora());
    let store = Arc::new(Store::open_with(d.path().join("j.db"), None, rel.clock()).unwrap());
    let ops = Arc::new(Operations::new(store.clone()));
    Amb { store, ops, rel, d }
}

fn runner(a: &Amb, cfg: JobsConfig) -> Arc<JobRunner> {
    Arc::new(JobRunner::new(a.store.clone(), a.ops.clone(), cfg).unwrap())
}

fn txt(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}

#[tokio::test]
async fn lembrete_vencido_vira_um_aviso_uma_vez_so() {
    let a = amb();
    a.ops.add_reminder("Pagar boleto", &"2026-10-03T11:00:00".into(), "banco", 0).unwrap();
    a.ops.add_reminder("Dentista", &"2026-10-05T09:00:00".into(), "", 0).unwrap();
    let j = runner(&a, JobsConfig::default());
    let rel = j.tick().await;
    assert!(rel.lembretes == 1 && rel.erros.is_empty(), "{rel:?}");
    let avisos = a.ops.pending_notifications(50, false).unwrap();
    assert_eq!(avisos.iter().map(|x| txt(x, "kind")).collect::<Vec<_>>(), ["lembrete"]);
    assert_eq!(txt(&avisos[0], "text"), "Lembrete (03/10 11:00): Pagar boleto — banco");
    assert!(txt(&avisos[0], "ref").starts_with("reminder:"));
    assert!(j.tick().await.lembretes == 0 && a.ops.pending_notifications(50, false).unwrap().len() == 1);
    a.rel.set(em(2026, 10, 5, 9, 1));
    assert_eq!(j.tick().await.lembretes, 1);
}

#[tokio::test]
async fn agendamento_avisa_recalcula_e_nao_executa_a_ferramenta() {
    let a = amb();
    let s = a.ops.add_schedule("Briefing", "diario", None, "14:00", 0, "executar_comando", None).unwrap();
    let j = runner(&a, JobsConfig::default());
    assert_eq!(j.tick().await.agendamentos, 0);
    a.rel.set(em(2026, 10, 3, 14, 1));
    assert_eq!(j.tick().await.agendamentos, 1);
    let aviso = &a.ops.pending_notifications(50, false).unwrap()[0];
    assert!(txt(aviso, "kind") == "agendamento" && txt(aviso, "ref").starts_with(&format!("schedule:{}:", s["id"])));
    assert!(txt(aviso, "text").contains("não é executada sozinha")); // só registrada: nunca roda sem aprovação
    assert_eq!(a.ops.list_schedules().unwrap()[0]["next_run"].as_f64().unwrap(), em(2026, 10, 4, 14, 0));
    assert_eq!(j.tick().await.agendamentos, 0);
}

#[tokio::test]
async fn embeddings_pendentes_sao_refeitos_com_pausa_entre_rodadas() {
    let d = tempfile::tempdir().unwrap();
    let rel = Relogio::em(agora());
    let emb = FakeEmbedder::novo();
    let store = Arc::new(Store::open_with(d.path().join("e.db"), Some(emb.clone()), rel.clock()).unwrap());
    let ops = Arc::new(Operations::new(store.clone()));
    emb.quebrar(true);
    store.add_fact("Antônio mora em Marília", "t", None).unwrap(); // embedding adiado
    emb.quebrar(false);
    let j = Arc::new(JobRunner::new(store.clone(), ops, JobsConfig::default()).unwrap());
    assert_eq!(j.tick().await.embeddings, 1);
    store.conn().execute("UPDATE facts SET embedded=0", []).unwrap();
    j.tick().await;
    let n = emb.chamadas();
    assert!(n >= 1);
    j.tick().await;
    assert_eq!(emb.chamadas(), n); // dentro da janela de 5 min: não repete
    rel.avanca(301.0);
    store.conn().execute("UPDATE facts SET embedded=0", []).unwrap();
    j.tick().await;
    assert!(emb.chamadas() > n);
}

#[tokio::test]
async fn backup_diario_com_poda_e_sem_repetir_no_dia() {
    let a = amb();
    a.ops.add_task("coisa a guardar").unwrap();
    let destino = a.d.path().join("nuvem").join("backups");
    let cfg = JobsConfig { backup_dir: Some(destino.clone()), backup_keep: 2, ..Default::default() };
    let j = runner(&a, cfg);
    let rel = j.tick().await;
    let bk = rel.backup.expect("o primeiro tick faz o backup");
    assert!(Path::new(&bk).exists());
    let contagens: std::collections::HashMap<_, _> = Store::verify_backup(Path::new(&bk)).unwrap().into_iter().collect();
    assert_eq!(contagens["tasks"], 1); // o backup restaura e tem os dados
    a.rel.avanca(3601.0);
    assert!(j.tick().await.backup.is_none()); // já existe o de hoje
    for _ in 0..3 {
        a.rel.avanca(86_400.0);
        j.tick().await;
    }
    let n = std::fs::read_dir(&destino).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with("orion-")).count();
    assert_eq!(n, 2); // poda
}

#[tokio::test]
async fn sem_pasta_de_backup_nao_faz_backup() {
    let a = amb();
    assert!(runner(&a, JobsConfig::default()).tick().await.backup.is_none());
}

#[tokio::test]
async fn vault_e_indexado_e_reindexado_so_quando_devido() {
    let a = amb();
    let vault = a.d.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("Orion.md"), "# Orion\nAssistente pessoal do Antônio").unwrap();
    let j = runner(&a, JobsConfig { vault_dir: Some(vault.clone()), ..Default::default() });
    assert_eq!(j.tick().await.vault.unwrap()["new"], 1);
    assert!(!a.store.search("assistente pessoal", 8, &[Kind::Chunk]).unwrap().is_empty());
    std::fs::write(vault.join("Nova.md"), "nota nova").unwrap();
    assert!(j.tick().await.vault.is_none()); // ainda dentro da hora
    a.rel.avanca(3601.0);
    assert_eq!(j.tick().await.vault.unwrap()["new"], 1);
}

struct Modelo(Mutex<VecDeque<Result<String, String>>>);
impl Completer for Modelo {
    fn complete(&self, _: Vec<Value>) -> BoxFuture<'_, anyhow::Result<String>> {
        Box::pin(async move { self.0.lock().unwrap().pop_front().unwrap().map_err(|e| anyhow::anyhow!(e)) })
    }
}

#[tokio::test]
async fn consolidacao_roda_na_cadencia_e_falha_tenta_de_novo_mais_cedo() {
    let a = amb();
    let s = a.store.new_session("telegram", None, None).unwrap();
    for i in 0..6 {
        a.store.add_message(&s.id, "user", &format!("fala {i}"), None, None).unwrap();
    }
    let modelo = Arc::new(Modelo(Mutex::new(VecDeque::from([Err("fora".to_string()), Ok(json!({"fatos": ["Antônio mora em Marília-SP"]}).to_string())]))));
    let cons = Arc::new(Consolidator::new(a.store.clone(), modelo));
    let j = Arc::new(JobRunner::new(a.store.clone(), a.ops.clone(), JobsConfig::default()).unwrap().com_consolidator(cons));
    let rel = j.tick().await;
    assert_eq!(rel.consolidacao, Some(json!({"ran": true, "falas": 6, "fatos": 0}))); // falhou: nada gravado
    assert!(a.store.facts().unwrap().is_empty());
    // falha: nova tentativa em 20% do intervalo de 6 h (4320 s), não no ciclo inteiro
    a.rel.avanca(150.0);
    assert!(j.tick().await.consolidacao.is_none()); // cedo demais
    a.rel.avanca(4300.0);
    let rel = j.tick().await;
    assert!(rel.consolidacao.as_ref().is_some_and(|c| c["fatos"] == 1), "{rel:?}");
    assert_eq!(a.store.facts().unwrap().len(), 1);
}

#[tokio::test]
async fn job_opcional_e_adiado_pela_cota() {
    let a = amb();
    let custos = Arc::new(Custos::new(a.store.clone(), vec![("gateway".into(), Cota { provedor: "gateway:".into(), nome: "Gateway".into(), limite: 10, periodo: "dia" })], Some(a.ops.clone())));
    for _ in 0..10 {
        a.store
            .add_external_call(&orion_store::library::ExternalCall { provider: "gateway:padrão", kind: "chat", ok: true, latency_ms: 100, model: "", bytes_out: 0, bytes_in: 0, content_kind: "", ts: None })
            .unwrap();
    }
    let s = a.store.new_session("telegram", None, None).unwrap();
    for i in 0..6 {
        a.store.add_message(&s.id, "user", &format!("fala {i}"), None, None).unwrap();
    }
    let modelo = Arc::new(Modelo(Mutex::new(VecDeque::new())));
    let j = Arc::new(
        JobRunner::new(a.store.clone(), a.ops.clone(), JobsConfig::default())
            .unwrap()
            .com_consolidator(Arc::new(Consolidator::new(a.store.clone(), modelo)))
            .com_custos(custos),
    );
    let rel = j.tick().await; // com a cota em 100% o modelo nem é chamado (a fila vazia faria o teste cair)
    assert!(rel.consolidacao.is_none() && rel.adiados == ["a consolidação da memória"], "{rel:?}");
    let kinds: Vec<_> = a.ops.pending_notifications(50, false).unwrap().iter().map(|n| txt(n, "kind")).collect();
    assert_eq!(kinds, ["cota"]);
}

// ── gateway fora do ar (E1.4), de test_costs.py ─────────────────────────────
fn chamada(a: &Amb, provider: &str, ok: bool, ts: f64) {
    a.store
        .add_external_call(&orion_store::library::ExternalCall { provider, kind: "chat", ok, latency_ms: 100, model: "", bytes_out: 0, bytes_in: 0, content_kind: "", ts: Some(ts) })
        .unwrap();
}

#[tokio::test]
async fn aviso_de_gateway_fora_do_ar_e_de_volta() {
    let a = amb();
    let j = runner(&a, JobsConfig::default());
    chamada(&a, "gateway:padrão", true, agora() - 3600.0); // funcionou há 1 h
    for min in [12.0, 8.0, 3.0] {
        // 3 falhas seguidas, a primeira há 12 min
        chamada(&a, "gateway:padrão", false, agora() - min * 60.0);
    }
    assert_eq!(j.tick().await.gateway.as_deref(), Some("fora"));
    let avisos = a.ops.pending_notifications(50, false).unwrap();
    assert_eq!(avisos.len(), 1);
    assert!(txt(&avisos[0], "kind") == "gateway" && txt(&avisos[0], "text").contains("fora do ar desde"));
    assert_eq!(avisos[0]["urgent"], 0); // o não perturbe segura este aviso
    a.rel.avanca(61.0);
    assert!(j.tick().await.gateway.is_none()); // um aviso só
    a.rel.avanca(60.0);
    chamada(&a, "gateway:padrão", true, a.rel.get()); // voltou
    assert_eq!(j.tick().await.gateway.as_deref(), Some("voltou"));
    assert!(txt(a.ops.pending_notifications(50, false).unwrap().last().unwrap(), "text").contains("de volta"));
}

#[tokio::test]
async fn falhas_recentes_demais_ou_poucas_nao_avisam() {
    let a = amb();
    let j = runner(&a, JobsConfig::default());
    for min in [5.0, 3.0, 1.0] {
        // 3 falhas, mas a primeira há só 5 min
        chamada(&a, "gateway:padrão", false, agora() - min * 60.0);
    }
    assert!(j.tick().await.gateway.is_none());
    chamada(&a, "ollama", true, agora()); // o modelo local respondeu: o gateway continua fora
    a.rel.avanca(15.0 * 60.0);
    assert_eq!(j.tick().await.gateway.as_deref(), Some("fora"));
}

#[tokio::test]
async fn briefing_sai_uma_vez_por_dia_dentro_da_janela() {
    let a = amb();
    a.rel.set(em(2026, 10, 7, 8, 30)); // quarta-feira
    let j = runner(&a, JobsConfig { briefing_at: "08:00".into(), ..Default::default() });
    assert!(j.tick().await.briefing);
    assert!(!j.tick().await.briefing); // já saiu hoje
    let avisos: Vec<_> = a.ops.pending_notifications(50, false).unwrap().iter().map(|n| txt(n, "kind")).collect();
    assert_eq!(avisos, ["briefing"]);
    a.rel.set(em(2026, 10, 8, 23, 0)); // passou da janela de 6 h: o dia fica sem
    assert!(!j.tick().await.briefing);
}

#[test]
fn hora_do_briefing_invalida_recusa_na_subida() {
    let a = amb();
    assert!(JobRunner::new(a.store.clone(), a.ops.clone(), JobsConfig { briefing_at: "25:00".into(), ..Default::default() }).is_err());
    assert!(JobRunner::new(a.store.clone(), a.ops.clone(), JobsConfig { briefing_at: "".into(), ..Default::default() }).is_ok());
}

#[tokio::test]
async fn laco_cancela_limpo() {
    let a = amb();
    let tarefa = tokio::spawn(runner(&a, JobsConfig::default()).run_forever(0.01));
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    tarefa.abort();
    assert!(tarefa.await.unwrap_err().is_cancelled());
    let _ = When::Epoch(0.0);
}

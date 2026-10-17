//! Paridade com `tests/test_modos.py` (Python): modo pânico (corte que não volta sozinho) e não
//! perturbe (fila que só espera).
//!
//! Fora daqui: o agente sem ferramentas em pânico, as rotas `/modo/*`, o `/panico` do Telegram, a
//! CLI e o texto do painel (`orion-core` agente/`orion-server`); a memória da tela e o sono em
//! pânico exigem os objetos reais desses passos (cobertos no teste de jobs de embeddings).

use chrono::TimeZone;
use orion_core::jobs::{JobRunner, JobsConfig};
use orion_core::modos::{na_janela, proxima, Modos};
use orion_store::ops::Operations;
use orion_store::Store;
use orion_testkit::embed::FakeEmbedder;
use orion_testkit::Relogio;
use std::sync::Arc;

fn em(d: u32, h: u32, m: u32) -> f64 {
    chrono::Local.with_ymd_and_hms(2026, 10, d, h, m, 0).earliest().unwrap().timestamp() as f64
}

struct Mundo {
    rel: Relogio,
    store: Arc<Store>,
    ops: Arc<Operations>,
    modos: Arc<Modos>,
    _d: tempfile::TempDir,
}

fn mundo(dnd_at: &str) -> Mundo {
    let d = tempfile::tempdir().unwrap();
    let rel = Relogio::em(em(9, 23, 0));
    let store = Arc::new(Store::open_with(d.path().join("m.db"), None, rel.clock()).unwrap());
    let ops = Arc::new(Operations::new(store.clone()));
    let o2 = ops.clone();
    let modos = Arc::new(Modos::new(store.clone(), dnd_at, Some(Arc::new(move |ev| o2.audit_add(ev).map(|_| ()).map_err(Into::into)))));
    Mundo { rel, store, ops, modos, _d: d }
}

#[test]
fn panico_liga_nao_volta_sozinho_e_o_audit_registra() {
    let m = mundo("");
    assert!(!m.modos.panico());
    assert!(m.modos.entrar_panico("cli").unwrap() && !m.modos.entrar_panico("cli").unwrap());
    m.rel.avanca(30.0 * 86400.0); // um mês depois: continua ligado
    assert!(m.modos.panico() && m.modos.estado()["panico_origem"] == "cli");
    assert!(m.modos.sair_panico("telegram").unwrap() && !m.modos.panico());
    let acoes: Vec<(String, String)> = m
        .ops
        .audit_recent(10, None, None, None)
        .unwrap()
        .iter()
        .rev()
        .map(|r| (r["tool"].as_str().unwrap().to_string(), r["action"].as_str().unwrap().to_string()))
        .collect();
    assert_eq!(acoes, [("modo_panico".to_string(), "entrar".to_string()), ("modo_panico".to_string(), "sair".to_string())]);
}

#[test]
fn sem_audit_nao_sai_do_panico_mas_entra_sempre() {
    let m = mundo("");
    let modos = Modos::new(m.store.clone(), "", Some(Arc::new(|_| anyhow::bail!("disco cheio"))));
    assert!(modos.entrar_panico("web").unwrap()); // cortar nunca é barrado
    assert!(modos.sair_panico("web").is_err());
    assert!(modos.panico());
}

#[test]
fn estado_corrompido_continua_em_panico() {
    let m = mundo("");
    m.store.meta_set("modo:panico", Some("{quebrado")).unwrap();
    assert!(m.modos.panico());
}

#[test]
fn janela_que_atravessa_a_meia_noite() {
    assert!(na_janela("22:30-07:00", em(9, 23, 0)) && na_janela("22:30-07:00", em(9, 6, 59)));
    assert!(!na_janela("22:30-07:00", em(9, 7, 0)) && !na_janela("22:30-07:00", em(9, 12, 0)));
    assert!(na_janela("13:00-14:00", em(9, 13, 30)) && !na_janela("", em(9, 13, 30)));
    assert_eq!(proxima("07:00", em(9, 23, 0)).unwrap(), em(10, 7, 0));
    assert!(proxima("25:00", em(9, 23, 0)).is_err());
}

#[test]
fn nao_perturbe_segura_os_nao_urgentes_e_entrega_depois() {
    let m = mundo("");
    let modos = m.modos.clone();
    *m.ops.segurar.write() = Arc::new(move || modos.nao_perturbe());
    m.ops.notify("gateway", "modelos fora do ar", None, false).unwrap();
    m.ops.notify("lembrete", "remédio", None, true).unwrap();
    let fim = m.modos.ligar_nao_perturbe("07:00").unwrap();
    assert_eq!(fim, em(10, 7, 0));
    let tipos = |todos: bool| m.ops.pending_notifications(50, todos).unwrap().iter().map(|n| n["kind"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(tipos(false), ["lembrete"]);
    assert_eq!(tipos(true).len(), 2); // nada descartado
    m.rel.set(fim + 1.0); // acabou: a fila sai
    assert_eq!(tipos(false), ["gateway", "lembrete"]);
}

#[test]
fn nao_perturbe_recusa_fim_no_passado_e_horario_ruim() {
    let m = mundo("");
    assert!(m.modos.ligar_nao_perturbe("25:00").is_err());
    assert!(m.modos.ligar_nao_perturbe("1.0").is_err()); // epoch no passado
    m.modos.ligar_nao_perturbe("23:59").unwrap();
    assert!(m.modos.estado()["nao_perturbe"] == true);
    m.modos.desligar_nao_perturbe().unwrap();
    assert!(m.modos.estado()["nao_perturbe_ate"].is_null());
}

#[tokio::test]
async fn panico_para_embeddings() {
    // os trechos vão ao Gemini: não em pânico
    let d = tempfile::tempdir().unwrap();
    let rel = Relogio::em(em(9, 23, 0));
    let emb = FakeEmbedder::novo();
    let store = Arc::new(Store::open_with(d.path().join("e.db"), Some(emb.clone()), rel.clock()).unwrap());
    let ops = Arc::new(Operations::new(store.clone()));
    emb.quebrar(true);
    store.add_fact("Antônio mora em Marília", "t", None).unwrap(); // embedding adiado
    emb.quebrar(false);
    let modos = Arc::new(Modos::new(store.clone(), "", None));
    let jobs = Arc::new(JobRunner::new(store.clone(), ops, JobsConfig::default()).unwrap().com_modos(modos.clone()));
    modos.entrar_panico("cli").unwrap();
    let rel1 = jobs.tick().await;
    assert!(emb.chamadas() == 0 && rel1.embeddings == 0, "embeddings em pânico");
    modos.sair_panico("cli").unwrap();
    rel.avanca(301.0); // a pausa de 5 min entre rodadas de embeddings já foi contada
    assert_eq!(jobs.tick().await.embeddings, 1);
}

#[tokio::test]
async fn lembrete_e_agendamento_sao_urgentes() {
    let m = mundo("");
    m.ops.add_reminder("remédio", &(m.rel.get() - 60.0).into(), "", 0).unwrap();
    let jobs = Arc::new(JobRunner::new(m.store.clone(), m.ops.clone(), JobsConfig::default()).unwrap());
    jobs.tick().await;
    assert_eq!(m.ops.pending_notifications(50, true).unwrap()[0]["urgent"], 1);
}

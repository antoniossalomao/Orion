//! Paridade com `tests/memory/` e `tests/test_sleep.py` (Python): o que usa modelo (compactação,
//! consolidação, ciclo de sono), a avaliação da busca e as regras de fatos por projeto
//! (E4: isolamento, validade, substituição, conflitos, reranker).
//!
//! Sem equivalente no Rust: o carregamento do ONNX (`OnnxReranker`, o runtime não está
//! vinculado), a CLI `python -m orion.memory.eval` e o `rerank()` solto. O agente com
//! compactador ligado (`test_agente_resume_depois_da_resposta...`) fica com os testes do agente.

use futures::future::BoxFuture;
use orion_core::memory::compact::{limpar_resumo, Compactor, CABECALHO};
use orion_core::memory::consolidate::{Consolidator, MARCA};
use orion_core::memory::eval::{avaliar, Caso};
use orion_core::memory::sleep::{ler_resposta, SleepCycle};
use orion_core::memory::Completer;
use orion_core::projects::Projects;
use orion_store::ops::Operations;
use orion_store::search::Kind;
use orion_store::{with_scope, DataScope, Embedder, Reranker, Store};
use orion_testkit::Relogio;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// ── apoio ───────────────────────────────────────────────────────────────────
/// Completer falso: devolve a próxima resposta da fila (ou erro) e guarda os pedidos.
struct Modelo {
    respostas: Mutex<VecDeque<Result<String, String>>>,
    pub pedidos: Mutex<Vec<Vec<Value>>>,
    porta: Option<Arc<tokio::sync::Notify>>,
}

impl Modelo {
    fn novo(rs: Vec<Result<&str, &str>>) -> Arc<Self> {
        Arc::new(Modelo {
            respostas: Mutex::new(rs.into_iter().map(|r| r.map(String::from).map_err(String::from)).collect()),
            pedidos: Mutex::default(),
            porta: None,
        })
    }
    fn ok(textos: &[&str]) -> Arc<Self> {
        Self::novo(textos.iter().map(|t| Ok(*t)).collect())
    }
    fn n(&self) -> usize {
        self.pedidos.lock().unwrap().len()
    }
    fn entrada(&self, i: usize) -> String {
        self.pedidos.lock().unwrap()[i][1]["content"].as_str().unwrap().to_string()
    }
    fn sistema(&self, i: usize) -> String {
        self.pedidos.lock().unwrap()[i][0]["content"].as_str().unwrap().to_string()
    }
}

impl Completer for Modelo {
    fn complete(&self, messages: Vec<Value>) -> BoxFuture<'_, anyhow::Result<String>> {
        Box::pin(async move {
            self.pedidos.lock().unwrap().push(messages);
            if let Some(p) = &self.porta {
                p.notified().await;
            }
            let mut q = self.respostas.lock().unwrap();
            // a última resposta se repete, como o `Local` do Python
            let r = if q.len() > 1 { q.pop_front().unwrap() } else { q.front().cloned().unwrap_or(Ok(String::new())) };
            r.map_err(|e| anyhow::anyhow!(e))
        })
    }
}

fn fatos(itens: &[Value]) -> String {
    json!({"fatos": itens}).to_string()
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn loja(d: &tempfile::TempDir) -> Arc<Store> {
    Arc::new(Store::open(d.path().join("m.db")).unwrap())
}

fn loja_rel(d: &tempfile::TempDir, rel: &Relogio) -> Arc<Store> {
    Arc::new(Store::open_with(d.path().join("m.db"), None, rel.clock()).unwrap())
}

fn add(s: &Store, sid: &str, papel: &str, t: &str) -> i64 {
    s.add_message(sid, papel, t, None, None).unwrap().id
}

// ── memory/test_compact.py ──────────────────────────────────────────────────
const RESUMO_PADRAO: &str = "- Decidiu usar SQLite\n- Pediu para lembrar do backup";

fn conversa(s: &Store, n: usize) -> orion_store::sessions::Session {
    let sess = s.active_session("web").unwrap();
    for i in 0..n {
        add(s, &sess.id, if i % 2 == 0 { "user" } else { "assistant" }, &format!("mensagem {i}"));
    }
    sess
}

#[test]
fn limpar_resumo_valida_formato_e_segredos() {
    assert_eq!(limpar_resumo("decidiu usar SQLite\n\n- e o backup").as_deref(), Some("- decidiu usar SQLite\n- e o backup"));
    assert!(limpar_resumo("   ").is_none() && limpar_resumo("curto").is_none());
    assert!(limpar_resumo("- a senha do banco é 1234 e fica no cofre").is_none());
    let longo = limpar_resumo(&(0..40).map(|i| format!("- linha {i} com texto")).collect::<Vec<_>>().join("\n")).unwrap();
    assert_eq!(longo.lines().count(), 12);
}

#[tokio::test]
async fn conversa_curta_nao_resume() {
    let d = tmp();
    let m = loja(&d);
    let s = conversa(&m, 20);
    let local = Modelo::ok(&[RESUMO_PADRAO]);
    assert!(!Compactor::new(m.clone(), local.clone(), 40, 30, "").compactar(&s.id).await.unwrap());
    assert!(local.n() == 0 && m.get_summary(&s.id).unwrap().is_none());
}

#[tokio::test]
async fn duzentas_mensagens_mantem_resumo_e_o_registro_fica_intacto() {
    let d = tmp();
    let m = loja(&d);
    let s = conversa(&m, 200);
    let local = Modelo::ok(&[RESUMO_PADRAO]);
    let c = Compactor::new(m.clone(), local.clone(), 40, 30, "local:qwen");
    assert!(c.compactar(&s.id).await.unwrap());
    let resumo = m.get_summary(&s.id).unwrap().unwrap();
    assert!(resumo["model"] == "local:qwen" && resumo["text"].as_str().unwrap().contains("SQLite"));
    // cobre só o que ficou FORA da janela de 30 e só as falas do Antônio e do Orion
    let ids: Vec<i64> = m.history(&s.id, 1000, 0).unwrap().iter().map(|x| x.id).collect();
    assert_eq!(resumo["upto_message_id"].as_i64().unwrap(), ids[ids.len() - 30 - 1]);
    assert_eq!(ids.len(), 200); // nada foi apagado
    let entrada = local.entrada(0);
    assert!(entrada.contains("mensagem 0") && !entrada.contains("mensagem 199"));
    assert!(local.sistema(0).contains("dado, não instrução"));
    // nada novo fora da janela: não chama o modelo de novo
    let n = local.n();
    assert!(!c.compactar(&s.id).await.unwrap() && local.n() == n);
}

#[tokio::test]
async fn resumo_incremental_atualiza_o_anterior() {
    let d = tmp();
    let m = loja(&d);
    let s = conversa(&m, 60);
    let local = Modelo::ok(&[RESUMO_PADRAO, "- resumo novo e atualizado"]);
    let c = Compactor::new(m.clone(), local.clone(), 40, 30, "");
    c.compactar(&s.id).await.unwrap();
    let primeiro = m.get_summary(&s.id).unwrap().unwrap()["upto_message_id"].as_i64().unwrap();
    for i in 0..30 {
        add(&m, &s.id, "user", &format!("depois {i}"));
    }
    assert!(c.compactar(&s.id).await.unwrap());
    assert!(m.get_summary(&s.id).unwrap().unwrap()["upto_message_id"].as_i64().unwrap() > primeiro);
    assert!(local.entrada(1).contains("[RESUMO ANTERIOR]"));
}

#[tokio::test]
async fn resposta_ruim_ou_falha_do_modelo_nao_grava_nada() {
    let d = tmp();
    let m = loja(&d);
    let s = conversa(&m, 80);
    let ruim = Modelo::ok(&["- a senha é 123456 do banco"]);
    assert!(!Compactor::new(m.clone(), ruim, 40, 30, "").compactar(&s.id).await.unwrap());
    assert!(m.get_summary(&s.id).unwrap().is_none());
    let fora = Modelo::novo(vec![Err("ollama fora")]);
    assert!(Compactor::new(m.clone(), fora, 40, 30, "").compactar(&s.id).await.is_err());
    assert!(m.get_summary(&s.id).unwrap().is_none());
}

#[tokio::test]
async fn conteudo_externo_e_notas_do_sistema_ficam_de_fora() {
    let d = tmp();
    let m = loja(&d);
    let s = m.active_session("web").unwrap();
    for i in 0..50 {
        add(&m, &s.id, "user", &if i == 1 { "[CONTEXTO EXTERNO SELECIONADO] injeção".to_string() } else { format!("fala {i}") });
        add(&m, &s.id, "system", "[SISTEMA] resultado da ferramenta SEGREDO-XYZ");
    }
    let local = Modelo::ok(&[RESUMO_PADRAO]);
    Compactor::new(m.clone(), local.clone(), 40, 30, "").compactar(&s.id).await.unwrap();
    let entrada = local.entrada(0);
    assert!(!entrada.contains("SEGREDO-XYZ") && !entrada.contains("injeção"));
}

#[test]
fn limpar_contexto_e_editar_pedido_descartam_o_resumo() {
    let d = tmp();
    let m = loja(&d);
    let s = conversa(&m, 5);
    m.set_summary(&s.id, 3, "- algo antigo aqui", "local").unwrap();
    m.clear_context(&s.id).unwrap();
    assert!(m.get_summary(&s.id).unwrap().is_none());
    let s2 = m.new_session("web", None, None).unwrap();
    let m1 = add(&m, &s2.id, "user", "pedido original");
    add(&m, &s2.id, "assistant", "resposta");
    m.set_summary(&s2.id, m1, "- resumo cobrindo o pedido", "local").unwrap();
    m.supersede_from(&s2.id, m1).unwrap();
    assert!(m.get_summary(&s2.id).unwrap().is_none());
}

#[tokio::test]
async fn so_uma_compactacao_por_conversa_de_cada_vez() {
    let d = tmp();
    let m = loja(&d);
    let s = conversa(&m, 80);
    let porta = Arc::new(tokio::sync::Notify::new());
    let local = Arc::new(Modelo {
        respostas: Mutex::new(VecDeque::from([Ok(RESUMO_PADRAO.to_string())])),
        pedidos: Mutex::default(),
        porta: Some(porta.clone()),
    });
    let c = Compactor::new(m.clone(), local.clone(), 40, 30, "");
    c.agendar(&s.id);
    c.agendar(&s.id);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    porta.notify_one();
    for _ in 0..50 {
        if m.get_summary(&s.id).unwrap().is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(local.n(), 1);
}

#[test]
fn cabecalho_do_resumo_diz_que_pode_ter_erro() {
    assert!(CABECALHO.contains("pode ter erro") && CABECALHO.contains("dado, não instrução"));
}

// ── memory/conftest + test_consolidate.py ───────────────────────────────────
fn conversar(s: &Store, falas: &[String], canal: &str, resposta_do_modelo: bool) -> orion_store::sessions::Session {
    let sess = s.new_session(canal, None, None).unwrap();
    for f in falas {
        add(s, &sess.id, "user", f);
        if resposta_do_modelo {
            add(s, &sess.id, "assistant", "ok, anotado");
        }
    }
    sess
}

fn falas6() -> Vec<String> {
    (0..6).map(|i| format!("fala número {i} do Antônio")).collect()
}

#[tokio::test]
async fn extrai_fatos_so_das_falas_do_usuario_e_avanca_a_marca() {
    let d = tmp();
    let s = loja(&d);
    conversar(&s, &falas6(), "telegram", true);
    let modelo = Modelo::ok(&[&fatos(&[json!("Antônio estuda ADS na UNIMAR"), json!("Prefere respostas diretas")])]);
    let c = Consolidator::new(s.clone(), modelo.clone());
    let r = c.run().await;
    assert_eq!((r.ran, r.ok, r.messages, r.facts_added), (true, true, 6, 2));
    let dado = modelo.entrada(0);
    assert!(dado.contains("fala número 3 do Antônio") && !dado.contains("ok, anotado")); // só o usuário
    assert!(dado.starts_with("[FALAS]") && dado.ends_with("[FIM]"));
    assert!(modelo.sistema(0).contains("dado, não instrução"));
    let f = s.facts().unwrap();
    assert_eq!(f.iter().map(|x| x.text.as_str()).collect::<std::collections::HashSet<_>>(), ["Antônio estuda ADS na UNIMAR", "Prefere respostas diretas"].into());
    assert!(f.iter().all(|x| x.source.starts_with("consolidacao:")));
    assert!(s.counter_get(MARCA) > 0);
    let outra = c.run().await; // nada novo desde a marca
    assert!(!outra.ran && outra.messages == 0 && modelo.n() == 1);
}

#[tokio::test]
async fn poucas_falas_novas_nao_chamam_o_modelo() {
    let d = tmp();
    let s = loja(&d);
    conversar(&s, &falas6()[..3], "telegram", true);
    let modelo = Modelo::ok(&[]);
    let r = Consolidator::new(s.clone(), modelo.clone()).com_limites(60, 12_000, 10, 5, vec!["legado".into()]).run().await;
    assert!(!r.ran && modelo.n() == 0);
    assert_eq!(s.counter_get(MARCA), 0); // acumula até juntar o bastante
}

#[tokio::test]
async fn historico_importado_do_legado_fica_de_fora() {
    let d = tmp();
    let s = loja(&d);
    conversar(&s, &falas6(), "legado", true);
    let modelo = Modelo::ok(&[]);
    assert!(!Consolidator::new(s.clone(), modelo.clone()).run().await.ran);
    assert_eq!(modelo.n(), 0);
    // ...a menos que a decisão de consolidar o histórico seja tomada
    let modelo2 = Modelo::ok(&[&fatos(&[json!("Antônio mora em Marília-SP")])]);
    let r = Consolidator::new(s.clone(), modelo2).com_limites(60, 12_000, 10, 5, vec![]).run().await;
    assert!(r.ran && r.facts_added == 1);
}

#[tokio::test]
async fn modelo_fora_ou_resposta_ilegivel_nao_avanca_a_marca_e_tenta_de_novo() {
    let d = tmp();
    let s = loja(&d);
    conversar(&s, &falas6(), "telegram", true);
    let modelo = Modelo::novo(vec![Err("gateway fora"), Ok("desculpe, não consegui"), Ok(&fatos(&[json!("Antônio mora em Marília-SP")]))]);
    let c = Consolidator::new(s.clone(), modelo);
    let (r1, r2) = (c.run().await, c.run().await);
    assert!(r1.ran && !r1.ok && !r2.ok);
    assert!(s.counter_get(MARCA) == 0 && s.facts().unwrap().is_empty());
    let r3 = c.run().await;
    assert!(r3.ok && r3.facts_added == 1 && s.counter_get(MARCA) > 0);
}

#[tokio::test]
async fn aceita_json_com_cerca_de_codigo() {
    for bruto in ["```json\n{\"fatos\": [\"Antônio mora em Marília-SP\"]}\n```", "  {\"fatos\": [\"Antônio mora em Marília-SP\"]}  "] {
        let d = tmp();
        let s = loja(&d);
        conversar(&s, &falas6(), "telegram", true);
        let r = Consolidator::new(s, Modelo::ok(&[bruto])).run().await;
        assert_eq!(r.facts_added, 1, "{bruto}");
    }
}

#[tokio::test]
async fn descarta_segredo_tamanho_ruim_e_nao_texto_e_respeita_o_limite() {
    let d = tmp();
    let s = loja(&d);
    conversar(&s, &falas6(), "telegram", true);
    let lixo = vec![
        json!("A senha do banco é 1234"),                                  // palavra de segredo
        json!(format!("Usa o token gsk_{} no script", "a".repeat(30))),   // padrão de chave
        json!("curto"),                                                    // curto demais
        json!("x".repeat(400)),                                            // longo demais
        json!(42),                                                         // não é texto
        json!("Antônio mora em Marília-SP"),
        json!("Antônio estuda na UNIMAR"),
        json!("Antônio trabalha com Python"),
    ];
    let r = Consolidator::new(s.clone(), Modelo::ok(&[&fatos(&lixo)])).com_limites(60, 12_000, 2, 5, vec!["legado".into()]).run().await;
    let mut textos: Vec<String> = s.facts().unwrap().into_iter().map(|f| f.text).collect();
    textos.reverse();
    assert_eq!(textos, ["Antônio mora em Marília-SP", "Antônio estuda na UNIMAR"]);
    assert_eq!(r.facts_added, 2);
}

#[tokio::test]
async fn fato_repetido_nao_duplica() {
    let d = tmp();
    let s = loja(&d);
    conversar(&s, &falas6(), "telegram", true);
    s.add_fact("Antônio mora em Marília-SP", "manual", None).unwrap();
    let r = Consolidator::new(s.clone(), Modelo::ok(&[&fatos(&[json!("antônio  mora em marília-sp")])])).run().await;
    assert!(r.facts_added == 0 && s.facts().unwrap().len() == 1);
}

#[tokio::test]
async fn lote_cortado_pelo_limite_de_caracteres_deixa_o_resto_para_a_proxima() {
    let d = tmp();
    let s = loja(&d);
    let msgs: Vec<String> = (0..8).map(|i| format!("mensagem {i}: {}", "palavra ".repeat(20))).collect();
    conversar(&s, &msgs, "telegram", false);
    let respostas: Vec<String> = (0..5).map(|i| fatos(&[json!(format!("Antônio escreve mensagens longas {i}"))])).collect();
    let modelo = Modelo::ok(&respostas.iter().map(String::as_str).collect::<Vec<_>>());
    let c = Consolidator::new(s.clone(), modelo.clone()).com_limites(60, 500, 10, 1, vec!["legado".into()]);
    let mut vistas = vec![];
    loop {
        let r = c.run().await;
        if !r.ran {
            break;
        }
        vistas.push(r.messages);
    }
    assert!(vistas.iter().sum::<usize>() == 8 && vistas.len() > 1 && *vistas.iter().max().unwrap() < 8); // em lotes, sem repetir
    let textos: Vec<String> = (0..modelo.n()).map(|i| modelo.entrada(i)).collect();
    for i in 0..8 {
        // cada mensagem aparece em exatamente um pedido
        assert_eq!(textos.iter().filter(|t| t.contains(&format!("mensagem {i}:"))).count(), 1, "mensagem {i}");
    }
    assert!(textos.last().unwrap().contains("mensagem 7:"));
}

#[tokio::test]
async fn resposta_com_formato_errado_e_recusada() {
    for bruto in ["[\"solto\"]", "{\"outra\": []}", "{}"] {
        let d = tmp();
        let s = loja(&d);
        conversar(&s, &falas6(), "telegram", true);
        assert!(!Consolidator::new(s, Modelo::ok(&[bruto])).run().await.ok, "{bruto}");
    }
}

// ── test_sleep.py ───────────────────────────────────────────────────────────
fn agora_sono() -> f64 {
    use chrono::TimeZone;
    chrono::Local.with_ymd_and_hms(2026, 10, 8, 3, 10, 0).earliest().unwrap().timestamp() as f64
}

fn hora(h: u32, m: u32) -> f64 {
    use chrono::TimeZone;
    chrono::Local.with_ymd_and_hms(2026, 10, 8, h, m, 0).earliest().unwrap().timestamp() as f64
}

struct Sono {
    store: Arc<Store>,
    ops: Arc<Operations>,
    rel: Relogio,
    _d: tempfile::TempDir,
}

fn sono() -> Sono {
    let d = tmp();
    let rel = Relogio::em(agora_sono());
    let store = loja_rel(&d, &rel);
    let ops = Arc::new(Operations::new(store.clone()));
    Sono { store, ops, rel, _d: d }
}

fn fatos_sono(s: &Store, n: usize) {
    for i in 0..n {
        s.add_fact(&format!("Antônio usa a ferramenta número {i} no projeto Orion"), "conversa", None).unwrap();
    }
}

fn ciclo(s: &Sono, m: Option<Arc<Modelo>>) -> SleepCycle {
    SleepCycle::new(s.store.clone(), s.ops.clone(), m.map(|m| m as Arc<dyn Completer>), "03:00").unwrap()
}

#[test]
fn ler_resposta_valida_tamanho_formato_e_segredo() {
    let bruto = json!({
        "relacoes": [
            ["Antônio", "Estuda em", "Unimar"],
            ["a", "x", "a"],                       // origem = destino
            ["Antônio", "usa", "token=abc123"],    // cara de segredo
            ["só", "dois"],
            ["Antônio", "", "Orion"],
        ],
        "padroes": ["Antônio concentra o trabalho no Orion", "curto", "a senha é 1234 pronto"],
    })
    .to_string();
    let (relacoes, padroes) = ler_resposta(&format!("```json\n{bruto}\n```")).unwrap();
    assert_eq!(relacoes, [("Antônio".to_string(), "estuda_em".to_string(), "Unimar".to_string())]);
    assert_eq!(padroes, ["Antônio concentra o trabalho no Orion"]);
    for ruim in ["nao json", "[]", "{\"padroes\": []}"] {
        assert!(ler_resposta(ruim).is_err(), "{ruim}");
    }
}

#[test]
fn devida_so_na_janela_e_uma_vez_por_dia() {
    let s = sono();
    let c = ciclo(&s, None);
    assert!(c.devida());
    s.rel.set(hora(2, 59));
    assert!(!c.devida());
    s.rel.set(hora(10, 0));
    assert!(!c.devida()); // passou da janela de 6 h
    assert!(SleepCycle::new(s.store.clone(), s.ops.clone(), None, "25:00").is_err());
}

#[tokio::test]
async fn sono_grava_relacoes_e_padroes_e_avanca_a_marca() {
    let s = sono();
    fatos_sono(&s.store, 7);
    let modelo = Modelo::ok(&[&json!({"relacoes": [["Antônio", "usa", "Orion"], ["Orion", "roda_em", "Windows"]],
        "padroes": ["Antônio concentra as ferramentas no projeto Orion"]})
    .to_string()]);
    let r = ciclo(&s, Some(modelo.clone())).run().await;
    assert_eq!((r.relacoes, r.padroes, r.ok), (2, 1, true));
    let dsts: Vec<_> = s.ops.neighbors("Antônio", Some("usa"), 50).unwrap().iter().map(|n| n["dst"].as_str().unwrap().to_string()).collect();
    assert_eq!(dsts, ["Orion"]);
    assert!(s.store.facts().unwrap().iter().any(|f| f.source.starts_with("sono:destilado:2026-10-08")));
    assert!(modelo.entrada(0).contains("[FATOS]"));
    // segunda noite sem fatos novos: nem chama o modelo
    let modelo2 = Modelo::ok(&["{}"]);
    s.store.counter_set("sono:ultimo", 0).unwrap();
    let r2 = ciclo(&s, Some(modelo2.clone())).run().await;
    assert!(modelo2.n() == 0 && !r2.ran_model);
}

#[tokio::test]
async fn fato_destilado_nao_vira_entrada_da_proxima_revisao() {
    let s = sono();
    fatos_sono(&s.store, 7);
    let modelo = Modelo::ok(&[&json!({"relacoes": [], "padroes": ["Antônio trabalha muito no Orion"]}).to_string()]);
    ciclo(&s, Some(modelo.clone())).run().await;
    s.store.counter_set("sono:fato", 0).unwrap();
    s.store.counter_set("sono:ultimo", 0).unwrap();
    ciclo(&s, Some(modelo.clone())).run().await;
    let segunda = modelo.entrada(1);
    assert!(!segunda.contains("destilado") && !segunda.contains("trabalha muito"), "{segunda}");
}

#[tokio::test]
async fn falha_do_modelo_nao_avanca_a_marca_e_nao_levanta() {
    let s = sono();
    fatos_sono(&s.store, 7);
    let r = ciclo(&s, Some(Modelo::novo(vec![Err("fora")]))).run().await;
    assert!(r.ran_model && !r.ok && s.store.counter_get("sono:fato") == 0);
    s.store.counter_set("sono:ultimo", 0).unwrap();
    let r = ciclo(&s, Some(Modelo::ok(&["lixo"]))).run().await;
    assert!(!r.ok);
}

#[tokio::test]
async fn duplicados_sao_avisados_uma_vez_e_nada_e_apagado() {
    let s = sono();
    s.store.add_fact("Antônio mora em Marília", "conversa", None).unwrap();
    s.store.add_fact("antonio mora em marilia", "vault", None).unwrap();
    let antes = s.store.facts().unwrap().len();
    let c = ciclo(&s, None); // sem gateway: só o passo 1
    assert_eq!(c.run().await.duplicados, 1);
    let avisos = |s: &Sono| s.ops.pending_notifications(50, false).unwrap().into_iter().filter(|n| n["kind"] == "sono").collect::<Vec<_>>();
    let a = avisos(&s);
    assert!(a.len() == 1 && a[0]["text"].as_str().unwrap().contains("orion esquecer"));
    s.store.counter_set("sono:ultimo", 0).unwrap();
    c.run().await; // o mesmo conjunto: não avisa de novo
    assert_eq!(avisos(&s).len(), 1);
    assert_eq!(s.store.facts().unwrap().len(), antes);
}

// ── memory/test_e4_memoria.py ───────────────────────────────────────────────
struct E4 {
    store: Arc<Store>,
    rel: Relogio,
    _d: tempfile::TempDir,
}

fn e4() -> E4 {
    let d = tmp();
    let rel = Relogio::em(1_800_000_000.0);
    E4 { store: loja_rel(&d, &rel), rel, _d: d }
}

fn no_projeto<T>(id: &str, f: impl FnOnce() -> T) -> T {
    with_scope(DataScope::project(Some(id.to_string()), false), f)
}

fn novo_projeto(s: &Arc<Store>, nome: &str) -> String {
    Projects::new(s.clone()).create(nome, "", false, None).unwrap()["id"].as_str().unwrap().to_string()
}

#[test]
fn search_facts_respeita_o_projeto() {
    let m = e4();
    let a = novo_projeto(&m.store, "A");
    let pessoal = m.store.add_fact("Canário pessoal vermelho", "m", None).unwrap();
    let doa = no_projeto(&a, || {
        let f = m.store.add_fact("Canário do projeto A azul", "m", None).unwrap();
        assert_eq!(m.store.search_facts("Canário", 10).unwrap().iter().map(|x| x.id).collect::<Vec<_>>(), [f.id]);
        f
    });
    let _ = doa;
    assert_eq!(m.store.search_facts("Canário", 10).unwrap().iter().map(|x| x.id).collect::<Vec<_>>(), [pessoal.id]);
}

#[test]
fn move_fact_troca_o_escopo_e_desfaz_ligacoes() {
    let m = e4();
    let a = novo_projeto(&m.store, "A");
    let velho = m.store.add_fact("Antônio mora em Marília", "m", None).unwrap();
    let novo = m.store.add_fact("Antônio mora em Bauru", "m", None).unwrap();
    m.store.supersede_fact(velho.id, novo.id).unwrap();
    let movido = m.store.move_fact(novo.id, Some(&a)).unwrap();
    assert_eq!(movido.project_id.as_deref(), Some(a.as_str()));
    assert!(m.store.get_fact(velho.id).unwrap().unwrap().superseded_by.is_none()); // não aponta para fato de outro escopo
    no_projeto(&a, || assert_eq!(m.store.facts().unwrap().iter().map(|f| f.id).collect::<Vec<_>>(), [novo.id]));
    assert_eq!(m.store.facts().unwrap().iter().map(|f| f.id).collect::<Vec<_>>(), [velho.id]);
    assert!(matches!(m.store.move_fact(999, None), Err(orion_store::StoreError::NotFound(_))));
}

#[tokio::test]
async fn consolidacao_grava_no_projeto_de_origem() {
    let m = e4();
    let a = novo_projeto(&m.store, "A");
    let sp = m.store.new_session("web", None, None).unwrap();
    let sa = m.store.new_session("web", None, Some(&a)).unwrap();
    for i in 0..5 {
        add(&m.store, &sp.id, "user", &format!("fala pessoal {i}"));
        add(&m.store, &sa.id, "user", &format!("fala do projeto {i}"));
    }
    let modelo = Modelo::ok(&[&fatos(&[json!("O Antônio gosta de café forte")]), &fatos(&[json!("O projeto A usa a biblioteca Zeta")])]);
    let r = Consolidator::new(m.store.clone(), modelo).run().await;
    assert!(r.ran && r.ok && r.facts_added == 2, "{r:?}");
    assert_eq!(m.store.facts().unwrap().iter().map(|f| f.text.clone()).collect::<Vec<_>>(), ["O Antônio gosta de café forte"]);
    no_projeto(&a, || {
        assert_eq!(m.store.facts().unwrap().iter().map(|f| f.text.clone()).collect::<Vec<_>>(), ["O projeto A usa a biblioteca Zeta"]);
    });
    let outra = Consolidator::new(m.store.clone(), Modelo::ok(&[&fatos(&[])])).run().await;
    assert!(!outra.ran); // as marcas de cada escopo avançaram
}

#[test]
fn fato_vencido_ou_substituido_pesa_menos_e_leva_a_marca() {
    let m = e4();
    let s = &m.store;
    let novo = s.add_fact("O notebook do Antônio é um MacBook", "m", None).unwrap();
    let velho = s.add_fact("O notebook do Antônio é um Lenovo", "m", None).unwrap();
    let outro = s.add_fact("O notebook de reserva é um Dell", "m", None).unwrap();
    let f = [Kind::Fact];
    assert!([novo.id, velho.id, outro.id].contains(&s.search("notebook", 5, &f).unwrap()[0].id));
    s.supersede_fact(velho.id, novo.id).unwrap();
    s.set_fact_validity(outro.id, Some(m.rel.get() + 100.0)).unwrap();
    let antes: std::collections::HashMap<_, _> = s.search("notebook", 5, &f).unwrap().into_iter().map(|h| (h.id, h)).collect();
    assert!(!antes[&outro.id].text.starts_with("(antigo)")); // ainda vale
    assert!(antes[&velho.id].text.starts_with("(antigo) "));
    m.rel.avanca(200.0); // o prazo do Dell passou
    let depois = s.search("notebook", 5, &f).unwrap();
    assert_eq!(depois[0].id, novo.id);
    let marcados: std::collections::HashSet<_> = depois.iter().filter(|h| h.text.starts_with("(antigo) ")).map(|h| h.id).collect();
    assert_eq!(marcados, [velho.id, outro.id].into());
    assert!(s.get_fact(outro.id).unwrap().unwrap().old && s.get_fact(velho.id).unwrap().unwrap().superseded_by == Some(novo.id));
    assert_eq!(s.search_facts("notebook", 10).unwrap()[0].id, novo.id); // vigente primeiro
}

#[test]
fn supersede_nao_aceita_o_proprio_fato_nem_outro_escopo() {
    let m = e4();
    let a = novo_projeto(&m.store, "A");
    let f = m.store.add_fact("um", "m", None).unwrap();
    assert!(matches!(m.store.supersede_fact(f.id, f.id), Err(orion_store::StoreError::Invalid(_))));
    let g = no_projeto(&a, || m.store.add_fact("dois", "m", None).unwrap());
    assert!(matches!(m.store.supersede_fact(f.id, g.id), Err(orion_store::StoreError::NotFound(_))));
}

#[test]
fn conflitos_sao_achados_e_decididos_so_pelo_antonio() {
    let m = e4();
    let s = &m.store;
    let velho = s.add_fact("Antônio mora em Marília desde 2020", "m", None).unwrap();
    m.rel.avanca(10.0);
    let novo = s.add_fact("Antônio mora em Bauru desde 2026", "m", None).unwrap();
    s.add_fact("O backup roda todo dia de madrugada", "m", None).unwrap();
    let pares = s.conflicting_facts(300).unwrap();
    assert_eq!(pares.iter().map(|(a, b)| (a.id, b.id)).collect::<Vec<_>>(), [(velho.id, novo.id)]);
    assert!(s.record_conflict(velho.id, novo.id).unwrap());
    assert!(!s.record_conflict(velho.id, novo.id).unwrap()); // nunca avisa duas vezes
    let p = s.pending_conflicts().unwrap();
    assert_eq!((p[0].0.id, p[0].1.id), (velho.id, novo.id));
    s.resolve_conflict(velho.id, novo.id, "novo").unwrap();
    assert!(s.get_fact(velho.id).unwrap().unwrap().superseded_by == Some(novo.id) && s.pending_conflicts().unwrap().is_empty());
    assert!(matches!(s.resolve_conflict(velho.id, novo.id, "ambos"), Err(orion_store::StoreError::NotFound(_)))); // já decidido
    assert!(s.conflicting_facts(300).unwrap().is_empty()); // o substituído sai da comparação
}

#[test]
fn os_dois_valem_encerra_o_aviso_sem_marcar_nada() {
    let m = e4();
    let s = &m.store;
    let a = s.add_fact("Antônio trabalha com Python no estágio", "m", None).unwrap();
    m.rel.avanca(5.0);
    let b = s.add_fact("Antônio trabalha com Java no estágio", "m", None).unwrap();
    assert!(s.record_conflict(a.id, b.id).unwrap());
    s.resolve_conflict(a.id, b.id, "ambos").unwrap();
    assert!(s.pending_conflicts().unwrap().is_empty());
    let fa = s.get_fact(a.id).unwrap().unwrap();
    assert!(fa.superseded_by.is_none() && !fa.old);
}

/// Reranker de mentira: nota por trecho (0 para o que não está na tabela).
struct Notas(std::collections::HashMap<String, f32>);
impl Reranker for Notas {
    fn score(&self, _p: &str, trechos: &[String]) -> anyhow::Result<Vec<f32>> {
        Ok(trechos.iter().map(|t| self.0.get(t).copied().unwrap_or(0.0)).collect())
    }
}

#[test]
fn reranker_reordena_e_o_desligado_nao_muda_nada() {
    let m = e4();
    let s = &m.store;
    s.add_fact("café com leite pela manhã", "m", None).unwrap();
    s.add_fact("café forte depois do almoço", "m", None).unwrap();
    let f = [Kind::Fact];
    let sem: Vec<String> = s.search("café", 5, &f).unwrap().into_iter().map(|h| h.text).collect();
    *s.reranker.lock() = Some(Arc::new(Notas([("café forte depois do almoço".to_string(), 0.9), ("café com leite pela manhã".to_string(), 0.1)].into())));
    let com: Vec<String> = s.search("café", 5, &f).unwrap().into_iter().map(|h| h.text).collect();
    assert_eq!(com, ["café forte depois do almoço", "café com leite pela manhã"]);
    assert_eq!(sem.iter().collect::<std::collections::HashSet<_>>(), com.iter().collect::<std::collections::HashSet<_>>());
}

#[test]
fn reranker_que_quebra_nao_derruba_a_busca() {
    struct Quebrado;
    impl Reranker for Quebrado {
        fn score(&self, _: &str, _: &[String]) -> anyhow::Result<Vec<f32>> {
            anyhow::bail!("onnx caiu")
        }
    }
    let m = e4();
    let s = &m.store;
    s.add_fact("café com leite", "m", None).unwrap();
    s.add_fact("café forte", "m", None).unwrap();
    let f = [Kind::Fact];
    let base: Vec<i64> = s.search("café", 5, &f).unwrap().iter().map(|h| h.id).collect();
    *s.reranker.lock() = Some(Arc::new(Quebrado));
    assert_eq!(s.search("café", 5, &f).unwrap().iter().map(|h| h.id).collect::<Vec<_>>(), base);
}

#[test]
fn reranker_onnx_sem_arquivos_avisa() {
    let d = tmp();
    assert!(orion_core::memory::rerank::LocalReranker::new(d.path()).is_err());
    std::fs::write(d.path().join("model.onnx"), b"x").unwrap();
    assert!(orion_core::memory::rerank::LocalReranker::new(d.path()).is_err()); // falta o tokenizer
    std::fs::write(d.path().join("tokenizer.json"), "{}").unwrap();
    assert!(orion_core::memory::rerank::LocalReranker::new(d.path()).is_err()); // tokenizer inválido também é recusado
}

// ── memory/test_eval.py (a avaliação do Rust usa um caso por pergunta) ──────
const EVAL_FATOS: &[&str] = &[
    "Antônio estuda Análise e Desenvolvimento de Sistemas (ADS) na UNIMAR",
    "Antônio mora em Marília-SP",
    "O notebook atual do Antônio é um Lenovo IdeaPad Slim 3 com 8GB de RAM e Windows",
    "Antônio vai migrar para um MacBook M2 de 16GB",
    "O orçamento do projeto Orion é R$0: só free tiers e as assinaturas que ele já paga",
    "Antônio prefere explicações diretas, sem enrolação",
    "O assistente se chama Orion e tem identidade masculina",
    "A palavra de ativação da voz é Orion",
    "O bot do Telegram só responde ao ID numérico do Antônio",
    "O acesso de fora de casa é pelo Tailscale, sem expor portas",
    "Antônio trabalha principalmente com Python, TypeScript e Java",
    "O backup da memória é diário, copiando o arquivo SQLite para a nuvem",
];

const EVAL_NOTAS: &[(&str, &str)] = &[
    ("Projetos/Orion.md", "# Orion\nA memória usa SQLite com FTS5 e vetores em numpy. A fusão dos resultados é RRF."),
    ("Estudos/UML.md", "# UML\nDiagrama de classes mostra herança e associação. Diagrama de sequência mostra mensagens."),
    ("Estudos/POO.md", "# POO\nEncapsulamento, herança e polimorfismo são os pilares da orientação a objetos."),
];

const fn caso(p: &'static str, e: &'static str) -> Caso {
    Caso { pergunta: p, esperado: e, tipo: None }
}

const EVAL_CASOS: &[Caso] = &[
    caso("onde o Antônio faz faculdade?", "UNIMAR"),
    caso("em que cidade ele mora?", "Marília"),
    caso("qual é o notebook dele?", "IdeaPad"),
    caso("quanto de RAM tem o computador?", "8GB"),
    caso("que máquina vai usar depois?", "MacBook"),
    caso("quanto posso gastar no projeto?", "R$0"),
    caso("como ele gosta que expliquem as coisas?", "diretas"),
    caso("qual a palavra que ativa a voz?", "ativação"),
    caso("quem pode falar com o bot do Telegram?", "ID numérico"),
    caso("como acesso de fora de casa?", "Tailscale"),
    caso("quais linguagens ele usa?", "Python"),
    caso("com que frequência o backup roda?", "diário"),
    caso("o que é RRF na memória?", "RRF"),
    caso("quais são os pilares de orientação a objetos?", "Encapsulamento"),
    caso("diagrama de sequência mostra o quê?", "mensagens"),
];

fn montar(store: &Store, d: &std::path::Path) {
    for f in EVAL_FATOS {
        store.add_fact(f, "seed", None).unwrap();
    }
    let vault = d.join("vault");
    for (rel, texto) in EVAL_NOTAS {
        let p = vault.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, texto).unwrap();
    }
    store.index_vault(&vault).unwrap();
}

#[test]
fn conjunto_fixo_palavra_chave() {
    let d = tmp();
    let s = loja(&d);
    montar(&s, d.path());
    let rel = avaliar(Some(&s), Some(EVAL_CASOS), 5);
    assert_eq!(rel["casos"], EVAL_CASOS.len());
    assert!(rel["recall_at_k"].as_f64().unwrap() >= 0.8, "{}", rel["falhas"]);
    assert!(rel["mrr"].as_f64().unwrap() >= 0.6);
}

/// Embedder falso: sinônimos viram o mesmo conceito (como em `memory/conftest.py`).
struct Sinonimos;
impl Embedder for Sinonimos {
    fn dim(&self) -> usize {
        128
    }
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        use unicode_normalization::UnicodeNormalization;
        let conceito = |t: &str| match t {
            "estudo" | "estuda" | "estudar" | "faculdade" | "universidade" | "unimar" | "curso" => "ESTUDAR".to_string(),
            "moro" | "mora" | "morar" | "cidade" | "residencia" | "vive" => "MORAR".to_string(),
            o => o.to_string(),
        };
        let paradas = ["a", "o", "de", "do", "da", "em", "no", "na", "que", "qual", "onde", "ele", "eu", "e", "um", "uma", "faz"];
        Ok(texts
            .iter()
            .map(|t| {
                let plano: String = t.to_lowercase().nfkd().filter(|c| c.is_ascii()).collect();
                let mut v = vec![0.0f32; 128];
                for tok in plano.split(|c: char| !c.is_alphanumeric()).filter(|x| !x.is_empty() && !paradas.contains(x)) {
                    let mut h = 0xcbf29ce484222325u64;
                    for b in conceito(tok).bytes() {
                        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
                    }
                    v[(h % 128) as usize] += 1.0;
                }
                v
            })
            .collect())
    }
}

#[test]
fn vetor_nao_piora_e_recupera_os_sinonimos() {
    let (d1, d2) = (tmp(), tmp());
    let base_store = loja(&d1);
    montar(&base_store, d1.path());
    let base = avaliar(Some(&base_store), Some(EVAL_CASOS), 5);
    let vec_store = Store::open_with(d2.path().join("v.db"), Some(Arc::new(Sinonimos)), orion_policy::wall_clock()).unwrap();
    montar(&vec_store, d2.path());
    let hibrido = avaliar(Some(&vec_store), Some(EVAL_CASOS), 5);
    assert!(hibrido["recall_at_k"].as_f64().unwrap() >= base["recall_at_k"].as_f64().unwrap());
    let sinonimos = [caso("onde ele faz faculdade?", "UNIMAR"), caso("em que cidade vive?", "Marília")];
    assert_eq!(avaliar(Some(&vec_store), Some(&sinonimos), 3)["recall_at_k"].as_f64().unwrap(), 1.0);
}

#[test]
fn relatorio_lista_os_erros() {
    let d = tmp();
    let s = loja(&d);
    s.add_fact("Antônio mora em Marília", "seed", None).unwrap();
    let rel = avaliar(Some(&s), Some(&[caso("qual a cor do carro?", "azul"), caso("onde mora?", "Marília")]), 5);
    assert_eq!((rel["casos"].as_u64(), rel["recall_at_k"].as_f64()), (Some(2), Some(0.5)));
    let falhas: Vec<_> = rel["falhas"].as_array().unwrap().iter().map(|f| f["pergunta"].as_str().unwrap()).collect();
    assert_eq!(falhas, ["qual a cor do carro?"]);
}

#[test]
fn k_menor_nao_quebra() {
    for k in [1usize, 3] {
        let d = tmp();
        let s = loja(&d);
        montar(&s, d.path());
        assert_eq!(avaliar(Some(&s), Some(&EVAL_CASOS[..3]), k)["casos"], 3);
    }
}

#[test]
fn avaliacao_padrao_embutida_alcanca_recall_alto() {
    assert!(avaliar(None, None, 5)["recall_at_k"].as_f64().unwrap() >= 0.9);
}

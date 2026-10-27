//! Paridade com os testes Python do módulo de modelos (ORION.md: "paridade antes de melhoria").
//!
//! Origem: `test_gateway`, `test_router` (pontuação e camadas), `test_costs`, `test_provedores`,
//! `test_saidas`, `test_netguard`, `test_delegate`, `test_transcribe`, `test_vision` e
//! `memory/test_embedders`. O `httpx.MockTransport` virou um servidor local (`comum`).
//!
//! Fora deste arquivo, por dependerem de outra camada: o agente com roteamento e a configuração
//! (`orion-core`/`orion-server`), os jobs e o doctor (`orion-core`), as ferramentas web e de
//! visão (`orion-core`), a prova de painel com `/chat` (`orion-server`). Sem equivalente por ora:
//! `netguard::buscar` com redirecionamentos e TLS (a barreira de rede não aceita resolvedor
//! injetado; só host público passa, então não há como apontar para um servidor local).

use orion_testkit::http::{servidor, sse, texto, Req, Resp, Servidor};
use orion_testkit::Relogio;
use orion_ai::costs::{cotas_de, host_gratuito, Cota, Custos, Quotas};
use orion_ai::gateway::{ChatGateway, Endpoint, Event, GatewayError};
use orion_ai::netguard;
use orion_ai::provedores::{chave_env, parse_provedores, CATALOGO};
use orion_ai::router::{classificar, LIMIAR_PESADO, PESADO, RAPIDO, VISAO};
use orion_ai::saidas::{self, Chamada};
use orion_store::ops::Operations;
use orion_store::Store;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, MutexGuard};

// ── apoio ───────────────────────────────────────────────────────────────────
static TRAVA: Mutex<()> = Mutex::new(());

/// O destino do registro de saída é global: quem fala com a rede segura esta trava.
fn trava() -> MutexGuard<'static, ()> {
    let g = TRAVA.lock().unwrap_or_else(|e| e.into_inner());
    saidas::definir_destino(None);
    g
}

fn registro() -> Arc<Mutex<Vec<Chamada>>> {
    let linhas: Arc<Mutex<Vec<Chamada>>> = Arc::default();
    let l = linhas.clone();
    saidas::definir_destino(Some(Arc::new(move |c: &Chamada| {
        l.lock().unwrap().push(c.clone());
        Ok(())
    })));
    linhas
}

fn em(a: i32, m: u32, d: u32, h: u32) -> f64 {
    use chrono::TimeZone;
    chrono::Local.with_ymd_and_hms(a, m, d, h, 0, 0).earliest().unwrap().timestamp() as f64
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn ep(nome: &str, base: &str, modelo: &str) -> Endpoint {
    let mut e = Endpoint::new(nome, base, modelo);
    e.api_key = Some("chave-secreta".into());
    e
}

fn ep_tier(nome: &str, base: &str, modelo: &str, tier: &str) -> Endpoint {
    let mut e = ep(nome, base, modelo);
    e.tier = tier.into();
    e
}

type Evt = Result<Event, GatewayError>;

async fn coletar(gw: &ChatGateway, tools: Option<Vec<Value>>, tier: Option<&str>) -> Vec<Evt> {
    use futures::StreamExt;
    gw.stream(vec![json!({"role": "user", "content": "oi"})], tools, tier).collect().await
}

fn ok(e: Vec<Evt>) -> Vec<Event> {
    e.into_iter().map(|x| x.unwrap()).collect()
}

/// Servidor com um endpoint por caminho (`/a/v1`, `/b/v1`...), o equivalente dos hosts `a.test`.
fn gw_com(h: impl Fn(&str, &Req) -> Resp + Send + Sync + 'static, nomes: &[&str]) -> (ChatGateway, Servidor) {
    let srv = servidor(move |r| {
        let quem = r.path.split('/').nth(1).unwrap_or("").to_string();
        h(&quem, r)
    });
    let eps = nomes.iter().map(|n| ep(n, &srv.url(&format!("/{n}/v1")), &format!("modelo-{n}"))).collect();
    (ChatGateway::with_client(eps, reqwest::Client::new(), 60.0).unwrap(), srv)
}

fn quem(srv: &Servidor) -> Vec<String> {
    srv.caminhos().iter().map(|c| c.split('/').nth(1).unwrap().to_string()).collect()
}

fn final_ok(corpo: &str) -> Resp {
    Resp::sse(sse(&[texto(corpo, Some("stop")), json!("[DONE]")]))
}

// ── test_gateway.py ─────────────────────────────────────────────────────────
#[tokio::test]
async fn streaming_de_texto_e_finish() {
    let _g = trava();
    let (gw, srv) = gw_com(|_, _| Resp::sse(sse(&[texto("Olá, ", None), texto("Antônio.", Some("stop")), json!("[DONE]")])), &["a"]);
    let ev = ok(coletar(&gw, None, None).await);
    assert_eq!(
        ev,
        vec![
            Event::TextDelta("Olá, ".into()),
            Event::TextDelta("Antônio.".into()),
            Event::Finish { reason: "stop".into(), endpoint: "a".into(), model: "modelo-a".into() }
        ]
    );
    let r = srv.req(0);
    assert_eq!(r.path, "/a/v1/chat/completions");
    assert_eq!(r.header("authorization"), "Bearer chave-secreta");
    let c = r.json();
    assert!(c["stream"] == true && c.get("tools").is_none());
}

fn tc(index: i64, id: Option<&str>, nome: &str, args: &str, fim: Option<&str>) -> Value {
    let mut f = json!({"index": index, "function": {"name": nome, "arguments": args}});
    if let Some(i) = id {
        f["id"] = json!(i);
    }
    json!({"choices": [{"delta": {"tool_calls": [f]}, "finish_reason": fim}]})
}

#[tokio::test]
async fn tool_calls_acumulam_fragmentos_e_parseiam_json() {
    let _g = trava();
    let (gw, _) = gw_com(
        |_, _| {
            Resp::sse(sse(&[
                tc(0, Some("c1"), "buscar_", "{\"consu", None),
                tc(0, None, "memoria", "lta\": \"x\"}", None),
                tc(1, Some("c2"), "listar_fatos", "", Some("tool_calls")),
                json!("[DONE]"),
            ]))
        },
        &["a"],
    );
    let ev = ok(coletar(&gw, Some(vec![json!({"type": "function"})]), None).await);
    let args = |v: Value| v.as_object().unwrap().clone();
    match (&ev[0], &ev[1]) {
        (Event::ToolCall(a), Event::ToolCall(b)) => {
            assert_eq!((a.id.as_str(), a.name.as_str(), &a.arguments), ("c1", "buscar_memoria", &args(json!({"consulta": "x"}))));
            assert_eq!((b.id.as_str(), b.name.as_str(), b.arguments.len()), ("c2", "listar_fatos", 0));
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(ev[2], Event::Finish { reason: "tool_calls".into(), endpoint: "a".into(), model: "modelo-a".into() });
}

#[tokio::test]
async fn argumentos_invalidos_viram_erro_sem_derrubar() {
    let _g = trava();
    let (gw, _) = gw_com(|_, _| Resp::sse(sse(&[tc(0, Some("c"), "f", "{quebrado", None), json!("[DONE]")])), &["a"]);
    let ev = ok(coletar(&gw, None, None).await);
    match &ev[0] {
        Event::ToolCall(c) => assert!(c.error.as_deref().is_some_and(|e| e.contains("inválidos")) && c.arguments.is_empty()),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn fallback_quando_o_primeiro_falha() {
    let _g = trava();
    let (gw, srv) = gw_com(|q, _| if q == "a" { Resp::texto(503, "indisponível") } else { final_ok("do b") }, &["a", "b"]);
    let ev = ok(coletar(&gw, None, None).await);
    assert_eq!(quem(&srv), ["a", "b"]);
    assert_eq!(ev.last().unwrap(), &Event::Finish { reason: "stop".into(), endpoint: "b".into(), model: "modelo-b".into() });
}

#[tokio::test]
async fn http_429_poe_em_quarentena_e_volta_depois() {
    let _g = trava();
    // Rust usa relógio monotônico (Instant): a quarentena mínima é de 1 s (Retry-After: 1)
    let limitado = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let l = limitado.clone();
    let (gw, srv) = gw_com(
        move |q, _| {
            if q == "a" && l.load(Ordering::SeqCst) {
                Resp::texto(429, "cota").com_header("retry-after", "1")
            } else {
                final_ok("ok")
            }
        },
        &["a", "b"],
    );
    coletar(&gw, None, None).await;
    assert_eq!(quem(&srv), ["a", "b"]);
    coletar(&gw, None, None).await; // ainda em quarentena: nem tenta o a
    assert_eq!(quem(&srv), ["a", "b", "b"]);
    limitado.store(false, Ordering::SeqCst);
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await; // voltou
    coletar(&gw, None, None).await;
    assert_eq!(quem(&srv), ["a", "b", "b", "a"]);
}

#[tokio::test]
async fn resposta_vazia_tenta_o_proximo() {
    let _g = trava();
    let (gw, _) = gw_com(|q, _| if q == "a" { Resp::sse(sse(&[json!("[DONE]")])) } else { final_ok("b") }, &["a", "b"]);
    assert_eq!(ok(coletar(&gw, None, None).await)[0], Event::TextDelta("b".into()));
}

#[tokio::test]
async fn todos_falham_levanta_com_as_tentativas() {
    let _g = trava();
    let (gw, _) = gw_com(|_, _| Resp::texto(500, "erro"), &["a", "b"]);
    let e = coletar(&gw, None, None).await.into_iter().last().unwrap().unwrap_err();
    assert_eq!(e.tentativas.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    assert!(e.tentativas[0].1.contains("500"));
    assert!(!format!("{e:?}").contains("chave-secreta")); // a chave não vai para o erro
}

#[tokio::test]
async fn erro_de_rede_tenta_o_proximo() {
    let _g = trava();
    let (gw, _) = gw_com(|q, _| if q == "a" { Resp::Drop } else { final_ok("b") }, &["a", "b"]);
    match coletar(&gw, None, None).await.into_iter().last().unwrap().unwrap() {
        Event::Finish { endpoint, .. } => assert_eq!(endpoint, "b"),
        o => panic!("{o:?}"),
    }
}

#[tokio::test]
async fn queda_no_meio_do_texto_nao_troca_de_endpoint() {
    let _g = trava();
    let (gw, srv) = gw_com(
        |_, _| Resp::Cut(sse(&[texto("começo ", None)])),
        &["a", "b"],
    );
    let ev = coletar(&gw, None, None).await;
    assert_eq!(ev[0].as_ref().unwrap(), &Event::TextDelta("começo ".into()));
    let e = ev.last().unwrap().as_ref().unwrap_err();
    assert!(e.mensagem.contains("interrompeu"), "{e}");
    assert_eq!(ev.len(), 2);
    assert_eq!(quem(&srv), ["a"]);
}

#[test]
fn precisa_de_endpoint() {
    assert!(ChatGateway::new(vec![]).is_err());
}

// ── test_router.py: camadas no gateway ──────────────────────────────────────
fn gw_camadas() -> (ChatGateway, Servidor) {
    let srv = servidor(|_| final_ok("ok"));
    let u = srv.url("/v1");
    let eps = vec![
        ep("gateway", &u, "padrao"),
        ep_tier("rapido", &u, "leve", "rapido"),
        ep_tier("pesado", &u, "forte", "pesado"),
    ];
    (ChatGateway::with_client(eps, reqwest::Client::new(), 60.0).unwrap(), srv)
}

fn modelos(srv: &Servidor) -> Vec<String> {
    (0..srv.n()).map(|i| srv.req(i).json()["model"].as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn a_camada_pedida_vai_primeiro_e_o_padrao_e_o_reserva() {
    let _g = trava();
    let (gw, srv) = gw_camadas();
    coletar(&gw, None, Some("pesado")).await;
    coletar(&gw, None, Some("rapido")).await;
    coletar(&gw, None, None).await; // sem camada: só o padrão
    coletar(&gw, None, Some("visao")).await; // camada sem endpoint próprio: cai no padrão
    assert_eq!(modelos(&srv), ["forte", "leve", "padrao", "padrao"]);
}

#[tokio::test]
async fn camada_que_falha_cai_no_padrao_e_a_outra_camada_nunca_entra() {
    let _g = trava();
    let srv = servidor(|r| if r.json()["model"] == "forte" { Resp::texto(503, "x") } else { final_ok("ok") });
    let u = srv.url("/v1");
    let eps = vec![ep("gateway", &u, "padrao"), ep_tier("rapido", &u, "leve", "rapido"), ep_tier("pesado", &u, "forte", "pesado")];
    let gw = ChatGateway::with_client(eps, reqwest::Client::new(), 60.0).unwrap();
    coletar(&gw, None, Some("pesado")).await;
    assert_eq!(modelos(&srv), ["forte", "padrao"]); // o "leve" nunca é tentado para trabalho pesado
}

#[tokio::test]
async fn configuracao_so_com_camadas_ainda_responde() {
    let _g = trava();
    let srv = servidor(|_| final_ok("ok"));
    let gw = ChatGateway::with_client(vec![ep_tier("so", &srv.url("/v1"), "unico", "rapido")], reqwest::Client::new(), 60.0).unwrap();
    coletar(&gw, None, None).await;
    assert_eq!(modelos(&srv), ["unico"]);
}

#[test]
fn estatisticas_dizem_a_camada_de_cada_endpoint() {
    let (gw, _) = gw_camadas();
    let v: Vec<(String, String)> =
        gw.stats().iter().map(|e| (e["nome"].as_str().unwrap().into(), e["camada"].as_str().unwrap().into())).collect();
    assert_eq!(
        v,
        [("gateway", "padrão"), ("rapido", "rapido"), ("pesado", "pesado")].map(|(a, b)| (a.to_string(), b.to_string()))
    );
}

// ── test_router.py: a pontuação ─────────────────────────────────────────────
const CODIGO: &str = "Analise o código abaixo e refatore:\n```python\ndef f(x):\n    return x * 2\n```";

#[test]
fn conversa_curta_e_comando_de_ferramenta_vao_para_o_rapido() {
    for m in [
        "oi",
        "que horas são?",
        "lembra de pagar o boleto amanhã às 9",
        "toca a próxima música",
        "qual a capital da França?",
        "obrigado!",
        "o que eu tenho para hoje?",
    ] {
        assert_eq!(classificar(m, 0).camada, RAPIDO, "{m}");
    }
}

#[test]
fn codigo_analise_e_texto_longo_vao_para_o_pesado() {
    let longo = "x".repeat(1300);
    for m in [
        CODIGO,
        "Compare SQLite e Postgres para o meu caso, com prós e contras em detalhe",
        "Planeje a migração do banco de dados do legado, passo a passo",
        "Traceback (most recent call last):\n  File 'a.py', line 3\nValueError: x\nme ajuda a depurar",
        longo.as_str(),
    ] {
        assert_eq!(classificar(m, 0).camada, PESADO, "{m}");
    }
}

#[test]
fn o_motivo_mostra_os_sinais_que_pontuaram() {
    let r = classificar(CODIGO, 0);
    assert_eq!(r.camada, PESADO);
    assert!(r.motivo.contains("código ou erro colado (+2)") && r.motivo.contains("analis"), "{}", r.motivo);
    assert_eq!(classificar("oi", 0).motivo, "pergunta ou comando curto");
}

#[test]
fn um_sinal_so_nao_basta_e_o_limiar_e_o_documentado() {
    assert_eq!(LIMIAR_PESADO, 3);
    assert_eq!(classificar("explique o diagrama de classes", 0).camada, RAPIDO); // só tema técnico (+1)
    assert_eq!(classificar(&format!("analise isso{}", " .".repeat(130)), 0).camada, PESADO); // pede (+2) + 261 chars (+1)
}

#[test]
fn acentos_e_maiusculas_nao_mudam_a_decisao() {
    assert_eq!(
        classificar("COMPARE as OPÇÕES, PRÓS E CONTRAS", 0).camada,
        classificar("compare as opcoes, pros e contras", 0).camada
    );
}

#[test]
fn imagem_vai_para_a_visao_e_a_etiqueta_forca_a_camada() {
    assert_eq!(classificar("o que é isso?", 1).camada, VISAO);
    assert_eq!(classificar("#pesado oi", 0).camada, PESADO);
    assert_eq!(classificar(&format!("  #rapido {CODIGO}"), 0).camada, RAPIDO);
    assert_eq!(classificar("#visao descreva", 0).camada, VISAO);
    assert_eq!(classificar("#pesado", 2).camada, PESADO); // a etiqueta vence a imagem
    assert_eq!(classificar("isto #pesado não é etiqueta", 0).camada, RAPIDO); // só no começo
    assert_eq!(classificar("#pesados", 0).camada, RAPIDO);
}

#[test]
fn classificar_e_deterministico() {
    assert!((0..5).all(|_| classificar(CODIGO, 0) == classificar(CODIGO, 0)));
}

// ── test_costs.py ───────────────────────────────────────────────────────────
struct Mundo {
    rel: Relogio,
    store: Arc<Store>,
    ops: Arc<Operations>,
    custos: Custos,
    _d: tempfile::TempDir,
}

fn cota(p: &str, n: &str, l: i64, per: &'static str) -> Cota {
    Cota { provedor: p.into(), nome: n.into(), limite: l, periodo: per }
}

fn mundo() -> Mundo {
    let d = tmp();
    let rel = Relogio::em(em(2026, 10, 9, 15));
    let store = Arc::new(Store::open_with(d.path().join("c.db"), None, rel.clock()).unwrap());
    let ops = Arc::new(Operations::new(store.clone()));
    let custos = Custos::new(
        store.clone(),
        vec![
            ("gateway".into(), cota("gateway:", "Gateway", 10, "dia")),
            ("brave".into(), cota("brave", "Brave", 4, "mes")),
        ],
        Some(ops.clone()),
    );
    Mundo { rel, store, ops, custos, _d: d }
}

fn chamadas(store: &Store, provider: &str, n: usize, ts: Option<f64>, ok: bool) {
    for _ in 0..n {
        store
            .add_external_call(&orion_store::library::ExternalCall {
                provider,
                kind: "chat",
                ok,
                latency_ms: 100,
                model: "",
                bytes_out: 0,
                bytes_in: 0,
                content_kind: "",
                ts,
            })
            .unwrap();
    }
}

#[test]
fn uso_conta_hoje_e_o_mes_e_soma_as_camadas_do_gateway() {
    let m = mundo();
    let agora = m.rel.get();
    chamadas(&m.store, "gateway:padrão", 3, None, true);
    chamadas(&m.store, "gateway:rapido", 2, None, true);
    chamadas(&m.store, "ollama", 5, None, true); // local não gasta cota
    chamadas(&m.store, "gateway:padrão", 4, Some(agora - 86400.0), true); // ontem
    chamadas(&m.store, "brave", 2, Some(em(2026, 10, 1, 9)), true); // começo do mês
    chamadas(&m.store, "brave", 1, Some(em(2026, 9, 30, 9)), true); // mês passado
    assert_eq!(m.custos.uso("gateway"), 5);
    assert_eq!(m.custos.uso("brave"), 2);
}

#[test]
fn job_opcional_para_com_90_porcento_e_avisa_uma_vez_por_dia() {
    let m = mundo();
    chamadas(&m.store, "gateway:padrão", 8, None, true);
    assert!(m.custos.pode_usar("gateway", true, "")); // 80%
    chamadas(&m.store, "gateway:padrão", 1, None, true);
    assert!(!m.custos.pode_usar("gateway", true, "o ciclo de sono"));
    assert!(!m.custos.pode_usar("gateway", true, "o ciclo de sono"));
    let avisos = m.ops.pending_notifications(50, false).unwrap();
    assert_eq!(avisos.len(), 1);
    let (kind, texto) = (avisos[0]["kind"].as_str().unwrap(), avisos[0]["text"].as_str().unwrap());
    assert_eq!(kind, "cota");
    assert!(texto.contains("9/10 hoje") && texto.contains("o ciclo de sono ficou para depois"), "{texto}");
    assert!(texto.contains("O chat continua"));
    assert!(m.custos.pode_usar("gateway", false, "")); // o chat nunca é bloqueado
    m.rel.set(m.rel.get() + 86400.0); // dia novo: cota zerada
    assert!(m.custos.pode_usar("gateway", true, ""));
}

#[test]
fn resumo_mostra_estado_de_cada_cota() {
    let m = mundo();
    chamadas(&m.store, "gateway:padrão", 12, None, true);
    let r = m.custos.resumo();
    let g = r.iter().find(|c| c["provedor"] == "gateway").unwrap();
    assert_eq!(
        g,
        &json!({"provedor": "gateway", "nome": "Gateway", "usado": 12, "limite": 10, "periodo": "dia", "pct": 120, "estado": "estourada"})
    );
    let b = r.iter().find(|c| c["provedor"] == "brave").unwrap();
    assert!(b["estado"] == "ok" && b["periodo"] == "mes");
}

#[test]
fn limite_zero_e_sem_limite() {
    let d = tmp();
    let store = Arc::new(Store::open(d.path().join("x.db")).unwrap());
    let q = Quotas { gateway_dia: 100, groq_dia: 100, gemini_dia: 0, brave_mes: 100 };
    let c = Custos::new(store.clone(), cotas_de(&q, &[]), None);
    chamadas(&store, "gemini", 50, None, true);
    assert!(c.pode_usar("gemini", true, ""));
    let r = c.resumo();
    assert_eq!(r.iter().find(|x| x["provedor"] == "gemini").unwrap()["estado"], "sem_limite");
}

#[test]
fn lista_de_gratuitos() {
    assert!(host_gratuito("http://127.0.0.1:20128/v1"));
    assert!(host_gratuito("https://api.groq.com/openai/v1"));
    assert!(!host_gratuito("https://api.openai.com/v1"));
}

// ── test_provedores.py ──────────────────────────────────────────────────────
#[test]
fn le_a_lista_json() {
    let l = parse_provedores(&json!([{"id": "Groq", "rapido": "r1", "limite_dia": 500}]).to_string()).unwrap();
    assert!(l[0].id == "groq" && l[0].limite_dia == 500);
    assert!(parse_provedores("").unwrap().is_empty());
}

#[test]
fn configuracao_invalida_falha_ao_subir() {
    for (item, erro) in [
        (json!({"id": "desconhecido", "padrao": "m"}), "fora do catálogo"),
        (json!({"id": "meu", "url": "http://exemplo.com/v1", "padrao": "m"}), "https"),
        (json!({"id": "groq"}), "ao menos um modelo"),
        (json!({"id": "com espaço", "padrao": "m"}), "id:"),
    ] {
        let e = parse_provedores(&json!([item]).to_string()).unwrap_err();
        assert!(e.contains(erro), "{e} !~ {erro}");
    }
}

#[test]
fn provedor_repetido_e_recusado() {
    let j = json!([{"id": "groq", "padrao": "a"}, {"id": "groq", "rapido": "b"}]).to_string();
    assert!(parse_provedores(&j).unwrap_err().contains("repetido"));
}

#[test]
fn modelos_e_url_do_provedor() {
    let l = parse_provedores(&json!([{"id": "groq", "rapido": "r1", "pesado": "p1"}]).to_string()).unwrap();
    assert_eq!(l[0].modelos(), [("rapido".to_string(), "r1".to_string()), ("pesado".to_string(), "p1".to_string())]);
    assert_eq!(l[0].base_url(), CATALOGO.iter().find(|(k, _)| *k == "groq").unwrap().1.base_url);
    assert_eq!(chave_env("groq"), "ORION_KEY_GROQ");
    assert_eq!(chave_env("meu-x"), "ORION_KEY_MEU_X");
}

#[test]
fn orcamento_do_dia_por_provedor() {
    let m = mundo();
    let provs = parse_provedores(&json!([{"id": "groq", "padrao": "m", "limite_dia": 2}, {"id": "gemini", "padrao": "g"}]).to_string()).unwrap();
    let q = Quotas { gateway_dia: 100, groq_dia: 100, gemini_dia: 100, brave_mes: 100 };
    let custos = Custos::new(m.store.clone(), cotas_de(&q, &provs), None);
    assert!(custos.cotas.read().iter().any(|(k, _)| k == "prov:groq") && !custos.cotas.read().iter().any(|(k, _)| k == "prov:gemini")); // sem limite
    assert!(custos.no_orcamento("groq") && custos.no_orcamento("gemini"));
    chamadas(&m.store, "gateway:groq", 2, None, true);
    assert!(!custos.no_orcamento("groq") && custos.no_orcamento("gemini"));
    assert_eq!(custos.uso("gateway"), 2); // a cota geral do gateway soma todos os provedores
}

#[test]
fn hosts_do_catalogo_contam_como_gratuitos() {
    assert!(CATALOGO.iter().all(|(_, c)| host_gratuito(c.base_url)));
    assert!(!host_gratuito("https://api.openai.com/v1"));
}

// ── test_saidas.py ──────────────────────────────────────────────────────────
#[test]
fn resumo_por_provedor_percentis_falhas_e_poda() {
    let d = tmp();
    let rel = Relogio::em(1_000_000.0);
    let m = Store::open_with(d.path().join("m.db"), None, rel.clock()).unwrap();
    let add = |p: &str, kind: &str, ok: bool, lat: i64, model: &str, bo: i64, bi: i64| {
        m.add_external_call(&orion_store::library::ExternalCall {
            provider: p, kind, ok, latency_ms: lat, model, bytes_out: bo, bytes_in: bi, content_kind: "", ts: None,
        })
        .unwrap();
    };
    for (i, lat) in [100, 200, 300, 400, 1000].into_iter().enumerate() {
        add("gateway:padrão", "chat", i != 4, lat, if i < 3 { "rapido" } else { "forte" }, 10, 5);
    }
    add("gemini", "embed", true, 50, "", 0, 0);
    let resumo = m.external_calls_summary(0.0).unwrap();
    let g = resumo.iter().find(|r| r["provider"] == "gateway:padrão" && r["kind"] == "chat").unwrap();
    assert_eq!((&g["chamadas"], &g["falhas"], &g["p50_ms"], &g["p95_ms"]), (&json!(5), &json!(1), &json!(300), &json!(1000)));
    assert_eq!((&g["bytes_out"], &g["bytes_in"], &g["modelo"]), (&json!(50), &json!(25), &json!("rapido")));
    assert_eq!(resumo.iter().find(|r| r["provider"] == "gemini" && r["kind"] == "embed").unwrap()["chamadas"], 1);
    assert_eq!(m.external_calls_count("gateway:", 0.0), 5);
    assert_eq!(m.external_calls(0.0, Some("gem"), 100).unwrap().len(), 1);
    rel.set(rel.get() + 91.0 * 86400.0);
    assert_eq!(m.prune_external_calls(90).unwrap(), 6);
    assert!(m.external_calls(0.0, None, 100).unwrap().is_empty());
}

#[test]
fn sem_destino_nao_faz_nada_e_destino_quebrado_nao_derruba() {
    let _g = trava();
    saidas::registrar("groq", "transcribe", true, 1.0, "", 0, 0, "");
    saidas::definir_destino(Some(Arc::new(|_| anyhow::bail!("banco travado"))));
    saidas::registrar("groq", "transcribe", true, 1.0, "", 0, 0, ""); // não levanta
    let mut m = saidas::medir("groq", "transcribe", "", 0, "");
    m.ok = true;
    drop(m);
    saidas::definir_destino(None);
}

#[test]
fn medir_registra_falha_quando_a_chamada_nao_confirma() {
    let _g = trava();
    let r = registro();
    {
        let _m = saidas::medir("brave", "search", "", 7, "");
        // a chamada "levantou": ninguém marcou `ok`
    }
    let l = r.lock().unwrap();
    assert!(!l.last().unwrap().ok && l.last().unwrap().bytes_out == 7);
    drop(l);
    saidas::definir_destino(None);
}

#[test]
fn provedor_pelo_host_sem_guardar_a_url() {
    assert_eq!(saidas::provedor_da_url("https://api.groq.com/openai/v1", ""), "groq");
    assert_eq!(saidas::provedor_da_url("https://generativelanguage.googleapis.com/x", ""), "gemini");
    assert_eq!(saidas::provedor_da_url("http://127.0.0.1:5678/webhook/a", "n8n"), "n8n");
    assert_eq!(saidas::provedor_da_url("https://n8n.exemplo.com/webhook/abc?t=1", ""), "n8n.exemplo.com");
}

#[tokio::test]
async fn gateway_registra_cada_tentativa_sem_o_conteudo() {
    let _g = trava();
    let r = registro();
    let (gw, _) = gw_com(|q, _| if q == "a" { Resp::texto(502, "fora") } else { final_ok("ok") }, &["a", "b"]);
    coletar(&gw, None, None).await;
    let l = r.lock().unwrap().clone();
    assert_eq!(l.len(), 2);
    let (a, b) = (&l[0], &l[1]);
    assert_eq!((a.provider.as_str(), a.kind.as_str(), a.ok, a.model.as_str()), ("gateway:padrão", "chat", false, "modelo-a"));
    assert_eq!((b.ok, b.model.as_str(), b.content_kind.as_str()), (true, "modelo-b", "texto"));
    assert!(b.bytes_out > 0 && b.bytes_in > 0);
    assert!(!format!("{l:?}").contains("\"oi\"")); // a mensagem não vai para o registro
    saidas::definir_destino(None);
}

#[tokio::test]
async fn gateway_marca_imagem_como_tipo_de_conteudo() {
    use futures::StreamExt;
    let _g = trava();
    let r = registro();
    let (gw, _) = gw_com(|_, _| final_ok("vi"), &["a"]);
    let segredo = "conteudo-que-nunca-pode-ir-para-o-registro-0123456789";
    let msgs = vec![json!({"role": "user", "content": [
        {"type": "text", "text": segredo},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}}]})];
    let _: Vec<_> = gw.stream(msgs, None, None).collect().await;
    let l = r.lock().unwrap().clone();
    assert_eq!(l.last().unwrap().content_kind, "imagem");
    assert!(!format!("{l:?}").contains(segredo));
    saidas::definir_destino(None);
}

// ── test_netguard.py ────────────────────────────────────────────────────────
#[tokio::test]
async fn esquema_host_e_credencial_invalidos() {
    for u in [
        "ftp://exemplo.com/x",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "http://",
        "http://user:senha@exemplo.com/",
        "http://exemplo.com:99999/",
        "",
        "   ",
    ] {
        assert!(netguard::validar(u).await.is_err(), "{u:?} deveria ser bloqueada");
    }
}

#[test]
fn ip_nao_publico_e_bloqueado_inclusive_tailscale_e_metadados() {
    for ip in [
        "127.0.0.1", "10.1.2.3", "192.168.0.10", "172.16.5.5", "169.254.169.254", "100.64.0.9", "::1", "fe80::1",
        "fd00::1", "0.0.0.0", "::ffff:127.0.0.1", "224.0.0.1",
    ] {
        assert!(!netguard::ip_publico(ip.parse().unwrap()), "{ip}");
    }
    for ip in ["93.184.216.34", "1.1.1.1", "2606:4700:4700::1111"] {
        assert!(netguard::ip_publico(ip.parse().unwrap()), "{ip}");
    }
}

#[tokio::test]
async fn ip_literal_privado_e_bloqueado_sem_dns() {
    for u in ["http://127.0.0.1:8000/chat", "http://[::1]/", "http://10.0.0.5/", "http://169.254.169.254/latest"] {
        let e = netguard::validar(u).await.unwrap_err();
        assert!(e.0.contains("não público"), "{u}: {e}");
    }
}

#[tokio::test]
async fn host_que_nao_resolve_e_bloqueado() {
    let e = netguard::validar("http://nao-existe.invalid/").await.unwrap_err();
    assert!(e.0.contains("não resolve"), "{e}");
}

#[tokio::test]
async fn destino_guarda_o_ip_conferido_e_o_caminho() {
    let d = netguard::validar("https://93.184.216.34:8443/a/b?q=1").await.unwrap();
    assert_eq!((d.ip.to_string(), d.porta, d.host.as_str()), ("93.184.216.34".to_string(), 8443, "93.184.216.34"));
    assert_eq!(d.url.path(), "/a/b");
    assert_eq!(d.url.query(), Some("q=1"));
}

#[tokio::test]
async fn buscar_nao_chega_a_conectar_em_destino_local() {
    let srv = servidor(|_| Resp::texto(200, "segredo"));
    let e = netguard::buscar(&srv.url("/admin"), 5.0, 1000).await.unwrap_err();
    assert!(e.to_string().contains("não público"), "{e}");
    assert_eq!(srv.n(), 0); // a requisição nem saiu
}

#[test]
fn html_vira_texto_sem_script_estilo_nem_marcacao() {
    let (titulo, texto) = netguard::html_para_texto(
        "<html><head><title> Meu  título </title><style>p{color:red}</style></head>\
         <body><h1>Olá</h1><script>alert(1)</script><p>um <b>dois</b></p><p>três &amp; quatro</p>\
         <noscript>ignora</noscript></body></html>",
    );
    assert_eq!(titulo, "Meu título");
    assert_eq!(texto, "Olá\n\num dois\n\ntrês & quatro"); // um parágrafo vazio entre blocos
    assert!(!texto.contains("alert") && !texto.contains("color"));
}

#[test]
fn e_texto() {
    assert!(netguard::e_texto("text/html") && netguard::e_texto("application/json"));
    assert!(netguard::e_texto("application/ld+json") && netguard::e_texto(""));
    assert!(!netguard::e_texto("image/png") && !netguard::e_texto("application/pdf"));
}

// ── test_delegate.py ────────────────────────────────────────────────────────
#[cfg(unix)]
mod delegar {
    use super::*;
    use orion_ai::delegate::{CliAgent, Delegator};

    /// CLI falsa: um `sh -c` que recebe o prompt como `$1`.
    fn fake(nome: &str, script: &str, limite: i64) -> CliAgent {
        CliAgent {
            name: nome.into(),
            argv: ["sh", "-c", script, "sh", "{prompt}"].map(String::from).to_vec(),
            daily_limit: limite,
        }
    }

    const OK: &str = "echo \"feito:$1\"";
    const COTA: &str = "echo 'Error 429: usage limit reached' >&2; exit 1";
    const FALHA: &str = "echo 'SyntaxError no projeto' >&2; exit 2";

    struct Amb {
        store: Arc<Store>,
        pasta: std::path::PathBuf,
        rel: Relogio,
        d: tempfile::TempDir,
    }

    fn amb() -> Amb {
        let d = tmp();
        let rel = Relogio::em(1_700_000_000.0);
        let store = Arc::new(Store::open_with(d.path().join("d.db"), None, rel.clock()).unwrap());
        let pasta = d.path().join("projeto");
        std::fs::create_dir(&pasta).unwrap();
        Amb { store, pasta, rel, d }
    }

    fn deleg(a: &Amb, agentes: Vec<CliAgent>) -> Delegator {
        Delegator::new(a.store.clone(), agentes, 10.0, 8000)
    }

    fn p(a: &Amb) -> String {
        a.pasta.to_string_lossy().into_owned()
    }

    #[test]
    fn executa_na_pasta_e_devolve_a_saida() {
        let _g = trava();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", "echo \"$(pwd -P)|$1\"", 2)]);
        let r = d.delegate("refatore o módulo", &p(&a), None);
        assert!(r["ok"] == true && r["agente"] == "claude", "{r}");
        let esperado = format!("{}|Tarefa: refatore o módulo", std::fs::canonicalize(&a.pasta).unwrap().display());
        assert_eq!(r["saida"], esperado);
    }

    #[test]
    fn prompt_com_hifen_e_com_metacaracteres_vai_como_um_argumento() {
        let _g = trava();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", OK, 2)]);
        let perigo = "--dangerously-skip-permissions; rm -rf / && $(whoami) `id`";
        let r = d.delegate(perigo, &p(&a), None);
        assert_eq!(r["saida"], format!("feito:Tarefa: {perigo}")); // nada foi interpretado
    }

    #[test]
    fn preferido_primeiro_e_cota_esgotada_cai_para_o_proximo() {
        let _g = trava();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", COTA, 2), fake("codex", OK, 2), fake("gemini", OK, 2)]);
        let r = d.delegate("x", &p(&a), None);
        assert_eq!(r["agente"], "codex");
        assert_eq!(r["tentativas"], json!([{"agente": "claude", "motivo": "cota esgotada"}]));
        // claude ficou marcado como esgotado: a próxima chamada nem tenta
        let r2 = d.delegate("y", &p(&a), None);
        assert_eq!(r2["tentativas"], json!([{"agente": "claude", "motivo": "limite diário atingido"}]));
        // pedir um agente específico começa por ele
        assert_eq!(d.delegate("z", &p(&a), Some("gemini"))["agente"], "gemini");
    }

    #[test]
    fn contador_diario_limita_e_zera_no_dia_seguinte() {
        let _g = trava();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", OK, 2)]);
        assert!(d.delegate("a", &p(&a), None)["ok"] == true && d.delegate("b", &p(&a), None)["ok"] == true);
        let r = d.delegate("c", &p(&a), None);
        assert!(r["ok"] == false && r["tentativas"][0]["motivo"] == "limite diário atingido");
        a.rel.set(a.rel.get() + 86_400.0);
        assert_eq!(d.delegate("d", &p(&a), None)["ok"], true);
    }

    #[test]
    fn cli_nao_instalada_e_pulada() {
        let _g = trava();
        let a = amb();
        let nao = |n: &str| CliAgent { name: n.into(), argv: vec!["orion-cli-que-nao-existe".into(), "{prompt}".into()], daily_limit: 2 };
        let d = deleg(&a, vec![nao("claude"), nao("codex")]);
        assert_eq!(
            d.delegate("x", &p(&a), None),
            json!({"ok": false, "erro": "nenhum agente disponível", "tentativas": [
                {"agente": "claude", "motivo": "CLI não instalada"},
                {"agente": "codex", "motivo": "CLI não instalada"}]})
        );
    }

    #[test]
    fn falha_real_nao_tenta_outro_agente() {
        let _g = trava();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", FALHA, 2), fake("codex", OK, 2)]);
        let r = d.delegate("x", &p(&a), None);
        assert!(r["ok"] == false && r["agente"] == "claude" && r["erro"].as_str().unwrap().contains("SyntaxError"));
    }

    #[test]
    fn tempo_limite() {
        let _g = trava();
        let a = amb();
        let d = Delegator::new(a.store.clone(), vec![fake("claude", "sleep 5", 2)], 0.3, 8000);
        let r = d.delegate("x", &p(&a), None);
        assert!(r["ok"] == false && r["erro"].as_str().unwrap().contains("tempo-limite"), "{r}");
    }

    #[test]
    fn entradas_invalidas() {
        let _g = trava();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", OK, 2)]);
        assert!(d.delegate("  ", &p(&a), None)["erro"].as_str().unwrap().contains("tarefa vazia"));
        let nao = a.d.path().join("nao");
        assert!(d.delegate("x", &nao.to_string_lossy(), None)["erro"].as_str().unwrap().contains("inexistente"));
        assert!(d.delegate("x", &p(&a), Some("gpt"))["erro"].as_str().unwrap().contains("desconhecido"));
    }

    #[test]
    fn saida_truncada() {
        let _g = trava();
        let a = amb();
        let d = Delegator::new(a.store.clone(), vec![fake("claude", "printf 'x%.0s' $(seq 500)", 2)], 10.0, 100);
        let r = d.delegate("x", &p(&a), None);
        assert!(r["saida"].as_str().unwrap().len() == 100 && r["truncado"] == true, "{r}");
    }

    #[test]
    fn segredo_do_orion_nao_chega_ao_processo_filho() {
        let _g = trava();
        let a = amb();
        std::env::set_var("ORION_ADMIN_TOKEN", "segredo");
        std::env::set_var("PATH_DA_CLI", "visivel");
        let d = deleg(&a, vec![fake("claude", "echo \"[${ORION_ADMIN_TOKEN-None}] ${PATH_DA_CLI-None}\"", 2)]);
        assert_eq!(d.delegate("x", &p(&a), None)["saida"], "[None] visivel");
    }

    #[test]
    fn processo_filho_nao_le_o_teclado() {
        let _g = trava();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", "printf \"[%s]\" \"$(cat)\"", 2)]);
        assert_eq!(d.delegate("x", &p(&a), None)["saida"], "[]");
    }

    #[test]
    fn erro_do_so_ao_executar() {
        use std::os::unix::fs::PermissionsExt;
        let _g = trava();
        let a = amb();
        let sem_permissao = a.d.path().join("sem-exec");
        std::fs::write(&sem_permissao, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&sem_permissao, std::fs::Permissions::from_mode(0o644)).unwrap();
        let ag = |n: &str| CliAgent { name: n.into(), argv: vec![sem_permissao.to_string_lossy().into(), "{prompt}".into()], daily_limit: 2 };
        let d = deleg(&a, vec![ag("claude"), ag("codex")]);
        let r = d.delegate("x", &p(&a), None);
        let motivos: Vec<_> = r["tentativas"].as_array().unwrap().iter().map(|t| t["motivo"].as_str().unwrap().split(':').next().unwrap().to_string()).collect();
        assert_eq!(motivos, ["não executou"; 2]);
    }

    #[test]
    fn contadores_do_store() {
        let a = amb();
        let s = &a.store;
        assert_eq!(s.counter_get("a"), 0);
        assert!(s.counter_incr("a", 1).unwrap() == 1 && s.counter_incr("a", 4).unwrap() == 5);
        s.counter_set("a", 2).unwrap();
        assert!(s.counter_get("a") == 2 && s.counter_get("b") == 0);
    }

    #[test]
    fn shim_cmd_do_windows_e_recusado() {
        let _g = trava();
        let a = amb();
        let shim = a.d.path().join("codex.cmd");
        std::fs::write(&shim, "@echo off").unwrap();
        let ag = |n: &str| CliAgent { name: n.into(), argv: vec![shim.to_string_lossy().into(), "{prompt}".into()], daily_limit: 2 };
        let d = deleg(&a, vec![ag("claude"), ag("codex")]);
        let r = d.delegate("x", &p(&a), None);
        let motivos: Vec<_> = r["tentativas"].as_array().unwrap().iter().map(|t| t["motivo"].clone()).collect();
        assert_eq!(motivos, vec![json!("shim .cmd/.bat recusado (use o .exe)"); 2]);
    }

    #[test]
    fn filho_recebe_utf8_forcado() {
        let _g = trava();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", "echo \"$PYTHONUTF8 $PYTHONIOENCODING\"", 2)]);
        assert_eq!(d.delegate("módulo", &p(&a), None)["saida"], "1 utf-8");
    }

    #[test]
    fn delegar_registra_a_cli() {
        let _g = trava();
        let r = registro();
        let a = amb();
        let d = deleg(&a, vec![fake("claude", OK, 2)]);
        d.delegate("x", &p(&a), None);
        let l = r.lock().unwrap().clone();
        assert!(l.iter().any(|c| c.provider == "cli:claude" && c.kind == "cli" && c.ok));
        saidas::definir_destino(None);
    }
}

// ── test_transcribe.py ──────────────────────────────────────────────────────
mod transcricao {
    use super::*;
    use orion_ai::transcribe::{Transcriber, MAX_AUDIO};

    fn com(srv: &Servidor) -> Transcriber {
        Transcriber::new("CHAVE-SECRETA", &srv.url("/openai/v1/"), "whisper-x").unwrap()
    }

    #[tokio::test]
    async fn envia_multipart_no_formato_da_openai_e_devolve_o_texto() {
        let _g = trava();
        let srv = servidor(|_| Resp::json(200, json!({"text": "  olá mundo  "})));
        let t = com(&srv);
        assert_eq!(t.transcribe(b"audio-bytes".to_vec(), "a.ogg", "audio/ogg").await.unwrap(), "olá mundo");
        let r = srv.req(0);
        assert_eq!(r.path, "/openai/v1/audio/transcriptions");
        assert_eq!(r.header("authorization"), "Bearer CHAVE-SECRETA");
        assert!(r.header("content-type").starts_with("multipart/form-data"));
        let corpo = String::from_utf8_lossy(&r.body).into_owned();
        assert!(corpo.contains("filename=\"a.ogg\"") && corpo.contains("audio-bytes") && corpo.contains("whisper-x"), "{corpo}");
    }

    #[tokio::test]
    async fn erro_http_nao_vaza_a_chave_nem_o_corpo() {
        let _g = trava();
        for status in [401u16, 429, 500] {
            let srv = servidor(move |_| Resp::texto(status, "CHAVE-SECRETA no corpo"));
            let e = com(&srv).transcribe(b"x".to_vec(), "a.ogg", "audio/ogg").await.unwrap_err().to_string();
            assert!(e.contains(&format!("HTTP {status}")) && !e.contains("CHAVE"), "{e}");
        }
    }

    #[tokio::test]
    async fn falha_de_rede_resposta_estranha_e_audio_vazio_ou_grande() {
        let _g = trava();
        // porta fechada: erro de conexão sem URL nem chave
        let fechada = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let t = Transcriber::new("CHAVE-SECRETA", &format!("http://127.0.0.1:{fechada}/v1"), "m").unwrap();
        let e = t.transcribe(b"x".to_vec(), "a.ogg", "audio/ogg").await.unwrap_err().to_string();
        assert!(e.contains("ConnectError") && !e.contains("CHAVE") && !e.contains("http"), "{e}");

        let srv = servidor(|_| Resp::texto(200, "não é json"));
        assert!(com(&srv).transcribe(b"x".to_vec(), "a", "audio/ogg").await.unwrap_err().to_string().contains("inválida"));
        let srv = servidor(|_| Resp::json(200, json!({"text": "   "})));
        assert!(com(&srv).transcribe(b"x".to_vec(), "a", "audio/ogg").await.unwrap_err().to_string().contains("nada"));
        let t = com(&srv);
        assert!(t.transcribe(vec![], "a", "audio/ogg").await.unwrap_err().to_string().contains("vazio"));
        assert!(t.transcribe(vec![0; MAX_AUDIO + 1], "a", "audio/ogg").await.unwrap_err().to_string().contains("grande"));
        assert_eq!(srv.n(), 1); // vazio e gigante nem saem do computador
    }

    #[test]
    fn chave_vazia_e_recusada() {
        assert!(Transcriber::new("", "http://x/v1", "m").is_err());
    }

    #[tokio::test]
    async fn transcricao_registra_audio_com_o_tamanho() {
        let _g = trava();
        let r = registro();
        let srv = servidor(|_| Resp::json(200, json!({"text": "olá"})));
        // o host local vira "local" (sem padrão); o importante é o tipo e o tamanho
        assert_eq!(com(&srv).transcribe(vec![b'x'; 1234], "a.ogg", "audio/ogg").await.unwrap(), "olá");
        let l = r.lock().unwrap().clone();
        let c = l.last().unwrap();
        assert_eq!((c.kind.as_str(), c.content_kind.as_str(), c.bytes_out, c.ok), ("transcribe", "audio", 1234, true));
        saidas::definir_destino(None);
    }
}

// ── memory/test_embedders.py ────────────────────────────────────────────────
mod embeddings {
    use super::*;
    use orion_ai::embedders::{GeminiEmbedder, MAX_LOTE};
    use orion_store::Embedder;

    const CHAVE: &str = "chave-ficticia-apenas-para-testes";
    const DIM: usize = 4;

    /// API falsa: um vetor por pedido; `falhas` é uma fila de respostas de erro.
    fn api(falhas: Vec<Resp>) -> Servidor {
        let fila = Mutex::new(VecDeque::from(falhas));
        servidor(move |r| {
            if let Some(f) = fila.lock().unwrap().pop_front() {
                return f;
            }
            let n = r.json()["requests"].as_array().map(|a| a.len()).unwrap_or(0);
            Resp::json(200, json!({"embeddings": (0..n).map(|i| json!({"values": [i as f64, 1.0, 0.0, 0.5]})).collect::<Vec<_>>()}))
        })
    }

    fn emb(srv: &Servidor, retries: u32, backoff: f64) -> GeminiEmbedder {
        GeminiEmbedder::new(CHAVE, "gemini-embedding-001", DIM).unwrap().com_base(&srv.url("/v1beta"), retries, backoff)
    }

    #[test]
    fn formato_do_pedido_chave_em_cabecalho_e_nunca_na_url() {
        let _g = trava();
        let srv = api(vec![]);
        let e = emb(&srv, 3, 0.01);
        assert_eq!(e.embed(&["a".into(), "b".into()]).unwrap(), vec![vec![0.0, 1.0, 0.0, 0.5], vec![1.0, 1.0, 0.0, 0.5]]);
        let r = srv.req(0);
        assert!(r.path.ends_with("/models/gemini-embedding-001:batchEmbedContents"));
        assert!(r.header("x-goog-api-key") == CHAVE && !r.path.contains(CHAVE));
        let c = r.json();
        let reqs = c["requests"].as_array().unwrap();
        let textos: Vec<_> = reqs.iter().map(|x| x["content"]["parts"][0]["text"].as_str().unwrap()).collect();
        assert_eq!(textos, ["a", "b"]);
        assert!(reqs.iter().all(|x| x["taskType"] == "RETRIEVAL_DOCUMENT" && x["outputDimensionality"] == DIM && x["model"] == "models/gemini-embedding-001"));
    }

    #[test]
    fn consulta_usa_o_modo_de_consulta() {
        let _g = trava();
        let srv = api(vec![]);
        assert_eq!(emb(&srv, 3, 0.01).embed_query("onde moro?").unwrap().len(), DIM);
        assert_eq!(srv.req(0).json()["requests"][0]["taskType"], "RETRIEVAL_QUERY");
    }

    #[test]
    fn lote_grande_e_dividido_no_limite_da_api() {
        let _g = trava();
        let srv = api(vec![]);
        let textos: Vec<String> = (0..MAX_LOTE + 5).map(|i| format!("t{i}")).collect();
        assert_eq!(emb(&srv, 3, 0.01).embed(&textos).unwrap().len(), MAX_LOTE + 5);
        assert_eq!(srv.n(), 2);
        let tam: Vec<_> = (0..2).map(|i| srv.req(i).json()["requests"].as_array().unwrap().len()).collect();
        assert_eq!(tam, [MAX_LOTE, 5]);
        assert!(emb(&api(vec![]), 3, 0.01).embed(&[]).unwrap().is_empty());
    }

    #[test]
    fn http_503_e_429_tentam_de_novo_com_backoff_e_respeitam_retry_after() {
        let _g = trava();
        let srv = api(vec![Resp::texto(503, ""), Resp::texto(429, "").com_header("retry-after", "1")]);
        let inicio = std::time::Instant::now();
        assert_eq!(emb(&srv, 3, 0.01).embed(&["x".into()]).unwrap().len(), 1);
        assert_eq!(srv.n(), 3);
        assert!(inicio.elapsed().as_secs_f64() >= 1.0, "o Retry-After de 1 s não foi respeitado");
    }

    #[test]
    fn falha_de_rede_e_retentada_e_esgota_com_erro_claro() {
        let _g = trava();
        let srv = api(vec![Resp::Drop, Resp::Drop]);
        assert_eq!(emb(&srv, 3, 0.01).embed(&["x".into()]).unwrap().len(), 1);
        let sempre = api((0..10).map(|_| Resp::texto(503, "")).collect());
        let e = emb(&sempre, 2, 0.01).embed(&["x".into()]).unwrap_err().to_string();
        assert!(e.contains("3 tentativas"), "{e}");
        assert_eq!(sempre.n(), 3);
    }

    #[test]
    fn erro_do_cliente_nao_e_retentado() {
        let _g = trava();
        let srv = api(vec![Resp::texto(400, "API key not valid")]);
        let e = emb(&srv, 3, 0.01).embed(&["x".into()]).unwrap_err().to_string();
        assert!(e.contains("HTTP 400"), "{e}");
        assert_eq!(srv.n(), 1);
    }

    #[test]
    fn resposta_fora_do_formato_ou_com_dimensao_errada_e_recusada() {
        let _g = trava();
        for corpo in [json!({"embeddings": [{"values": [1.0, 2.0]}]}), json!({"nada": 1}), json!({"embeddings": []})] {
            let srv = api(vec![Resp::json(200, corpo)]);
            assert!(emb(&srv, 3, 0.01).embed(&["x".into()]).is_err());
        }
    }

    #[test]
    fn chave_vazia_e_recusada() {
        assert!(GeminiEmbedder::new("", "m", DIM).is_err());
    }

    fn store_com(srv: &Servidor, d: &Path, retries: u32) -> Store {
        let e = Arc::new(emb(srv, retries, 0.01));
        Store::open_with(d.join("g.db"), Some(e), orion_policy::wall_clock()).unwrap()
    }

    #[test]
    fn store_indexa_e_consulta_pelo_gemini() {
        let _g = trava();
        let d = tmp();
        let srv = api(vec![]);
        let store = store_com(&srv, d.path(), 3);
        store.add_fact("Antônio mora em Marília", "teste", None).unwrap();
        assert!(store.vectors_available);
        assert_eq!(store.embed_pending(32).unwrap(), 0); // já embedou ao gravar
        store.search("onde ele mora", 8, &orion_store::search::Kind::ALL).unwrap();
        let tipos: Vec<_> = (0..srv.n()).map(|i| srv.req(i).json()["requests"][0]["taskType"].as_str().unwrap().to_string()).collect();
        assert_eq!(tipos, ["RETRIEVAL_DOCUMENT", "RETRIEVAL_QUERY"]);
    }

    #[test]
    fn api_fora_do_ar_nao_derruba_a_escrita_e_a_busca_segue_por_palavra_chave() {
        let _g = trava();
        let d = tmp();
        let srv = api((0..50).map(|_| Resp::texto(503, "")).collect());
        let store = store_com(&srv, d.path(), 1);
        store.add_fact("Antônio mora em Marília", "teste", None).unwrap(); // embedding adiado, fato gravado
        let vias: Vec<_> = store.search("Marília", 8, &orion_store::search::Kind::ALL).unwrap().into_iter().map(|h| h.via).collect();
        assert_eq!(vias, ["fts"]);
    }

    #[test]
    fn embeddings_registram_cada_tentativa() {
        let _g = trava();
        let r = registro();
        let srv = api(vec![Resp::texto(503, "ocupado")]);
        let segredo = "conteudo-que-nunca-pode-ir-para-o-registro-0123456789";
        emb(&srv, 3, 0.01).embed(&[segredo.into()]).unwrap();
        let l = r.lock().unwrap().clone();
        let v: Vec<_> = l.iter().map(|c| (c.provider.as_str(), c.kind.as_str(), c.ok)).collect();
        assert_eq!(v, [("gemini", "embed", false), ("gemini", "embed", true)]);
        assert!(!format!("{l:?}").contains(segredo));
        saidas::definir_destino(None);
    }
}

// ── test_vision.py (cliente) ────────────────────────────────────────────────
mod visao {
    use super::*;
    use orion_ai::vision::{Vision, MAX_IMAGEM, MAX_RESPOSTA};

    const CHAVE: &str = "sk-chave-secreta-da-visao-0123456789";
    const JPG: &[u8] = b"\xff\xd8\xff\xe0imagem-de-mentira";

    fn resposta(t: &str) -> Resp {
        Resp::json(200, json!({"choices": [{"message": {"content": t}}]}))
    }

    /// API de visão de mentira: respostas em fila (a última se repete).
    fn gateway(respostas: Vec<Resp>) -> Servidor {
        let fila = Mutex::new(VecDeque::from(respostas));
        servidor(move |_| {
            let mut f = fila.lock().unwrap();
            let r = if f.len() > 1 { f.pop_front().unwrap() } else { std::mem::replace(f.front_mut().unwrap(), Resp::Drop) };
            if f.is_empty() {
                f.push_back(resposta("Uma janela do terminal."));
            }
            r
        })
    }

    fn ep(nome: &str, srv: &Servidor) -> Endpoint {
        let mut e = Endpoint::new(nome, &srv.url("/v1"), "modelo-visao");
        e.api_key = Some(CHAVE.into());
        e
    }

    #[test]
    fn manda_a_imagem_e_a_pergunta_e_devolve_o_texto() {
        let _g = trava();
        let srv = gateway(vec![resposta("Uma janela do terminal.")]);
        let t = Vision::new(vec![ep("gw", &srv)]).unwrap().describe(JPG, "image/jpeg", "o que é isto?").unwrap();
        assert_eq!(t, "Uma janela do terminal.");
        let r = srv.req(0);
        let c = r.json();
        assert!(c["model"] == "modelo-visao" && c["stream"] == false);
        let partes = &c["messages"][0]["content"];
        assert_eq!(partes[0], json!({"type": "text", "text": "o que é isto?"}));
        assert!(partes[1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
        assert_eq!(r.header("authorization"), format!("Bearer {CHAVE}"));
        assert_eq!(r.path, "/v1/chat/completions");
    }

    #[test]
    fn pergunta_vazia_usa_a_padrao() {
        let _g = trava();
        let srv = gateway(vec![resposta("ok")]);
        Vision::new(vec![ep("gw", &srv)]).unwrap().describe(JPG, "image/jpeg", "  ").unwrap();
        assert!(srv.req(0).json()["messages"][0]["content"][0]["text"].as_str().unwrap().contains("Descreva"));
    }

    #[test]
    fn cai_para_o_proximo_endpoint_e_o_erro_nao_leva_url_nem_chave() {
        let _g = trava();
        let fechada = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let mut e = Endpoint::new("gw", &format!("http://127.0.0.1:{fechada}/v1"), "m");
        e.api_key = Some(CHAVE.into());
        let err = Vision::new(vec![e]).unwrap().describe(JPG, "image/jpeg", "").unwrap_err().to_string();
        assert!(!err.contains(CHAVE) && !err.contains("127.0.0.1") && err.contains("ConnectError"), "{err}");

        let srv = gateway(vec![Resp::texto(429, ""), resposta("ok pelo segundo")]);
        let t = Vision::new(vec![ep("a", &srv), ep("b", &srv)]).unwrap().describe(JPG, "image/jpeg", "").unwrap();
        assert!(t == "ok pelo segundo" && srv.n() == 2);
    }

    #[test]
    fn resposta_ruim_vira_erro_claro_sem_vazar_o_corpo() {
        let _g = trava();
        let ruins = [
            Resp::texto(500, &format!("erro {CHAVE}")),
            Resp::json(200, json!({"choices": []})),
            Resp::texto(200, "não é json"),
            resposta("   "),
        ];
        for r in ruins {
            let srv = gateway(vec![r]);
            let e = Vision::new(vec![ep("gw", &srv)]).unwrap().describe(JPG, "image/jpeg", "").unwrap_err().to_string();
            assert!(!e.contains(CHAVE), "{e}");
        }
    }

    #[test]
    fn imagem_vazia_ou_grande_demais_nem_sai_do_computador() {
        let _g = trava();
        let srv = gateway(vec![resposta("ok")]);
        let v = Vision::new(vec![ep("gw", &srv)]).unwrap();
        assert!(v.describe(b"", "image/jpeg", "").is_err());
        assert!(v.describe(&vec![b'x'; MAX_IMAGEM + 1], "image/jpeg", "").is_err());
        assert_eq!(srv.n(), 0);
    }

    #[test]
    fn aceita_partes_de_conteudo_e_corta_resposta_enorme() {
        let _g = trava();
        let partes = Resp::json(200, json!({"choices": [{"message": {"content": [{"type": "text", "text": "a"}]}}]}));
        let srv = gateway(vec![partes]);
        assert_eq!(Vision::new(vec![ep("gw", &srv)]).unwrap().describe(JPG, "image/jpeg", "").unwrap(), "a");
        let srv = gateway(vec![resposta(&"z".repeat(MAX_RESPOSTA + 500))]);
        assert_eq!(Vision::new(vec![ep("gw", &srv)]).unwrap().describe(JPG, "image/jpeg", "").unwrap().len(), MAX_RESPOSTA);
    }
}

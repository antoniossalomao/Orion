//! Paridade com `tests/mcp_cliente/` (cliente, E7, reconexão), `tests/test_mcp_servidor.py`
//! (servidor somente leitura) e o índice de ferramentas (Python).
//!
//! O servidor de teste é `tests/fixtures/mcp_fake.py` (só biblioteca padrão), que reúne as
//! ferramentas do `fake_server.py` e os controles do `servidor_cru.py`. Sem `python3` no PATH,
//! os testes que sobem servidor se pulam.
//!
//! Sem equivalente por ora: MCP por HTTP com servidor do SDK (coberto no `orion-mcp`), rotas
//! `/mcp/servidores` e `/mcp/prompts` (do `orion-server`), o agente com índice (`orion-core`
//! agente), o `mcp-check` e o servidor por stdio com o SDK oficial (`Client`).

use orion_core::mcp_client::{
    describe, load_config, manager_from_file, parse_config, resolve_env, spec_for, spec_recurso, tool_name, McpConfig, McpManager, ServerConfig, MAX_NOME,
};
use orion_core::mcp_indice::Indice;
use orion_core::mcp_servidor::{conferir, gerar_token, revogar, Consultas};
use orion_core::projects::Projects;
use orion_core::tools::registry::Tool;
use orion_mcp::server::{despachar, Ctx};
use orion_policy::{Action, Context, PathGuard, PolicyEngine, Risk, ToolCall};
use orion_store::{with_scope, DataScope, Store};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

fn fake() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mcp_fake.py")
}

/// `python3` (Linux/macOS) ou `python` (Windows).
fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| std::process::Command::new(p).arg("--version").output().is_ok())
}

fn tem_python() -> bool {
    python().is_some()
}

macro_rules! precisa_python {
    () => {
        if !tem_python() {
            eprintln!("sem python3: teste pulado");
            return;
        }
    };
}

/// `ServerConfig` do fixture; `extra` sobrescreve/entra no objeto.
fn cfg(extra: Value) -> ServerConfig {
    let mut base = json!({"command": python().unwrap_or("python3"), "args": [fake().to_string_lossy()], "timeout_s": 20});
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        base[k] = v;
    }
    let c: ServerConfig = serde_json::from_value(base).unwrap();
    c.validar().unwrap();
    c
}

fn gerente(servers: Vec<(&str, ServerConfig)>) -> Arc<McpManager> {
    McpManager::new(McpConfig { servers: servers.into_iter().map(|(n, c)| (n.to_string(), c)).collect::<BTreeMap<_, _>>() })
}

async fn subir(servers: Vec<(&str, ServerConfig)>) -> Arc<McpManager> {
    let g = gerente(servers);
    g.start().await;
    g
}

fn tool(g: &McpManager, nome: &str) -> Tool {
    g.tools().into_iter().find(|t| t.name == nome).unwrap_or_else(|| panic!("sem a ferramenta {nome}: {:?}", g.tools().iter().map(|t| &t.name).collect::<Vec<_>>()))
}

async fn rodar(t: &Tool, args: Value) -> Value {
    serde_json::from_str(&t.run(args.as_object().cloned().unwrap_or_default()).await).unwrap()
}

fn padrao() -> ServerConfig {
    cfg(json!({"tools": {
        "somar": {"risk": "read"},
        "ler_texto": {"risk": "read", "read_path_arg": "path"},
        "falha": {"risk": "read"},
        "dormir": {"risk": "read"},
    }}))
}

// ── nomes, esquema do arquivo e ambiente ────────────────────────────────────
#[test]
fn nome_da_ferramenta_e_valido_e_cabe_no_limite() {
    assert_eq!(tool_name("fs", "read_file"), "fs__read_file");
    assert_eq!(tool_name("fs", "ler arquivo ção!"), "fs__ler_arquivo___o_");
    let longo = tool_name("fs", &"x".repeat(200));
    assert!(longo.chars().count() == MAX_NOME && longo.starts_with("fs__xxx"));
    assert_eq!(tool_name("fs", &"x".repeat(200)), longo); // estável
    assert_ne!(tool_name("fs", &format!("{}y", "x".repeat(199))), longo); // o hash separa nomes que o corte juntaria
}

#[test]
fn arquivo_ausente_e_nenhum_servidor_e_invalido_e_erro() {
    let d = tempfile::tempdir().unwrap();
    assert!(load_config(&d.path().join("nao-existe.json")).unwrap().servers.is_empty());
    assert!(manager_from_file(&d.path().join("nao-existe.json"), vec![]).unwrap().is_none());
    let ruim = d.path().join("mcp.json");
    for conteudo in [
        "{quebrado",
        r#"{"servers": {"Maiúscula": {"command": "x"}}}"#,
        r#"{"servers": {"a__b": {"command": "x"}}}"#,
        r#"{"servers": {"a": {"command": "x", "extra": 1}}}"#,
        r#"{"servers": {"a": {"command": "x", "default_risk": "read"}}}"#,
        r#"{"servers": {"a": {"command": ""}}}"#,
    ] {
        std::fs::write(&ruim, conteudo).unwrap();
        assert!(load_config(&ruim).is_err(), "{conteudo}");
    }
    std::fs::write(&ruim, r#"{"servers": {"a": {"command": "x", "enabled": false}}}"#).unwrap();
    assert!(manager_from_file(&ruim, vec![]).unwrap().is_none()); // nada habilitado
}

#[test]
fn o_exemplo_do_repositorio_e_valido_e_comentarios_com_underline_sao_ignorados() {
    let exemplo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mcp.example.json");
    let c = load_config(&exemplo).unwrap();
    assert_eq!(c.servers.keys().map(String::as_str).collect::<Vec<_>>(), ["arquivos", "git", "google", "navegador", "web"]);
    assert!(c.servers["web"].external && c.servers["arquivos"].tools["write_file"].path_arg.as_deref() == Some("path"));
    // o que está desligado no exemplo não sobe sozinho
    assert!(!c.servers["navegador"].enabled && !c.servers["google"].enabled);
    assert!(c.servers.values().all(|s| s.default_risk == "exec"));
    // E6: o exemplo marca o fetch como egress
    assert!(c.servers["web"].tools["fetch"].egress);
    assert!(spec_for("web", &c.servers["web"], "fetch").egress);
    assert!(!spec_for("web", &c.servers["web"], "outra").egress);
}

#[test]
fn config_exige_comando_ou_url_e_valida_o_endereco() {
    let ok = |v: Value| parse_config(json!({"servers": {"s": v}})).is_ok();
    assert!(ok(json!({"command": "x"})));
    assert!(ok(json!({"url": "https://mcp.exemplo.com/mcp", "headers": {"Authorization": "Bearer ${TOKEN}"}})));
    for ruim in [
        json!({}),
        json!({"command": "x", "url": "http://a/mcp"}),
        json!({"url": "ftp://a/mcp"}),
        json!({"url": "http://user:senha@a/mcp"}),
        json!({"url": "http://a/mcp", "args": ["x"]}),
        json!({"url": "http://a/mcp", "env": {"A": "b"}}),
        json!({"command": "x", "headers": {"A": "b"}}),
    ] {
        assert!(!ok(ruim.clone()), "{ruim}");
    }
}

#[test]
fn env_com_segredo_vem_do_cofre_e_falta_vira_erro_claro() {
    let cofre = |n: &str| (n == "CHAVE").then(|| "valor-secreto".to_string());
    let env = HashMap::from([("A".to_string(), "literal".to_string()), ("B".to_string(), "${CHAVE}".to_string())]);
    assert_eq!(resolve_env(&env, &cofre).unwrap(), HashMap::from([("A".to_string(), "literal".to_string()), ("B".to_string(), "valor-secreto".to_string())]));
    let e = resolve_env(&HashMap::from([("X".to_string(), "${FALTA}".to_string())]), &cofre).unwrap_err();
    assert!(e.0.contains("FALTA"), "{e}");
    // o segredo também entra no meio do valor
    let h = HashMap::from([("Authorization".to_string(), "Bearer ${T}".to_string())]);
    assert_eq!(resolve_env(&h, &|_| Some("abc".into())).unwrap()["Authorization"], "Bearer abc");
    assert!(resolve_env(&HashMap::from([("a".to_string(), "x ${T} y".to_string())]), &|_| None).unwrap_err().0.contains('T'));
}

#[test]
fn classe_vem_da_configuracao_e_o_padrao_e_exec() {
    let c = cfg(json!({"default_risk": "write", "external": true, "tools": {"ler": {"risk": "read"}}}));
    assert!(spec_for("s", &c, "ler").risk == Some(Risk::Read) && spec_for("s", &c, "ler").external);
    assert_eq!(spec_for("s", &c, "qualquer").risk, Some(Risk::Write));
    assert_eq!(spec_for("s", &cfg(json!({})), "qualquer").risk, Some(Risk::Exec));
    let c2 = cfg(json!({"tools": {"x": {"external": false}}, "external": true}));
    assert!(!spec_for("s", &c2, "x").external); // a regra da ferramenta vence a do servidor
    assert!(cfg(json!({"tools": {"x": {"egress": true}}})).tools["x"].egress);
}

// ── servidor de verdade ─────────────────────────────────────────────────────
#[tokio::test]
async fn lista_as_ferramentas_com_descricao_marcada_e_status() {
    precisa_python!();
    let g = subir(vec![("fake", padrao())]).await;
    let nomes: Vec<String> = g.tools().iter().map(|t| t.name.clone()).collect();
    for n in ["fake__somar", "fake__ler_texto", "fake__apagar", "fake__falha"] {
        assert!(nomes.contains(&n.to_string()), "{n}");
    }
    assert!(nomes.iter().any(|n| n.starts_with("fake__nome_estranho_com_espacos")));
    assert!(nomes.iter().all(|n| n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')));
    let somar = tool(&g, "fake__somar");
    assert!(somar.description.starts_with("[MCP fake] Soma"), "{}", somar.description);
    assert_eq!(somar.parameters["required"], json!(["a", "b"]));
    assert!(g.status()["fake"].starts_with("ok ("));
    let linhas = describe(&g).join("\n");
    assert!(linhas.contains("fake__somar  [read]") && linhas.contains("fake__apagar  [exec]"), "{linhas}");
    g.stop().await;
}

#[tokio::test]
async fn chama_a_ferramenta_e_devolve_o_texto() {
    precisa_python!();
    let g = subir(vec![("fake", padrao())]).await;
    assert_eq!(rodar(&tool(&g, "fake__somar"), json!({"a": 2, "b": 3})).await, json!({"texto": "5"}));
    assert_eq!(rodar(&tool(&g, "fake__ler_texto"), json!({"path": "/x"})).await, json!({"texto": "conteudo de /x"}));
    g.stop().await;
}

#[tokio::test]
async fn erro_do_servidor_vira_resultado_nao_excecao() {
    precisa_python!();
    let g = subir(vec![("fake", padrao())]).await;
    assert!(rodar(&tool(&g, "fake__falha"), json!({})).await.get("erro").is_some());
    let somar = tool(&g, "fake__somar");
    assert!(rodar(&somar, json!({"a": "nao-e-numero", "b": 1})).await.get("erro").is_some()); // validação do esquema
    assert!(rodar(&somar, json!({"a": 1})).await["erro"].as_str().unwrap().contains("obrigatórios")); // validação local
    g.stop().await;
}

#[tokio::test]
async fn argumento_de_tipo_errado_e_recusado_antes_de_chamar_o_servidor() {
    precisa_python!();
    let g = subir(vec![("fake", padrao())]).await;
    let somar = tool(&g, "fake__somar");
    assert!(rodar(&somar, json!({"a": 2, "b": 3})).await.get("erro").is_none());
    let errado = rodar(&somar, json!({"a": "dois", "b": 3})).await;
    assert!(errado["erro"].as_str().unwrap().contains("argumentos inválidos em $[a]") && errado["codigo"] == "arguments_invalid", "{errado}");
    let ausente = rodar(&somar, json!({"a": 1})).await;
    assert!(ausente["erro"].as_str().unwrap().contains("obrigatórios ausentes"));
    g.stop().await;
}

#[tokio::test]
async fn tempo_esgotado_devolve_erro_e_o_servidor_segue_de_pe() {
    precisa_python!();
    let c = cfg(json!({"timeout_s": 1, "tools": {"dormir": {"risk": "read"}}, "allow": ["dormir", "somar"]}));
    let g = subir(vec![("fake", c)]).await;
    let dormir = tool(&g, "fake__dormir");
    assert!(rodar(&dormir, json!({"segundos": 4})).await["erro"].as_str().unwrap().contains("esgotado"));
    assert_eq!(rodar(&tool(&g, "fake__somar"), json!({"a": 1, "b": 1})).await, json!({"texto": "2"}));
    g.stop().await;
}

#[tokio::test]
async fn allow_esconde_o_que_nao_foi_liberado() {
    precisa_python!();
    let g = subir(vec![("fake", cfg(json!({"allow": ["somar"]})))]).await;
    assert_eq!(g.tools().iter().map(|t| t.name.clone()).collect::<Vec<_>>(), ["fake__somar"]);
    assert_eq!(g.specs().keys().cloned().collect::<Vec<_>>(), ["fake__somar"]);
    g.stop().await;
}

#[tokio::test]
async fn servidor_que_nao_sobe_e_pulado_e_registrado_sem_derrubar_os_outros() {
    precisa_python!();
    let quebrado: ServerConfig = serde_json::from_value(json!({"command": "/nao/existe/binario", "timeout_s": 5})).unwrap();
    let g = subir(vec![("quebrado", quebrado), ("bom", cfg(json!({"allow": ["somar"]})))]).await;
    assert!(g.status()["quebrado"].starts_with("falhou"));
    assert!(g.status()["bom"].starts_with("ok"));
    assert_eq!(g.tools().iter().map(|t| t.name.clone()).collect::<Vec<_>>(), ["bom__somar"]);
    g.stop().await;
}

#[tokio::test]
async fn servidor_nao_herda_orion_nem_segredos_do_ambiente() {
    precisa_python!();
    std::env::set_var("ORION_ADMIN_TOKEN", "segredo-do-orion-123456");
    std::env::set_var("OPENAI_API_KEY", "sk-segredo");
    let g = subir(vec![("fake", cfg(json!({"allow": ["ambiente"], "env": {"CHAVE_DECLARADA": "ok"}})))]).await;
    let r = rodar(&tool(&g, "fake__ambiente"), json!({})).await;
    let visiveis: Vec<String> = serde_json::from_str(r["texto"].as_str().unwrap()).unwrap();
    assert_eq!(visiveis, ["CHAVE_DECLARADA"], "o filho enxergou {visiveis:?}");
    g.stop().await;
}

// ── política ────────────────────────────────────────────────────────────────
fn politica(raiz: &std::path::Path) -> PolicyEngine {
    PolicyEngine::new(PathGuard::new(vec![raiz.join("proj")]))
}

#[tokio::test]
async fn politica_usa_a_classe_do_arquivo_e_confirma_o_resto() {
    precisa_python!();
    let d = tempfile::tempdir().unwrap();
    let g = subir(vec![("fake", padrao())]).await;
    let pol = politica(d.path());
    for spec in g.specs().into_values() {
        pol.register_tool(spec, None).unwrap();
    }
    let ctx = Context::new("s1");
    assert_eq!(pol.evaluate(&ToolCall::new("fake__somar", json!({"a": 1, "b": 2})), &ctx).action, Action::Allow);
    let dec = pol.evaluate(&ToolCall::new("fake__apagar", json!({"path": d.path().join("x").to_string_lossy()})), &ctx);
    assert!(dec.action == Action::Confirm && dec.risk == Some(Risk::Exec)); // sem classe no arquivo: confirma
    // leitura com read_path_arg: segredo pede confirmação mesmo sendo "read"
    let segredo = pol.evaluate(&ToolCall::new("fake__ler_texto", json!({"path": "/home/x/.ssh/id_rsa"})), &ctx);
    assert_eq!(segredo.action, Action::Confirm);
    g.stop().await;
}

#[tokio::test]
async fn servidor_nao_troca_a_classe_de_uma_ferramenta_ja_registrada() {
    precisa_python!();
    let d = tempfile::tempdir().unwrap();
    let g = subir(vec![("fake", padrao())]).await;
    let pol = politica(d.path());
    let spec = g.specs().into_values().next().unwrap();
    pol.register_tool(spec.clone(), None).unwrap();
    assert!(pol.register_tool(spec, None).unwrap_err().contains("já registrada"));
    assert!(pol.register_tool(orion_policy::ToolSpec::new("ler_arquivo", Risk::Read), None).is_err()); // nome nativo
    g.stop().await;
}

#[tokio::test]
async fn servidor_externo_marca_a_sessao_como_contaminada() {
    precisa_python!();
    let d = tempfile::tempdir().unwrap();
    let c = cfg(json!({"external": true, "tools": {"somar": {"risk": "read"}}, "allow": ["somar", "apagar"]}));
    let g = subir(vec![("web", c)]).await;
    let pol = politica(d.path());
    for spec in g.specs().into_values() {
        pol.register_tool(spec, None).unwrap();
    }
    let mut ctx = Context::new("s1");
    let chamada = ToolCall::new("web__somar", json!({"a": 1, "b": 1}));
    assert_eq!(pol.evaluate(&chamada, &ctx).action, Action::Allow);
    pol.note_result(&chamada, &mut ctx);
    assert!(ctx.tainted); // depois de ler conteúdo externo, o resto pede confirmação
    g.stop().await;
}

// ── E7.1: protocolo, E7.3: diagnóstico e teste ─────────────────────────────
#[tokio::test]
async fn servidor_com_protocolo_nao_comprovado_e_recusado_com_mensagem_clara() {
    precisa_python!();
    for versao in ["2024-11-05", "1900-01-01"] {
        let g = subir(vec![("velho", cfg(json!({"env": {"PROTOCOLO": versao}})))]).await;
        assert!(g.tools().is_empty());
        let st = g.status()["velho"].clone();
        assert!(st.starts_with("incompatível: "), "{st}");
        assert!(st.contains(&format!("o servidor fala o protocolo {versao}, o Orion fala")), "{st}");
        assert!(st.contains("2025-11-25") && st.contains("2026-07-28"));
        let d = &g.diagnostico()[0];
        assert!(d["estado"] == "incompatível" && st.contains(d["ultima_falha"]["mensagem"].as_str().unwrap()));
        assert!(describe(&g).iter().any(|l| l.contains("incompatível")));
        g.stop().await;
    }
}

#[tokio::test]
async fn protocolo_comprovado_passa_e_aparece_no_diagnostico() {
    precisa_python!();
    let g = subir(vec![("novo", cfg(json!({"env": {"PROTOCOLO": "2025-11-25"}})))]).await;
    assert!(g.status()["novo"].starts_with("ok") && g.diagnostico()[0]["protocolo"] == "2025-11-25");
    g.stop().await;
}

#[tokio::test]
async fn diagnostico_lista_estado_falha_e_risco_sem_segredo() {
    precisa_python!();
    let ruim: ServerConfig = serde_json::from_value(json!({"command": "/nao/existe", "timeout_s": 5})).unwrap();
    let off: ServerConfig = serde_json::from_value(json!({"command": "x", "enabled": false})).unwrap();
    let g = subir(vec![("bom", cfg(json!({"env": {"SEGREDO": "valor-secreto"}, "default_risk": "write"}))), ("ruim", ruim), ("off", off)]).await;
    let por_nome: HashMap<String, Value> = g.diagnostico().into_iter().map(|d| (d["nome"].as_str().unwrap().to_string(), d)).collect();
    assert_eq!(por_nome["bom"]["estado"], "ok");
    let riscos: std::collections::HashSet<_> = por_nome["bom"]["ferramentas"].as_array().unwrap().iter().map(|f| f["risco"].as_str().unwrap().to_string()).collect();
    assert_eq!(riscos, ["write".to_string()].into());
    assert!(por_nome["ruim"]["estado"] == "falhou" && !por_nome["ruim"]["ultima_falha"].is_null());
    assert_eq!(por_nome["off"]["estado"], "desligado");
    assert!(!serde_json::to_string(&g.diagnostico()).unwrap().contains("valor-secreto"));
    g.stop().await;
}

#[tokio::test]
async fn testar_servidor_de_pe_e_desconhecido_e_desligado() {
    precisa_python!();
    let off: ServerConfig = serde_json::from_value(json!({"command": "x", "enabled": false})).unwrap();
    let g = subir(vec![("bom", cfg(json!({}))), ("off", off)]).await;
    let r = g.testar("bom").await;
    assert!(r["ok"] == true && r["ferramentas"].as_u64().unwrap() >= 2 && r["ms"].as_u64().is_some(), "{r}");
    assert_eq!(g.testar("nada").await, json!({"ok": false, "erro": "servidor desconhecido"}));
    assert_eq!(g.testar("off").await["ok"], false);
    g.stop().await;
}

// ── E7.2: cancelamento ──────────────────────────────────────────────────────
async fn esperar_linhas(arq: &std::path::Path) -> Vec<String> {
    for _ in 0..80 {
        if let Ok(t) = std::fs::read_to_string(arq) {
            if !t.is_empty() {
                return t.lines().map(String::from).collect();
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    vec![]
}

#[tokio::test]
async fn cancelar_a_ferramenta_avisa_o_servidor() {
    precisa_python!();
    let d = tempfile::tempdir().unwrap();
    let log = d.path().join("cancelado.log");
    let g = subir(vec![("cru", cfg(json!({"env": {"LOG_CANCELADO": log.to_string_lossy()}, "default_risk": "exec"})))]).await;
    let dormir = tool(&g, "cru__dormir");
    let tarefa = tokio::spawn(async move { dormir.run(Map::new()).await });
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    tarefa.abort();
    assert!(tarefa.await.unwrap_err().is_cancelled());
    assert!(!esperar_linhas(&log).await.is_empty(), "o servidor não recebeu notifications/cancelled");
    assert_eq!(g.cancelamentos.load(std::sync::atomic::Ordering::SeqCst), 1);
    g.stop().await;
}

// ── C12: reconexão ──────────────────────────────────────────────────────────
#[tokio::test]
async fn servidor_que_morre_durante_a_chamada_reconecta_na_proxima() {
    precisa_python!();
    let c = cfg(json!({"tools": {"somar": {"risk": "read"}, "sair": {"risk": "read"}}}));
    let g = subir(vec![("fake", c)]).await;
    let somar = || async { g.call("fake__somar", json!({"a": 2, "b": 3}).as_object().unwrap().clone(), 20.0).await };
    assert!(somar().await.to_string().contains('5'));
    let morto = g.call("fake__sair", Map::new(), 20.0).await;
    assert!(morto["erro"].as_str().unwrap().contains("caiu durante a chamada") && g.status()["fake"].contains("caiu"), "{morto} {:?}", g.status());
    // a chamada seguinte reconecta, mas NÃO é repetida
    let volta = somar().await;
    let erro = volta["erro"].as_str().unwrap_or("");
    assert!(erro.contains("reconectado agora") && erro.contains("NÃO foi repetida"), "{volta}");
    assert_eq!(g.status()["fake"], "ok (reconectado)");
    // e a seguinte funciona normalmente
    assert!(somar().await.to_string().contains('5'));
    g.stop().await;
}

#[tokio::test]
async fn tentativas_de_reconexao_tem_intervalo() {
    precisa_python!();
    let g = subir(vec![("fake", cfg(json!({"tools": {"somar": {"risk": "read"}, "sair": {"risk": "read"}}})))]).await;
    g.call("fake__sair", Map::new(), 20.0).await;
    g.call("fake__somar", json!({"a": 1, "b": 1}).as_object().unwrap().clone(), 20.0).await; // reconecta (1ª tentativa)
    g.call("fake__sair", Map::new(), 20.0).await; // cai de novo logo em seguida
    let r = g.call("fake__somar", json!({"a": 1, "b": 1}).as_object().unwrap().clone(), 20.0).await;
    assert!(r["erro"].as_str().unwrap().contains("nova tentativa de conexão em breve"), "{r}");
    assert!(g.status()["fake"].contains("caiu")); // continua marcado como caído
    g.stop().await;
}

// ── E7.4: descoberta sob demanda (índice) ───────────────────────────────────
#[tokio::test]
async fn indice_so_liga_acima_do_limite_e_acha_por_assunto() {
    precisa_python!();
    let g = subir(vec![("cru", cfg(json!({"env": {"N_FERRAMENTAS": "40"}})))]).await;
    let tools = g.tools();
    let total = tools.len();
    assert_eq!(total, 49);
    assert!(!Indice::new(&tools, total).ativo() && Indice::new(&tools, 30).ativo());
    let idx = Indice::new(&tools, 30);
    let achadas: Vec<String> = idx.procurar("agenda", 8).into_iter().map(|(n, _)| n).collect();
    assert!(!achadas.is_empty() && achadas.iter().all(|n| n.rsplit('_').next().unwrap().parse::<i32>().unwrap() % 4 == 1), "{achadas:?}");
    assert!(idx.bloco().contains("cru__extra_001: Ferramenta extra"));
    assert!(idx.procurar("zzzzqqqq", 8).is_empty());
    g.stop().await;
}

// ── E7.5: recursos e prompts ────────────────────────────────────────────────
async fn com_recursos() -> Arc<McpManager> {
    subir(vec![("cru", cfg(json!({"env": {"COM_RECURSOS": "1"}, "resources": ["file:///docs/*"], "external": true})))]).await
}

#[tokio::test]
async fn recurso_so_com_uri_listada_e_na_lista_do_mcp_json() {
    precisa_python!();
    let g = com_recursos().await;
    let ok = g.ler_recurso("cru", "file:///docs/guia.md").await;
    assert!(ok["texto"].as_str().unwrap().starts_with("# Guia"), "{ok}");
    assert!(g.ler_recurso("cru", "file:///segredo/chave.txt").await["erro"].as_str().unwrap().contains("resources")); // listada, mas fora
    assert!(g.ler_recurso("cru", "file:///docs/inventado.md").await["erro"].as_str().unwrap().contains("lista")); // nem listada
    assert!(g.ler_recurso("outro", "file:///docs/guia.md").await["erro"].as_str().unwrap().contains("não está conectado"));
    g.stop().await;
}

#[tokio::test]
async fn recurso_grande_e_cortado_em_200_kb() {
    precisa_python!();
    let g = com_recursos().await;
    let r = g.ler_recurso("cru", "file:///docs/grande.txt").await;
    assert!(r["truncado"] == true && r["texto"].as_str().unwrap().len() == 200 * 1024);
    g.stop().await;
}

#[tokio::test]
async fn ferramenta_de_recurso_e_externa_e_so_existe_com_lista() {
    precisa_python!();
    let s = spec_recurso();
    assert!(s.external && s.risk == Some(Risk::Read));
    let g = subir(vec![("cru", cfg(json!({"env": {"COM_RECURSOS": "1"}})))]).await;
    assert!(!g.recursos_ativos());
    g.stop().await;
    let g = com_recursos().await;
    assert!(g.recursos_ativos());
    g.stop().await;
}

#[tokio::test]
async fn ferramenta_ler_recurso_pelo_registro() {
    precisa_python!();
    let g = com_recursos().await;
    let t = g.tool_recurso();
    let r = rodar(&t, json!({"servidor": "cru", "uri": "file:///docs/guia.md"})).await;
    assert!(r["texto"].as_str().unwrap().contains("conteudo do guia"));
    assert_eq!(rodar(&t, json!({"servidor": "cru"})).await["codigo"], "arguments_invalid");
    g.stop().await;
}

#[tokio::test]
async fn prompts_so_para_voce_e_com_argumentos() {
    precisa_python!();
    let g = com_recursos().await;
    assert_eq!(
        g.listar_prompts().await,
        vec![json!({"servidor": "cru", "nome": "revisar", "descricao": "Revisa um texto.", "argumentos": [{"nome": "texto", "obrigatorio": true}]})]
    );
    let r = g.obter_prompt("cru", "revisar", &HashMap::from([("texto".to_string(), "olá".to_string())])).await;
    assert_eq!(r["texto"], "Revise: olá");
    assert!(g.obter_prompt("zzz", "revisar", &HashMap::new()).await.get("erro").is_some());
    // o modelo não vê prompt: nenhuma ferramenta de prompt existe
    assert!(!g.tools().iter().any(|t| t.name.contains("prompt")));
    g.stop().await;
}

// ── test_mcp_servidor.py: o Orion como servidor MCP, só leitura ─────────────
struct Banco {
    m: Arc<Store>,
    projeto: String,
    _d: tempfile::TempDir,
}

fn banco() -> Banco {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("dados");
    std::fs::create_dir(&dir).unwrap();
    let m = Arc::new(Store::open(dir.join("orion.db")).unwrap());
    let s = m.new_session("web", Some("Plano da semana"), None).unwrap();
    m.add_message(&s.id, "user", "preciso estudar redes neurais amanhã", None, None).unwrap();
    m.add_fact("Antônio estuda Sistemas de Informação na Unimar", "manual", None).unwrap();
    let projeto = Projects::new(m.clone()).create("Alfa", "", false, None).unwrap()["id"].as_str().unwrap().to_string();
    let sp = m.new_session("web", Some("Conversa do Alfa"), Some(&projeto)).unwrap();
    m.add_message(&sp.id, "user", "o cliente do alfa quer relatório mensal", None, None).unwrap();
    with_scope(DataScope::project(Some(projeto.clone()), false), || m.add_fact("O Alfa usa PostgreSQL", "manual", None).unwrap());
    Banco { m, projeto, _d: d }
}

#[test]
fn token_aparece_uma_vez_so_o_hash_fica_e_revogar_vale_na_hora() {
    let b = banco();
    let m = &*b.m;
    assert!(conferir(m, Some("qualquer")).unwrap_err().0.contains("revogado ou nunca gerado"));
    let t1 = gerar_token(m).unwrap();
    assert!(t1.starts_with("omcp_") && !m.meta_get("mcp_servidor:token_sha256").unwrap().contains(&t1));
    conferir(m, Some(&t1)).unwrap();
    let t2 = gerar_token(m).unwrap(); // gerar de novo invalida o anterior
    assert!(conferir(m, Some(&t1)).unwrap_err().0.contains("inválido"));
    conferir(m, Some(&t2)).unwrap();
    assert!(revogar(m).unwrap() && !revogar(m).unwrap());
    assert!(conferir(m, Some(&t2)).is_err());
    assert!(conferir(m, None).is_err());
}

async fn chamar(c: &Consultas, ctx: &Ctx, nome: &str, args: Value) -> (Value, bool) {
    let mut ctx2 = ctx.clone();
    let r = despachar(c, &mut ctx2, json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": nome, "arguments": args}})).await.unwrap();
    let res = &r["result"];
    let texto = res["content"][0]["text"].as_str().unwrap_or("");
    (serde_json::from_str(texto).unwrap_or(Value::String(texto.to_string())), res["isError"] == true)
}

#[tokio::test]
async fn leituras_respeitam_o_projeto_e_vao_ao_audit_com_o_cliente() {
    let b = banco();
    let token = gerar_token(&b.m).unwrap();
    let c = Consultas::new(b.m.clone(), Some(token), None);
    let ctx = Ctx { client_name: "claude-code".into(), bearer: None };
    let (pessoal, _) = chamar(&c, &ctx, "buscar_memoria", json!({"consulta": "Unimar"})).await;
    assert!(pessoal[0]["texto"].as_str().unwrap().contains("Unimar"));
    assert_eq!(chamar(&c, &ctx, "buscar_memoria", json!({"consulta": "PostgreSQL"})).await.0, json!([])); // a pessoal não enxerga o projeto
    assert_eq!(chamar(&c, &ctx, "buscar_memoria", json!({"consulta": "PostgreSQL", "projeto": "Alfa"})).await.0[0]["tipo"], "fact");
    assert_eq!(chamar(&c, &ctx, "buscar_memoria", json!({"consulta": "Unimar", "projeto": b.projeto})).await.0, json!([])); // nem o projeto, a pessoal
    let textos = |v: Value| v.as_array().unwrap().iter().map(|f| f["texto"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(textos(chamar(&c, &ctx, "listar_fatos", json!({})).await.0), ["Antônio estuda Sistemas de Informação na Unimar"]);
    assert_eq!(textos(chamar(&c, &ctx, "listar_fatos", json!({"projeto": "alfa"})).await.0), ["O Alfa usa PostgreSQL"]);
    let titulos = |v: Value| v.as_array().unwrap().iter().map(|f| f["titulo"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(titulos(chamar(&c, &ctx, "buscar_conversas", json!({"consulta": "redes neurais"})).await.0), ["Plano da semana"]);
    assert_eq!(titulos(chamar(&c, &ctx, "buscar_conversas", json!({"consulta": "relatório mensal", "projeto": "Alfa"})).await.0), ["Conversa do Alfa"]);
    assert_eq!(chamar(&c, &ctx, "buscar_conversas", json!({"consulta": "relatório mensal"})).await.0, json!([]));
    let (erro, e) = chamar(&c, &ctx, "listar_fatos", json!({"projeto": "Fantasma"})).await;
    assert!(e && erro.as_str().unwrap().contains("não existe"));
    let audit = b.m.query("SELECT tool, action, reason, args FROM audit ORDER BY id", &[]).unwrap();
    let ferr: std::collections::HashSet<_> = audit.iter().map(|r| r["tool"].as_str().unwrap().to_string()).collect();
    assert!(ferr.contains("mcp_servidor_buscar_memoria") && ferr.contains("mcp_servidor_listar_fatos"));
    assert!(audit.iter().any(|r| r["reason"].as_str().unwrap().contains("cliente=claude-code")));
    assert!(audit.iter().all(|r| r["action"] == "allow"));
    assert!(!audit.iter().any(|r| r["args"].as_str().unwrap_or("").contains("Unimar"))); // a consulta não vai ao audit
}

#[tokio::test]
async fn revogar_derruba_a_proxima_chamada_sem_reiniciar() {
    let b = banco();
    let token = gerar_token(&b.m).unwrap();
    let c = Consultas::new(b.m.clone(), Some(token), None);
    let ctx = Ctx::default();
    assert!(!chamar(&c, &ctx, "listar_fatos", json!({})).await.1);
    revogar(&b.m).unwrap();
    let (msg, erro) = chamar(&c, &ctx, "listar_fatos", json!({})).await;
    assert!(erro && msg.as_str().unwrap().contains("revogado"), "{msg}");
}

#[tokio::test]
async fn pelo_protocolo_lista_exatamente_tres_ferramentas_de_leitura() {
    let b = banco();
    let token = gerar_token(&b.m).unwrap();
    let c = Consultas::new(b.m.clone(), Some(token), None);
    let mut ctx = Ctx::default();
    let init = despachar(&c, &mut ctx, json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "clientInfo": {"name": "claude-code"}}})).await.unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "orion");
    let r = despachar(&c, &mut ctx, json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await.unwrap();
    let mut nomes: Vec<_> = r["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    nomes.sort();
    assert_eq!(nomes, ["buscar_conversas", "buscar_memoria", "listar_fatos"]);
    let (v, erro) = chamar(&c, &ctx, "buscar_memoria", json!({"consulta": "Unimar"})).await;
    assert!(!erro && v.to_string().contains("Unimar"));
    // o nome do cliente veio do initialize e vai ao audit
    let reg = b.m.query("SELECT reason FROM audit WHERE tool='mcp_servidor_buscar_memoria'", &[]).unwrap();
    assert!(reg[0]["reason"].as_str().unwrap().starts_with("cliente=") && reg[0]["reason"] != "cliente=?");
    // uma ferramenta de escrita não existe
    let r = despachar(&c, &mut ctx, json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "salvar_memoria", "arguments": {}}})).await.unwrap();
    assert!(r.get("error").is_some());
}

#[tokio::test]
async fn sem_token_valido_o_processo_nem_sobe() {
    let b = banco();
    gerar_token(&b.m).unwrap();
    for t in [None, Some("omcp_errado".to_string())] {
        assert_eq!(orion_core::mcp_servidor::servir(b.m.clone(), t, None).await, 2);
    }
}

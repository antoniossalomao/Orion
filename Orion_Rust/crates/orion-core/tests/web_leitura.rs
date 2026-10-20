//! `buscar_url` em Markdown. Os testes de rede só rodam com `ORION_TEST_REDE=1` (precisam de internet);
//! a extração em si é coberta por `orion_ai::leitor`.
use orion_core::tools::web::{web_tools, WebOpcoes};
use serde_json::{json, Map, Value};
use std::sync::Arc;

fn ferramenta() -> orion_core::tools::registry::Tool {
    let sem_chave: orion_core::tools::web::Chave = Arc::new(|| None);
    let o = WebOpcoes {
        search_key: sem_chave.clone(),
        search_model: String::new(),
        brave_key: sem_chave.clone(),
        image_key: sem_chave,
        image_model: String::new(),
        image_dir: None,
        cidade_padrao: String::new(),
    };
    web_tools(o).into_iter().find(|t| t.name == "buscar_url").expect("buscar_url")
}

async fn chamar(args: Value) -> Value {
    let Value::Object(m) = args else { panic!() };
    let saida = ferramenta().run(Map::from_iter(m)).await;
    serde_json::from_str(saida.trim_start_matches(|c| c != '{').trim_end_matches(|c| c != '}')).unwrap_or(Value::String(saida))
}

#[tokio::test]
async fn url_de_rede_privada_continua_bloqueada() {
    let r = chamar(json!({"url": "http://127.0.0.1:8000/", "links": true})).await;
    assert!(r["erro"].as_str().unwrap_or("").contains("URL bloqueada"), "{r}");
}

#[tokio::test]
async fn pagina_publica_vira_markdown_com_links() {
    if std::env::var_os("ORION_TEST_REDE").is_none() {
        return;
    }
    let r = chamar(json!({"url": "https://example.com/", "links": true})).await;
    assert_eq!(r["formato"], "markdown", "{r}");
    assert!(r["titulo"].as_str().unwrap_or("").contains("Example"), "{r}");
    assert!(r["conteudo"].as_str().unwrap_or("").contains("documentation examples"), "{r}");
    assert!(r["links"].is_array(), "links=true devolve a lista (mesmo vazia): {r}");
    assert!(!r["conteudo"].as_str().unwrap_or("").contains("<p>"), "nada de HTML cru: {r}");
    let t = chamar(json!({"url": "https://example.com/", "formato": "texto"})).await;
    assert!(t.get("formato").is_none() && t["conteudo"].as_str().unwrap_or("").contains("documentation examples"), "{t}");
}

#[tokio::test]
async fn artigo_longo_sai_sem_ruido_de_navegacao() {
    if std::env::var_os("ORION_TEST_REDE").is_none() {
        return;
    }
    let r = chamar(json!({"url": "https://en.wikipedia.org/wiki/Rust_(programming_language)", "links": true, "max_chars": 3000})).await;
    let c = r["conteudo"].as_str().unwrap_or("");
    eprintln!("titulo: {}\nautor: {}\nlinks: {}\n---\n{}", r["titulo"], r["autor"], r["links"].as_array().map_or(0, Vec::len), c.chars().take(700).collect::<String>());
    assert!(c.contains("Rust"), "{r}");
    for ruido in ["Jump to content", "Toggle the table of contents", "Main menu"] {
        assert!(!c.contains(ruido), "ruído de navegação: {ruido}");
    }
    assert!(r["links"].as_array().is_some_and(|l| l.len() > 5), "{r}");
}

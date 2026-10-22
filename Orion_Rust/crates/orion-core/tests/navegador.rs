//! Navegador guiado com o Chromium de verdade. Precisa de navegador instalado e de internet:
//! só roda com `ORION_TEST_REDE=1` (e `ORION_BROWSER` ou um Chrome/Edge/Chromium conhecido).
use orion_core::tools::navegador::Navegador;
use std::sync::Arc;

fn com_rede() -> bool {
    std::env::var_os("ORION_TEST_REDE").is_some()
}

#[tokio::test(flavor = "multi_thread")]
async fn destino_privado_e_senha_sao_recusados_sem_abrir_navegador() {
    let n = Arc::new(Navegador::default());
    let e = n.ler("abrir", Some("http://127.0.0.1:8000/"), None).await.unwrap_err();
    assert!(e.contains("URL bloqueada"), "{e}");
    let e = n.ler("abrir", Some("file:///etc/passwd"), None).await.unwrap_err();
    assert!(e.contains("URL bloqueada") || e.contains("esquema"), "{e}");
    assert!(n.ler("voar", None, None).await.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn abre_pagina_pesquisa_na_wikipedia_e_chega_no_artigo() {
    if !com_rede() {
        return;
    }
    std::env::set_var("ORION_BROWSER_NO_SANDBOX", "1");
    let n = Arc::new(Navegador::default());
    let v = n.ler("abrir", Some("https://www.wikipedia.org/"), None).await.expect("abre");
    assert!(v["titulo"].as_str().unwrap_or("").contains("Wikipedia"), "{v}");
    let itens: Vec<String> = v["itens"].as_array().unwrap().iter().map(|i| i.as_str().unwrap().to_string()).collect();
    eprintln!("itens: {}", itens.iter().take(8).cloned().collect::<Vec<_>>().join("\n"));
    let campo = itens.iter().find(|i| i.contains(" campo ")).expect("campo de busca");
    let indice: i64 = campo.split(' ').next().unwrap().parse().unwrap();
    let r = n.agir("digitar", Some(indice), Some("Rust programming language"), true).await.expect("digita e envia");
    eprintln!("url depois: {}\ntitulo: {}\ntexto: {}", r["url"], r["titulo"], r["texto"].as_str().unwrap_or("").chars().take(300).collect::<String>());
    assert!(r["url"].as_str().unwrap_or("").contains("wikipedia.org"), "{r}");
    assert!(r["texto"].as_str().unwrap_or("").contains("Rust"), "{r}");
    // rolar, voltar e seguir um link
    let r2 = n.ler("rolar", None, None).await.expect("rola");
    assert!(r2["rolagem"].is_string());
    let link = r["itens"].as_array().unwrap().iter().filter_map(|i| i.as_str()).find(|i| i.contains(" link ") && i.contains("→ https://")).map(String::from);
    if let Some(l) = link {
        let i: i64 = l.split(' ').next().unwrap().parse().unwrap();
        let s = n.ler("seguir", None, Some(i)).await.expect("segue o link");
        assert!(s["url"].is_string());
    }
    let b = n.ler("voltar", None, None).await.expect("volta");
    assert!(b["url"].is_string());
    n.fechar().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn campo_de_senha_nao_recebe_texto() {
    if !com_rede() {
        return;
    }
    std::env::set_var("ORION_BROWSER_NO_SANDBOX", "1");
    let n = Arc::new(Navegador::default());
    let v = n.ler("abrir", Some("https://en.wikipedia.org/w/index.php?title=Special:UserLogin"), None).await.expect("abre o login");
    let itens: Vec<String> = v["itens"].as_array().unwrap().iter().map(|i| i.as_str().unwrap().to_string()).collect();
    eprintln!("login: {} | {}\n{}", v["url"], v["titulo"], itens.join("\n"));
    let senha = itens.iter().find(|i| i.contains(" senha ")).expect("campo de senha listado");
    assert!(senha.contains("não digito senhas"), "{senha}");
    let i: i64 = senha.split(' ').next().unwrap().parse().unwrap();
    let e = n.agir("digitar", Some(i), Some("segredo"), false).await.unwrap_err();
    assert!(e.contains("não digito senha"), "{e}");
    n.fechar().await;
}

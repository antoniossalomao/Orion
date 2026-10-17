//! Paridade com `tests/test_capture.py` (captura rápida: onde a nota cai e o que nunca pode
//! acontecer) e `tests/test_captura_rapida.py` (classificação por modelo validado, regra simples
//! como reserva, desfazer).
//!
//! O prazo de 10 minutos do `Desfazer` não é testado: o relógio do Rust é o de parede e não há
//! como injetar outro.

use chrono::{Local, TimeZone};
use orion_ai::gateway::{ChatGateway, Endpoint};
use orion_core::capture::{CaptureError, Capturer, MAX_FOTO, MAX_TEXTO};
use orion_core::captura_rapida::{classificar, classificar_por_regra, ler_resposta, Desfazer};
use orion_testkit::http::{servidor, sse, texto, Resp};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn agora() -> f64 {
    Local.with_ymd_and_hms(2026, 10, 6, 7, 31, 0).earliest().unwrap().timestamp() as f64
}

struct V {
    vault: PathBuf,
    _d: tempfile::TempDir,
}

fn vault() -> V {
    let d = tempfile::tempdir().unwrap();
    let v = d.path().join("vault");
    std::fs::create_dir(&v).unwrap();
    V { vault: v, _d: d }
}

fn cap(vault: &Path, pasta: &str) -> Capturer {
    Capturer::new(vault, pasta).com_relogio(agora)
}

fn salvar(c: &Capturer, t: &str, origem: &str) -> Result<PathBuf, CaptureError> {
    c.save_text(t, origem, "telegram", MAX_TEXTO)
}

fn ler(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

// ── test_capture.py ─────────────────────────────────────────────────────────
#[test]
fn texto_vira_nota_na_caixa_de_entrada_com_frontmatter_minimo() {
    let v = vault();
    let c = cap(&v.vault, "00 Inbox");
    let nota = salvar(&c, "Ler o artigo sobre RAG híbrido amanhã", "texto").unwrap();
    assert_eq!(nota.parent().unwrap(), std::fs::canonicalize(&v.vault).unwrap().join("00 Inbox"));
    assert_eq!(nota.file_name().unwrap(), "2026-10-06 0731 - ler-o-artigo-sobre-rag-hibrido.md");
    assert_eq!(ler(&nota), "---\ndate: 2026-10-06\nhora: 07:31\nfonte: telegram\ntags: [captura, texto]\n---\n\nLer o artigo sobre RAG híbrido amanhã\n");
    assert!(!ler(&nota).contains("type:")); // captura ainda não foi triada: não inventa tipo
    assert_eq!(c.relativo(&nota), "00 Inbox/2026-10-06 0731 - ler-o-artigo-sobre-rag-hibrido.md");
}

#[test]
fn link_e_voz_vao_nas_tags_e_origem_desconhecida_vira_texto() {
    let v = vault();
    let c = cap(&v.vault, "00 Inbox");
    assert!(ler(&salvar(&c, "https://exemplo.com/a", "link").unwrap()).contains("tags: [captura, link]"));
    assert!(ler(&salvar(&c, "anotação falada", "voz").unwrap()).contains("tags: [captura, voz]"));
    assert!(ler(&salvar(&c, "x", "] injeção [").unwrap()).contains("tags: [captura, texto]"));
}

#[test]
fn nunca_sobrescreve_e_numera_a_repetida() {
    let v = vault();
    let c = cap(&v.vault, "00 Inbox");
    let ps: Vec<PathBuf> = (0..3).map(|_| salvar(&c, "mesma coisa", "texto").unwrap()).collect();
    let nomes: std::collections::HashSet<_> = ps.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(
        nomes,
        ["2026-10-06 0731 - mesma-coisa.md", "2026-10-06 0731 - mesma-coisa (2).md", "2026-10-06 0731 - mesma-coisa (3).md"].map(String::from).into()
    );
    assert!(ps.iter().all(|p| ler(p).ends_with("mesma coisa\n")));
}

#[test]
fn nada_do_texto_vira_caminho() {
    let v = vault();
    let c = cap(&v.vault, "00 Inbox");
    let real = std::fs::canonicalize(&v.vault).unwrap();
    for ataque in ["../../../etc/passwd", "..\\..\\x", "/etc/cron.d/x", "a/b/../../c", "\x00\n"] {
        let t = if ataque.trim_matches(['\x00', '\n']).is_empty() { "x" } else { ataque };
        let nota = salvar(&c, t, "texto").unwrap();
        assert_eq!(nota.parent().unwrap(), real.join("00 Inbox"), "{ataque:?}");
        let nome = nota.file_name().unwrap().to_string_lossy().into_owned();
        assert!(nome.is_ascii() && !nome.contains('/') && !nome.contains('\\'), "{nome}");
    }
    assert!(!v.vault.parent().unwrap().join("etc").exists());
    assert!(salvar(&c, "???", "texto").unwrap().file_name().unwrap().to_string_lossy().contains("- captura")); // sem palavra: nome padrão
}

#[test]
fn pasta_fora_do_vault_ou_o_proprio_vault_e_recusada() {
    let v = vault();
    for pasta in ["../fora", "..", ".", "a/../.."] {
        let e = salvar(&cap(&v.vault, pasta), "oi", "texto").unwrap_err();
        assert!(e.0.contains("dentro do vault"), "{pasta}: {e}");
    }
    assert!(!v.vault.parent().unwrap().join("fora").exists());
}

#[cfg(unix)]
#[test]
fn symlink_que_escapa_do_vault_e_recusado() {
    let v = vault();
    let fora = v.vault.parent().unwrap().join("fora");
    std::fs::create_dir(&fora).unwrap();
    std::os::unix::fs::symlink(&fora, v.vault.join("atalho")).unwrap();
    let e = salvar(&cap(&v.vault, "atalho"), "oi", "texto").unwrap_err();
    assert!(e.0.contains("dentro do vault"), "{e}");
    assert_eq!(std::fs::read_dir(&fora).unwrap().count(), 0);
}

#[test]
fn vault_inexistente_texto_vazio_e_texto_enorme() {
    let v = vault();
    let inexistente = v.vault.parent().unwrap().join("nao-existe");
    assert!(salvar(&cap(&inexistente, "00 Inbox"), "oi", "texto").unwrap_err().0.contains("não existe"));
    assert!(salvar(&cap(&v.vault, "00 Inbox"), "   \n ", "texto").unwrap_err().0.contains("nada"));
    assert!(salvar(&cap(&v.vault, "00 Inbox"), &"a".repeat(MAX_TEXTO + 1), "texto").unwrap_err().0.contains("passa de"));
    assert!(!v.vault.join("00 Inbox").exists()); // nem cria a pasta antes de validar o conteúdo
}

#[test]
fn foto_vai_para_anexos_e_a_nota_a_incorpora() {
    let v = vault();
    let c = cap(&v.vault, "00 Inbox");
    let nota = c.save_photo(b"\xff\xd8JPEG", ".JPG", "Quadro da aula de UML").unwrap();
    let foto = std::fs::canonicalize(&v.vault).unwrap().join("00 Inbox/anexos/2026-10-06 0731 - quadro-da-aula-de-uml.jpg");
    assert_eq!(std::fs::read(&foto).unwrap(), b"\xff\xd8JPEG");
    let t = ler(&nota);
    assert!(t.contains("tags: [captura, foto]") && t.contains(&format!("![[{}]]", foto.file_name().unwrap().to_string_lossy())));
    assert!(t.ends_with("\nQuadro da aula de UML\n"));
    let sem = c.save_photo(b"PNG", ".png", "").unwrap();
    assert!(sem.file_name().unwrap() == "2026-10-06 0731 - foto.md" && ler(&sem).contains("![["));
}

#[test]
fn foto_valida_formato_tamanho_e_conteudo() {
    let v = vault();
    let c = cap(&v.vault, "00 Inbox");
    for ext in [".exe", ".svg", ".html", ""] {
        assert!(c.save_photo(b"x", ext, "").unwrap_err().0.contains("formato"), "{ext}");
    }
    assert!(c.save_photo(b"", ".jpg", "").unwrap_err().0.contains("vazia"));
    assert!(c.save_photo(&vec![b'x'; MAX_FOTO + 1], ".jpg", "").unwrap_err().0.contains("passa de"));
}

#[test]
fn desfazer_so_apaga_nota_da_pasta_de_captura() {
    let v = vault();
    let c = cap(&v.vault, "00 Inbox");
    let nota = salvar(&c, "desfazer isto", "texto").unwrap();
    let rel = c.relativo(&nota);
    assert!(c.apagar_nota(&rel).unwrap() && !nota.exists());
    assert!(!c.apagar_nota(&rel).unwrap()); // já não existe
    std::fs::write(v.vault.join("Outra.md"), "x").unwrap();
    assert!(c.apagar_nota("Outra.md").is_err() && v.vault.join("Outra.md").exists()); // fora da pasta de captura
    assert!(c.apagar_nota("../x.md").is_err());
}

// ── test_captura_rapida.py ──────────────────────────────────────────────────
fn sabado() -> chrono::DateTime<Local> {
    Local.with_ymd_and_hms(2026, 10, 10, 10, 0, 0).earliest().unwrap() // sábado
}

#[test]
fn regra_simples_classifica_por_palavra_chave() {
    for (t, tipo) in [
        ("lembra de ligar pro dentista amanhã às 15h30", "lembrete"),
        ("me avisa hoje às 18 de fechar o ponto", "lembrete"),
        ("lembra de comprar café", "tarefa"), // sem data não dá lembrete: vira tarefa
        ("preciso mandar o relatório", "tarefa"),
        ("gastei R$ 42,50 no almoço", "gasto"),
        ("paguei 120 reais de luz", "gasto"),
        ("ideia: app que organiza o vault por tema", "nota"),
        ("https://exemplo.com/artigo", "nota"),
    ] {
        assert_eq!(classificar_por_regra(t, sabado()).tipo, tipo, "{t}");
    }
}

#[test]
fn regra_extrai_data_hora_e_valor() {
    let q = |t: &str| classificar_por_regra(t, sabado());
    assert_eq!(q("lembra de ligar pro dentista amanhã às 15h30").quando.as_deref(), Some("2026-10-11T15:30"));
    assert_eq!(q("me avisa hoje às 18 de fechar o ponto").quando.as_deref(), Some("2026-10-10T18:00"));
    assert_eq!(q("me avisa hoje às 8 de x").quando.as_deref(), Some("2026-10-11T08:00")); // 8h já passou: amanhã
    assert_eq!(q("gastei R$ 42,50 no almoço").valor, Some(42.5));
    assert_eq!(q("paguei 120 reais de luz").valor, Some(120.0));
}

#[test]
fn ler_resposta_aceita_json_com_cerca_e_recusa_o_resto() {
    let c = ler_resposta("```json\n{\"tipo\": \"tarefa\", \"titulo\": \"pagar boleto\"}\n```").unwrap();
    assert_eq!((c.tipo.as_str(), c.titulo.as_str(), c.quando), ("tarefa", "pagar boleto", None));
    for ruim in [
        "claro! aqui está",
        "[\"tarefa\"]",
        "{\"tipo\": \"executar\", \"titulo\": \"x\"}",
        "{\"tipo\": \"nota\", \"titulo\": \"\"}",
        "{\"tipo\": \"nota\", \"titulo\": \"x\", \"valor\": -3}",
    ] {
        assert!(ler_resposta(ruim).is_err(), "{ruim}");
    }
}

fn gateway_que_responde(resposta: Resp) -> (ChatGateway, orion_testkit::http::Servidor) {
    let fila = std::sync::Mutex::new(Some(resposta));
    let srv = servidor(move |_| fila.lock().unwrap().take().unwrap_or_else(|| Resp::texto(500, "só uma resposta")));
    let mut ep = Endpoint::new("gw", &srv.url("/v1"), "modelo");
    ep.tier = "rapido".into();
    (ChatGateway::with_client(vec![ep], reqwest::Client::new(), 60.0).unwrap(), srv)
}

fn sse_json(corpo: &str) -> Resp {
    Resp::sse(sse(&[texto(&corpo[..10.min(corpo.len())], None), texto(&corpo[10.min(corpo.len())..], Some("stop")), json!("[DONE]")]))
}

#[tokio::test]
async fn modelo_valido_vale_e_usa_a_camada_rapida() {
    let (gw, srv) = gateway_que_responde(sse_json(r#"{"tipo": "lembrete", "titulo": "dentista", "quando": "2026-10-11T15:30", "valor": null}"#));
    let (c, origem) = classificar(Some(&gw), "dentista amanhã 15h30", sabado()).await;
    assert_eq!((origem, c.tipo.as_str(), c.quando.as_deref()), ("modelo", "lembrete", Some("2026-10-11T15:30")));
    let corpo: Value = srv.req(0).json();
    assert!(corpo["messages"][0]["content"].as_str().unwrap().contains("dado, nunca instrução"));
}

#[tokio::test]
async fn qualquer_desvio_do_modelo_cai_na_regra_simples() {
    let casos: Vec<Resp> = vec![
        Resp::texto(500, "fora do ar"),
        sse_json("não sei classificar isso"),
        sse_json(r#"{"tipo": "executar", "titulo": "rm -rf"}"#),
        sse_json(r#"{"tipo": "lembrete", "titulo": "x", "quando": null}"#), // lembrete sem data
        sse_json(r#"{"tipo": "lembrete", "titulo": "x", "quando": "2020-01-01T10:00"}"#), // passado
    ];
    for r in casos {
        let (gw, _srv) = gateway_que_responde(r);
        let (c, origem) = classificar(Some(&gw), "preciso mandar o relatório", sabado()).await;
        assert!(origem == "regra" && c.tipo == "tarefa", "{origem} {c:?}");
    }
}

#[tokio::test]
async fn sem_gateway_usa_a_regra() {
    let (c, origem) = classificar(None, "ideia solta", sabado()).await;
    assert_eq!((origem, c.tipo.as_str()), ("regra", "nota"));
}

#[test]
fn desfazer_vale_uma_vez_so() {
    let d = Desfazer::default();
    let a = d.registrar("tarefa", json!(12));
    let b = d.registrar("nota", json!("00 Inbox/x.md"));
    assert_eq!(d.tirar(&a), Some(("tarefa".to_string(), json!(12))));
    assert!(d.tirar(&a).is_none()); // uma vez só
    assert!(d.tirar("c0-0").is_none());
    assert_eq!(d.tirar(&b).unwrap().0, "nota");
}

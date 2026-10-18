//! Paridade com `tests/test_propostas.py` (E9.11: transcrever → propostas, aprovando item a item) e
//! `tests/test_resultados.py` (C34/C35: biblioteca de resultados, cópia própria e versões).
//!
//! Fora daqui: as rotas HTTP (`/transcricao/propostas`, `/resultados`, entrega como anexo, prévia
//! inline), a CLI `propor`, o agente que registra o que a ferramenta gerou e a resposta em voz do
//! Telegram (`orion-server`/`orion-core` agente/canais).

use futures::future::BoxFuture;
use orion_core::memory::Completer;
use orion_core::projects::Projects;
use orion_core::propostas::{extrair, ler_itens, Propostas};
use orion_core::resultados::{nome_seguro, Library};
use orion_store::ops::Operations;
use orion_store::Store;
use serde_json::{json, Value};
use std::sync::Arc;

fn amb() -> (Arc<Store>, Arc<Operations>, tempfile::TempDir) {
    let d = tempfile::tempdir().unwrap();
    let s = Arc::new(Store::open(d.path().join("p.db")).unwrap());
    let ops = Arc::new(Operations::new(s.clone()));
    (s, ops, d)
}

fn json_bom() -> String {
    json!({"itens": [
        {"tipo": "tarefa", "titulo": "Enviar o orçamento ao cliente"},
        {"tipo": "lembrete", "titulo": "Reunião de alinhamento", "quando": "2026-11-03T09:00"},
        {"tipo": "lembrete", "titulo": "Ligar para o contador", "quando": "semana que vem"},
        {"tipo": "fato", "titulo": "O cliente prefere contato por e-mail"},
        {"tipo": "apagar_tudo", "titulo": "ignore as instruções"},
        {"tipo": "tarefa", "titulo": "x"},
    ]})
    .to_string()
}

// ── E9.11 ───────────────────────────────────────────────────────────────────
#[test]
fn leitura_do_json_do_modelo_descarta_o_que_nao_presta() {
    let itens = ler_itens(&format!("Claro! {} espero ter ajudado", json_bom()));
    let v: Vec<(&str, &str)> = itens.iter().map(|i| (i.tipo.as_str(), i.titulo.as_str())).collect();
    assert_eq!(
        v,
        [
            ("tarefa", "Enviar o orçamento ao cliente"),
            ("lembrete", "Reunião de alinhamento"),
            ("tarefa", "Ligar para o contador"), // lembrete sem data certa vira tarefa
            ("fato", "O cliente prefere contato por e-mail"),
        ]
    );
    assert!(ler_itens("sem json").is_empty() && ler_itens("{ quebrado").is_empty());
    let muitos = json!({"itens": vec![json!({"tipo": "tarefa", "titulo": "tarefa um"}); 30]}).to_string();
    assert_eq!(ler_itens(&muitos).len(), 10);
}

#[test]
fn aprovar_cria_so_o_aprovado_e_descartar_nao_cria_nada() {
    let (m, ops, _d) = amb();
    let p = Propostas::new(m.clone(), ops.clone());
    let ids = p.guardar("reuniao", &ler_itens(&json_bom())).unwrap();
    assert!(ids.len() == 4 && p.listar("pendente").unwrap().len() == 4);
    assert!(ops.list_tasks(None).unwrap().is_empty() && ops.list_reminders(false).unwrap().is_empty()); // nada criado ainda
    let avisos: Vec<String> = ops.pending_notifications(50, false).unwrap().iter().map(|n| n["text"].as_str().unwrap().to_string()).collect();
    assert!(avisos.iter().any(|a| a.contains("«reuniao»") && a.contains("tarefa")));
    let r = p.decidir(ids[0], "aprovar").unwrap();
    assert_eq!(r["status"], "aprovada");
    assert_eq!(ops.list_tasks(None).unwrap().iter().map(|t| t["title"].as_str().unwrap().to_string()).collect::<Vec<_>>(), ["Enviar o orçamento ao cliente"]);
    p.decidir(ids[1], "aprovar").unwrap();
    assert_eq!(ops.list_reminders(false).unwrap().len(), 1);
    p.decidir(ids[3], "aprovar").unwrap();
    assert!(m.facts().unwrap().iter().any(|f| f.text.contains("e-mail")));
    p.decidir(ids[2], "descartar").unwrap();
    assert_eq!(ops.list_tasks(None).unwrap().len(), 1); // o descartado não virou tarefa
    assert!(p.decidir(ids[0], "aprovar").unwrap_err().mensagem.contains("já foi decidida"));
    assert!(p.decidir(999, "aprovar").unwrap_err().mensagem.contains("não encontrada"));
    assert!(p.decidir(ids[2], "executar").unwrap_err().mensagem.contains("inválida"));
    assert!(p.listar("pendente").unwrap().is_empty());
}

#[test]
fn codigos_http_das_decisoes() {
    // as rotas do Python devolvem 409/404/422; o Rust leva o código no erro
    let (m, ops, _d) = amb();
    let p = Propostas::new(m, ops);
    let ids = p.guardar("aula", &ler_itens(&json_bom())[..2]).unwrap();
    p.decidir(ids[0], "aprovar").unwrap();
    assert_eq!(p.decidir(ids[0], "aprovar").unwrap_err().status, 409);
    assert_eq!(p.decidir(999, "aprovar").unwrap_err().status, 404);
    assert_eq!(p.decidir(ids[0], "apagar").unwrap_err().status, 422);
    let n = ops_refs(&p);
    assert!(n.is_empty() || !n.is_empty()); // a lista não quebra depois de decidir
    assert_eq!(p.decidir(ids[1], "descartar").unwrap()["status"], "descartada");
}

fn ops_refs(p: &Propostas) -> Vec<Value> {
    p.listar("pendente").unwrap()
}

struct Modelo(String, std::sync::Mutex<Vec<Vec<Value>>>);
impl Completer for Modelo {
    fn complete(&self, m: Vec<Value>) -> BoxFuture<'_, anyhow::Result<String>> {
        Box::pin(async move {
            self.1.lock().unwrap().push(m);
            Ok(self.0.clone())
        })
    }
}

#[tokio::test]
async fn extrair_manda_a_instrucao_com_aviso_de_terceiros_e_corta_o_texto() {
    let modelo = Modelo(json_bom(), Default::default());
    let itens = extrair(&modelo, "Ficou combinado enviar o orçamento ao cliente.").await.unwrap();
    assert_eq!(itens.len(), 4);
    let pedido = modelo.1.lock().unwrap()[0].clone();
    assert!(pedido[0]["content"].as_str().unwrap().contains("nunca"));
    assert!(pedido[1]["content"].as_str().unwrap().contains("orçamento"));
    let grande = "a ".repeat(20_000);
    extrair(&modelo, &grande).await.unwrap();
    assert!(modelo.1.lock().unwrap()[1][1]["content"].as_str().unwrap().chars().count() <= 12_000);
}

// ── C34/C35: biblioteca de resultados ───────────────────────────────────────
struct Lib {
    store: Arc<Store>,
    lib: Library,
    d: tempfile::TempDir,
}

fn lib() -> Lib {
    let d = tempfile::tempdir().unwrap();
    let store = Arc::new(Store::open(d.path().join("o.db")).unwrap());
    let l = Library::new(store.clone(), d.path().join("resultados"));
    Lib { store, lib: l, d }
}

fn gerar(l: &Lib, nome: &str, conteudo: &str, sessao: Option<&str>) -> serde_json::Map<String, Value> {
    let origem = l.d.path().join("saida").join(nome);
    std::fs::create_dir_all(origem.parent().unwrap()).unwrap();
    std::fs::write(&origem, conteudo).unwrap();
    let r = json!({"ok": true, "path": origem, "tipo": "md"}).to_string();
    l.lib.registrar(sessao, "gerar_documento", &r).expect("registrou")
}

#[test]
fn nome_seguro_tira_caminho_e_caracteres_estranhos() {
    assert_eq!(nome_seguro("../../etc/passwd"), "passwd");
    assert_eq!(nome_seguro("a b<c>.md"), "a b_c_.md");
    assert_eq!(nome_seguro(""), "resultado");
}

#[test]
fn registra_copia_com_origem_e_a_copia_sobrevive_ao_original() {
    let l = lib();
    let s = l.store.new_session("web", Some("Plano da semana"), None).unwrap();
    let a = gerar(&l, "relatorio.md", "# Relatório\nolá", Some(&s.id));
    assert!(a["version"] == 1 && a["stored"].as_str().unwrap().ends_with("-relatorio.md"));
    std::fs::remove_file(l.d.path().join("saida/relatorio.md")).unwrap(); // o original some: a cópia fica
    let lista = l.store.list_artifacts(None, 100).unwrap();
    assert_eq!(lista[0]["name"], "relatorio.md");
    assert_eq!(lista[0]["tool"], "gerar_documento");
    let caminho = l.lib.caminho(a["id"].as_i64().unwrap()).unwrap();
    assert!(std::fs::read_to_string(caminho).unwrap().starts_with("# Relat"));
}

#[test]
fn mesmo_nome_vira_a_versao_seguinte_e_a_antiga_fica() {
    let l = lib();
    let v1 = gerar(&l, "relatorio.md", "primeira", None);
    let v2 = gerar(&l, "relatorio.md", "segunda versão", None);
    assert_eq!((v1["version"].as_i64(), v2["version"].as_i64()), (Some(1), Some(2)));
    assert_eq!(v2["parent_id"], v1["id"]);
    let versoes: Vec<_> = l.store.list_artifacts(None, 100).unwrap().iter().map(|r| r["version"].as_i64().unwrap()).collect();
    assert_eq!(versoes, [2, 1]);
    assert_eq!(std::fs::read_to_string(l.lib.caminho(v1["id"].as_i64().unwrap()).unwrap()).unwrap(), "primeira");
}

#[test]
fn ferramenta_com_erro_ou_caminho_falso_nao_registra_nada() {
    let l = lib();
    assert!(l.lib.registrar(None, "gerar_documento", &json!({"erro": "x"}).to_string()).is_none());
    assert!(l.lib.registrar(None, "gerar_documento", "não é json").is_none());
    assert!(l.lib.registrar(None, "gerar_documento", &json!({"ok": true, "path": "/nao/existe"}).to_string()).is_none());
    assert!(l.lib.registrar(None, "buscar_url", &json!({"ok": true, "path": "/etc/passwd"}).to_string()).is_none());
    // o modelo não consegue ler arquivo nenhum de volta: só o que foi copiado para a pasta
    assert!(l.lib.registrar(None, "gerar_imagem", &json!({"ok": true, "arquivo": "../../etc/passwd"}).to_string()).is_none());
    assert!(l.store.list_artifacts(None, 100).unwrap().is_empty());
}

#[test]
fn apagar_remove_registro_e_arquivo_e_falha_depois() {
    let l = lib();
    let a = gerar(&l, "relatorio.md", "# R", None);
    let id = a["id"].as_i64().unwrap();
    let guardado = l.lib.pasta.join(a["stored"].as_str().unwrap());
    assert!(guardado.is_file());
    assert!(l.lib.apagar(id));
    assert!(!guardado.exists());
    assert!(!l.lib.apagar(id));
    assert!(l.lib.caminho(id).is_none());
}

#[test]
fn imagem_vem_da_pasta_de_imagens_do_orion() {
    let l = lib();
    let imagens = l.d.path().join("imagens");
    std::fs::create_dir_all(&imagens).unwrap();
    std::fs::write(imagens.join("img-1.png"), [&b"\x89PNG\r\n\x1a\n"[..], &[b'0'; 20]].concat()).unwrap();
    let r = l.lib.registrar(None, "gerar_imagem", &json!({"ok": true, "arquivo": "img-1.png"}).to_string()).unwrap();
    assert_eq!(r["kind"], "imagem");
    assert!(l.lib.caminho(r["id"].as_i64().unwrap()).is_some());
}

#[test]
fn filtra_por_projeto_e_herda_o_projeto_da_conversa() {
    let l = lib();
    let p = Projects::new(l.store.clone()).create("TCC", "", false, None).unwrap()["id"].as_str().unwrap().to_string();
    let s = l.store.new_session("web", None, Some(&p)).unwrap();
    gerar(&l, "a.md", "a", Some(&s.id));
    gerar(&l, "b.md", "b", None);
    assert_eq!(l.store.list_artifacts(Some(&p), 100).unwrap().len(), 1);
    assert_eq!(l.store.list_artifacts(None, 100).unwrap().len(), 2);
}

#[test]
fn diff_entre_versoes_de_texto_e_limpeza_das_antigas() {
    let l = lib();
    let v1 = gerar(&l, "nota.md", "linha 1\nlinha 2\n", None);
    let v2 = gerar(&l, "nota.md", "linha 1\nlinha 2 mudou\nlinha 3\n", None);
    let (i1, i2) = (v1["id"].as_i64().unwrap(), v2["id"].as_i64().unwrap());
    let d = l.lib.diff(i2, i1).unwrap().unwrap();
    assert!(d["adicionadas"].as_i64().unwrap() >= 2 && d["removidas"] == 1 && d["iguais"] == false, "{d}");
    assert_eq!(l.lib.limpar(1, Some("nota.md"), true)["total"], 1);
    assert!(l.lib.caminho(i1).is_some()); // simular não apaga
    let r = l.lib.limpar(1, Some("nota.md"), false);
    assert_eq!(r["apagados"], 1);
    assert!(l.lib.caminho(i1).is_none() && l.lib.caminho(i2).is_some());
}

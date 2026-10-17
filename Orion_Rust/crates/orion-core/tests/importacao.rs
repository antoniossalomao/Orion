//! Paridade com `tests/memory/test_importer.py` e `tests/memory/test_migration.py` (Python):
//! importação do export do SurrealDB e a conferência (`verify-export`) antes do corte.
//!
//! Fora daqui: os comandos de linha (`import-surreal`, `verify-export`), que são do
//! `orion-server`, e a prova com a contagem sabotada por `monkeypatch` (sem ponto de injeção).

use chrono::TimeZone;
use orion_core::importer::import_surreal_export;
use orion_core::migration::verify_export;
use orion_store::ops::Operations;
use orion_store::search::Kind;
use orion_store::Store;
use orion_testkit::Relogio;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const SESSAO_A: &str = "11111111-aaaa-bbbb-cccc-000000000001";
const SESSAO_B: &str = "22222222-aaaa-bbbb-cccc-000000000002";

fn local(a: i32, m: u32, d: u32, h: u32, mi: u32) -> f64 {
    chrono::Local.with_ymd_and_hms(a, m, d, h, mi, 0).earliest().unwrap().timestamp() as f64
}

fn agora() -> f64 {
    local(2026, 10, 3, 12, 0)
}

fn escrever(pasta: &Path, tabela: &str, registros: Value) {
    std::fs::write(pasta.join(format!("{tabela}.json")), registros.to_string()).unwrap();
}

fn evento(i: u32, ator: &str, texto: &str, ts: &str, sessao: Option<&str>, extra: Value) -> Value {
    let mut e = json!({"id": format!("evento:⟨e{i}⟩"), "fonte": "chat", "ator": ator, "texto": texto, "timestamp": ts});
    if let Some(s) = sessao {
        e["sessao_id"] = json!(s);
    }
    if let Some(o) = extra.as_object() {
        for (k, v) in o {
            e[k] = v.clone();
        }
    }
    e
}

/// O export falso do SurrealDB (formato do `backup_memoria` do legado).
fn export(d: &Path) -> PathBuf {
    let pasta = d.join("backup");
    std::fs::create_dir_all(&pasta).unwrap();
    escrever(
        &pasta,
        "sessao",
        json!([
            {"id": format!("sessao:`{SESSAO_A}`"), "criada": "2026-06-01T10:00:00", "titulo": "Projeto Orion"},
            {"id": format!("sessao:⟨{SESSAO_B}⟩"), "criada": "2026-06-02T09:00:00", "titulo": null},
            {"titulo": "sem id"},
        ]),
    );
    escrever(
        &pasta,
        "evento",
        json!([
            evento(2, "Orion", "Marília-SP, certo.", "2026-06-01T10:00:05", Some(SESSAO_A), json!({"intencao": "resposta", "fontes_rag": ["p1"]})),
            evento(1, "Antônio", "Em que cidade eu moro?", "2026-06-01T10:00:00", Some(SESSAO_A), json!({"intencao": "objetivo"})),
            evento(3, "Lyra", "resposta de quando eu tinha outro nome", "2026-06-02T09:00:10", Some(SESSAO_B), json!({})),
            evento(4, "Antonio", "mensagem antiga sem sessão", "2026-01-01T08:00:00", None, json!({})),
            evento(5, "sistema", "briefing automático", "2026-06-02T09:00:00", Some(SESSAO_B), json!({})),
            evento(6, "Antônio", "   ", "2026-06-02T09:00:20", Some(SESSAO_B), json!({})), // vazio
            {"fonte": "chat", "ator": "Antônio", "texto": "sem id", "timestamp": "2026-06-02T09:00:30"}, // sem id
            evento(7, "Antônio", "sessão que não existe no export", "2026-06-03T09:00:00", Some("99999999-aaaa"), json!({})),
        ]),
    );
    escrever(
        &pasta,
        "lembrete",
        json!([
            {"id": "lembrete:a", "titulo": "Pagar boleto", "quando": "2026-06-05T09:00:00", "criado": "2026-06-01T08:00:00", "concluido": false},
            {"id": "lembrete:b", "titulo": "Entregar trabalho", "quando": "2099-01-01T09:00:00", "nota": "UML", "concluido": false},
        ]),
    );
    escrever(&pasta, "numero", json!([]));
    pasta
}

/// O export com as demais tabelas operacionais e as relações do grafo.
fn export_completo(d: &Path) -> PathBuf {
    let pasta = export(d);
    escrever(
        &pasta,
        "agendamento",
        json!([
            // diário atrasado: o próximo horário é recalculado a partir de agora
            {"id": "agendamento:d", "titulo": "Checar saúde", "tipo": "diario", "horario": "08:00",
             "proxima_execucao": "2026-06-01T08:00:00", "ferramenta": "checar_saude_sistema",
             "parametros": "{\"verbose\": true}", "ativo": true},
            // único que nunca disparou e já passou: entra desativado
            {"id": "agendamento:u", "titulo": "Aviso velho", "tipo": "unico", "proxima_execucao": "2026-06-01T08:00:00", "ativo": true},
            // pausado pelo usuário continua pausado
            {"id": "agendamento:p", "titulo": "Pausado", "tipo": "intervalo_min", "intervalo_min": 30,
             "proxima_execucao": "2026-10-03T12:30:00", "ativo": false, "parametros": "{quebrado"},
            {"id": "agendamento:x", "titulo": "Tipo desconhecido", "tipo": "cron"},
        ]),
    );
    escrever(
        &pasta,
        "tarefa",
        json!([
            {"id": "tarefa:a", "titulo": "Estudar UML", "status": "em_andamento"},
            {"id": "tarefa:b", "titulo": "Status esquisito", "status": "talvez"},
            {"id": "tarefa:c", "titulo": ""},
        ]),
    );
    escrever(
        &pasta,
        "numero",
        json!([
            {"id": "numero:a", "alvo": "fatura", "score": 7, "motivo": "vence amanhã", "notificado": true},
            {"id": "numero:b", "alvo": "log", "score": "alto"},
        ]),
    );
    escrever(&pasta, "prompt", json!([{"id": "prompt:a", "titulo": "Resumo", "comando": "resumo", "conteudo": "Resuma em 5 linhas"}]));
    escrever(
        &pasta,
        "precedeu",
        json!([
            {"id": "precedeu:1", "in": "evento:⟨e1⟩", "out": "evento:⟨e2⟩"},
            {"id": "precedeu:2", "in": "evento:⟨e6⟩", "out": "evento:⟨e1⟩"}, // e6 é vazio: não importado
        ]),
    );
    escrever(
        &pasta,
        "sobre",
        json!([
            {"id": "sobre:1", "in": "evento:⟨e1⟩", "out": "topico:⟨marília⟩"},
            {"id": "sobre:2", "in": "evento:⟨e2⟩", "out": "topico:⟨marília⟩"},
            {"id": "sobre:3", "in": "pessoa:⟨x⟩", "out": "topico:⟨marília⟩"},
        ]),
    );
    escrever(
        &pasta,
        "conecta",
        json!([{"id": "conecta:1", "in": "topico:⟨marília⟩", "out": "topico:⟨unimar⟩", "tipo": "rem", "peso": 2}]),
    );
    pasta
}

struct Amb {
    d: tempfile::TempDir,
    store: Arc<Store>,
}

fn amb() -> Amb {
    let d = tempfile::tempdir().unwrap();
    let rel = Relogio::em(agora());
    let store = Arc::new(Store::open_with(d.path().join("o.db"), None, rel.clock()).unwrap());
    Amb { d, store }
}

fn a(x: &[&str]) -> Vec<String> {
    x.iter().map(|s| s.to_string()).collect()
}

fn rows(s: &Store, sql: &str) -> Vec<orion_store::ops::Row> {
    s.query(sql, &[]).unwrap()
}

// ── test_importer.py ────────────────────────────────────────────────────────
#[test]
fn importa_conversas_com_datas_papeis_e_sessoes() {
    let t = amb();
    let exp = export(t.d.path());
    let rel = import_surreal_export(&t.store, &exp, &a(&["Orion", "Lyra"])).unwrap();
    assert_eq!((rel.sessoes, rel.mensagens, rel.ja_importadas, rel.invalidas), (4, 6, 0, 3));
    assert_eq!(rel.operacionais.get("lembrete"), Some(&2));
    assert_eq!(rel.operacionais.len(), 1);

    let legado: std::collections::HashMap<_, _> =
        t.store.list_sessions(Some("legado"), 100, None, None).unwrap().into_iter().map(|s| (s.title.clone().unwrap(), s)).collect();
    let titulos: std::collections::HashSet<_> = legado.keys().cloned().collect();
    assert_eq!(
        titulos,
        ["Projeto Orion", "conversa sem título", "conversas antigas (pré-sessões)", "sessão 99999999"].map(String::from).into()
    );
    let msgs = t.store.history(&legado["Projeto Orion"].id, 100, 0).unwrap();
    assert_eq!(msgs.iter().map(|m| (m.role.as_str(), m.text.as_str())).collect::<Vec<_>>(), [("user", "Em que cidade eu moro?"), ("assistant", "Marília-SP, certo.")]);
    assert!(msgs[0].created_at < msgs[1].created_at); // ordem cronológica pela data original
    assert_eq!(
        msgs[1].provenance,
        Some(json!({"importado": "surrealdb", "fonte": "chat", "intencao": "resposta", "fontes_rag": ["p1"]}))
    );
    let b = t.store.history(&legado["conversa sem título"].id, 100, 0).unwrap();
    assert_eq!(b.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(), ["system", "assistant"]); // "sistema" não vira fala de ninguém; Lyra = assistente
    let antigas = t.store.history(&legado["conversas antigas (pré-sessões)"].id, 100, 0).unwrap();
    assert_eq!(antigas.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(), ["user"]);
}

#[test]
fn importado_e_pesquisavel_e_nao_vira_sessao_ativa() {
    let t = amb();
    import_surreal_export(&t.store, &export(t.d.path()), &a(&["Orion"])).unwrap();
    let hit = &t.store.search("em que cidade eu moro", 8, &[Kind::Message]).unwrap()[0];
    assert!(hit.source == "sessão legado" && hit.text.contains("cidade"));
    assert!(t.store.list_sessions(Some("web"), 100, None, None).unwrap().is_empty());
    let importadas: std::collections::HashSet<_> = t.store.list_sessions(Some("legado"), 100, None, None).unwrap().into_iter().map(|s| s.id).collect();
    assert!(!importadas.contains(&t.store.active_session("legado").unwrap().id)); // arquivadas: nenhuma é reaberta
}

#[test]
fn nome_antigo_do_assistente_sem_configurar_vira_system() {
    let t = amb();
    import_surreal_export(&t.store, &export(t.d.path()), &a(&["Orion"])).unwrap();
    let mut papeis = std::collections::HashMap::new();
    for s in t.store.list_sessions(Some("legado"), 100, None, None).unwrap() {
        for m in t.store.history(&s.id, 100, 0).unwrap() {
            papeis.insert(m.text, m.role);
        }
    }
    assert_eq!(papeis["resposta de quando eu tinha outro nome"], "system");
}

#[test]
fn reimportar_nao_duplica() {
    let t = amb();
    let exp = export(t.d.path());
    import_surreal_export(&t.store, &exp, &a(&["Orion"])).unwrap();
    let de_novo = import_surreal_export(&t.store, &exp, &a(&["Orion"])).unwrap();
    assert_eq!((de_novo.sessoes, de_novo.mensagens, de_novo.ja_importadas), (0, 0, 6));
    let total: i64 = t.store.conn().query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0)).unwrap();
    assert!(total == 6 && t.store.list_sessions(Some("legado"), 100, None, None).unwrap().len() == 4);
}

#[test]
fn export_incompleto_ou_corrompido() {
    let t = amb();
    let vazia = t.d.path().join("vazia");
    std::fs::create_dir_all(&vazia).unwrap();
    assert!(import_surreal_export(&t.store, &vazia, &[]).unwrap_err().contains("backup_memoria"));
    std::fs::write(vazia.join("evento.json"), "{\"nao\": \"lista\"}").unwrap();
    assert!(import_surreal_export(&t.store, &vazia, &[]).unwrap_err().contains("lista"));
    std::fs::write(vazia.join("evento.json"), "{quebrado").unwrap();
    assert!(import_surreal_export(&t.store, &vazia, &[]).is_err());
}

#[test]
fn importa_operacao_com_regras_de_seguranca_dos_avisos() {
    let t = amb();
    let exp = export_completo(t.d.path());
    let rel = import_surreal_export(&t.store, &exp, &a(&["Orion"])).unwrap();
    let esperado: std::collections::BTreeMap<String, usize> =
        [("lembrete", 2), ("agendamento", 3), ("tarefa", 2), ("numero", 1), ("prompt", 1)].map(|(k, v)| (k.to_string(), v)).into();
    assert_eq!(rel.operacionais, esperado);
    // base + agendamento de tipo estranho + tarefa sem título + número com score "alto"
    assert_eq!(rel.invalidas, 3 + 1 + 1 + 1);
    let ops = Operations::new(t.store.clone());

    let por_titulo: std::collections::HashMap<_, _> = ops.list_reminders(false).unwrap().into_iter().map(|r| (r["title"].as_str().unwrap().to_string(), r)).collect();
    let (antigo, futuro) = (&por_titulo["Pagar boleto"], &por_titulo["Entregar trabalho"]);
    assert!(antigo["notified"] == 1 && futuro["notified"] == 0); // sem rajada de aviso velho
    assert!(futuro["note"] == "UML" && antigo["due_at"].as_f64().unwrap() == local(2026, 6, 5, 9, 0));
    assert!(ops.due_reminders(Some(agora())).unwrap().is_empty());

    let ag: std::collections::HashMap<_, _> = ops.list_schedules().unwrap().into_iter().map(|s| (s["title"].as_str().unwrap().to_string(), s)).collect();
    assert_eq!(ag["Checar saúde"]["next_run"].as_f64().unwrap(), local(2026, 10, 4, 8, 0)); // recalculado
    assert_eq!(ag["Checar saúde"]["tool"], "checar_saude_sistema");
    let params: Value = serde_json::from_str(ag["Checar saúde"]["params"].as_str().unwrap()).unwrap();
    assert_eq!(params, json!({"verbose": true}));
    assert!(ag["Aviso velho"]["active"] == 0 && ag["Aviso velho"]["next_run"].is_null());
    assert!(ag["Pausado"]["active"] == 0 && ag["Pausado"]["params"] == "{}"); // JSON quebrado não entra
    assert!(ops.due_schedules(Some(agora())).unwrap().is_empty());

    let tarefas: std::collections::HashMap<_, _> = ops.list_tasks(None).unwrap().into_iter().map(|t| (t["title"].as_str().unwrap().to_string(), t["status"].as_str().unwrap().to_string())).collect();
    assert_eq!(tarefas, [("Estudar UML", "em_andamento"), ("Status esquisito", "pendente")].map(|(a, b)| (a.to_string(), b.to_string())).into());
    let numeros = ops.list_numbers(false, 0.0).unwrap();
    assert_eq!(numeros.iter().map(|n| n["target"].as_str().unwrap()).collect::<Vec<_>>(), ["fatura"]);
    assert!(numeros[0]["score"].as_f64().unwrap() == 1.0 && numeros[0]["notified"] == 1); // 7 → limitado a 1
    assert_eq!(ops.list_prompts().unwrap()[0]["content"], "Resuma em 5 linhas");
}

#[test]
fn importa_grafo_resolvendo_eventos_para_mensagens() {
    let t = amb();
    let rel = import_surreal_export(&t.store, &export_completo(t.d.path()), &a(&["Orion"])).unwrap();
    assert_eq!((rel.arestas, rel.arestas_repetidas, rel.arestas_sem_no), (4, 0, 2));
    let ops = Operations::new(t.store.clone());
    let msg: std::collections::HashMap<String, String> = rows(&t.store, "SELECT external_id, ref FROM imported WHERE kind='mensagem'")
        .into_iter()
        .map(|r| (r["external_id"].as_str().unwrap().to_string(), r["ref"].as_str().unwrap().to_string()))
        .collect();
    let (m1, m2) = (format!("message:{}", msg["e1"]), format!("message:{}", msg["e2"]));
    let tr = |v: Vec<orion_store::ops::Row>| -> std::collections::HashSet<(String, String, String)> {
        v.iter().map(|n| (n["rel"].as_str().unwrap().into(), n["src"].as_str().unwrap().into(), n["dst"].as_str().unwrap().into())).collect()
    };
    assert_eq!(
        tr(ops.neighbors(&m1, None, 50).unwrap()),
        [("precedeu".to_string(), m1.clone(), m2.clone()), ("sobre".to_string(), m1.clone(), "topic:marília".to_string())].into()
    );
    let vizinhos = ops.neighbors("topic:marília", None, 50).unwrap();
    let dirs: std::collections::HashSet<_> = vizinhos.iter().map(|n| (n["direction"].as_str().unwrap().to_string(), n["rel"].as_str().unwrap().to_string())).collect();
    assert!(dirs.contains(&("in".into(), "sobre".into())) && dirs.contains(&("out".into(), "conecta".into())));
    let conecta = vizinhos.iter().find(|n| n["rel"] == "conecta").unwrap();
    assert_eq!((conecta["dst"].as_str(), conecta["kind"].as_str(), conecta["weight"].as_f64()), (Some("topic:unimar"), Some("rem"), Some(2.0)));
    let sobre: Vec<_> = ops.neighbors("topic:marília", Some("sobre"), 50).unwrap().iter().map(|n| n["src"].as_str().unwrap().to_string()).collect();
    assert_eq!(sobre, [m1, m2]);
}

#[test]
fn reimportar_operacao_e_grafo_nao_duplica() {
    let t = amb();
    let exp = export_completo(t.d.path());
    import_surreal_export(&t.store, &exp, &a(&["Orion"])).unwrap();
    let ops = Operations::new(t.store.clone());
    let conta = |ops: &Operations| (ops.list_reminders(false).unwrap().len(), ops.list_schedules().unwrap().len(), ops.edge_count());
    let antes = conta(&ops);
    let de_novo = import_surreal_export(&t.store, &exp, &a(&["Orion"])).unwrap();
    assert!(de_novo.operacionais.is_empty() && de_novo.arestas == 0);
    let rep: std::collections::BTreeMap<String, usize> = [("lembrete", 2), ("agendamento", 3), ("tarefa", 2), ("numero", 1), ("prompt", 1)].map(|(k, v)| (k.to_string(), v)).into();
    assert_eq!(de_novo.operacionais_repetidos, rep);
    assert_eq!(de_novo.arestas_repetidas, 4);
    assert_eq!(conta(&ops), antes);
}

#[test]
fn importar_so_operacao_sem_grafo_funciona() {
    let t = amb();
    let rel = import_surreal_export(&t.store, &export(t.d.path()), &a(&["Orion"])).unwrap();
    assert_eq!((rel.arestas, rel.arestas_repetidas, rel.arestas_sem_no), (0, 0, 0));
}

#[test]
fn resumo_do_import_avisa_ator_desconhecido() {
    for n in [0usize, 1] {
        let t = amb();
        let rel = import_surreal_export(&t.store, &export(t.d.path()), &a(&["Orion", "Lyra"][..n + 1])).unwrap();
        assert_eq!(rel.resumo().contains("ATENÇÃO"), n == 0);
        serde_json::to_string(&rel.atores_como_system).unwrap(); // serializável (vira JSON em relatórios)
    }
}

// ── test_migration.py ───────────────────────────────────────────────────────
fn ensaio_de(rel: &orion_core::migration::MigrationReport, t: &str) -> i64 {
    rel.ensaio.iter().find(|(k, _)| k == t).map(|(_, n)| *n).unwrap_or(-1)
}

#[test]
fn export_completo_passa_no_ensaio_de_importacao_backup_e_restauracao() {
    let t = amb();
    let exp = export_completo(t.d.path());
    let rel = verify_export(&exp, &a(&["Orion", "Lyra"]), None, None, None, true);
    assert!(rel.ok(), "{:?}", rel.erros);
    assert!(rel.tabelas["evento"] == 8 && rel.tabelas["agendamento"] == 4);
    assert!(rel.ausentes.is_empty()); // todas as tabelas esperadas existem neste export
    assert_eq!(rel.importacao.as_ref().unwrap().mensagens, 6);
    assert!(ensaio_de(&rel, "messages") == 6 && ensaio_de(&rel, "edges") == 4 && ensaio_de(&rel, "tasks") == 2);
    let texto = rel.linhas().join("\n");
    assert!(texto.contains("RESULTADO: PRONTO") && texto.contains("BACKUP E RESTAURAÇÃO"));
    // registros inválidos do export aparecem como aviso (não derrubam, mas não passam em silêncio)
    assert!(rel.avisos.iter().any(|x| x.contains("inválido(s) em agendamento.json")), "{:?}", rel.avisos);
}

#[test]
fn tabela_que_o_legado_nunca_usou_e_so_aviso() {
    let t = amb();
    let rel = verify_export(&export(t.d.path()), &a(&["Orion", "Lyra"]), None, None, None, true);
    assert!(rel.ok(), "{:?}", rel.erros);
    for tabela in ["agendamento", "tarefa", "prompt", "precedeu", "sobre", "conecta"] {
        assert!(rel.ausentes.iter().any(|x| x == tabela), "{tabela} não consta em {:?}", rel.ausentes);
    }
}

#[test]
fn ator_desconhecido_e_avisado_para_nao_perder_as_respostas_do_nome_antigo() {
    let t = amb();
    let rel = verify_export(&export(t.d.path()), &a(&["Orion"]), None, None, None, true); // "Lyra" não informado
    assert!(rel.ok());
    assert!(rel.avisos.iter().any(|x| x.contains("lyra") && x.contains("--assistente")), "{:?}", rel.avisos);
    assert_eq!(rel.importacao.unwrap().atores_como_system.get("lyra"), Some(&1));
}

#[test]
fn export_sem_os_arquivos_obrigatorios_ou_vazio_ou_corrompido_nao_esta_pronto() {
    let t = amb();
    assert!(!verify_export(&t.d.path().join("nao-existe"), &[], None, None, None, true).ok());
    let pasta = t.d.path().join("bk");
    std::fs::create_dir_all(&pasta).unwrap();
    let rel = verify_export(&pasta, &[], None, None, None, true);
    assert!(!rel.ok() && rel.erros.iter().any(|e| e.contains("evento.json não existe")));
    assert!(rel.erros.iter().any(|e| e.contains("sessao.json não existe")));
    escrever(&pasta, "evento", json!([]));
    escrever(&pasta, "sessao", json!([]));
    assert!(verify_export(&pasta, &[], None, None, None, true).erros.iter().any(|e| e.contains("vazio")));
    std::fs::write(pasta.join("evento.json"), "{quebrado").unwrap();
    let rel = verify_export(&pasta, &[], None, None, None, true);
    assert!(rel.erros.iter().any(|e| e.contains("evento.json ilegível")), "{:?}", rel.erros);
    assert!(!rel.erros.iter().any(|e| e.contains("evento.json não existe"))); // um erro só, não dois
    std::fs::write(pasta.join("evento.json"), "{\"nao\": \"lista\"}").unwrap();
    let rel = verify_export(&pasta, &[], None, None, None, true);
    assert!(rel.erros.iter().any(|e| e.contains("lista de registros")), "{:?}", rel.erros);
    assert!(rel.importacao.is_none()); // com erro no export, nem tenta o ensaio
}

#[test]
fn conferencia_dos_arquivos_a_copiar_do_pc_sem_vazar_valor_de_segredo() {
    let t = amb();
    let d = t.d.path();
    let env = d.join(".env");
    std::fs::write(&env, format!("# comentário\nGROQ_API_KEY=gsk_{}\nTELEGRAM_BOT_TOKEN=123:abc\nVAZIA=\n", "s".repeat(30))).unwrap();
    let gauth = d.join("google_auth");
    std::fs::create_dir_all(&gauth).unwrap();
    std::fs::write(gauth.join("token.json"), "{}").unwrap();
    std::fs::write(gauth.join("vazio.json"), "").unwrap();
    let vault = d.join("vault");
    std::fs::create_dir_all(vault.join(".obsidian")).unwrap();
    std::fs::write(vault.join(".obsidian/x.md"), "config").unwrap();
    std::fs::write(vault.join("Nota.md"), "# nota").unwrap();
    let rel = verify_export(&export(d), &a(&["Orion", "Lyra"]), Some(&env), Some(&gauth), Some(&vault), true);
    assert!(rel.ok(), "{:?}", rel.erros);
    let texto = rel.linhas().join("\n");
    assert!(texto.contains("GROQ_API_KEY") && texto.contains("TELEGRAM_BOT_TOKEN") && !texto.contains("VAZIA"));
    assert!(!texto.contains("gsk_") && !texto.contains("123:abc")); // só os nomes, nunca os valores
    let arq: std::collections::HashMap<_, _> = rel.arquivos.iter().cloned().collect();
    assert!(arq["google_auth/"].starts_with("1 arquivo") && arq["vault"] == "1 notas .md", "{arq:?}");
}

#[test]
fn arquivos_ausentes_ou_vazios_reprovam() {
    let t = amb();
    let vazio = t.d.path().join("vazio");
    std::fs::create_dir_all(&vazio).unwrap();
    let rel = verify_export(&export(t.d.path()), &a(&["Orion", "Lyra"]), Some(&t.d.path().join("nao-tem.env")), Some(&vazio), Some(&vazio), true);
    assert!(!rel.ok() && rel.erros.len() == 3, "{:?}", rel.erros);
    assert!(rel.erros.iter().any(|e| e.contains(".env")) && rel.erros.iter().any(|e| e.contains("google_auth")) && rel.erros.iter().any(|e| e.contains("vault")));
}

#[test]
fn ensaio_nao_toca_no_banco_real() {
    // o ensaio usa um banco temporário: nenhum `orion.db` aparece ao lado do export
    let t = amb();
    let exp = export(t.d.path());
    let antes: Vec<_> = std::fs::read_dir(t.d.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    verify_export(&exp, &a(&["Orion", "Lyra"]), None, None, None, true);
    let depois: Vec<_> = std::fs::read_dir(t.d.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(antes.len(), depois.len());
    assert!(!t.d.path().join("backup").join("orion.db").exists());
}

#[test]
fn sem_ensaio_nao_importa_nada() {
    let t = amb();
    let rel = verify_export(&export_completo(t.d.path()), &a(&["Lyra"]), None, None, None, false);
    assert!(rel.ok() && rel.importacao.is_none());
    assert!(!rel.linhas().join("\n").contains("IMPORTAÇÃO DE ENSAIO"));
    let rel = verify_export(&t.d.path().join("nada"), &[], None, None, None, true);
    assert!(rel.linhas().join("\n").contains("NÃO está pronto"));
}

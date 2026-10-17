//! Paridade com `tests/memory/` (Python): os testes Python são a especificação do `orion-store`
//! (ORION.md: "paridade antes de melhoria"). Cada teste cita o teste de origem.
//!
//! Não portados por dependerem de internals do Python: troca do embedder em runtime
//! (`store._embedder = Curto`) e `main(["restore", ...])` (CLI, fica no `orion-server`).

use orion_store::documents::IndexResult;
use orion_store::ops::{next_run, parse_when, When};
use orion_store::search::Kind;
use orion_store::text::{chunk_text, fts_query, strip_frontmatter};
use orion_store::vectors::{normalizar, para_blob, VectorIndex};
use orion_store::{Embedder, Store, StoreError};
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;
use orion_testkit::Relogio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// ── apoio ───────────────────────────────────────────────────────────────────
const STOPWORDS: &str = "a o as os um uma uns umas de do da dos das em no na nos nas por para com \
sem sobre entre e ou mas que se como qual quais quem onde quando é ser foi são era eu me meu minha \
meus minhas você seu sua tu te ele ela isso isto esse essa este esta já não sim mais muito tem \
tinha ter há foi vai vou sei fala falou disse";

fn plano(t: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    t.to_lowercase().nfkd().filter(|c| c.is_ascii()).collect()
}

fn conceito(tok: &str) -> &str {
    match tok {
        "estudo" | "estuda" | "estudar" | "faculdade" | "universidade" | "unimar" | "curso" => "ESTUDAR",
        "moro" | "mora" | "morar" | "cidade" | "residencia" => "MORAR",
        "carro" | "veiculo" | "automovel" => "VEICULO",
        t => t,
    }
}

/// Embedder falso e determinístico: sinônimos viram o mesmo "conceito".
struct Fake {
    quebrado: AtomicBool,
    dim: usize,
}

impl Fake {
    fn novo() -> Arc<Self> {
        Arc::new(Fake { quebrado: AtomicBool::new(false), dim: 128 })
    }
}

impl Embedder for Fake {
    fn dim(&self) -> usize {
        self.dim
    }
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        if self.quebrado.load(Ordering::SeqCst) {
            anyhow::bail!("API de embedding fora do ar");
        }
        let paradas: HashSet<String> = STOPWORDS.split_whitespace().map(plano).collect();
        let re = regex::Regex::new(r"\w+").unwrap();
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0.0f32; self.dim];
                for m in re.find_iter(&plano(t)) {
                    if paradas.contains(m.as_str()) {
                        continue;
                    }
                    let chave = conceito(m.as_str());
                    v[crc32(chave.as_bytes()) as usize % self.dim] += 1.0;
                }
                v
            })
            .collect())
    }
}

fn crc32(b: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &x in b {
        c ^= x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

fn loja(dir: &Path) -> Store {
    Store::open(dir.join("orion.db")).unwrap()
}

fn loja_vec(dir: &Path, emb: &Arc<Fake>) -> Store {
    Store::open_with(dir.join("orion_vec.db"), Some(emb.clone()), orion_policy::wall_clock()).unwrap()
}

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn em(a: i32, m: u32, d: u32, h: u32, mi: u32) -> f64 {
    use chrono::TimeZone;
    chrono::Local.with_ymd_and_hms(a, m, d, h, mi, 0).earliest().unwrap().timestamp() as f64
}

fn versao(s: &Store) -> String {
    s.query("SELECT value FROM meta WHERE key='schema_version'", &[]).unwrap()[0]["value"]
        .as_str()
        .unwrap()
        .to_string()
}

fn vetores(s: &Store) -> i64 {
    s.conn().query_row("SELECT COUNT(*) FROM vectors", [], |r| r.get(0)).unwrap()
}

fn fts_f() -> [Kind; 1] {
    [Kind::Fact]
}

// ── test_text.py ────────────────────────────────────────────────────────────
#[test]
fn fts_query_remove_stopwords_e_usa_prefixo_em_termos_longos() {
    let q = fts_query("onde eu trabalho hoje?").unwrap();
    assert!(q.contains("\"traba\"*") && !q.contains("\"onde\"") && !q.contains("\"eu\""), "{q}");
}

#[test]
fn fts_query_neutraliza_sintaxe_fts() {
    for sujo in ["\"", "a OR", "NEAR(a b)", "col:valor", "*", "a*b", "()", "--", "'; DROP TABLE facts;--"] {
        if let Some(q) = fts_query(sujo) {
            let limpo = q.replace('"', "").replace('*', "").replace(" OR ", " ");
            assert!(!limpo.contains(['(', ')', ':']), "{sujo} → {q}");
        }
    }
}

#[test]
fn fts_query_vazia() {
    assert!(fts_query("").is_none() && fts_query("  ?! ").is_none());
}

#[test]
fn chunk_respeita_tamanho_e_sobreposicao() {
    let texto: Vec<String> = (0..10).map(|i| format!("Parágrafo {i} {}", "palavra ".repeat(40))).collect();
    let pedacos = chunk_text(&texto.join("\n\n"), 500, 60);
    assert!(pedacos.len() > 3);
    assert!(pedacos.iter().all(|p| p.chars().count() <= 500 + 80));
    let primeira = pedacos[1].split_whitespace().next().unwrap();
    assert!(pedacos[0].contains(primeira)); // a cauda do anterior abre o seguinte
}

#[test]
fn chunk_paragrafo_gigante_e_vazio() {
    assert!(chunk_text("", 800, 100).is_empty());
    assert!(chunk_text(&"x ".repeat(2000), 300, 0).len() > 5);
}

#[test]
fn strip_frontmatter_remove_so_o_cabecalho() {
    assert_eq!(strip_frontmatter("---\ntype: nota\ntags: [a]\n---\n# Título\ncorpo"), "# Título\ncorpo");
    assert_eq!(strip_frontmatter("sem frontmatter"), "sem frontmatter");
}

// ── test_vectors.py ─────────────────────────────────────────────────────────
fn linhas(vs: Vec<Vec<f32>>) -> impl Fn() -> Vec<(i64, Vec<u8>)> {
    move || vs.iter().enumerate().map(|(i, v)| (i as i64 + 1, para_blob(v))).collect()
}

#[test]
fn ordena_por_cosseno_e_ignora_o_tamanho_do_vetor() {
    let mut idx = VectorIndex::default();
    let c = linhas(vec![vec![10., 0., 0.], vec![0.9, 0.1, 0.], vec![0., 1., 0.], vec![0., 0., 5.]]);
    let ids = idx.topk("fact", &[1., 0.05, 0.], 4, c);
    assert_eq!(&ids[..2], &[1, 2]);
    assert_eq!(ids.iter().copied().collect::<HashSet<_>>(), HashSet::from([1, 2, 3, 4]));
}

#[test]
fn k_menor_que_o_total_devolve_so_os_melhores() {
    let mut idx = VectorIndex::default();
    let c = linhas((0..10).map(|i| vec![1., i as f32 / 10., 0.]).collect());
    assert_eq!(idx.topk("chunk", &[1., 0., 0.], 3, c), vec![1, 2, 3]);
}

#[test]
fn indice_vazio_e_dimensao_diferente() {
    let mut idx = VectorIndex::default();
    assert!(idx.topk("fact", &[1., 0.], 5, Vec::new).is_empty());
    assert!(idx.topk("chunk", &[1., 0.], 5, linhas(vec![vec![1., 0., 0.]])).is_empty());
}

#[test]
fn cache_so_recarrega_quando_invalidado() {
    let mut idx = VectorIndex::default();
    let chamadas = std::cell::Cell::new(0);
    let c = || {
        chamadas.set(chamadas.get() + 1);
        linhas(vec![vec![1., 0.]])()
    };
    idx.topk("fact", &[1., 0.], 1, c);
    idx.topk("fact", &[1., 0.], 1, c);
    assert_eq!(chamadas.get(), 1);
    idx.invalidar(Some("fact"));
    idx.topk("fact", &[1., 0.], 1, c);
    assert_eq!(chamadas.get(), 2);
    idx.invalidar(None);
    idx.topk("fact", &[1., 0.], 1, c);
    assert_eq!(chamadas.get(), 3);
}

#[test]
fn normalizacao() {
    let n = normalizar(&[3., 4.]);
    assert!(((n[0] * n[0] + n[1] * n[1]).sqrt() - 1.0).abs() < 1e-6);
    assert_eq!(normalizar(&[0., 0.]), vec![0., 0.]);
    assert_eq!(para_blob(&[1., 2., 3.]).len(), 12); // 3 × float32
}

#[test]
fn desempenho_com_dezenas_de_milhares_de_trechos() {
    // 30 mil trechos de 384 dim respondem bem abaixo de 1 s (espelha o teste Python)
    let mut estado = 0x9E37_79B9_7F4A_7C15u64;
    let mut rnd = move || {
        estado ^= estado << 13;
        estado ^= estado >> 7;
        estado ^= estado << 17;
        (estado >> 11) as f32 / (1u64 << 53) as f32 - 0.5
    };
    let base: Vec<Vec<f32>> = (0..30_000).map(|_| (0..384).map(|_| rnd()).collect()).collect();
    let carregar = || base.iter().enumerate().map(|(i, v)| (i as i64, para_blob(v))).collect::<Vec<_>>();
    let mut idx = VectorIndex::default();
    idx.topk("chunk", &base[7], 5, carregar); // carga fria
    let t = std::time::Instant::now();
    let ids = idx.topk("chunk", &base[7], 5, carregar);
    assert_eq!(ids[0], 7);
    assert!(t.elapsed().as_secs_f64() < 1.0, "{:?}", t.elapsed());
}

#[test]
fn k_extremos() {
    for k in [1usize, 2, 50] {
        let mut idx = VectorIndex::default();
        let c = linhas(vec![vec![1., 0.], vec![0., 1.], vec![1., 1.]]);
        assert_eq!(idx.topk("fact", &[1., 0.], k, c).len(), k.min(3));
    }
}

// ── test_store.py: sessões ──────────────────────────────────────────────────
#[test]
fn sessao_ativa_por_canal_nao_mistura_historicos() {
    let d = tmp();
    let s = loja(d.path());
    let (tg, web) = (s.active_session("telegram").unwrap(), s.active_session("web").unwrap());
    assert!(tg.id != web.id && s.active_session("telegram").unwrap().id == tg.id);
    s.add_message(&tg.id, "user", "oi pelo telegram", None, None).unwrap();
    s.add_message(&web.id, "user", "oi pela web", None, None).unwrap();
    let textos = |id: &str| s.history(id, 50, 0).unwrap().into_iter().map(|m| m.text).collect::<Vec<_>>();
    assert_eq!(textos(&tg.id), ["oi pelo telegram"]);
    assert_eq!(textos(&web.id), ["oi pela web"]);
}

#[test]
fn historico_ordem_limite_e_proveniencia() {
    let d = tmp();
    let s = loja(d.path());
    let sess = s.new_session("web", None, None).unwrap();
    for i in 0..5 {
        let prov = (i == 4).then(|| json!({"fontes": [4]}));
        s.add_message(&sess.id, if i % 2 == 0 { "user" } else { "assistant" }, &format!("m{i}"), prov.as_ref(), None)
            .unwrap();
    }
    let h = s.history(&sess.id, 3, 0).unwrap();
    assert_eq!(h.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), ["m2", "m3", "m4"]);
    assert_eq!(h[2].provenance, Some(json!({"fontes": [4]})));
    assert!(h[0].provenance.is_none());
}

#[test]
fn arquivar_sessao_cria_nova_na_proxima() {
    let d = tmp();
    let s = loja(d.path());
    let a = s.active_session("web").unwrap();
    s.archive_session(&a.id).unwrap();
    assert_ne!(s.active_session("web").unwrap().id, a.id);
}

#[test]
fn papel_invalido_e_sessao_inexistente_sao_recusados() {
    let d = tmp();
    let s = loja(d.path());
    let sess = s.new_session("web", None, None).unwrap();
    assert!(matches!(s.add_message(&sess.id, "root", "x", None, None), Err(StoreError::Sql(_))));
    assert!(matches!(s.add_message("nao-existe", "user", "x", None, None), Err(StoreError::Sql(_))));
}

#[test]
fn proveniencia_e_json_valido() {
    let d = tmp();
    let s = loja(d.path());
    let sess = s.new_session("web", None, None).unwrap();
    let m = s
        .add_message(&sess.id, "assistant", "x", Some(&json!({"modelo": "m", "fontes": ["fato:1"]})), None)
        .unwrap();
    let bruto: String =
        s.conn().query_row("SELECT provenance FROM messages WHERE id=?", [m.id], |r| r.get(0)).unwrap();
    let v: serde_json::Value = serde_json::from_str(&bruto).unwrap();
    assert_eq!(v["fontes"][0], "fato:1");
}

// ── test_store.py: fatos ────────────────────────────────────────────────────
#[test]
fn fato_idempotente_edita_e_esquece_de_verdade() {
    let d = tmp();
    let s = loja(d.path());
    let a = s.add_fact("Antônio estuda ADS na UNIMAR", "conversa", None).unwrap();
    let b = s.add_fact("  antônio   estuda ADS na unimar ", "outra fonte", None).unwrap();
    assert!(a.id == b.id && s.facts().unwrap().len() == 1);

    s.update_fact(a.id, "Antônio cursa ADS na UNIMAR", Some("manual")).unwrap();
    assert_eq!(s.facts().unwrap()[0].source, "manual");
    assert_eq!(s.search("cursa", 8, &fts_f()).unwrap()[0].id, a.id);
    assert!(s.search("estuda", 8, &fts_f()).unwrap().is_empty()); // índice acompanhou a edição

    assert!(s.forget_fact(a.id).unwrap() && !s.forget_fact(a.id).unwrap());
    assert!(s.search("UNIMAR", 8, &Kind::ALL).unwrap().is_empty());
    let n: i64 = s
        .conn()
        .query_row("SELECT COUNT(*) FROM facts_fts WHERE facts_fts MATCH 'unimar'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn fato_vazio_e_edicao_de_inexistente() {
    let d = tmp();
    let s = loja(d.path());
    assert!(matches!(s.add_fact("   ", "x", None), Err(StoreError::Invalid(_))));
    assert!(matches!(s.update_fact(999, "x", None), Err(StoreError::NotFound(_))));
}

#[test]
fn exporta_fatos_para_o_vault() {
    let d = tmp();
    let s = loja(d.path());
    s.add_fact("Antônio mora em Marília-SP", "conversa", None).unwrap();
    let arq = s.export_facts(&d.path().join("vault").join("Orion"), "Fatos").unwrap();
    let texto = std::fs::read_to_string(arq).unwrap();
    assert!(texto.starts_with("---\ntype: fatos-orion") && texto.contains("fonte: conversa"), "{texto}");
    assert!(texto.contains("Marília-SP") && texto.contains("id 1"), "{texto}");
}

// ── test_store.py: busca por palavra-chave ──────────────────────────────────
#[test]
fn busca_ignora_acento_e_casa_flexoes() {
    let d = tmp();
    let s = loja(d.path());
    s.add_fact("Antônio trabalha com açúcar e café", "manual", None).unwrap();
    for q in ["acucar", "ACÚCAR", "cafe", "onde eu trabalho?", "trabalhei"] {
        assert!(!s.search(q, 8, &Kind::ALL).unwrap().is_empty(), "{q}");
    }
}

#[test]
fn busca_com_sintaxe_estranha_nao_quebra() {
    let d = tmp();
    let s = loja(d.path());
    s.add_fact("fato qualquer", "manual", None).unwrap();
    for sujo in ["\"", "a OR", "NEAR(a b)", "col:valor", "*", "'; DROP TABLE facts;--", "", "?!"] {
        assert!(s.search(sujo, 8, &Kind::ALL).is_ok(), "{sujo}");
    }
    assert_eq!(s.facts().unwrap().len(), 1);
}

#[test]
fn busca_cobre_conversas_e_filtra_por_tipo() {
    let d = tmp();
    let s = loja(d.path());
    let sess = s.active_session("telegram").unwrap();
    s.add_message(&sess.id, "user", "preciso comprar um notebook novo", None, None).unwrap();
    s.add_message(&sess.id, "tool", "notebook notebook notebook", None, None).unwrap(); // ferramenta não é memória
    let hits = s.search("notebook", 8, &Kind::ALL).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].kind, hits[0].source.as_str()), (Kind::Message, "sessão telegram"));
    assert!(s.search("notebook", 8, &fts_f()).unwrap().is_empty());
}

// ── test_store.py: documentos / vault ───────────────────────────────────────
#[test]
fn documento_idempotente_e_reindexa_quando_muda() {
    let d = tmp();
    let s = loja(d.path());
    let md = "---\ntype: x\n---\nconteúdo sobre redes neurais";
    assert_eq!(s.index_document("a.md", "Nota A", md).unwrap(), IndexResult::New);
    assert_eq!(s.index_document("a.md", "Nota A", md).unwrap(), IndexResult::Same);
    assert_eq!(s.index_document("a.md", "Nota A", "agora fala de bancos de dados").unwrap(), IndexResult::Updated);
    assert!(s.search("redes neurais", 8, &Kind::ALL).unwrap().is_empty());
    let h = &s.search("bancos de dados", 8, &Kind::ALL).unwrap()[0];
    assert!(h.kind == Kind::Chunk && h.source == "a.md" && h.text.starts_with("Nota A"));
    assert!(!h.text.contains("type: x"));
}

#[test]
fn vault_ignora_ocultos_e_remove_o_que_sumiu() {
    let d = tmp();
    let s = loja(d.path());
    let v = d.path().join("vault");
    std::fs::create_dir_all(v.join(".obsidian")).unwrap();
    std::fs::create_dir_all(v.join("Projetos")).unwrap();
    std::fs::write(v.join(".obsidian/config.md"), "segredo da config").unwrap();
    std::fs::write(v.join("Projetos/Orion.md"), "# Orion\nassistente pessoal").unwrap();
    std::fs::write(v.join("Ideias.md"), "ideia de app").unwrap();
    let c = s.index_vault(&v).unwrap();
    assert_eq!((c.new, c.updated, c.same, c.removed), (2, 0, 0, 0));
    assert!(s.search("segredo da config", 8, &Kind::ALL).unwrap().is_empty());
    std::fs::remove_file(v.join("Ideias.md")).unwrap();
    std::fs::write(v.join("Projetos/Orion.md"), "# Orion\nassistente pessoal v2").unwrap();
    let c = s.index_vault(&v).unwrap();
    assert_eq!((c.new, c.updated, c.same, c.removed), (0, 1, 0, 1));
    assert!(s.search("ideia de app", 8, &Kind::ALL).unwrap().is_empty());
}

#[test]
fn vault_ignora_arquivo_gigante() {
    // o Python reduz MAX_NOTA_BYTES com monkeypatch; aqui o arquivo passa do teto real (5 MiB)
    let d = tmp();
    let s = loja(d.path());
    let v = d.path().join("vault");
    std::fs::create_dir_all(&v).unwrap();
    std::fs::write(v.join("pequena.md"), "curta").unwrap();
    std::fs::write(v.join("grande.md"), "x ".repeat(orion_store::documents::MAX_NOTA_BYTES as usize / 2 + 10)).unwrap();
    assert_eq!(s.index_vault(&v).unwrap().new, 1);
}

// ── test_store.py: vetores ──────────────────────────────────────────────────
#[test]
fn vetor_acha_sinonimo_que_a_palavra_chave_nao_acha() {
    let d = tmp();
    let emb = Fake::novo();
    let (s, sv) = (loja(d.path()), loja_vec(d.path(), &emb));
    for x in [&s, &sv] {
        x.add_fact("Antônio estuda ADS na UNIMAR", "manual", None).unwrap();
        x.add_fact("Antônio mora em Marília-SP", "manual", None).unwrap();
    }
    assert!(s.search("universidade", 8, &fts_f()).unwrap().is_empty()); // só FTS: nada
    let hit = &sv.search("universidade", 8, &fts_f()).unwrap()[0];
    assert!(hit.text.contains("UNIMAR") && hit.via == "vec");
    assert!(!hit.text.contains("MORAR"));
}

#[test]
fn busca_hibrida_combina_as_duas_vias() {
    let d = tmp();
    let sv = loja_vec(d.path(), &Fake::novo());
    sv.add_fact("Antônio estuda ADS na UNIMAR", "manual", None).unwrap();
    assert_eq!(sv.search("estuda UNIMAR", 8, &fts_f()).unwrap()[0].via, "fts+vec");
}

#[test]
fn embedder_fora_do_ar_nao_derruba_escrita_nem_busca() {
    let d = tmp();
    let emb = Fake::novo();
    let sv = loja_vec(d.path(), &emb);
    emb.quebrado.store(true, Ordering::SeqCst);
    let f = sv.add_fact("Antônio mora em Marília-SP", "manual", None).unwrap(); // não falha
    let emb_flag: i64 = sv.conn().query_row("SELECT embedded FROM facts WHERE id=?", [f.id], |r| r.get(0)).unwrap();
    assert_eq!(emb_flag, 0);
    let hits = sv.search("Marília", 8, &Kind::ALL).unwrap(); // cai para palavra-chave
    assert!(!hits.is_empty() && hits[0].via == "fts");
    emb.quebrado.store(false, Ordering::SeqCst);
    assert_eq!(sv.embed_pending(32).unwrap(), 1);
    assert_eq!(sv.search("cidade onde moro", 8, &fts_f()).unwrap()[0].id, f.id);
}

#[test]
fn editar_e_esquecer_atualizam_o_vetor() {
    let d = tmp();
    let sv = loja_vec(d.path(), &Fake::novo());
    let f = sv.add_fact("Antônio mora em Marília", "manual", None).unwrap();
    sv.update_fact(f.id, "Antônio estuda na faculdade", Some("manual")).unwrap();
    assert_eq!(sv.search("universidade", 8, &fts_f()).unwrap()[0].id, f.id);
    sv.forget_fact(f.id).unwrap();
    assert!(sv.search("universidade", 8, &fts_f()).unwrap().is_empty());
    assert_eq!(vetores(&sv), 0);
}

struct Outro;
impl Embedder for Outro {
    fn dim(&self) -> usize {
        16
    }
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![1.0; 16]).collect())
    }
}

#[test]
fn troca_de_dimensao_exige_reset() {
    let d = tmp();
    let emb = Fake::novo();
    let caminho = d.path().join("x.db");
    {
        let s = Store::open_with(&caminho, Some(emb.clone()), orion_policy::wall_clock()).unwrap();
        s.add_fact("Antônio mora em Marília", "manual", None).unwrap();
    }
    let e = Store::open_with(&caminho, Some(Arc::new(Outro)), orion_policy::wall_clock()).err().unwrap();
    assert!(e.to_string().contains("reset_vectors"), "{e}");
    let s = Store::open_with(&caminho, Some(emb), orion_policy::wall_clock()).unwrap();
    s.reset_vectors().unwrap();
    assert_eq!(s.embed_pending(32).unwrap(), 1);
}

#[test]
fn sem_embedder_nao_ha_vetores_e_a_busca_e_so_por_palavra() {
    let d = tmp();
    let s = loja(d.path());
    s.add_fact("Antônio mora em Marília", "manual", None).unwrap();
    assert!(!s.vectors_available && s.embed_pending(32).unwrap() == 0);
    assert_eq!(s.search("Marília", 8, &Kind::ALL).unwrap()[0].via, "fts");
    assert_eq!(vetores(&s), 0);
}

#[test]
fn vetores_ficam_no_banco_e_sobrevivem_a_backup_e_reabertura() {
    let d = tmp();
    let emb = Fake::novo();
    let s = Store::open_with(d.path().join("v.db"), Some(emb.clone()), orion_policy::wall_clock()).unwrap();
    s.add_fact("Antônio estuda ADS na UNIMAR", "manual", None).unwrap();
    let bk = s.backup_to(&d.path().join("bk.db")).unwrap();
    drop(s);
    // `Store::restore` abre sem embedder; reabre com o mesmo embedder sobre o arquivo restaurado
    Store::restore(&bk, &d.path().join("novo.db")).unwrap();
    let novo = Store::open_with(d.path().join("novo.db"), Some(emb), orion_policy::wall_clock()).unwrap();
    assert_eq!(novo.embed_pending(32).unwrap(), 0); // nada a refazer: os vetores vieram no backup
    assert_eq!(novo.search("universidade", 8, &fts_f()).unwrap()[0].via, "vec");
}

#[test]
fn limpeza_preserva_documentos_busca_e_vetores() {
    let d = tmp();
    let sv = loja_vec(d.path(), &Fake::novo());
    let sess = sv.new_session("web", None, None).unwrap();
    sv.add_message(&sess.id, "user", "Antônio estuda na UNIMAR", None, None).unwrap();
    sv.add_fact("Antônio estuda na UNIMAR", "manual", None).unwrap();
    sv.index_document("nota.md", "Estudos", "Antônio estuda na UNIMAR").unwrap();
    sv.embed_pending(32).unwrap();
    let snap = |s: &Store| {
        s.search("universidade", 8, &Kind::ALL).unwrap().into_iter().map(|h| (h.kind, h.id, h.text)).collect::<Vec<_>>()
    };
    let vecs = |s: &Store| s.query("SELECT * FROM vectors ORDER BY kind, ref_id", &[]).unwrap();
    let (antes, vantes) = (snap(&sv), vecs(&sv));
    assert!(!antes.is_empty() && !vantes.is_empty());
    sv.clear_context(&sess.id).unwrap();
    assert!(sv.context_history(&sess.id, 50).unwrap().is_empty());
    assert_eq!(snap(&sv), antes);
    assert_eq!(vecs(&sv), vantes);
}

// ── test_store.py: backup e restauração ─────────────────────────────────────
fn popular(s: &Store) {
    s.add_fact("Antônio mora em Marília-SP", "manual", None).unwrap();
    s.index_document("n.md", "Nota", "texto sobre o projeto orion").unwrap();
    let sess = s.active_session("web").unwrap();
    s.add_message(&sess.id, "user", "lembra do projeto orion?", None, None).unwrap();
}

#[test]
fn backup_diario_rotaciona_e_nao_duplica_no_mesmo_dia() {
    let d = tmp();
    let rel = Relogio::em(1_700_000_000.0);
    let s = Store::open_with(d.path().join("orion.db"), None, rel.clock()).unwrap();
    popular(&s);
    let pasta = d.path().join("bk");
    assert!(s.daily_backup(&pasta, 3).unwrap().is_some());
    assert!(s.daily_backup(&pasta, 3).unwrap().is_none()); // já tem o de hoje
    for _ in 0..4 {
        rel.avanca(86_400.0);
        s.daily_backup(&pasta, 3).unwrap();
    }
    let n = std::fs::read_dir(&pasta).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("orion-")).count();
    assert_eq!(n, 3);
}

#[test]
fn restore_drill_backup_restaura_e_responde_igual() {
    let d = tmp();
    let s = loja(d.path());
    popular(&s);
    let bk = s.backup_to(&d.path().join("bk").join("orion.db")).unwrap();
    let contagens: std::collections::BTreeMap<_, _> = Store::verify_backup(&bk).unwrap().into_iter().collect();
    let esperado: std::collections::BTreeMap<_, _> = [
        ("sessions", 1), ("messages", 1), ("facts", 1), ("documents", 1), ("chunks", 1),
        ("reminders", 0), ("schedules", 0), ("tasks", 0), ("numbers", 0), ("prompts", 0), ("edges", 0),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    assert_eq!(contagens, esperado);
    let novo = Store::restore(&bk, &d.path().join("novo").join("orion.db")).unwrap();
    let tipos: HashSet<_> = novo.search("projeto orion", 8, &Kind::ALL).unwrap().into_iter().map(|h| h.kind).collect();
    assert_eq!(tipos, HashSet::from([Kind::Chunk, Kind::Message]));
    assert_eq!(novo.search("Marília", 8, &Kind::ALL).unwrap()[0].kind, Kind::Fact);
    let ativa = novo.active_session("web").unwrap();
    assert_eq!(novo.history(&ativa.id, 50, 0).unwrap()[0].text, "lembra do projeto orion?");
}

#[test]
fn backup_corrompido_ou_estranho_e_recusado() {
    let d = tmp();
    let s = loja(d.path());
    popular(&s);
    let bk = s.backup_to(&d.path().join("ok.db")).unwrap();
    let ruim = d.path().join("ruim.db");
    std::fs::write(&ruim, &std::fs::read(&bk).unwrap()[..2048]).unwrap();
    assert!(Store::verify_backup(&ruim).is_err()); // corrompido: qualquer erro do sqlite serve
    let estranho = d.path().join("estranho.db");
    let c = rusqlite::Connection::open(&estranho).unwrap();
    c.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT); INSERT INTO meta VALUES('schema_version','99');").unwrap();
    drop(c);
    let e = Store::verify_backup(&estranho).unwrap_err();
    assert!(matches!(e, StoreError::Invalid(_)) && e.to_string().contains("esquema"), "{e}");
}

// ── test_sessions.py ────────────────────────────────────────────────────────
#[test]
fn resposta_atrasada_e_relogio_igual_nao_trocam_selecao() {
    let d = tmp();
    let caminho = d.path().join("sessions.db");
    let (a, b);
    {
        let s = Store::open_with(&caminho, None, Relogio::em(100.0).clock()).unwrap();
        a = s.new_session("web", None, None).unwrap();
        b = s.new_session("web", None, None).unwrap();
        s.activate_session("web", &a.id).unwrap();
        s.add_message(&b.id, "assistant", "Resposta atrasada de B", None, None).unwrap();
        assert_eq!(s.active_session("web").unwrap().id, a.id);
    }
    let s = Store::open_with(&caminho, None, Relogio::em(1.0).clock()).unwrap();
    assert_eq!(s.active_session("web").unwrap().id, a.id);
    s.activate_session("web", &b.id).unwrap();
    s.add_message(&a.id, "assistant", "Resposta atrasada de A", None, None).unwrap();
    assert_eq!(s.active_session("web").unwrap().id, b.id);
}

#[test]
fn primeiro_acesso_concorrente_cria_uma_sessao_por_canal() {
    let d = tmp();
    let s = Arc::new(loja(d.path()));
    let ids: Vec<String> = (0..8)
        .map(|_| {
            let s = s.clone();
            std::thread::spawn(move || (0..3).map(|_| s.active_session("web").unwrap().id).collect::<Vec<_>>())
        })
        .collect::<Vec<_>>()
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 1);
    assert_eq!(s.list_sessions(Some("web"), 100, None, None).unwrap().len(), 1);
}

#[test]
fn arquivar_ativa_nao_reabre_conversa_anterior() {
    let d = tmp();
    let s = loja(d.path());
    let antiga = s.new_session("web", None, None).unwrap();
    let atual = s.new_session("web", None, None).unwrap();
    s.archive_session(&atual.id).unwrap();
    assert!(s.selected_session("web").unwrap().is_none());
    let nova = s.active_session("web").unwrap().id;
    assert!(nova != antiga.id && nova != atual.id);
}

/// Cria um banco no esquema `v` aplicando os arquivos `sql/vNN.sql`.
fn banco_ate(caminho: &Path, v: u32, depois: &str) {
    let c = rusqlite::Connection::open(caminho).unwrap();
    let sqls = [
        include_str!("../sql/v01.sql"),
        include_str!("../sql/v02.sql"),
        include_str!("../sql/v03.sql"),
        include_str!("../sql/v04.sql"),
    ];
    for s in &sqls[..v as usize] {
        c.execute_batch(s).unwrap();
    }
    c.execute("INSERT INTO meta VALUES ('schema_version', ?)", [v.to_string()]).unwrap();
    c.execute_batch(depois).unwrap();
}

#[test]
fn banco_v2_migra_selecao_sem_perder_mensagens() {
    let d = tmp();
    let caminho = d.path().join("v2.db");
    banco_ate(
        &caminho,
        2,
        "INSERT INTO sessions VALUES ('old','web','old',1,1,0);
         INSERT INTO sessions VALUES ('new','web','new',2,2,0);
         INSERT INTO sessions VALUES ('archive','web','archive',9,9,1);
         INSERT INTO sessions VALUES ('tg','telegram','tg',3,3,0);
         INSERT INTO messages(session_id, role, text, created_at) VALUES ('new','user','Mensagem v2',2);",
    );
    {
        let s = Store::open(&caminho).unwrap();
        assert_eq!(versao(&s), orion_store::schema::SCHEMA_VERSION.to_string());
        assert_eq!(s.selected_session("web").unwrap().unwrap().id, "new");
        assert_eq!(s.selected_session("telegram").unwrap().unwrap().id, "tg");
        assert_eq!(s.history("new", 50, 0).unwrap()[0].text, "Mensagem v2");
        s.activate_session("web", "old").unwrap();
    }
    let s = Store::open(&caminho).unwrap();
    assert_eq!(s.active_session("web").unwrap().id, "old");
    assert_eq!(s.history("new", 50, 0).unwrap()[0].text, "Mensagem v2");
}

#[test]
fn v4_migra_indice_de_titulos_preservando_fixacao_e_contexto() {
    let d = tmp();
    let caminho = d.path().join("v4.db");
    banco_ate(
        &caminho,
        4,
        "INSERT INTO sessions VALUES ('abc','web','Cérebro antigo',1,1,0,1);
         INSERT INTO active_sessions VALUES ('web','abc');
         INSERT INTO messages(id,session_id,role,text,created_at) VALUES (1,'abc','user','antiga',1);
         INSERT INTO meta VALUES ('counter:history_after:abc','1');",
    );
    let s = Store::open(&caminho).unwrap();
    let (resultado, total) = s.search_sessions("web", "cerebro", 20, 0, None).unwrap();
    assert!(total == 1 && resultado[0].0.favorite);
    assert_eq!(s.selected_session("web").unwrap().unwrap().id, "abc");
    assert!(s.context_history("abc", 50).unwrap().is_empty());
    assert_eq!(s.history("abc", 50, 0).unwrap()[0].text, "antiga");
}

// ── test_ops.py ─────────────────────────────────────────────────────────────
use orion_store::ops::Operations;

const AGORA_ARGS: (i32, u32, u32, u32, u32) = (2026, 10, 3, 12, 0);

fn agora() -> f64 {
    let (a, m, d, h, mi) = AGORA_ARGS;
    em(a, m, d, h, mi)
}

fn ops_em(d: &Path) -> (Operations, Relogio) {
    let rel = Relogio::em(agora());
    let s = Store::open_with(d.join("orion.db"), None, rel.clock()).unwrap();
    (Operations::new(Arc::new(s)), rel)
}

fn n(r: &orion_store::ops::Row, k: &str) -> f64 {
    r[k].as_f64().unwrap()
}

fn titulos(rs: &[orion_store::ops::Row]) -> Vec<String> {
    rs.iter().map(|r| r["title"].as_str().unwrap().to_string()).collect()
}

#[test]
fn parse_when_aceita_iso_e_epoch_e_recusa_lixo() {
    assert_eq!(parse_when(&"2026-06-25T14:30:00".into()).unwrap(), em(2026, 6, 25, 14, 30));
    assert_eq!(parse_when(&When::Epoch(123.5)).unwrap(), 123.5);
    let e = parse_when(&"amanhã".into()).unwrap_err();
    assert!(e.to_string().contains("ISO"), "{e}");
}

#[test]
fn proximo_disparo_de_cada_tipo() {
    let a = agora();
    assert_eq!(next_run("unico", a, Some(a + 60.0), "", 0).unwrap(), a + 60.0);
    assert_eq!(next_run("diario", a, None, "14:30", 0).unwrap(), em(2026, 10, 3, 14, 30));
    assert_eq!(next_run("diario", a, None, "08:00", 0).unwrap(), em(2026, 10, 4, 8, 0)); // já passou hoje
    assert_eq!(next_run("diario", a, None, "12:00", 0).unwrap(), em(2026, 10, 4, 12, 0)); // igual a agora: amanhã
    assert_eq!(next_run("intervalo_min", a, None, "", 15).unwrap(), a + 900.0);
    for (tipo, tod) in [("unico", ""), ("diario", "8h"), ("intervalo_min", "")] {
        assert!(next_run(tipo, a, None, tod, 0).is_err(), "{tipo}");
    }
    assert!(next_run("cron", a, None, "", 0).unwrap_err().to_string().contains("tipo inválido"));
}

#[test]
fn lembrete_vence_avisa_uma_vez_e_conclui() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    let r = ops.add_reminder("Pagar boleto", &"2026-10-03T11:00:00".into(), "banco", 0).unwrap();
    let futuro = ops.add_reminder("Dentista", &"2026-10-05T09:00:00".into(), "", 0).unwrap();
    assert!(n(&r, "due_at") == em(2026, 10, 3, 11, 0) && r["note"] == "banco" && r["done"] == 0);
    assert_eq!(titulos(&ops.due_reminders(None).unwrap()), ["Pagar boleto"]);
    ops.mark_reminder_notified(r["id"].as_i64().unwrap()).unwrap();
    assert!(ops.due_reminders(None).unwrap().is_empty()); // avisado uma vez só
    assert_eq!(titulos(&ops.due_reminders(Some(em(2026, 10, 6, 0, 0))).unwrap()), ["Dentista"]);
    assert_eq!(ops.complete_reminder(futuro["id"].as_i64().unwrap()).unwrap()["done"], 1);
    assert_eq!(titulos(&ops.list_reminders(false).unwrap()), ["Pagar boleto"]);
    assert_eq!(ops.list_reminders(true).unwrap().len(), 2);
    let id = r["id"].as_i64().unwrap();
    assert!(ops.remove_reminder(id).unwrap() && !ops.remove_reminder(id).unwrap());
}

#[test]
fn lembrete_invalido() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    assert!(ops.add_reminder("  ", &"2026-10-03T11:00:00".into(), "", 0).unwrap_err().to_string().contains("título"));
    assert!(ops.add_reminder("x", &"semana que vem".into(), "", 0).unwrap_err().to_string().contains("ISO"));
    assert!(matches!(ops.complete_reminder(999), Err(StoreError::NotFound(_))));
}

#[test]
fn agendamento_diario_dispara_e_recalcula() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    let s = ops.add_schedule("Briefing", "diario", None, "14:00", 0, "checar_saude_sistema", None).unwrap();
    let id = s["id"].as_i64().unwrap();
    assert!(n(&s, "next_run") == em(2026, 10, 3, 14, 0) && s["params"] == "{}");
    assert!(ops.due_schedules(None).unwrap().is_empty());
    let depois = em(2026, 10, 3, 14, 5);
    assert_eq!(titulos(&ops.due_schedules(Some(depois)).unwrap()), ["Briefing"]);
    let feito = ops.mark_schedule_fired(id, Some(depois)).unwrap();
    assert!(n(&feito, "last_run") == depois && n(&feito, "next_run") == em(2026, 10, 4, 14, 0));
    // app ficou três dias desligado: dispara uma vez e segue do horário seguinte (sem repetição)
    let muito_depois = em(2026, 10, 7, 20, 0);
    assert_eq!(ops.due_schedules(Some(muito_depois)).unwrap().len(), 1);
    assert_eq!(n(&ops.mark_schedule_fired(id, Some(muito_depois)).unwrap(), "next_run"), em(2026, 10, 8, 14, 0));
}

#[test]
fn agendamento_unico_desativa_depois_de_disparar() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    let q: When = "2026-10-03T12:30:00".into();
    let s = ops.add_schedule("Uma vez", "unico", Some(&q), "", 0, "", None).unwrap();
    ops.mark_schedule_fired(s["id"].as_i64().unwrap(), Some(em(2026, 10, 3, 12, 31))).unwrap();
    let fim = &ops.list_schedules().unwrap()[0];
    assert!(fim["active"] == 0 && fim["next_run"].is_null());
    assert!(ops.due_schedules(Some(em(2030, 1, 1, 0, 0))).unwrap().is_empty());
}

#[test]
fn agendamento_pausar_e_retomar() {
    let d = tmp();
    let (ops, rel) = ops_em(d.path());
    let s = ops.add_schedule("A cada meia hora", "intervalo_min", None, "", 30, "", None).unwrap();
    let id = s["id"].as_i64().unwrap();
    assert_eq!(n(&s, "next_run"), agora() + 1800.0);
    assert_eq!(ops.set_schedule_active(id, false).unwrap()["active"], 0);
    assert!(ops.due_schedules(Some(agora() + 99_999.0)).unwrap().is_empty());
    rel.set(agora() + 7200.0); // 2h depois, retoma: não dispara o atraso, recalcula
    let r = ops.set_schedule_active(id, true).unwrap();
    assert!(r["active"] == 1 && n(&r, "next_run") == agora() + 7200.0 + 1800.0);
}

#[test]
fn agendamento_guarda_parametros_como_json_e_valida() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    let s = ops
        .add_schedule("Clima", "diario", None, "07:30", 0, "consultar_clima", Some(&json!({"cidade": "Marília"})))
        .unwrap();
    assert_eq!(s["params"], "{\"cidade\": \"Marília\"}");
    assert!(ops.add_schedule("", "diario", None, "07:30", 0, "", None).is_err());
    assert!(ops.add_schedule("sem horário", "diario", None, "", 0, "", None).is_err());
    assert!(ops.remove_schedule(s["id"].as_i64().unwrap()).unwrap());
}

#[test]
fn tarefas_e_objetivos_em_aberto() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    let a = ops.add_task("Estudar UML").unwrap();
    let b = ops.add_task("Entregar trabalho").unwrap();
    ops.add_reminder("Pagar boleto", &"2026-10-04T09:00:00".into(), "", 0).unwrap();
    assert_eq!(ops.set_task_status(a["id"].as_i64().unwrap(), "concluida").unwrap()["status"], "concluida");
    assert_eq!(titulos(&ops.list_tasks(Some("pendente")).unwrap()), ["Entregar trabalho"]);
    assert_eq!(
        ops.open_goals(10).unwrap(),
        ["tarefa (pendente): Entregar trabalho", "lembrete para 04/10 09:00: Pagar boleto"]
    );
    assert_eq!(ops.open_goals(1).unwrap().len(), 1);
    let bid = b["id"].as_i64().unwrap();
    assert!(ops.set_task_status(bid, "feito").unwrap_err().to_string().contains("status"));
    assert!(ops.add_task(" ").unwrap_err().to_string().contains("título"));
    assert!(ops.remove_task(bid).unwrap());
}

#[test]
fn numeros_limitam_score_e_filtram_pendentes() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    let alto = ops.add_number("fatura", 5.0, "vence amanhã", "").unwrap();
    ops.add_number("log", 0.2, "", "").unwrap();
    assert_eq!(n(&alto, "score"), 1.0);
    let alvos: Vec<_> = ops.list_numbers(true, 0.5).unwrap().iter().map(|r| r["target"].as_str().unwrap().to_string()).collect();
    assert_eq!(alvos, ["fatura"]);
    ops.mark_number_notified(alto["id"].as_i64().unwrap()).unwrap();
    assert!(ops.list_numbers(true, 0.5).unwrap().is_empty());
    assert_eq!(ops.list_numbers(false, 0.0).unwrap().len(), 2);
    assert!(ops.add_number(" ", 0.5, "", "").is_err());
}

#[test]
fn prompts_salvos() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    let p = ops.add_prompt("Resumo", "Resuma em 5 linhas", "resumo").unwrap();
    assert_eq!(ops.list_prompts().unwrap()[0]["command"], "resumo");
    assert!(ops.remove_prompt(p["id"].as_i64().unwrap()).unwrap() && ops.list_prompts().unwrap().is_empty());
    assert!(ops.add_prompt("sem conteúdo", " ", "").is_err());
}

#[test]
fn grafo_nao_duplica_aresta_e_lista_os_dois_sentidos() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    assert!(ops.add_edge("message:1", "sobre", "topic:unimar", 1.0, "").unwrap());
    assert!(!ops.add_edge("message:1", "sobre", "topic:unimar", 1.0, "").unwrap());
    ops.add_edge("topic:unimar", "conecta", "topic:ads", 2.0, "rem").unwrap();
    assert_eq!(ops.edge_count(), 2);
    let viz = ops.neighbors("topic:unimar", None, 50).unwrap();
    let pares: HashSet<_> = viz.iter().map(|v| (v["direction"].as_str().unwrap().to_string(), v["rel"].as_str().unwrap().to_string())).collect();
    assert_eq!(pares, HashSet::from([("in".into(), "sobre".into()), ("out".into(), "conecta".into())]));
    let so: Vec<_> = ops.neighbors("topic:unimar", Some("conecta"), 50).unwrap().iter().map(|v| v["dst"].as_str().unwrap().to_string()).collect();
    assert_eq!(so, ["topic:ads"]);
    assert!(ops.neighbors("topic:inexistente", None, 50).unwrap().is_empty());
}

#[test]
fn fila_de_avisos_entrega_em_ordem_e_confirma_uma_vez() {
    let d = tmp();
    let (ops, _) = ops_em(d.path());
    let a = ops.notify("lembrete", "Pagar boleto", Some("reminder:1"), false).unwrap();
    let b = ops.notify("agendamento", "Briefing", None, false).unwrap();
    let ids = |ops: &Operations| ops.pending_notifications(50, false).unwrap().iter().map(|r| r["id"].as_i64().unwrap()).collect::<Vec<_>>();
    assert_eq!(ids(&ops), [a, b]);
    assert!(ops.ack_notification(a).unwrap() && !ops.ack_notification(a).unwrap());
    let textos: Vec<_> = ops.pending_notifications(50, false).unwrap().iter().map(|r| r["text"].as_str().unwrap().to_string()).collect();
    assert_eq!(textos, ["Briefing"]);
}

fn banco_v1(caminho: &Path) {
    banco_ate(
        caminho,
        1,
        "INSERT INTO sessions VALUES ('s1', 'web', 'antiga', 1.0, 1.0, 0);
         INSERT INTO messages(session_id, role, text, created_at) VALUES ('s1','user','oi',1.0);",
    );
}

#[test]
fn banco_v1_sobe_para_a_versao_atual_sem_perder_dados() {
    let d = tmp();
    let caminho = d.path().join("v1.db");
    banco_v1(&caminho);
    {
        let s = Arc::new(Store::open(&caminho).unwrap());
        assert_eq!(versao(&s), orion_store::schema::SCHEMA_VERSION.to_string());
        assert_eq!(s.history("s1", 50, 0).unwrap()[0].text, "oi");
        Operations::new(s.clone()).add_task("funciona depois da migração").unwrap();
        assert_eq!(s.search("oi", 8, &[Kind::Message]).unwrap()[0].text, "oi"); // FTS segue de pé
    }
    Store::open(&caminho).unwrap(); // idempotente: abrir de novo não migra de novo
}

#[test]
fn backup_v1_e_aceito_e_banco_de_versao_futura_e_recusado() {
    let d = tmp();
    let antigo = d.path().join("antigo.db");
    banco_v1(&antigo);
    let contagens: std::collections::HashMap<_, _> = Store::verify_backup(&antigo).unwrap().into_iter().collect();
    assert!(contagens["messages"] == 1 && !contagens.contains_key("reminders")); // v1: sem tabelas novas
    let restaurado = Store::restore(&antigo, &d.path().join("novo").join("orion.db")).unwrap();
    assert_eq!(versao(&restaurado), orion_store::schema::SCHEMA_VERSION.to_string());

    let futuro = d.path().join("futuro.db");
    banco_v1(&futuro);
    rusqlite::Connection::open(&futuro).unwrap().execute("UPDATE meta SET value='99' WHERE key='schema_version'", []).unwrap();
    let e = Store::open(&futuro).err().unwrap();
    assert!(e.to_string().contains("só entende até"), "{e}");
}

#[test]
fn banco_v2_sobe_para_v3_e_ganha_a_tabela_audit() {
    let d = tmp();
    let caminho = d.path().join("v2.db");
    banco_ate(&caminho, 2, "");
    let s = Arc::new(Store::open(&caminho).unwrap());
    assert_eq!(versao(&s), orion_store::schema::SCHEMA_VERSION.to_string());
    assert!(Operations::new(s).audit_recent(50, None, None, None).unwrap().is_empty());
}

#[test]
fn audit_grava_filtra_e_poda() {
    let d = tmp();
    let (ops, rel) = ops_em(d.path());
    ops.audit_add(&json!({"session_id": "s1", "tool": "executar_comando", "action": "confirm", "risk": "exec",
        "reason": "fora da lista", "tainted": true, "args": {"cmd": "rm -rf x"}}))
        .unwrap();
    ops.audit_add(&json!({"session_id": "s1", "tool": "ler_arquivo", "action": "allow", "risk": "read"})).unwrap();
    let todos = ops.audit_recent(50, None, None, None).unwrap();
    let ferr: Vec<_> = todos.iter().map(|r| r["tool"].as_str().unwrap().to_string()).collect();
    assert_eq!(ferr, ["ler_arquivo", "executar_comando"]); // mais novo primeiro
    assert!(todos[1]["args"] == json!({"cmd": "rm -rf x"}) && todos[1]["tainted"] == 1);
    assert_eq!(ops.audit_recent(50, Some("executar_comando"), None, None).unwrap().len(), 1);
    assert_eq!(ops.audit_recent(50, None, Some("allow"), None).unwrap().len(), 1);
    assert!(ops.audit_recent(50, None, None, Some(agora() + 1.0)).unwrap().is_empty());
    assert_eq!(ops.audit_prune(1).unwrap(), 0);
    rel.set(agora() + 91.0 * 86400.0);
    assert_eq!(ops.audit_prune(90).unwrap(), 2);
    assert!(ops.audit_recent(50, None, None, None).unwrap().is_empty());
}

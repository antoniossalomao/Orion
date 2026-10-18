//! Grafo da memória: fatos, tópicos, falas, relações do sono e notas, e o caminho mais curto
//! entre duas coisas ("o que liga X a Y?"). Montado na hora, só de leitura, no formato que o
//! front espera: `{nodes, links}`. Só a memória pessoal tem arestas (importação, sono e vault); um
//! projeto vê os fatos e os documentos DELE (REGRAS.md: o que é de um projeto fica no projeto).

use orion_store::{with_scope, DataScope, Store};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use unicode_normalization::UnicodeNormalization;

pub const MAX_ARESTAS: i64 = 20_000;
pub const LIMITE_PADRAO: usize = 500;

pub fn plano(texto: &str) -> String {
    let sem: String = texto.nfkd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect();
    sem.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn id_do_no(bruto: &str) -> String {
    if ["topic:", "message:", "doc:", "fato:"].iter().any(|p| bruto.starts_with(p)) {
        bruto.to_string()
    } else {
        format!("entidade:{bruto}")
    }
}

fn s<'a>(m: &'a Map<String, Value>, k: &str) -> &'a str {
    m.get(k).and_then(Value::as_str).unwrap_or("")
}

fn f(m: &Map<String, Value>, k: &str) -> f64 {
    m.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// `limite = None`: o grafo todo (para achar caminhos).
#[derive(Default)]
struct Nos {
    nos: Vec<(String, Value)>,
    indice: HashMap<String, usize>,
}

impl Nos {
    fn put(&mut self, id: String, v: Value) {
        if let Some(&i) = self.indice.get(&id) {
            self.nos[i].1 = v;
        } else {
            self.indice.insert(id.clone(), self.nos.len());
            self.nos.push((id, v));
        }
    }
    fn tem(&self, id: &str) -> bool {
        self.indice.contains_key(id)
    }
}

pub fn montar(memory: &Store, project_id: Option<&str>, limite: Option<usize>, pasta: &str) -> anyhow::Result<Value> {
    let mut g = Nos::default();
    let mut ligacoes: Vec<Value> = vec![];

    let (fatos, notas) = with_scope(DataScope::project(project_id.map(String::from), false), || -> anyhow::Result<_> {
        let fatos = memory.facts()?;
        let notas = memory.query(
            "SELECT source, title, indexed_at FROM documents WHERE project_id IS ? ORDER BY indexed_at DESC LIMIT 3000",
            &[&project_id],
        )?;
        Ok((fatos, notas))
    })?;
    for fa in &fatos {
        let id = format!("fato:{}", fa.id);
        let label: String = fa.text.chars().take(140).collect();
        g.put(id.clone(), json!({"id": id, "tipo": "fato", "label": label, "ts": fa.updated_at * 1000.0, "antigo": fa.old, "ref": fa.id}));
    }
    for n in &notas {
        let fonte = s(n, "source");
        if !pasta.is_empty() && !fonte.starts_with(&format!("{}/", pasta.trim_end_matches('/'))) {
            continue;
        }
        let id = format!("doc:{fonte}");
        let dir = fonte.rsplit_once('/').map(|x| x.0).unwrap_or("");
        g.put(id.clone(), json!({"id": id, "tipo": "nota", "label": s(n, "title"), "ts": f(n, "indexed_at") * 1000.0, "pasta": dir}));
    }

    if project_id.is_none() {
        let arestas = memory.query("SELECT src, rel, dst, kind FROM edges ORDER BY created_at DESC LIMIT ?", &[&MAX_ARESTAS])?;
        let mut mensagens: HashSet<i64> = HashSet::new();
        for a in &arestas {
            for ponta in [s(a, "src"), s(a, "dst")] {
                if let Some(n) = ponta.strip_prefix("message:").and_then(|x| x.parse::<i64>().ok()) {
                    mensagens.insert(n);
                }
            }
        }
        let textos = mensagens_por_id(memory, &mensagens)?;
        for a in &arestas {
            let (src, dst) = (s(a, "src"), s(a, "dst"));
            let (origem, destino) = (id_do_no(src), id_do_no(dst));
            if s(a, "kind") == "vault" && !(g.tem(&origem) && g.tem(&destino)) {
                continue; // nota fora do filtro de pasta
            }
            for (bruto, nid) in [(src, &origem), (dst, &destino)] {
                if g.tem(nid) {
                    continue;
                }
                if nid.starts_with("topic:") {
                    g.put(nid.clone(), json!({"id": nid, "tipo": "topico", "label": &bruto[6..]}));
                } else if nid.starts_with("message:") {
                    let Some(q) = bruto[8..].parse::<i64>().ok().and_then(|i| textos.get(&i)) else { break };
                    let mut v = json!({"id": nid, "tipo": "evento"});
                    for (k, x) in q {
                        v[k] = x.clone();
                    }
                    g.put(nid.clone(), v);
                } else if nid.starts_with("entidade:") {
                    g.put(nid.clone(), json!({"id": nid, "tipo": "topico", "label": bruto}));
                } else if nid.starts_with("doc:") {
                    break; // nota que não está indexada (apagada do vault)
                }
            }
            if g.tem(&origem) && g.tem(&destino) {
                ligacoes.push(json!({"source": origem, "target": destino, "rel": a["rel"]}));
            }
        }
    }

    ligar_fatos_a_coisas(&g.nos, &mut ligacoes);

    let mut truncado = false;
    let mut nos = g.nos;
    if let Some(lim) = limite.filter(|l| nos.len() > *l) {
        truncado = true;
        let mut grau: HashMap<String, i64> = HashMap::new();
        for lk in &ligacoes {
            for k in ["source", "target"] {
                *grau.entry(lk[k].as_str().unwrap_or("").to_string()).or_default() += 1;
            }
        }
        let ts = |v: &Value| v["ts"].as_f64().unwrap_or(0.0);
        let mut ordem: Vec<usize> = (0..nos.len()).collect();
        ordem.sort_by(|&a, &b| {
            let (ga, gb) = (grau.get(&nos[a].0).copied().unwrap_or(0), grau.get(&nos[b].0).copied().unwrap_or(0));
            gb.cmp(&ga).then(ts(&nos[b].1).partial_cmp(&ts(&nos[a].1)).unwrap_or(std::cmp::Ordering::Equal))
        });
        let manter: HashSet<usize> = ordem.into_iter().take(lim).collect();
        nos = nos.into_iter().enumerate().filter(|(i, _)| manter.contains(i)).map(|(_, n)| n).collect();
        let ids: HashSet<&str> = nos.iter().map(|n| n.0.as_str()).collect();
        ligacoes.retain(|lk| ids.contains(lk["source"].as_str().unwrap_or("")) && ids.contains(lk["target"].as_str().unwrap_or("")));
    }
    Ok(json!({"nodes": nos.into_iter().map(|n| n.1).collect::<Vec<_>>(), "links": ligacoes, "truncado": truncado}))
}

fn mensagens_por_id(memory: &Store, ids: &HashSet<i64>) -> anyhow::Result<HashMap<i64, Map<String, Value>>> {
    let mut achado = HashMap::new();
    let mut lista: Vec<i64> = ids.iter().copied().collect();
    lista.sort_unstable();
    for lote in lista.chunks(500) {
        let marcas = vec!["?"; lote.len()].join(",");
        let params: Vec<&dyn rusqlite_dyn::ToSql> = lote.iter().map(|i| i as &dyn rusqlite_dyn::ToSql).collect();
        for r in memory.query(&format!("SELECT id, role, text, created_at FROM messages WHERE id IN ({marcas})"), &params)? {
            let label: String = s(&r, "text").split_whitespace().collect::<Vec<_>>().join(" ").chars().take(120).collect();
            let mut m = Map::new();
            m.insert("label".into(), json!(label));
            m.insert("ator".into(), json!(if s(&r, "role") == "user" { "Antônio" } else { "Orion" }));
            m.insert("ts".into(), json!(f(&r, "created_at") * 1000.0));
            achado.insert(r["id"].as_i64().unwrap_or(0), m);
        }
    }
    Ok(achado)
}
use orion_store::rusqlite as rusqlite_dyn;

/// Um fato que cita o nome de um tópico ou entidade fica ligado a ele (até 3 por fato).
fn ligar_fatos_a_coisas(nos: &[(String, Value)], ligacoes: &mut Vec<Value>) {
    let coisas: Vec<(&str, String)> = nos
        .iter()
        .filter(|(_, n)| n["tipo"] == "topico")
        .map(|(i, n)| (i.as_str(), plano(n["label"].as_str().unwrap_or(""))))
        .filter(|(_, nome)| nome.chars().count() >= 3)
        .take(400)
        .collect();
    if coisas.is_empty() {
        return;
    }
    let regs: Vec<Option<Regex>> = coisas.iter().map(|(_, nome)| Regex::new(&format!(r"\b{}\b", regex::escape(nome))).ok()).collect();
    for (i, n) in nos {
        if n["tipo"] != "fato" {
            continue;
        }
        let texto = plano(n["label"].as_str().unwrap_or(""));
        let achados = coisas.iter().zip(&regs).filter(|(_, r)| r.as_ref().is_some_and(|r| r.is_match(&texto))).take(3);
        for ((cid, _), _) in achados {
            ligacoes.push(json!({"source": i, "target": cid, "rel": "cita"}));
        }
    }
}

/// O nó que melhor casa com o que o Antônio disse: nome igual, começo do nome, trecho do nome,
/// e por fim a maior sobreposição de palavras. Sem acento nem caixa.
pub fn achar_no(nos: &[Value], texto: &str) -> Option<Value> {
    let q = plano(texto);
    if q.is_empty() {
        return None;
    }
    let palavras: HashSet<&str> = q.split(' ').filter(|p| p.chars().count() > 2).collect();
    let mut melhor: Option<(f64, &Value)> = None;
    for n in nos {
        let nome = plano(n.get("label").and_then(Value::as_str).filter(|l| !l.is_empty()).or(n["id"].as_str()).unwrap_or(""));
        let nota = if nome == q {
            4.0
        } else if nome.starts_with(&q) {
            3.0
        } else if nome.contains(&q) {
            2.0
        } else {
            let comuns = nome.split(' ').filter(|p| palavras.contains(p)).collect::<HashSet<_>>().len();
            if !palavras.is_empty() && comuns > 0 { comuns as f64 / palavras.len() as f64 } else { 0.0 }
        };
        if nota > 0.0 && melhor.is_none_or(|(m, _)| nota > m) {
            melhor = Some((nota, n));
        }
    }
    melhor.filter(|(m, _)| *m >= 0.5).map(|(_, n)| n.clone())
}

/// Menor caminho (em número de ligações, sem direção) entre dois nós; vazio se não se conectam.
pub fn caminho(grafo: &Value, de: &str, para: &str) -> Vec<String> {
    if de == para {
        return vec![de.into()];
    }
    let mut viz: HashMap<&str, Vec<&str>> = HashMap::new();
    for lk in grafo["links"].as_array().into_iter().flatten() {
        let (a, b) = (lk["source"].as_str().unwrap_or(""), lk["target"].as_str().unwrap_or(""));
        viz.entry(a).or_default().push(b);
        viz.entry(b).or_default().push(a);
    }
    let mut anterior: HashMap<&str, Option<&str>> = HashMap::from([(de, None)]);
    let mut fila = VecDeque::from([de]);
    while let Some(atual) = fila.pop_front() {
        for &v in viz.get(atual).into_iter().flatten() {
            if anterior.contains_key(v) {
                continue;
            }
            anterior.insert(v, Some(atual));
            if v == para {
                let mut rota = vec![v.to_string()];
                let mut cur = v;
                while let Some(Some(p)) = anterior.get(cur) {
                    rota.push(p.to_string());
                    cur = p;
                }
                rota.reverse();
                return rota;
            }
            fila.push_back(v);
        }
    }
    vec![]
}

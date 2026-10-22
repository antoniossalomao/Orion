//! Avaliação da busca da memória (E4.5): conjunto fixo de fatos, documentos e conversas com
//! perguntas e respostas esperadas. Mede recall@k e MRR em menos de um segundo, sem rede.

use orion_store::search::Kind;
use orion_store::Store;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct Caso {
    pub pergunta: &'static str,
    pub esperado: &'static str,
    pub tipo: Option<Kind>,
}

pub const FATOS: &[(&str, &str)] = &[
    ("O Antônio mora em Belo Horizonte e estuda Ciência de Dados.", "perfil"),
    ("O cachorro do Antônio chama Thor e toma antipulgas todo dia 10.", "casa"),
    ("A chave reserva do apartamento fica com a dona Helena no 302.", "casa"),
    ("O servidor de casa roda Debian 12 no IP 192.168.1.40.", "infra"),
    ("O médico cardiologista é o Dr. Marcos Viana, consulta em março.", "saude"),
    ("A bicicleta Caloi foi comprada em outubro por R$ 2.400.", "compras"),
    ("O projeto Orion usa SQLite com FTS5 para busca híbrida.", "orion"),
    ("A senha do Wi-Fi de visitantes muda todo primeiro domingo do mês.", "casa"),
];

pub const DOCUMENTOS: &[(&str, &str)] = &[
    (
        "notas/estudos-estatistica.md",
        "# Regressão linear\nO estimador de mínimos quadrados ordinários minimiza a soma dos resíduos ao quadrado.\nQuando há multicolinearidade forte entre variáveis explicativas, a variância dos coeficientes cresce e o Ridge (penalização L2) estabiliza a estimação.",
    ),
    (
        "notas/receita-pao.md",
        "# Pão de fermentação natural\nMisture 500 g de farinha de trigo, 350 ml de água morna, 100 g de levain ativo e 10 g de sal. Dobre a massa a cada 40 minutos nas primeiras duas horas e asse em panela de ferro a 240 graus.",
    ),
    (
        "notas/viagem-ouro-preto.md",
        "# Roteiro Ouro Preto\nSábado: Museu da Inconfidência pela manhã, almoço no Bené da Flauta e Igreja de São Francisco de Assis à tarde. Domingo: Mina da Passagem em Mariana.",
    ),
];

pub const CONVERSAS: &[(&str, &str)] = &[
    ("user", "Preciso renovar o seguro do carro antes do dia 22 de agosto, a corretora é a Porto Azul."),
    ("assistant", "Anotei: seguro do carro vence em 22 de agosto, corretora Porto Azul."),
    ("user", "Na reunião de sexta ficou decidido adiar a migração do banco para depois do feriado."),
];

pub const CASOS_PADRAO: &[Caso] = &[
    Caso { pergunta: "qual o nome do cachorro", esperado: "Thor", tipo: Some(Kind::Fact) },
    Caso { pergunta: "quem tem a chave reserva do apartamento", esperado: "Helena", tipo: Some(Kind::Fact) },
    Caso { pergunta: "qual o IP do servidor Debian", esperado: "192.168.1.40", tipo: Some(Kind::Fact) },
    Caso { pergunta: "nome do cardiologista", esperado: "Marcos Viana", tipo: Some(Kind::Fact) },
    Caso { pergunta: "penalização L2 multicolinearidade", esperado: "Ridge", tipo: Some(Kind::Chunk) },
    Caso { pergunta: "quantos gramas de levain no pão", esperado: "100 g de levain", tipo: Some(Kind::Chunk) },
    Caso { pergunta: "onde almoçar em Ouro Preto", esperado: "Bené da Flauta", tipo: Some(Kind::Chunk) },
    Caso { pergunta: "qual corretora do seguro do carro", esperado: "Porto Azul", tipo: Some(Kind::Message) },
    Caso { pergunta: "quando ficou a migração do banco", esperado: "depois do feriado", tipo: Some(Kind::Message) },
    Caso { pergunta: "banco de dados da busca do Orion", esperado: "FTS5", tipo: Some(Kind::Fact) },
];

pub fn popular(store: &Store) -> orion_store::Result<()> {
    for &(texto, fonte) in FATOS {
        store.add_fact(texto, fonte, None)?;
    }
    for &(fonte, corpo) in DOCUMENTOS {
        store.index_document(fonte, fonte, corpo)?;
    }
    let s = store.new_session("eval", Some("conversa de avaliação"), None)?;
    for &(papel, fala) in CONVERSAS {
        store.add_message(&s.id, papel, fala, None, None)?;
    }
    Ok(())
}

/// Roda os casos num `Store` (cria um em memória e popula se não for passado).
pub fn avaliar(store: Option<&Store>, casos: Option<&[Caso]>, k: usize) -> Value {
    let proprio;
    let st = match store {
        Some(s) => s,
        None => {
            proprio = Store::open(":memory:").expect("banco em memória");
            popular(&proprio).expect("popular avaliação");
            &proprio
        }
    };
    let lista = casos.unwrap_or(CASOS_PADRAO);
    let mut acertos = 0usize;
    let mut soma_rr = 0.0f64;
    let mut falhas = vec![];
    for c in lista {
        let hits = st.search(c.pergunta, k, &Kind::ALL).unwrap_or_default();
        let alvo = c.esperado.to_lowercase();
        let mut posicao = None;
        for (idx, h) in hits.iter().enumerate() {
            if h.text.to_lowercase().contains(&alvo) && (c.tipo.is_none() || Some(h.kind) == c.tipo) {
                posicao = Some(idx + 1);
                break;
            }
        }
        if let Some(pos) = posicao {
            acertos += 1;
            soma_rr += 1.0 / (pos as f64);
        } else {
            falhas.push(json!({
                "pergunta": c.pergunta,
                "esperado": c.esperado,
                "topo": hits.iter().take(3).map(|h| h.text.chars().take(80).collect::<String>()).collect::<Vec<_>>(),
            }));
        }
    }
    let total = lista.len().max(1) as f64;
    json!({
        "casos": lista.len(),
        "k": k,
        "acertos": acertos,
        "recall_at_k": ((acertos as f64 / total) * 10000.0).round() / 10000.0,
        "mrr": ((soma_rr / total) * 10000.0).round() / 10000.0,
        "falhas": falhas,
    })
}

pub fn resumo(rel: &Value) -> String {
    let r = rel["recall_at_k"].as_f64().unwrap_or(0.0) * 100.0;
    let mrr = rel["mrr"].as_f64().unwrap_or(0.0);
    let mut linhas = vec![format!(
        "busca: {}/{} acertos (recall@{}={r:.0}%, MRR={mrr:.2})",
        rel["acertos"].as_u64().unwrap_or(0),
        rel["casos"].as_u64().unwrap_or(0),
        rel["k"].as_u64().unwrap_or(5)
    )];
    for f in rel["falhas"].as_array().into_iter().flatten() {
        linhas.push(format!("  [falhou] {:?} -> esperava {:?}", f["pergunta"].as_str().unwrap_or(""), f["esperado"].as_str().unwrap_or("")));
    }
    linhas.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avaliacao_padrao_alcanca_recall_alto() {
        let rel = avaliar(None, None, 5);
        assert!(rel["recall_at_k"].as_f64().unwrap() >= 0.9);
        assert!(resumo(&rel).contains("recall@5="));
    }
}

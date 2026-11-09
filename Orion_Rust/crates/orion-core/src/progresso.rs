//! F04.20: progresso do turno salvo a cada passo, para retomar depois de uma queda.
//!
//! Cada turno (de conversa; os de só leitura em segundo plano ficam de fora) guarda o pedido e os passos de
//! ferramenta que já rodaram numa chave do `meta` (`valor:turno:<sessão>`). Terminando o turno, a chave
//! some. Se o PC cair ou o Orion fechar no meio, ela fica: na próxima abertura um aviso conta que a tarefa
//! foi interrompida, e `/retomar` na conversa continua. A retomada nunca repete sozinha: os passos já feitos
//! entram como **conteúdo externo** (dado, nunca instrução), o que também contamina a sessão, e por isso
//! qualquer escrita ou execução volta a pedir a sua confirmação.

use crate::extensions::context::ExternalData;
use orion_store::Store;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const PREFIXO: &str = "turno:";
pub const COMANDO: &str = "/retomar";
const MAX_PASSOS: usize = 30;
const MAX_ARGS: usize = 300;
const VALIDADE_S: f64 = 24.0 * 3600.0;

#[derive(Debug, Clone)]
pub struct Interrompido {
    pub sessao: String,
    pub pedido: String,
    pub passos: Vec<Value>,
    pub iniciado: f64,
    pub anunciado: bool,
}

fn chave(sessao: &str) -> String {
    format!("{PREFIXO}{sessao}")
}

fn gravar(m: &Store, sessao: &str, pedido: &str, iniciado: f64, passos: &[Value], anunciado: bool) {
    let v = json!({"pedido": pedido, "iniciado": iniciado, "passos": passos, "anunciado": anunciado});
    let _ = m.meta_set(&chave(sessao), Some(&v.to_string()));
}

fn ler(m: &Store, sessao: &str) -> Option<Interrompido> {
    let bruto = m.meta_get(&chave(sessao))?;
    let v: Value = serde_json::from_str(&bruto).ok()?;
    Some(Interrompido {
        sessao: sessao.to_string(),
        pedido: v["pedido"].as_str()?.to_string(),
        passos: v["passos"].as_array().cloned().unwrap_or_default(),
        iniciado: v["iniciado"].as_f64().unwrap_or(0.0),
        anunciado: v["anunciado"].as_bool().unwrap_or(false),
    })
}

/// Começa o registro do turno. `anteriores` são os passos de uma retomada (o registro continua acumulando).
pub fn iniciar(m: &Store, sessao: &str, pedido: &str, anteriores: &[Value]) {
    let passos: Vec<Value> = anteriores.iter().rev().take(MAX_PASSOS).rev().cloned().collect();
    gravar(m, sessao, pedido, m.now(), &passos, false);
}

/// Acrescenta um passo de ferramenta. Sem turno registrado (turno de só leitura, por exemplo), não faz nada.
pub fn passo(m: &Store, sessao: &str, ferramenta: &str, args: &Map<String, Value>, estado: &str) {
    let Some(i) = ler(m, sessao) else { return };
    let mut passos = i.passos;
    let resumo: String = orion_policy::redact_pii(&Value::Object(args.clone()), MAX_ARGS).to_string().chars().take(MAX_ARGS).collect();
    passos.push(json!({"ferramenta": ferramenta, "estado": estado, "args": resumo}));
    if passos.len() > MAX_PASSOS {
        passos.remove(0);
    }
    gravar(m, sessao, &i.pedido, i.iniciado, &passos, false);
}

/// O turno terminou de forma normal: nada a retomar.
pub fn fechar(m: &Store, sessao: &str) {
    let _ = m.meta_set(&chave(sessao), None);
}

pub fn obter(m: &Store, sessao: &str) -> Option<Interrompido> {
    ler(m, sessao).filter(|i| m.now() - i.iniciado <= VALIDADE_S)
}

/// Na abertura: os turnos que ficaram pelo caminho e ainda não foram anunciados (marca como anunciados).
/// Registros com mais de 24 h são apagados.
pub fn anunciar(m: &Store) -> Vec<Interrompido> {
    let Ok(linhas) = m.query("SELECT key FROM meta WHERE key LIKE 'valor:turno:%'", &[]) else { return vec![] };
    let mut novos = vec![];
    for l in linhas {
        let Some(sessao) = l.get("key").and_then(Value::as_str).and_then(|k| k.strip_prefix(&format!("valor:{PREFIXO}"))) else { continue };
        let Some(i) = ler(m, sessao) else { continue };
        if m.now() - i.iniciado > VALIDADE_S {
            fechar(m, sessao);
            continue;
        }
        if !i.anunciado {
            gravar(m, sessao, &i.pedido, i.iniciado, &i.passos, true);
            novos.push(i);
        }
    }
    novos
}

/// Os passos já feitos, como dado externo para o turno de retomada.
pub fn dados(i: &Interrompido) -> ExternalData {
    let texto = serde_json::to_string_pretty(&json!({"passos_ja_feitos": i.passos})).unwrap_or_default();
    ExternalData {
        connection: "orion".into(),
        kind: "passos_anteriores".into(),
        key: i.sessao.clone(),
        digest: hex::encode(Sha256::digest(texto.as_bytes())),
        text: texto,
        truncated: false,
    }
}

/// O pedido de retomada. O pedido original é o que você mesmo escreveu antes; os passos vão à parte, como dado.
pub fn texto_de_retomada(i: &Interrompido) -> String {
    format!(
        "Retome a tarefa que foi interrompida (o PC caiu ou o Orion fechou). Pedido original: «{}». Os passos que já rodaram estão no \
         conteúdo externo «passos_anteriores» (ferramenta, estado e argumentos resumidos). Não repita passo de escrita ou execução que \
         consta como concluído; continue de onde parou e termine o pedido.",
        i.pedido
    )
}

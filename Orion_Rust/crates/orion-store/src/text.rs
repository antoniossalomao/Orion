//! Utilitários de texto da memória: consulta FTS5 e divisão em trechos.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

const STOPWORDS: &str = "a o as os um uma uns umas de do da dos das em no na nos nas por para com \
sem sobre entre e ou mas que se como qual quais quem onde quando é ser foi são era eu me meu minha \
meus minhas você seu sua tu te ele ela isso isto esse essa este esta já não sim mais muito tem \
tinha ter há foi vai vou sei fala falou disse";

static STOP: LazyLock<HashSet<&'static str>> = LazyLock::new(|| STOPWORDS.split_whitespace().collect());
static PALAVRA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\w+").unwrap());
static FRONTMATTER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)\A---[ \t]*\n.*?\n---[ \t]*\n").unwrap());
static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\[\]|#^\n]{1,120})(?:[#^|][^\[\]\n]*)?\]\]").unwrap());
static PARAGRAFOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n\s*\n").unwrap());

/// Consulta FTS5 segura: termos entre aspas, ligados por OR.
///
/// Sem stemmer em português, termos longos viram prefixo (`trabalho` → `trabal*`) para casar
/// flexões. `None` se não sobrar termo.
pub fn fts_query(texto: &str) -> Option<String> {
    let termos: Vec<String> = PALAVRA.find_iter(texto).map(|m| m.as_str().to_lowercase()).collect();
    let uteis: Vec<&String> = termos
        .iter()
        .filter(|t| !STOP.contains(t.as_str()) && (t.chars().count() > 1 || t.chars().all(|c| c.is_numeric())))
        .collect();
    let base: Vec<&String> = if uteis.is_empty() { termos.iter().collect() } else { uteis };
    let mut vistos = HashSet::new();
    let saida: Vec<String> = base
        .into_iter()
        .filter(|t| vistos.insert(t.as_str()))
        .map(|t| {
            let n = t.chars().count();
            if n >= 6 {
                let corte: String = t.chars().take((n - 3).max(4)).collect();
                format!("\"{corte}\"*")
            } else {
                format!("\"{t}\"")
            }
        })
        .collect();
    if saida.is_empty() { None } else { Some(saida.join(" OR ")) }
}

/// Nomes citados em `[[Nota]]`, `[[Nota|apelido]]`, `[[Nota#seção]]` e `[[pasta/Nota]]`.
pub fn wikilinks(markdown: &str) -> HashSet<String> {
    WIKILINK
        .captures_iter(markdown)
        .map(|c| c[1].trim().rsplit('/').next().unwrap_or("").trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

pub fn strip_frontmatter(markdown: &str) -> String {
    FRONTMATTER.replace(markdown, "").into_owned()
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn char_slice(s: &str, ini: usize, fim: usize) -> String {
    s.chars().skip(ini).take(fim.saturating_sub(ini)).collect()
}

/// Agrupa parágrafos em trechos de até `tamanho` caracteres, com `sobreposicao` caracteres
/// (em limite de palavra) entre eles.
pub fn chunk_text(texto: &str, tamanho: usize, sobreposicao: usize) -> Vec<String> {
    let paragrafos: Vec<String> =
        PARAGRAFOS.split(texto).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
    let mut pedacos: Vec<String> = Vec::new();
    let mut atual = String::new();
    for mut p in paragrafos {
        while char_len(&p) > tamanho {
            // parágrafo gigante: corta em limite de palavra
            let prefixo = char_slice(&p, 0, tamanho);
            let corte = prefixo.rfind(' ').map(|b| prefixo[..b].chars().count());
            let corte = match corte {
                Some(c) if c > tamanho / 2 => c,
                _ => tamanho,
            };
            if !atual.is_empty() {
                pedacos.push(std::mem::take(&mut atual));
            }
            pedacos.push(char_slice(&p, 0, corte).trim().to_string());
            p = char_slice(&p, corte, char_len(&p)).trim().to_string();
        }
        if !atual.is_empty() && char_len(&atual) + char_len(&p) + 2 > tamanho {
            pedacos.push(std::mem::take(&mut atual));
        }
        atual = if atual.is_empty() { p } else { format!("{atual}\n\n{p}") };
    }
    if !atual.is_empty() {
        pedacos.push(atual);
    }
    if sobreposicao == 0 || pedacos.len() < 2 {
        return pedacos;
    }
    let mut saida = vec![pedacos[0].clone()];
    for par in pedacos.windows(2) {
        let n = char_len(&par[0]);
        let cauda = char_slice(&par[0], n.saturating_sub(sobreposicao), n);
        let cauda = match cauda.find(' ') {
            Some(i) => cauda[i + 1..].to_string(),
            None => cauda,
        };
        saida.push(format!("{cauda} {}", par[1]));
    }
    saida
}

/// `re.sub(r"\s+", " ", t).strip()`.
pub fn colapsar_espacos(t: &str) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Escapa `%`, `_` e `\` para `LIKE ... ESCAPE '\'`.
pub fn like(texto: &str) -> String {
    texto.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

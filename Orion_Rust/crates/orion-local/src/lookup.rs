//! Rascunho por repetição de trecho (L1): a resposta local muitas vezes repete palavras do pedido
//! ou do que ela mesma já escreveu. Este módulo só propõe o rascunho; quem confere é o modelo, que
//! aceita apenas o que ele mesmo escolheria (amostragem gulosa), então o texto final é o mesmo
//! que sem rascunho. Sem rede, sem ferramenta, sem estado (REGRAS.md: modelo local é reserva sem
//! ferramentas).

/// Tamanho do trecho final que precisa reaparecer para haver rascunho.
pub const NGRAM: usize = 3;
/// Máximo de tokens propostos por passo; também o teto aceito em `Request`.
pub const DRAFT_MAX: u32 = 16;

/// Procura a ocorrência anterior mais recente dos últimos `NGRAM` tokens de `history` e devolve
/// até `max` tokens que vieram depois dela. Vazio se não houver trecho repetido.
pub fn draft(history: &[i32], max: usize) -> Vec<i32> {
    let n = history.len();
    if max == 0 || n <= NGRAM {
        return Vec::new();
    }
    let suffix = &history[n - NGRAM..];
    // a ocorrência tem de terminar antes do sufixo: começa em [0, n - NGRAM - 1]
    for start in (0..n - NGRAM).rev() {
        if &history[start..start + NGRAM] == suffix {
            let from = start + NGRAM;
            let to = (from + max).min(n);
            return history[from..to].to_vec();
        }
    }
    Vec::new()
}

//! Rascunho por repetição de trecho (L1): a resposta local muitas vezes repete palavras do pedido
//! ou do que ela mesma já escreveu. Este módulo só propõe o rascunho; quem confere é o modelo, que
//! aceita apenas o que ele mesmo escolheria (amostragem gulosa), então o texto final é o mesmo
//! que sem rascunho. Sem rede, sem ferramenta, sem estado (REGRAS.md: modelo local é reserva sem
//! ferramentas).

/// Tamanho do trecho final que precisa reaparecer para haver rascunho.
pub const NGRAMA: usize = 3;
/// Máximo de tokens propostos por passo; também o teto aceito em `Request`.
pub const RASCUNHO_MAX: u32 = 16;

/// Procura a ocorrência anterior mais recente dos últimos `NGRAMA` tokens de `historico` e devolve
/// até `max` tokens que vieram depois dela. Vazio se não houver trecho repetido.
pub fn rascunho(historico: &[i32], max: usize) -> Vec<i32> {
    let n = historico.len();
    if max == 0 || n <= NGRAMA {
        return Vec::new();
    }
    let sufixo = &historico[n - NGRAMA..];
    // a ocorrência tem de terminar antes do sufixo: começa em [0, n - NGRAMA - 1]
    for inicio in (0..n - NGRAMA).rev() {
        if &historico[inicio..inicio + NGRAMA] == sufixo {
            let de = inicio + NGRAMA;
            let ate = (de + max).min(n);
            return historico[de..ate].to_vec();
        }
    }
    Vec::new()
}

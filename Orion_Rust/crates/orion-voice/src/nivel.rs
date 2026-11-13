//! Nível do som que o Orion está falando, em faixas de frequência, para a esfera reagir à voz.
//! Calculado fora do callback de áudio (que não pode alocar nem esperar): uma thread leve lê a
//! posição tocada e mede a janela atual. Só o número sai daqui; o áudio nunca é guardado.
use std::sync::OnceLock;

pub const FAIXAS: usize = 8;
/// Centros das faixas em Hz (escala aproximadamente logarítmica até perto de 8 kHz).
const CENTROS: [f32; FAIXAS] = [140.0, 260.0, 450.0, 750.0, 1200.0, 1900.0, 3000.0, 4600.0];
pub(crate) const JANELA: usize = 512;

static SAIDA: OnceLock<fn(Option<[f32; FAIXAS]>)> = OnceLock::new();

/// Registra quem recebe o nível (o servidor liga isto à esfera do app). `None` = parou de falar.
pub fn ao_nivel(f: fn(Option<[f32; FAIXAS]>)) {
    let _ = SAIDA.set(f);
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn publicar(v: Option<[f32; FAIXAS]>) {
    if let Some(f) = SAIDA.get() {
        f(v);
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn tem_saida() -> bool {
    SAIDA.get().is_some()
}

/// Energia de cada faixa na janela que termina em `fim` (Goertzel: barato e sem FFT).
pub fn medir(amostras: &[f32], fim: usize, taxa: u32) -> [f32; FAIXAS] {
    let fim = fim.min(amostras.len());
    let ini = fim.saturating_sub(JANELA);
    let janela = &amostras[ini..fim];
    let mut out = [0.0f32; FAIXAS];
    if janela.len() < 64 {
        return out;
    }
    let n = janela.len() as f32;
    for (k, f) in CENTROS.iter().enumerate() {
        let w = 2.0 * std::f32::consts::PI * f / taxa as f32;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for (i, x) in janela.iter().enumerate() {
            // janela de Hann para não vazar energia entre faixas
            let h = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1.0)).cos();
            let s0 = x * h + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let pot = (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0);
        out[k] = (pot.sqrt() / (n * 0.25)).min(4.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tom_puro_acende_a_faixa_certa() {
        let taxa = 16_000;
        let a: Vec<f32> = (0..2048).map(|i| (2.0 * std::f32::consts::PI * 750.0 * i as f32 / taxa as f32).sin() * 0.5).collect();
        let m = medir(&a, 2048, taxa);
        let maior = m.iter().enumerate().max_by(|x, y| x.1.total_cmp(y.1)).unwrap().0;
        assert_eq!(maior, 3);
        assert!(medir(&vec![0.0; 2048], 2048, taxa).iter().all(|v| *v < 1e-6));
    }
}

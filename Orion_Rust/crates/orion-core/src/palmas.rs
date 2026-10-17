//! Duas palmas como atalho (E6.3; REGRAS.md: palmas só abrem; nunca aprovam nem executam).
//!
//! Roda no mesmo fluxo de quadros PCM (16 kHz, 16-bit mono, 32 ms) da wake word — sem segundo
//! microfone, sem modelo, sem rede.
//!
//! - uma palma é um pico alto e curto: silêncio antes (2 quadros), subida acima de `limiar` por no
//!   máximo 5 quadros (~160 ms) e queda rápida abaixo de `limiar * 0.25`. Fala contínua sustenta
//!   energia por mais de 160 ms e é descartada;
//! - duas palmas separadas por 150 ms a 700 ms disparam e entram em surdez por 2 s;
//! - o que as duas palmas podem fazer: acordar o Orion (`on_clap`), sempre em escuta — nunca aprovar
//!   ferramenta, nunca executar rotina, nunca pular política. Em modo pânico ou não perturbe, as
//!   palmas são ignoradas.

use crate::modos::Modos;

pub const PICO_MINIMO: i32 = 4500;
pub const QUEDA: f64 = 0.25;
pub const ENTRE_MIN_S: f64 = 0.15;
pub const ENTRE_MAX_S: f64 = 0.7;
pub const SURDO_S: f64 = 2.0;
pub const SILENCIO_ANTES: usize = 2;
pub const SILENCIO_DEPOIS: usize = 2;
pub const LONGO: usize = 6;

/// Maior `|amostra|` do quadro int16 little-endian.
pub fn pico(quadro: &[u8]) -> i32 {
    if quadro.len() < 2 {
        return 0;
    }
    quadro.chunks_exact(2).map(|b| (i16::from_le_bytes([b[0], b[1]]) as i32).abs()).max().unwrap_or(0)
}

pub struct ClapDetector {
    pub limiar: i32,
    chao: i32,
    quietos: usize,
    altos: usize,
    pos_quietos: usize,
    primeira_em: Option<f64>,
    surdo_ate: f64,
}

impl ClapDetector {
    pub fn new(limiar: i32) -> Self {
        let lim = limiar.max(1500);
        Self { limiar: lim, chao: (lim as f64 * QUEDA) as i32, quietos: 0, altos: 0, pos_quietos: 0, primeira_em: None, surdo_ate: f64::NEG_INFINITY }
    }

    /// REGRAS.md: palmas só abrem; nunca aprovam nem executam. Calibra no percentil 95 do ruído.
    pub fn calibrate(&mut self, noise_peaks: &mut [i32], ratio: f64) {
        if noise_peaks.is_empty() {
            return;
        }
        noise_peaks.sort_unstable();
        let noise = noise_peaks[(noise_peaks.len() - 1) * 95 / 100];
        self.limiar = ((noise as f64 * ratio).round() as i32).clamp(1500, 30000);
        self.chao = (self.limiar as f64 * QUEDA) as i32;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.quietos = 0;
        self.altos = 0;
        self.pos_quietos = 0;
        self.primeira_em = None;
    }

    /// Alimenta um quadro de 32 ms; `true` quando fechou duas palmas.
    pub fn feed(&mut self, quadro: &[u8], agora_s: f64) -> bool {
        if agora_s < self.surdo_ate {
            return false;
        }
        if let Some(p1) = self.primeira_em {
            if agora_s - p1 > ENTRE_MAX_S {
                self.primeira_em = None;
            }
        }
        let p = pico(quadro);
        if self.altos == 0 {
            if p >= self.limiar && self.quietos >= SILENCIO_ANTES {
                self.altos = 1;
                self.pos_quietos = 0;
            } else if p <= self.chao {
                self.quietos += 1;
            } else {
                self.quietos = 0;
            }
            return false;
        }
        if p >= self.limiar {
            self.altos += 1;
            self.pos_quietos = 0;
            if self.altos >= LONGO {
                // som sustentado (voz, música, batida longa): não é palma
                self.altos = 0;
                self.quietos = 0;
                self.primeira_em = None;
            }
            return false;
        }
        if p <= self.chao {
            self.pos_quietos += 1;
            if self.pos_quietos >= SILENCIO_DEPOIS {
                self.altos = 0;
                self.quietos = self.pos_quietos;
                return self.palma_confirmada(agora_s);
            }
            return false;
        }
        // caiu, mas não até o chão: espera mais um quadro
        self.altos += 1;
        if self.altos >= LONGO {
            self.altos = 0;
            self.quietos = 0;
        }
        false
    }

    fn palma_confirmada(&mut self, agora_s: f64) -> bool {
        let Some(p1) = self.primeira_em else {
            self.primeira_em = Some(agora_s);
            return false;
        };
        let dt = agora_s - p1;
        if (ENTRE_MIN_S..=ENTRE_MAX_S).contains(&dt) {
            self.primeira_em = None;
            self.surdo_ate = agora_s + SURDO_S;
            return true;
        }
        self.primeira_em = Some(agora_s);
        false
    }
}

/// Decide o que duas palmas fazem agora: `'panico'`, `'nao_perturbe'` ou `'abrir'`.
pub fn acao_das_palmas(modos: Option<&Modos>) -> &'static str {
    if let Some(m) = modos {
        if m.panico() {
            return "panico";
        }
        if m.nao_perturbe() {
            return "nao_perturbe";
        }
    }
    "abrir"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quadro(amplitude: i16) -> Vec<u8> {
        let mut b = Vec::with_capacity(512 * 2);
        for _ in 0..512 {
            b.extend_from_slice(&amplitude.to_le_bytes());
        }
        b
    }

    fn alimentar(det: &mut ClapDetector, seq: &[(i16, usize)], t0: f64) -> (Vec<f64>, f64) {
        let mut t = t0;
        let mut disparos = vec![];
        for &(amp, n) in seq {
            let q = quadro(amp);
            for _ in 0..n {
                if det.feed(&q, t) {
                    disparos.push((t * 1000.0).round() / 1000.0);
                }
                t += 0.032;
            }
        }
        (disparos, t)
    }

    #[test]
    fn duas_palmas_disparam_uma_vez() {
        let mut d = ClapDetector::new(5000);
        let (disparos, _) = alimentar(&mut d, &[(0, 4), (9000, 2), (0, 6), (9000, 2), (0, 4)], 0.0);
        assert_eq!(disparos.len(), 1);
    }

    #[test]
    fn uma_palma_sozinha_nao_dispara() {
        let mut d = ClapDetector::new(5000);
        let (disparos, _) = alimentar(&mut d, &[(0, 4), (9000, 2), (0, 30)], 0.0);
        assert!(disparos.is_empty());
    }

    #[test]
    fn duas_palmas_lentas_demais_nao_disparam() {
        let mut d = ClapDetector::new(5000);
        let (disparos, _) = alimentar(&mut d, &[(0, 4), (9000, 2), (0, 30), (9000, 2), (0, 4)], 0.0);
        assert!(disparos.is_empty());
    }

    #[test]
    fn som_sustentado_de_fala_nao_e_palma() {
        let mut d = ClapDetector::new(5000);
        let (disparos, _) = alimentar(&mut d, &[(0, 4), (9000, 10), (0, 5), (9000, 10), (0, 4)], 0.0);
        assert!(disparos.is_empty());
    }

    #[test]
    fn ruido_baixo_nao_dispara() {
        let mut d = ClapDetector::new(5000);
        let (disparos, _) = alimentar(&mut d, &[(0, 4), (2000, 2), (0, 6), (2000, 2), (0, 4)], 0.0);
        assert!(disparos.is_empty());
    }

    #[test]
    fn surdez_pos_disparo_evita_repeticao_imediata() {
        let mut d = ClapDetector::new(5000);
        let (d1, t) = alimentar(&mut d, &[(0, 4), (9000, 2), (0, 6), (9000, 2), (0, 4)], 0.0);
        let (d2, _) = alimentar(&mut d, &[(0, 4), (9000, 2), (0, 6), (9000, 2), (0, 4)], t);
        let (d3, _) = alimentar(&mut d, &[(0, 4), (9000, 2), (0, 6), (9000, 2), (0, 4)], t + 2.5);
        assert_eq!(d1.len(), 1);
        assert!(d2.is_empty());
        assert_eq!(d3.len(), 1);
    }
}

//! Reconhecimento facial clássico local baseado em Haar Cascades e LBPH (Local Binary Pattern Histogram)
//! (E11.2; F18.05; REGRAS.md: rosto é biometria: fica cifrado, nunca sai do computador e nunca governa ação).
//!
//! Algoritmo 100% nativo em Rust, sem dependências de rede, sem modelos neurais pesados e sem ONNX.
//! 1. Detecção facial por padrões Haar e imagem integral (Viola-Jones clássico).
//! 2. Equalização de histograma para invariância a iluminação.
//! 3. Extração de padrões binários locais (LBPH) em grade espacial 8x8 gerando vetor de 512 floats normalizado.

pub struct ImagemIntegral {
    pub w: usize,
    pub h: usize,
    pub sat: Vec<u64>,
}

impl ImagemIntegral {
    pub fn new(w: usize, h: usize, pixels: &[u8]) -> Self {
        let stride = w + 1;
        let mut sat = vec![0u64; stride * (h + 1)];
        for y in 0..h {
            let mut soma_linha = 0u64;
            for x in 0..w {
                soma_linha += pixels[y * w + x] as u64;
                let cima = sat[y * stride + (x + 1)];
                sat[(y + 1) * stride + (x + 1)] = cima + soma_linha;
            }
        }
        Self { w, h, sat }
    }

    #[inline]
    pub fn soma(&self, x: usize, y: usize, w: usize, h: usize) -> u64 {
        if w == 0 || h == 0 || x + w > self.w || y + h > self.h {
            return 0;
        }
        let stride = self.w + 1;
        let a = self.sat[y * stride + x];
        let b = self.sat[y * stride + (x + w)];
        let c = self.sat[(y + h) * stride + x];
        let d = self.sat[(y + h) * stride + (x + w)];
        d + a - b - c
    }

    #[inline]
    pub fn media(&self, x: usize, y: usize, w: usize, h: usize) -> f64 {
        let area = (w * h) as f64;
        if area <= 0.0 {
            0.0
        } else {
            self.soma(x, y, w, h) as f64 / area
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetanguloFace {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

/// Detecta a região frontal de rosto mais proeminente através de recursos Haar clássicos.
/// Avalia contraste de olhos, ponte nasal, bochechas e simetria facial.
pub fn detectar_face_haar(w: usize, h: usize, sat: &ImagemIntegral) -> Option<RetanguloFace> {
    if w < 24 || h < 24 {
        return None;
    }

    let min_dim = w.min(h);
    let mut melhor_score = 0.0f64;
    let mut melhor_face = None;

    // Varredura multi-escala (janelas de 25% a 95% da menor dimensão)
    let escalas = [0.30, 0.45, 0.60, 0.75, 0.90];
    for &escala in &escalas {
        let win_size = ((min_dim as f64) * escala).round() as usize;
        if win_size < 24 || win_size > w || win_size > h {
            continue;
        }

        let passo = (win_size / 6).max(2);
        let max_y = h - win_size;
        let max_x = w - win_size;

        let mut y = 0;
        while y <= max_y {
            let mut x = 0;
            while x <= max_x {
                let score = avaliar_haar_face(sat, x, y, win_size);
                if score > 0.48 && score > melhor_score {
                    melhor_score = score;
                    melhor_face = Some(RetanguloFace {
                        x,
                        y,
                        w: win_size,
                        h: win_size,
                    });
                }
                x += passo;
            }
            y += passo;
        }
    }

    // Se a imagem for um crop direto de retrato onde a face ocupa o centro
    if melhor_face.is_none() {
        let score_centro = avaliar_haar_face(sat, w / 8, h / 8, (w * 3 / 4).min(h * 3 / 4));
        if score_centro > 0.35 {
            melhor_face = Some(RetanguloFace {
                x: w / 8,
                y: h / 8,
                w: (w * 3 / 4).min(h * 3 / 4),
                h: (w * 3 / 4).min(h * 3 / 4),
            });
        }
    }

    melhor_face
}

fn avaliar_haar_face(sat: &ImagemIntegral, x: usize, y: usize, s: usize) -> f64 {
    // Geometria da face normalizada em s x s:
    // 1. Testa: y .. y + s*2/8
    // 2. Olhos: y + s*2/8 .. y + s*4/8
    // 3. Nariz/Bochechas: y + s*4/8 .. y + s*6/8
    // 4. Boca/Queixo: y + s*6/8 .. y + s
    let y_testa = y;
    let h_testa = s * 2 / 8;

    let y_olhos = y + h_testa;
    let h_olhos = s * 2 / 8;

    let y_meio = y_olhos + h_olhos;
    let h_meio = s * 2 / 8;

    let y_boca = y_meio + h_meio;
    let h_boca = s - (h_testa + h_olhos + h_meio);

    let w_terco = s / 3;

    // Médias de intensidade
    let m_testa = sat.media(x, y_testa, s, h_testa);
    let m_olho_esq = sat.media(x, y_olhos, w_terco, h_olhos);
    let m_nariz_cima = sat.media(x + w_terco, y_olhos, s - 2 * w_terco, h_olhos);
    let m_olho_dir = sat.media(x + s - w_terco, y_olhos, w_terco, h_olhos);

    let m_bochecha_esq = sat.media(x, y_meio, w_terco, h_meio);
    let m_nariz = sat.media(x + w_terco, y_meio, s - 2 * w_terco, h_meio);
    let m_bochecha_dir = sat.media(x + s - w_terco, y_meio, w_terco, h_meio);

    let m_boca = sat.media(x + w_terco / 2, y_boca, s - w_terco, h_boca);

    let m_olhos_media = (m_olho_esq + m_olho_dir) / 2.0;
    let m_bochechas_media = (m_bochecha_esq + m_bochecha_dir) / 2.0;

    let mut pontos = 0.0f64;

    // Regra Haar 1: Cavidades oculares são mais escuras que a testa
    if m_testa > m_olhos_media + 2.0 {
        pontos += 0.25;
    }

    // Regra Haar 2: Bochechas são mais claras que as órbitas dos olhos
    if m_bochechas_media > m_olhos_media + 1.0 {
        pontos += 0.25;
    }

    // Regra Haar 3: Ponte nasal é mais clara ou contrastante em relação aos olhos
    if m_nariz_cima >= m_olhos_media - 5.0 || m_nariz > m_olhos_media {
        pontos += 0.20;
    }

    // Regra Haar 4: Simetria bilateral entre os dois lados da face
    let diff_simetria = (m_olho_esq - m_olho_dir).abs() + (m_bochecha_esq - m_bochecha_dir).abs();
    if diff_simetria < 35.0 {
        pontos += 0.15;
    }

    // Regra Haar 5: Boca/lábios possuem padrão diferente do queixo/bochechas
    if (m_bochechas_media - m_boca).abs() > 1.0 {
        pontos += 0.15;
    }

    pontos
}

/// Equalização de histograma cumulativo (CDF) para reduzir sensibilidade a iluminação.
pub fn equalizar_histograma(pixels: &mut [u8]) {
    if pixels.is_empty() {
        return;
    }
    let mut hist = [0u32; 256];
    for &p in pixels.iter() {
        hist[p as usize] += 1;
    }

    let mut cdf = [0u32; 256];
    let mut acc = 0u32;
    let mut cdf_min = None;
    for i in 0..256 {
        acc += hist[i];
        cdf[i] = acc;
        if acc > 0 && cdf_min.is_none() {
            cdf_min = Some(acc);
        }
    }

    let Some(min_val) = cdf_min else { return };
    let total = pixels.len() as u32;
    if total <= min_val {
        return;
    }

    for p in pixels.iter_mut() {
        let val = cdf[*p as usize];
        let eq = ((val.saturating_sub(min_val) as f64) / ((total - min_val) as f64) * 255.0).round();
        *p = eq.clamp(0.0, 255.0) as u8;
    }
}

/// Recorta uma sub-região retangular da imagem em escala de cinza e redimensiona para `novo_w` x `novo_h`.
pub fn recortar_e_redimensionar(
    orig: &[u8],
    orig_w: usize,
    orig_h: usize,
    ret: &RetanguloFace,
    novo_w: usize,
    novo_h: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; novo_w * novo_h];
    let rw = ret.w.max(1);
    let rh = ret.h.max(1);

    for ny in 0..novo_h {
        let sy = ret.y + (ny * rh) / novo_h;
        let sy_safe = sy.min(orig_h.saturating_sub(1));
        for nx in 0..novo_w {
            let sx = ret.x + (nx * rw) / novo_w;
            let sx_safe = sx.min(orig_w.saturating_sub(1));
            out[ny * novo_w + nx] = orig[sy_safe * orig_w + sx_safe];
        }
    }

    out
}

/// Extrai vetor característico LBPH (Local Binary Pattern Histogram) em grade 8x8.
/// Retorna um vetor normalizado de 512 dimensões (64 células x 8 bins).
pub fn extrair_lbph_grid(patch: &[u8], w: usize, h: usize) -> Vec<f64> {
    let mut lbp_img = vec![0u8; w * h];

    // Calcula LBP circular de 8 vizinhos para cada pixel interno
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let c = patch[y * w + x];
            let mut pattern = 0u8;

            let vizinhos = [
                patch[(y - 1) * w + (x - 1)],
                patch[(y - 1) * w + x],
                patch[(y - 1) * w + (x + 1)],
                patch[y * w + (x + 1)],
                patch[(y + 1) * w + (x + 1)],
                patch[(y + 1) * w + x],
                patch[(y + 1) * w + (x - 1)],
                patch[y * w + (x - 1)],
            ];

            for (i, &v) in vizinhos.iter().enumerate() {
                if v >= c {
                    pattern |= 1 << i;
                }
            }

            lbp_img[y * w + x] = pattern;
        }
    }

    // Grade espacial de 8x8 células = 64 blocos
    // Cada bloco gera um histograma de 8 bins (0..=255 dividido em 8 faixas de 32 valores)
    // 64 * 8 = 512 floats
    const GRID_X: usize = 8;
    const GRID_Y: usize = 8;
    const BINS: usize = 8;
    let mut descritor = vec![0.0f64; GRID_X * GRID_Y * BINS];

    let cell_w = w / GRID_X;
    let cell_h = h / GRID_Y;

    for gy in 0..GRID_Y {
        let y_start = gy * cell_h;
        let y_end = if gy == GRID_Y - 1 { h } else { y_start + cell_h };

        for gx in 0..GRID_X {
            let x_start = gx * cell_w;
            let x_end = if gx == GRID_X - 1 { w } else { x_start + cell_w };

            let celula_idx = (gy * GRID_X + gx) * BINS;

            for y in y_start..y_end {
                for x in x_start..x_end {
                    let val = lbp_img[y * w + x];
                    let bin = ((val as usize) / 32).min(BINS - 1);
                    descritor[celula_idx + bin] += 1.0;
                }
            }
        }
    }

    // Normalização L2 do descritor global
    let norma = descritor.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norma > 1e-9 {
        for x in descritor.iter_mut() {
            *x /= norma;
        }
    }

    descritor
}

/// Extrator facial clássico completo (Haar + Equalização + LBPH 512d) a partir de JPEG em memória.
pub fn extrator_haar_lbph(jpeg: &[u8]) -> Option<Vec<f64>> {
    let img = image::load_from_memory(jpeg).ok()?.to_luma8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let raw = img.into_raw();

    let sat = ImagemIntegral::new(w, h, &raw);
    let face = detectar_face_haar(w, h, &sat)?;

    // Normaliza patch facial para 64x64
    let mut patch = recortar_e_redimensionar(&raw, w, h, &face, 64, 64);
    equalizar_histograma(&mut patch);

    let vetor = extrair_lbph_grid(&patch, 64, 64);
    Some(vetor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imagem_integral_calcula_somas_corretamente() {
        let pixels = vec![
            1, 2, 3,
            4, 5, 6,
            7, 8, 9,
        ];
        let sat = ImagemIntegral::new(3, 3, &pixels);
        assert_eq!(sat.soma(0, 0, 3, 3), 45);
        assert_eq!(sat.soma(1, 1, 2, 2), 5 + 6 + 8 + 9);
        assert_eq!(sat.soma(0, 0, 1, 1), 1);
        assert_eq!(sat.soma(2, 2, 1, 1), 9);
    }

    #[test]
    fn equalizacao_distribui_intensidades() {
        let mut pixels = vec![10, 10, 10, 20, 20, 30, 40, 50];
        equalizar_histograma(&mut pixels);
        assert_eq!(pixels.len(), 8);
        assert!(pixels[0] < pixels[7]);
    }

    #[test]
    fn lbph_gera_512_dimensoes_normalizadas() {
        let patch = vec![128u8; 64 * 64];
        let desc = extrair_lbph_grid(&patch, 64, 64);
        assert_eq!(desc.len(), 512);
        let norma = desc.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((norma - 1.0).abs() < 1e-4);
    }
}

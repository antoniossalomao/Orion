//! Reconhecimento facial clássico local baseado em Haar Cascades e LBPH (Local Binary Pattern Histogram)
//! (E11.2; F18.05; REGRAS.md: rosto é local, cadastrado só pelo dono, e nunca sai do computador).
//!
//! Algoritmo 100% nativo em Rust, sem dependências de rede, sem modelos neurais pesados e sem ONNX.
//! 1. Medição de luz (média e contraste) para recusar quadros escuros antes de qualquer conta cara.
//! 2. Detecção facial por padrões Haar e imagem integral (Viola-Jones clássico) numa cópia reduzida.
//! 3. Equalização de histograma para invariância a iluminação.
//! 4. Padrões binários locais *uniformes* (59 rótulos) em grade 8x8, com raiz de Hellinger: o
//!    cosseno entre dois descritores aproxima a distância qui-quadrado do LBPH original.

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

/// Rótulo uniforme de cada padrão de 8 bits: 0..=57 para os 58 padrões com no máximo duas
/// transições 0↔1 (bordas, cantos, pontos) e 58 para todo o resto (ruído).
const fn tabela_uniforme() -> [u8; 256] {
    let mut t = [58u8; 256];
    let mut rotulo = 0u8;
    let mut p = 0usize;
    while p < 256 {
        let b = p as u8;
        if (b ^ b.rotate_left(1)).count_ones() <= 2 {
            t[p] = rotulo;
            rotulo += 1;
        }
        p += 1;
    }
    t
}

static UNIFORME: [u8; 256] = tabela_uniforme();
pub const BINS_UNIFORMES: usize = 59;
pub const GRADE: usize = 8;
/// Dimensão do descritor uniforme: 64 células x 59 rótulos.
pub const DIM_UNIFORME: usize = GRADE * GRADE * BINS_UNIFORMES;
/// Lado do recorte facial normalizado.
pub const LADO_PATCH: usize = 64;
/// Largura máxima da cópia usada só para achar o rosto (a varredura Haar é a parte cara).
pub const LARGURA_DETECCAO: usize = 160;

/// LBPH uniforme em grade 8x8: cada célula vira um histograma de 59 rótulos normalizado, com raiz
/// quadrada (Hellinger) e normalização L2 global. Retorna 3776 floats.
pub fn extrair_lbph_uniforme(patch: &[u8], w: usize, h: usize) -> Vec<f64> {
    let mut descritor = vec![0.0f64; DIM_UNIFORME];
    if w < GRADE * 3 || h < GRADE * 3 || patch.len() < w * h {
        return descritor;
    }
    let (cell_w, cell_h) = (w / GRADE, h / GRADE);
    for y in 1..h - 1 {
        let gy = (y / cell_h).min(GRADE - 1);
        for x in 1..w - 1 {
            let c = patch[y * w + x];
            let v = [
                patch[(y - 1) * w + (x - 1)],
                patch[(y - 1) * w + x],
                patch[(y - 1) * w + (x + 1)],
                patch[y * w + (x + 1)],
                patch[(y + 1) * w + (x + 1)],
                patch[(y + 1) * w + x],
                patch[(y + 1) * w + (x - 1)],
                patch[y * w + (x - 1)],
            ];
            let mut padrao = 0u8;
            for (i, &n) in v.iter().enumerate() {
                if n >= c {
                    padrao |= 1 << i;
                }
            }
            let gx = (x / cell_w).min(GRADE - 1);
            descritor[(gy * GRADE + gx) * BINS_UNIFORMES + UNIFORME[padrao as usize] as usize] += 1.0;
        }
    }
    for celula in descritor.chunks_mut(BINS_UNIFORMES) {
        let total: f64 = celula.iter().sum();
        if total > 0.0 {
            for x in celula.iter_mut() {
                *x = (*x / total).sqrt();
            }
        }
    }
    let norma = descritor.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norma > 1e-9 {
        for x in descritor.iter_mut() {
            *x /= norma;
        }
    }
    descritor
}

/// Luz do quadro: média e desvio-padrão da luminância (0..255).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Luz {
    pub media: f64,
    pub desvio: f64,
}

/// Abaixo disto o quadro é escuro ou chapado demais para o LBPH ter informação.
pub const LUZ_MINIMA: f64 = 38.0;
pub const CONTRASTE_MINIMO: f64 = 14.0;

impl Luz {
    pub fn suficiente(&self) -> bool {
        self.media >= LUZ_MINIMA && self.desvio >= CONTRASTE_MINIMO
    }
}

/// Mede a luz por amostragem (1 a cada 4 pixels basta e custa 4x menos).
pub fn medir_luz(pixels: &[u8]) -> Luz {
    let amostra = pixels.iter().step_by(4);
    let n = pixels.len().div_ceil(4).max(1) as f64;
    let soma: f64 = amostra.clone().map(|&p| p as f64).sum();
    let media = soma / n;
    let var = amostra.map(|&p| (p as f64 - media).powi(2)).sum::<f64>() / n;
    Luz { media, desvio: var.sqrt() }
}

/// Redução por média de caixa (sem dependência e sem serrilhado). Só reduz; nunca amplia.
pub fn reduzir_luma(pixels: &[u8], w: usize, h: usize, max_w: usize) -> (Vec<u8>, usize, usize) {
    if w <= max_w || w == 0 || h == 0 {
        return (pixels.to_vec(), w, h);
    }
    let fator = w.div_ceil(max_w);
    let (nw, nh) = (w / fator, h / fator);
    let mut out = vec![0u8; nw * nh];
    let area = (fator * fator) as u32;
    for ny in 0..nh {
        for nx in 0..nw {
            let mut soma = 0u32;
            for dy in 0..fator {
                let linha = (ny * fator + dy) * w + nx * fator;
                for dx in 0..fator {
                    soma += pixels[linha + dx] as u32;
                }
            }
            out[ny * nw + nx] = (soma / area) as u8;
        }
    }
    (out, nw, nh)
}

/// Resultado da análise de um quadro em escala de cinza.
#[derive(Debug, Clone)]
pub struct Analise {
    pub luz: Luz,
    /// Rosto achado (em coordenadas do quadro original) e o descritor uniforme dele.
    pub rosto: Option<(RetanguloFace, Vec<f64>)>,
}

/// Pipeline completo sobre um quadro cinza: luz → detecção numa cópia de até 160 px de largura →
/// recorte no quadro original → equalização → LBPH uniforme. Quadro escuro sai cedo, sem varredura.
pub fn analisar_quadro(luma: &[u8], w: usize, h: usize) -> Analise {
    let luz = medir_luz(luma);
    if !luz.suficiente() || w < 24 || h < 24 || luma.len() < w * h {
        return Analise { luz, rosto: None };
    }
    let (pequeno, pw, ph) = reduzir_luma(luma, w, h, LARGURA_DETECCAO);
    let sat = ImagemIntegral::new(pw, ph, &pequeno);
    let Some(r) = detectar_face_haar(pw, ph, &sat) else {
        return Analise { luz, rosto: None };
    };
    let (fx, fy) = (w as f64 / pw as f64, h as f64 / ph as f64);
    let face = RetanguloFace {
        x: (r.x as f64 * fx) as usize,
        y: (r.y as f64 * fy) as usize,
        w: ((r.w as f64 * fx) as usize).min(w),
        h: ((r.h as f64 * fy) as usize).min(h),
    };
    let mut patch = recortar_e_redimensionar(luma, w, h, &face, LADO_PATCH, LADO_PATCH);
    equalizar_histograma(&mut patch);
    Analise { luz, rosto: Some((face, extrair_lbph_uniforme(&patch, LADO_PATCH, LADO_PATCH))) }
}

/// Extrator facial clássico completo (luz + Haar + equalização + LBPH uniforme) a partir de JPEG.
pub fn extrator_haar_lbph(jpeg: &[u8]) -> Option<Vec<f64>> {
    let img = image::load_from_memory(jpeg).ok()?.to_luma8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    analisar_quadro(img.as_raw(), w, h).rosto.map(|(_, v)| v)
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

    #[test]
    fn tabela_uniforme_tem_58_rotulos_mais_ruido() {
        let distintos: std::collections::BTreeSet<u8> = UNIFORME.iter().copied().collect();
        assert_eq!(distintos.len(), 59);
        assert_eq!(UNIFORME[0], 0);
        assert_eq!(UNIFORME[0b0101_0101], 58);
    }

    #[test]
    fn lbph_uniforme_normalizado_e_discriminante() {
        let liso = vec![128u8; 64 * 64];
        let listras: Vec<u8> = (0..64 * 64).map(|i| if (i % 64) / 4 % 2 == 0 { 40 } else { 210 }).collect();
        let a = extrair_lbph_uniforme(&liso, 64, 64);
        let b = extrair_lbph_uniforme(&listras, 64, 64);
        assert_eq!(a.len(), DIM_UNIFORME);
        let norma = a.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!((norma - 1.0).abs() < 1e-6);
        let cos: f64 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
        assert!(cos < 0.9, "texturas diferentes não podem coincidir (cos={cos})");
    }

    #[test]
    fn quadro_escuro_nao_varre() {
        let escuro = vec![12u8; 320 * 240];
        let a = analisar_quadro(&escuro, 320, 240);
        assert!(!a.luz.suficiente());
        assert!(a.rosto.is_none());
    }

    #[test]
    fn reduzir_mantem_proporcao_e_media() {
        let px = vec![100u8; 640 * 480];
        let (out, w, h) = reduzir_luma(&px, 640, 480, 160);
        assert_eq!((w, h), (160, 120));
        assert!(out.iter().all(|&p| p == 100));
        let (same, w2, _) = reduzir_luma(&px[..100 * 80], 100, 80, 160);
        assert_eq!((same.len(), w2), (8000, 100));
    }
}

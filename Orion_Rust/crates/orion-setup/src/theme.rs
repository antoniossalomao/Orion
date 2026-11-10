//! Tokens de cor, dimensões e curva de animação do `Orion Setup.exe`
//! (`ORION_INSTALADOR_ESPECIFICACAO.md`, §§ 2–5).

use iced::Color;

pub const WINDOW_W: f32 = 960.0;
pub const WINDOW_H: f32 = 600.0;
pub const TITLEBAR_H: f32 = 36.0;
pub const BODY_H: f32 = 564.0;
pub const SKY_W: f32 = 468.0;
pub const PANEL_W: f32 = 492.0;

/// Escala de espaçamento, a mesma do app (tema próprio do instalador, tokens espelhados) (que usa 2, 6, 10 e 14 além da grade de 4).
/// `SP_PX` é o fio de 1 px; zero fica literal. Medidas de componente (larguras, alturas) não entram aqui.
pub const SP_PX: f32 = 1.0;
pub const SP_05: f32 = 2.0;
pub const SP_1: f32 = 4.0;
pub const SP_15: f32 = 6.0;
pub const SP_2: f32 = 8.0;
pub const SP_25: f32 = 10.0;
pub const SP_3: f32 = 12.0;
pub const SP_35: f32 = 14.0;
pub const SP_4: f32 = 16.0;
pub const SP_45: f32 = 18.0;
pub const SP_5: f32 = 20.0;
pub const SP_6: f32 = 24.0;
pub const SP_7: f32 = 28.0;
pub const SP_8: f32 = 32.0;
pub const SP_9: f32 = 36.0;
pub const SP_10: f32 = 40.0;
pub const SP_11: f32 = 44.0;
pub const SP_12: f32 = 48.0;
pub const SP_14: f32 = 56.0;
pub const SP_18: f32 = 72.0;
/// Raios: etiqueta, controle, campo, cartão e pílula.
pub const R_TAG: f32 = 6.0;
pub const R_CTRL: f32 = 8.0;
pub const R_FIELD: f32 = 10.0;
pub const R_CARD: f32 = 12.0;
pub const R_PILL: f32 = 999.0;

pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

/// Fundo fora da janela (protótipo): `#0a0b0e`.
pub const BG: Color = rgb(0x0a, 0x0b, 0x0e);
/// Barra de título: `#0e0f11`.
pub const TITLEBAR: Color = rgb(0x0e, 0x0f, 0x11);
/// Fundo da janela e do painel: `#111214`.
pub const SIDE: Color = rgb(0x11, 0x12, 0x14);
/// Campos, cartões, botão secundário: `#1b1c1f`.
pub const RAISE: Color = rgb(0x1b, 0x1c, 0x1f);
/// Bordas e divisórias: `#25272c`.
pub const LINE: Color = rgb(0x25, 0x27, 0x2c);
/// Texto principal: `#ececee`.
pub const FG: Color = rgb(0xec, 0xec, 0xee);
/// Texto secundário: `#a9aab0`.
pub const FG2: Color = rgb(0xa9, 0xaa, 0xb0);
/// Texto legenda: `#8e8f96`, o mesmo do app. O `#75767d` da especificação dava ~4,0:1 sobre `SIDE`; este passa de 4,5:1.
pub const FG3: Color = rgb(0x8e, 0x8f, 0x96);
/// Foco, seleção, etapas concluídas, switches: `#a4b3de`.
pub const RIGEL: Color = rgb(0xa4, 0xb3, 0xde);
/// Etapa atual, selo recomendado, nome da estrela, alerta: `#e9b467`.
pub const BETEL: Color = rgb(0xe9, 0xb4, 0x67);
/// Botão principal: `#b9c6ea`.
pub const PRIMARY: Color = rgb(0xb9, 0xc6, 0xea);
/// Texto sobre `primary`: `#0f1320`.
pub const ON_PRIMARY: Color = rgb(0x0f, 0x13, 0x20);
/// Itens concluídos, checagem OK: `#74cfae`.
pub const OK: Color = rgb(0x74, 0xcf, 0xae);
/// Erro na instalação: `#f07c88`.
pub const DANGER: Color = rgb(0xf0, 0x7c, 0x88);
/// Fundo do canvas do céu: `#07080b`.
pub const SKY_BG: Color = rgb(0x07, 0x08, 0x0b);
/// Fundo de cartão de modelo selecionado: `#1c2030`.
pub const MODEL_SELECTED_BG: Color = rgb(0x1c, 0x20, 0x30);

/// Véu sobre o céu atrás do rótulo da estrela: `SKY_BG` a 78%.
pub const SKY_VEIL: Color = rgba(7, 8, 11, 0.78);
/// Selo "recomendado": fundo e borda em `BETEL`.
pub const BETEL_SOFT: Color = rgba(233, 180, 103, 0.08);
pub const BETEL_LINE: Color = rgba(233, 180, 103, 0.42);
/// Fundo do botão fechar ao passar o mouse: `DANGER` a 22%.
pub const CLOSE_HOVER: Color = rgba(240, 124, 136, 0.22);

/// Aplica opacidade multiplicativa a uma cor.
pub fn with_alpha(mut c: Color, alpha: f32) -> Color {
    c.a = (c.a * alpha).clamp(0.0, 1.0);
    c
}

/// Curva padrão da interface: `cubic-bezier(0.22, 0.61, 0.36, 1.0)` (§3).
pub fn standard_curve(x: f32) -> f32 {
    cubic_bezier(0.22, 0.61, 0.36, 1.0, x.clamp(0.0, 1.0))
}

/// Curva ease-in-out cúbica das linhas da constelação (§8.8):
/// `p < 0.5 ? 4p³ : 1 - (-2p + 2)³ / 2`.
pub fn ease_in_out_cubic(p: f32) -> f32 {
    let p = p.clamp(0.0, 1.0);
    if p < 0.5 {
        4.0 * p * p * p
    } else {
        let u = -2.0 * p + 2.0;
        1.0 - (u * u * u) * 0.5
    }
}

/// Resolve `y(x)` de uma curva de Bézier cúbica `(x1, y1, x2, y2)` via Newton-Raphson.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let sample_x = |t: f32| -> f32 {
        let inv = 1.0 - t;
        3.0 * inv * inv * t * x1 + 3.0 * inv * t * t * x2 + t * t * t
    };
    let sample_dx = |t: f32| -> f32 {
        let inv = 1.0 - t;
        3.0 * inv * inv * x1 + 6.0 * inv * t * (x2 - x1) + 3.0 * t * t * (1.0 - x2)
    };
    let sample_y = |t: f32| -> f32 {
        let inv = 1.0 - t;
        3.0 * inv * inv * t * y1 + 3.0 * inv * t * t * y2 + t * t * t
    };

    let mut t = x;
    for _ in 0..8 {
        let err = sample_x(t) - x;
        if err.abs() < 1e-5 {
            break;
        }
        let d = sample_dx(t);
        if d.abs() < 1e-6 {
            break;
        }
        t = (t - err / d).clamp(0.0, 1.0);
    }
    sample_y(t)
}

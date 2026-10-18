//! Tokens de cor, dimensões e curva de animação do `Orion Setup.exe`
//! (`ORION_INSTALADOR_ESPECIFICACAO.md`, §§ 2–5).

use iced::Color;

pub const WINDOW_W: f32 = 960.0;
pub const WINDOW_H: f32 = 600.0;
pub const TITLEBAR_H: f32 = 36.0;
pub const BODY_H: f32 = 564.0;
pub const SKY_W: f32 = 468.0;
pub const PANEL_W: f32 = 492.0;

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
/// Texto legenda: `#75767d`.
pub const FG3: Color = rgb(0x75, 0x76, 0x7d);
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

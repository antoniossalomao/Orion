//! Tokens do protótipo `orion-ui-mockup.html` (ORION - UI (ESPECIFICAÇÃO), §4 e §5). Tema único: Grafite escuro.

use iced::font::{Family, Style, Weight};
use iced::{Color, Font};

pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}
pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}
pub fn alpha(c: Color, a: f32) -> Color {
    Color { a: c.a * a, ..c }
}
/// Clareia a cor multiplicando os canais por `k` (realce de hover do botão principal).
pub fn lighten(c: Color, k: f32) -> Color {
    Color { r: (c.r * k).min(1.0), g: (c.g * k).min(1.0), b: (c.b * k).min(1.0), a: 1.0 }
}
/// Interpola duas cores, canal a canal.
pub fn lerp(a: Color, b: Color, k: f32) -> Color {
    Color { r: a.r + (b.r - a.r) * k, g: a.g + (b.g - a.g) * k, b: a.b + (b.b - a.b) * k, a: a.a + (b.a - a.a) * k }
}
/// `color-mix(in srgb, c p%, transparent)`.
pub fn mix(c: Color, p: f32) -> Color {
    alpha(c, p)
}

pub const BG: Color = rgb(0x15, 0x16, 0x18);
pub const SIDE: Color = rgb(0x11, 0x12, 0x14);
pub const RAISE: Color = rgb(0x1b, 0x1c, 0x1f);
pub const RAISE2: Color = rgb(0x22, 0x23, 0x27);
pub const SEL: Color = rgb(0x26, 0x27, 0x2b);
pub const HOVER: Color = rgb(0x1e, 0x1f, 0x22);
pub const LINE: Color = rgba(255, 255, 255, 0.065);
pub const LINE2: Color = rgba(255, 255, 255, 0.11);
pub const LINE3: Color = rgba(255, 255, 255, 0.20);
/// Borda de campo de texto: ~3,5:1 sobre as superfícies (WCAG 1.4.11).
pub const FIELD_LINE: Color = rgba(255, 255, 255, 0.38);
/// Raios: controle, cartão, painel e pílula. Formas mínimas (barras, pontos) ficam fora da escala.
pub const R_CTRL: f32 = 8.0;
pub const R_CARD: f32 = 12.0;
pub const R_PANEL: f32 = 14.0;
pub const R_PILL: f32 = 999.0;
/// Etiqueta e código em linha (5 px no protótipo).
pub const R_TAG: f32 = 5.0;
/// Escala de espaçamento, ajustada ao protótipo (que usa 2, 6, 10 e 14 além da grade de 4).
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
/// Escala de tipo da interface: 11/12/13/14/16/18/20/24. Meios-passos caem no vizinho.
pub fn snap_size(s: f32) -> f32 {
    match (s * 2.0).round() as i32 {
        19..=22 => 11.0,
        23 | 25 => 12.0,
        27 => 14.0,
        29 | 30 | 31 | 34 => 16.0,
        38 => 18.0,
        _ => s,
    }
}
pub const FG: Color =rgb(0xec, 0xec, 0xee);
pub const FG2: Color = rgb(0xa9, 0xaa, 0xb0);
pub const FG3: Color = rgb(0x8e, 0x8f, 0x96);
pub const FG4: Color = rgb(0x4e, 0x4f, 0x55);
pub const PRIMARY: Color = rgb(0xb9, 0xc6, 0xea);
pub const PRIMARY_INK: Color = rgb(0x0f, 0x13, 0x20);
pub const ACCENT: Color = rgb(0xa4, 0xb3, 0xde);
pub const ACCENT_SOFT: Color = rgba(164, 179, 222, 0.12);
pub const SIGNAL: Color = rgb(0xe9, 0xb4, 0x67);
pub const SIGNAL_SOFT: Color = rgba(233, 180, 103, 0.11);
pub const OK: Color = rgb(0x74, 0xcf, 0xae);
pub const OK_SOFT: Color = rgba(116, 207, 174, 0.11);
pub const DANGER: Color = rgb(0xf0, 0x7c, 0x88);
pub const DANGER_SOFT: Color = rgba(240, 124, 136, 0.11);
/// Glifo sobre o fundo de perigo (botão fechar ao passar o mouse).
pub const ON_DANGER: Color = rgb(0xff, 0xff, 0xff);
pub const STAR: Color = rgb(0xee, 0xf0, 0xf6);
pub const SCRIM: Color = rgba(6, 6, 8, 0.6);
pub const CAM_DOT: Color = rgb(0xff, 0x4d, 0x5e);
pub const TERM_BG: Color = rgb(0x0b, 0x0c, 0x0d);
pub const TERM_FG: Color = rgb(0xcf, 0xd2, 0xd8);
pub const ISLAND_BG: Color = rgb(0x0b, 0x0c, 0x10);
pub const GRAPH_ARESTA: Color = rgb(0x9a, 0xa3, 0xc0);
/// Céu do grafo: centro do degradê (a borda é `ISLAND_BG`), fundo do rótulo e a névoa azul da distância.
pub const GRAPH_SKY_CORE: Color = rgb(0x15, 0x19, 0x2a);
pub const GRAPH_PILL: Color = rgb(0x0c, 0x0e, 0x13);
pub const ATMOS: Color = rgb(140, 168, 250);
/// Estrelas de fundo: fria e quente (a neutra é `ON_VEIL`).
pub const STAR_COOL: Color = rgb(184, 204, 255);
pub const STAR_WARM: Color = rgb(255, 230, 199);
/// Halo do núcleo do grafo, de fora (azul) para dentro (quente).
pub const GRAPH_HALO: [Color; 5] = [rgb(140, 168, 255), rgb(158, 184, 255), rgb(217, 217, 255), rgb(255, 237, 204), rgb(255, 242, 217)];
/// Véus pretos sobre a janela: captura rápida, pedido de senha, recorte de tela e legenda ao vivo.
pub const VEIL_LIGHT: Color = rgba(0, 0, 0, 0.25);
pub const VEIL_MID: Color = rgba(0, 0, 0, 0.35);
pub const VEIL_STRONG: Color = rgba(0, 0, 0, 0.45);
pub const VEIL_CAPTION: Color = rgba(0, 0, 0, 0.78);
pub const ON_VEIL: Color = rgb(0xff, 0xff, 0xff);
/// Fim do degradê do painel do login (tom dentro do mesmo neutro de `SIDE`).
pub const LOGIN_PANEL: Color = rgb(19, 20, 24);

/// Cores por tipo de nota (§4.2), na ordem de `TIPOS`.
pub const N_PROJETO: Color = rgb(0xe9, 0xb4, 0x67);
pub const N_AREA: Color = rgb(0xd9, 0x8f, 0x6a);
pub const N_CONCEITO: Color = rgb(0x9f, 0xb2, 0xea);
pub const N_PESQUISA: Color = rgb(0x82, 0xcf, 0xa6);
pub const N_RECURSO: Color = rgb(0xa7, 0xab, 0xb6);
pub const N_CONVERSA: Color = rgb(0xe7, 0x96, 0xbf);
pub const N_RELATORIO: Color = rgb(0x6a, 0x6e, 0x78);

// Medidas (§3, §4.3)
pub const SIDE_W: f32 = 272.0;
pub const CTX_W: f32 = 344.0;
pub const BAR_H: f32 = 46.0;
pub const DRAWER_MAX: f32 = 760.0;
pub const CTX_MIN_W: f32 = 1120.0;

// Escala de tipo (F01.03): texto base 14, h1 24, eyebrow 11, rótulos 12, legenda 11.
pub const TEXT_BASE: f32 = 14.0;
pub const TEXT_H1: f32 = 24.0;
pub const TEXT_EYEBROW: f32 = 11.0;
pub const TEXT_LABEL: f32 = 12.0;
pub const TEXT_CAPTION: f32 = 11.0;

// Formas (F01.04): cantos entre 10 e 14 px.
pub const RADIUS_MAX: f32 = 14.0;

// Fontes (§5). Os arquivos estão em `assets/fonts` e vão dentro do binário.
const fn font(name: &'static str, weight: Weight, style: Style) -> Font {
    Font {
        family: Family::Name(name),
        weight,
        stretch: iced::font::Stretch::Normal,
        style,
    }
}
pub const UI: Font = font("Geist", Weight::Normal, Style::Normal);
pub const UI_MED: Font = font("Geist", Weight::Medium, Style::Normal);
pub const UI_SEMI: Font = font("Geist", Weight::Semibold, Style::Normal);
pub const MONO: Font = font("Geist Mono", Weight::Normal, Style::Normal);
pub const MONO_MED: Font = font("Geist Mono", Weight::Medium, Style::Normal);
pub const SERIF: Font = font("Newsreader", Weight::Normal, Style::Normal);
pub const SERIF_MED: Font = font("Newsreader", Weight::Medium, Style::Normal);
pub const SERIF_SEMI: Font = font("Newsreader", Weight::Semibold, Style::Normal);
pub const SERIF_IT: Font = font("Newsreader", Weight::Normal, Style::Italic);

pub fn fonts() -> Vec<std::borrow::Cow<'static, [u8]>> {
    macro_rules! f {
        ($n:literal) => {
            std::borrow::Cow::Borrowed(include_bytes!(concat!("../assets/fonts/", $n, ".ttf")).as_slice())
        };
    }
    vec![
        f!("geist-sans-latin-400-normal"),
        f!("geist-sans-latin-500-normal"),
        f!("geist-sans-latin-600-normal"),
        f!("geist-mono-latin-400-normal"),
        f!("geist-mono-latin-500-normal"),
        f!("newsreader-latin-400-normal"),
        f!("newsreader-latin-500-normal"),
        f!("newsreader-latin-600-normal"),
        f!("newsreader-latin-400-italic"),
    ]
}

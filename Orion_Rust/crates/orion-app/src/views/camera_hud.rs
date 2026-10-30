//! HUD da câmera, no Orion com um toque de *Person of Interest*: vídeo cinza escurecido, cantos
//! finos em volta do rosto (nada de caixa cheia) e telemetria discreta no canto inferior esquerdo.
//! Serve ao Face ID (login e cadastro) e fica pronto para o módulo de câmera (REGRAS.md: rosto é
//! local, cadastrado só pelo dono, e nunca sai do computador; o quadro vive só na memória da janela).

use crate::app::Msg;
use crate::net::FacePreview;
use crate::theme as t;
use crate::ui::*;
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, LineCap, Path, Stroke};
use iced::widget::{canvas as cv, column, container, image, stack};
use iced::{Border, Color, ContentFit, Length, Point, Rectangle, Renderer, Size, Theme};

/// Largura do painel; a altura segue a proporção do quadro.
pub const HUD_W: f32 = 360.0;

/// Cantos e marcas sobre o vídeo.
struct Overlay {
    face: Option<[f32; 4]>,
    /// 21 pontos da mão (0..1), quando o motor de gestos vê uma.
    hand: Option<Vec<[f32; 2]>>,
    tone: Color,
    /// Segundos desde o começo da fase (respiração dos cantos).
    t: f32,
    reduced: bool,
}

/// Quatro cantos em L em volta de `r`, com braço de `arm` px.
fn corners(f: &mut Frame, r: Rectangle, arm: f32, color: Color, width: f32) {
    let stroke = Stroke::default().with_color(color).with_width(width).with_line_cap(LineCap::Square);
    for (cx, cy, sx, sy) in [(r.x, r.y, 1.0, 1.0), (r.x + r.width, r.y, -1.0, 1.0), (r.x + r.width, r.y + r.height, -1.0, -1.0), (r.x, r.y + r.height, 1.0, -1.0)] {
        let p = Path::new(|p| {
            p.move_to(Point::new(cx + sx * arm, cy));
            p.line_to(Point::new(cx, cy));
            p.line_to(Point::new(cx, cy + sy * arm));
        });
        f.stroke(&p, stroke.clone());
    }
}

impl canvas::Program<Msg> for Overlay {
    type State = ();
    fn draw(&self, _: &(), r: &Renderer, _: &Theme, b: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(r, b.size());
        let pulse = if self.reduced { 1.0 } else { 0.75 + 0.25 * (self.t * 3.2).sin() };
        // moldura do visor: cantos bem apagados nas bordas do quadro
        corners(&mut f, Rectangle::new(Point::new(10.0, 10.0), Size::new(b.width - 20.0, b.height - 20.0)), 14.0, t::alpha(Color::WHITE, 0.22), 1.0);
        if let Some([x, y, w, h]) = self.face {
            // um pouco de folga em volta do rosto, como o recorte da série
            let m = 0.08;
            let rect = Rectangle::new(Point::new((x - w * m) * b.width, (y - h * m) * b.height), Size::new(w * (1.0 + 2.0 * m) * b.width, h * (1.0 + 2.0 * m) * b.height));
            let arm = rect.width.min(rect.height) * 0.18;
            corners(&mut f, rect, arm, t::alpha(self.tone, 0.95 * pulse), 1.6);
            // mira mínima no centro do rosto
            let c = rect.center();
            for (dx, dy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                let p = Path::new(|p| {
                    p.move_to(Point::new(c.x + dx * 3.0, c.y + dy * 3.0));
                    p.line_to(Point::new(c.x + dx * 7.0, c.y + dy * 7.0));
                });
                f.stroke(&p, Stroke::default().with_color(t::alpha(self.tone, 0.55)).with_width(1.0));
            }
        }
        if let Some(points) = self.hand.as_ref().filter(|p| p.len() == 21) {
            let at = |i: usize| Point::new(points[i][0] * b.width, points[i][1] * b.height);
            // esqueleto fino: palma e os cinco dedos
            for chain in HAND_CHAINS {
                let p = Path::new(|p| {
                    p.move_to(at(chain[0]));
                    for &i in &chain[1..] {
                        p.line_to(at(i));
                    }
                });
                f.stroke(&p, Stroke::default().with_color(t::alpha(t::ACCENT, 0.55)).with_width(1.2).with_line_cap(LineCap::Round));
            }
            for (i, _) in points.iter().enumerate() {
                let r = if [4, 8, 12, 16, 20].contains(&i) { 2.6 } else { 1.8 };
                f.fill(&Path::circle(at(i), r), t::alpha(t::STAR, 0.9));
            }
            // cantos em volta da mão, no mesmo traço do rosto
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in points {
                x0 = x0.min(p[0] * b.width);
                y0 = y0.min(p[1] * b.height);
                x1 = x1.max(p[0] * b.width);
                y1 = y1.max(p[1] * b.height);
            }
            let rect = Rectangle::new(Point::new(x0 - 8.0, y0 - 8.0), Size::new(x1 - x0 + 16.0, y1 - y0 + 16.0));
            corners(&mut f, rect, rect.width.min(rect.height) * 0.16, t::alpha(t::ACCENT, 0.9), 1.4);
        }
        vec![f.into_geometry()]
    }
}

/// Ligações dos 21 pontos do MediaPipe Hands: palma, polegar, indicador, médio, anelar, mínimo.
const HAND_CHAINS: [&[usize]; 6] = [&[0, 5, 9, 13, 17, 0], &[0, 1, 2, 3, 4], &[5, 6, 7, 8], &[9, 10, 11, 12], &[13, 14, 15, 16], &[17, 18, 19, 20]];

/// Um bloco de telemetria: `[ROTULO: valor]` em Geist Mono sobre fundo translúcido.
fn block<'a>(s: String, color: Color) -> El<'a> {
    container(txf(s, 10.5, color, t::MONO))
        .padding(pad(1.0, 6.0, 1.0, 6.0))
        .style(|_| container::Style { background: Some(t::alpha(Color::BLACK, 0.55).into()), border: Border { radius: 3.0.into(), ..Default::default() }, ..Default::default() })
        .into()
}

/// O painel: vídeo, cantos e telemetria (`[CONFIDENCE]`, `[PROCESSOR]`). `tag` aparece sobre o
/// rosto quando reconhecido.
pub fn view<'a>(preview: &FacePreview, handle: &image::Handle, tone: Color, tag: Option<String>, t_phase: f32, reduced: bool) -> El<'a> {
    let cpu = preview.cpu.map(|c| format!("{c:.0}%")).unwrap_or_else(|| "--".into());
    // no cadastro ainda não há modelo: só o processador aparece
    let mut telemetry = vec![];
    if let Some(c) = preview.confidence {
        telemetry.push((format!("[CONFIDENCE: {:.1}%]", c * 100.0), t::alpha(tone, 0.95)));
    }
    telemetry.push((format!("[PROCESSOR: {cpu} CPU]"), t::FG2));
    frame(Hud { w: preview.w, h: preview.h, handle, face: preview.face, hand: None, tone, tag, telemetry, t_phase, reduced, width: HUD_W })
}

/// Tudo que o painel desenha (Face ID ou módulo de câmera).
pub struct Hud<'h> {
    pub w: u32,
    pub h: u32,
    pub handle: &'h image::Handle,
    pub face: Option<[f32; 4]>,
    pub hand: Option<Vec<[f32; 2]>>,
    pub tone: Color,
    pub tag: Option<String>,
    pub telemetry: Vec<(String, Color)>,
    pub t_phase: f32,
    pub reduced: bool,
    pub width: f32,
}

/// O painel genérico: vídeo, cantos no rosto, esqueleto da mão e blocos de telemetria.
pub fn frame<'a>(hud: Hud<'_>) -> El<'a> {
    let width = hud.width;
    let h = (width * hud.h as f32 / hud.w.max(1) as f32).round();
    let video = image(hud.handle.clone()).width(Length::Fixed(width)).height(Length::Fixed(h)).content_fit(ContentFit::Cover);
    let overlay = cv(Overlay { face: hud.face, hand: hud.hand, tone: hud.tone, t: hud.t_phase, reduced: hud.reduced }).width(Length::Fixed(width)).height(Length::Fixed(h));
    let mut blocks = column![].spacing(4);
    for (text, color) in hud.telemetry {
        blocks = blocks.push(block(text, color));
    }
    let telemetry = container(blocks)
        .width(Length::Fixed(width))
        .height(Length::Fixed(h))
        .align_bottom(Length::Fixed(h))
        .padding(pad(0.0, 0.0, 12.0, 14.0));
    let mut layers = stack![video, overlay, telemetry];
    if let (Some(tag), Some([x, y, _, _])) = (hud.tag, hud.face) {
        let (left, top) = ((x * width).max(6.0), (y * h - 22.0).max(6.0));
        layers = layers.push(container(block(tag, hud.tone)).padding(pad(top, 0.0, 0.0, left)));
    }
    container(layers)
        .width(Length::Fixed(width))
        .height(Length::Fixed(h))
        .clip(true)
        .style(|_| container::Style { background: Some(t::ISLAND_BG.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_CARD.into() }, shadow: shadow(), ..Default::default() })
        .into()
}

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
        vec![f.into_geometry()]
    }
}

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
    let h = (HUD_W * preview.h as f32 / preview.w.max(1) as f32).round();
    let video = image(handle.clone()).width(Length::Fixed(HUD_W)).height(Length::Fixed(h)).content_fit(ContentFit::Cover);
    let overlay = cv(Overlay { face: preview.face, tone, t: t_phase, reduced }).width(Length::Fixed(HUD_W)).height(Length::Fixed(h));
    let cpu = preview.cpu.map(|c| format!("{c:.0}%")).unwrap_or_else(|| "--".into());
    // no cadastro ainda não há modelo: só o processador aparece
    let mut blocks = column![].spacing(4);
    if let Some(c) = preview.confidence {
        blocks = blocks.push(block(format!("[CONFIDENCE: {:.1}%]", c * 100.0), t::alpha(tone, 0.95)));
    }
    let telemetry = container(blocks.push(block(format!("[PROCESSOR: {cpu} CPU]"), t::FG2)))
        .width(Length::Fixed(HUD_W))
        .height(Length::Fixed(h))
        .align_bottom(Length::Fixed(h))
        .padding(pad(0.0, 0.0, 12.0, 14.0));
    let mut layers = stack![video, overlay, telemetry];
    if let (Some(tag), Some([x, y, _, _])) = (tag, preview.face) {
        let (left, top) = ((x * HUD_W).max(6.0), (y * h - 22.0).max(6.0));
        layers = layers.push(container(block(tag, tone)).padding(pad(top, 0.0, 0.0, left)));
    }
    container(layers)
        .width(Length::Fixed(HUD_W))
        .height(Length::Fixed(h))
        .clip(true)
        .style(|_| container::Style { background: Some(t::ISLAND_BG.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_CARD.into() }, shadow: shadow(), ..Default::default() })
        .into()
}

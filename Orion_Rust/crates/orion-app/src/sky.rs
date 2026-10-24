//! Céu do protótipo (`drawSky`, §26.2): 240 estrelas (Park–Miller, seed 7) e a constelação estática.
//! Entrada finita com GPU; em repouso e em software a constelação permanece estática.

use crate::app::Msg;
use crate::theme as t;
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme};

#[derive(Clone, Copy)]
pub struct Sky {
    pub alpha: f32,
    pub constellation: bool,
    /// Progresso finito da entrada; 1 mantém a geometria em cache sem animação.
    pub intro: f32,
    /// 0 = Cinturão branco; 1 = verde (voz ao vivo ligada).
    pub belt: f32,
}

struct Rnd(u64);
impl Rnd {
    fn next(&mut self) -> f32 {
        self.0 = (self.0 * 16807) % 2147483647;
        self.0 as f32 / 2147483647.0
    }
}

fn glow(frame: &mut Frame, p: Point, r: f32, color: Color, a: f32) {
    const N: usize = 14;
    for k in 0..N {
        let rr = r * (1.0 - k as f32 / N as f32);
        frame.fill(&Path::circle(p, rr), t::alpha(color, a * 0.8 / N as f32));
    }
}

impl canvas::Program<Msg> for Sky {
    type State = (canvas::Cache, std::cell::Cell<Option<(f32, bool, f32, f32)>>);

    fn draw(&self, state: &Self::State, renderer: &Renderer, _th: &Theme, bounds: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let key = (self.alpha, self.constellation, self.belt, self.intro);
        if state.1.replace(Some(key)) != Some(key) { state.0.clear(); }
        vec![state.0.draw(renderer, bounds.size(), |frame| draw_sky(frame, bounds.width, bounds.height, *self))]
    }
}

pub fn draw_sky(f: &mut Frame, w: f32, h: f32, sky: Sky) {
    {
        let this = &sky;
        let mut rnd = Rnd(7);
        for _ in 0..240 {
            let a = (0.06 + rnd.next() * 0.38) * this.alpha;
            let x = rnd.next() * w;
            let y = rnd.next() * h;
            let r = 0.3 + rnd.next() * 0.9;
            f.fill(&Path::circle(Point::new(x, y), r), t::alpha(t::STAR, a));
        }
        if this.constellation {
            let s = w.min(h) * 0.92;
            let (ox, oy) = (w / 2.0 - s / 2.0, h / 2.0 - s / 2.0);
            let pts: Vec<Point> = [(0.28, 0.12), (0.72, 0.18), (0.42, 0.50), (0.50, 0.53), (0.58, 0.56), (0.32, 0.88), (0.71, 0.86)].iter().map(|(x, y)| Point::new(ox + x * s, oy + y * s)).collect();
            for (a, b) in [(0, 2), (1, 4), (2, 3), (3, 4), (2, 5), (4, 6)] {
                let progress = ((this.intro - 0.15) / 0.85).clamp(0.0, 1.0);
                let end = Point::new(pts[a].x + (pts[b].x - pts[a].x) * progress, pts[a].y + (pts[b].y - pts[a].y) * progress);
                f.stroke(&Path::line(pts[a], end), Stroke::default().with_width(0.8).with_color(t::alpha(t::STAR, 0.16 * this.alpha)));
            }
            for (i, p) in pts.iter().enumerate() {
                let is_belt = (2..=4).contains(&i);
                let col = if i == 0 {
                    t::SIGNAL
                } else if i == 6 {
                    t::N_CONCEITO
                } else if is_belt && this.belt > 0.0 {
                    t::OK
                } else {
                    t::STAR
                };
                let base = if i == 0 || i == 6 { 3.6 } else { 2.0 };
                let r = base * if is_belt && this.belt > 0.0 { 1.0 + this.belt * 0.6 } else { 1.0 };
                let ga = if is_belt && this.belt > 0.0 { 0.25 + this.belt * 0.4 } else { 0.25 };
                let reveal = (this.intro * 1.7 - i as f32 * 0.1).clamp(0.0, 1.0);
                let pulse = (reveal * std::f32::consts::PI).sin().max(0.0);
                glow(f, *p, r * (6.0 + pulse * 2.0), col, ga * this.alpha * reveal);
                for (dx, dy) in [(r * 3.0, 0.0), (0.0, r * 3.0)] {
                    f.stroke(&Path::line(Point::new(p.x - dx, p.y - dy), Point::new(p.x + dx, p.y + dy)), Stroke::default().with_width(0.6).with_color(t::alpha(col, 0.16 * this.alpha * reveal)));
                }
                f.fill(&Path::circle(*p, r), t::alpha(col, 0.95 * this.alpha * reveal));
            }
        }
    }
}

/// Girando `.spin`: anel de 12 px com o topo em destaque.
pub struct Spinner {
    pub angle: f32,
}
impl canvas::Program<Msg> for Spinner {
    type State = ();
    fn draw(&self, _state: &Self::State, renderer: &Renderer, _th: &Theme, bounds: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let c = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let r = bounds.width.min(bounds.height) / 2.0 - 0.75;
        f.stroke(&Path::circle(c, r), Stroke::default().with_width(1.5).with_color(t::LINE3));
        let arc = Path::new(|b| {
            b.arc(canvas::path::Arc { center: c, radius: r, start_angle: iced::Radians(self.angle - 1.2), end_angle: iced::Radians(self.angle + 0.3) });
        });
        f.stroke(&arc, Stroke::default().with_width(1.5).with_color(t::FG).with_line_cap(canvas::LineCap::Round));
        vec![f.into_geometry()]
    }
}
pub fn spinner<'a>(angle: f32, size: f32) -> iced::Element<'a, Msg> {
    iced::widget::canvas(Spinner { angle }).width(size).height(size).into()
}

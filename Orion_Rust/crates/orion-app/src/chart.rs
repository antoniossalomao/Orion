//! Gráfico de linha (§26.3): grade, série com área, ponto final, marca tracejada e rótulos nas pontas.

use crate::app::Msg;
use crate::theme as t;
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Color, Point, Rectangle, Renderer, Theme};

pub struct Line {
    pub vals: Vec<f32>,
    pub min: f32,
    pub max: f32,
    pub ticks: Vec<f32>,
    pub fmt: fn(f32) -> String,
    pub start: &'static str,
    pub end: &'static str,
    pub color: Color,
    pub mark: Option<(usize, &'static str)>,
}

impl canvas::Program<Msg> for Line {
    type State = ();
    fn draw(&self, _s: &(), renderer: &Renderer, _th: &Theme, bounds: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let (w, h) = (bounds.width, bounds.height);
        let (pl, pr, pt, pb) = (44.0, 10.0, 12.0, 22.0);
        let (iw, ih) = (w - pl - pr, h - pt - pb);
        let n = self.vals.len();
        let x = |i: usize| pl + i as f32 * iw / (n - 1) as f32;
        let y = |v: f32| pt + ih - (v - self.min) / (self.max - self.min) * ih;
        let label = |f: &mut Frame, s: String, p: Point, align: iced::alignment::Horizontal, col: Color| {
            f.fill_text(Text { content: s, position: p, color: col, size: 10.5.into(), font: t::UI, align_x: align.into(), align_y: iced::alignment::Vertical::Center, ..Text::default() });
        };
        for tk in &self.ticks {
            f.stroke(&Path::line(Point::new(pl, y(*tk)), Point::new(w - pr, y(*tk))), Stroke::default().with_width(1.0).with_color(t::LINE2));
            label(&mut f, (self.fmt)(*tk), Point::new(pl - 8.0, y(*tk)), iced::alignment::Horizontal::Right, t::FG3);
        }
        if let Some((mi, ml)) = self.mark {
            let mx = x(mi);
            let mut d = 0.0;
            while d < ih {
                f.stroke(&Path::line(Point::new(mx, pt + d), Point::new(mx, (pt + d + 3.0).min(pt + ih))), Stroke::default().with_width(1.0).with_color(t::SIGNAL));
                d += 6.0;
            }
            label(&mut f, ml.to_string(), Point::new(mx + 6.0, pt + 8.0), iced::alignment::Horizontal::Left, t::SIGNAL);
        }
        let pts: Vec<Point> = self.vals.iter().enumerate().map(|(i, v)| Point::new(x(i), y(*v))).collect();
        let area = Path::new(|b| {
            b.move_to(pts[0]);
            for p in &pts[1..] {
                b.line_to(*p);
            }
            b.line_to(Point::new(x(n - 1), pt + ih));
            b.line_to(Point::new(pl, pt + ih));
            b.close();
        });
        f.fill(&area, t::alpha(self.color, 0.08));
        let line = Path::new(|b| {
            b.move_to(pts[0]);
            for p in &pts[1..] {
                b.line_to(*p);
            }
        });
        f.stroke(&line, Stroke::default().with_width(1.6).with_color(self.color).with_line_join(canvas::LineJoin::Round).with_line_cap(canvas::LineCap::Round));
        let last = pts[n - 1];
        f.fill(&Path::circle(last, 3.5), t::BG);
        f.stroke(&Path::circle(last, 3.5), Stroke::default().with_width(1.6).with_color(self.color));
        label(&mut f, self.start.to_string(), Point::new(pl, h - 8.0), iced::alignment::Horizontal::Left, t::FG3);
        label(&mut f, self.end.to_string(), Point::new(w - pr, h - 8.0), iced::alignment::Horizontal::Right, t::FG3);
        vec![f.into_geometry()]
    }
}

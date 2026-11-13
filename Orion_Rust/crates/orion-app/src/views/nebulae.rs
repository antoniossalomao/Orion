//! Fundo da tela inicial: espaço escuro e discreto (pedido do Antônio em 13/11/2026, no lugar da
//! constelação). Preto-azulado com leve degradê, nebulosas desfocadas e estrelas, sem linhas.
//! Animação bem leve: as nebulosas respiram e derivam devagar (ciclos de 40 a 70 s) e poucas
//! estrelas cintilam. Céu e estrelas fixas ficam em cache; só as nebulosas e o brilho mudam.
use iced::mouse;
use iced::widget::canvas::{self, gradient, Cache, Fill, Frame, Geometry, Path};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use std::f32::consts::TAU;

/// Céu: topo e base do degradê (mais escuro que o `BG` do tema, puxado para o azul).
const TOP: Color = Color::from_rgb(0.035, 0.040, 0.065);
const BASE: Color = Color::from_rgb(0.020, 0.022, 0.035);

/// (x, y, raio relativo ao menor lado, cor, intensidade, período em segundos)
const NEBULAE: [(f32, f32, f32, (u8, u8, u8), f32, f32); 4] = [
    (0.20, 0.25, 0.60, (70, 95, 190), 0.07, 61.0),    // azul profundo, alto à esquerda
    (0.85, 0.40, 0.50, (120, 80, 180), 0.055, 47.0),  // violeta à direita
    (0.55, 0.95, 0.65, (40, 110, 150), 0.05, 71.0),   // azul-petróleo embaixo
    (0.50, 0.48, 0.28, (120, 140, 220), 0.025, 39.0), // halo leve atrás da esfera
];

const STARS: usize = 170;
const TWINKLING: usize = 14;

/// `t`: segundos desde a abertura do app (fica parado com movimento reduzido).
pub struct Nebulae {
    pub t: f32,
}

#[derive(Default)]
pub struct Layers {
    sky: Cache,
    stars: Cache,
}

fn rnd(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*seed >> 33) as f32) / (u32::MAX >> 1) as f32
}

impl<Message> canvas::Program<Message> for Nebulae {
    type State = Layers;

    fn draw(&self, c: &Layers, r: &Renderer, _t: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let sky = c.sky.draw(r, b.size(), |f| {
            let g = gradient::Linear::new(Point::ORIGIN, Point::new(0.0, b.height)).add_stop(0.0, TOP).add_stop(1.0, BASE);
            f.fill_rectangle(Point::ORIGIN, b.size(), Fill::from(g));
        });

        // nebulosas: deriva de ~2% do lado e respiração de ±12% na intensidade
        let mut clouds = Frame::new(r, b.size());
        let side = b.width.min(b.height);
        for (i, (x, y, radius, (red, green, blue), strength, period)) in NEBULAE.into_iter().enumerate() {
            let phase = self.t / period * TAU + i as f32 * 1.7;
            let center = Point::new(b.width * x + side * 0.02 * phase.sin(), b.height * y + side * 0.015 * (phase * 0.8).cos());
            let radius = side * radius * (1.0 + 0.03 * (phase * 1.3).sin());
            let strength = strength * (1.0 + 0.12 * (phase * 0.9).sin());
            // desfoque: muitos círculos concêntricos de alfa baixo (perfil quase gaussiano)
            const STEPS: usize = 36;
            for k in 0..STEPS {
                let k = 1.0 - k as f32 / STEPS as f32;
                let a = strength / STEPS as f32 * 2.2 * (1.0 - k * k * 0.4);
                clouds.fill(&Path::circle(center, radius * k), Color::from_rgba8(red, green, blue, a));
            }
        }

        // estrelas paradas, sempre nas mesmas posições (semente fixa); poucas com brilho maior
        let stars = c.stars.draw(r, b.size(), |f| {
            let mut seed: u64 = 20261113;
            for _ in 0..STARS {
                let (x, y, v) = (rnd(&mut seed) * b.width, rnd(&mut seed) * b.height, rnd(&mut seed));
                let glow = 0.10 + 0.35 * v * v * v;
                let px_size = if v > 0.97 { 1.8 } else { 1.0 };
                let color = if v > 0.9 { Color::from_rgba(0.85, 0.89, 1.0, glow) } else { Color::from_rgba(0.75, 0.80, 0.95, glow) };
                if v > 0.97 {
                    f.fill(&Path::circle(Point::new(x, y), 4.0), Color::from_rgba(0.7, 0.78, 1.0, 0.04));
                }
                f.fill_rectangle(Point::new(x, y), Size::new(px_size, px_size), color);
            }
        });

        // cintilantes: poucas estrelas que acendem e apagam devagar, cada uma no seu ritmo
        let mut glow = Frame::new(r, b.size());
        let mut seed: u64 = 7_051_113;
        for _ in 0..TWINKLING {
            let (x, y) = (rnd(&mut seed) * b.width, rnd(&mut seed) * b.height);
            let (period, phase) = (5.0 + rnd(&mut seed) * 9.0, rnd(&mut seed) * TAU);
            let s = ((self.t / period * TAU + phase).sin() * 0.5 + 0.5).powi(3);
            let a = 0.05 + 0.45 * s;
            glow.fill(&Path::circle(Point::new(x, y), 3.5), Color::from_rgba(0.7, 0.78, 1.0, 0.05 * s));
            glow.fill_rectangle(Point::new(x, y), Size::new(1.4, 1.4), Color::from_rgba(0.88, 0.92, 1.0, a));
        }

        vec![sky, clouds.into_geometry(), stars, glow.into_geometry()]
    }
}

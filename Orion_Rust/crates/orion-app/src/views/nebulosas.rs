//! Fundo da tela inicial: espaço escuro, estático e discreto (pedido do Antônio em 13/11/2026, no
//! lugar da constelação animada). Preto-azulado com leve degradê, nebulosas desfocadas e estrelas
//! paradas, sem linhas. Desenha uma vez por tamanho de janela.
use iced::mouse;
use iced::widget::canvas::{self, gradient, Cache, Fill, Geometry, Path};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};

/// Céu: topo e base do degradê (mais escuro que o `BG` do tema, puxado para o azul).
const TOPO: Color = Color::from_rgb(0.035, 0.040, 0.065);
const BASE: Color = Color::from_rgb(0.020, 0.022, 0.035);

/// (x, y, raio relativo ao menor lado, cor, intensidade)
const NEBULOSAS: [(f32, f32, f32, (u8, u8, u8), f32); 4] = [
    (0.20, 0.25, 0.60, (70, 95, 190), 0.07),   // azul profundo, alto à esquerda
    (0.85, 0.40, 0.50, (120, 80, 180), 0.055), // violeta à direita
    (0.55, 0.95, 0.65, (40, 110, 150), 0.05),  // azul-petróleo embaixo
    (0.50, 0.48, 0.28, (120, 140, 220), 0.025), // halo leve atrás da esfera
];

const ESTRELAS: usize = 170;

pub struct Nebulosas;

impl<Message> canvas::Program<Message> for Nebulosas {
    type State = Cache;

    fn draw(&self, cache: &Cache, r: &Renderer, _t: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        vec![cache.draw(r, b.size(), |f| {
            let ceu = gradient::Linear::new(Point::ORIGIN, Point::new(0.0, b.height)).add_stop(0.0, TOPO).add_stop(1.0, BASE);
            f.fill_rectangle(Point::ORIGIN, b.size(), Fill::from(ceu));

            let lado = b.width.min(b.height);
            for (x, y, raio, (cr, cg, cb), forca) in NEBULOSAS {
                let centro = Point::new(b.width * x, b.height * y);
                let raio = lado * raio;
                // desfoque: muitos círculos concêntricos de alfa baixo (perfil quase gaussiano)
                const PASSOS: usize = 36;
                for i in 0..PASSOS {
                    let k = 1.0 - i as f32 / PASSOS as f32;
                    let a = forca / PASSOS as f32 * 2.2 * (1.0 - k * k * 0.4);
                    f.fill(&Path::circle(centro, raio * k), Color::from_rgba8(cr, cg, cb, a));
                }
            }

            // estrelas paradas, sempre nas mesmas posições (semente fixa); poucas com brilho maior
            let mut seed: u64 = 20261113;
            let mut rnd = || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((seed >> 33) as f32) / (u32::MAX >> 1) as f32
            };
            for _ in 0..ESTRELAS {
                let (x, y, v) = (rnd() * b.width, rnd() * b.height, rnd());
                let brilho = 0.10 + 0.35 * v * v * v;
                let lado_px = if v > 0.97 { 1.8 } else { 1.0 };
                let cor = if v > 0.9 { Color::from_rgba(0.85, 0.89, 1.0, brilho) } else { Color::from_rgba(0.75, 0.80, 0.95, brilho) };
                if v > 0.97 {
                    f.fill(&Path::circle(Point::new(x, y), 4.0), Color::from_rgba(0.7, 0.78, 1.0, 0.04));
                }
                f.fill_rectangle(Point::new(x, y), Size::new(lado_px, lado_px), cor);
            }
        })]
    }
}

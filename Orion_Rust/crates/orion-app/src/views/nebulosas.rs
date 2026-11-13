//! Fundo da tela inicial: cor sólida com nebulosas suaves e desfocadas (pedido do Antônio em
//! 13/11/2026, no lugar da constelação animada). Estático: desenha uma vez por tamanho de janela.
use crate::theme as t;
use iced::mouse;
use iced::widget::canvas::{self, Cache, Geometry, Path};
use iced::{Color, Point, Rectangle, Renderer, Theme};

/// (x, y, raio relativo ao menor lado, cor, intensidade)
const NEBULOSAS: [(f32, f32, f32, (u8, u8, u8), f32); 4] = [
    (0.22, 0.28, 0.55, (110, 128, 196), 0.05), // azul Rigel, alto à esquerda
    (0.82, 0.36, 0.48, (150, 120, 196), 0.04), // violeta à direita
    (0.55, 0.92, 0.60, (70, 120, 150), 0.035),  // azul-petróleo embaixo
    (0.50, 0.50, 0.30, (140, 155, 210), 0.02), // brilho leve no centro, atrás da esfera
];

pub struct Nebulosas;

impl<Message> canvas::Program<Message> for Nebulosas {
    type State = Cache;

    fn draw(&self, cache: &Cache, r: &Renderer, _t: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        vec![cache.draw(r, b.size(), |f| {
            f.fill_rectangle(Point::ORIGIN, b.size(), t::BG);
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
        })]
    }
}

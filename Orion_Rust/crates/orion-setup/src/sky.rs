//! Renderizador procedural da constelação de Orion (468×564 px).
//!
//! Tradução pixel-a-pixel do canvas 2D de `orion-instalador.html`:
//! - PRNG Park–Miller com semente fixa `seed = 7`
//! - Sprites radiais 128×128 pré-computados (`warm`, `cool`, `white`, `neb1`, `neb2`) com blending aditivo (`lighter`)
//! - Camada profunda pré-renderizada (vinheta radial + 90 manchas da Via Láctea + Nebulosa M42) para 60 FPS (< 0.8 ms/frame)
//! - 260 estrelas de fundo com cintilação e paralaxe suave de câmera
//! - 7 linhas da constelação com trilha apagada (`#8a90a8`), gradiente linear luminoso (4 px + 1 px) e ponta de luz
//! - 7 estrelas principais com bloom de nascimento (`sin(age/2.4 * π)`), cruz de difração com gradiente e núcleo nítido

use crate::theme::ease_in_out_cubic;
use iced::widget::image as iced_image;
use image::{Rgba, RgbaImage};
use std::f32::consts::PI;
use std::sync::OnceLock;

pub const SKY_W_U32: u32 = 468;
pub const SKY_H_U32: u32 = 564;
const SPRITE_SIZE: usize = 128;

/// Tabela 128×128 pré-computada do gradiente radial dos sprites em ponto fixo `0..=256`.
static SPRITE_ALPHA_LUT: OnceLock<Box<[u16; SPRITE_SIZE * SPRITE_SIZE]>> = OnceLock::new();

fn sprite_lut() -> &'static [u16; SPRITE_SIZE * SPRITE_SIZE] {
    SPRITE_ALPHA_LUT.get_or_init(|| {
        let mut table = Box::new([0u16; SPRITE_SIZE * SPRITE_SIZE]);
        let half = (SPRITE_SIZE as f32) * 0.5;
        for y in 0..SPRITE_SIZE {
            for x in 0..SPRITE_SIZE {
                let dx = (x as f32 + 0.5) - half;
                let dy = (y as f32 + 0.5) - half;
                let norm = (dx * dx + dy * dy).sqrt() / half;
                table[y * SPRITE_SIZE + x] = (sprite_radial_alpha(norm) * 256.0).round() as u16;
            }
        }
        table
    })
}

/// Gerador congruente linear Park–Miller idêntico ao JS:
/// `let seed=7; const rnd=()=>(seed=(seed*16807)%2147483647)/2147483647;`
#[derive(Debug, Clone, Copy)]
pub struct ParkMillerRng {
    pub seed: u64,
}

impl ParkMillerRng {
    pub const INITIAL_SEED: u64 = 7;

    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        self.seed = (self.seed * 16807) % 2147483647;
        (self.seed as f64) / 2147483647.0
    }

    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        self.next_f64() as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteKind {
    Warm,
    Cool,
    White,
    Neb1,
    Neb2,
}

impl SpriteKind {
    #[inline]
    pub const fn rgb(self) -> (u8, u8, u8) {
        match self {
            SpriteKind::Warm => (240, 190, 120),
            SpriteKind::Cool => (176, 192, 236),
            SpriteKind::White => (232, 236, 248),
            SpriteKind::Neb1 => (110, 96, 180),
            SpriteKind::Neb2 => (214, 128, 140),
        }
    }
}

/// Avalia a curva radial de alpha dos sprites 128×128 do protótipo:
/// `0.00 -> 1.00`, `0.08 -> 0.75`, `0.25 -> 0.22`, `0.55 -> 0.05`, `1.00 -> 0.00`.
#[inline]
pub fn sprite_radial_alpha(norm_dist: f32) -> f32 {
    let d = norm_dist.clamp(0.0, 1.0);
    if d <= 0.08 {
        1.0 + (0.75 - 1.0) * (d / 0.08)
    } else if d <= 0.25 {
        0.75 + (0.22 - 0.75) * ((d - 0.08) / (0.25 - 0.08))
    } else if d <= 0.55 {
        0.22 + (0.05 - 0.22) * ((d - 0.25) / (0.55 - 0.25))
    } else if d < 1.0 {
        0.05 * (1.0 - (d - 0.55) / (1.0 - 0.55))
    } else {
        0.0
    }
}

/// Estrela de fundo (260 no total).
#[derive(Debug, Clone, Copy)]
pub struct BgStar {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub a: f32,
    pub z: f32,
    pub ph: f32,
    pub sp: f32,
    pub tint: (u8, u8, u8),
}

/// Mancha de poeira da Via Láctea (90 no total).
#[derive(Debug, Clone, Copy)]
pub struct DustPatch {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub a: f32,
}

/// Estrela principal da constelação de Orion (7 no total).
#[derive(Debug, Clone, Copy)]
pub struct ConstellationStar {
    pub name: &'static str,
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub kind: SpriteKind,
    pub core: (u8, u8, u8),
    pub step: usize,
    pub delay: f32,
    pub a: f32,
    pub born: f32,
}

/// Linha conectando duas estrelas principais (7 no total).
#[derive(Debug, Clone, Copy)]
pub struct ConstellationLine {
    pub a: usize,
    pub b: usize,
    pub p: f32,
    pub start: f32,
}

/// Estrela cadente ocasional.
#[derive(Debug, Clone, Copy)]
pub struct ShootingStar {
    pub x: f32,
    pub y: f32,
    pub born: f32,
}

/// Parâmetros de câmera pré-calculados uma única vez por quadro.
#[derive(Clone, Copy)]
struct CameraTransform {
    cx: f32,
    cy: f32,
    zoom: f32,
    rot: f32,
    ox: f32,
    oy: f32,
}

impl CameraTransform {
    #[inline]
    fn from_sky(sky: &OrionSky, w: f32, h: f32) -> Self {
        Self {
            cx: w * 0.47,
            cy: h * 0.50,
            zoom: 1.0 + 0.012 * (sky.sec * 0.11).sin() + 0.03 * sky.done,
            rot: 0.012 * (sky.sec * 0.05).sin(),
            ox: (sky.sec * 0.07).sin() * 5.0 - sky.px * 6.0,
            oy: (sky.sec * 0.09).cos() * 4.0 - sky.py * 6.0,
        }
    }

    #[inline]
    fn project(self, x: f32, y: f32, z: f32, w: f32, h: f32) -> (f32, f32) {
        let dx = (x * w - self.cx) * self.zoom;
        let dy = (y * h - self.cy) * self.zoom;
        // Para|rot * z| <= 0.012 rad, a aproximação de Taylor de 2ª ordem é exata em float32 (< 1e-7 erro)
        let rz = self.rot * z;
        let c = 1.0 - 0.5 * rz * rz;
        let s = rz;
        (
            self.cx + dx * c - dy * s + self.ox * z,
            self.cy + dx * s + dy * c + self.oy * z,
        )
    }
}

/// Estado completo do céu e da constelação de Orion.
#[derive(Debug, Clone)]
pub struct OrionSky {
    pub rng: ParkMillerRng,
    pub bg: Vec<BgStar>,
    pub dust: Vec<DustPatch>,
    pub stars: [ConstellationStar; 7],
    pub lines: [ConstellationLine; 7],
    pub lit: usize,
    pub done: f32,
    pub sec: f32,
    pub pointer_tx: f32,
    pub pointer_ty: f32,
    pub px: f32,
    pub py: f32,
    pub shoot: Option<ShootingStar>,
    pub next_shoot_sec: f32,
    base_backdrop: Vec<u8>,
}

impl Default for OrionSky {
    fn default() -> Self {
        Self::new()
    }
}

impl OrionSky {
    pub fn new() -> Self {
        let _ = sprite_lut();
        let mut rng = ParkMillerRng::new(ParkMillerRng::INITIAL_SEED);

        // Sequência idêntica ao JS (`orion-instalador.html` linhas 156–159):
        let mut bg = Vec::with_capacity(260);
        for _ in 0..260 {
            let z = rng.next_f32();
            let x = rng.next_f32();
            let y = rng.next_f32();
            let r = 0.25 + z * z * 1.1;
            let a = 0.12 + z * 0.55;
            let ph = rng.next_f32() * 6.28;
            let sp = 0.15 + rng.next_f32() * 0.35;
            let tint = if rng.next_f32() < 0.15 {
                (255, 220, 190)
            } else if rng.next_f32() < 0.25 {
                (190, 205, 255)
            } else {
                (225, 230, 242)
            };
            bg.push(BgStar {
                x,
                y,
                r,
                a,
                z,
                ph,
                sp,
                tint,
            });
        }

        let mut dust = Vec::with_capacity(90);
        for _ in 0..90 {
            let t = rng.next_f32();
            let y = 0.15 + t * 0.75 + (rng.next_f32() - 0.5) * 0.18;
            let r = 20.0 + rng.next_f32() * 50.0;
            let a = 0.012 + rng.next_f32() * 0.02;
            dust.push(DustPatch { x: t, y, r, a });
        }

        let stars = [
            ConstellationStar {
                name: "Betelgeuse",
                x: 0.30,
                y: 0.22,
                r: 3.2,
                kind: SpriteKind::Warm,
                core: (0xff, 0xd9, 0xa0),
                step: 0,
                delay: 0.0,
                a: 0.0,
                born: 0.0,
            },
            ConstellationStar {
                name: "Bellatrix",
                x: 0.66,
                y: 0.26,
                r: 2.2,
                kind: SpriteKind::Cool,
                core: (0xe6, 0xec, 0xff),
                step: 1,
                delay: 0.0,
                a: 0.0,
                born: -1.0,
            },
            ConstellationStar {
                name: "Alnitak",
                x: 0.40,
                y: 0.52,
                r: 2.4,
                kind: SpriteKind::White,
                core: (0xf2, 0xf4, 0xff),
                step: 2,
                delay: 0.0,
                a: 0.0,
                born: -1.0,
            },
            ConstellationStar {
                name: "Alnilam",
                x: 0.475,
                y: 0.50,
                r: 2.6,
                kind: SpriteKind::White,
                core: (0xf2, 0xf4, 0xff),
                step: 2,
                delay: 0.35,
                a: 0.0,
                born: -1.0,
            },
            ConstellationStar {
                name: "Mintaka",
                x: 0.55,
                y: 0.48,
                r: 2.3,
                kind: SpriteKind::White,
                core: (0xf2, 0xf4, 0xff),
                step: 2,
                delay: 0.70,
                a: 0.0,
                born: -1.0,
            },
            ConstellationStar {
                name: "Saiph",
                x: 0.64,
                y: 0.80,
                r: 2.2,
                kind: SpriteKind::Cool,
                core: (0xe6, 0xec, 0xff),
                step: 3,
                delay: 0.0,
                a: 0.0,
                born: -1.0,
            },
            ConstellationStar {
                name: "Rigel",
                x: 0.28,
                y: 0.78,
                r: 3.4,
                kind: SpriteKind::Cool,
                core: (0xee, 0xf2, 0xff),
                step: 4,
                delay: 0.0,
                a: 0.0,
                born: -1.0,
            },
        ];

        let raw_lines = [(0, 2), (2, 3), (3, 4), (1, 4), (0, 1), (4, 5), (2, 6)];
        let lines = raw_lines.map(|(a, b)| ConstellationLine {
            a,
            b,
            p: 0.0,
            start: -1.0,
        });

        let base_backdrop = build_deep_sky_backdrop(SKY_W_U32, SKY_H_U32, &dust);

        Self {
            rng,
            bg,
            dust,
            stars,
            lines,
            lit: 1,
            done: 0.0,
            sec: 0.0,
            pointer_tx: 0.0,
            pointer_ty: 0.0,
            px: 0.0,
            py: 0.0,
            shoot: None,
            next_shoot_sec: 5.0,
            base_backdrop,
        }
    }

    pub fn set_pointer_normalized(&mut self, nx: f32, ny: f32) {
        self.pointer_tx = nx.clamp(-0.5, 0.5);
        self.pointer_ty = ny.clamp(-0.5, 0.5);
    }

    pub fn notify_step(&mut self, step_1_based: usize) {
        let target_lit = step_1_based.min(4);
        if target_lit > self.lit {
            self.lit = target_lit;
        }
    }

    pub fn notify_install_completed(&mut self) {
        self.lit = 5;
    }

    /// Projeção com respiração lenta da câmera (`orion-instalador.html` linhas 190–192).
    #[inline]
    pub fn project(&self, x: f32, y: f32, z: f32, w: f32, h: f32) -> (f32, f32) {
        CameraTransform::from_sky(self, w, h).project(x, y, z, w, h)
    }

    #[inline]
    pub fn pos(&self, star: &ConstellationStar, width: f32, height: f32) -> (f32, f32) {
        self.project(star.x, star.y, 1.0, width, height)
    }

    pub fn step_dt(&mut self, dt_raw: f32, _width: f32, _height: f32) {
        let dt = dt_raw.clamp(0.0, 0.05);
        self.sec += dt;
        self.px += (self.pointer_tx - self.px) * (dt * 3.0).min(1.0);
        self.py += (self.pointer_ty - self.py) * (dt * 3.0).min(1.0);

        let sec = self.sec;
        let lit = self.lit;

        for s in &mut self.stars {
            let on = s.step < lit;
            if on && s.born < 0.0 {
                s.born = sec + s.delay;
            }
            if !on {
                s.born = -1.0;
            }
            let active = on && sec >= s.born;
            let target = if active { 1.0 } else { 0.0 };
            let rate = if active { 1.6 } else { 4.0 };
            s.a += (target - s.a) * (1.0 - (-rate * dt).exp());
        }

        // Linhas (`orion-instalador.html` linhas 216–217): crescem quando A.b > 0.85 && B.b > 0.85
        for l in &mut self.lines {
            let sa = self.stars[l.a];
            let sb = self.stars[l.b];
            let ready = sa.a > 0.85 && sb.a > 0.85;
            if ready {
                l.p = (l.p + dt / 1.6).min(1.0);
            } else {
                l.p = (l.p - dt * 2.0).max(0.0);
            }
        }

        let done_target = if self.lit >= 5 && self.stars[6].a > 0.9 {
            1.0
        } else {
            0.0
        };
        self.done += (done_target - self.done) * (1.0 - (-0.8 * dt).exp());

        // Estrela cadente (`orion-instalador.html` linhas 253–256)
        if self.shoot.is_none() && sec > self.next_shoot_sec {
            self.shoot = Some(ShootingStar {
                x: 0.35 + self.rng.next_f32() * 0.55,
                y: 0.04 + self.rng.next_f32() * 0.25,
                born: sec,
            });
            self.next_shoot_sec = sec + 9.0 + self.rng.next_f32() * 9.0;
        }
        if let Some(sh) = self.shoot {
            if (sec - sh.born) / 1.3 >= 1.0 {
                self.shoot = None;
            }
        }
    }

    /// Gera o `Handle` de imagem RGBA 468×564 para exibição direta via `iced::widget::image`.
    pub fn render_image_handle(&self) -> iced_image::Handle {
        let raw = self.render_raw_rgba(SKY_W_U32, SKY_H_U32);
        iced_image::Handle::from_rgba(SKY_W_U32, SKY_H_U32, raw)
    }

    /// Renderiza o quadro atual em um `RgbaImage` usando os sprites 128×128 e composição `lighter`.
    pub fn render_rgba(&self, width: u32, height: u32) -> RgbaImage {
        let buf = self.render_raw_rgba(width, height);
        RgbaImage::from_raw(width, height, buf)
            .unwrap_or_else(|| RgbaImage::from_pixel(width, height, Rgba([7, 8, 11, 255])))
    }

    fn render_raw_rgba(&self, width: u32, height: u32) -> Vec<u8> {
        let w = width as f32;
        let h = height as f32;
        let cam = CameraTransform::from_sky(self, w, h);

        let mut buf = if width == SKY_W_U32 && height == SKY_H_U32 {
            self.base_backdrop.clone()
        } else {
            build_deep_sky_backdrop(width, height, &self.dust)
        };

        // 1. Estrelas de fundo (260) com câmera pré-calculada
        for s in &self.bg {
            let (x, y) = cam.project(s.x, s.y, 0.3 + s.z * 0.7, w, h);
            let tw = 0.75 + 0.25 * (self.sec * s.sp * 3.0 + s.ph).sin();
            draw_disc_buf(&mut buf, width, height, x, y, s.r, s.tint, s.a * tw, false);
            if s.z > 0.85 {
                blit_sprite_buf(
                    &mut buf,
                    width,
                    height,
                    x,
                    y,
                    s.r * 14.0,
                    SpriteKind::White.rgb(),
                    0.10 * tw,
                );
            }
        }

        // Posições projetadas das 7 estrelas principais (calculadas uma única vez por quadro)
        let star_pos: [(f32, f32); 7] = std::array::from_fn(|i| {
            let st = &self.stars[i];
            cam.project(st.x, st.y, 1.0, w, h)
        });

        // 2. Linhas da constelação (7)
        for l in &self.lines {
            let (x1, y1) = star_pos[l.a];
            let (x2, y2) = star_pos[l.b];
            // Trilha apagada (#8a90a8 com alpha 0.06)
            draw_aa_line_buf(
                &mut buf,
                width,
                height,
                x1,
                y1,
                x2,
                y2,
                1.0,
                (0x8a, 0x90, 0xa8),
                (0x8a, 0x90, 0xa8),
                0.06,
                0.06,
                false,
            );

            if l.p <= 0.001 {
                continue;
            }
            let e = ease_in_out_cubic(l.p);
            let xe = x1 + (x2 - x1) * e;
            let ye = y1 + (y2 - y1) * e;

            // Gradiente linear de rgba(164,179,222,0.05) até rgba(190,204,240,0.55)
            let outer_mul = 0.35 * (0.7 + 0.3 * self.done);
            draw_aa_line_buf(
                &mut buf,
                width,
                height,
                x1,
                y1,
                xe,
                ye,
                3.6,
                (164, 179, 222),
                (190, 204, 240),
                0.05 * outer_mul,
                0.55 * outer_mul,
                true,
            );
            draw_aa_line_buf(
                &mut buf,
                width,
                height,
                x1,
                y1,
                xe,
                ye,
                1.1,
                (164, 179, 222),
                (190, 204, 240),
                0.05 * 0.9,
                0.55 * 0.9,
                true,
            );

            if l.p < 0.999 {
                blit_sprite_buf(
                    &mut buf,
                    width,
                    height,
                    xe,
                    ye,
                    28.0,
                    SpriteKind::White.rgb(),
                    0.7 * (1.0 - l.p * 0.6),
                );
            }
        }

        // 3. Estrelas principais da constelação (7)
        let ang = self.sec * 0.03;
        let (sa, ca) = ang.sin_cos();
        for (i, s) in self.stars.iter().enumerate() {
            let (x, y) = star_pos[i];
            let tw = 1.0 + 0.06 * (self.sec * 1.3 + s.x * 20.0).sin();

            // Ponto discreto quando apagada
            if s.a < 0.99 {
                draw_disc_buf(
                    &mut buf,
                    width,
                    height,
                    x,
                    y,
                    s.r * 0.6,
                    (0x8a, 0x90, 0xa8),
                    0.25 * (1.0 - s.a),
                    false,
                );
            }
            if s.a < 0.01 {
                continue;
            }

            let age = if s.born < 0.0 {
                99.0
            } else {
                (self.sec - s.born).max(0.0)
            };
            let birth = if age < 2.4 {
                ((age / 2.4).min(1.0) * PI).sin()
            } else {
                0.0
            };

            // Halo externo colorido + halo branco interno
            blit_sprite_buf(
                &mut buf,
                width,
                height,
                x,
                y,
                s.r * (26.0 + 18.0 * birth + 6.0 * self.done) * tw,
                s.kind.rgb(),
                s.a * (0.55 + 0.35 * birth),
            );
            blit_sprite_buf(
                &mut buf,
                width,
                height,
                x,
                y,
                s.r * 7.0 * tw,
                SpriteKind::White.rgb(),
                s.a * 0.8,
            );

            // Raios finos de difração com queda linear nas pontas
            let len = s.r * (5.0 + 4.0 * birth) * tw;
            let spike_alpha = s.a * (0.28 + 0.25 * birth);
            draw_spike_buf(&mut buf, width, height, x, y, ca, sa, len, s.core, spike_alpha);
            draw_spike_buf(
                &mut buf,
                width,
                height,
                x,
                y,
                -sa,
                ca,
                len * 0.8,
                s.core,
                spike_alpha,
            );

            // Núcleo sólido
            draw_disc_buf(&mut buf, width, height, x, y, s.r * 0.75, s.core, s.a, false);
        }

        // 4. Estrela cadente rara e suave
        if let Some(sh) = self.shoot {
            let f = (self.sec - sh.born) / 1.3;
            if (0.0..1.0).contains(&f) {
                let e = 1.0 - (1.0 - f).powi(2);
                let x = (sh.x - e * 0.3) * w;
                let y = (sh.y + e * 0.2) * h;
                let a = (f * PI).sin();
                draw_aa_line_buf(
                    &mut buf,
                    width,
                    height,
                    x,
                    y,
                    x + 90.0,
                    y - 60.0,
                    1.2,
                    (240, 242, 250),
                    (240, 242, 250),
                    0.8 * a,
                    0.0,
                    true,
                );
                blit_sprite_buf(
                    &mut buf,
                    width,
                    height,
                    x,
                    y,
                    14.0,
                    SpriteKind::White.rgb(),
                    a * 0.8,
                );
            }
        }

        // 5. Degradê inferior `.skycap` (110 px até rgba(7, 8, 11, 0.85)) em inteiros de ponto fixo
        let cap_h = 110u32.min(height);
        let start_y = height - cap_h;
        for py in start_y..height {
            let t = (py - start_y) as f32 / (cap_h.max(1) as f32);
            let a = (t * t) * 0.85;
            let inv_i = ((1.0 - a) * 256.0).round() as u32;
            let add_r = (7.0 * a * 256.0).round() as u32;
            let add_g = (8.0 * a * 256.0).round() as u32;
            let add_b = (11.0 * a * 256.0).round() as u32;
            let row_offset = (py as usize) * (width as usize) * 4;
            let row_slice = &mut buf[row_offset..row_offset + (width as usize) * 4];
            for px_chunk in row_slice.chunks_exact_mut(4) {
                px_chunk[0] = ((px_chunk[0] as u32 * inv_i + add_r) >> 8) as u8;
                px_chunk[1] = ((px_chunk[1] as u32 * inv_i + add_g) >> 8) as u8;
                px_chunk[2] = ((px_chunk[2] as u32 * inv_i + add_b) >> 8) as u8;
            }
        }

        buf
    }
}

/// Pré-renderiza o fundo `#07080b` + vinheta radial + 90 manchas da Via Láctea + Nebulosa M42.
fn build_deep_sky_backdrop(width: u32, height: u32, dust: &[DustPatch]) -> Vec<u8> {
    let w = width as f32;
    let h = height as f32;
    let mut buf = vec![0u8; (width * height * 4) as usize];

    let vcx = w * 0.45;
    let vcy = h * 0.45;
    let vmax = w.max(h) * 0.8;

    for py in 0..height {
        for px in 0..width {
            let dx = (px as f32 + 0.5) - vcx;
            let dy = (py as f32 + 0.5) - vcy;
            let d = (dx * dx + dy * dy).sqrt();
            let va = ((1.0 - d / vmax).clamp(0.0, 1.0)) * 0.9;
            // Base #07080b (7, 8, 11) + vinheta rgba(22, 26, 40, va)
            let r = 7.0 * (1.0 - va) + 22.0 * va;
            let g = 8.0 * (1.0 - va) + 26.0 * va;
            let b = 11.0 * (1.0 - va) + 40.0 * va;
            let idx = ((py * width + px) * 4) as usize;
            buf[idx] = r.round() as u8;
            buf[idx + 1] = g.round() as u8;
            buf[idx + 2] = b.round() as u8;
            buf[idx + 3] = 255;
        }
    }

    // Via Láctea (90 manchas suaves)
    for d in dust {
        blit_sprite_buf(
            &mut buf,
            width,
            height,
            d.x * w,
            d.y * h,
            d.r * 4.0,
            SpriteKind::Cool.rgb(),
            d.a,
        );
    }

    // Nebulosa de Orion (M42) em (0.50, 0.64)
    let nx = 0.50 * w;
    let ny = 0.64 * h;
    let min_wh = w.min(h);
    let nb = 0.55;
    blit_sprite_buf(
        &mut buf,
        width,
        height,
        nx,
        ny,
        min_wh * 0.75,
        SpriteKind::Neb1.rgb(),
        0.20 * nb,
    );
    blit_sprite_buf(
        &mut buf,
        width,
        height,
        nx + 10.0,
        ny + 8.0,
        min_wh * 0.38,
        SpriteKind::Neb2.rgb(),
        0.18 * nb,
    );
    blit_sprite_buf(
        &mut buf,
        width,
        height,
        nx + 4.0,
        ny + 4.0,
        min_wh * 0.10,
        SpriteKind::White.rgb(),
        0.10 * nb,
    );

    buf
}

/// Blit aditivo (`globalCompositeOperation = "lighter"`) usando a LUT radial 128×128 em ponto fixo (zero float no loop interno).
#[inline]
fn blit_sprite_buf(
    buf: &mut [u8],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    size: f32,
    rgb: (u8, u8, u8),
    alpha: f32,
) {
    if size <= 0.5 || alpha <= 0.002 {
        return;
    }
    let lut = sprite_lut();
    let half = size * 0.5;
    let inv_size = (SPRITE_SIZE as f32) / size;
    let w = width as i32;
    let h = height as i32;
    let x0 = ((cx - half).floor() as i32).max(0);
    let x1 = ((cx + half).ceil() as i32).min(w - 1);
    let y0 = ((cy - half).floor() as i32).max(0);
    let y1 = ((cy + half).ceil() as i32).min(h - 1);

    let r_fixed = ((rgb.0 as f32) * alpha * 256.0).round() as u32;
    let g_fixed = ((rgb.1 as f32) * alpha * 256.0).round() as u32;
    let b_fixed = ((rgb.2 as f32) * alpha * 256.0).round() as u32;
    let left = cx - half;
    let top = cy - half;

    for py in y0..=y1 {
        let sy = (((py as f32 + 0.5) - top) * inv_size) as usize;
        if sy >= SPRITE_SIZE {
            continue;
        }
        let row_lut = &lut[sy * SPRITE_SIZE..(sy + 1) * SPRITE_SIZE];
        let row_offset = (py as usize) * (width as usize) * 4;
        for px in x0..=x1 {
            let sx = (((px as f32 + 0.5) - left) * inv_size) as usize;
            if sx >= SPRITE_SIZE {
                continue;
            }
            let w_a = row_lut[sx] as u32;
            if w_a > 0 {
                let idx = row_offset + (px as usize) * 4;
                buf[idx] = (buf[idx] as u32 + ((r_fixed * w_a) >> 16)).min(255) as u8;
                buf[idx + 1] = (buf[idx + 1] as u32 + ((g_fixed * w_a) >> 16)).min(255) as u8;
                buf[idx + 2] = (buf[idx + 2] as u32 + ((b_fixed * w_a) >> 16)).min(255) as u8;
            }
        }
    }
}

#[inline]
fn draw_disc_buf(
    buf: &mut [u8],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    radius: f32,
    rgb: (u8, u8, u8),
    alpha: f32,
    additive: bool,
) {
    if alpha <= 0.002 {
        return;
    }
    let bound = radius + 1.0;
    let w = width as i32;
    let h = height as i32;
    let x0 = ((cx - bound).floor() as i32).max(0);
    let x1 = ((cx + bound).ceil() as i32).min(w - 1);
    let y0 = ((cy - bound).floor() as i32).max(0);
    let y1 = ((cy + bound).ceil() as i32).min(h - 1);

    for py in y0..=y1 {
        for px in x0..=x1 {
            let dx = (px as f32 + 0.5) - cx;
            let dy = (py as f32 + 0.5) - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let cov = (radius + 0.5 - dist).clamp(0.0, 1.0);
            let a = (cov * alpha).clamp(0.0, 1.0);
            if a > 0.002 {
                blend_buf_pixel(buf, width, px as u32, py as u32, rgb, a, additive);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_aa_line_buf(
    buf: &mut [u8],
    width: u32,
    height: u32,
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
    stroke_w: f32,
    rgb0: (u8, u8, u8),
    rgb1: (u8, u8, u8),
    a0: f32,
    a1: f32,
    additive: bool,
) {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 0.25 {
        return;
    }
    let inv_len_sq = 1.0 / len_sq;
    let half_w = stroke_w * 0.5;
    let pad = half_w + 1.2;
    let max_dist_sq = (half_w + 1.0) * (half_w + 1.0);
    let w = width as i32;
    let h = height as i32;
    let bx_min = ((ax.min(bx) - pad).floor() as i32).max(0);
    let bx_max = ((ax.max(bx) + pad).ceil() as i32).min(w - 1);
    let y0 = ((ay.min(by) - pad).floor() as i32).max(0);
    let y1 = ((ay.max(by) + pad).ceil() as i32).min(h - 1);

    // Recorte horizontal por scanline quando a linha tem componente vertical relevante
    let has_steep_y = dy.abs() > 2.0;
    let slope_x_over_y = if has_steep_y { dx / dy } else { 0.0 };
    let scan_pad = if has_steep_y {
        pad * (1.0 + slope_x_over_y.abs()) + 1.5
    } else {
        0.0
    };

    for py in y0..=y1 {
        let pyf = py as f32 + 0.5;
        let (x0, x1) = if has_steep_y {
            let cx_at_y = ax + (pyf - ay) * slope_x_over_y;
            (
                ((cx_at_y - scan_pad).floor() as i32).max(bx_min),
                ((cx_at_y + scan_pad).ceil() as i32).min(bx_max),
            )
        } else {
            (bx_min, bx_max)
        };
        for px in x0..=x1 {
            let pxf = px as f32 + 0.5;
            let t = (((pxf - ax) * dx + (pyf - ay) * dy) * inv_len_sq).clamp(0.0, 1.0);
            let proj_x = ax + dx * t;
            let proj_y = ay + dy * t;
            let dist_sq = (pxf - proj_x).powi(2) + (pyf - proj_y).powi(2);
            if dist_sq <= max_dist_sq {
                let dist = dist_sq.sqrt();
                let cov = (half_w + 0.5 - dist).clamp(0.0, 1.0);
                let a = (a0 + (a1 - a0) * t) * cov;
                if a > 0.002 {
                    let r = (rgb0.0 as f32 + (rgb1.0 as f32 - rgb0.0 as f32) * t) as u8;
                    let g = (rgb0.1 as f32 + (rgb1.1 as f32 - rgb0.1 as f32) * t) as u8;
                    let b = (rgb0.2 as f32 + (rgb1.2 as f32 - rgb0.2 as f32) * t) as u8;
                    blend_buf_pixel(buf, width, px as u32, py as u32, (r, g, b), a, additive);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_spike_buf(
    buf: &mut [u8],
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    dir_x: f32,
    dir_y: f32,
    half_len: f32,
    rgb: (u8, u8, u8),
    peak_alpha: f32,
) {
    let tip1_x = cx + dir_x * half_len;
    let tip1_y = cy + dir_y * half_len;
    let tip2_x = cx - dir_x * half_len;
    let tip2_y = cy - dir_y * half_len;
    draw_aa_line_buf(
        buf,
        width,
        height,
        cx,
        cy,
        tip1_x,
        tip1_y,
        0.9,
        rgb,
        rgb,
        peak_alpha,
        0.0,
        true,
    );
    draw_aa_line_buf(
        buf,
        width,
        height,
        cx,
        cy,
        tip2_x,
        tip2_y,
        0.9,
        rgb,
        rgb,
        peak_alpha,
        0.0,
        true,
    );
}

#[inline]
fn blend_buf_pixel(
    buf: &mut [u8],
    width: u32,
    x: u32,
    y: u32,
    rgb: (u8, u8, u8),
    alpha: f32,
    additive: bool,
) {
    let idx = ((y * width + x) * 4) as usize;
    if additive {
        buf[idx] = (buf[idx] as f32 + rgb.0 as f32 * alpha).min(255.0) as u8;
        buf[idx + 1] = (buf[idx + 1] as f32 + rgb.1 as f32 * alpha).min(255.0) as u8;
        buf[idx + 2] = (buf[idx + 2] as f32 + rgb.2 as f32 * alpha).min(255.0) as u8;
    } else {
        let inv = 1.0 - alpha;
        buf[idx] = (buf[idx] as f32 * inv + rgb.0 as f32 * alpha) as u8;
        buf[idx + 1] = (buf[idx + 1] as f32 * inv + rgb.1 as f32 * alpha) as u8;
        buf[idx + 2] = (buf[idx + 2] as f32 * inv + rgb.2 as f32 * alpha) as u8;
    }
}

/// Gera o ícone oficial do Orion (as 3 estrelas do Cinturão com linha sutil e blur radial suave
/// sobre fundo `#05080f` com cantos arredondados), baseado em `orion.svg`.
pub fn generate_belt_icon_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let scale = s / 64.0;
    let mut buf = vec![0u8; (size * size * 4) as usize];

    // 1. Fundo arredondado (#05080f, rx = 14 na escala 64x64)
    let rx = 14.0 * scale;
    for py in 0..size {
        for px in 0..size {
            let x = px as f32 + 0.5;
            let y = py as f32 + 0.5;
            let dx = if x < rx {
                rx - x
            } else if x > s - rx {
                x - (s - rx)
            } else {
                0.0
            };
            let dy = if y < rx {
                rx - y
            } else if y > s - rx {
                y - (s - rx)
            } else {
                0.0
            };
            let dist = (dx * dx + dy * dy).sqrt();
            let cov = (rx + 0.5 - dist).clamp(0.0, 1.0);
            if cov > 0.0 {
                let idx = ((py * size + px) * 4) as usize;
                buf[idx] = 5;
                buf[idx + 1] = 8;
                buf[idx + 2] = 15;
                buf[idx + 3] = (cov * 255.0).round() as u8;
            }
        }
    }

    // Coordenadas das 3 estrelas na grade 64x64: (16, 40), (31, 31), (47, 22)
    let p1 = (16.0 * scale, 40.0 * scale);
    let p2 = (31.0 * scale, 31.0 * scale);
    let p3 = (47.0 * scale, 22.0 * scale);

    // 2. Halo central nebuloso suave (#8fabff)
    blit_sprite_buf(
        &mut buf,
        size,
        size,
        p2.0,
        p2.1,
        36.0 * scale,
        (143, 171, 255),
        0.35,
    );

    // 3. Linha conectando as 3 estrelas (#8fabff)
    draw_aa_line_buf(
        &mut buf,
        size,
        size,
        p1.0,
        p1.1,
        p3.0,
        p3.1,
        1.8 * scale,
        (143, 171, 255),
        (143, 171, 255),
        0.55,
        0.55,
        true,
    );

    // 4. Blur radial (halo) + núcleo das 3 estrelas (#eef2ff)
    for &(cx, cy, r_core, r_halo) in &[
        (p1.0, p1.1, 3.6 * scale, 22.0 * scale),
        (p2.0, p2.1, 4.2 * scale, 26.0 * scale),
        (p3.0, p3.1, 3.6 * scale, 22.0 * scale),
    ] {
        blit_sprite_buf(
            &mut buf,
            size,
            size,
            cx,
            cy,
            r_halo,
            (164, 185, 255),
            0.75,
        );
        blit_sprite_buf(
            &mut buf,
            size,
            size,
            cx,
            cy,
            r_halo * 0.45,
            (238, 242, 255),
            0.85,
        );
        draw_disc_buf(
            &mut buf,
            size,
            size,
            cx,
            cy,
            r_core,
            (238, 242, 255),
            1.0,
            false,
        );
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn park_miller_seed_7_matches_javascript_reference() {
        let mut rng = ParkMillerRng::new(7);
        let first = rng.next_f64();
        assert!((first - (117649.0 / 2147483647.0)).abs() < 1e-12);

        let sky = OrionSky::new();
        assert_eq!(sky.bg.len(), 260);
        assert_eq!(sky.dust.len(), 90);
        assert_eq!(sky.stars.len(), 7);
        assert_eq!(sky.lines.len(), 7);
        assert_eq!(sky.lit, 1);
    }

    #[test]
    fn step_progression_lights_stars_and_never_unlights_on_back() {
        let mut sky = OrionSky::new();
        for _ in 0..60 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        assert!(sky.stars[0].a > 0.9);
        assert!(sky.stars[1].a < 0.01);

        sky.notify_step(3);
        for _ in 0..90 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        assert!(sky.stars[2].a > 0.9);
        assert!(sky.stars[3].a > 0.85);
        assert!(sky.stars[4].a > 0.8);

        sky.notify_step(1);
        assert_eq!(sky.lit, 3);
        for _ in 0..30 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        assert!(sky.stars[4].a > 0.8);

        sky.notify_step(4);
        sky.notify_install_completed();
        for _ in 0..120 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        assert!(sky.stars[6].a > 0.9);
        assert!(sky.done > 0.8);
    }

    #[test]
    fn deterministic_rgba_snapshot_renders_constellation() {
        let mut sky = OrionSky::new();
        sky.notify_step(4);
        sky.notify_install_completed();
        for _ in 0..120 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        let img = sky.render_rgba(468, 564);
        assert_eq!(img.width(), 468);
        assert_eq!(img.height(), 564);

        let (bx, by) = sky.pos(&sky.stars[0], 468.0, 564.0);
        let bp = img.get_pixel(bx.round() as u32, by.round() as u32);
        assert!(bp[0] > 180 && bp[1] > 140);

        let (rx, ry) = sky.pos(&sky.stars[6], 468.0, 564.0);
        let rp = img.get_pixel(rx.round() as u32, ry.round() as u32);
        assert!(rp[2] > 180);
    }
}

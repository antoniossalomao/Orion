//! Renderizador procedural da constelação de Orion (468×564 px).
//!
//! Tradução fiel do canvas 2D de `orion-instalador.html` e de
//! `ORION_INSTALADOR_ESPECIFICACAO.md` (§5):
//! - PRNG Park–Miller com semente fixa `seed = 7`
//! - 260 estrelas de fundo com paralaxe e cintilação senoidal
//! - 90 manchas de poeira da Via Láctea
//! - Nebulosa de Orion (M42) abaixo da espada
//! - 7 estrelas principais com nascimento suave, bloom inicial (2.4 s) e cruz de difração girando
//! - 7 linhas da constelação com crescimento cúbico (1.6 s) e ponta de luz
//! - Estrela cadente ocasional no topo direito

use crate::theme::{self, ease_in_out_cubic};
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use image::{Rgba, RgbaImage};

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
    pub const fn rgb(self) -> (u8, u8, u8) {
        match self {
            SpriteKind::Warm => (240, 190, 120),
            SpriteKind::Cool => (176, 192, 236),
            SpriteKind::White => (232, 236, 248),
            SpriteKind::Neb1 => (110, 96, 180),
            SpriteKind::Neb2 => (214, 128, 140),
        }
    }

    pub fn color_with_alpha(self, alpha: f32) -> Color {
        let (r, g, b) = self.rgb();
        Color::from_rgba8(r, g, b, alpha.clamp(0.0, 1.0))
    }
}

/// Avalia a curva radial de alpha dos sprites 128×128 do protótipo:
/// - `0.00 -> 1.00`
/// - `0.08 -> 0.75`
/// - `0.25 -> 0.22`
/// - `0.55 -> 0.05`
/// - `1.00 -> 0.00`
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
    pub vx: f32,
    pub vy: f32,
    pub born: f32,
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
}

impl Default for OrionSky {
    fn default() -> Self {
        Self::new()
    }
}

impl OrionSky {
    pub fn new() -> Self {
        let mut rng = ParkMillerRng::new(ParkMillerRng::INITIAL_SEED);

        // Sequência idêntica ao JS (incluindo curto-circuito no operador ternário de `tint`):
        // const BG=[...Array(260)].map(()=>{const z=rnd();return{x:rnd(),y:rnd(),r:.25+z*z*1.1,a:.12+z*.55,z,ph:rnd()*6.28,sp:.15+rnd()*.35,
        //   tint:rnd()<.15?"255,220,190":rnd()<.25?"190,205,255":"225,230,242"}});
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

        // const DUST=[...Array(90)].map(()=>{const t=rnd();return{x:t,y:.15+t*.75+(rnd()-.5)*.18,r:20+rnd()*50,a:.012+rnd()*.02}});
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
                born: -1.0,
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

        Self {
            rng,
            bg,
            dust,
            stars,
            lines,
            lit: 1, // Etapa 1 acende Betelgeuse (step == 0 < lit)
            done: 0.0,
            sec: 0.0,
            pointer_tx: 0.0,
            pointer_ty: 0.0,
            px: 0.0,
            py: 0.0,
            shoot: None,
        }
    }

    /// Atualiza o alvo de paralaxe a partir da posição normalizada `(-0.5..=0.5)` do cursor na janela.
    pub fn set_pointer_normalized(&mut self, nx: f32, ny: f32) {
        self.pointer_tx = nx.clamp(-0.5, 0.5);
        self.pointer_ty = ny.clamp(-0.5, 0.5);
    }

    /// Registra avanço para a etapa `step_1_based` (`1..=5`).
    /// Voltar etapas nunca reduz `lit` (as estrelas conquistadas permanecem acesas).
    pub fn notify_step(&mut self, step_1_based: usize) {
        let target_lit = step_1_based.min(4);
        if target_lit > self.lit {
            self.lit = target_lit;
        }
    }

    /// Acende Rigel (`lit = 5`) quando a instalação chega a 100%.
    pub fn notify_install_completed(&mut self) {
        self.lit = 5;
    }

    /// Avança a simulação por `dt` segundos (limitado a `0.05 s` por frame, como no protótipo).
    pub fn step_dt(&mut self, dt_raw: f32, width: f32, height: f32) {
        let dt = dt_raw.clamp(0.0, 0.05);
        self.sec += dt;
        self.px += (self.pointer_tx - self.px) * (dt * 3.0).min(1.0);
        self.py += (self.pointer_ty - self.py) * (dt * 3.0).min(1.0);

        let sec = self.sec;
        let lit = self.lit;

        for s in &mut self.stars {
            let want = s.step < lit;
            if want && s.born < 0.0 {
                s.born = sec + s.delay;
            }
            if !want {
                s.born = -1.0;
            }
            let active = want && sec >= s.born;
            let target = if active { 1.0 } else { 0.0 };
            let rate = if active { 1.6 } else { 4.0 };
            s.a += (target - s.a) * (1.0 - (-dt * rate).exp());
        }

        for l in &mut self.lines {
            let sa = self.stars[l.a];
            let sb = self.stars[l.b];
            let ready = sa.a > 0.35 && sb.a > 0.35;
            if ready && l.start < 0.0 {
                l.start = sec + 0.15;
            }
            if !ready {
                l.start = -1.0;
                l.p = (l.p - dt * 3.0).max(0.0);
            } else if sec >= l.start {
                l.p = ((sec - l.start) / 1.6).min(1.0);
            }
        }

        let done_target = if self.lit >= 5 { 1.0 } else { 0.0 };
        self.done += (done_target - self.done) * (1.0 - (-dt * 1.8).exp());

        // Estrela cadente ocasional
        if self.shoot.is_none() && self.rng.next_f32() < dt * 0.07 {
            self.shoot = Some(ShootingStar {
                x: (0.2 + self.rng.next_f32() * 0.6) * width,
                y: (0.08 + self.rng.next_f32() * 0.35) * height,
                vx: 260.0,
                vy: 150.0,
                born: sec,
            });
        }
        if let Some(sh) = self.shoot {
            let age = sec - sh.born;
            if age > 0.7 {
                self.shoot = None;
            }
        }
    }

    #[inline]
    pub fn drift(&self) -> (f32, f32) {
        (
            (self.sec * 0.05).sin() * 2.5,
            (self.sec * 0.04).cos() * 2.0,
        )
    }

    #[inline]
    pub fn pos(&self, star: &ConstellationStar, width: f32, height: f32) -> (f32, f32) {
        let (drift_x, drift_y) = self.drift();
        (
            star.x * width + drift_x - self.px * 8.0,
            star.y * height + drift_y - self.py * 8.0,
        )
    }

    /// Renderiza o céu determinístico em um buffer RGBA (`image::RgbaImage`) para testes de snapshot ou exportação.
    pub fn render_rgba(&self, width: u32, height: u32) -> RgbaImage {
        let mut img = RgbaImage::new(width, height);
        let w = width as f32;
        let h = height as f32;

        // 1. Gradiente vertical de fundo + radial em (38%, 45%)
        let rcx = w * 0.38;
        let rcy = h * 0.45;
        let rmax = h * 0.65;
        for py in 0..height {
            let ty = (py as f32) / h.max(1.0);
            // #07080b -> #0b0d14 (55%) -> #0f121b (100%)
            let (mut br, mut bg_c, mut bb) = if ty <= 0.55 {
                let u = ty / 0.55;
                (
                    7.0 + (11.0 - 7.0) * u,
                    8.0 + (13.0 - 8.0) * u,
                    11.0 + (20.0 - 11.0) * u,
                )
            } else {
                let u = (ty - 0.55) / 0.45;
                (
                    11.0 + (15.0 - 11.0) * u,
                    13.0 + (18.0 - 13.0) * u,
                    20.0 + (27.0 - 20.0) * u,
                )
            };

            for px in 0..width {
                let dx = (px as f32) - rcx;
                let dy = (py as f32) - rcy;
                let d = (dx * dx + dy * dy).sqrt();
                if d < rmax {
                    let a = 1.0 - d / rmax;
                    // Blending de #1c2030 sobre a base
                    br = br + (28.0 - br) * a * 0.45;
                    bg_c = bg_c + (32.0 - bg_c) * a * 0.45;
                    bb = bb + (48.0 - bb) * a * 0.45;
                }
                img.put_pixel(
                    px,
                    py,
                    Rgba([
                        br.round().clamp(0.0, 255.0) as u8,
                        bg_c.round().clamp(0.0, 255.0) as u8,
                        bb.round().clamp(0.0, 255.0) as u8,
                        255,
                    ]),
                );
            }
        }

        // 2. Poeira da Via Láctea
        for d in &self.dust {
            let cx = d.x * w - self.px * 3.0;
            let cy = d.y * h - self.py * 3.0;
            blend_sprite_rgba(&mut img, cx, cy, d.r, SpriteKind::Cool.rgb(), d.a, true);
        }

        // 3. Nebulosa de Orion (M42) abaixo da espada
        let (drift_x, drift_y) = self.drift();
        let nx = 0.465 * w + drift_x - self.px * 6.0;
        let ny = 0.62 * h + drift_y - self.py * 6.0;
        let neb_a = 0.16 + self.stars[3].a * 0.22 + self.done * 0.12;
        blend_sprite_rgba(&mut img, nx, ny, 75.0, SpriteKind::Neb1.rgb(), neb_a, true);
        blend_sprite_rgba(
            &mut img,
            nx + 6.0,
            ny - 4.0,
            42.0,
            SpriteKind::Neb2.rgb(),
            neb_a * 0.85,
            true,
        );

        // 4. 260 estrelas de fundo
        for b in &self.bg {
            let tw = 0.72 + 0.28 * (self.sec * b.sp + b.ph).sin();
            let x = (b.x * w - self.px * (2.0 + b.z * 5.0)).rem_euclid(w);
            let y = (b.y * h - self.py * (2.0 + b.z * 5.0)).rem_euclid(h);
            blend_disc_rgba(&mut img, x, y, b.r, b.tint, b.a * tw, false);
        }

        // 5. 7 linhas da constelação
        for l in &self.lines {
            if l.p <= 0.001 {
                continue;
            }
            let (ax, ay) = self.pos(&self.stars[l.a], w, h);
            let (bx, by) = self.pos(&self.stars[l.b], w, h);
            let ep = ease_in_out_cubic(l.p);
            let ex = ax + (bx - ax) * ep;
            let ey = ay + (by - ay) * ep;
            let alpha = (0.28 + self.done * 0.18) * self.stars[l.a].a.min(self.stars[l.b].a);
            blend_line_rgba(&mut img, ax, ay, ex, ey, (190, 206, 245), alpha);
            if l.p < 0.999 {
                blend_sprite_rgba(&mut img, ex, ey, 11.0, SpriteKind::Cool.rgb(), 0.55 * (1.0 - l.p), true);
            }
        }

        // 6. 7 estrelas principais
        for (i, s) in self.stars.iter().enumerate() {
            if s.a <= 0.002 {
                continue;
            }
            let (x, y) = self.pos(s, w, h);
            let age = (self.sec - s.born).max(0.0);
            let birth = if age < 2.4 {
                (age / 0.35).min(1.0) * (1.0 - (age - 0.35).max(0.0) / 2.05)
            } else {
                0.0
            };
            let breath = 1.0 + 0.04 * (self.sec * 1.3 + (i as f32) * 1.1).sin();
            let fin = 1.0 + self.done * 0.22;
            let halo = s.r * (9.5 + birth * 7.0) * breath * fin;

            blend_sprite_rgba(
                &mut img,
                x,
                y,
                halo,
                s.kind.rgb(),
                (s.a * (0.78 + birth * 0.35)).min(1.0),
                true,
            );
            blend_sprite_rgba(&mut img, x, y, s.r * 3.2 * fin, s.kind.rgb(), s.a, true);
            blend_disc_rgba(&mut img, x, y, s.r * 0.85, s.core, s.a, true);
        }

        img
    }
}

fn blend_sprite_rgba(
    img: &mut RgbaImage,
    cx: f32,
    cy: f32,
    radius: f32,
    rgb: (u8, u8, u8),
    alpha: f32,
    additive: bool,
) {
    if radius <= 0.1 || alpha <= 0.001 {
        return;
    }
    let w = img.width() as i32;
    let h = img.height() as i32;
    let x0 = ((cx - radius).floor() as i32).max(0);
    let x1 = ((cx + radius).ceil() as i32).min(w - 1);
    let y0 = ((cy - radius).floor() as i32).max(0);
    let y1 = ((cy + radius).ceil() as i32).min(h - 1);

    for py in y0..=y1 {
        for px in x0..=x1 {
            let dx = (px as f32) + 0.5 - cx;
            let dy = (py as f32) + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist <= radius {
                let a = (sprite_radial_alpha(dist / radius) * alpha).clamp(0.0, 1.0);
                if a > 0.001 {
                    blend_pixel(img, px as u32, py as u32, rgb, a, additive);
                }
            }
        }
    }
}

fn blend_disc_rgba(
    img: &mut RgbaImage,
    cx: f32,
    cy: f32,
    radius: f32,
    rgb: (u8, u8, u8),
    alpha: f32,
    additive: bool,
) {
    let r_bound = radius.max(1.0) + 1.0;
    let w = img.width() as i32;
    let h = img.height() as i32;
    let x0 = ((cx - r_bound).floor() as i32).max(0);
    let x1 = ((cx + r_bound).ceil() as i32).min(w - 1);
    let y0 = ((cy - r_bound).floor() as i32).max(0);
    let y1 = ((cy + r_bound).ceil() as i32).min(h - 1);

    for py in y0..=y1 {
        for px in x0..=x1 {
            let dx = (px as f32) + 0.5 - cx;
            let dy = (py as f32) + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let cov = (radius + 0.5 - dist).clamp(0.0, 1.0);
            let a = (cov * alpha).clamp(0.0, 1.0);
            if a > 0.001 {
                blend_pixel(img, px as u32, py as u32, rgb, a, additive);
            }
        }
    }
}

fn blend_line_rgba(
    img: &mut RgbaImage,
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
    rgb: (u8, u8, u8),
    alpha: f32,
) {
    let dx = bx - ax;
    let dy = by - ay;
    let steps = (dx.hypot(dy) * 2.0).ceil() as usize;
    if steps == 0 {
        return;
    }
    let w = img.width() as i32;
    let h = img.height() as i32;
    for i in 0..=steps {
        let t = (i as f32) / (steps as f32);
        let x = ax + dx * t;
        let y = ay + dy * t;
        let px = x.round() as i32;
        let py = y.round() as i32;
        if px >= 0 && px < w && py >= 0 && py < h {
            blend_pixel(img, px as u32, py as u32, rgb, (alpha * 0.5).clamp(0.0, 1.0), false);
        }
    }
}

#[inline]
fn blend_pixel(img: &mut RgbaImage, x: u32, y: u32, rgb: (u8, u8, u8), alpha: f32, additive: bool) {
    let dst = img.get_pixel_mut(x, y);
    if additive {
        dst[0] = ((dst[0] as f32) + (rgb.0 as f32) * alpha).round().min(255.0) as u8;
        dst[1] = ((dst[1] as f32) + (rgb.1 as f32) * alpha).round().min(255.0) as u8;
        dst[2] = ((dst[2] as f32) + (rgb.2 as f32) * alpha).round().min(255.0) as u8;
    } else {
        dst[0] = ((dst[0] as f32) * (1.0 - alpha) + (rgb.0 as f32) * alpha)
            .round()
            .clamp(0.0, 255.0) as u8;
        dst[1] = ((dst[1] as f32) * (1.0 - alpha) + (rgb.1 as f32) * alpha)
            .round()
            .clamp(0.0, 255.0) as u8;
        dst[2] = ((dst[2] as f32) * (1.0 - alpha) + (rgb.2 as f32) * alpha)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
}

/// Desenha um halo radial aproximado em camadas concêntricas no `Frame` do `iced::widget::canvas`.
fn draw_glow(frame: &mut Frame, center: Point, radius: f32, kind: SpriteKind, alpha: f32) {
    if radius <= 0.2 || alpha <= 0.002 {
        return;
    }
    const RINGS: [(f32, f32); 5] = [
        (1.00, 0.03),
        (0.62, 0.07),
        (0.35, 0.16),
        (0.16, 0.36),
        (0.07, 0.55),
    ];
    for (r_frac, a_frac) in RINGS {
        let a = (alpha * a_frac).clamp(0.0, 1.0);
        if a > 0.002 {
            frame.fill(
                &Path::circle(center, radius * r_frac),
                kind.color_with_alpha(a),
            );
        }
    }
}

impl<Message> canvas::Program<Message> for OrionSky {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;

        // 1. Fundo noturno
        frame.fill_rectangle(Point::ORIGIN, Size::new(w, h), theme::SKY_BG);
        draw_glow(
            &mut frame,
            Point::new(w * 0.38, h * 0.45),
            h * 0.65,
            SpriteKind::Cool,
            0.18,
        );

        // 2. Poeira da Via Láctea
        for d in &self.dust {
            let cx = d.x * w - self.px * 3.0;
            let cy = d.y * h - self.py * 3.0;
            draw_glow(
                &mut frame,
                Point::new(cx, cy),
                d.r,
                SpriteKind::Cool,
                d.a * 1.4,
            );
        }

        // 3. Nebulosa de Orion (M42)
        let (drift_x, drift_y) = self.drift();
        let nx = 0.465 * w + drift_x - self.px * 6.0;
        let ny = 0.62 * h + drift_y - self.py * 6.0;
        let neb_a = 0.16 + self.stars[3].a * 0.22 + self.done * 0.12;
        draw_glow(&mut frame, Point::new(nx, ny), 75.0, SpriteKind::Neb1, neb_a);
        draw_glow(
            &mut frame,
            Point::new(nx + 6.0, ny - 4.0),
            42.0,
            SpriteKind::Neb2,
            neb_a * 0.85,
        );

        // 4. 260 estrelas de fundo
        for b in &self.bg {
            let tw = 0.72 + 0.28 * (self.sec * b.sp + b.ph).sin();
            let x = (b.x * w - self.px * (2.0 + b.z * 5.0)).rem_euclid(w);
            let y = (b.y * h - self.py * (2.0 + b.z * 5.0)).rem_euclid(h);
            let (tr, tg, tb) = b.tint;
            frame.fill(
                &Path::circle(Point::new(x, y), b.r),
                Color::from_rgba8(tr, tg, tb, (b.a * tw).clamp(0.0, 1.0)),
            );
        }

        // 5. 7 linhas da constelação
        for l in &self.lines {
            if l.p <= 0.001 {
                continue;
            }
            let (ax, ay) = self.pos(&self.stars[l.a], w, h);
            let (bx, by) = self.pos(&self.stars[l.b], w, h);
            let ep = ease_in_out_cubic(l.p);
            let ex = ax + (bx - ax) * ep;
            let ey = ay + (by - ay) * ep;
            let alpha = (0.28 + self.done * 0.18) * self.stars[l.a].a.min(self.stars[l.b].a);

            let line_path = Path::line(Point::new(ax, ay), Point::new(ex, ey));
            frame.stroke(
                &line_path,
                Stroke::default()
                    .with_width(0.85)
                    .with_color(Color::from_rgba8(190, 206, 245, alpha.clamp(0.0, 1.0))),
            );
            if l.p < 0.999 {
                draw_glow(
                    &mut frame,
                    Point::new(ex, ey),
                    11.0,
                    SpriteKind::Cool,
                    0.55 * (1.0 - l.p),
                );
            }
        }

        // 6. 7 estrelas principais + cruz de difração no nascimento
        for (i, s) in self.stars.iter().enumerate() {
            if s.a <= 0.002 {
                continue;
            }
            let (x, y) = self.pos(s, w, h);
            let age = (self.sec - s.born).max(0.0);
            let birth = if age < 2.4 {
                (age / 0.35).min(1.0) * (1.0 - (age - 0.35).max(0.0) / 2.05)
            } else {
                0.0
            };
            let breath = 1.0 + 0.04 * (self.sec * 1.3 + (i as f32) * 1.1).sin();
            let fin = 1.0 + self.done * 0.22;
            let halo = s.r * (9.5 + birth * 7.0) * breath * fin;

            draw_glow(
                &mut frame,
                Point::new(x, y),
                halo,
                s.kind,
                (s.a * (0.78 + birth * 0.35)).min(1.0),
            );
            draw_glow(
                &mut frame,
                Point::new(x, y),
                s.r * 3.2 * fin,
                s.kind,
                s.a,
            );

            if birth > 0.02 {
                let len = (18.0 + s.r * 5.0) * birth;
                let angle = age * 0.35;
                let ca = angle.cos();
                let sa = angle.sin();
                let cross_color = Color::from_rgba8(235, 240, 255, (0.38 * birth).clamp(0.0, 1.0));
                let p1 = Path::line(
                    Point::new(x - len * ca, y - len * sa),
                    Point::new(x + len * ca, y + len * sa),
                );
                let p2 = Path::line(
                    Point::new(x + len * sa, y - len * ca),
                    Point::new(x - len * sa, y + len * ca),
                );
                frame.stroke(&p1, Stroke::default().with_width(0.7).with_color(cross_color));
                frame.stroke(&p2, Stroke::default().with_width(0.7).with_color(cross_color));
            }

            let (cr, cg, cb) = s.core;
            frame.fill(
                &Path::circle(Point::new(x, y), s.r * 0.85),
                Color::from_rgba8(cr, cg, cb, s.a.clamp(0.0, 1.0)),
            );
        }

        // 7. Estrela cadente ocasional
        if let Some(sh) = self.shoot {
            let age = self.sec - sh.born;
            if (0.0..=0.7).contains(&age) {
                let p = age / 0.7;
                let a = (p * std::f32::consts::PI).sin() * 0.55;
                let sx = sh.x + sh.vx * age;
                let sy = sh.y + sh.vy * age;
                let trail = Path::line(Point::new(sx, sy), Point::new(sx - 55.0, sy - 32.0));
                frame.stroke(
                    &trail,
                    Stroke::default()
                        .with_width(1.1)
                        .with_color(Color::from_rgba8(230, 236, 255, a.clamp(0.0, 1.0))),
                );
            }
        }

        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn park_miller_seed_7_matches_javascript_reference() {
        let mut rng = ParkMillerRng::new(7);
        // 7 * 16807 = 117649
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
        // Etapa 1: só Betelgeuse (step 0) acende
        for _ in 0..60 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        assert!(sky.stars[0].a > 0.9);
        assert!(sky.stars[1].a < 0.01);

        // Etapa 3: Cinturão (Alnitak, Alnilam, Mintaka)
        sky.notify_step(3);
        for _ in 0..90 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        assert!(sky.stars[2].a > 0.9);
        assert!(sky.stars[3].a > 0.85);
        assert!(sky.stars[4].a > 0.8);

        // Voltar para a etapa 1 não apaga o Cinturão
        sky.notify_step(1);
        assert_eq!(sky.lit, 3);
        for _ in 0..30 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        assert!(sky.stars[4].a > 0.8);

        // Fim da instalação acende Rigel (índice 6)
        sky.notify_step(4);
        sky.notify_install_completed();
        for _ in 0..90 {
            sky.step_dt(1.0 / 30.0, 468.0, 564.0);
        }
        assert!(sky.stars[6].a > 0.9);
        assert!(sky.done > 0.9);
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

        // Verifica que Betelgeuse (~0.30*468, ~0.22*564) e Rigel (~0.28*468, ~0.78*564) estão luminosas
        let (bx, by) = sky.pos(&sky.stars[0], 468.0, 564.0);
        let bp = img.get_pixel(bx.round() as u32, by.round() as u32);
        assert!(bp[0] > 180 && bp[1] > 140);

        let (rx, ry) = sky.pos(&sky.stars[6], 468.0, 564.0);
        let rp = img.get_pixel(rx.round() as u32, ry.round() as u32);
        assert!(rp[2] > 180);
    }
}

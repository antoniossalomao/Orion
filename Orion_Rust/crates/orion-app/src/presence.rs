//! Presença do Orion: esfera de poeira de estrelas que respira e reage à voz (protótipo aprovado
//! pelo Antônio em 13/11/2026, "esfera de poeira", com a reação ao som 20% acima do protótipo).
//!
//! - Estado (repouso, ouvindo, pensando, falando, aguarda você) muda ritmo, giro e brilho; nada pula,
//!   tudo é interpolado. "Aguarda você" fica âmbar (Betelgeuse), a cor de aprovação pendente.
//! - Som real: o `orion-voice` publica 8 faixas do que está tocando (graves deformam a parte de baixo,
//!   agudos a de cima) via [`publicar`]. Sem nível real, "falando" usa um ritmo de sílabas simulado.
//! - Leve: ~1800 pontos agrupados em 10 camadas de brilho (10 preenchimentos por quadro).
//! - "Reduzir movimento": só respira devagar e ignora o som.
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub const BANDS: usize = 8;
const POINTS: usize = 1800;
const LAYERS: usize = 10;
/// Reação ao som: valores do protótipo × 1,2 (pedido do Antônio).
const REACTION: f32 = 1.2;

static LEVEL: std::sync::Mutex<Option<([f32; BANDS], Instant)>> = std::sync::Mutex::new(None);

/// Chamado pelo servidor (gancho do `orion-voice`) enquanto a voz toca; `None` quando para.
pub fn publish(v: Option<[f32; BANDS]>) {
    if let Ok(mut n) = LEVEL.lock() {
        *n = v.map(|b| (b, Instant::now()));
    }
}

fn real_level() -> Option<[f32; BANDS]> {
    LEVEL.lock().ok()?.filter(|(_, at)| at.elapsed() < Duration::from_millis(250)).map(|(b, _)| b)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PresenceState {
    #[default]
    Idle,
    Listening,
    Thinking,
    Speaking,
    Waiting,
}

#[derive(Clone, Copy)]
struct Target {
    energy: f32,
    speed: f32,
    spin: f32,
    brightness: f32,
}

fn target(e: PresenceState) -> Target {
    match e {
        PresenceState::Idle => Target { energy: 0.25, speed: 0.25, spin: 0.05, brightness: 0.65 },
        PresenceState::Listening => Target { energy: 0.55, speed: 0.6, spin: 0.08, brightness: 0.9 },
        PresenceState::Thinking => Target { energy: 0.45, speed: 1.0, spin: 0.45, brightness: 0.78 },
        PresenceState::Speaking => Target { energy: 0.7, speed: 0.55, spin: 0.1, brightness: 1.0 },
        PresenceState::Waiting => Target { energy: 0.35, speed: 0.3, spin: 0.04, brightness: 0.85 },
    }
}

const RIGEL: [f32; 3] = [185.0, 198.0, 234.0];
const AMBER: [f32; 3] = [233.0, 180.0, 103.0];

struct Geo {
    pts: Vec<[f32; 3]>,
    waves: [[f32; 5]; 5],
}

static GEO: LazyLock<Geo> = LazyLock::new(|| {
    let g = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    let pts = (0..POINTS)
        .map(|i| {
            let y = 1.0 - (i as f32 / (POINTS - 1) as f32) * 2.0;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let a = i as f32 * g;
            [a.cos() * r, y, a.sin() * r]
        })
        .collect();
    // mesmas ondas do protótipo (semente 7, gerador de Park–Miller)
    let mut seed: u64 = 7;
    let mut rnd = || {
        seed = (seed * 16807) % 2_147_483_647;
        seed as f32 / 2_147_483_647.0
    };
    let mut waves = [[0.0; 5]; 5];
    for (i, o) in waves.iter_mut().enumerate() {
        let (x, y, z) = (rnd() * 2.0 - 1.0, rnd() * 2.0 - 1.0, rnd() * 2.0 - 1.0);
        let l = (x * x + y * y + z * z).sqrt().max(1e-6);
        let f = 2.2 + i as f32 * 1.3;
        *o = [x / l * f, y / l * f, z / l * f, 0.6 + rnd() * 0.9, rnd() * 6.28];
    }
    Geo { pts, waves }
});

/// Estado de animação; vive no `App` e avança a cada quadro.
#[derive(Clone, Debug)]
pub struct Engine {
    pub state: PresenceState,
    energy: f32,
    speed: f32,
    spin: f32,
    brightness: f32,
    warmth: f32,
    level: f32,
    bands: [f32; BANDS],
    peak: f32,
    clock: f32,
    rot: f32,
    last: Option<Instant>,
}

impl Default for Engine {
    fn default() -> Self {
        let a = target(PresenceState::Idle);
        Engine { state: PresenceState::Idle, energy: a.energy, speed: a.speed, spin: a.spin, brightness: a.brightness, warmth: 0.0, level: 0.0, bands: [0.0; BANDS], peak: 0.3, clock: 0.0, rot: 0.0, last: None }
    }
}

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

impl Engine {
    /// Avança a animação até `agora`. `calmo` = reduzir movimento.
    pub fn advance(&mut self, now: Instant, calm: bool) {
        let dt = self.last.map(|u| now.saturating_duration_since(u).as_secs_f32()).unwrap_or(0.0).min(0.05);
        self.last = Some(now);
        self.clock += dt * if calm { 0.15 } else { 1.0 };
        let a = target(self.state);
        let k = 1.0 - 0.03f32.powf(dt);
        self.energy = lerp(self.energy, a.energy, k);
        self.speed = lerp(self.speed, a.speed, k);
        self.spin = lerp(self.spin, a.spin, k);
        self.brightness = lerp(self.brightness, a.brightness, k);
        self.warmth = lerp(self.warmth, if self.state == PresenceState::Waiting { 1.0 } else { 0.0 }, k);

        let ks = 1.0 - 0.01f32.powf(dt); // segue a voz com suavidade, sem tremer a cada sílaba
        let real = if calm { None } else { real_level() };
        if let Some(raw) = real {
            // ganho automático: voz baixa também mexe a esfera; o pico decai devagar
            let top = raw.iter().copied().fold(0.0f32, f32::max);
            self.peak = (self.peak * 0.6f32.powf(dt)).max(top).max(0.12);
            let mut sum = 0.0;
            for (i, v) in raw.iter().enumerate() {
                let n = (v / self.peak).min(1.0);
                self.bands[i] = lerp(self.bands[i], n, ks);
                sum += n;
            }
            self.level = lerp(self.level, (sum / BANDS as f32 * 1.6).min(1.0), ks);
        } else {
            let target_level = if calm { 0.0 } else { self.simulated() };
            self.level = lerp(self.level, target_level, ks);
            let t = self.clock;
            for (b, f) in self.bands.iter_mut().enumerate() {
                let w = 0.55 + 0.45 * (t * (2.1 + b as f32 * 0.7) + b as f32 * 1.7).sin();
                *f = lerp(*f, target_level * w, ks);
            }
        }
        self.rot += dt * self.spin * if calm { 0.2 } else { 1.1 };
    }

    /// Ritmo simulado quando não há nível real (ex.: app aberto sem o servidor).
    fn simulated(&self) -> f32 {
        let t = self.clock;
        match self.state {
            PresenceState::Listening => (0.5 * (t * 3.1).sin() + 0.35 * (t * 7.7 + 1.0).sin() + 0.2 * (t * 13.3).sin()).max(0.0) * 0.7,
            PresenceState::Speaking => {
                let s = (t * 9.5).sin().max(0.0) * (0.55 + 0.45 * (t * 1.7).sin());
                s * if (t * 0.8).sin() > -0.6 { 1.0 } else { 0.1 }
            }
            PresenceState::Waiting => ((t * 2.2).sin() + 1.0) / 2.0 * 0.3,
            _ => ((t * 0.9).sin() + 1.0) / 2.0 * 0.08,
        }
    }

    /// A esfera precisa de quadros novos? (sempre, enquanto visível: ela respira)
    pub fn alive(&self) -> bool {
        true
    }
}

/// Desenha o `Motor` num canvas do tamanho que receber.
pub struct Sphere<'a> {
    pub engine: &'a Engine,
}

impl<'a, Message> canvas::Program<Message> for Sphere<'a> {
    type State = ();

    fn draw(&self, _s: &(), r: &Renderer, _t: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let m = self.engine;
        let mut f = Frame::new(r, b.size());
        let (w, h) = (b.width, b.height);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let radius = w.min(h) * 0.34 * (1.0 + m.level * 0.05 * REACTION);
        let color = [0, 1, 2].map(|i| lerp(RIGEL[i], AMBER[i], m.warmth) / 255.0);
        let amp = 0.045 + m.energy * 0.05;
        let tt = m.clock * m.speed * 0.85;
        let (cos_y, sin_y) = (m.rot.cos(), m.rot.sin());
        let (cos_x, sin_x) = (0.38f32.cos(), 0.38f32.sin());

        // halo suave atrás da esfera: degradê radial (círculos concêntricos de alfa baixo), sem borda
        // dura, para não virar um disco cinza em cima do fundo escuro
        let strength = 0.05 * m.brightness * (1.0 + m.level * 0.4);
        const STEPS: usize = 18;
        for i in 0..STEPS {
            let k = 1.0 - i as f32 / STEPS as f32;
            f.fill(&Path::circle(Point::new(cx, cy), radius * (0.6 + 0.85 * k)), Color::from_rgba(color[0], color[1], color[2], strength / STEPS as f32 * 2.0));
        }

        let base_size = (w.min(h) / 260.0).clamp(0.8, 1.6);
        let mut layers: Vec<canvas::path::Builder> = (0..LAYERS).map(|_| canvas::path::Builder::new()).collect();
        let geo = &*GEO;
        for p in &geo.pts {
            let (x, y, z) = (p[0], p[1], p[2]);
            let mut s = 0.0;
            for o in &geo.waves {
                s += (x * o[0] + y * o[1] + z * o[2] + tt * o[3] + o[4]).sin();
            }
            // som: cada altura da esfera responde a uma faixa, em ondas que correm pela superfície
            let u = (1.0 - y) / 2.0 * (BANDS - 1) as f32;
            let b0 = (u.floor() as usize).min(BANDS - 1);
            let b1 = (b0 + 1).min(BANDS - 1);
            let band = m.bands[b0] + (m.bands[b1] - m.bands[b0]) * (u - b0 as f32);
            let ripple = 0.55 + 0.45 * (x * 4.2 + z * 3.6 - tt * 6.5).sin();
            let d = 1.0 + amp * s / 5.0 + 0.13 * REACTION * band * ripple + 0.03 * REACTION * m.level;
            let (x, y, z) = (x * d, y * d, z * d);
            let x1 = x * cos_y - z * sin_y;
            let z1 = x * sin_y + z * cos_y;
            let y1 = y * cos_x - z1 * sin_x;
            let z2 = y * sin_x + z1 * cos_x;
            let persp = 2.6 / (2.6 - z2);
            let depth = ((z2 + 1.0) / 2.0).clamp(0.0, 0.999);
            let side = base_size * (0.6 + depth * 0.9) * (1.0 + band * 0.18 * REACTION);
            let px = cx + x1 * radius * persp - side / 2.0;
            let py = cy + y1 * radius * persp - side / 2.0;
            layers[(depth * LAYERS as f32) as usize].rectangle(Point::new(px, py), Size::new(side, side));
        }
        for (i, c) in layers.into_iter().enumerate() {
            let depth = (i as f32 + 0.5) / LAYERS as f32;
            let a = ((0.12 + 0.70 * depth * depth) * m.brightness * (1.0 + m.level * 0.3 * REACTION)).min(1.0);
            f.fill(&c.build(), Color::from_rgba(color[0], color[1], color[2], a));
        }
        vec![f.into_geometry()]
    }
}

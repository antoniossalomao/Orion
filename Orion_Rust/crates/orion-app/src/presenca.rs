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

pub const FAIXAS: usize = 8;
const PONTOS: usize = 1800;
const CAMADAS: usize = 10;
/// Reação ao som: valores do protótipo × 1,2 (pedido do Antônio).
const REACAO: f32 = 1.2;

static NIVEL: std::sync::Mutex<Option<([f32; FAIXAS], Instant)>> = std::sync::Mutex::new(None);

/// Chamado pelo servidor (gancho do `orion-voice`) enquanto a voz toca; `None` quando para.
pub fn publicar(v: Option<[f32; FAIXAS]>) {
    if let Ok(mut n) = NIVEL.lock() {
        *n = v.map(|b| (b, Instant::now()));
    }
}

fn nivel_real() -> Option<[f32; FAIXAS]> {
    NIVEL.lock().ok()?.filter(|(_, at)| at.elapsed() < Duration::from_millis(250)).map(|(b, _)| b)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Estado {
    #[default]
    Repouso,
    Ouvindo,
    Pensando,
    Falando,
    Aguarda,
}

#[derive(Clone, Copy)]
struct Alvo {
    energia: f32,
    velocidade: f32,
    giro: f32,
    brilho: f32,
}

fn alvo(e: Estado) -> Alvo {
    match e {
        Estado::Repouso => Alvo { energia: 0.25, velocidade: 0.25, giro: 0.05, brilho: 0.65 },
        Estado::Ouvindo => Alvo { energia: 0.55, velocidade: 0.6, giro: 0.08, brilho: 0.9 },
        Estado::Pensando => Alvo { energia: 0.45, velocidade: 1.0, giro: 0.45, brilho: 0.78 },
        Estado::Falando => Alvo { energia: 0.7, velocidade: 0.55, giro: 0.1, brilho: 1.0 },
        Estado::Aguarda => Alvo { energia: 0.35, velocidade: 0.3, giro: 0.04, brilho: 0.85 },
    }
}

const RIGEL: [f32; 3] = [185.0, 198.0, 234.0];
const AMBAR: [f32; 3] = [233.0, 180.0, 103.0];

struct Geo {
    pts: Vec<[f32; 3]>,
    ondas: [[f32; 5]; 5],
}

static GEO: LazyLock<Geo> = LazyLock::new(|| {
    let g = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    let pts = (0..PONTOS)
        .map(|i| {
            let y = 1.0 - (i as f32 / (PONTOS - 1) as f32) * 2.0;
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
    let mut ondas = [[0.0; 5]; 5];
    for (i, o) in ondas.iter_mut().enumerate() {
        let (x, y, z) = (rnd() * 2.0 - 1.0, rnd() * 2.0 - 1.0, rnd() * 2.0 - 1.0);
        let l = (x * x + y * y + z * z).sqrt().max(1e-6);
        let f = 2.2 + i as f32 * 1.3;
        *o = [x / l * f, y / l * f, z / l * f, 0.6 + rnd() * 0.9, rnd() * 6.28];
    }
    Geo { pts, ondas }
});

/// Estado de animação; vive no `App` e avança a cada quadro.
#[derive(Clone, Debug)]
pub struct Motor {
    pub estado: Estado,
    energia: f32,
    velocidade: f32,
    giro: f32,
    brilho: f32,
    calor: f32,
    nivel: f32,
    faixas: [f32; FAIXAS],
    pico: f32,
    relogio: f32,
    rot: f32,
    ultimo: Option<Instant>,
}

impl Default for Motor {
    fn default() -> Self {
        let a = alvo(Estado::Repouso);
        Motor { estado: Estado::Repouso, energia: a.energia, velocidade: a.velocidade, giro: a.giro, brilho: a.brilho, calor: 0.0, nivel: 0.0, faixas: [0.0; FAIXAS], pico: 0.3, relogio: 0.0, rot: 0.0, ultimo: None }
    }
}

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

impl Motor {
    /// Avança a animação até `agora`. `calmo` = reduzir movimento.
    pub fn avancar(&mut self, agora: Instant, calmo: bool) {
        let dt = self.ultimo.map(|u| agora.saturating_duration_since(u).as_secs_f32()).unwrap_or(0.0).min(0.05);
        self.ultimo = Some(agora);
        self.relogio += dt * if calmo { 0.15 } else { 1.0 };
        let a = alvo(self.estado);
        let k = 1.0 - 0.03f32.powf(dt);
        self.energia = lerp(self.energia, a.energia, k);
        self.velocidade = lerp(self.velocidade, a.velocidade, k);
        self.giro = lerp(self.giro, a.giro, k);
        self.brilho = lerp(self.brilho, a.brilho, k);
        self.calor = lerp(self.calor, if self.estado == Estado::Aguarda { 1.0 } else { 0.0 }, k);

        let ks = 1.0 - 0.01f32.powf(dt); // segue a voz com suavidade, sem tremer a cada sílaba
        let real = if calmo { None } else { nivel_real() };
        if let Some(raw) = real {
            // ganho automático: voz baixa também mexe a esfera; o pico decai devagar
            let topo = raw.iter().copied().fold(0.0f32, f32::max);
            self.pico = (self.pico * 0.6f32.powf(dt)).max(topo).max(0.12);
            let mut soma = 0.0;
            for (i, v) in raw.iter().enumerate() {
                let n = (v / self.pico).min(1.0);
                self.faixas[i] = lerp(self.faixas[i], n, ks);
                soma += n;
            }
            self.nivel = lerp(self.nivel, (soma / FAIXAS as f32 * 1.6).min(1.0), ks);
        } else {
            let alvo_nivel = if calmo { 0.0 } else { self.simulado() };
            self.nivel = lerp(self.nivel, alvo_nivel, ks);
            let t = self.relogio;
            for (b, f) in self.faixas.iter_mut().enumerate() {
                let w = 0.55 + 0.45 * (t * (2.1 + b as f32 * 0.7) + b as f32 * 1.7).sin();
                *f = lerp(*f, alvo_nivel * w, ks);
            }
        }
        self.rot += dt * self.giro * if calmo { 0.2 } else { 1.1 };
    }

    /// Ritmo simulado quando não há nível real (ex.: app aberto sem o servidor).
    fn simulado(&self) -> f32 {
        let t = self.relogio;
        match self.estado {
            Estado::Ouvindo => (0.5 * (t * 3.1).sin() + 0.35 * (t * 7.7 + 1.0).sin() + 0.2 * (t * 13.3).sin()).max(0.0) * 0.7,
            Estado::Falando => {
                let s = (t * 9.5).sin().max(0.0) * (0.55 + 0.45 * (t * 1.7).sin());
                s * if (t * 0.8).sin() > -0.6 { 1.0 } else { 0.1 }
            }
            Estado::Aguarda => ((t * 2.2).sin() + 1.0) / 2.0 * 0.3,
            _ => ((t * 0.9).sin() + 1.0) / 2.0 * 0.08,
        }
    }

    /// A esfera precisa de quadros novos? (sempre, enquanto visível: ela respira)
    pub fn viva(&self) -> bool {
        true
    }
}

/// Desenha o `Motor` num canvas do tamanho que receber.
pub struct Esfera<'a> {
    pub motor: &'a Motor,
}

impl<'a, Message> canvas::Program<Message> for Esfera<'a> {
    type State = ();

    fn draw(&self, _s: &(), r: &Renderer, _t: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let m = self.motor;
        let mut f = Frame::new(r, b.size());
        let (w, h) = (b.width, b.height);
        let (cx, cy) = (w / 2.0, h / 2.0);
        let raio = w.min(h) * 0.34 * (1.0 + m.nivel * 0.05 * REACAO);
        let cor = [0, 1, 2].map(|i| lerp(RIGEL[i], AMBAR[i], m.calor) / 255.0);
        let amp = 0.045 + m.energia * 0.05;
        let tt = m.relogio * m.velocidade * 0.85;
        let (cos_y, sin_y) = (m.rot.cos(), m.rot.sin());
        let (cos_x, sin_x) = (0.38f32.cos(), 0.38f32.sin());

        // halo suave atrás da esfera
        let halo = Color::from_rgba(cor[0], cor[1], cor[2], 0.05 * m.brilho * (1.0 + m.nivel * 0.4));
        f.fill(&Path::circle(Point::new(cx, cy), raio * 1.25), halo);

        let tam_base = (w.min(h) / 260.0).clamp(0.8, 1.6);
        let mut camadas: Vec<canvas::path::Builder> = (0..CAMADAS).map(|_| canvas::path::Builder::new()).collect();
        let geo = &*GEO;
        for p in &geo.pts {
            let (x, y, z) = (p[0], p[1], p[2]);
            let mut s = 0.0;
            for o in &geo.ondas {
                s += (x * o[0] + y * o[1] + z * o[2] + tt * o[3] + o[4]).sin();
            }
            // som: cada altura da esfera responde a uma faixa, em ondas que correm pela superfície
            let u = (1.0 - y) / 2.0 * (FAIXAS - 1) as f32;
            let b0 = (u.floor() as usize).min(FAIXAS - 1);
            let b1 = (b0 + 1).min(FAIXAS - 1);
            let faixa = m.faixas[b0] + (m.faixas[b1] - m.faixas[b0]) * (u - b0 as f32);
            let ondula = 0.55 + 0.45 * (x * 4.2 + z * 3.6 - tt * 6.5).sin();
            let d = 1.0 + amp * s / 5.0 + 0.13 * REACAO * faixa * ondula + 0.03 * REACAO * m.nivel;
            let (x, y, z) = (x * d, y * d, z * d);
            let x1 = x * cos_y - z * sin_y;
            let z1 = x * sin_y + z * cos_y;
            let y1 = y * cos_x - z1 * sin_x;
            let z2 = y * sin_x + z1 * cos_x;
            let persp = 2.6 / (2.6 - z2);
            let prof = ((z2 + 1.0) / 2.0).clamp(0.0, 0.999);
            let lado = tam_base * (0.6 + prof * 0.9) * (1.0 + faixa * 0.18 * REACAO);
            let px = cx + x1 * raio * persp - lado / 2.0;
            let py = cy + y1 * raio * persp - lado / 2.0;
            camadas[(prof * CAMADAS as f32) as usize].rectangle(Point::new(px, py), Size::new(lado, lado));
        }
        for (i, c) in camadas.into_iter().enumerate() {
            let prof = (i as f32 + 0.5) / CAMADAS as f32;
            let a = ((0.08 + 0.62 * prof * prof) * m.brilho * (1.0 + m.nivel * 0.3 * REACAO)).min(1.0);
            f.fill(&c.build(), Color::from_rgba(cor[0], cor[1], cor[2], a));
        }
        vec![f.into_geometry()]
    }
}

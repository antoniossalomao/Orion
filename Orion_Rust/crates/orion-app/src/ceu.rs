//! F01.29: o céu do `orion-sky` também no login e na tela inicial.
//! 20 quadros/s parado, 60 enquanto o ponteiro ou o teclado mexem; pausado com a janela fora de vista
//! e estático (um quadro) com "reduzir movimento" no Orion ou no Windows.

use std::time::{Duration, Instant};

const PARADO: f32 = 0.05;
const INTERACAO: f32 = 0.015;
const QUENTE: Duration = Duration::from_millis(1500);

pub struct Ceu {
    sky: orion_sky::OrionSky,
    pub img: iced::widget::image::Handle,
    at: Instant,
    quente_ate: Instant,
    tamanho: (u32, u32),
    estatico: bool,
}

fn hardware() -> bool {
    std::env::var("ICED_BACKEND").is_ok_and(|b| b == "wgpu")
}

/// Quadro do céu: a área onde ele aparece, na densidade da tela, para não ser esticado nem cortado.
/// O maior lado tem teto (REGRAS.md: portabilidade e RAM < 120 MB): 2560 px com GPU, 1920 na CPU.
/// Devolve o tamanho em pixels e quantos pixels cabem em cada ponto lógico.
fn quadro(area: iced::Size, densidade: f32) -> ((u32, u32), f32) {
    let (w, h) = (area.width.max(64.0), area.height.max(64.0));
    let teto = if hardware() { 2560.0 } else { 1920.0 };
    let k = densidade.clamp(0.5, 4.0).min(teto / w.max(h));
    (((w * k).round() as u32, (h * k).round() as u32), k)
}

impl Ceu {
    pub fn new(area: iced::Size, densidade: f32) -> Self {
        let mut sky = orion_sky::OrionSky::new();
        let (tamanho, k) = quadro(area, densidade);
        sky.set_pixel_scale(k);
        sky.set_size(tamanho.0, tamanho.1);
        sky.one_star_per_step();
        // Constelação distante: ocupa o centro da área e deixa o céu em volta.
        sky.compact_constellation(0.6);
        sky.light_to(7);
        for _ in 0..160 {
            sky.step_dt(0.05, 0.0, 0.0);
        }
        let img = sky.render_image_handle();
        let agora = Instant::now();
        Self { sky, img, at: agora, quente_ate: agora, tamanho, estatico: false }
    }

    /// Ponteiro ou tecla: sobe para 60 quadros/s por 1,5 s e leva a paralaxe junto.
    pub fn toque(&mut self, agora: Instant, ponteiro: Option<(f32, f32)>, win: iced::Size) {
        self.quente_ate = agora + QUENTE;
        if let Some((x, y)) = ponteiro {
            self.sky.set_pointer_normalized(x / win.width.max(1.0) - 0.5, y / win.height.max(1.0) - 0.5);
        }
    }

    pub fn quente(&self, agora: Instant) -> bool {
        agora < self.quente_ate
    }

    /// Intervalo do relógio de quadros pedido à assinatura.
    pub fn intervalo(&self, agora: Instant) -> Duration {
        Duration::from_secs_f32(if self.quente(agora) { INTERACAO } else { PARADO })
    }

    /// Avança até `agora`; `estatico` congela em um quadro (movimento reduzido).
    pub fn tick(&mut self, agora: Instant, area: iced::Size, densidade: f32, estatico: bool) {
        let dt = agora.saturating_duration_since(self.at).as_secs_f32();
        let (novo, k) = quadro(area, densidade);
        let mudou = novo != self.tamanho || estatico != self.estatico;
        if !mudou && dt < if self.quente(agora) { INTERACAO * 0.9 } else { PARADO * 0.9 } {
            return;
        }
        self.at = agora;
        self.estatico = estatico;
        if mudou {
            self.tamanho = novo;
            self.sky.set_pixel_scale(k);
            self.sky.set_size(novo.0, novo.1);
        }
        if !estatico {
            self.sky.step_dt(dt, 0.0, 0.0);
        }
        if !estatico || mudou {
            self.img = self.sky.render_image_handle();
        }
    }

    /// Refaz o quadro só se a área ou a densidade mudaram (login → início, barra recolhida, outro monitor).
    pub fn ajustar(&mut self, area: iced::Size, densidade: f32) {
        let (novo, k) = quadro(area, densidade);
        if novo != self.tamanho {
            self.tamanho = novo;
            self.sky.set_pixel_scale(k);
            self.sky.set_size(novo.0, novo.1);
            self.img = self.sky.render_image_handle();
        }
    }
}

/// "Mostrar animações" desligado no Windows (`SPI_GETCLIENTAREAANIMATION`); fora do Windows, falso.
#[cfg(windows)]
pub fn sistema_reduz_movimento() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, pv_param: *mut std::ffi::c_void, win_ini: u32) -> i32;
    }
    let mut ligado: i32 = 1;
    // SAFETY: `ligado` é um BOOL válido durante a chamada; SPI_GETCLIENTAREAANIMATION (0x1042) só escreve nele.
    let ok = unsafe { SystemParametersInfoW(0x1042, 0, &mut ligado as *mut i32 as *mut std::ffi::c_void, 0) };
    ok != 0 && ligado == 0
}

#[cfg(not(windows))]
pub fn sistema_reduz_movimento() -> bool {
    false
}

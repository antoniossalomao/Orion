//! F01.29: o céu do `orion-sky` também no login e na tela inicial, no mesmo desenho do instalador:
//! o quadro é composto numa thread própria, com um único quadro pendente (nunca acumula atraso), a resolução cai
//! sozinha se a composição passar de 14 ms, e nada é pedido com a janela sem foco ou minimizada.
//! Com "reduzir movimento" (Orion ou Windows) o céu fica num quadro estático.

use iced::widget::image::Handle;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

const LIMITE_QUADRO: Duration = Duration::from_millis(14);
const ESCALA_MIN: f32 = 0.5;

pub struct Ceu {
    sky: orion_sky::OrionSky,
    pub img: Handle,
    pedido: SyncSender<orion_sky::OrionSky>,
    resultado: Receiver<(Handle, Duration)>,
    ocupado: bool,
    ultimo: Option<Instant>,
    tamanho: (u32, u32),
    escala: f32,
    quer_quadro: bool,
}

fn hardware() -> bool {
    std::env::var("ICED_BACKEND").is_ok_and(|b| b == "wgpu")
}

/// Resolução do quadro: a da janela com GPU; na CPU, reduzida (REGRAS.md: portabilidade e RAM < 120 MB).
fn pixels(win: iced::Size, escala: f32) -> (u32, u32) {
    let (w, h) = (win.width.max(200.0), win.height.max(300.0));
    let alvo = if hardware() { 1280.0 } else { 800.0 } * escala;
    let k = (alvo / w.max(h)).min(1.0);
    (((w * k) as u32).max(64), ((h * k) as u32).max(64))
}

impl Ceu {
    pub fn new(win: iced::Size) -> Self {
        let mut sky = orion_sky::OrionSky::new();
        let tamanho = pixels(win, 1.0);
        sky.set_size(tamanho.0, tamanho.1);
        sky.one_star_per_step();
        sky.compact_constellation(0.8);
        sky.light_to(7);
        for _ in 0..160 {
            sky.step_dt(0.05, 0.0, 0.0);
        }
        let img = sky.render_image_handle();
        let (pedido, pedidos) = mpsc::sync_channel::<orion_sky::OrionSky>(1);
        let (resposta, resultado) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(sky) = pedidos.recv() {
                let inicio = Instant::now();
                let quadro = sky.render_image_handle();
                if resposta.send((quadro, inicio.elapsed())).is_err() {
                    break;
                }
            }
        });
        Self { sky, img, pedido, resultado, ocupado: false, ultimo: None, tamanho, escala: 1.0, quer_quadro: false }
    }

    /// Janela sem foco: o relógio recomeça sem salto quando ela voltar.
    pub fn pausar(&mut self) {
        self.ultimo = None;
    }

    /// Chamado pelo relógio de quadros. `animar` falso mantém um quadro estático (só refaz ao mudar o tamanho).
    pub fn tick(&mut self, agora: Instant, win: iced::Size, animar: bool) {
        let dt = self.ultimo.map(|p| agora.saturating_duration_since(p).as_secs_f32()).unwrap_or(0.05).clamp(0.001, 0.05);
        self.ultimo = Some(agora);
        if let Ok((quadro, gasto)) = self.resultado.try_recv() {
            self.img = quadro;
            self.ocupado = false;
            if gasto > LIMITE_QUADRO && self.escala > ESCALA_MIN {
                self.escala = (self.escala * 0.85).max(ESCALA_MIN);
                self.quer_quadro = true;
            }
        }
        let novo = pixels(win, self.escala);
        if novo != self.tamanho {
            self.tamanho = novo;
            self.sky.set_size(novo.0, novo.1);
            self.quer_quadro = true;
        }
        if animar {
            self.sky.step_dt(dt, 0.0, 0.0);
        }
        if (animar || self.quer_quadro) && !self.ocupado && self.pedido.try_send(self.sky.clone()).is_ok() {
            self.ocupado = true;
            self.quer_quadro = false;
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

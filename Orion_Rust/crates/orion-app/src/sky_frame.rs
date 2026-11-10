//! F01.29: o céu do `orion-sky` também no login e na tela inicial, no mesmo desenho do instalador:
//! o frame é composto numa thread própria, com um único frame pendente (nunca acumula atraso), a resolução cai
//! sozinha se a composição passar de 14 ms, e nada é request_tx com a janela sem foco ou minimizada.
//! Com "reduzir movimento" (Orion ou Windows) o céu fica num frame estático.

use iced::widget::image::Handle;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

const FRAME_LIMIT: Duration = Duration::from_millis(14);
const MIN_SCALE: f32 = 0.5;

pub struct SkyFrame {
    sky: orion_sky::OrionSky,
    pub img: Handle,
    request_tx: SyncSender<orion_sky::OrionSky>,
    result_rx: Receiver<(Handle, Duration)>,
    busy: bool,
    last_tick: Option<Instant>,
    size: (u32, u32),
    scale: f32,
    wants_frame: bool,
}

fn hardware() -> bool {
    std::env::var("ICED_BACKEND").is_ok_and(|b| b == "wgpu")
}

/// Resolução do frame: a da janela com GPU; na CPU, reduzida (REGRAS.md: portabilidade e RAM < 120 MB).
fn frame_pixels(win: iced::Size, scale: f32) -> (u32, u32) {
    let (w, h) = (win.width.max(200.0), win.height.max(300.0));
    let target = if hardware() { 1280.0 } else { 800.0 } * scale;
    let k = (target / w.max(h)).min(1.0);
    (((w * k) as u32).max(64), ((h * k) as u32).max(64))
}

impl SkyFrame {
    pub fn new(win: iced::Size) -> Self {
        let mut sky = orion_sky::OrionSky::new();
        let size = frame_pixels(win, 1.0);
        sky.set_size(size.0, size.1);
        sky.one_star_per_step();
        sky.compact_constellation(0.8);
        sky.light_to(7);
        for _ in 0..160 {
            sky.step_dt(0.05, 0.0, 0.0);
        }
        let img = sky.render_image_handle();
        let (request_tx, requests) = mpsc::sync_channel::<orion_sky::OrionSky>(1);
        let (results, result_rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(sky) = requests.recv() {
                let started = Instant::now();
                let frame = sky.render_image_handle();
                if results.send((frame, started.elapsed())).is_err() {
                    break;
                }
            }
        });
        Self { sky, img, request_tx, result_rx, busy: false, last_tick: None, size, scale: 1.0, wants_frame: false }
    }

    /// Janela sem foco: o relógio recomeça sem salto quando ela voltar.
    pub fn pause(&mut self) {
        self.last_tick = None;
    }

    /// Chamado pelo relógio de quadros. `animate` falso mantém um frame estático (só refaz ao mudar o size).
    pub fn tick(&mut self, now: Instant, win: iced::Size, animate: bool) {
        let dt = self.last_tick.map(|p| now.saturating_duration_since(p).as_secs_f32()).unwrap_or(0.05).clamp(0.001, 0.05);
        self.last_tick = Some(now);
        if let Ok((frame, spent)) = self.result_rx.try_recv() {
            self.img = frame;
            self.busy = false;
            if spent > FRAME_LIMIT && self.scale > MIN_SCALE {
                self.scale = (self.scale * 0.85).max(MIN_SCALE);
                self.wants_frame = true;
            }
        }
        let new_size = frame_pixels(win, self.scale);
        if new_size != self.size {
            self.size = new_size;
            self.sky.set_size(new_size.0, new_size.1);
            self.wants_frame = true;
        }
        if animate {
            self.sky.step_dt(dt, 0.0, 0.0);
        }
        if (animate || self.wants_frame) && !self.busy && self.request_tx.try_send(self.sky.clone()).is_ok() {
            self.busy = true;
            self.wants_frame = false;
        }
    }
}

/// "Mostrar animações" desligado no Windows (`SPI_GETCLIENTAREAANIMATION`); fora do Windows, falso.
#[cfg(windows)]
pub fn system_reduces_motion() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn SystemParametersInfoW(action: u32, param: u32, pv_param: *mut std::ffi::c_void, win_ini: u32) -> i32;
    }
    let mut enabled: i32 = 1;
    // SAFETY: `enabled` é um BOOL válido durante a chamada; SPI_GETCLIENTAREAANIMATION (0x1042) só escreve nele.
    let ok = unsafe { SystemParametersInfoW(0x1042, 0, &mut enabled as *mut i32 as *mut std::ffi::c_void, 0) };
    ok != 0 && enabled == 0
}

#[cfg(not(windows))]
pub fn system_reduces_motion() -> bool {
    false
}

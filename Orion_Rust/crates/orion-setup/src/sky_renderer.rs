//! Composição do céu fora da thread da interface, sem acumular quadros atrasados.
//! O fundo imutável é compartilhado pelas cópias da simulação (REGRAS.md: portabilidade).

use crate::sky::OrionSky;
use iced::widget::image::Handle;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

pub(crate) struct SkyRenderer {
    request: SyncSender<OrionSky>,
    result: Receiver<(Handle, Duration)>,
    busy: bool,
}

impl SkyRenderer {
    pub(crate) fn new() -> Self {
        let (request, requests) = mpsc::sync_channel::<OrionSky>(1);
        let (results, result) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(sky) = requests.recv() {
                let started = Instant::now();
                let frame = sky.render_image_handle();
                if results.send((frame, started.elapsed())).is_err() {
                    break;
                }
            }
        });
        Self { request, result, busy: false }
    }

    pub(crate) fn take_frame(&mut self) -> Option<(Handle, Duration)> {
        let frame = self.result.try_recv().ok()?;
        self.busy = false;
        Some(frame)
    }

    pub(crate) fn request(&mut self, sky: &OrionSky) {
        if !self.busy && self.request.try_send(sky.clone()).is_ok() {
            self.busy = true;
        }
    }
}

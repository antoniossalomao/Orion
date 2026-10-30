//! Módulo de câmera na janela: abre e fecha a câmera, busca o último quadro a cada ~66 ms e
//! transforma gestos em ações de navegação (REGRAS.md: gesto só abre e navega; nunca aprova nem
//! executa). O vídeo e a análise ficam no núcleo; aqui só chegam a miniatura e os rótulos.

use crate::app::{App, Msg, View};
use crate::net::CameraSnapshot;
use iced::Task;
use std::time::{Duration, Instant};

/// Ritmo de leitura do último quadro (o núcleo analisa no mesmo ritmo).
pub const POLL: Duration = Duration::from_millis(66);
/// Quanto o último gesto fica escrito no HUD.
pub const GESTURE_SHOWN: Duration = Duration::from_millis(1500);

pub struct CameraUi {
    pub opened_at: Instant,
    pub opening: bool,
    pub snapshot: Option<(CameraSnapshot, iced::widget::image::Handle)>,
    pub error: Option<String>,
    pub last_gesture: Option<(String, Instant)>,
}

#[derive(Debug, Clone)]
pub enum CameraMsg {
    Open,
    Opened(Result<(), String>),
    Poll,
    Polled(Option<CameraSnapshot>),
    Close,
}

/// Nome do gesto e o que ele faz, para a ilha e o HUD.
pub fn describe(label: &str, voice_on: bool) -> (&'static str, &'static str) {
    match label {
        "OPEN PALM" => ("Mão aberta", "Início"),
        "PEACE" => ("Paz", "Captura rápida"),
        "POINT" => ("Apontar", "Paleta de comandos"),
        "THUMBS UP" => ("Joinha", "OK"),
        "FIST" if voice_on => ("Punho", "Voz pausada"),
        "FIST" => ("Punho", "Nada para pausar"),
        "SWIPE LEFT" => ("Deslizar para a esquerda", "Voltar"),
        "SWIPE RIGHT" => ("Deslizar para a direita", "Avançar"),
        _ => ("Gesto", ""),
    }
}

/// A ação de cada gesto: só abrir telas, navegar ou cortar a escuta. Nenhum gesto aprova,
/// envia, apaga ou executa ferramenta.
fn action(label: &str, voice_on: bool) -> Option<Msg> {
    match label {
        "OPEN PALM" => Some(Msg::Go(View::Home)),
        "PEACE" => Some(Msg::QcapOpen),
        "POINT" => Some(Msg::PalOpen),
        "FIST" if voice_on => Some(Msg::Live),
        "SWIPE LEFT" => Some(Msg::Back),
        "SWIPE RIGHT" => Some(Msg::Fwd),
        _ => None,
    }
}

impl App {
    pub fn update_camera(&mut self, m: CameraMsg) -> Task<Msg> {
        let now = Instant::now();
        self.now = now;
        match m {
            CameraMsg::Open => {
                let Some(api) = self.api.clone() else { return Task::none() };
                if self.camera.is_some() {
                    return Task::none();
                }
                if self.island.busy() {
                    self.toast("O Face ID está usando a câmera agora.");
                    return Task::none();
                }
                self.camera = Some(CameraUi { opened_at: now, opening: true, snapshot: None, error: None, last_gesture: None });
                Task::perform(async move { api.camera_open().await }, |r| Msg::Camera(CameraMsg::Opened(r)))
            }
            CameraMsg::Opened(r) => {
                let Some(cam) = self.camera.as_mut() else {
                    // fechou enquanto abria: desliga já
                    if r.is_ok() {
                        if let Some(api) = self.api.as_ref() {
                            api.camera_close();
                        }
                    }
                    return Task::none();
                };
                cam.opening = false;
                if let Err(e) = r {
                    cam.error = Some(e);
                }
                Task::none()
            }
            CameraMsg::Poll => {
                let (Some(api), Some(cam)) = (self.api.clone(), self.camera.as_ref()) else { return Task::none() };
                if cam.opening || cam.error.is_some() {
                    return Task::none();
                }
                Task::perform(async move { api.camera_poll().await }, |s| Msg::Camera(CameraMsg::Polled(s)))
            }
            CameraMsg::Polled(Some(snap)) => {
                let voice = self.voice;
                let Some(cam) = self.camera.as_mut() else { return Task::none() };
                let gestures = snap.gestures.clone();
                if snap.w > 0 && cam.snapshot.as_ref().is_none_or(|(s, _)| s.seq != snap.seq) {
                    let handle = iced::widget::image::Handle::from_rgba(snap.w, snap.h, snap.rgba.clone());
                    cam.snapshot = Some((snap, handle));
                }
                let mut tasks = vec![];
                for g in gestures {
                    if let Some(cam) = self.camera.as_mut() {
                        cam.last_gesture = Some((g.clone(), now));
                    }
                    let (name, what) = describe(&g, voice);
                    tasks.push(self.island_gesture(name, what));
                    if let Some(msg) = action(&g, voice) {
                        tasks.push(Task::done(msg));
                    }
                }
                Task::batch(tasks)
            }
            CameraMsg::Polled(None) => Task::none(),
            CameraMsg::Close => {
                if self.camera.take().is_some() {
                    if let Some(api) = self.api.as_ref() {
                        api.camera_close();
                    }
                }
                Task::none()
            }
        }
    }
}

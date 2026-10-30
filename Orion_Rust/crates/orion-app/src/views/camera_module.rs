//! Painel flutuante do módulo de câmera (canto inferior direito): o mesmo HUD do Face ID, com o
//! esqueleto da mão e a telemetria dos gestos (REGRAS.md: a câmera só liga por pedido seu; gesto
//! só abre e navega; nunca aprova nem executa). O resto da janela continua clicável.

use crate::app::{App, Msg};
use crate::camera_ui::{CameraMsg, GESTURE_SHOWN};
use crate::theme as t;
use crate::ui::*;
use crate::views::camera_hud::{self, Hud};
use iced::alignment::Vertical;
use iced::widget::{column, container, opaque, row};
use iced::{Border, Length};

const PANEL_W: f32 = 400.0;

pub fn view(app: &App) -> El<'_> {
    let Some(cam) = app.camera.as_ref() else { return sp_h(0.0) };
    let now = app.now;
    let elapsed = now.saturating_duration_since(cam.opened_at).as_secs_f32();
    // ponto vermelho de câmera ligada, sempre visível enquanto o módulo está aberto
    let rec = container(sp_w(0.0)).width(Length::Fixed(7.0)).height(Length::Fixed(7.0)).style(|_| container::Style { background: Some(t::CAM_DOT.into()), border: Border { radius: 4.0.into(), ..Default::default() }, ..Default::default() });
    let accel = cam.snapshot.as_ref().and_then(|(s, _)| s.accel.clone());
    let head = row![
        rec,
        txf(crate::i18n::text("Câmera"), 12.0, t::FG, t::UI_MED),
        txf(accel.map(|a| format!("· {a}")).unwrap_or_default(), 11.0, t::FG3, t::MONO),
        grow(),
        tip(ib("close", Some(Msg::Camera(CameraMsg::Close)), false), crate::i18n::text("Desligar a câmera"))
    ]
    .spacing(8)
    .align_y(Vertical::Center);

    let body: El = match (&cam.snapshot, &cam.error) {
        (_, Some(e)) => container(tx(crate::i18n::status(e), 12.5, t::DANGER)).padding(pad(18.0, 4.0, 18.0, 4.0)).into(),
        (Some((snap, handle)), None) if snap.w > 0 => {
            let gesture = cam.last_gesture.as_ref().filter(|(_, at)| now.saturating_duration_since(*at) < GESTURE_SHOWN).map(|(g, _)| g.clone());
            let state = match (&gesture, snap.hand.is_some(), snap.awake, snap.accel.is_some()) {
                (Some(g), _, _, _) => g.clone(),
                (None, true, _, _) => "TRACKING".into(),
                (None, false, true, _) => "SCANNING".into(),
                (None, false, false, true) => "STANDBY".into(),
                (None, false, false, false) => "OFF".into(),
            };
            let cpu = snap.cpu.map(|c| format!("{c:.0}%")).unwrap_or_else(|| "--".into());
            let mut telemetry = vec![(format!("[GESTURE: {state}]"), if gesture.is_some() { t::ACCENT } else { t::FG2 }), (format!("[PROCESSOR: {cpu} CPU]"), t::FG2)];
            if let (Some(a), Some(ms)) = (&snap.accel, snap.infer_ms) {
                telemetry.push((format!("[{}: {ms:.0} ms]", a.to_uppercase()), t::FG3));
            }
            let tone = if gesture.is_some() { t::ACCENT } else { t::FG2 };
            let hud = camera_hud::frame(Hud { w: snap.w, h: snap.h, handle, face: snap.face, hand: snap.hand.clone(), tone, tag: None, telemetry, t_phase: elapsed, reduced: app.p.reduced, width: PANEL_W - 16.0 });
            let mut col = column![hud].spacing(8);
            if let Some(note) = &snap.note {
                col = col.push(tx(crate::i18n::status(note), 11.5, t::FG3));
            }
            col.into()
        }
        _ => container(tx(crate::i18n::text("Ligando a câmera…"), 12.5, t::FG3)).padding(pad(18.0, 4.0, 18.0, 4.0)).into(),
    };

    let panel = container(column![head, body].spacing(8))
        .padding(pad(8.0, 8.0, 8.0, 8.0))
        .width(Length::Fixed(PANEL_W.min(app.win.width - 32.0)))
        .style(|_| container::Style { background: Some(t::ISLAND_BG.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_PANEL.into() }, shadow: shadow(), ..Default::default() });
    let p = crate::anim::progress(Some(cam.opened_at), now, 320, app.p.reduced);
    container(opaque(rise(fade(panel, p, t::BG), p, 12.0)))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_right(Length::Fill)
        .align_bottom(Length::Fill)
        .padding(pad(0.0, 16.0, 16.0, 0.0))
        .into()
}

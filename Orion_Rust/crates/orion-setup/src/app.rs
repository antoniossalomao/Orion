//! Máquina de estados e interface visual (`iced 0.14`) do instalador `Orion Setup`.
//!
//! Tradução fiel de `orion-instalador.html` e `ORION_INSTALADOR_ESPECIFICACAO.md`:
//! - Janela 960×600 sem borda nativa (`decorations: false`, `resizable: false`)
//! - Barra de título `.bar` (36 px, `#0e0f11`) com `✦ Orion Setup · v0.1.0` e botões `—` / `✕`
//! - Painel esquerdo `.sky` (468×564 px) com a constelação de Orion renderizada via sprites 128×128
//! - Painel direito `.pane` (492×564 px) com barra de 5 segmentos (3 px), palco com transição de 340 ms e rodapé

use crate::audio::SoundController;
use crate::i18n::{format_gb, install_step_texts, star_name, step_counter, texts, Language, Texts};
use crate::sky::OrionSky;
use crate::system::{InstallPlan, PcInfo, SetupChoices, StepItemStatus};
use crate::theme::{self, standard_curve, with_alpha};

fn preview_root() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("orion-setup-preview-{}-{id}", std::process::id()))
}
use iced::alignment::{Horizontal, Vertical};
use iced::event::{self, Event};
use iced::font::{self, Font};
use iced::keyboard;
use iced::mouse;
use iced::time;
use iced::widget::{
    button, column, container, image as iced_image, mouse_area, row, scrollable, stack, text, text_input, Column, Space,
};
use iced::window;
use iced::{Alignment, Background, Border, Color, Element, Length, Point, Subscription, Task, Theme};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const STAGE_TRANSITION_SECS: f32 = 0.34;
pub const STEP_WEIGHTS: [f32; 6] = [10.0, 12.0, 20.0, 40.0, 10.0, 8.0];

pub const FONT_SANS: Font = Font::with_name("Segoe UI");
pub const FONT_SANS_SEMIBOLD: Font = Font {
    family: font::Family::Name("Segoe UI"),
    weight: font::Weight::Semibold,
    stretch: font::Stretch::Normal,
    style: font::Style::Normal,
};
pub const FONT_MONO: Font = Font::with_name("Consolas");

/// Fase da transição entre etapas (`0.34 s` Out com `-8 px` -> troca etapa -> `0.34 s` In de `+10 px`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionPhase {
    Out { target_step: usize, elapsed: f32 },
    In { elapsed: f32 },
}

#[derive(Debug, Clone)]
pub enum Message {
    AnimationTick(Instant),
    WindowActive(bool),
    CursorMoved(Point),
    DragTitlebar,
    MinimizeWindow,
    RequestCloseWindow,
    ConfirmCancelYes,
    ConfirmCancelNo,
    SelectLanguage(Language),
    ToggleLanguage,
    NextStep,
    PrevStep,
    CycleAppDir,
    CycleModelsDisk,
    SelectModelsDisk(usize),
    ToggleModel(String),
    ToggleAutostart(bool),
    ToggleVoice(bool),
    ToggleVulkan(bool),
    WakeBundleChanged(String),
    ToggleTaskbar(bool),
    ToggleCrashReports(bool),
    ToggleErrorDetails,
    RetryInstall,
    OpenOrion,
}

pub struct SetupApp {
    pub step: usize,
    pub sky: OrionSky,
    pub sky_handle: iced_image::Handle,
    pub belt_icon_handle: iced_image::Handle,
    pub pc: PcInfo,
    pub choices: SetupChoices,
    pub install_plan: InstallPlan,
    pub step_statuses: [StepItemStatus; 6],
    pub current_install_idx: usize,
    pub install_substep_progress: f32,
    pub install_started: bool,
    pub install_error: Option<String>,
    pub transition: Option<TransitionPhase>,
    pub star_label_alpha: f32,
    pub confirm_cancel_modal: bool,
    pub show_error_details: bool,
    pub sound: SoundController,
    pub dry_run: bool,
    pub last_tick: Option<Instant>,
    /// Escala do quadro do céu (1.0 = nítido); cai para 0.75 se o PC não acompanha.
    sky_scale: f32,
    slow_frames: u32,
    frame_skip: u32,
    window_active: bool,
    sky_renderer: crate::sky_renderer::SkyRenderer,
    pub selected_disk_idx: usize,
    download_worker: Option<std::sync::mpsc::Receiver<(InstallPlan, Result<String, String>)>>,
    download_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    download_progress: std::sync::Arc<std::sync::atomic::AtomicU64>,
    cancel_after_download: bool,
}

impl Default for SetupApp {
    fn default() -> Self {
        Self::new(PcInfo::detect(), true)
    }
}

impl SetupApp {
    pub fn new(pc: PcInfo, dry_run: bool) -> Self {
        let choices = SetupChoices::new(&pc);
        let root_override = if dry_run {
            Some(preview_root())
        } else {
            None
        };
        let install_plan = InstallPlan::new(root_override);
        let mut sky = OrionSky::new();
        // Resolução cheia do painel: nítido, sem ampliar. `AnimationTick` reduz só se o PC não acompanhar.
        sky.set_size(theme::SKY_W as u32, theme::BODY_H as u32);
        // Avança suavemente o primeiro segundo para Betelgeuse já iniciar acendendo
        sky.step_dt(0.016, theme::SKY_W, theme::BODY_H);
        let sky_handle = sky.render_image_handle();
        let belt_icon_rgba = crate::sky::generate_belt_icon_rgba(36);
        let belt_icon_handle = iced_image::Handle::from_rgba(36, 36, belt_icon_rgba);

        let selected_disk_idx = pc
            .disks
            .iter()
            .enumerate()
            .max_by_key(|(_, d)| d.free_gb)
            .map(|(i, _)| i)
            .unwrap_or(0);

        Self {
            step: 1,
            sky,
            sky_handle,
            belt_icon_handle,
            pc,
            choices,
            install_plan,
            step_statuses: [StepItemStatus::Pending; 6],
            current_install_idx: 0,
            install_substep_progress: 0.0,
            install_started: false,
            install_error: None,
            transition: None,
            star_label_alpha: 1.0,
            confirm_cancel_modal: false,
            show_error_details: false,
            sound: SoundController::new(),
            dry_run,
            last_tick: None,
            sky_scale: 1.0,
            slow_frames: 0,
            frame_skip: 0,
            window_active: true,
            sky_renderer: crate::sky_renderer::SkyRenderer::new(),
            selected_disk_idx,
            download_worker: None,
            download_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            download_progress: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            cancel_after_download: false,
        }
    }

    pub fn new_with_sound(pc: PcInfo, dry_run: bool) -> Self {
        let mut app = Self::new(pc, dry_run);
        app.sound = SoundController::start_enabled();
        app
    }

    /// Constrói a aplicação usando os valores de referência do protótipo HTML (§6).
    pub fn new_prototype_reference() -> Self {
        Self::new(PcInfo::prototype_reference(), true)
    }

    pub fn title(&self) -> String {
        "Orion Setup".to_string()
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn is_install_completed(&self) -> bool {
        self.step_statuses.iter().all(|s| *s == StepItemStatus::Done)
    }

    /// Offset vertical (`px`) e opacidade (`0.0..=1.0`) atuais do palco durante a transição de 340 ms.
    pub fn stage_transform(&self) -> (f32, f32) {
        match self.transition {
            None => (0.0, 1.0),
            Some(TransitionPhase::Out { elapsed, .. }) => {
                let p = standard_curve((elapsed / STAGE_TRANSITION_SECS).clamp(0.0, 1.0));
                (-8.0 * p, 1.0 - p)
            }
            Some(TransitionPhase::In { elapsed }) => {
                let p = standard_curve((elapsed / STAGE_TRANSITION_SECS).clamp(0.0, 1.0));
                (10.0 * (1.0 - p), p)
            }
        }
    }

    /// Inicia a transição animada para `target_step` (`1..=5`).
    pub fn request_go_to_step(&mut self, target_step: usize) {
        let clamped = target_step.clamp(1, 5);
        if clamped == self.step && self.transition.is_none() {
            return;
        }
        self.transition = Some(TransitionPhase::Out {
            target_step: clamped,
            elapsed: 0.0,
        });
        self.star_label_alpha = 0.0;
    }

    /// Aplica imediatamente a troca de etapa (usado ao fim da fase `Out` ou em testes diretos).
    pub fn apply_step_immediately(&mut self, target_step: usize) {
        let clamped = target_step.clamp(1, 5);
        self.step = clamped;
        self.sky.notify_step(clamped);
        if clamped == 5 && !self.install_started {
            self.start_installation();
        }
    }

    pub fn start_installation(&mut self) {
        self.download_cancel.store(false, std::sync::atomic::Ordering::SeqCst);
        self.download_progress.store(0, std::sync::atomic::Ordering::SeqCst);
        let root_override = if self.dry_run {
            self.install_plan.root_override.clone().or_else(|| Some(preview_root()))
        } else {
            None
        };
        self.install_plan = InstallPlan::new(root_override);
        self.step_statuses = [StepItemStatus::Pending; 6];
        self.step_statuses[0] = StepItemStatus::Running;
        self.current_install_idx = 0;
        self.install_substep_progress = 0.0;
        self.install_started = true;
        self.install_error = None;
        self.show_error_details = false;
    }

    /// Se o quadro leva mais de 14 ms por 30 quadros seguidos, passa a 75% da resolução (uma vez).
    fn adapt_sky_quality(&mut self, spent: Duration) {
        if self.sky_scale < 1.0 {
            return;
        }
        if spent > Duration::from_millis(14) {
            self.slow_frames += 1;
        } else {
            self.slow_frames = 0;
        }
        if self.slow_frames >= 30 {
            self.sky_scale = 0.75;
            self.sky.set_size((theme::SKY_W * 0.75) as u32, (theme::BODY_H * 0.75) as u32);
        }
    }

    /// Avança a simulação temporal em `dt` segundos (constelação, transição de etapa e progresso de instalação).
    pub fn tick_dt(&mut self, dt: f32) {
        if self.window_active {
            self.sky.step_dt(dt, theme::SKY_W, theme::BODY_H);
        }

        if self.star_label_alpha < 1.0
            && !matches!(self.transition, Some(TransitionPhase::Out { .. }))
        {
            self.star_label_alpha = (self.star_label_alpha + dt / 0.35).min(1.0);
        }

        // Transição de palco (Out 0.34 s -> troca etapa -> In 0.34 s)
        if let Some(phase) = self.transition {
            match phase {
                TransitionPhase::Out {
                    target_step,
                    elapsed,
                } => {
                    let next_elapsed = elapsed + dt;
                    if next_elapsed >= STAGE_TRANSITION_SECS {
                        self.apply_step_immediately(target_step);
                        self.transition = Some(TransitionPhase::In { elapsed: 0.0 });
                    } else {
                        self.transition = Some(TransitionPhase::Out {
                            target_step,
                            elapsed: next_elapsed,
                        });
                    }
                }
                TransitionPhase::In { elapsed } => {
                    let next_elapsed = elapsed + dt;
                    if next_elapsed >= STAGE_TRANSITION_SECS {
                        self.transition = None;
                    } else {
                        self.transition = Some(TransitionPhase::In {
                            elapsed: next_elapsed,
                        });
                    }
                }
            }
        }

        if let Some(worker) = &self.download_worker {
            if let Ok((plan, result)) = worker.try_recv() {
                self.download_worker = None;
                self.install_plan = plan;
                match result {
                    Ok(_) if !self.cancel_after_download => {
                        self.step_statuses[5] = StepItemStatus::Done;
                        self.current_install_idx = 6;
                        self.install_substep_progress = 0.0;
                        self.sky.notify_install_completed();
                    }
                    _ => {
                        self.step_statuses[5] = StepItemStatus::Error;
                        self.install_error = Some(result.err().unwrap_or_else(|| "Instalação cancelada".into()));
                        self.install_plan.rollback();
                    }
                }
            }
        }
        // Downloads reais rodam fora do laço da UI, sem progresso temporal fictício.
        if self.step == 5
            && self.install_started
            && !self.is_install_completed()
            && self.install_error.is_none()
        {
            let step_duration = 0.85;
            self.install_substep_progress += dt / step_duration;
            while self.install_substep_progress >= 1.0
                && self.current_install_idx < 6
                && self.install_error.is_none()
            {
                self.install_substep_progress -= 1.0;
                let idx = self.current_install_idx;
                if idx == 5 && !self.dry_run {
                    self.install_substep_progress = 0.0;
                    if self.download_worker.is_none() {
                        let mut plan = self.install_plan.clone();
                        let choices = self.choices.clone();
                        let cancel = self.download_cancel.clone();
                        let progress = self.download_progress.clone();
                        let (send, receive) = std::sync::mpsc::channel();
                        self.download_worker = Some(receive);
                        std::thread::spawn(move || {
                            let result = plan.install_models(&choices, &cancel, &|n, total| {
                                progress.store(n.saturating_mul(1000) / total.max(1), std::sync::atomic::Ordering::SeqCst);
                            });
                            let _ = send.send((plan, result));
                        });
                    }
                    break;
                }
                match self.install_plan.execute_step(idx, &self.choices) {
                    Ok(_) => {
                        self.step_statuses[idx] = StepItemStatus::Done;
                        self.current_install_idx += 1;
                        if self.current_install_idx < 6 {
                            self.step_statuses[self.current_install_idx] = StepItemStatus::Running;
                        } else {
                            self.sky.notify_install_completed();
                            self.install_substep_progress = 0.0;
                            break;
                        }
                    }
                    Err(e) => {
                        self.step_statuses[idx] = StepItemStatus::Error;
                        self.install_error = Some(e);
                        self.install_plan.rollback();
                        break;
                    }
                }
            }
        }
    }

    pub fn current_install_percent(&self) -> f32 {
        if self.is_install_completed() {
            return 100.0;
        }
        let done_count = self
            .step_statuses
            .iter()
            .filter(|s| **s == StepItemStatus::Done)
            .count();
        let active_frac = if self.download_worker.is_some() {
            self.download_progress.load(std::sync::atomic::Ordering::SeqCst) as f32 / 1000.0
        } else if self.current_install_idx < 6 {
            self.install_substep_progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        (((done_count as f32 + active_frac) / 6.0) * 100.0).clamp(0.0, 100.0)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AnimationTick(now) => {
                let dt = self
                    .last_tick
                    .map(|prev| now.saturating_duration_since(prev).as_secs_f32())
                    .unwrap_or(1.0 / 30.0)
                    .clamp(0.001, 0.05);
                self.last_tick = Some(now);
                self.tick_dt(dt);
                // "Reduzir movimento" do Windows: o céu continua vivo, mas a 4 quadros por segundo.
                self.frame_skip = (self.frame_skip + 1) % 15;
                if let Some((frame, spent)) = self.sky_renderer.take_frame() {
                    self.sky_handle = frame;
                    self.adapt_sky_quality(spent);
                }
                if self.window_active && (!self.pc.reduce_motion || self.frame_skip == 0) {
                    self.sky_renderer.request(&self.sky);
                }
                if self.cancel_after_download && self.download_worker.is_none() {
                    return window::latest().and_then(window::close);
                }
                Task::none()
            }
            Message::WindowActive(active) => {
                self.window_active = active;
                self.last_tick = None;
                Task::none()
            }
            Message::CursorMoved(point) => {
                let nx = (point.x / theme::WINDOW_W) - 0.5;
                let ny = (point.y / theme::WINDOW_H) - 0.5;
                self.sky.set_pointer_normalized(nx, ny);
                Task::none()
            }
            Message::DragTitlebar => window::latest().and_then(window::drag),
            Message::MinimizeWindow => {
                self.window_active = false;
                window::latest().and_then(|id| window::minimize(id, true))
            }
            Message::RequestCloseWindow => {
                if self.step == 5 && self.install_started && !self.is_install_completed() {
                    self.confirm_cancel_modal = true;
                    Task::none()
                } else {
                    self.sound.fade_out();
                    window::latest().and_then(window::close)
                }
            }
            Message::ConfirmCancelYes => {
                self.confirm_cancel_modal = false;
                if self.download_worker.is_some() {
                    self.download_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    self.cancel_after_download = true;
                    return Task::none();
                }
                self.install_plan.rollback();
                self.sound.fade_out();
                window::latest().and_then(window::close)
            }
            Message::ConfirmCancelNo => {
                self.confirm_cancel_modal = false;
                Task::none()
            }
            Message::SelectLanguage(lang) => {
                self.choices.language = lang;
                Task::none()
            }
            Message::ToggleLanguage => {
                self.choices.language = match self.choices.language {
                    Language::PtBr => Language::En,
                    Language::En => Language::PtBr,
                };
                Task::none()
            }
            Message::NextStep => {
                if self.step == 1 && !self.pc.is_windows_11 {
                    return Task::none();
                }
                if self.step < 5 {
                    self.request_go_to_step(self.step + 1);
                } else if self.is_install_completed() {
                    self.sound.fade_out();
                    return window::latest().and_then(window::close);
                }
                Task::none()
            }
            Message::PrevStep => {
                if self.step > 1 && self.step < 5 {
                    self.request_go_to_step(self.step - 1);
                }
                Task::none()
            }
            Message::CycleAppDir => {
                let default_dir = PathBuf::from(r"C:\Program Files\Orion");
                let alt_dir = PathBuf::from(r"C:\Orion");
                if self.choices.app_dir == default_dir {
                    self.choices.app_dir = alt_dir;
                } else {
                    self.choices.app_dir = default_dir;
                }
                Task::none()
            }
            Message::CycleModelsDisk => {
                if !self.pc.disks.is_empty() {
                    self.selected_disk_idx = (self.selected_disk_idx + 1) % self.pc.disks.len();
                    let d = &self.pc.disks[self.selected_disk_idx];
                    self.choices.models_dir = PathBuf::from(format!("{}\\Orion\\Modelos", d.mount));
                }
                Task::none()
            }
            Message::SelectModelsDisk(idx) => {
                if let Some(d) = self.pc.disks.get(idx) {
                    self.selected_disk_idx = idx;
                    self.choices.models_dir = PathBuf::from(format!("{}\\Orion\\Modelos", d.mount));
                }
                Task::none()
            }
            Message::ToggleModel(id) => {
                let selected = self.choices.models.iter().find(|m| m.id == id).is_some_and(|m| m.selected);
                let artifact = orion_local::catalog::find(&id);
                // Um LLM e um STT ativos por vez; rejeita escolhas que excedem o teto.
                if !selected {
                    if let Some(a) = &artifact {
                        let total = self.pc.ram_gb as u64 * 1024 * 1024 * 1024;
                        if a.kind == "llm" && !orion_local::admitted(a.bytes, total, total) { return Task::none(); }
                        for m in &mut self.choices.models {
                            if orion_local::catalog::find(&m.id).is_some_and(|other| other.kind == a.kind) { m.selected = false; }
                        }
                    }
                }
                if let Some(m) = self.choices.models.iter_mut().find(|m| m.id == id) {
                    m.selected = !m.selected;
                }
                Task::none()
            }
            Message::ToggleAutostart(val) => {
                self.choices.start_with_windows = val;
                Task::none()
            }
            Message::ToggleVoice(val) => {
                self.choices.enable_voice = val;
                Task::none()
            }
            Message::ToggleVulkan(val) => {
                self.choices.use_vulkan = val;
                Task::none()
            }
            Message::WakeBundleChanged(path) => {
                self.choices.wake_bundle = path;
                Task::none()
            }
            Message::ToggleTaskbar(val) => {
                self.choices.pin_to_taskbar = val;
                Task::none()
            }
            Message::ToggleCrashReports(val) => {
                self.choices.send_crash_reports = val;
                Task::none()
            }
            Message::ToggleErrorDetails => {
                self.show_error_details = !self.show_error_details;
                Task::none()
            }
            Message::RetryInstall => {
                self.start_installation();
                Task::none()
            }
            Message::OpenOrion => {
                if !self.dry_run {
                    let executable = self.choices.app_dir.join("orion.exe");
                    let mut command = std::process::Command::new(executable);
                    command.current_dir(&self.choices.app_dir).env_clear();
                    for key in ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP", "USERPROFILE", "APPDATA", "LOCALAPPDATA"] {
                        if let Some(value) = std::env::var_os(key) { command.env(key, value); }
                    }
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        command.creation_flags(0x0800_0000);
                    }
                    if command.spawn().is_err() {
                        self.install_error = Some("Não foi possível abrir o Orion instalado. Confira as permissões da pasta.".into());
                        return Task::none();
                    }
                }
                self.sound.fade_out();
                window::latest().and_then(window::close)
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let tick = time::every(Duration::from_millis(if self.window_active { 16 } else { 250 })).map(Message::AnimationTick);
        let events = event::listen_with(|event, _status, _window_id| match event {
            Event::Window(window::Event::Focused) => Some(Message::WindowActive(true)),
            Event::Window(window::Event::Unfocused) => Some(Message::WindowActive(false)),
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved(position))
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key {
                keyboard::Key::Named(keyboard::key::Named::Enter) => Some(Message::NextStep),
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    Some(Message::RequestCloseWindow)
                }
                _ => None,
            },
            _ => None,
        });
        Subscription::batch([tick, events])
    }

    pub fn view(&self) -> Element<'_, Message> {
        let s = texts(self.choices.language);

        let titlebar = self.view_titlebar();
        let divider_top = container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::LINE)),
                ..Default::default()
            });

        let sky_panel = self.view_sky_panel();
        let divider_v = container(
            Space::new()
                .width(Length::Fixed(1.0))
                .height(Length::Fixed(theme::BODY_H)),
        )
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(theme::LINE)),
            ..Default::default()
        });
        let right_pane = self.view_right_pane(s);

        let body = row![sky_panel, divider_v, right_pane]
            .width(Length::Fixed(theme::WINDOW_W))
            .height(Length::Fixed(theme::BODY_H));

        let main_layout = column![titlebar, divider_top, body]
            .width(Length::Fixed(theme::WINDOW_W))
            .height(Length::Fixed(theme::WINDOW_H));

        let root: Element<'_, Message> = if self.confirm_cancel_modal {
            stack![main_layout, self.view_cancel_modal(s)].into()
        } else {
            main_layout.into()
        };

        container(root)
            .width(Length::Fixed(theme::WINDOW_W))
            .height(Length::Fixed(theme::WINDOW_H))
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::SIDE)),
                text_color: Some(theme::FG),
                border: Border {
                    color: theme::LINE,
                    width: 1.0,
                    radius: 12.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    /// Barra de título `.bar` (`orion-instalador.html` linhas 23–28, 113–116).
    fn view_titlebar(&self) -> Element<'_, Message> {
        let icon_img = iced_image(self.belt_icon_handle.clone())
            .width(Length::Fixed(18.0))
            .height(Length::Fixed(18.0));

        let drag_zone = mouse_area(
            container(
                row![
                    hspace(16.0),
                    icon_img,
                    hspace(8.0),
                    text("Orion Setup")
                        .size(12)
                        .font(FONT_SANS_SEMIBOLD)
                        .color(theme::FG2),
                    hspace(5.0),
                    text("· v0.1.0").size(12).font(FONT_SANS).color(theme::FG3),
                    fill_hspace(),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fixed(theme::TITLEBAR_H - 1.0))
            .align_y(Vertical::Center),
        )
        .on_press(Message::DragTitlebar);

        let min_btn = button(
            container(text("—").size(12).color(theme::FG3))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        )
        .width(Length::Fixed(34.0))
        .height(Length::Fixed(24.0))
        .padding(0)
        .on_press(Message::MinimizeWindow)
        .style(titlebar_btn_style(false));

        let close_btn = button(
            container(text("✕").size(12).color(theme::FG3))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        )
        .width(Length::Fixed(34.0))
        .height(Length::Fixed(24.0))
        .padding(0)
        .on_press(Message::RequestCloseWindow)
        .style(titlebar_btn_style(true));

        let controls =
            row![min_btn, hspace(2.0), close_btn, hspace(8.0)]
                .align_y(Alignment::Center);

        container(row![drag_zone, controls].align_y(Alignment::Center))
            .width(Length::Fixed(theme::WINDOW_W))
            .height(Length::Fixed(theme::TITLEBAR_H - 1.0))
            .align_y(Vertical::Center)
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::TITLEBAR)),
                ..Default::default()
            })
            .into()
    }

    /// Painel esquerdo `.sky` (`468×564 px`) com imagem RGBA ao vivo e legenda `.skycap`.
    fn view_sky_panel(&self) -> Element<'_, Message> {
        let sky_img = iced_image(self.sky_handle.clone())
            .width(Length::Fixed(theme::SKY_W))
            .height(Length::Fixed(theme::BODY_H));

        let step_caption = step_counter(self.step as u8, self.choices.language);
        let star_lbl = star_name(self.step as u8, self.choices.language);
        let betel_faded = with_alpha(theme::BETEL, self.star_label_alpha);

        let overlay = column![
            fill_vspace(),
            container(
                row![
                    text("Orion")
                        .size(22)
                        .font(FONT_SANS_SEMIBOLD)
                        .color(theme::FG),
                    fill_hspace(),
                    column![
                        text(step_caption)
                            .size(11)
                            .font(FONT_MONO)
                            .color(theme::FG3),
                        text(star_lbl)
                            .size(11)
                            .font(FONT_MONO)
                            .color(betel_faded),
                    ]
                    .align_x(Horizontal::Right)
                    .spacing(3),
                ]
                .align_y(Alignment::End),
            )
            .width(Length::Fill)
            .padding([20, 22])
        ]
        .width(Length::Fixed(theme::SKY_W))
        .height(Length::Fixed(theme::BODY_H));

        container(stack![sky_img, overlay])
            .width(Length::Fixed(theme::SKY_W))
            .height(Length::Fixed(theme::BODY_H))
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::SKY_BG)),
                ..Default::default()
            })
            .into()
    }

    /// Painel direito `.pane` (`orion-instalador.html` linhas 36–98, 125–135).
    fn view_right_pane<'a>(&'a self, s: &'a Texts) -> Element<'a, Message> {
        let mut stepbar = row![].spacing(6);
        for idx in 1..=5 {
            let color = if idx < self.step
                || (idx == 5 && self.step == 5 && self.is_install_completed())
            {
                theme::RIGEL
            } else if idx == self.step {
                theme::BETEL
            } else {
                theme::LINE
            };
            stepbar = stepbar.push(
                container(Space::new().width(Length::Fill).height(Length::Fixed(3.0))).style(
                    move |_theme: &Theme| container::Style {
                        background: Some(Background::Color(color)),
                        border: Border {
                            radius: 2.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                ),
            );
        }

        let (offset_y, stage_alpha) = self.stage_transform();
        let top_pad = offset_y.max(0.0);

        let stage_body = match self.step {
            1 => self.view_step_1(s, stage_alpha),
            2 => self.view_step_2(s, stage_alpha),
            3 => self.view_step_3(s, stage_alpha),
            4 => self.view_step_4(s, stage_alpha),
            _ => self.view_step_5(s, stage_alpha),
        };

        let stage = container(scrollable(column![vspace(top_pad), stage_body].width(Length::Fill))
            .direction(scrollable::Direction::Vertical(scrollable::Scrollbar::new().width(4).scroller_width(4).margin(2))))
            .width(Length::Fill)
            .height(Length::Fill);

        // Rodapé `.foot`
        let footer_caption = match self.step {
            1 => s.s1_footer.to_string(),
            2 => s.s2_footer.to_string(),
            3 => self.pc.step3_footer(self.choices.language),
            4 => s.s4_footer.to_string(),
            _ => {
                if self.is_install_completed() {
                    s.s5_footer_done.to_string()
                } else {
                    String::new()
                }
            }
        };

        let show_back = self.step > 1 && self.step < 5;
        let back_el: Element<'a, Message> = if show_back {
            button(text(s.back).size(13).font(FONT_SANS_SEMIBOLD).color(theme::FG))
                .padding([8, 12])
                .on_press(Message::PrevStep)
                .style(ghost_button_style())
                .into()
        } else {
            hspace(0.0)
        };

        let (next_label, next_enabled, next_msg) = match self.step {
            1 => (
                s.next,
                self.pc.is_windows_11,
                if self.pc.is_windows_11 {
                    Some(Message::NextStep)
                } else {
                    None
                },
            ),
            2 | 3 => (s.next, true, Some(Message::NextStep)),
            4 => (s.install, true, Some(Message::NextStep)),
            _ => (
                s.open_orion,
                self.is_install_completed(),
                if self.is_install_completed() {
                    Some(Message::OpenOrion)
                } else {
                    None
                },
            ),
        };

        let mut next_btn = button(
            text(next_label)
                .size(13)
                .font(FONT_SANS_SEMIBOLD)
                .color(if next_enabled {
                    theme::ON_PRIMARY
                } else {
                    with_alpha(theme::ON_PRIMARY, 0.5)
                }),
        )
        .padding([9, 18])
        .style(primary_button_style(next_enabled));

        if let Some(msg) = next_msg {
            next_btn = next_btn.on_press(msg);
        }

        let footer_divider = container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::LINE)),
                ..Default::default()
            });

        let footer = column![
            footer_divider,
            vspace(14.0),
            row![
                text(footer_caption)
                    .size(11)
                    .font(FONT_MONO)
                    .width(Length::Fill)
                    .color(theme::FG3),
                back_el,
                if show_back { hspace(8.0) } else { hspace(0.0) },
                next_btn,
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill),
        ];

        container(
            column![stepbar, vspace(16.0), stage, vspace(8.0), footer]
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fixed(theme::PANEL_W - 1.0))
        .height(Length::Fixed(theme::BODY_H))
        .padding(iced::Padding {
            top: 24.0,
            right: 26.0,
            bottom: 18.0,
            left: 26.0,
        })
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(theme::SIDE)),
            ..Default::default()
        })
        .into()
    }

    /// Etapa 1 (`orion-instalador.html` linhas 279–289): Boas-vindas + seletor de idioma + grade 2×2 `.checks`.
    fn view_step_1<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let lang_select = button(
            row![
                text(self.choices.language.label())
                    .size(13)
                    .font(FONT_MONO)
                    .color(with_alpha(theme::FG, alpha)),
                fill_hspace(),
                text("▾")
                    .size(12)
                    .color(with_alpha(theme::FG3, alpha)),
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([10, 12])
        .on_press(Message::ToggleLanguage)
        .style(input_box_button_style());

        let lang_field = column![
            text(s.s1_lang_label)
                .size(12)
                .color(with_alpha(theme::FG2, alpha)),
            vspace(6.0),
            lang_select,
        ];

        let gpu_val = if self.pc.vram_gb == 0 {
            format!("{} · {}", self.pc.gpu_name, self.pc.gpu_api)
        } else {
            format!("{} · {} GB · {}", self.pc.gpu_name, self.pc.vram_gb, self.pc.gpu_api)
        };
        let ram_val = format!("{} GB RAM", self.pc.ram_gb);
        let disk_val = self.pc.disk_summary();

        // Grade 2×2 `.checks` idêntica a `orion-instalador.html` linhas 58–63, 283–287
        let row1 = row![
            chk_card(s.s1_sys_label, &self.pc.os_label, !self.pc.is_windows_11, alpha),
            hspace(8.0),
            chk_card(s.s1_ram_label, &ram_val, false, alpha),
        ];
        let row2 = row![
            chk_card(s.s1_gpu_label, &gpu_val, false, alpha),
            hspace(8.0),
            chk_card(s.s1_disk_label, &disk_val, self.pc.c_drive_low(), alpha),
        ];

        let mut col = column![
            text(s.s1_eyebrow)
                .size(11)
                .font(FONT_MONO)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(8.0),
            text(s.s1_title)
                .size(24)
                .font(FONT_SANS_SEMIBOLD)
                .color(with_alpha(theme::FG, alpha)),
            vspace(10.0),
            text(s.s1_lead)
                .size(14)
                .color(with_alpha(theme::FG2, alpha)),
            vspace(16.0),
            lang_field,
            vspace(18.0),
            text(s.s1_pc_eyebrow)
                .size(11)
                .font(FONT_MONO)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(8.0),
            row1,
            vspace(8.0),
            row2,
        ];

        if let Some(hint) = self.pc.amber_hint(self.choices.language) {
            col = col.push(vspace(10.0)).push(
                text(hint)
                    .size(12)
                    .color(with_alpha(theme::FG3, alpha)),
            );
        }

        if !self.pc.is_windows_11 {
            col = col
                .push(vspace(8.0))
                .push(text(s.s1_win10_block).size(12).color(theme::DANGER));
        }

        col.into()
    }

    /// Etapa 2 (`orion-instalador.html` linhas 290–294): 3 campos de pasta com botão `Alterar`.
    fn view_step_2<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let f1 = folder_field(
            s.s2_app_label,
            &self.choices.app_dir.display().to_string(),
            Some((s.change, Message::CycleAppDir)),
            s.s2_app_hint,
            alpha,
        );

        let models_hint = self.pc.step2_models_hint(self.choices.language);
        let f2 = folder_field(
            s.s2_models_label,
            &self.choices.models_dir.display().to_string(),
            Some((s.change, Message::CycleModelsDisk)),
            &models_hint,
            alpha,
        );

        let f3 = folder_field(
            s.s2_data_label,
            r"%ProgramData%\Orion",
            None,
            s.s2_data_hint,
            alpha,
        );

        column![
            text(s.s2_eyebrow)
                .size(11)
                .font(FONT_MONO)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(8.0),
            text(s.s2_title)
                .size(24)
                .font(FONT_SANS_SEMIBOLD)
                .color(with_alpha(theme::FG, alpha)),
            vspace(14.0),
            f1,
            vspace(12.0),
            f2,
            vspace(12.0),
            f3,
        ]
        .into()
    }

    /// Etapa 3 (`orion-instalador.html` linhas 64–76, 295–302): Grade 2×4 `.models` + rodapé `.total`.
    fn view_step_3<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let mut grid = Column::new().spacing(6);

        for pair in self.choices.models.chunks(2) {
            let mut r = row![].spacing(8);
            for m in pair {
                let mut meta_row = row![
                    text(format_gb(m.size_gb, self.choices.language))
                        .size(9.5)
                        .font(FONT_MONO)
                        .wrapping(text::Wrapping::None)
                        .color(with_alpha(theme::FG3, alpha)),
                    hspace(6.0),
                    text(if m.min_ram_gb > self.pc.ram_gb { format!("RAM ≥{} GB", m.min_ram_gb) } else { format!("RAM ~{} GB", m.ram_gb) })
                        .size(9.5)
                        .font(FONT_MONO)
                        .wrapping(text::Wrapping::None)
                        .color(with_alpha(theme::FG3, alpha)),
                    fill_hspace(),
                ]
                .align_y(Alignment::Center);

                if m.recommended {
                    meta_row = meta_row.push(tag_pill(s.s3_rec_badge));
                }

                let card_col = column![
                    row![
                        text(&m.name)
                            .size(12)
                            .font(FONT_SANS_SEMIBOLD)
                            .width(Length::Fill)
                            .line_height(text::LineHeight::Relative(1.15))
                            .color(with_alpha(theme::FG, alpha)),
                        check_dot(m.selected),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    container(text(m.desc(self.choices.language))
                        .size(11)
                        .width(Length::Fill)
                        .line_height(text::LineHeight::Relative(1.2))
                        .color(with_alpha(theme::FG2, alpha)))
                        .height(Length::Fill),
                    meta_row,
                ]
                .spacing(3)
                .width(Length::Fill)
                .height(Length::Fill);

                let card = button(card_col)
                    .width(Length::FillPortion(1))
                    .height(Length::Fixed(76.0))
                    .padding([6, 9])
                    .on_press_maybe((m.min_ram_gb <= self.pc.ram_gb).then(|| Message::ToggleModel(m.id.to_string())))
                    .style(model_card_style(m.selected));
                r = r.push(card);
            }
            grid = grid.push(r);
        }

        let top_line = container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::LINE)),
                ..Default::default()
            });

        let summary = column![
            top_line,
            vspace(8.0),
            row![
                text(self.choices.total_line_left())
                    .size(11)
                    .font(FONT_MONO)
                    .color(with_alpha(theme::FG2, alpha)),
                fill_hspace(),
                text(s.s3_bg_note)
                    .size(11)
                    .font(FONT_MONO)
                    .color(with_alpha(theme::FG2, alpha)),
            ]
            .align_y(Alignment::Center),
        ];

        column![
            text(s.s3_eyebrow)
                .size(11)
                .font(FONT_MONO)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(4.0),
            text(s.s3_title)
                .size(22)
                .font(FONT_SANS_SEMIBOLD)
                .color(with_alpha(theme::FG, alpha)),
            vspace(4.0),
            text(s.s3_lead)
                .size(12)
                .color(with_alpha(theme::FG2, alpha)),
            vspace(8.0),
            grid,
            vspace(8.0),
            summary,
        ]
        .into()
    }

    /// Etapa 4 (`orion-instalador.html` linhas 77–84, 303–307): 4 linhas `.opt` separadas por divisória fina.
    fn view_step_4<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let items = [
            (
                s.s4_opt_vulkan.0,
                s.s4_opt_vulkan.1,
                self.choices.use_vulkan,
                Message::ToggleVulkan(!self.choices.use_vulkan),
            ),
            (
                s.s4_opt_autostart.0,
                s.s4_opt_autostart.1,
                self.choices.start_with_windows,
                Message::ToggleAutostart(!self.choices.start_with_windows),
            ),
            (
                s.s4_opt_voice.0,
                s.s4_opt_voice.1,
                self.choices.enable_voice,
                Message::ToggleVoice(!self.choices.enable_voice),
            ),
            (
                s.s4_opt_taskbar.0,
                s.s4_opt_taskbar.1,
                self.choices.pin_to_taskbar,
                Message::ToggleTaskbar(!self.choices.pin_to_taskbar),
            ),
            (
                s.s4_opt_crash.0,
                s.s4_opt_crash.1,
                self.choices.send_crash_reports,
                Message::ToggleCrashReports(!self.choices.send_crash_reports),
            ),
        ];

        let mut list = Column::new();
        for (title, sub, active, msg) in items {
            let opt_row = button(
                row![
                    column![
                        text(title).size(14).width(Length::Fill).color(with_alpha(theme::FG, alpha)),
                        vspace(2.0),
                        text(sub).size(12).width(Length::Fill).line_height(text::LineHeight::Relative(1.3)).color(with_alpha(theme::FG3, alpha)),
                    ]
                    .width(Length::Fill),
                    hspace(12.0),
                    switch_pill(active),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([11, 0])
            .on_press(msg)
            .style(transparent_row_button_style());

            let div = container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
                .style(|_theme: &Theme| container::Style {
                    background: Some(Background::Color(theme::LINE)),
                    ..Default::default()
                });

            list = list.push(opt_row).push(div);
        }

        column![
            text(s.s4_eyebrow)
                .size(11)
                .font(FONT_MONO)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(8.0),
            text(s.s4_title)
                .size(24)
                .font(FONT_SANS_SEMIBOLD)
                .color(with_alpha(theme::FG, alpha)),
            vspace(12.0),
            list,
            text_input(s.s4_wake_bundle, &self.choices.wake_bundle)
                .on_input(Message::WakeBundleChanged).size(11),
            text(s.s4_wake_hint).size(10),
        ]
        .into()
    }

    /// Etapa 5 (`orion-instalador.html` linhas 85–90, 308–310, 326–332): Barra `.track` (6 px) e log `.log`.
    fn view_step_5<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let done = self.is_install_completed();
        let has_err = self.install_error.is_some();

        let title = if has_err {
            s.s5_title_error
        } else if done {
            s.s5_title_done
        } else {
            s.s5_title_installing
        };

        let pct = self.current_install_percent();
        let bar_frac = (pct / 100.0).clamp(0.0, 1.0);
        let prog_bar = progress_bar_single(bar_frac);

        let labels = install_step_texts(self.choices.selected_models_gb(), self.choices.language);
        let mut log_col = Column::new().spacing(6);

        for (i, label) in labels.iter().enumerate() {
            let st = self.step_statuses[i];
            match st {
                StepItemStatus::Done => {
                    log_col = log_col.push(
                        text(format!("✓ {label}"))
                            .size(12)
                            .font(FONT_MONO)
                            .color(with_alpha(theme::OK, alpha)),
                    );
                }
                StepItemStatus::Running => {
                    log_col = log_col.push(
                        text(format!("… {label}"))
                            .size(12)
                            .font(FONT_MONO)
                            .color(with_alpha(theme::FG2, alpha)),
                    );
                }
                StepItemStatus::Error => {
                    log_col = log_col.push(
                        text(format!("✕ {label}"))
                            .size(12)
                            .font(FONT_MONO)
                            .color(with_alpha(theme::DANGER, alpha)),
                    );
                }
                StepItemStatus::Pending => {}
            }
        }

        let mut col = column![
            text(s.s5_eyebrow)
                .size(11)
                .font(FONT_MONO)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(8.0),
            text(title)
                .size(24)
                .font(FONT_SANS_SEMIBOLD)
                .color(with_alpha(theme::FG, alpha)),
            vspace(14.0),
            prog_bar,
            vspace(12.0),
            log_col,
        ];

        if let Some(err) = &self.install_error {
            let err_card = container(
                column![
                    text(err.clone()).size(12).color(theme::DANGER),
                    vspace(8.0),
                    row![
                        button(text(s.retry).size(12).color(theme::FG))
                            .padding([6, 12])
                            .on_press(Message::RetryInstall)
                            .style(ghost_button_style()),
                        hspace(8.0),
                        button(text(s.view_details).size(12).color(theme::FG2))
                            .padding([6, 12])
                            .on_press(Message::ToggleErrorDetails)
                            .style(ghost_button_style()),
                    ]
                ],
            )
            .padding([12, 16])
            .width(Length::Fill)
            .style(surface_card_style());
            col = col.push(vspace(12.0)).push(err_card);
        }

        col.into()
    }

    fn view_cancel_modal<'a>(&'a self, s: &'a Texts) -> Element<'a, Message> {
        let dialog = container(
            column![
                text(s.cancel_title)
                    .size(18)
                    .font(FONT_SANS_SEMIBOLD)
                    .color(theme::FG),
                vspace(16.0),
                row![
                    fill_hspace(),
                    button(text(s.cancel_keep).size(12).color(theme::FG))
                        .padding([8, 14])
                        .on_press(Message::ConfirmCancelNo)
                        .style(ghost_button_style()),
                    hspace(10.0),
                    button(text(s.cancel_confirm).size(12).color(theme::DANGER))
                        .padding([8, 14])
                        .on_press(Message::ConfirmCancelYes)
                        .style(ghost_button_style()),
                ]
                .align_y(Alignment::Center),
            ]
            .width(Length::Fixed(360.0)),
        )
        .padding([20, 24])
        .style(surface_card_style());

        container(dialog)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgba8(7, 8, 11, 0.78))),
                ..Default::default()
            })
            .into()
    }
}

fn hspace<'a>(w: f32) -> Element<'a, Message> {
    Space::new().width(Length::Fixed(w)).into()
}

fn vspace<'a>(h: f32) -> Element<'a, Message> {
    Space::new().height(Length::Fixed(h)).into()
}

fn fill_hspace<'a>() -> Element<'a, Message> {
    Space::new().width(Length::Fill).into()
}

fn fill_vspace<'a>() -> Element<'a, Message> {
    Space::new().height(Length::Fill).into()
}

/// Cartão `.chk` da Etapa 1 (`orion-instalador.html` linhas 59–63).
fn chk_card<'a>(label: &'a str, val: &str, warn: bool, alpha: f32) -> Element<'a, Message> {
    let dot_col = with_alpha(if warn { theme::BETEL } else { theme::OK }, alpha);
    let status_dot = container(Space::new().width(Length::Fixed(7.0)).height(Length::Fixed(7.0)))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(dot_col)),
            border: Border {
                radius: 99.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    container(
        column![
            text(label).size(11).color(with_alpha(theme::FG3, alpha)),
            vspace(4.0),
            row![
                status_dot,
                hspace(7.0),
                text(val.to_string())
                    .size(12)
                    .font(FONT_MONO)
                    .width(Length::Fill)
                    .line_height(text::LineHeight::Relative(1.2))
                    .color(with_alpha(theme::FG, alpha)),
            ]
            .align_y(Alignment::Center),
        ],
    )
    .width(Length::FillPortion(1))
    .height(70.0)
    .align_y(Vertical::Center)
    .padding([10, 12])
    .style(surface_card_style())
    .into()
}

/// Campo `.field` da Etapa 2 (`orion-instalador.html` linhas 50–57, 291–293).
fn folder_field<'a>(
    label: &'a str,
    path_val: &str,
    action: Option<(&'a str, Message)>,
    hint: &str,
    alpha: f32,
) -> Element<'a, Message> {
    let input_box = container(
        text(path_val.to_string())
            .size(13)
            .font(FONT_MONO)
            .color(with_alpha(theme::FG, alpha)),
    )
    .width(Length::Fill)
    .padding([9, 11])
    .style(surface_card_style());

    let input_row: Element<'a, Message> = if let Some((btn_lbl, msg)) = action {
        row![
            input_box,
            hspace(8.0),
            button(
                text(btn_lbl)
                    .size(13)
                    .font(FONT_SANS_SEMIBOLD)
                    .color(with_alpha(theme::FG, alpha))
            )
            .padding([8, 12])
            .on_press(msg)
            .style(ghost_button_style()),
        ]
        .align_y(Alignment::Center)
        .into()
    } else {
        input_box.into()
    };

    column![
        text(label).size(12).color(with_alpha(theme::FG2, alpha)),
        vspace(6.0),
        input_row,
        vspace(6.0),
        text(hint.to_string())
            .size(12)
            .color(with_alpha(theme::FG3, alpha)),
    ]
    .into()
}

/// Selo `.tag` da Etapa 3 (`orion-instalador.html` linha 75).
fn tag_pill<'a>(label: &'a str) -> Element<'a, Message> {
    container(
        text(label)
            .size(9)
            .font(FONT_SANS_SEMIBOLD)
            .wrapping(text::Wrapping::None)
            .color(theme::BETEL),
    )
    .height(16)
    .center_y(16)
    .padding([0, 6])
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(Color::from_rgba8(233, 180, 103, 0.08))),
        border: Border {
            color: Color::from_rgba8(233, 180, 103, 0.42),
            width: 1.0,
            radius: 999.0.into(),
        },
        ..Default::default()
    })
    .into()
}

fn check_dot<'a>(selected: bool) -> Element<'a, Message> {
    let label = if selected { "✓" } else { "" };
    container(text(label).size(11).color(theme::ON_PRIMARY))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .align_x(Horizontal::Center)
        .align_y(Vertical::Center)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(if selected {
                theme::RIGEL
            } else {
                Color::TRANSPARENT
            })),
            border: Border {
                color: if selected { theme::RIGEL } else { theme::FG3 },
                width: 1.0,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .into()
}

/// Switch `.sw` de 36×20 px (`orion-instalador.html` linhas 80–83).
fn switch_pill<'a>(active: bool) -> Element<'a, Message> {
    let knob = container(Space::new().width(Length::Fixed(14.0)).height(Length::Fixed(14.0)))
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(if active {
                theme::ON_PRIMARY
            } else {
                theme::FG2
            })),
            border: Border {
                radius: 99.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let content: Element<'a, Message> = if active {
        row![fill_hspace(), knob].align_y(Alignment::Center).into()
    } else {
        row![knob, fill_hspace()].align_y(Alignment::Center).into()
    };

    container(content)
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(20.0))
        .padding([3, 3])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(if active {
                theme::RIGEL
            } else {
                theme::LINE
            })),
            border: Border {
                radius: 999.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// Barra `.track` de 6 px da Etapa 5 (`orion-instalador.html` linhas 85–86).
fn progress_bar_single<'a>(fraction: f32) -> Element<'a, Message> {
    let total_w = 434.0_f32;
    let fill_w = (total_w * fraction.clamp(0.0, 1.0)).max(2.0);
    container(
        row![
            container(
                Space::new()
                    .width(Length::Fixed(fill_w))
                    .height(Length::Fixed(6.0))
            )
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::RIGEL)),
                border: Border {
                    radius: 999.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
            fill_hspace(),
        ],
    )
    .width(Length::Fill)
    .height(Length::Fixed(6.0))
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(theme::RAISE)),
        border: Border {
            radius: 999.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}

fn surface_card_style() -> impl Fn(&Theme) -> container::Style {
    |_theme: &Theme| container::Style {
        background: Some(Background::Color(theme::RAISE)),
        border: Border {
            color: theme::LINE,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

fn input_box_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    |_theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(theme::RAISE)),
            text_color: theme::FG,
            border: Border {
                color: if hovered { theme::RIGEL } else { theme::LINE },
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    }
}

fn model_card_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if selected {
            theme::MODEL_SELECTED_BG
        } else {
            theme::RAISE
        };
        let border_col = if selected || hovered {
            theme::RIGEL
        } else {
            theme::LINE
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: theme::FG,
            border: Border {
                color: border_col,
                width: 1.0,
                radius: 10.0.into(),
            },
            ..Default::default()
        }
    }
}

fn transparent_row_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    |_theme: &Theme, _status: button::Status| button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: theme::FG,
        border: Border::default(),
        ..Default::default()
    }
}

fn titlebar_btn_style(is_close: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let hover_bg = if is_close {
            Color::from_rgba8(224, 107, 107, 0.22)
        } else {
            theme::RAISE
        };
        let hover_fg = if is_close { theme::DANGER } else { theme::FG };
        button::Style {
            background: Some(Background::Color(if hovered {
                hover_bg
            } else {
                Color::TRANSPARENT
            })),
            text_color: if hovered { hover_fg } else { theme::FG3 },
            border: Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

fn ghost_button_style() -> impl Fn(&Theme, button::Status) -> button::Style {
    |_theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: Some(Background::Color(if hovered {
                theme::rgb(0x22, 0x24, 0x2a)
            } else {
                theme::RAISE
            })),
            text_color: theme::FG,
            border: Border {
                color: theme::LINE,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        }
    }
}

fn primary_button_style(enabled: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if !enabled {
            with_alpha(theme::PRIMARY, 0.4)
        } else if hovered {
            theme::rgb(0xc5, 0xd1, 0xf2)
        } else {
            theme::PRIMARY
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: theme::ON_PRIMARY,
            border: Border {
                radius: 8.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_wizard_state_machine_and_transitions_work() {
        let mut app = SetupApp::new_prototype_reference();
        assert_eq!(app.step, 1);
        assert_eq!(app.choices.language, Language::PtBr);

        let _ = app.update(Message::ToggleLanguage);
        assert_eq!(app.choices.language, Language::En);
        let _ = app.update(Message::ToggleLanguage);
        assert_eq!(app.choices.language, Language::PtBr);

        let _ = app.update(Message::NextStep);
        assert!(matches!(
            app.transition,
            Some(TransitionPhase::Out { target_step: 2, .. })
        ));
        for _ in 0..15 {
            app.tick_dt(0.03);
        }
        assert_eq!(app.step, 2);
        assert_eq!(app.sky.lit, 2);

        let _ = app.update(Message::SelectModelsDisk(1));
        assert!(app.choices.models_dir.to_string_lossy().starts_with("D:"));

        app.apply_step_immediately(3);
        assert_eq!(app.step, 3);
        assert_eq!(app.sky.lit, 3);

        let initial_gb = app.choices.selected_models_gb();
        let _ = app.update(Message::ToggleModel("phi-4-mini-3.8b".to_string()));
        assert!(app.choices.selected_models_gb() > initial_gb);

        app.apply_step_immediately(4);
        assert_eq!(app.sky.lit, 4);
        app.apply_step_immediately(3);
        assert_eq!(app.sky.lit, 4);

        app.apply_step_immediately(5);
        assert!(app.install_started);
        for _ in 0..200 {
            app.tick_dt(0.05);
        }
        assert!(app.is_install_completed());
        assert_eq!(app.sky.lit, 5);
        assert!((app.current_install_percent() - 100.0).abs() < 0.01);
        app.install_plan.rollback();
    }

    #[test]
    fn closing_during_active_install_opens_confirmation_and_rolls_back() {
        let mut app = SetupApp::new_prototype_reference();
        app.apply_step_immediately(5);
        app.tick_dt(0.9);

        let _ = app.update(Message::RequestCloseWindow);
        assert!(app.confirm_cancel_modal);

        let _ = app.update(Message::ConfirmCancelYes);
        assert!(!app.confirm_cancel_modal);
        assert!(app.install_plan.created_paths.is_empty());
    }

    #[test]
    fn duas_previas_nao_sobrescrevem_nem_desfazem_a_instalacao_uma_da_outra() {
        let mut a = SetupApp::new_prototype_reference();
        let mut b = SetupApp::new_prototype_reference();
        a.apply_step_immediately(5);
        b.apply_step_immediately(5);
        for _ in 0..200 { a.tick_dt(0.05); b.tick_dt(0.05); }
        assert!(a.is_install_completed() && b.is_install_completed());
        let files = b.install_plan.created_paths.clone();
        a.install_plan.rollback();
        assert!(files.iter().all(|p| p.exists()));
        b.install_plan.rollback();
    }
}

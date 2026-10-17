//! Máquina de estados e interface visual (`iced 0.14`) do instalador `Orion Setup`.
//!
//! Implementa fielmente `orion-instalador.html` e `ORION_INSTALADOR_ESPECIFICACAO.md`:
//! - Janela 960×600 sem borda nativa (`decorations: false`, `resizable: false`)
//! - Barra de título customizada (36 px) com arraste, seletor de idioma, som ambiente, minimizar e fechar
//! - Painel esquerdo (468×564 px) com a constelação de Orion ao vivo e legenda da estrela
//! - Painel direito (492×564 px) com barra de 5 segmentos, palco com transição Out/In de 340 ms e rodapé

use crate::audio::SoundController;
use crate::i18n::{format_gb, install_step_texts, star_name, step_counter, texts, Language, Texts};
use crate::sky::OrionSky;
use crate::system::{InstallPlan, PcInfo, SetupChoices, StepItemStatus};
use crate::theme::{self, standard_curve, with_alpha};
use iced::alignment::{Horizontal, Vertical};
use iced::event::{self, Event};
use iced::keyboard;
use iced::mouse;
use iced::time;
use iced::widget::{
    button, canvas, column, container, mouse_area, row, scrollable, stack, text, Column, Space,
};
use iced::window;
use iced::{Alignment, Background, Border, Color, Element, Length, Point, Subscription, Task, Theme};
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub const STAGE_TRANSITION_SECS: f32 = 0.34;
pub const STEP_WEIGHTS: [f32; 6] = [10.0, 12.0, 20.0, 40.0, 10.0, 8.0];

/// Fase da transição entre etapas (`0.34 s` Out com `-8 px` -> troca etapa -> `0.34 s` In de `+10 px`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransitionPhase {
    Out { target_step: usize, elapsed: f32 },
    In { elapsed: f32 },
}

#[derive(Debug, Clone)]
pub enum Message {
    AnimationTick(Instant),
    CursorMoved(Point),
    DragTitlebar,
    MinimizeWindow,
    RequestCloseWindow,
    ConfirmCancelYes,
    ConfirmCancelNo,
    SelectLanguage(Language),
    ToggleLanguage,
    ToggleSound,
    NextStep,
    PrevStep,
    SelectModelsDisk(usize),
    ToggleModel(String),
    ToggleAutostart(bool),
    ToggleVoice(bool),
    ToggleTaskbar(bool),
    ToggleCrashReports(bool),
    ToggleErrorDetails,
    RetryInstall,
    OpenOrion,
}

pub struct SetupApp {
    pub step: usize,
    pub sky: OrionSky,
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
            Some(std::env::temp_dir().join("orion-setup-preview"))
        } else {
            None
        };
        let install_plan = InstallPlan::new(root_override);
        Self {
            step: 1,
            sky: OrionSky::new(),
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
            sound: if dry_run {
                SoundController::new()
            } else {
                SoundController::start_enabled()
            },
            dry_run,
            last_tick: None,
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
        let root_override = if self.dry_run {
            Some(std::env::temp_dir().join("orion-setup-preview"))
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

    /// Avança a simulação temporal em `dt` segundos (constelação, transição de etapa e progresso de instalação).
    pub fn tick_dt(&mut self, dt: f32) {
        self.sky.step_dt(dt, theme::SKY_W, theme::BODY_H);

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

        // Progresso da etapa 5 (Instalação)
        if self.step == 5
            && self.install_started
            && !self.is_install_completed()
            && self.install_error.is_none()
        {
            let step_duration = 0.72;
            self.install_substep_progress += dt / step_duration;
            while self.install_substep_progress >= 1.0
                && self.current_install_idx < 6
                && self.install_error.is_none()
            {
                self.install_substep_progress -= 1.0;
                let idx = self.current_install_idx;
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
        let mut pct = 0.0;
        for (i, st) in self.step_statuses.iter().enumerate() {
            match st {
                StepItemStatus::Done => pct += STEP_WEIGHTS[i],
                StepItemStatus::Running => {
                    pct += STEP_WEIGHTS[i] * self.install_substep_progress.clamp(0.0, 1.0);
                }
                _ => {}
            }
        }
        pct.clamp(0.0, 100.0)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::AnimationTick(now) => {
                let dt = self
                    .last_tick
                    .map(|prev| now.saturating_duration_since(prev).as_secs_f32())
                    .unwrap_or(1.0 / 60.0)
                    .clamp(0.001, 0.05);
                self.last_tick = Some(now);
                self.tick_dt(dt);
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
            Message::ToggleSound => {
                self.sound.toggle();
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
            Message::SelectModelsDisk(idx) => {
                if let Some(d) = self.pc.disks.get(idx) {
                    self.choices.models_dir = PathBuf::from(format!("{}\\Orion\\Modelos", d.mount));
                }
                Task::none()
            }
            Message::ToggleModel(id) => {
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
                self.sound.fade_out();
                window::latest().and_then(window::close)
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let tick = time::every(Duration::from_millis(16)).map(Message::AnimationTick);
        let events = event::listen_with(|event, _status, _window_id| match event {
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
        let sky_panel = self.view_sky_panel();
        let right_pane = self.view_right_pane(s);

        let body = row![sky_panel, right_pane]
            .width(Length::Fixed(theme::WINDOW_W))
            .height(Length::Fixed(theme::BODY_H));

        let main_layout = column![titlebar, body]
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
                    radius: 10.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn view_titlebar(&self) -> Element<'_, Message> {
        let drag_zone = mouse_area(
            container(
                row![
                    hspace(14.0),
                    text("Orion Setup").size(12).color(theme::FG3),
                    fill_hspace(),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .height(Length::Fixed(theme::TITLEBAR_H)),
        )
        .on_press(Message::DragTitlebar);

        let lang_code = match self.choices.language {
            Language::PtBr => "PT",
            Language::En => "EN",
        };
        let lang_btn = button(
            text(lang_code)
                .size(11)
                .color(if self.choices.language == Language::En {
                    theme::FG
                } else {
                    theme::FG2
                }),
        )
        .padding([3, 8])
        .on_press(Message::ToggleLanguage)
        .style(pill_button_style(self.choices.language == Language::En));

        let sound_label = if self.sound.is_enabled() { "♪" } else { "♪×" };
        let sound_btn = button(
            text(sound_label)
                .size(12)
                .color(if self.sound.is_enabled() {
                    theme::FG
                } else {
                    theme::FG2
                }),
        )
        .padding([3, 8])
        .on_press(Message::ToggleSound)
        .style(pill_button_style(self.sound.is_enabled()));

        let min_btn = button(
            container(text("—").size(11).color(theme::FG2))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        )
        .width(Length::Fixed(42.0))
        .height(Length::Fixed(theme::TITLEBAR_H))
        .on_press(Message::MinimizeWindow)
        .style(window_control_style(false));

        let close_btn = button(
            container(text("✕").size(12).color(theme::FG2))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        )
        .width(Length::Fixed(42.0))
        .height(Length::Fixed(theme::TITLEBAR_H))
        .on_press(Message::RequestCloseWindow)
        .style(window_control_style(true));

        let controls = row![
            lang_btn,
            hspace(6.0),
            sound_btn,
            hspace(6.0),
            min_btn,
            close_btn
        ]
        .align_y(Alignment::Center);

        container(row![drag_zone, controls].align_y(Alignment::Center))
            .width(Length::Fixed(theme::WINDOW_W))
            .height(Length::Fixed(theme::TITLEBAR_H))
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::TITLEBAR)),
                border: Border {
                    color: theme::LINE,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn view_sky_panel(&self) -> Element<'_, Message> {
        let sky_canvas = canvas(&self.sky)
            .width(Length::Fixed(theme::SKY_W))
            .height(Length::Fixed(theme::BODY_H));

        let step_caption = step_counter(self.step as u8, self.choices.language);
        let star_lbl = if self.step == 5 && self.is_install_completed() {
            match self.choices.language {
                Language::PtBr => "Rigel · constelação completa",
                Language::En => "Rigel · constellation complete",
            }
        } else {
            star_name(self.step as u8, self.choices.language)
        };

        let betel_faded = with_alpha(theme::BETEL, self.star_label_alpha);

        let overlay = column![
            fill_vspace(),
            container(
                row![
                    text("Orion").size(22).color(theme::FG),
                    fill_hspace(),
                    column![
                        text(step_caption).size(12).color(theme::FG3),
                        text(star_lbl).size(12).color(betel_faded),
                    ]
                    .align_x(Horizontal::Right)
                    .spacing(2),
                ]
                .align_y(Alignment::End),
            )
            .width(Length::Fill)
            .padding([24, 28])
        ]
        .width(Length::Fixed(theme::SKY_W))
        .height(Length::Fixed(theme::BODY_H));

        container(stack![sky_canvas, overlay])
            .width(Length::Fixed(theme::SKY_W))
            .height(Length::Fixed(theme::BODY_H))
            .style(|_theme: &Theme| container::Style {
                border: Border {
                    color: theme::LINE,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn view_right_pane<'a>(&'a self, s: &'a Texts) -> Element<'a, Message> {
        // Barra superior de 5 segmentos
        let mut stepbar = row![].spacing(6);
        for idx in 1..=5 {
            let color = if idx < self.step || (idx == 5 && self.step == 5 && self.is_install_completed()) {
                theme::RIGEL
            } else if idx == self.step {
                theme::BETEL
            } else {
                theme::LINE
            };
            stepbar = stepbar.push(
                container(Space::new().width(Length::Fill).height(Length::Fixed(2.0))).style(
                    move |_theme: &Theme| container::Style {
                        background: Some(Background::Color(color)),
                        border: Border {
                            radius: 1.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                ),
            );
        }

        let (offset_y, stage_alpha) = self.stage_transform();
        let top_pad = (8.0 + offset_y).max(0.0);

        let stage_body = match self.step {
            1 => self.view_step_1(s, stage_alpha),
            2 => self.view_step_2(s, stage_alpha),
            3 => self.view_step_3(s, stage_alpha),
            4 => self.view_step_4(s, stage_alpha),
            _ => self.view_step_5(s, stage_alpha),
        };

        let stage = scrollable(
            column![vspace(top_pad), stage_body]
                .width(Length::Fill)
                .padding([6, 2]),
        )
        .width(Length::Fill)
        .height(Length::Fill);

        // Rodapé com legenda contextual, botão Voltar e botão primário
        let footer_caption = match self.step {
            1 => s.s1_footer.to_string(),
            2 => s.s2_footer.to_string(),
            3 => self.pc.step3_footer(self.choices.language),
            4 => s.s4_footer.to_string(),
            _ => {
                if self.is_install_completed() {
                    s.s5_footer_done.to_string()
                } else {
                    s.s5_footer_installing.to_string()
                }
            }
        };

        let show_back = self.step > 1 && self.step < 5;
        let back_el: Element<'a, Message> = if show_back {
            button(text(s.back).size(13).color(theme::FG2))
                .padding([8, 14])
                .on_press(Message::PrevStep)
                .style(ghost_button_style())
                .into()
        } else {
            hspace(1.0)
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
            _ => {
                if self.is_install_completed() {
                    (s.open_orion, true, Some(Message::OpenOrion))
                } else {
                    (s.s5_eyebrow, false, None)
                }
            }
        };

        let mut next_btn = button(
            text(next_label).size(13).color(if next_enabled {
                theme::ON_PRIMARY
            } else {
                theme::FG3
            }),
        )
        .padding([9, 18])
        .style(primary_button_style(next_enabled));

        if let Some(msg) = next_msg {
            next_btn = next_btn.on_press(msg);
        }

        let footer = container(
            row![
                text(footer_caption).size(11).color(theme::FG3),
                fill_hspace(),
                back_el,
                hspace(8.0),
                next_btn,
            ]
            .align_y(Alignment::Center)
            .width(Length::Fill),
        )
        .padding([12, 0])
        .style(|_theme: &Theme| container::Style {
            border: Border {
                color: theme::LINE,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        });

        container(
            column![stepbar, vspace(14.0), stage, footer]
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fixed(theme::PANEL_W))
        .height(Length::Fixed(theme::BODY_H))
        .padding([24, 34])
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(theme::SIDE)),
            ..Default::default()
        })
        .into()
    }

    fn view_step_1<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let lang_row = row![
            text(s.s1_lang_label)
                .size(12)
                .color(with_alpha(theme::FG2, alpha)),
            fill_hspace(),
            button(
                text("Português (Brasil)")
                    .size(12)
                    .color(if self.choices.language == Language::PtBr {
                        theme::FG
                    } else {
                        theme::FG2
                    }),
            )
            .padding([5, 10])
            .on_press(Message::SelectLanguage(Language::PtBr))
            .style(pill_button_style(self.choices.language == Language::PtBr)),
            hspace(6.0),
            button(
                text("English")
                    .size(12)
                    .color(if self.choices.language == Language::En {
                        theme::FG
                    } else {
                        theme::FG2
                    }),
            )
            .padding([5, 10])
            .on_press(Message::SelectLanguage(Language::En))
            .style(pill_button_style(self.choices.language == Language::En)),
        ]
        .align_y(Alignment::Center);

        let gpu_val = format!(
            "{} · {} GB · {}",
            self.pc.gpu_name, self.pc.vram_gb, self.pc.gpu_api
        );
        let ram_val = format!("{} GB RAM", self.pc.ram_gb);
        let disk_val = self.pc.disk_summary();
        let disk_color = if self.pc.c_drive_low() {
            theme::BETEL
        } else {
            theme::FG
        };

        let mut pc_col = column![
            text(s.s1_pc_eyebrow)
                .size(11)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(6.0),
            kv_row(s.s1_sys_label, &self.pc.os_label, theme::FG, alpha),
            divider(),
            kv_row(s.s1_ram_label, &ram_val, theme::FG, alpha),
            divider(),
            kv_row(s.s1_gpu_label, &gpu_val, theme::FG, alpha),
            divider(),
            kv_row(s.s1_disk_label, &disk_val, disk_color, alpha),
        ]
        .spacing(6);

        if let Some(hint) = self.pc.amber_hint(self.choices.language) {
            pc_col = pc_col
                .push(vspace(4.0))
                .push(text(hint).size(11).color(with_alpha(theme::BETEL, alpha)));
        }

        if !self.pc.is_windows_11 {
            pc_col = pc_col
                .push(vspace(4.0))
                .push(text(s.s1_win10_block).size(12).color(theme::DANGER));
        }

        let card = container(pc_col)
            .padding([13, 15])
            .width(Length::Fill)
            .style(surface_card_style());

        column![
            text(s.s1_eyebrow)
                .size(11)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(6.0),
            text(s.s1_title)
                .size(23)
                .color(with_alpha(theme::FG, alpha)),
            vspace(8.0),
            text(s.s1_lead)
                .size(13)
                .color(with_alpha(theme::FG2, alpha)),
            vspace(14.0),
            lang_row,
            vspace(14.0),
            card,
        ]
        .into()
    }

    fn view_step_2<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let app_card = container(
            column![
                row![
                    text(s.s2_app_label)
                        .size(12)
                        .color(with_alpha(theme::FG2, alpha)),
                    fill_hspace(),
                    text(s.s2_app_hint)
                        .size(11)
                        .color(with_alpha(theme::FG3, alpha)),
                ],
                vspace(4.0),
                text(self.choices.app_dir.display().to_string())
                    .size(12)
                    .color(with_alpha(theme::FG, alpha)),
            ],
        )
        .padding([11, 14])
        .width(Length::Fill)
        .style(surface_card_style());

        let mut disk_buttons = row![].spacing(6);
        for (idx, d) in self.pc.disks.iter().enumerate() {
            let expected = PathBuf::from(format!("{}\\Orion\\Modelos", d.mount));
            let active = self.choices.models_dir == expected;
            disk_buttons = disk_buttons.push(
                button(
                    text(format!("{} ({} GB)", d.mount, d.free_gb))
                        .size(11)
                        .color(if active { theme::FG } else { theme::FG2 }),
                )
                .padding([4, 10])
                .on_press(Message::SelectModelsDisk(idx))
                .style(pill_button_style(active)),
            );
        }

        let models_card = container(
            column![
                row![
                    text(s.s2_models_label)
                        .size(12)
                        .color(with_alpha(theme::FG2, alpha)),
                    fill_hspace(),
                    text(format!(
                        "~{}",
                        format_gb(self.choices.selected_models_gb(), self.choices.language)
                    ))
                    .size(11)
                    .color(with_alpha(theme::BETEL, alpha)),
                ],
                vspace(6.0),
                text(self.choices.models_dir.display().to_string())
                    .size(12)
                    .color(with_alpha(theme::FG, alpha)),
                vspace(8.0),
                disk_buttons,
                vspace(6.0),
                text(self.pc.step2_models_hint(self.choices.language))
                    .size(11)
                    .color(with_alpha(theme::FG3, alpha)),
            ],
        )
        .padding([11, 14])
        .width(Length::Fill)
        .style(surface_card_style());

        let data_card = container(
            column![
                text(s.s2_data_label)
                    .size(12)
                    .color(with_alpha(theme::FG2, alpha)),
                vspace(4.0),
                text(self.choices.data_dir.display().to_string())
                    .size(12)
                    .color(with_alpha(theme::FG, alpha)),
                vspace(4.0),
                text(s.s2_data_hint)
                    .size(11)
                    .color(with_alpha(theme::FG3, alpha)),
            ],
        )
        .padding([11, 14])
        .width(Length::Fill)
        .style(surface_card_style());

        column![
            text(s.s2_eyebrow)
                .size(11)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(6.0),
            text(s.s2_title)
                .size(23)
                .color(with_alpha(theme::FG, alpha)),
            vspace(14.0),
            app_card,
            vspace(10.0),
            models_card,
            vspace(10.0),
            data_card,
        ]
        .into()
    }

    fn view_step_3<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let mut grid = Column::new().spacing(8);

        for pair in self.choices.models.chunks(2) {
            let mut r = row![].spacing(8);
            for m in pair {
                let card = button(
                    column![
                        row![
                            check_dot(m.selected),
                            hspace(8.0),
                            text(m.name)
                                .size(12)
                                .color(with_alpha(theme::FG, alpha)),
                        ]
                        .align_y(Alignment::Center),
                        vspace(4.0),
                        text(m.desc(self.choices.language))
                            .size(11)
                            .color(with_alpha(theme::FG2, alpha)),
                        vspace(6.0),
                        row![
                            text(m.meta_line(self.choices.language))
                                .size(10)
                                .color(with_alpha(theme::FG3, alpha)),
                            fill_hspace(),
                            if m.recommended {
                                badge(s.s3_rec_badge, true)
                            } else {
                                hspace(1.0)
                            },
                        ]
                        .align_y(Alignment::Center),
                    ]
                    .width(Length::Fill),
                )
                .width(Length::FillPortion(1))
                .padding([10, 11])
                .on_press(Message::ToggleModel(m.id.to_string()))
                .style(selectable_card_style(m.selected));
                r = r.push(card);
            }
            grid = grid.push(r);
        }

        let summary = container(
            row![
                text(self.choices.total_line_left())
                    .size(12)
                    .color(with_alpha(theme::FG, alpha)),
                fill_hspace(),
                text(s.s3_bg_note)
                    .size(11)
                    .color(with_alpha(theme::FG3, alpha)),
            ]
            .align_y(Alignment::Center),
        )
        .padding([10, 14])
        .width(Length::Fill)
        .style(surface_card_style());

        column![
            text(s.s3_eyebrow)
                .size(11)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(6.0),
            text(s.s3_title)
                .size(23)
                .color(with_alpha(theme::FG, alpha)),
            vspace(6.0),
            text(s.s3_lead)
                .size(12)
                .color(with_alpha(theme::FG2, alpha)),
            vspace(12.0),
            grid,
            vspace(10.0),
            summary,
        ]
        .into()
    }

    fn view_step_4<'a>(&'a self, s: &'a Texts, alpha: f32) -> Element<'a, Message> {
        let items = [
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

        let mut list = Column::new().spacing(8);
        for (title, sub, active, msg) in items {
            let row_btn = button(
                row![
                    column![
                        text(title).size(13).color(with_alpha(theme::FG, alpha)),
                        vspace(2.0),
                        text(sub).size(11).color(with_alpha(theme::FG2, alpha)),
                    ]
                    .width(Length::Fill),
                    hspace(12.0),
                    switch_pill(active),
                ]
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([12, 14])
            .on_press(msg)
            .style(selectable_card_style(active));

            list = list.push(row_btn);
        }

        column![
            text(s.s4_eyebrow)
                .size(11)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(6.0),
            text(s.s4_title)
                .size(23)
                .color(with_alpha(theme::FG, alpha)),
            vspace(14.0),
            list,
        ]
        .into()
    }

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
        let mut log_col = Column::new().spacing(8);

        for (i, label) in labels.iter().enumerate() {
            let st = self.step_statuses[i];
            let (dot_str, dot_col, text_col) = match st {
                StepItemStatus::Done => ("✓", theme::OK, theme::FG),
                StepItemStatus::Running => ("●", theme::BETEL, theme::FG),
                StepItemStatus::Pending => ("·", theme::FG3, theme::FG3),
                StepItemStatus::Error => ("!", theme::DANGER, theme::DANGER),
            };

            log_col = log_col.push(
                row![
                    text(dot_str)
                        .size(12)
                        .color(with_alpha(dot_col, alpha)),
                    hspace(10.0),
                    text(label.clone())
                        .size(12)
                        .color(with_alpha(text_col, alpha)),
                ]
                .align_y(Alignment::Center),
            );
        }

        let log_card = container(log_col)
            .padding([13, 15])
            .width(Length::Fill)
            .style(surface_card_style());

        let mut col = column![
            text(s.s5_eyebrow)
                .size(11)
                .color(with_alpha(theme::FG3, alpha)),
            vspace(6.0),
            text(title)
                .size(23)
                .color(with_alpha(theme::FG, alpha)),
            vspace(14.0),
            row![
                text(format!("{:.0}%", pct))
                    .size(12)
                    .color(with_alpha(theme::FG2, alpha)),
                fill_hspace(),
            ],
            vspace(6.0),
            prog_bar,
            vspace(14.0),
            log_card,
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
                            .style(pill_button_style(true)),
                        hspace(8.0),
                        button(text(s.view_details).size(12).color(theme::FG2))
                            .padding([6, 12])
                            .on_press(Message::ToggleErrorDetails)
                            .style(pill_button_style(false)),
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
                text(s.cancel_title).size(18).color(theme::FG),
                vspace(16.0),
                row![
                    fill_hspace(),
                    button(text(s.cancel_keep).size(12).color(theme::FG))
                        .padding([8, 14])
                        .on_press(Message::ConfirmCancelNo)
                        .style(pill_button_style(false)),
                    hspace(10.0),
                    button(text(s.cancel_confirm).size(12).color(theme::DANGER))
                        .padding([8, 14])
                        .on_press(Message::ConfirmCancelYes)
                        .style(pill_button_style(true)),
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

fn kv_row<'a>(k: &'a str, v: &str, v_col: Color, alpha: f32) -> Element<'a, Message> {
    row![
        text(k).size(12).color(with_alpha(theme::FG2, alpha)),
        fill_hspace(),
        text(v.to_string()).size(12).color(with_alpha(v_col, alpha)),
    ]
    .align_y(Alignment::Center)
    .into()
}

fn divider<'a>() -> Element<'a, Message> {
    container(Space::new().width(Length::Fill).height(Length::Fixed(1.0)))
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(theme::LINE)),
            ..Default::default()
        })
        .into()
}

fn badge<'a>(label: &'a str, warm: bool) -> Element<'a, Message> {
    let fg = if warm { theme::BETEL } else { theme::RIGEL };
    let bg = if warm {
        Color::from_rgba8(233, 180, 103, 0.12)
    } else {
        Color::from_rgba8(164, 179, 222, 0.12)
    };
    let border_col = if warm {
        Color::from_rgba8(233, 180, 103, 0.28)
    } else {
        Color::from_rgba8(164, 179, 222, 0.24)
    };
    container(text(label).size(9).color(fg))
        .padding([2, 6])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                color: border_col,
                width: 1.0,
                radius: 99.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn check_dot<'a>(selected: bool) -> Element<'a, Message> {
    let label = if selected { "✓" } else { "" };
    container(text(label).size(10).color(theme::ON_PRIMARY))
        .width(Length::Fixed(15.0))
        .height(Length::Fixed(15.0))
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
                width: 1.4,
                radius: 4.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn switch_pill<'a>(active: bool) -> Element<'a, Message> {
    let knob = container(Space::new().width(Length::Fixed(12.0)).height(Length::Fixed(12.0)))
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
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(18.0))
        .padding([2, 3])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(if active {
                theme::RIGEL
            } else {
                theme::RAISE
            })),
            border: Border {
                color: if active { theme::RIGEL } else { theme::LINE },
                width: 1.0,
                radius: 99.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn progress_bar_single<'a>(fraction: f32) -> Element<'a, Message> {
    let total_w = 416.0_f32;
    let fill_w = (total_w * fraction.clamp(0.0, 1.0)).max(2.0);
    container(
        row![
            container(
                Space::new()
                    .width(Length::Fixed(fill_w))
                    .height(Length::Fixed(4.0))
            )
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(theme::RIGEL)),
                ..Default::default()
            }),
            fill_hspace(),
        ],
    )
    .width(Length::Fill)
    .height(Length::Fixed(4.0))
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(theme::LINE)),
        border: Border {
            radius: 99.0.into(),
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
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}

fn selectable_card_style(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
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
                radius: 6.0.into(),
            },
            ..Default::default()
        }
    }
}

fn pill_button_style(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme: &Theme, _status: button::Status| button::Style {
        background: Some(Background::Color(if active {
            theme::MODEL_SELECTED_BG
        } else {
            Color::TRANSPARENT
        })),
        text_color: if active { theme::FG } else { theme::FG2 },
        border: Border {
            color: if active { theme::RIGEL } else { theme::LINE },
            width: 1.0,
            radius: 99.0.into(),
        },
        ..Default::default()
    }
}

fn window_control_style(is_close: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if hovered {
            if is_close {
                Color::from_rgba8(240, 124, 136, 0.18)
            } else {
                Color::from_rgba8(255, 255, 255, 0.06)
            }
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: if hovered && is_close {
                theme::DANGER
            } else {
                theme::FG2
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
                theme::RAISE
            } else {
                Color::TRANSPARENT
            })),
            text_color: if hovered { theme::FG } else { theme::FG2 },
            border: Border {
                color: if hovered { theme::LINE } else { Color::TRANSPARENT },
                width: 1.0,
                radius: 6.0.into(),
            },
            ..Default::default()
        }
    }
}

fn primary_button_style(enabled: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_theme: &Theme, status: button::Status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if !enabled {
            theme::RAISE
        } else if hovered {
            theme::RIGEL
        } else {
            theme::PRIMARY
        };
        button::Style {
            background: Some(Background::Color(bg)),
            text_color: if enabled {
                theme::ON_PRIMARY
            } else {
                theme::FG3
            },
            border: Border {
                radius: 6.0.into(),
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

        // Alterna idioma PT <-> EN sem perder estado
        let _ = app.update(Message::ToggleLanguage);
        assert_eq!(app.choices.language, Language::En);
        let _ = app.update(Message::ToggleLanguage);
        assert_eq!(app.choices.language, Language::PtBr);

        // Avança da etapa 1 para 2 com transição Out (0.34 s) + In (0.34 s)
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

        // Na etapa 2, escolhe o disco D:
        let _ = app.update(Message::SelectModelsDisk(1));
        assert!(app.choices.models_dir.to_string_lossy().starts_with("D:"));

        // Avança para etapa 3 (Cinturão)
        app.apply_step_immediately(3);
        assert_eq!(app.step, 3);
        assert_eq!(app.sky.lit, 3);

        // Alterna um modelo adicional (Phi-4-mini 3.8B)
        let initial_gb = app.choices.selected_models_gb();
        let _ = app.update(Message::ToggleModel("phi-4-mini-3.8b".to_string()));
        assert!(app.choices.selected_models_gb() > initial_gb);

        // Avança para etapa 4 (Saiph) e volta para etapa 3 sem apagar Saiph
        app.apply_step_immediately(4);
        assert_eq!(app.sky.lit, 4);
        app.apply_step_immediately(3);
        assert_eq!(app.sky.lit, 4);

        // Avança para etapa 5 (Instalação) e simula conclusão até Rigel acender
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
        app.tick_dt(0.8);

        let _ = app.update(Message::RequestCloseWindow);
        assert!(app.confirm_cancel_modal);

        let _ = app.update(Message::ConfirmCancelYes);
        assert!(!app.confirm_cancel_modal);
        assert!(app.install_plan.created_paths.is_empty());
    }
}

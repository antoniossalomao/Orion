//! Ilha do desktop: janela independente, sem ativar captura nem conceder ações.
//! (REGRAS.md: ponte de desktop só abre e cola o que você aprovou;
//! palmas só abrem; nunca aprovam nem executam.)
use crate::app::{App, Msg};
use iced::{window, Size, Task};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode { Compact, Desktop, Expanded }

impl Mode {
    pub fn size(self) -> Size {
        match self {
            Self::Compact => Size::new(156.0, 36.0),
            Self::Desktop => Size::new(344.0, 60.0),
            Self::Expanded => Size::new(496.0, 264.0),
        }
    }
}

pub struct DesktopIsland {
    pub id: Option<window::Id>,
    pub mode: Mode,
    pub desktop: bool,
    pub focused: bool,
    pub locked: bool,
    pub shown: bool,
    ready: bool,
    transition: Option<(Instant, Size)>,
    wake_generation: u64,
}

impl Default for DesktopIsland {
    fn default() -> Self {
        Self { id: None, mode: Mode::Desktop, desktop: true, focused: false, locked: false, shown: false, ready: false, transition: None, wake_generation: 0 }
    }
}

impl DesktopIsland {
    pub fn animating(&self) -> bool {
        self.transition.is_some_and(|(start, _)| start.elapsed() < Duration::from_millis(240))
    }

    fn size(&self) -> Size {
        let target = self.mode.size();
        let Some((start, from)) = self.transition else { return target };
        let progress = (start.elapsed().as_secs_f32() / 0.24).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - progress).powi(3);
        Size::new(from.width + (target.width - from.width) * eased, from.height + (target.height - from.height) * eased)
    }

    fn set_mode(&mut self, mode: Mode) {
        if self.mode != mode {
            self.transition = Some((Instant::now(), self.size()));
            self.mode = mode;
        }
    }
}

impl App {
    pub fn view_window(&self, id: window::Id) -> crate::ui::El<'_> {
        if self.desktop_island.id == Some(id) {
            crate::i18n::set_english(self.p.english);
            crate::views::mini::view(self)
        } else { self.view() }
    }

    /// O contexto Windows lê só a classe da janela, nunca seu título ou conteúdo.
    pub fn update_desktop_island(&mut self, event: Event) -> Task<Msg> {
        match event {
            Event::Opened(id) => {
                self.desktop_island.id = Some(id);
                self.desktop_island.ready = true;
                return self.layout_desktop_island();
            }
            Event::Context { desktop, locked } => {
                self.desktop_island.desktop = desktop;
                self.desktop_island.locked = locked;
                if locked || !self.logged {
                    self.desktop_island.focused = false;
                    self.desktop_island.set_mode(Mode::Compact);
                } else if !self.desktop_island.focused {
                    self.desktop_island.set_mode(if desktop { Mode::Desktop } else { Mode::Compact });
                }
                if desktop && !locked && self.logged && self.desktop_island.ready {
                    if let Some(id) = self.desktop_island.id {
                        return self.layout_desktop_island().chain(window::minimize(id, false));
                    }
                }
            }
            Event::Focus(focused) => {
                self.desktop_island.focused = focused;
                if !focused {
                    self.desktop_island.set_mode(if self.desktop_island.desktop { Mode::Desktop } else { Mode::Compact });
                }
            }
            Event::Wake => {
                if self.desktop_island.locked { return Task::none(); }
                if !self.logged {
                    return self.update(Msg::Mostrar);
                }
                self.desktop_island.focused = true;
                self.desktop_island.set_mode(Mode::Expanded);
                self.desktop_island.wake_generation = self.desktop_island.wake_generation.wrapping_add(1);
                let generation = self.desktop_island.wake_generation;
                // O Windows pode negar foco a uma ativação por som. Nesse caso, recolhe após 6 s.
                let settle = Task::perform(async move { tokio::time::sleep(Duration::from_secs(6)).await; generation }, |generation| Msg::DesktopIsland(Event::Settle(generation)));
                let layout = self.layout_desktop_island();
                return match self.desktop_island.id {
                    Some(id) => Task::batch([layout.chain(window::gain_focus(id)).chain(iced::widget::operation::focus("desktop-island-composer")), settle]),
                    None => layout,
                };
            }
            Event::Collapse => {
                self.desktop_island.focused = false;
                self.desktop_island.set_mode(if self.desktop_island.desktop { Mode::Desktop } else { Mode::Compact });
            }
            Event::Refresh | Event::Frame => {}
            Event::Settle(generation) => {
                if generation != self.desktop_island.wake_generation || self.desktop_island.mode != Mode::Expanded { return Task::none(); }
                if let Some(id) = self.desktop_island.id {
                    return window::run(id, |window| {
                        let focused = window.window_handle().ok().map(|handle| handle.as_raw()).is_some_and(|handle| match handle {
                            window::raw_window_handle::RawWindowHandle::Win32(handle) => orion_platform::window_focused(handle.hwnd.get()),
                            _ => true,
                        });
                        Msg::DesktopIsland(Event::Focus(focused))
                    });
                }
            }
        }
        self.layout_desktop_island()
    }

    pub fn layout_desktop_island(&mut self) -> Task<Msg> {
        if !self.desktop_island.ready { return Task::none(); }
        let Some(id) = self.desktop_island.id else { return Task::none() };
        let show = self.logged && self.onboarding.is_none() && !self.desktop_island.locked;
        let changed = show != self.desktop_island.shown;
        self.desktop_island.shown = show;
        let visibility = if changed { window::set_mode(id, if show { window::Mode::Windowed } else { window::Mode::Hidden }) } else { Task::none() };
        if show {
            let size = if self.p.reduced || self.system_reduces_motion { self.desktop_island.mode.size() } else { self.desktop_island.size() };
            let radius = if self.desktop_island.mode == Mode::Expanded { 30 } else { 32 };
            let interactive = self.desktop_island.mode == Mode::Expanded;
            return window::resize(id, size).chain(window::run(id, move |handle| {
                if let Ok(handle) = handle.window_handle() {
                    if let window::raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                        orion_platform::style_island_window(h.hwnd.get(), radius, interactive);
                    }
                }
                Msg::Noop
            })).chain(visibility);
        }
        visibility
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Opened(window::Id),
    Context { desktop: bool, locked: bool },
    Focus(bool),
    Wake,
    Collapse,
    Refresh,
    Frame,
    Settle(u64),
}

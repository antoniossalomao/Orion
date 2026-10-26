//! Mini tela "O que mudou": o céu do instalador com as sete estrelas acesas e as notas da versão
//! que acabou de ser instalada (ORION - INSTALADOR, mesma arte reaproveitada na atualização).

use crate::sky::OrionSky;
use crate::theme;
use iced::widget::{button, column, container, image as iced_image, row, scrollable, text, Space};
use iced::{time, Element, Length, Size, Subscription, Task, Theme};
use orion_updater::WhatsNew;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SKY_W: u32 = 220;
const SKY_H: u32 = 300;

#[derive(Debug, Clone)]
enum Message {
    Tick(Instant),
    Close,
}

/// Textos da janela (pt-BR e en), como no resto do instalador.
struct Texts {
    title: &'static str,
    heading: &'static str,
    version: &'static str,
    fallback: &'static str,
    cont: &'static str,
}

fn texts(english: bool) -> Texts {
    if english {
        Texts { title: "What's new in Orion", heading: "What's new", version: "Version", fallback: "Fixes and improvements.", cont: "Continue" }
    } else {
        Texts { title: "O que mudou no Orion", heading: "O que mudou", version: "Versão", fallback: "Correções e melhorias.", cont: "Continuar" }
    }
}

struct State {
    english: bool,
    app_dir: PathBuf,
    news: WhatsNew,
    sky: OrionSky,
    handle: iced_image::Handle,
    last: Option<Instant>,
}

impl State {
    fn new(app_dir: PathBuf, news: WhatsNew, english: bool) -> Self {
        let mut sky = OrionSky::new();
        sky.set_size(SKY_W, SKY_H);
        sky.one_star_per_step();
        sky.compact_constellation(0.62);
        sky.light_to(7);
        sky.step_dt(0.016, 0.0, 0.0);
        let handle = sky.render_image_handle();
        Self { english, app_dir, news, sky, handle, last: None }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick(now) => {
                let dt = self.last.map(|p| now.saturating_duration_since(p).as_secs_f32()).unwrap_or(0.033);
                self.last = Some(now);
                self.sky.step_dt(dt, 0.0, 0.0);
                self.handle = self.sky.render_image_handle();
            }
            Message::Close => {
                orion_updater::dismiss_whats_new(&self.app_dir);
                return iced::exit();
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let t = texts(self.english);
        let notes = if self.news.notes.trim().is_empty() { t.fallback } else { self.news.notes.trim() };
        let subtitle = if self.news.previous_version.is_empty() {
            format!("{} {}", t.version, self.news.version)
        } else {
            format!("{} → {}", self.news.previous_version, self.news.version)
        };
        let sky = iced_image(self.handle.clone()).width(Length::Fixed(SKY_W as f32)).height(Length::Fixed(SKY_H as f32));
        let right = column![
            text(t.heading).size(22).color(theme::FG),
            text(subtitle).size(13).color(theme::BETEL),
            Space::new().height(Length::Fixed(6.0)),
            scrollable(text(notes).size(14).color(theme::FG2)).height(Length::Fill),
            button(t.cont).on_press(Message::Close),
        ]
        .spacing(8)
        .width(Length::Fill);
        container(row![sky, Space::new().width(Length::Fixed(20.0)), right])
            .padding(0)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style { background: Some(theme::SIDE.into()), ..Default::default() })
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        time::every(Duration::from_millis(33)).map(Message::Tick)
    }
}

/// Abre a janela se houver novidade ainda não vista em `app_dir`; volta quando ela fecha.
pub fn run(app_dir: PathBuf, english: bool) -> iced::Result {
    let Some(news) = orion_updater::pending_whats_new(&app_dir) else { return Ok(()) };
    if std::env::var_os("ICED_BACKEND").is_none() {
        std::env::set_var("ICED_BACKEND", "tiny-skia");
    }
    let dir = app_dir.clone();
    let result = iced::application(move || (State::new(app_dir.clone(), news.clone(), english), Task::none()), State::update, State::view)
        .title(|s: &State| texts(s.english).title.to_string())
        .theme(|_: &State| Theme::Dark)
        .subscription(State::subscription)
        .window_size(Size::new(640.0, 300.0))
        .resizable(false)
        .run();
    // Fechar pelo X ou Alt+F4 também conta como visto: o aviso não volta na próxima abertura.
    orion_updater::dismiss_whats_new(&dir);
    result
}

//! Janela do desinstalador: três perguntas separadas (app, modelos, dados). O vault nunca é
//! tocado, e modelos e dados começam desmarcados.

use crate::register::{uninstall, UninstallOptions, UninstallReport, WindowsRegistrar};
use crate::theme;
use iced::widget::{button, checkbox, column, container, row, text, Space};
use iced::{Element, Length, Size, Task, Theme};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Message {
    Models(bool),
    Data(bool),
    Remove,
    Close,
}

struct State {
    dir: PathBuf,
    options: UninstallOptions,
    result: Option<Result<UninstallReport, String>>,
}

impl State {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Models(v) => self.options.remove_models = v,
            Message::Data(v) => self.options.remove_data = v,
            Message::Remove => self.result = Some(uninstall(&self.dir, &WindowsRegistrar, self.options)),
            Message::Close => return iced::exit(),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match &self.result {
            None => column![
                text("Desinstalar o Orion").size(20).color(theme::FG),
                text("O app e os atalhos sempre saem. O seu vault nunca é tocado.").size(13).color(theme::FG2),
                Space::new().height(Length::Fixed(8.0)),
                checkbox(true).label("Remover o app (obrigatório)").text_size(14),
                checkbox(self.options.remove_models).label("Remover os modelos baixados pelo instalador").on_toggle(Message::Models).text_size(14),
                checkbox(self.options.remove_data).label("Remover os dados (banco, configurações, memória)").on_toggle(Message::Data).text_size(14),
                text("Dados removidos não voltam. Se for reinstalar, deixe desmarcado.").size(12).color(theme::FG3),
                Space::new().height(Length::Fixed(12.0)),
                row![button("Cancelar").on_press(Message::Close), button("Desinstalar").on_press(Message::Remove)].spacing(10),
            ]
            .spacing(10)
            .into(),
            Some(Ok(r)) => column![
                text("Orion removido").size(20).color(theme::OK),
                text(format!("{} arquivos do app, {} modelos{}.", r.removed_files, r.removed_models, if r.removed_data { ", dados" } else { "" })).size(13).color(theme::FG2),
                text(if r.kept_modified.is_empty() { String::new() } else { format!("Mantidos por terem sido alterados: {}", r.kept_modified.join(", ")) }).size(12).color(theme::FG3),
                button("Fechar").on_press(Message::Close),
            ]
            .spacing(10)
            .into(),
            Some(Err(e)) => column![text("Não consegui desinstalar").size(20).color(theme::DANGER), text(e.clone()).size(13).color(theme::FG2), button("Fechar").on_press(Message::Close)].spacing(10).into(),
        };
        container(body).padding(24).width(Length::Fill).height(Length::Fill).style(|_| container::Style { background: Some(theme::BG.into()), ..Default::default() }).into()
    }
}

/// Abre a janela e só volta quando ela fecha.
pub fn run(dir: PathBuf) -> iced::Result {
    if std::env::var_os("ICED_BACKEND").is_none() {
        std::env::set_var("ICED_BACKEND", "tiny-skia");
    }
    iced::application(move || (State { dir: dir.clone(), options: UninstallOptions::default(), result: None }, Task::none()), State::update, State::view)
        .title(|_: &State| "Desinstalar o Orion".to_string())
        .theme(|_: &State| Theme::Dark)
        .window_size(Size::new(520.0, 360.0))
        .resizable(false)
        .run()
}

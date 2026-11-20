//! Ilha do desktop: cápsula em repouso, painel de conversa ao ser chamada.
//! (REGRAS.md: palmas só abrem; nunca aprovam nem executam.)
use crate::app::{App, Msg};
use crate::data::ChatMsg;
use crate::desktop_island::{Event, Mode};
use crate::theme as t;
use crate::ui::*;
use iced::alignment::Vertical;
use iced::widget::{button, canvas, column, container, row, scrollable, text_editor, Space};
use iced::{Border, Color, Length};

fn action(event: Event) -> Msg { Msg::DesktopIsland(event) }

pub fn settings() -> El<'static> {
    setting(crate::i18n::text("Ilha do desktop"), crate::i18n::text("Fica no topo do desktop e recolhe ao focar outro app. Use o atalho configurado para chamar."), btn(crate::i18n::text("Mostrar"), Kind::Normal, true, None, Some(Msg::Mini(true))))
}

/// Cinturão e onda estáticos no repouso; nenhum pulso simula microfone ligado.
struct Signal { active: bool, compact: bool }
impl canvas::Program<Msg> for Signal {
    type State = ();
    fn draw(&self, _: &(), renderer: &iced::Renderer, _: &iced::Theme, bounds: iced::Rectangle, _: iced::mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let cy = bounds.height / 2.0;
        let color = if self.active { t::OK } else { t::FG2 };
        if self.compact {
            for (i, h) in [5.0, 11.0, 17.0, 9.0, 13.0, 5.0].iter().enumerate() {
                let x = 2.0 + i as f32 * 4.0;
                frame.fill_rectangle(iced::Point::new(x, cy - h / 2.0), iced::Size::new(2.0, *h), color);
            }
        } else {
            for (x, y, r) in [(6.0, cy + 6.0, 1.8), (14.0, cy + 1.0, 2.3), (22.0, cy - 5.0, 1.8)] {
                frame.fill(&canvas::Path::circle(iced::Point::new(x, y), r), t::STAR);
            }
        }
        vec![frame.into_geometry()]
    }
}

pub fn view(app: &App) -> El<'_> {
    let mode = app.desktop_island.mode;
    let shortcut = crate::keys::display(&crate::keys::effective("g_mini"));
    let active = app.voice || app.mic_rec;
    let signal = |compact| canvas(Signal { active, compact }).width(28).height(28);
    let status = if active { "Ouvindo você" } else if app.streaming.is_some() { "Pensando…" } else { "Estou por aqui" };
    let body: El = match mode {
        Mode::Compact => button(
            row![signal(false), Space::new().width(Length::Fill), signal(true)].align_y(Vertical::Center)
        ).padding([4, 16]).width(Length::Fill).height(Length::Fill).on_press(action(Event::Wake))
            .style(|_, _| button::Style { background: Some(t::ISLAND_BG.into()), text_color: t::FG, border: Border { radius: 30.0.into(), ..Default::default() }, ..Default::default() }).into(),
        Mode::Desktop => button(
            row![signal(false), column![txf("Orion", 16.0, t::FG, t::SERIF), tx(crate::i18n::text(status), 11.0, t::FG3)].spacing(0), grow(),
                container(tx(shortcut.clone(), 10.0, t::FG2)).padding([4, 8]).style(surface(Some(t::RAISE), Some(t::LINE2), 7.0)), signal(true)]
                .spacing(10).align_y(Vertical::Center)
        ).padding([9, 18]).width(Length::Fill).height(Length::Fill).on_press(action(Event::Wake))
            .style(|_, _| button::Style { background: Some(t::ISLAND_BG.into()), text_color: t::FG, border: Border { radius: 30.0.into(), ..Default::default() }, ..Default::default() }).into(),
        Mode::Expanded => {
            let header = row![signal(false), txf("Orion", 18.0, t::FG, t::SERIF),
                tx(crate::i18n::text(status), 11.0, if active { t::OK } else { t::FG3 }), grow(),
                tip(ib("out", Some(Msg::Mini(false)), false), crate::i18n::text("Abrir o Orion")),
                tip(ib("minus", Some(action(Event::Collapse)), false), crate::i18n::text("Recolher ilha"))]
                .spacing(8).align_y(Vertical::Center);
            let editor = text_editor(&app.composer).placeholder(crate::i18n::text("O que vamos fazer?"))
                .id("desktop-island-composer")
                .font(t::UI).size(15).padding(12).height(66)
                .on_action(Msg::Edit)
                .key_binding(|press| {
                    use iced::keyboard::{Key, key::Named};
                    if press.key == Key::Named(Named::Enter) && !press.modifiers.shift() {
                        Some(text_editor::Binding::Custom(Msg::Send))
                    } else { text_editor::Binding::from_key_press(press) }
                })
                .style(|_, _| text_editor::Style { background: t::RAISE.into(), border: Border { color: t::LINE2, width: 1.0, radius: 16.0.into() }, placeholder: t::FG3, value: t::FG, selection: t::ACCENT_SOFT });
            let answer = if !app.logged { None } else {
                app.conv.and_then(|i| app.convs.get(i)).and_then(|c| c.msgs.iter().rev().find_map(|m| if let ChatMsg::Orion(o) = m { Some(o) } else { None }))
            };
            let preview: El = if let Some((message, _)) = &app.toast {
                container(tx(crate::i18n::status(message), 12.0, t::SIGNAL)).height(40).into()
            } else { match answer {
                Some(answer) => scrollable(tx(ell(&answer.body.replace(['\n', '#', '*'], " "), 750.0, 12.0), 12.0, t::FG2)).height(40).style(thin_scroll).into(),
                None => container(tx(crate::i18n::text("Uma pergunta, uma ideia ou o próximo passo."), 12.0, t::FG3)).height(40).center_y(40).into(),
            } };
            let controls = row![
                tip(ib("mic", Some(Msg::Mic), app.mic_rec), crate::i18n::text("Falar (clique para gravar)")),
                tip(ib("wave", Some(Msg::Live), app.voice), crate::i18n::text("Voz ao vivo")),
                tx(crate::i18n::text(if active { "Microfone em uso" } else { "Clique no microfone para falar" }), 11.0, if active { t::OK } else { t::FG3 }),
                grow(),
                if app.streaming.is_some() { button(crate::icons::stop_square(16.0, t::FG)).padding(7).on_press(Msg::Stop).into() } else { ib("up", (!app.draft().trim().is_empty()).then_some(Msg::Send), false) },
            ].spacing(4).align_y(Vertical::Center);
            let context: El = if app.attach.is_empty() {
                tx(shortcut.clone(), 10.0, t::FG3).into()
            } else {
                let names = app.attach.iter().map(|file| file.0.as_str()).collect::<Vec<_>>().join(", ");
                button(row![crate::icons::icon("file", 12.0, t::FG2), tx(ell(&format!("{} · {names}", app.attach.len()), 260.0, 10.0), 10.0, t::FG2)].spacing(5))
                    .padding(0).on_press(Msg::Mini(false)).into()
            };
            let footer = row![context, grow(), tx(crate::i18n::text("Esc para recolher"), 10.0, t::FG3)];
            container(column![header, editor, preview, controls, footer].spacing(8)).padding([14, 20]).width(Length::Fill).height(Length::Fill).into()
        }
    };
    container(body).width(Length::Fill).height(Length::Fill)
        .style(move |_| container::Style { background: Some(t::ISLAND_BG.into()), text_color: Some(t::FG), border: Border { color: Color::from_rgba(1.0, 1.0, 1.0, 0.10), width: 1.0, radius: if mode == Mode::Expanded { 30.0.into() } else { 32.0.into() } }, ..Default::default() }).into()
}

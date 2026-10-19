//! Campo de mensagem (§12): caixa, rodapé, metadados, sugestões `/` e `@`, comandos.

use crate::app::{App, Msg, Pop, View, EFFORTS};
use crate::data::{self, PROJ};
use crate::icons::{icon, stop_square};
use crate::popover::{Popover, Side};
use crate::theme as t;
use crate::ui::*;
use crate::views::menus;
use iced::alignment::Vertical;
use iced::widget::{button, column, container, row, text, text_editor};
use iced::{Background, Border, Length, Task};

/// (texto a inserir, rótulo, descrição à direita)
pub fn suggestions(app: &App) -> Vec<(String, String, String)> {
    let d = app.draft();
    if d.starts_with('/') && !d.contains(char::is_whitespace) {
        return data::CMDS.iter().filter(|(c, _)| c.starts_with(d.as_str())).map(|(c, desc)| (format!("{c} "), c.to_string(), desc.to_string())).collect();
    }
    if let Some(tok) = d.split(char::is_whitespace).next_back() {
        if let Some(q) = tok.strip_prefix('@') {
            if q.chars().count() <= 40 && (d.len() == tok.len() || d[..d.len() - tok.len()].ends_with(char::is_whitespace)) {
                let q = q.to_lowercase();
                return app
                    .mem
                    .nodes
                    .iter()
                    .filter(|n| n.label.to_lowercase().contains(&q))
                    .take(8)
                    .map(|n| (format!("@[[{}]] ", n.label), n.label.clone(), n.folder.split('/').next().unwrap_or("").to_string()))
                    .collect();
            }
        }
    }
    vec![]
}

fn est_lines(app: &App, width: f32) -> usize {
    let per = ((width - 38.0) / 7.4).max(10.0) as usize;
    let mut n = 0;
    for l in app.composer.text().split('\n') {
        n += (l.chars().count().max(1) + per - 1) / per;
    }
    n.max(1)
}

pub fn view(app: &App, home: bool) -> El<'_> {
    let width = if home { 660.0 } else { (app.p.thread_w() - 32.0).min(app.win.width - 64.0) };
    let busy = app.streaming.is_some() && !home;
    let h = (20.0 + est_lines(app, width) as f32 * 23.25).clamp(54.0, 220.0);
    let sugs = suggestions(app);
    let sug_open = !sugs.is_empty();
    let placeholder = if home { "Pergunte ou peça algo ao Orion. / para comandos, @ para citar uma nota" } else { "Responder ao Orion" };
    let editor = text_editor(&app.composer)
        .placeholder(placeholder)
        .on_action(Msg::Edit)
        .font(t::UI)
        .size(15)
        .line_height(text::LineHeight::Relative(1.55))
        .padding(pad(16.0, 18.0, 4.0, 18.0))
        .height(Length::Fixed(h))
        .key_binding(move |kp| {
            use iced::keyboard::{key::Named, Key};
            use text_editor::Binding;
            match kp.key.as_ref() {
                Key::Named(Named::Enter) if !kp.modifiers.shift() => Some(if sug_open { Binding::Custom(Msg::SugPick) } else { Binding::Custom(Msg::Send) }),
                Key::Named(Named::Tab) if sug_open => Some(Binding::Custom(Msg::SugPick)),
                Key::Named(Named::ArrowUp) if sug_open => Some(Binding::Custom(Msg::PalMove(-1))),
                Key::Named(Named::ArrowDown) if sug_open => Some(Binding::Custom(Msg::PalMove(1))),
                Key::Named(Named::ArrowUp) if kp.status == text_editor::Status::Focused { is_hovered: false } || true => {
                    // ↑ com o campo vazio repete a última mensagem
                    Some(Binding::Custom(Msg::RepeatLastIfEmpty))
                }
                _ => Binding::from_key_press(kp),
            }
        })
        .style(|_, _| text_editor::Style { background: Background::Color(iced::Color::TRANSPARENT), border: Border::default(), placeholder: t::FG3, value: t::FG, selection: t::ACCENT_SOFT });

    let mut foot = row![ib("plus", Some(Msg::Attach), false)].spacing(2).align_y(Vertical::Center);
    for (i, a) in app.attach.iter().enumerate() {
        foot = foot.push(
            container(row![icon("file", 14.0, t::FG2), tx(a.0.clone(), 12.0, t::FG2), tx(a.1.clone(), 12.0, t::FG3), button(icon("close", 14.0, t::FG3)).padding(0).on_press(Msg::Unattach(i)).style(|_, _| button::Style::default())].spacing(6).align_y(Vertical::Center))
                .height(Length::Fixed(26.0))
                .center_y(Length::Fixed(26.0))
                .padding(pad(0.0, 6.0, 0.0, 8.0))
                .style(surface(Some(t::RAISE2), None, 7.0)),
        );
    }
    foot = foot.push(grow());
    foot = foot.push(ib("mic", Some(Msg::Mic), false));
    foot = foot.push(ib("wave", Some(Msg::Live), app.voice));
    let has_text = !app.draft().trim().is_empty();
    let send: El = if busy {
        send_btn(stop_square(14.0, t::PRIMARY_INK).into(), Some(Msg::Stop), true)
    } else {
        send_btn(icon("up", 16.0, if has_text { t::PRIMARY_INK } else { t::FG4 }).into(), if has_text { Some(Msg::Send) } else { None }, has_text)
    };
    foot = foot.push(container(send).padding(pad(0.0, 0.0, 0.0, 4.0)));

    let boxed = container(column![editor, container(foot).padding(pad(6.0, 8.0, 8.0, 8.0))])
        .width(Length::Fill)
        .style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: 18.0.into() }, shadow: iced::Shadow { color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.18), offset: iced::Vector::new(0.0, 10.0), blur_radius: 30.0 }, ..Default::default() });

    // sugestões
    let sug_el: El = if sug_open {
        let mut c = column![].spacing(0);
        for (i, (ins, label, desc)) in sugs.iter().enumerate() {
            let mono = ins.starts_with('/');
            let hl = i == app.sug_sel.min(sugs.len() - 1);
            let l: El = if mono { txf(label.clone(), 12.5, t::FG, t::MONO_MED).into() } else { row![icon("file", 14.0, t::FG2), tx(label.clone(), 13.0, t::FG)].spacing(10).into() };
            c = c.push(
                button(row![l, grow(), tx(ell(desc, width * 0.5, 12.0), 12.0, t::FG3)].spacing(10).align_y(Vertical::Center))
                    .width(Length::Fill)
                    .padding(pad(7.0, 10.0, 7.0, 10.0))
                    .on_press(Msg::Sugg(ins.clone()))
                    .style(move |_, st| button::Style { background: if hl || matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: 8.0.into(), ..Default::default() }, shadow: Default::default(), snap: true }),
            );
        }
        container(c).width(Length::Fixed(width - 16.0)).padding(4).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: 12.0.into() }, shadow: shadow(), ..Default::default() }).into()
    } else {
        sp_w(0.0)
    };
    let boxed: El = Popover::new(boxed, sug_el, sug_open).above(!home).offset_x(8.0).on_dismiss(Msg::ClosePop).into();

    // metadados
    let mut meta = row![].spacing(2).align_y(Vertical::Center).padding(pad(0.0, 6.0, 0.0, 6.0));
    if home {
        let chip_proj = chip(PROJ.get(app.p.proj).map(|p| p.n).unwrap_or("Sem projeto").to_string(), None, Some("folder"), true, Msg::Pop(Pop::Project));
        let pr: El = Popover::new(chip_proj, menus::project_menu(app), app.pop == Some(Pop::Project)).on_dismiss(Msg::ClosePop).into();
        meta = meta.push(pr);
    }
    meta = meta.push(grow());
    let soft = if app.p.has_effort() { Some(EFFORTS[app.p.effort].to_string()) } else { None };
    let chip_model = chip(app.p.model_label(), soft, None, true, Msg::Pop(Pop::Model));
    let mm: El = Popover::new(chip_model, menus::model_menu(app), app.pop == Some(Pop::Model)).side(Side::Right).on_dismiss(Msg::ClosePop).into();
    meta = meta.push(mm);
    column![boxed, meta].spacing(4).width(Length::Fixed(width)).into()
}

fn send_btn<'a>(inner: El<'a>, msg: Option<Msg>, active: bool) -> El<'a> {
    let mut b = button(container(inner).center_x(Length::Fill).center_y(Length::Fill)).width(Length::Fixed(32.0)).height(Length::Fixed(32.0)).padding(0);
    if let Some(m) = msg {
        b = b.on_press(m);
    }
    b.style(move |_, _| button::Style { background: Some(Background::Color(if active { t::PRIMARY } else { t::RAISE2 })), text_color: t::PRIMARY_INK, border: Border { radius: 16.0.into(), ..Default::default() }, shadow: Default::default(), snap: true }).into()
}

pub fn run_command(app: &mut App, word: &str, full: &str) -> Task<Msg> {
    let rest = full[word.len()..].trim().to_string();
    let known = data::CMDS.iter().any(|(c, _)| &c[1..] == word);
    if !known {
        app.toast(format!("Comando /{word} não existe. /ajuda lista todos."));
        return Task::none();
    }
    match word {
        "aprovar" => app.view = View::Aprovacoes,
        "rotina" => app.view = View::Rotinas,
        "autonomo" => return app.update(Msg::Toggle("auto")),
        "voz" => app.voice = !app.voice,
        "limpar" => {
            app.view = View::Home;
            app.conv = None;
        }
        "modelo" => app.pop = Some(Pop::Model),
        "nota" => {
            if app.conv.is_some() {
                app.toast("Nota gravada e commit na main");
            } else {
                app.toast("Abra uma conversa primeiro");
            }
        }
        "vault" => {
            app.mem.q = rest.clone();
            app.mem_q = rest;
            app.view = View::Memoria;
        }
        "nuvem" => {
            app.cloud_extra.push(("Claude".into(), rest));
            app.view = View::Nuvem;
            app.toast("Tarefa enviada para a nuvem");
        }
        "imagem" => app.toast("Gerando imagem com Gemini (simulado). Fica na conversa e em 03 Recursos/Orion/Imagens."),
        "mcp" | "skill" => return app.update(Msg::OpenSettings(Some(crate::app::Sec::Conectores))),
        "resumo" => app.toast("Resumo gerado na nota desta conversa"),
        "ajuda" => app.toast("Comandos: /nota /vault /rotina /modelo /projeto /autonomo /nuvem /imagem /limpar /resumo /aprovar /voz /mcp /skill"),
        "projeto" => app.pop = Some(Pop::Project),
        _ => {}
    }
    Task::none()
}

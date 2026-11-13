//! Telas de página (§15): Projetos, Aprovações, Rotinas, Nuvem, Conhecimento, Arquivadas.

use crate::app::{App, Msg, View};
use crate::chart::Line;
use crate::data;
use crate::icons::icon;
use crate::theme as t;
use crate::ui::*;
use crate::views::login::field_style;
use crate::sky::spinner;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, canvas, column, container, row, scrollable, stack, text_input};
use iced::{Border, Length};

pub fn view(app: &App, v: View) -> El<'_> {
    match v {
        View::Projetos => projetos(app),
        View::Aprovacoes => aprovacoes(app),
        View::Rotinas => rotinas(app),
        View::Nuvem => nuvem(app),
        View::Conhecimento => conhecimento(app),
        View::Arquivadas => arquivadas(app),
        _ => sp_w(0.0),
    }
}

/// `.page`: largura 800, título Newsreader 32 + descrição, botão à direita.
pub fn page<'a>(title: &str, desc: &str, action: Option<El<'a>>, body: El<'a>) -> El<'a> {
    let mut head = row![column![txf(title.to_string(), t::TEXT_H1, t::FG, t::SERIF).line_height(text::LineHeight::Relative(1.1)), tx(desc.to_string(), t::TEXT_BASE, t::FG3)].spacing(t::SP_1).width(Length::Fill)].spacing(t::SP_3).align_y(Vertical::Bottom);
    if let Some(a) = action {
        head = head.push(a);
    }
    let col = column![container(head).padding(pad(0.0, 0.0, t::SP_6, 0.0)), body].width(Length::Fill);
    let inner = container(col).max_width(800.0).width(Length::Fill).padding(pad(t::SP_9, t::SP_5, t::SP_18, t::SP_5));
    scrollable(container(inner).width(Length::Fill).center_x(Length::Fill)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).width(Length::Fill).height(Length::Fill).into()
}

pub fn li<'a>(left: El<'a>, right: El<'a>) -> El<'a> {
    column![row![container(left).width(Length::Fill), right].spacing(t::SP_4).align_y(Vertical::Center).padding(pad(t::SP_3, t::SP_1, t::SP_3, t::SP_1)), hline()].into()
}
fn li_title<'a>(parts: Vec<El<'a>>) -> El<'a> {
    row(parts).spacing(t::SP_2).align_y(Vertical::Center).into()
}
pub(crate) fn li_desc<'a>(s: impl text::IntoFragment<'a>) -> El<'a> {
    container(tx(s, 12.5, t::FG3)).padding(pad(t::SP_PX, 0.0, 0.0, 0.0)).into()
}
use iced::widget::text;
fn li_right<'a>(parts: Vec<El<'a>>) -> El<'a> {
    row(parts).spacing(t::SP_2).align_y(Vertical::Center).into()
}
pub fn list<'a>(rows: Vec<El<'a>>) -> El<'a> {
    column![hline(), column(rows)].into()
}
fn h3<'a>(s: &str, first: bool) -> El<'a> {
    container(txf(crate::i18n::status(s), t::TEXT_EYEBROW, t::FG3, t::UI_MED)).padding(pad(if first { 0.0 } else { 26.0 }, t::SP_1, t::SP_2, t::SP_1)).into()
}
fn search_box<'a>(placeholder: &str, value: &str, on: impl Fn(String) -> Msg + 'a) -> El<'a> {
    let input = text_input(placeholder, value).on_input(on).size(13).font(t::UI).padding(pad(t::SP_2, t::SP_3, t::SP_2, t::SP_8)).style(field_style(t::RAISE, t::FIELD_LINE, t::ACCENT, 9.0));
    stack![input, container(icon("search", 14.0, t::FG3)).padding(pad(t::SP_25, 0.0, 0.0, t::SP_3))].into()
}

fn projetos(app: &App) -> El<'_> {
    let q = app.proj_q.trim().to_lowercase();
    let items: Vec<(usize, &data::Project)> = app.projects.iter().enumerate().filter(|(_, p)| q.is_empty() || format!("{}{}", p.n, p.d).to_lowercase().contains(&q)).collect();
    let card = |(i, p): (usize, &data::Project)| -> El<'static> {
        let mut tags = row![].spacing(t::SP_1);
        if !p.repo.is_empty() {
            tags = tags.push(badge_ic(p.repo.split('/').nth(1).unwrap_or(""), if p.ci { Tone::Ok } else { Tone::Plain }, "branch"));
        }
        tags = tags.push(badge_ic(&p.model, Tone::Plain, "cpu"));
        if p.vault {
            tags = tags.push(badge_ic(crate::i18n::text("Nota no vault"), Tone::Ok, "file"));
        }
        if p.rot > 0 {
            tags = tags.push(badge_ic(&crate::i18n::formatted("{} rotina{}", format!("{} rotina{}", p.rot, if p.rot > 1 { "s" } else { "" })), Tone::Plain, "clock"));
        }
        if !p.pastas.is_empty() {
            tags = tags.push(badge_ic(&ell(&p.pastas, 150.0, 11.0), Tone::Plain, "folder"));
        }
        let body = column![txf(p.n.clone(), 20.0, t::FG, t::SERIF), tx(p.d.clone(), 13.0, t::FG2), container(tags.wrap().vertical_spacing(4)).padding(pad(t::SP_05, 0.0, 0.0, 0.0)), container(sp_h(0.0)).height(Length::Fill), row![tx(p.pend.clone(), 12.0, t::FG3), grow(), tx(crate::i18n::formatted("Atualizado {}", format!("Atualizado {}", crate::i18n::status(&p.up))), 12.0, t::FG3)]].spacing(t::SP_15);
        button(container(body).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(164.0))
            .padding(pad(t::SP_4, t::SP_45, t::SP_4, t::SP_45))
            .on_press(Msg::OpenProject(i))
            .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(t::RAISE.into()) } else { None }, text_color: t::FG, border: Border { color: if matches!(st, button::Status::Hovered) { t::LINE3 } else { t::LINE2 }, width: 1.0, radius: t::R_PANEL.into() }, shadow: Default::default(), snap: true })
            .into()
    };
    let mut grid = column![].spacing(t::SP_3);
    let cards: Vec<El> = items.into_iter().map(card).collect();
    if cards.is_empty() {
        grid = grid.push(tx(crate::i18n::text("Nenhum projeto encontrado"), 14.0, t::FG3));
    }
    let mut it = cards.into_iter();
    while let Some(a) = it.next() {
        let b = it.next().unwrap_or_else(|| sp_w(0.0));
        grid = grid.push(row![container(a).width(Length::FillPortion(1)), container(b).width(Length::FillPortion(1))].spacing(t::SP_3));
    }
    let mut body = column![].spacing(t::SP_3);
    if let Some(form) = &app.project_form {
        let mut name = text_input(crate::i18n::text("Nome do projeto"), &form.name).id("project-name").padding(t::SP_25).size(14);
        let mut instructions = text_input(crate::i18n::text("Instruções (opcional)"), &form.instructions).padding(t::SP_25).size(14);
        let mut root = text_input(crate::i18n::text("Pasta local existente (opcional)"), &form.root).padding(t::SP_25).size(14);
        let mut git_url = text_input(crate::i18n::text("Ou URL de um repositório público do GitHub (opcional)"), &form.git_url).padding(t::SP_25).size(14);
        if !form.busy {
            name = name.on_input(Msg::ProjectName);
            instructions = instructions.on_input(Msg::ProjectInstructions);
            root = root.on_input(Msg::ProjectRoot).on_submit(Msg::ProjectSave);
            git_url = git_url.on_input(Msg::ProjectGitUrl).on_submit(Msg::ProjectSave);
        }
        let mut fields = column![
            tx(crate::i18n::text("Novo projeto"), 16.0, t::FG), name, instructions, root, git_url,
            tx(crate::i18n::text("Cria o projeto com memória isolada e uma nota no vault. Use uma pasta existente ou a URL de um repositório público do GitHub, que será clonado."), 12.0, t::FG3),
            row![btn(if form.busy { crate::i18n::text("Criando…") } else { crate::i18n::text("Criar projeto") }, Kind::Primary, true, None, (!form.busy).then_some(Msg::ProjectSave)), btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, (!form.busy).then_some(Msg::ProjectClose))].spacing(t::SP_2),
        ].spacing(t::SP_25);
        if !form.error.is_empty() { fields = fields.push(tx(crate::i18n::status(&form.error), 12.0, t::DANGER)); }
        body = body.push(container(fields).padding(t::SP_4).width(Length::Fill).style(surface(Some(t::RAISE), Some(t::LINE2), 12.0)));
    }
    body = body.push(search_box("Buscar projetos", &app.proj_q, Msg::ProjQ)).push(grid);
    page(crate::i18n::text("Projetos"), crate::i18n::text("Conversas, instruções e pasta local de cada projeto"), Some(btn(crate::i18n::text("Novo projeto"), Kind::Primary, false, None, app.project_form.is_none().then_some(Msg::ProjectStart))), body.into())
}

fn aprovacoes(app: &App) -> El<'_> {
    let pend = app.pend_list();
    let mut col = column![h3(&format!("Aguardando você · {}", app.pend_count()), true)].spacing(t::SP_25);
    for (ci, mi) in pend {
        let c = &app.convs[ci];
        if let data::ChatMsg::Orion(o) = &c.msgs[mi] {
            let Some(a) = o.approval.as_ref() else { continue };
            let txt = crate::md::render(&crate::md::cached_parse(&a.text), crate::md::Style { font: t::UI, bold: t::UI_SEMI, size: 14.0, line: 1.5, h_font: t::UI_MED, h1: 16.0, h2: 15.0, h3: 14.0, gap: 4.0 });
            let mut acoes = approval_actions(a, Msg::Approve(ci, mi), Msg::Deny(ci, mi));
            acoes.push(btn(crate::i18n::text("Abrir conversa"), Kind::Ghost, true, None, Some(Msg::OpenConv(ci))));
            col = col.push(qcard(
                row![dot_halo(t::SIGNAL, t::SIGNAL_SOFT), tx(format!("{} · {}", c.title, c.proj), 12.5, t::FG3), grow(), tx(o.time.clone(), 12.5, t::FG3)].spacing(t::SP_2).align_y(Vertical::Center).into(),
                column![txt, approval_detail(a), crate::ui::approval_confirm(a, move |s| Msg::ApprType(ci, mi, s)).unwrap_or_else(|| column![].into())].spacing(t::SP_2).into(),
                iced::widget::Row::with_children(acoes).spacing(t::SP_2).into(),
            ));
        }
    }
    if app.rot_pend {
        let txt: El = row![tx(crate::i18n::text("Mover 38 arquivos de "), 14.0, t::FG), txf("Downloads", 12.0, t::FG, t::MONO), tx(crate::i18n::text(" para "), 14.0, t::FG), txf("Downloads\\2026-10", 12.0, t::FG, t::MONO), tx(crate::i18n::text(". Nada é apagado."), 14.0, t::FG)].align_y(Vertical::Center).into();
        col = col.push(qcard(
            row![dot_halo(t::SIGNAL, t::SIGNAL_SOFT), tx(crate::i18n::text("Rotina “Organizar Downloads” · proposta"), 12.5, t::FG3), grow(), tx(crate::i18n::text("sábado"), 12.5, t::FG3)].spacing(t::SP_2).align_y(Vertical::Center).into(),
            txt,
            row![btn(crate::i18n::text("Aprovar"), Kind::Normal, true, None, Some(Msg::ApproveRot)), btn(crate::i18n::text("Negar"), Kind::Normal, true, None, Some(Msg::DenyRot))].spacing(t::SP_2).into(),
        ));
    }
    col = col.push(container(row![icon("send2", 14.0, t::FG3), tx(crate::i18n::text("As mesmas pendências chegam no Telegram, com botões, só para o seu ID"), 12.0, t::FG3)].spacing(t::SP_15).align_y(Vertical::Center)).padding(pad(t::SP_1, t::SP_1, 0.0, t::SP_1)));
    col = col.push(h3(crate::i18n::text("Histórico de atividade"), false));
    let opts = [crate::i18n::text("Todas"), crate::i18n::text("Aguardando"), crate::i18n::text("Negadas ou com falha"), crate::i18n::text("Concluídas")];
    col = col.push(container(seg(&opts, app.act_filter, Msg::ActFilter)).padding(pad(0.0, 0.0, t::SP_3, 0.0)));
    let mut rows = vec![];
    for (i, a) in app.acts.iter().enumerate() {
        let keep = match app.act_filter {
            1 => a.kind == "warn",
            2 => a.kind == "err",
            3 => a.kind == "ok",
            _ => true,
        };
        if !keep {
            continue;
        }
        let mut title = vec![tx(a.text.clone(), 14.0, t::FG).into()];
        if a.undone && a.label == "desfeito" {
            title.push(badge(crate::i18n::text("desfeito"), Tone::Plain));
        }
        let tone = match a.kind.as_str() {
            "ok" => Tone::Ok,
            "warn" => Tone::Warn,
            _ => Tone::Err,
        };
        let mut right = vec![badge(&a.label, if a.label == "revert" { Tone::Plain } else { tone })];
        if a.undoable && !a.undone {
            right.push(btn(crate::i18n::text("Desfazer"), Kind::Ghost, true, Some("undo"), Some(Msg::Undo(i))));
        }
        rows.push(li(column![li_title(title), li_desc(a.when.clone())].into(), li_right(right)));
    }
    if rows.is_empty() {
        col = col.push(container(tx(crate::i18n::text("Nada neste filtro"), 14.0, t::FG3)).padding(pad(t::SP_4, t::SP_1, t::SP_4, t::SP_1)));
    } else {
        col = col.push(list(rows));
    }
    // ligar o modo autônomo não é convite desta página: fica em Configurações › Privacidade e autonomia
    // (e pede a senha). Aqui só o atalho para desligar, quando já está ligado.
    let auto = app.p.auto.then(|| btn(crate::i18n::text("Desligar modo autônomo"), Kind::Normal, true, Some("bolt"), Some(Msg::Toggle("auto"))));
    page(crate::i18n::text("Aprovações"), crate::i18n::text("Pendências de conversas e rotinas, e o histórico do que o Orion fez fora do chat"), auto, col.into())
}

/// Cartão de pendência em superfície neutra: o âmbar fica só no ponto do cabeçalho, para a fila inteira não acender.
fn qcard<'a>(head: El<'a>, body: El<'a>, actions: El<'a>) -> El<'a> {
    container(column![head, body, actions].spacing(t::SP_2)).width(Length::Fill).padding(pad(t::SP_3, t::SP_4, t::SP_3, t::SP_4)).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_PANEL.into() }, ..Default::default() }).into()
}

fn runs<'a>(r: &str) -> El<'a> {
    let mut row_ = row![].spacing(t::SP_1);
    for ch in r.chars() {
        let c = match ch {
            'f' => t::DANGER,
            's' => t::FG3,
            _ => t::OK,
        };
        row_ = row_.push(container(sp_w(0.0)).width(Length::Fixed(8.0)).height(Length::Fixed(8.0)).style(surface(Some(c), None, 2.0)));
    }
    row_.into()
}

fn rotinas(app: &App) -> El<'_> {
    let mut rows = vec![];
    for (i, r) in app.rots.iter().enumerate() {
        let mut title: Vec<El> = vec![tx(r.t.clone(), 14.0, t::FG).into()];
        if r.news {
            title.push(badge(crate::i18n::text("resultado novo"), Tone::Ok));
        }
        if r.crit {
            title.push(badge_ic(crate::i18n::text("nuvem se desligado"), Tone::Info, "cloud"));
        }
        let st_tone = match r.st.as_str() {
            "ok" | "concedida" => Tone::Ok,
            "falhou" => Tone::Err,
            _ => Tone::Plain,
        };
        let when = if !r.on { crate::i18n::text("pausada").to_string() } else if r.next.is_empty() { String::new() } else { crate::i18n::formatted("próxima {}", format!("próxima {}", r.next)) };
        let mut linha = r.d.clone();
        for p in [when, if r.last.is_empty() { String::new() } else { format!("última {}", r.last) }] {
            if !p.is_empty() {
                linha = if linha.is_empty() { p } else { format!("{linha} · {p}") };
            }
        }
        let d: El = row![tx(format!("{linha} "), 12.5, t::FG3), badge(&r.st, st_tone)].spacing(t::SP_1).align_y(Vertical::Center).into();
        let mut lc = column![li_title(title), container(d).padding(pad(t::SP_PX, 0.0, 0.0, 0.0))];
        if !r.out.is_empty() {
            lc = lc.push(li_desc(format!("Saída: {}", r.out)));
        }
        let left = lc.into();
        rows.push(li(left, li_right(vec![runs(&r.runs), btn(crate::i18n::text("Rodar agora"), Kind::Normal, true, Some("play"), Some(Msg::RotRun(i))), switch(r.on, Msg::RotToggle(i))])));
    }
    page(crate::i18n::text("Rotinas"), crate::i18n::text("Crie pelo chat (“todo dia às 8h me manda o briefing”), por /rotina ou aqui. As críticas rodam na nuvem do Claude se o PC estiver desligado."), Some(btn(crate::i18n::text("Nova rotina"), Kind::Primary, false, None, Some(if app.api.is_some(){Msg::Resources(crate::resources::Event::Open(crate::resources::FormKind::Routine))}else{Msg::Toast("Formulário: nome, horário, prompt, projeto, modelo, saída".into())}))), list(rows))
}

fn nuvem(app: &App) -> El<'_> {
    // linhas quietas: ponto ou spinner na coluna fixa, título, metadado em uma linha e ações à direita
    let mut rows: Vec<El> = vec![];
    let lead_spin = || -> El { container(spinner(app.spin, 10.0)).width(Length::Fixed(t::SP_4)).height(Length::Fixed(t::SP_5)).center_y(Length::Fixed(t::SP_5)).into() };
    for (i, c) in app.cloud.iter().enumerate() {
        let (lead, state, color, mut actions): (El, String, iced::Color, Vec<El>) = if c.running {
            (lead_spin(), crate::i18n::text("rodando").to_string(), t::ACCENT, vec![btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(Msg::CloudCancel(i)))])
        } else if c.proj == "na_fila" {
            // F04.21: até 3 tarefas rodam juntas; a fila espera a vez e pode ser cancelada
            (status_dot(t::FG4), crate::i18n::text("na fila").to_string(), t::FG3, vec![btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(Msg::CloudCancel(i)))])
        } else {
            let color = match c.proj.as_str() {
                "falhou" | "erro" => t::DANGER,
                _ => t::FG3,
            };
            (status_dot(if color == t::DANGER { t::DANGER } else { t::FG4 }), c.proj.clone(), color, vec![])
        };
        let meta = if c.link.is_empty() { format!("{} · {}", c.svc, c.at) } else { format!("{} · {} · {}", c.svc, c.at, c.link) };
        actions.insert(0, txf(state, 12.0, color, t::UI_MED).into());
        rows.push(task_row(lead, c.t.clone(), meta, actions));
    }
    for (svc, t_) in &app.cloud_extra {
        rows.push(task_row(lead_spin(), t_.clone(), format!("{svc} · agora"), vec![txf(crate::i18n::text("rodando"), 12.0, t::ACCENT, t::UI_MED).into()]));
    }
    for (i, n) in app.nuvem.iter().enumerate() {
        let failed = matches!(n.estado.as_str(), "error" | "failed" | "erro");
        let meta = if n.url.is_empty() { format!("{} · nuvem", n.svc) } else { format!("{} · {}", n.svc, n.url) };
        let actions = vec![
            txf(n.estado.clone(), 12.0, if failed { t::DANGER } else { t::FG3 }, t::UI_MED).into(),
            btn(crate::i18n::text("Trazer"), Kind::Normal, true, None, Some(Msg::NuvemTrazer(i))),
            btn(crate::i18n::text("Descartar"), Kind::Ghost, true, None, Some(Msg::NuvemDescartar(i))),
        ];
        rows.push(task_row(status_dot(if failed { t::DANGER } else { t::FG4 }), n.t.clone(), meta, actions));
    }
    let rows: El = if rows.is_empty() {
        container(column![tx(crate::i18n::text("Nenhuma tarefa ainda"), 14.0, t::FG2), li_desc("Peça com /tarefa: o Orion segue sozinho, só lê e propõe, e avisa quando terminar.")].spacing(t::SP_1)).padding(pad(t::SP_4, t::SP_4, t::SP_4, t::SP_4)).width(Length::Fill).style(surface(Some(t::RAISE), Some(t::LINE), t::R_CARD)).into()
    } else {
        let mut col = column![];
        let n = rows.len();
        for (k, r) in rows.into_iter().enumerate() {
            col = col.push(r);
            if k + 1 < n {
                col = col.push(container(hline()).padding(pad(0.0, t::SP_4, 0.0, t::SP_10)));
            }
        }
        container(col).width(Length::Fill).style(surface(Some(t::RAISE), Some(t::LINE), t::R_CARD)).into()
    };
    let tasks_head = container(txf(crate::i18n::text("Tarefas"), t::TEXT_BASE + 2.0, t::FG, t::UI_MED)).padding(pad(t::SP_2, t::SP_1, t::SP_2, t::SP_1));
    let body: El = if app.maestro.is_empty() { rows } else { column![maestro_section(app), tasks_head, rows].into() };
    page(crate::i18n::text("Segundo plano"), crate::i18n::text("Tarefas longas que o Orion faz sozinho: só lê e propõe, dentro do tempo e dos passos que você deu. Crie com /tarefa."), Some(btn(crate::i18n::text("Nova tarefa"), Kind::Normal, true, Some("cloud"), Some(Msg::CloudNew))), body)
}

/// Estado de missão ou etapa do Maestro: rótulo e cor do ponto. A cor só aparece no que pede atenção
/// (rodando, esperando você, falhou); o resto fica neutro.
fn maestro_status(state: &str) -> (&'static str, iced::Color) {
    match state {
        "planejando" => (crate::i18n::text("planejando"), t::ACCENT),
        "aguarda_aprovacao" => (crate::i18n::text("aguarda aprovação"), t::SIGNAL),
        "rodando" => (crate::i18n::text("rodando"), t::ACCENT),
        "integrando" => (crate::i18n::text("juntando"), t::ACCENT),
        "concluida" | "ok" => (crate::i18n::text("concluída"), t::FG3),
        "falhou" => (crate::i18n::text("falhou"), t::DANGER),
        "cancelada" => (crate::i18n::text("cancelada"), t::FG4),
        "interrompida" => (crate::i18n::text("interrompida"), t::SIGNAL),
        "pulada" => (crate::i18n::text("pulada"), t::FG4),
        _ => (crate::i18n::text("pendente"), t::FG4),
    }
}

fn effort_label(effort: &str) -> &'static str {
    match effort {
        "baixo" => crate::i18n::text("baixo"),
        "alto" => crate::i18n::text("alto"),
        _ => crate::i18n::text("médio"),
    }
}

/// Ponto de estado de 6 px numa caixa fixa, para alinhar a coluna.
fn status_dot<'a>(color: iced::Color) -> El<'a> {
    let dot = container(sp_w(0.0)).width(Length::Fixed(6.0)).height(Length::Fixed(6.0)).style(surface(Some(color), None, 3.0));
    container(dot).width(Length::Fixed(t::SP_4)).height(Length::Fixed(t::SP_5)).center_y(Length::Fixed(t::SP_5)).into()
}

/// Marca de etapa na linha do tempo: feita, rodando, falhou ou por fazer.
fn step_mark<'a>(app: &App, state: &str) -> El<'a> {
    let mark: El = match state {
        "ok" => icon("check", 12.0, t::FG2).into(),
        "rodando" => spinner(app.spin, 10.0),
        "falhou" => icon("close", 12.0, t::DANGER).into(),
        _ => container(sp_w(0.0)).width(Length::Fixed(6.0)).height(Length::Fixed(6.0)).style(surface(None, Some(t::FG4), 3.0)).into(),
    };
    container(mark).width(Length::Fixed(t::SP_4)).height(Length::Fixed(t::SP_5)).center_x(Length::Fixed(t::SP_4)).center_y(Length::Fixed(t::SP_5)).into()
}

/// Linha de tarefa: marca na coluna fixa, título, metadado e ações alinhadas ao centro.
fn task_row<'a>(lead: El<'a>, title: String, meta: String, actions: Vec<El<'a>>) -> El<'a> {
    let text = column![tx(title, t::TEXT_BASE, t::FG), txf(meta, 12.0, t::FG3, t::UI)].spacing(t::SP_05).width(Length::Fill);
    container(row![lead, text, row(actions).spacing(t::SP_2).align_y(Vertical::Center)].spacing(t::SP_2).align_y(Vertical::Center)).padding(pad(t::SP_3, t::SP_4, t::SP_3, t::SP_3)).width(Length::Fill).into()
}

/// Barra fina de progresso (etapas concluídas sobre o total).
fn progress_line<'a>(done: usize, total: usize, color: iced::Color) -> El<'a> {
    let track = |c: iced::Color| container(sp_w(0.0)).height(Length::Fixed(2.0)).style(surface(Some(c), None, 1.0));
    let (d, rest) = (done.min(total) as u16, total.saturating_sub(done) as u16);
    let mut r = row![].width(Length::Fill);
    if d > 0 {
        r = r.push(track(color).width(Length::FillPortion(d)));
    }
    if rest > 0 {
        r = r.push(track(t::LINE2).width(Length::FillPortion(rest)));
    }
    r.into()
}

/// Seção do Maestro: cada missão é um bloco com o pedido, o estado, o progresso e, aberta, a
/// linha do tempo das etapas (agente e esforço em mono) e o resultado em markdown.
fn maestro_section(app: &App) -> El<'_> {
    let head = row![
        column![
            txf("Maestro", t::TEXT_BASE + 2.0, t::FG, t::UI_MED),
            tx(crate::i18n::text("O Sonnet divide o pedido, distribui entre os agentes e junta o resultado. Plano com CLI espera sua aprovação."), 12.5, t::FG3),
        ]
        .spacing(t::SP_1)
        .width(Length::Fill),
    ]
    .align_y(Vertical::Top)
    .spacing(t::SP_3);
    let mut col = column![container(head).padding(pad(0.0, t::SP_1, t::SP_2, t::SP_1))].spacing(t::SP_2);
    for m in &app.maestro {
        col = col.push(mission_card(app, m));
    }
    container(col).padding(pad(0.0, 0.0, t::SP_4, 0.0)).into()
}

fn mission_card<'a>(app: &'a App, m: &'a serde_json::Value) -> El<'a> {
    let s = |k: &str| m[k].as_str().unwrap_or("").to_string();
    let (id, state) = (s("id"), s("estado"));
    let (label, color) = maestro_status(&state);
    let active = ["planejando", "rodando", "integrando"].contains(&state.as_str());
    let open = app.maestro_aberta.as_deref() == Some(id.as_str());
    let steps = m["subtarefas"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let done = steps.iter().filter(|x| x["estado"] == "ok").count();

    let mut actions = row![].spacing(t::SP_1).align_y(Vertical::Center);
    if ["aguarda_aprovacao", "interrompida"].contains(&state.as_str()) {
        actions = actions.push(btn(crate::i18n::text("Aprovar"), Kind::Primary, true, None, Some(Msg::MaestroAprovar(id.clone()))));
    }
    if active || state == "aguarda_aprovacao" {
        actions = actions.push(btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(Msg::MaestroCancelar(id.clone()))));
    } else {
        actions = actions.push(tip(ib("trash", Some(Msg::MaestroApagar(id.clone())), false), crate::i18n::text("Apagar")));
    }
    actions = actions.push(tip(ib(if open { "chevD" } else { "chevR" }, Some(Msg::MaestroAbrir(id.clone())), false), crate::i18n::text(if open { "Fechar" } else { "Ver" })));

    // estado · etapas · resumo, numa linha só de metadado
    let mut meta = row![txf(label, 12.0, if color == t::FG4 || color == t::FG3 { t::FG3 } else { color }, t::UI_MED)].spacing(t::SP_2).align_y(Vertical::Center);
    if !steps.is_empty() {
        meta = meta.push(txf(format!("{done}/{}", steps.len()), 12.0, t::FG3, t::MONO));
    }
    let summary = s("resumo");
    if !summary.is_empty() {
        meta = meta.push(tx(summary, 12.0, t::FG3).wrapping(text::Wrapping::None));
    }

    let title = column![tx(s("pedido"), t::TEXT_BASE, t::FG).line_height(text::LineHeight::Relative(1.4)), meta].spacing(t::SP_1).width(Length::Fill);
    let top = row![if active { container(spinner(app.spin, 10.0)).width(Length::Fixed(t::SP_4)).height(Length::Fixed(t::SP_5)).center_y(Length::Fixed(t::SP_5)).into() } else { status_dot(color) }, title, actions].spacing(t::SP_2).align_y(Vertical::Top);
    let mut body = column![top].spacing(t::SP_3);
    if !steps.is_empty() && !["concluida", "cancelada"].contains(&state.as_str()) {
        body = body.push(container(progress_line(done, steps.len(), if state == "falhou" { t::DANGER } else { t::ACCENT })).padding(pad(0.0, 0.0, 0.0, t::SP_6)));
    }
    if open {
        let mut timeline = column![].spacing(t::SP_1);
        for x in steps {
            let xs = |k: &str| x[k].as_str().unwrap_or("").to_string();
            let step_state = xs("estado");
            let who = if xs("agente_usado").is_empty() { xs("agente") } else { xs("agente_usado") };
            let title_color = if step_state == "pendente" || step_state == "pulada" { t::FG3 } else { t::FG2 };
            timeline = timeline.push(row![step_mark(app, &step_state), tx(xs("titulo"), 13.0, title_color).width(Length::Fill), txf(format!("{who} · {}", effort_label(&xs("esforco"))), 11.5, t::FG3, t::MONO)].spacing(t::SP_2).align_y(Vertical::Center));
            if step_state == "falhou" {
                timeline = timeline.push(container(tx(xs("saida").chars().take(300).collect::<String>(), 12.0, t::FG3)).padding(pad(0.0, 0.0, t::SP_1, t::SP_6)));
            }
        }
        body = body.push(container(timeline).padding(pad(0.0, 0.0, 0.0, t::SP_6)));
        let error = s("erro");
        if !error.is_empty() {
            body = body.push(container(tx(error, 12.5, t::DANGER)).padding(pad(0.0, 0.0, 0.0, t::SP_6)));
        }
        let result = m["resultado"].as_str().unwrap_or("");
        if !result.is_empty() {
            let rendered = crate::md::render(&crate::md::cached_parse(result), crate::md::Style { font: t::UI, bold: t::UI_SEMI, size: 13.5, line: 1.5, h_font: t::UI_MED, h1: 16.0, h2: 15.0, h3: 14.0, gap: 4.0 });
            body = body.push(column![hline(), container(rendered).padding(pad(t::SP_3, 0.0, 0.0, t::SP_6))]);
        }
    }
    container(body).padding(pad(t::SP_4, t::SP_4, t::SP_4, t::SP_3)).width(Length::Fill).style(surface(Some(t::RAISE), Some(if state == "aguarda_aprovacao" { t::alpha(t::SIGNAL, 0.35) } else { t::LINE }), t::R_CARD)).into()
}

pub fn src_rows(app: &App) -> Vec<El<'_>> {
    data::SRC.iter().enumerate().map(|(i, (_, tt, d))| setting(tt, d, if app.api.is_some() && (app.resources.sources.is_none() || app.resources.source_busy){badge(crate::i18n::text("Carregando…"),Tone::Plain)}else{switch(app.p.src[i], Msg::TogA("src", i))})).collect()
}

/// Linhas de fatos com Editar e Esquecer; usadas em Conhecimento e em Configurações › Sobre você (F06.22).
pub fn fact_rows<'a>(app: &'a App, filtro: impl Fn(&crate::data::Fact) -> bool) -> Vec<El<'a>> {
    let mut rows = vec![];
    for (i, f) in app.facts.iter().enumerate().filter(|(_, f)| filtro(f)) {
        let editing = app.edit_fact.as_ref().filter(|e| e.0 == i);
        let left: El = match editing {
            Some(e) => column![text_input("", &e.1).on_input(Msg::FactText).on_submit(Msg::FactSave).size(14).font(t::UI).padding(pad(t::SP_1, t::SP_2, t::SP_1, t::SP_2)).style(field_style(t::RAISE, t::FIELD_LINE, t::ACCENT, 7.0)), li_desc(f.src.clone())].spacing(t::SP_1).into(),
            None => column![tx(f.t.clone(), 14.0, t::FG), li_desc(f.src.clone())].into(),
        };
        let right: El = if editing.is_some() { btn(crate::i18n::text("Salvar"), Kind::Primary, true, None, Some(Msg::FactSave)) } else { row![tip(ib("edit", Some(Msg::FactEdit(i)), false), crate::i18n::text("Editar")), tip(ib("trash", Some(Msg::FactForget(i)), false), crate::i18n::text("Esquecer"))].into() };
        rows.push(li(left, right));
    }
    rows
}

fn conhecimento(app: &App) -> El<'_> {
    let mut col = column![];
    let mut groups: Vec<&str> = vec![];
    for f in &app.facts {
        if !groups.contains(&f.g.as_str()) {
            groups.push(&f.g);
        }
    }
    if app.facts.is_empty() {
        col = col.push(tx(crate::i18n::text("Memória vazia"), 14.0, t::FG3));
    }
    for g in groups {
        col = col.push(h3(g, g == app.facts[0].g));
        col = col.push(list(fact_rows(app, |f| f.g == g)));
    }
    page(crate::i18n::text("Conhecimento"), crate::i18n::text("Fatos que o Orion aprendeu, cada um com a fonte. Corrigir ou esquecer muda a nota no vault."), Some(btn(crate::i18n::text("Ver no grafo"), Kind::Normal, true, Some("graph"), Some(Msg::Go(View::Memoria)))), col.into())
}

fn arquivadas(app: &App) -> El<'_> {
    let mut rows = vec![];
    for (i, a) in app.archived.iter().enumerate() {
        rows.push(li(column![tx(a.0.clone(), 14.0, t::FG), li_desc(format!("{} · {}", a.1, a.2))].into(), li_right(vec![btn(crate::i18n::text("Desarquivar"), Kind::Normal, true, None, Some(Msg::Unarchive(i))), ib("trash", Some(Msg::DeleteArchived(i)), false)])));
    }
    let body: El = if rows.is_empty() { container(tx(crate::i18n::text("Nenhuma conversa arquivada"), 14.0, t::FG3)).padding(pad(t::SP_4, t::SP_1, t::SP_4, t::SP_1)).into() } else { list(rows) };
    page(crate::i18n::text("Conversas arquivadas"), crate::i18n::text("Só arquivam quando você pede. Desarquive para continuar ou apague de vez."), None, body)
}

pub fn hbar<'a>(label: &str, pct: f32, value: &str, color: iced::Color) -> El<'a> {
    hbar_sz(label, pct, value, color, 13.0)
}
pub fn hbar_sz<'a>(label: &str, pct: f32, value: &str, color: iced::Color, vsize: f32) -> El<'a> {
    row![container(tx(label.to_string(), 13.0, t::FG2)).width(Length::Fixed(140.0)), container(meter(pct, 5.0, color)).width(Length::Fill), container(tx(value.to_string(), vsize, t::FG3)).width(Length::Fixed(124.0)).align_x(Horizontal::Right)].spacing(t::SP_3).align_y(Vertical::Center).padding(pad(t::SP_15, t::SP_1, t::SP_15, t::SP_1)).into()
}

pub fn stats<'a>(items: [(&str, &str); 4]) -> El<'a> {
    let mut r = row![].width(Length::Fill);
    for (i, (n, l)) in items.iter().enumerate() {
        let cell = container(column![txf(n.to_string(), 30.0, t::FG, t::SERIF).line_height(text::LineHeight::Relative(1.1)), tx(l.to_string(), 12.0, t::FG3)]).width(Length::FillPortion(1)).padding(pad(t::SP_4, t::SP_1, t::SP_4, if i == 0 { 4.0 } else { 16.0 }));
        if i > 0 {
            r = r.push(vline(80.0));
        }
        r = r.push(cell);
    }
    column![hline(), r, hline()].into()
}

pub fn painel(app: &App) -> El<'_> {
    if app.api.is_some() {
        return painel_live(app);
    }
    // Sem servidor não há números reais: o painel fica vazio em vez de mostrar dados de exemplo (F01.28).
    let aviso = crate::i18n::text("Sem dados ainda. Conecte o Orion para ver cotas, modelos e a semana.");
    let col = column![container(tx(aviso, t::TEXT_BASE, t::FG3)).padding(pad(t::SP_1, t::SP_1, t::SP_1, t::SP_1))];
    painel_section(app, col.into())
}

/// Painel com os números do servidor (`/painel` e `/budget`).
fn painel_live(app: &App) -> El<'_> {
    use serde_json::Value;
    let mut col = column![h3(crate::i18n::text("Cotas das chamadas · semana"), true)];
    let quotas: Vec<Value> = app.budget.as_ref().and_then(|b| b.get("quotas")).and_then(|q| q.as_array()).cloned().unwrap_or_default();
    if quotas.is_empty() {
        col = col.push(container(tx(crate::i18n::text("Nenhuma cota configurada. Defina em Configurações › Modelos."), 13.0, t::FG3)).padding(pad(t::SP_1, t::SP_1, t::SP_1, t::SP_1)));
    }
    for q in &quotas {
        let pct = q.get("percent").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        let (used, cap) = (q.get("used").and_then(|x| x.as_i64()).unwrap_or(0), q.get("weekly_calls").and_then(|x| x.as_i64()).unwrap_or(0));
        col = col.push(hbar(q.get("provider").and_then(|x| x.as_str()).unwrap_or("?"), pct, &if pct >= 100.0 { crate::i18n::text("esgotada").to_string() } else { format!("{used}/{cap}") }, if pct >= 100.0 { t::DANGER } else { t::FG2 }));
    }
    let p = app.painel.clone().unwrap_or(Value::Null);
    col = col.push(h3(crate::i18n::text("Modelos"), false));
    let eps = p["modelos"]["endpoints"].as_array().cloned().unwrap_or_default();
    if eps.is_empty() {
        col = col.push(container(tx(if p["modelos"]["configurado"].as_bool().unwrap_or(false) { crate::i18n::text("Modelos configurados") } else { crate::i18n::text("Nenhum modelo configurado") }, 13.0, t::FG3)).padding(pad(t::SP_1, t::SP_1, t::SP_1, t::SP_1)));
    }
    for e in &eps {
        let nome = e["nome"].as_str().or(e["name"].as_str()).unwrap_or("modelo");
        col = col.push(li(tx(nome.to_string(), 14.0, t::FG).into(), li_right(vec![badge(e[crate::i18n::text("estado")].as_str().unwrap_or(crate::i18n::text("ativo")), Tone::Ok)])));
    }
    // F10.17: versão de cada CLI de assinatura (lida pelo servidor com --version)
    let clis = p["clis"].as_array().cloned().unwrap_or_default();
    if !clis.is_empty() {
        col = col.push(h3(crate::i18n::text("CLIs de assinatura"), false));
        for c in &clis {
            let nome = c["nome"].as_str().unwrap_or("cli").to_string();
            let estado = if !c["instalada"].as_bool().unwrap_or(false) {
                badge(crate::i18n::text("não instalada"), Tone::Plain)
            } else {
                match c["versao"].as_str() {
                    Some(v) => badge(&format!("v{v}"), Tone::Ok),
                    None => badge(crate::i18n::text("versão desconhecida"), Tone::Warn),
                }
            };
            col = col.push(li(tx(nome, 14.0, t::FG).into(), li_right(vec![estado])));
        }
    }
    let semana: Vec<f32> = p["semana"].as_array().map(|a| a.iter().map(|d| d["total"].as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
    let (total, erros): (f32, f32) = (semana.iter().sum(), p["semana"].as_array().map(|a| a.iter().map(|d| d["erros"].as_f64().unwrap_or(0.0) as f32).sum()).unwrap_or(0.0));
    col = col.push(h3("A semana", false));
    let up = p["uptime_s"].as_f64().unwrap_or(0.0);
    let upt = if up >= 3600.0 { format!("{} h", (up / 3600.0) as i64) } else { format!("{} min", (up / 60.0) as i64) };
    col = col.push(container(stats([(&format!("{total:.0}"), "chamadas de modelo"), (&format!("{erros:.0}"), "com erro"), (&app.pend_count().to_string(), "aguardando você"), (&upt, "servidor no ar")])).padding(pad(0.0, 0.0, t::SP_7, 0.0)));
    if semana.len() >= 2 {
        col = col.push(h3(crate::i18n::text("Chamadas por dia · 7 dias"), false));
        let mx = semana.iter().cloned().fold(1.0f32, f32::max);
        let ch = Line { vals: semana, min: 0.0, max: mx.max(1.0), ticks: vec![0.0, mx / 2.0, mx], fmt: |v| format!("{v:.0}"), start: crate::i18n::text("7 dias atrás"), end: crate::i18n::text("hoje"), color: t::FG2, mark: None };
        col = col.push(canvas(ch).width(Length::Fill).height(Length::Fixed(150.0)));
    }
    // F09.04: tokens por dia, 7 e 30 dias. O registro de saídas guarda bytes, então o número é estimado (bytes ÷ 4).
    let tokens: Vec<f32> = p["consumo"]["dias"].as_array().map(|a| a.iter().map(|d| d["tokens"].as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
    if tokens.len() >= 7 {
        let compacto: fn(f32) -> String = |v| if v >= 1_000_000.0 { format!("{:.1}M", v / 1_000_000.0) } else if v >= 1000.0 { format!("{:.0}k", v / 1000.0) } else { format!("{v:.0}") };
        let grafico = |vals: Vec<f32>, inicio: &'static str| {
            let mx = vals.iter().cloned().fold(1.0f32, f32::max);
            canvas(Line { vals, min: 0.0, max: mx, ticks: vec![0.0, mx / 2.0, mx], fmt: compacto, start: inicio, end: crate::i18n::text("hoje"), color: t::FG2, mark: None }).width(Length::Fill).height(Length::Fixed(150.0))
        };
        col = col.push(h3(crate::i18n::text("Tokens por dia · 7 dias (estimado)"), false));
        col = col.push(grafico(tokens[tokens.len() - 7..].to_vec(), crate::i18n::text("7 dias atrás")));
        col = col.push(h3(crate::i18n::text("Tokens por dia · 30 dias (estimado)"), false));
        col = col.push(grafico(tokens.clone(), crate::i18n::text("30 dias atrás")));
        col = col.push(container(tx(crate::i18n::text("Estimativa a partir do tamanho das respostas"), 11.5, t::FG3)).padding(pad(t::SP_1, t::SP_1, t::SP_1, t::SP_1)));
    }
    painel_section(app, col.into())
}

/// Conteúdo do painel dentro da rolagem das configurações, sem outra página aninhada.
fn painel_section<'a>(app: &'a App, body: El<'a>) -> El<'a> {
    column![
        container(row![tx(crate::i18n::text("Cotas, modelos e a semana, direto do servidor"), 12.5, t::FG3).width(Length::Fill), btn(crate::i18n::text("Exportar o Você fez de hoje"), Kind::Normal, true, None, Some(Msg::VoceFezExportar)), btn(crate::i18n::text("Atualizar"), Kind::Normal, true, Some("refresh"), Some(Msg::Refresh))].spacing(t::SP_3).align_y(Vertical::Center)).padding(pad(t::SP_3, 0.0, t::SP_6, 0.0)),
        weekly_block(app),
        body,
    ].into()
}


/// Resumo semanal: minimizado por padrão; o botão abre (e busca o texto no núcleo) e fecha.
fn weekly_block(app: &App) -> El<'_> {
    if !app.is_live() {
        return sp_h(0.0).into();
    }
    let label = if app.weekly_open { crate::i18n::text("Esconder o resumo semanal") } else { crate::i18n::text("Resumo semanal") };
    let mut col = column![row![btn(label, Kind::Normal, true, None, Some(Msg::WeeklyToggle)), grow()]].spacing(t::SP_25);
    if app.weekly_open {
        let corpo: El = match &app.weekly_text {
            Some(t) => tx(t.clone(), 12.5, t::FG2).into(),
            None => tx(crate::i18n::text("Carregando…"), 12.5, t::FG3).into(),
        };
        col = col.push(container(corpo).padding(pad(t::SP_3, t::SP_35, t::SP_3, t::SP_35)).width(Length::Fill).style(surface(Some(t::RAISE), Some(t::LINE2), 8.0)));
        col = col.push(row![btn(crate::i18n::text("Exportar para o vault"), Kind::Normal, app.weekly_text.is_some(), None, Some(Msg::WeeklyExport)), grow()]);
    }
    container(col).padding(pad(0.0, 0.0, t::SP_6, 0.0)).width(Length::Fill).into()
}

//! Telas de página (§15): Projetos, Aprovações, Rotinas, Resultados, Nuvem, Fontes, Conhecimento, Arquivadas, Vida, Painel.

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
        View::Resultados => resultados(app),
        View::Nuvem => nuvem(app),
        View::Fontes => fontes(app),
        View::Conhecimento => conhecimento(app),
        View::Arquivadas => arquivadas(app),
        View::Vida => vida(),
        View::Painel => painel(app),
        _ => sp_w(0.0),
    }
}

/// `.page`: largura 800, título Newsreader 32 + descrição, botão à direita.
pub fn page<'a>(title: &str, desc: &str, action: Option<El<'a>>, body: El<'a>) -> El<'a> {
    let mut head = row![column![txf(title.to_string(), 32.0, t::FG, t::SERIF).line_height(text::LineHeight::Relative(1.1)), tx(desc.to_string(), 13.0, t::FG3)].spacing(4).width(Length::Fill)].spacing(12).align_y(Vertical::Bottom);
    if let Some(a) = action {
        head = head.push(a);
    }
    let col = column![container(head).padding(pad(0.0, 0.0, 22.0, 0.0)), body].width(Length::Fill);
    let inner = container(col).max_width(800.0).width(Length::Fill).padding(pad(36.0, 20.0, 72.0, 20.0));
    scrollable(container(inner).width(Length::Fill).center_x(Length::Fill)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).width(Length::Fill).height(Length::Fill).into()
}

pub fn li<'a>(left: El<'a>, right: El<'a>) -> El<'a> {
    column![row![container(left).width(Length::Fill), right].spacing(16).align_y(Vertical::Center).padding(pad(13.0, 4.0, 13.0, 4.0)), hline()].into()
}
fn li_title<'a>(parts: Vec<El<'a>>) -> El<'a> {
    row(parts).spacing(8).align_y(Vertical::Center).into()
}
fn li_desc<'a>(s: impl text::IntoFragment<'a>) -> El<'a> {
    container(tx(s, 12.5, t::FG3)).padding(pad(1.0, 0.0, 0.0, 0.0)).into()
}
use iced::widget::text;
fn li_right<'a>(parts: Vec<El<'a>>) -> El<'a> {
    row(parts).spacing(8).align_y(Vertical::Center).into()
}
fn rt<'a>(s: impl text::IntoFragment<'a>) -> El<'a> {
    tx(s, 12.0, t::FG3).into()
}
fn list<'a>(rows: Vec<El<'a>>) -> El<'a> {
    column![hline(), column(rows)].into()
}
fn h3<'a>(s: &str, first: bool) -> El<'a> {
    container(txf(s.to_string(), 11.5, t::FG3, t::UI_MED)).padding(pad(if first { 0.0 } else { 26.0 }, 4.0, 8.0, 4.0)).into()
}
fn search_box<'a>(placeholder: &str, value: &str, on: impl Fn(String) -> Msg + 'a) -> El<'a> {
    let input = text_input(placeholder, value).on_input(on).size(13).font(t::UI).padding(pad(8.0, 12.0, 8.0, 34.0)).style(field_style(t::RAISE, t::LINE2, t::LINE3, 9.0));
    stack![input, container(icon("search", 14.0, t::FG3)).padding(pad(10.0, 0.0, 0.0, 11.0))].into()
}

fn projetos(app: &App) -> El<'_> {
    let q = app.proj_q.trim().to_lowercase();
    let items: Vec<(usize, &data::Project)> = app.projects.iter().enumerate().filter(|(_, p)| q.is_empty() || format!("{}{}", p.n, p.d).to_lowercase().contains(&q)).collect();
    let card = |(i, p): (usize, &data::Project)| -> El<'static> {
        let mut tags = row![].spacing(4);
        if !p.repo.is_empty() {
            tags = tags.push(badge_ic(p.repo.split('/').nth(1).unwrap_or(""), if p.ci { Tone::Ok } else { Tone::Plain }, "branch"));
        }
        tags = tags.push(badge_ic(&p.model, Tone::Plain, "cpu"));
        if p.rot > 0 {
            tags = tags.push(badge_ic(&format!("{} rotina{}", p.rot, if p.rot > 1 { "s" } else { "" }), Tone::Plain, "clock"));
        }
        if !p.pastas.is_empty() {
            tags = tags.push(badge_ic(&ell(&p.pastas, 150.0, 11.0), Tone::Plain, "folder"));
        }
        let body = column![txf(p.n.clone(), 20.0, t::FG, t::SERIF), tx(p.d.clone(), 13.0, t::FG2), container(tags).padding(pad(2.0, 0.0, 0.0, 0.0)), container(sp_h(0.0)).height(Length::Fill), row![tx(p.pend.clone(), 12.0, t::FG3), grow(), tx(format!("Atualizado {}", p.up), 12.0, t::FG3)]].spacing(6);
        button(container(body).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(150.0))
            .padding(pad(16.0, 18.0, 16.0, 18.0))
            .on_press(Msg::OpenProject(i))
            .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(t::RAISE.into()) } else { None }, text_color: t::FG, border: Border { color: if matches!(st, button::Status::Hovered) { t::LINE3 } else { t::LINE2 }, width: 1.0, radius: 14.0.into() }, shadow: Default::default(), snap: true })
            .into()
    };
    let mut grid = column![].spacing(12);
    let cards: Vec<El> = items.into_iter().map(card).collect();
    if cards.is_empty() {
        grid = grid.push(tx("Nenhum projeto encontrado.", 14.0, t::FG3));
    }
    let mut it = cards.into_iter();
    while let Some(a) = it.next() {
        let b = it.next().unwrap_or_else(|| sp_w(0.0));
        grid = grid.push(row![container(a).width(Length::FillPortion(1)), container(b).width(Length::FillPortion(1))].spacing(12));
    }
    page("Projetos", "Nota no vault, instruções, repositório, pastas, modelo padrão e rotinas de cada projeto.", Some(btn("Novo projeto", Kind::Primary, false, None, Some(Msg::Toast("Criar projeto não faz parte do protótipo".into())))), column![container(search_box("Buscar projetos", &app.proj_q, Msg::ProjQ)).padding(pad(0.0, 0.0, 14.0, 0.0)), grid].into())
}

fn aprovacoes(app: &App) -> El<'_> {
    let pend = app.pend_list();
    let mut col = column![h3(&format!("Aguardando você · {}", app.pend_count()), true)].spacing(10);
    for (ci, mi) in pend {
        let c = &app.convs[ci];
        if let data::ChatMsg::Orion(o) = &c.msgs[mi] {
            let a = o.approval.as_ref().unwrap();
            let txt = crate::md::render(&crate::md::parse(&a.text), crate::md::Style { font: t::UI, bold: t::UI_SEMI, size: 14.0, line: 1.5, h_font: t::UI_MED, h1: 16.0, h2: 15.0, h3: 14.0, gap: 4.0 });
            col = col.push(qcard(
                row![dot_halo(t::SIGNAL, t::SIGNAL_SOFT), tx(format!("{} · {}", c.title, c.proj), 12.5, t::FG3), grow(), tx(o.time.clone(), 12.5, t::FG3)].spacing(8).align_y(Vertical::Center).into(),
                txt,
                row![btn("Aprovar", Kind::Primary, true, None, Some(Msg::Approve(ci, mi))), btn("Negar", Kind::Normal, true, None, Some(Msg::Deny(ci, mi))), btn("Abrir conversa", Kind::Ghost, true, None, Some(Msg::OpenConv(ci)))].spacing(8).into(),
            ));
        }
    }
    if app.rot_pend {
        let txt: El = row![tx("Mover 38 arquivos de ", 14.0, t::FG), txf("Downloads", 12.0, t::FG, t::MONO), tx(" para ", 14.0, t::FG), txf("Downloads\\2026-10", 12.0, t::FG, t::MONO), tx(". Nada é apagado.", 14.0, t::FG)].align_y(Vertical::Center).into();
        col = col.push(qcard(
            row![dot_halo(t::SIGNAL, t::SIGNAL_SOFT), tx("Rotina “Organizar Downloads” · proposta", 12.5, t::FG3), grow(), tx("sábado", 12.5, t::FG3)].spacing(8).align_y(Vertical::Center).into(),
            txt,
            row![btn("Aprovar", Kind::Primary, true, None, Some(Msg::ApproveRot)), btn("Negar", Kind::Normal, true, None, Some(Msg::DenyRot))].spacing(8).into(),
        ));
    }
    col = col.push(container(row![icon("send2", 14.0, t::FG3), tx("As mesmas pendências chegam no Telegram, com botões, só para o seu ID.", 12.0, t::FG3)].spacing(6).align_y(Vertical::Center)).padding(pad(4.0, 4.0, 0.0, 4.0)));
    col = col.push(h3("Histórico de atividade", false));
    let opts = ["Todas", "Aguardando", "Negadas ou com falha", "Concluídas"];
    col = col.push(container(seg(&opts, app.act_filter, Msg::ActFilter)).padding(pad(0.0, 0.0, 14.0, 0.0)));
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
            title.push(badge("desfeito", Tone::Plain));
        }
        let tone = match a.kind.as_str() {
            "ok" => Tone::Ok,
            "warn" => Tone::Warn,
            _ => Tone::Err,
        };
        let mut right = vec![badge(&a.label, if a.label == "revert" { Tone::Plain } else { tone })];
        if a.undoable && !a.undone {
            right.push(btn("Desfazer", Kind::Ghost, true, Some("undo"), Some(Msg::Undo(i))));
        }
        rows.push(li(column![li_title(title), li_desc(a.when.clone())].into(), li_right(right)));
    }
    if rows.is_empty() {
        col = col.push(container(tx("Nada neste filtro.", 14.0, t::FG3)).padding(pad(16.0, 4.0, 16.0, 4.0)));
    } else {
        col = col.push(list(rows));
    }
    let auto = btn(if app.p.auto { "Desligar modo autônomo" } else { "Ligar modo autônomo" }, Kind::Normal, true, Some("bolt"), Some(Msg::Toggle("auto")));
    page("Aprovações", "Pendências de todas as conversas e rotinas, e o histórico de tudo o que o Orion fez fora do chat.", Some(auto), col.into())
}

fn qcard<'a>(head: El<'a>, body: El<'a>, actions: El<'a>) -> El<'a> {
    container(column![head, body, actions].spacing(8)).width(Length::Fill).padding(pad(14.0, 16.0, 14.0, 16.0)).style(|_| container::Style { background: Some(t::SIGNAL_SOFT.into()), border: Border { color: t::mix(t::SIGNAL, 0.30), width: 1.0, radius: 14.0.into() }, ..Default::default() }).into()
}

fn runs<'a>(r: &str) -> El<'a> {
    let mut row_ = row![].spacing(3);
    for ch in r.chars() {
        let c = match ch {
            'f' => t::DANGER,
            's' => t::FG4,
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
            title.push(badge("resultado novo", Tone::Ok));
        }
        if r.crit {
            title.push(badge_ic("nuvem se desligado", Tone::Info, "cloud"));
        }
        let st_tone = match r.st.as_str() {
            "ok" | "concedida" => Tone::Ok,
            "falhou" => Tone::Err,
            _ => Tone::Plain,
        };
        let when = if !r.on { "pausada".to_string() } else if r.next.is_empty() { String::new() } else { format!("próxima {}", r.next) };
        let mut linha = r.d.clone();
        for p in [when, if r.last.is_empty() { String::new() } else { format!("última {}", r.last) }] {
            if !p.is_empty() {
                linha = if linha.is_empty() { p } else { format!("{linha} · {p}") };
            }
        }
        let d: El = row![tx(format!("{linha} "), 12.5, t::FG3), badge(&r.st, st_tone)].spacing(4).align_y(Vertical::Center).into();
        let mut lc = column![li_title(title), container(d).padding(pad(1.0, 0.0, 0.0, 0.0))];
        if !r.out.is_empty() {
            lc = lc.push(li_desc(format!("Saída: {}", r.out)));
        }
        let left = lc.into();
        rows.push(li(left, li_right(vec![runs(&r.runs), btn("Rodar agora", Kind::Normal, true, Some("play"), Some(Msg::RotRun(i))), switch(r.on, Msg::RotToggle(i))])));
    }
    page("Rotinas", "Crie pelo chat (“todo dia às 8h me manda o briefing”), por /rotina ou aqui. As críticas rodam na nuvem do Claude se o PC estiver desligado.", Some(btn("Nova rotina", Kind::Primary, false, None, Some(Msg::Toast("Formulário: nome, horário, prompt, projeto, modelo, saída".into())))), list(rows))
}

fn resultados(app: &App) -> El<'_> {
    let rows = app.results.iter().map(|r| li(column![tx(r.0.clone(), 14.0, t::FG), li_desc(r.1.clone())].into(), li_right(vec![badge(&r.3, Tone::Plain), txf(r.4.clone(), 12.0, t::FG3, t::MONO).into(), rt(r.2.clone())]))).collect();
    page("Seus resultados", "Conteúdo que vale guardar, com versões e fontes.", None, list(rows))
}

fn nuvem(app: &App) -> El<'_> {
    let mut rows = vec![];
    for (i, c) in app.cloud.iter().enumerate() {
        let right: Vec<El> = if c.running {
            vec![container(row![spinner(app.spin, 10.0), txf("rodando", 11.0, t::SIGNAL, t::UI_MED)].spacing(5).align_y(Vertical::Center)).padding(pad(1.0, 8.0, 1.0, 8.0)).style(surface(Some(t::SIGNAL_SOFT), None, 999.0)).into(), btn("Cancelar", Kind::Normal, true, None, Some(Msg::CloudCancel(i)))]
        } else {
            let tone = match c.proj.as_str() {
                "concluída" | "concluida" => Tone::Ok,
                "falhou" | "erro" => Tone::Err,
                _ => Tone::Plain,
            };
            vec![badge(&c.proj, tone)]
        };
        let desc = if c.link.is_empty() { c.at.clone() } else { format!("{} · {}", c.at, c.link) };
        rows.push(li(column![li_title(vec![badge(&c.svc, Tone::Info), tx(c.t.clone(), 14.0, t::FG).into()]), li_desc(desc)].into(), li_right(right)));
    }
    for (svc, t_) in &app.cloud_extra {
        rows.push(li(column![li_title(vec![badge(svc, Tone::Info), tx(t_.clone(), 14.0, t::FG).into()]), li_desc("Orion · agora · enviada")].into(), li_right(vec![container(row![spinner(app.spin, 10.0), txf("rodando", 11.0, t::SIGNAL, t::UI_MED)].spacing(5).align_y(Vertical::Center)).padding(pad(1.0, 8.0, 1.0, 8.0)).style(surface(Some(t::SIGNAL_SOFT), None, 999.0)).into()])));
    }
    if rows.is_empty() {
        rows.push(li(column![li_title(vec![tx("Nenhuma tarefa ainda.", 14.0, t::FG3).into()]), li_desc("Peça com /tarefa: o Orion segue sozinho, só lê e propõe, e avisa quando terminar.")].into(), li_right(vec![])));
    }
    page("Segundo plano", "Tarefas longas que o Orion faz sozinho: só lê e propõe, dentro do tempo e dos passos que você deu. Crie com /tarefa.", Some(btn("Nova tarefa", Kind::Normal, true, Some("cloud"), Some(Msg::CloudNew))), list(rows))
}

pub fn src_rows(app: &App) -> Vec<El<'_>> {
    data::SRC.iter().enumerate().map(|(i, (_, tt, d))| setting(tt, d, switch(app.p.src[i], Msg::TogA("src", i)))).collect()
}

fn fontes(app: &App) -> El<'_> {
    page("Fontes", "Só o que está ligado é lido. O desligado não é consultado.", None, list(src_rows(app)))
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
        col = col.push(tx("Memória vazia.", 14.0, t::FG3));
    }
    for g in groups {
        col = col.push(h3(g, g == app.facts[0].g));
        let mut rows = vec![];
        for (i, f) in app.facts.iter().enumerate().filter(|(_, f)| f.g == g) {
            let editing = app.edit_fact.as_ref().filter(|e| e.0 == i);
            let left: El = match editing {
                Some(e) => column![text_input("", &e.1).on_input(Msg::FactText).on_submit(Msg::FactSave).size(14).font(t::UI).padding(pad(5.0, 8.0, 5.0, 8.0)).style(field_style(t::RAISE, t::LINE3, t::LINE3, 7.0)), li_desc(f.src.clone())].spacing(4).into(),
                None => column![tx(f.t.clone(), 14.0, t::FG), li_desc(f.src.clone())].into(),
            };
            let right: El = if editing.is_some() { btn("Salvar", Kind::Primary, true, None, Some(Msg::FactSave)) } else { row![ib("edit", Some(Msg::FactEdit(i)), false), ib("trash", Some(Msg::FactForget(i)), false)].into() };
            rows.push(li(left, right));
        }
        col = col.push(list(rows));
    }
    page("Conhecimento", "Fatos que o Orion aprendeu, cada um com a fonte. Corrigir ou esquecer muda a nota no vault.", Some(btn("Ver no grafo", Kind::Normal, true, Some("graph"), Some(Msg::Go(View::Memoria)))), col.into())
}

fn arquivadas(app: &App) -> El<'_> {
    let mut rows = vec![];
    for (i, a) in app.archived.iter().enumerate() {
        rows.push(li(column![tx(a.0.clone(), 14.0, t::FG), li_desc(format!("{} · {}", a.1, a.2))].into(), li_right(vec![btn("Desarquivar", Kind::Normal, true, None, Some(Msg::Unarchive(i))), ib("trash", Some(Msg::DeleteArchived(i)), false)])));
    }
    let body: El = if rows.is_empty() { container(tx("Nenhuma conversa arquivada.", 14.0, t::FG3)).padding(pad(16.0, 4.0, 16.0, 4.0)).into() } else { list(rows) };
    page("Conversas arquivadas", "Só arquivam quando você pede. Desarquive para continuar ou apague de vez.", None, body)
}

fn vida<'a>() -> El<'a> {
    let t_: Vec<iced::widget::text::Span<'a, (), iced::Font>> = vec![
        iced::widget::span("Entra ").font(t::UI).size(13.5).color(t::FG2),
        iced::widget::span("depois da v1").font(t::UI_SEMI).size(13.5).color(t::FG),
        iced::widget::span(", numa fase própria. Mesmo conteúdo da tela Vida do front original: colar a fatura, revisar e lançar; preços acompanhados; linha do tempo; escudo de foco que só pergunta.").font(t::UI).size(13.5).color(t::FG2),
    ];
    let soon = container(row![icon("clock", 14.0, t::FG2), iced::widget::rich_text(t_).width(Length::Fill).line_height(iced::widget::text::LineHeight::Relative(1.5))].spacing(8)).width(Length::Fill).padding(18);
    page("Vida", "Finanças locais, monitor de preços, linha do tempo e foco. Nada sai do computador.", None, dashed(soon, 14.0, t::LINE2))
}

pub fn hbar<'a>(label: &str, pct: f32, value: &str, color: iced::Color) -> El<'a> {
    hbar_sz(label, pct, value, color, 13.0)
}
pub fn hbar_sz<'a>(label: &str, pct: f32, value: &str, color: iced::Color, vsize: f32) -> El<'a> {
    row![container(tx(label.to_string(), 13.0, t::FG2)).width(Length::Fixed(140.0)), container(meter(pct, 5.0, color)).width(Length::Fill), container(tx(value.to_string(), vsize, t::FG3)).width(Length::Fixed(70.0)).align_x(Horizontal::Right)].spacing(12).align_y(Vertical::Center).padding(pad(7.0, 4.0, 7.0, 4.0)).into()
}

pub fn stats<'a>(items: [(&str, &str); 4]) -> El<'a> {
    let mut r = row![].width(Length::Fill);
    for (i, (n, l)) in items.iter().enumerate() {
        let cell = container(column![txf(n.to_string(), 30.0, t::FG, t::SERIF).line_height(text::LineHeight::Relative(1.1)), tx(l.to_string(), 12.0, t::FG3)]).width(Length::FillPortion(1)).padding(pad(16.0, 4.0, 16.0, if i == 0 { 4.0 } else { 16.0 }));
        if i > 0 {
            r = r.push(vline(80.0));
        }
        r = r.push(cell);
    }
    column![hline(), r, hline()].into()
}

fn painel(app: &App) -> El<'_> {
    if app.api.is_some() {
        return painel_live(app);
    }
    let quota = [("Gemini (API)", 62.0, "62%"), ("Groq", 100.0, "esgotada"), ("Cerebras", 18.0, "18%"), ("OpenRouter (free)", 40.0, "40%"), ("Mistral", 9.0, "9%")];
    let subs = [("Claude (assinatura)", 41.0), ("Codex (assinatura)", 23.0), ("Gemini CLI", 12.0)];
    let mut col = column![h3("Cotas das APIs gratuitas · hoje", true)];
    for (l, v, s) in quota {
        col = col.push(hbar(l, v, s, if v >= 100.0 { t::DANGER } else { t::FG2 }));
    }
    col = col.push(h3("Assinaturas", false));
    for (l, v) in subs {
        col = col.push(hbar(l, v, &format!("{v}%"), t::FG2));
    }
    col = col.push(h3("Saúde do sistema", false));
    col = col.push(container(stats([("296 MB", "RAM do Orion"), ("0,4%", "CPU parado"), ("12%", "GPU"), ("41 GB", "livres no disco")])).padding(pad(0.0, 0.0, 28.0, 0.0)));
    col = col.push(h3("A semana", false));
    let pend = app.pend_count().to_string();
    col = col.push(container(stats([("38", "conversas"), ("12", "ações aprovadas"), (&pend, "aguardando você"), ("45", "notas gravadas no vault")])).padding(pad(0.0, 0.0, 28.0, 0.0)));
    col = col.push(h3("Primeira resposta · últimos 14 dias", false));
    let ch = Line { vals: data::LAT.to_vec(), min: 1.0, max: 2.5, ticks: vec![1.0, 1.5, 2.0, 2.5], fmt: |v| format!("{v:.1} s").replace('.', ","), start: "2 out.", end: "hoje", color: t::FG2, mark: None };
    col = col.push(canvas(ch).width(Length::Fill).height(Length::Fixed(150.0)));
    page("Painel", "Cotas, assinaturas, saúde do sistema e a semana. Dados de exemplo.", Some(btn("Atualizar", Kind::Normal, true, Some("refresh"), Some(Msg::Refresh))), col.into())
}

/// Painel com os números do servidor (`/painel` e `/budget`).
fn painel_live(app: &App) -> El<'_> {
    use serde_json::Value;
    let mut col = column![h3("Cotas das chamadas · semana", true)];
    let quotas: Vec<Value> = app.budget.as_ref().and_then(|b| b.get("quotas")).and_then(|q| q.as_array()).cloned().unwrap_or_default();
    if quotas.is_empty() {
        col = col.push(container(tx("Nenhuma cota configurada. Defina em ORION_BUDGET ou em Configurações › Modelos.", 13.0, t::FG3)).padding(pad(4.0, 4.0, 4.0, 4.0)));
    }
    for q in &quotas {
        let pct = q.get("percent").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        let (used, cap) = (q.get("used").and_then(|x| x.as_i64()).unwrap_or(0), q.get("weekly_calls").and_then(|x| x.as_i64()).unwrap_or(0));
        col = col.push(hbar(q.get("provider").and_then(|x| x.as_str()).unwrap_or("?"), pct, &if pct >= 100.0 { "esgotada".to_string() } else { format!("{used}/{cap}") }, if pct >= 100.0 { t::DANGER } else { t::FG2 }));
    }
    let p = app.painel.clone().unwrap_or(Value::Null);
    col = col.push(h3("Modelos", false));
    let eps = p["modelos"]["endpoints"].as_array().cloned().unwrap_or_default();
    if eps.is_empty() {
        col = col.push(container(tx(if p["modelos"]["configurado"].as_bool().unwrap_or(false) { "Modelos configurados." } else { "Nenhum modelo configurado (ORION_PROVEDORES ou ORION_GATEWAY_URL/MODEL)." }, 13.0, t::FG3)).padding(pad(4.0, 4.0, 4.0, 4.0)));
    }
    for e in &eps {
        let nome = e["nome"].as_str().or(e["name"].as_str()).unwrap_or("modelo");
        col = col.push(li(tx(nome.to_string(), 14.0, t::FG).into(), li_right(vec![badge(e["estado"].as_str().unwrap_or("ativo"), Tone::Ok)])));
    }
    let semana: Vec<f32> = p["semana"].as_array().map(|a| a.iter().map(|d| d["total"].as_f64().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
    let (total, erros): (f32, f32) = (semana.iter().sum(), p["semana"].as_array().map(|a| a.iter().map(|d| d["erros"].as_f64().unwrap_or(0.0) as f32).sum()).unwrap_or(0.0));
    col = col.push(h3("A semana", false));
    let up = p["uptime_s"].as_f64().unwrap_or(0.0);
    let upt = if up >= 3600.0 { format!("{} h", (up / 3600.0) as i64) } else { format!("{} min", (up / 60.0) as i64) };
    col = col.push(container(stats([(&format!("{total:.0}"), "chamadas de modelo"), (&format!("{erros:.0}"), "com erro"), (&app.pend_count().to_string(), "aguardando você"), (&upt, "servidor no ar")])).padding(pad(0.0, 0.0, 28.0, 0.0)));
    if semana.len() >= 2 {
        col = col.push(h3("Chamadas por dia · 7 dias", false));
        let mx = semana.iter().cloned().fold(1.0f32, f32::max);
        let ch = Line { vals: semana, min: 0.0, max: mx.max(1.0), ticks: vec![0.0, mx / 2.0, mx], fmt: |v| format!("{v:.0}"), start: "7 dias atrás", end: "hoje", color: t::FG2, mark: None };
        col = col.push(canvas(ch).width(Length::Fill).height(Length::Fixed(150.0)));
    }
    page("Painel", "Cotas, modelos e a semana, direto do servidor.", Some(btn("Atualizar", Kind::Normal, true, Some("refresh"), Some(Msg::Refresh))), col.into())
}

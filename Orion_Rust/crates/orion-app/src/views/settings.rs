//! Configurações (§18): modal 1060×740, navegação com busca por palavra-chave e as 14 seções.

use crate::app::{App, Msg, Panic, Pop, Sec};
use crate::chart::Line;
use crate::data;
use crate::icons::icon;
use crate::popover::{Popover, Side};
use crate::theme as t;
use crate::ui::*;
use crate::views::login::field_style;
use crate::views::menus;
use crate::views::pages::{hbar, hbar_sz, src_rows};
use iced::alignment::Vertical;
use iced::widget::{button, canvas, column, container, mouse_area, opaque, row, scrollable, stack, text_input};
use iced::{Background, Border, Length};

type Entry = (Sec, &'static str, &'static str, &'static str);
const GROUPS: [(&str, &[Entry]); 2] = [
    (
        "Configurações",
        &[
            (Sec::Geral, "gear", "Geral", "idioma fonte texto largura escala densidade movimento tom respostas"),
            (Sec::Notificacoes, "bell", "Notificações e sons", "notificações sons não perturbe"),
            (Sec::Conta, "user", "Conta", "nome apelido senha sair"),
            (Sec::Privacidade, "shield", "Privacidade e autonomia", "modo autônomo permissões nunca sai pausar apagar histórico"),
            (Sec::Uso, "gauge", "Uso", "limite tokens cotas"),
            (Sec::Modelos, "cpu", "Modelos", "auto claude codex gemini orion provedores ordem cotas chaves"),
            (Sec::Memoria, "graph", "Memória", "vault fatos grafo pasta commit diário"),
            (Sec::Fontes, "db", "Fontes", "vault calendário email web repositório"),
            (Sec::Conectores, "puzzle", "Conectores", "mcp skills plugins wasm importar claude codex telegram"),
        ],
    ),
    (
        "Este computador",
        &[
            (Sec::Sistema, "cpu", "Sistema", "ram memória inicialização win+shift+espaço mica bandeja atualizações backup drive pastas versão"),
            (Sec::Locais, "db", "Modelos locais", "llama.cpp qwen fallback baixar"),
            (Sec::Voz, "wave", "Voz", "palavra orion palmas idioma estilo mini barra"),
            (Sec::Camera, "camera", "Câmera", "presença onde eu deixei rosto legenda objetos"),
            (Sec::Atalhos, "keys", "Atalhos", "teclado comandos"),
        ],
    ),
];

fn all() -> impl Iterator<Item = &'static Entry> {
    GROUPS.iter().flat_map(|g| g.1.iter())
}

pub fn search(q: &str) -> Vec<Sec> {
    let q = q.trim().to_lowercase();
    all().filter(|(_, _, l, kw)| q.is_empty() || format!("{l} {kw}").to_lowercase().contains(&q)).map(|e| e.0).collect()
}

fn sh<'a>(s: &str) -> El<'a> {
    container(txf(s.to_string(), 13.0, t::FG, t::UI_SEMI)).padding(pad(30.0, 0.0, 2.0, 0.0)).into()
}
fn lede<'a>(s: &str) -> El<'a> {
    container(tx(s.to_string(), 12.5, t::FG3)).padding(pad(0.0, 0.0, 6.0, 0.0)).into()
}
fn input<'a>(val: &str, on: impl Fn(String) -> Msg + 'a) -> El<'a> {
    container(text_input("", val).on_input(on).size(13).font(t::UI).padding(pad(7.0, 10.0, 7.0, 10.0)).style(field_style(t::RAISE, t::LINE2, t::LINE3, 8.0))).width(Length::Fixed(220.0)).into()
}
/// Seletor simples `.sel` (sem caixa) com popover.
fn sel_pop<'a>(label: String, pop: Pop, app: &App, menu: El<'a>) -> El<'a> {
    let b = button(row![tx(label, 13.0, t::FG), icon("chevD", 14.0, t::FG3)].spacing(6).align_y(Vertical::Center))
        .height(Length::Fixed(32.0))
        .padding(pad(0.0, 8.0, 0.0, 12.0))
        .on_press(Msg::Pop(pop))
        .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: 8.0.into(), ..Default::default() }, shadow: Default::default(), snap: true });
    Popover::new(b, menu, app.pop == Some(pop)).side(Side::Right).on_dismiss(Msg::ClosePop).into()
}
fn sel_box_pop<'a>(label: String, pop: Pop, app: &App, menu: El<'a>) -> El<'a> {
    Popover::new(sel_box(label, 200.0, Msg::Pop(pop)), menu, app.pop == Some(pop)).side(Side::Right).on_dismiss(Msg::ClosePop).into()
}
fn pick_seg<'a>(opts: &[&str], sel: usize, key: &'static str) -> El<'a> {
    seg(opts, sel, move |i| Msg::Pick(key, i))
}
fn row_c<'a>(items: Vec<El<'a>>) -> El<'a> {
    row(items).spacing(8).align_y(Vertical::Center).into()
}
fn mono_muted<'a>(s: &str) -> El<'a> {
    txf(s.to_string(), 12.0, t::FG3, t::MONO).into()
}
fn t_badge<'a>(title: &str, b: El<'a>) -> El<'a> {
    row![tx(title.to_string(), 13.5, t::FG), b].spacing(8).align_y(Vertical::Center).into()
}
fn never<'a>(items: &[(&str, &str)]) -> El<'a> {
    let mut col = column![].spacing(6);
    let mut it = items.iter();
    while let Some(a) = it.next() {
        let cell = |x: &(&str, &str)| -> El<'a> { row![icon(x.0, 14.0, t::OK), tx(x.1.to_string(), 13.0, t::FG2)].spacing(8).align_y(Vertical::Center).into() };
        let b = it.next().map(cell).unwrap_or_else(|| sp_w(0.0));
        col = col.push(row![container(cell(a)).width(Length::FillPortion(1)), container(b).width(Length::FillPortion(1))].spacing(20));
    }
    container(col).padding(pad(10.0, 0.0, 10.0, 0.0)).into()
}

fn section<'a>(app: &'a App) -> El<'a> {
    let p = &app.p;
    let sw = |on: bool, k: &'static str| switch(on, Msg::Toggle(k));
    let swa = |arr: &'static str, i: usize, on: bool| switch(on, Msg::TogA(arr, i));
    let toast = |s: &str| Msg::Toast(s.to_string());
    let mut c = column![].spacing(0);
    match app.sec {
        Sec::Geral => {
            c = c.push(sh("Interface"));
            c = c.push(setting("Idioma", "Português e inglês desde a primeira versão.", pick_seg(&["Português", "English"], p.english as usize, "lang")));
            let sc = [90, 100, 115, 130, 150].iter().position(|x| *x == p.scale).unwrap_or(1);
            c = c.push(setting("Escala da interface", "Também com Ctrl + e Ctrl −.", pick_seg(&["90%", "100%", "115%", "130%", "150%"], sc, "scale")));
            c = c.push(setting("Densidade", "Altura das linhas na barra lateral e nas listas.", pick_seg(&["Confortável", "Compacta"], p.compact as usize, "density")));
            c = c.push(setting("Movimento", "Animações discretas. Reduzido desliga o texto em tempo real e a rotação do grafo.", pick_seg(&["Sistema", "Reduzido"], p.reduced as usize, "motion")));
            c = c.push(sh("Conversa"));
            let fonts = ["Newsreader", "Geist", "Fonte do sistema"];
            let menu = menus::menu_box(vec![menus::item(None, "Newsreader", Some("Serifada, como uma página"), p.font == 0, false, Msg::Pick("font", 0)), menus::item(None, "Geist", Some("Sem serifa, igual à interface"), p.font == 1, false, Msg::Pick("font", 1)), menus::item(None, "Fonte do sistema", Some("Segoe UI no Windows"), p.font == 2, false, Msg::Pick("font", 2))], 260.0);
            c = c.push(setting("Fonte do chat", "Usada nas respostas do Orion.", sel_pop(fonts[p.font].to_string(), Pop::ChatFont, app, menu)));
            c = c.push(setting("Tamanho do texto", "", pick_seg(&["Pequeno", "Médio", "Grande"], p.size, "size")));
            c = c.push(setting("Largura da transcrição", "", pick_seg(&["Estreito", "Médio", "Largo"], p.width, "width")));
            c = c.push(setting("Tom das respostas", "Vale para todos os projetos; cada projeto pode ter instruções próprias.", pick_seg(&["Conciso", "Normal", "Detalhado"], p.tone, "tone")));
            let ta = container(text_input("Ex.: sem preâmbulo, código completo, português técnico", &p.tone_text).on_input(Msg::ToneText).size(13).font(t::UI).padding(pad(10.0, 12.0, 10.0, 12.0)).style(field_style(t::RAISE, t::LINE2, t::LINE3, 10.0))).width(Length::Fill).height(Length::Fixed(70.0));
            c = c.push(container(column![tx("Como o Orion deve responder (opcional)", 12.5, t::FG3), ta].spacing(6)).padding(pad(4.0, 0.0, 14.0, 0.0)));
        }
        Sec::Notificacoes => {
            c = c.push(sh("Notificações do Windows"));
            for (i, l) in ["Aprovação pendente", "Rotina concluída", "Tarefa em segundo plano terminou", "Resposta pronta com a janela em segundo plano"].iter().enumerate() {
                c = c.push(setting(l, "", swa("notif", i, p.notif[i])));
            }
            c = c.push(sh("Sons sutis"));
            c = c.push(setting("Aprovação pendente", "", swa("sounds", 0, p.sounds[0])));
            c = c.push(setting("Resposta pronta (janela fora de foco)", "", swa("sounds", 1, p.sounds[1])));
            c = c.push(setting("Rotina concluída", "", swa("sounds", 2, p.sounds[2])));
            c = c.push(setting("Ativação da voz", "Ao ouvir “Orion” ou duas palmas.", swa("sounds", 3, p.sounds[3])));
            c = c.push(sh("Não perturbe"));
            c = c.push(setting("Não perturbe", "Silencia notificações e sons. Rotinas e Telegram continuam.", sw(p.dnd, "dnd")));
            c = c.push(lede("Com a janela fechada, avisos também chegam pelo Telegram e por um ponto no ícone da bandeja."));
        }
        Sec::Conta => {
            c = c.push(sh("Perfil"));
            c = c.push(setting("Nome completo", "", input(&p.name, Msg::NameChanged)));
            c = c.push(setting("Como o Orion deve te chamar", "Aparece na saudação e na barra lateral.", input(&p.call, Msg::CallChanged)));
            c = c.push(setting("Senha deste computador", "Pedida ao abrir o Orion e quando o Windows é bloqueado (Win + L).", btn("Trocar", Kind::Normal, true, None, Some(toast("Trocar senha não faz parte do protótipo")))));
            c = c.push(sh("Sessão"));
            c = c.push(setting("Sair deste computador", "Pede a senha de novo para entrar.", btn("Sair", Kind::Normal, true, None, Some(Msg::Logout))));
        }
        Sec::Privacidade => {
            c = c.push(sh("Modo autônomo"));
            c = c.push(setting("Modo autônomo", "Ligado, o Orion age sem pedir aprovação. Continua pedindo para tudo que pode quebrar o PC ou o projeto: apagar, mexer no sistema, force-push, reescrever histórico, desinstalar.", sw(p.auto, "auto")));
            c = c.push(sh("Sempre sem aprovação"));
            c = c.push(never(&[("check", "Ler arquivos nas pastas confiáveis"), ("check", "Gravar notas no vault + commit na main"), ("check", "Busca na web sem dados pessoais"), ("check", "Comandos de leitura (git status, cargo check)")]));
            c = c.push(sh("Nunca sai do computador"));
            c = c.push(never(&[("shield", "Senhas e chaves (filtradas antes de enviar)"), ("shield", "Documentos pessoais (RG, CPF, banco: mascarados)")]));
            c = c.push(lede("Cada resposta que usou uma API de fora mostra um ícone discreto; clicando, aparece o que saiu e para qual provedor."));
            c = c.push(sh("Parada"));
            let ctl: El = match app.panic {
                Panic::Idle => btn("Pausar", Kind::Normal, true, None, Some(Msg::PanicArm)),
                Panic::Armed => row_c(vec![btn("Cancelar", Kind::Normal, true, None, Some(Msg::PanicCancel)), btn("Confirmar pausa", Kind::Danger, true, None, Some(Msg::PanicConfirm))]),
                Panic::Paused => {
                    let mut v: Vec<El> = vec![badge("Pausado", Tone::Err)];
                    if app.api.is_some() {
                        v.push(container(text_input("Senha para retomar", &app.resume_pw).on_input(Msg::ResumePw).on_submit(Msg::PanicResume).secure(true).size(13).font(t::UI).padding(pad(6.0, 10.0, 6.0, 10.0)).style(field_style(t::RAISE, t::LINE2, t::LINE3, 8.0))).width(Length::Fixed(170.0)).into());
                    }
                    v.push(btn("Retomar", Kind::Normal, true, None, Some(Msg::PanicResume)));
                    row_c(v)
                }
            };
            c = c.push(setting("Pausar tudo", "Corta ferramentas de escrita, rede e execução até você retomar.", ctl));
            c = c.push(sh("Dados"));
            let ctl: El = if app.wipe { row_c(vec![btn("Cancelar", Kind::Normal, true, None, Some(Msg::WipeCancel)), btn(&format!("Apagar {} conversas", app.convs.len()), Kind::Danger, true, None, Some(Msg::WipeConfirm))]) } else { btn("Apagar…", Kind::Danger, true, None, Some(Msg::WipeArm)) };
            c = c.push(setting("Apagar histórico de conversas", "Remove as conversas deste computador. As notas no vault continuam.", ctl));
        }
        Sec::Uso => {
            if app.api.is_some() {
                c = c.push(sh("Cotas das chamadas · semana"));
                let qs: Vec<serde_json::Value> = app.budget.as_ref().and_then(|b| b.get("quotas")).and_then(|q| q.as_array()).cloned().unwrap_or_default();
                if qs.is_empty() {
                    c = c.push(lede("Nenhuma cota configurada."));
                }
                for q in &qs {
                    let pct = q.get("percent").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    let (u, cap) = (q.get("used").and_then(|x| x.as_i64()).unwrap_or(0), q.get("weekly_calls").and_then(|x| x.as_i64()).unwrap_or(0));
                    c = c.push(hbar_sz(q.get("provider").and_then(|x| x.as_str()).unwrap_or("?"), pct, &format!("{u} de {cap}"), if pct >= 100.0 { t::DANGER } else { t::FG2 }, 11.5));
                }
            } else {
            c = c.push(sh("Cotas das APIs gratuitas · hoje"));
            for (l, v, s) in [("Gemini (API)", 62.0, "930 de 1.500"), ("Groq", 100.0, "esgotada até 00:00"), ("Cerebras", 18.0, "180 de 1.000"), ("OpenRouter (free)", 40.0, "20 de 50"), ("Mistral", 9.0, "90 de 1.000")] {
                c = c.push(hbar_sz(l, v, s, if v >= 100.0 { t::DANGER } else { t::FG2 }, 11.5));
            }
            c = c.push(sh("Assinaturas"));
            for (l, v, s) in [("Claude (assinatura)", 41.0, "41%"), ("Codex (assinatura)", 23.0, "23%"), ("Gemini CLI", 12.0, "12%")] {
                c = c.push(hbar(l, v, s, t::FG2));
            }
                    }
        }
        Sec::Modelos => {
            c = c.push(sh("Seletor do chat"));
            c = c.push(lede("Auto, Claude, Codex, Gemini e Orion. Claude, Codex e Gemini usam as assinaturas pelas CLIs oficiais, com modelo e esforço."));
            c = c.push(setting("Padrão para conversas novas", "", sel_box_pop(p.model_label(), Pop::DefaultModel, app, menus::model_menu(app))));
            c = c.push(setting("Mostrar quem respondeu", "Discreto, na linha da hora de cada resposta.", badge("sempre", Tone::Ok)));
            c = c.push(sh("Orion · ordem das APIs gratuitas"));
            c = c.push(lede("Quando uma cota acaba, passa para a próxima. Se todas acabarem ou não houver internet, usa o modelo local."));
            for (i, (l, v)) in [("Gemini (API)", 62), ("Groq", 100), ("Cerebras", 18), ("OpenRouter (free)", 40), ("Mistral", 9)].iter().enumerate() {
                let d = if *v >= 100 { "Sem cota até 00:00".to_string() } else { format!("{}% da cota disponível", 100 - v) };
                c = c.push(setting_el(row![mono_muted(&(i + 1).to_string()), tx(l.to_string(), 13.5, t::FG)].spacing(8).into(), &d, row_c(vec![ib("up", Some(toast("Reordenar (simulado)")), false), btn("Chave", Kind::Normal, true, None, Some(toast("Colar a chave (simulado)")))])));
            }
            c = c.push(setting_el(row![mono_muted("∞"), tx("Local · Qwen 2.5 7B", 13.5, t::FG)].spacing(8).into(), "Fallback sem internet. Gerenciado em Modelos locais.", badge("fallback", Tone::Plain)));
        }
        Sec::Memoria => {
            c = c.push(sh("Memória no vault"));
            let (n, l) = (app.mem.nodes.len(), app.mem.links.len());
            c = c.push(lede(&format!("A memória é o vault: {} notas ligadas por {} wikilinks.", fmt_n(n), fmt_n(l))));
            c = c.push(setting("Usar memória nas respostas", "Desligado, cada conversa começa do zero.", sw(p.mem, "mem")));
            c = c.push(setting("Pasta das notas do Orion", "Todas as conversas viram resumo aqui, sem perguntar.", txf("03 Recursos/Orion/Memória/", 12.0, t::FG, t::MONO).into()));
            c = c.push(setting("Resumo diário das conversas triviais", "Uma nota por dia em Memória/Diário, gerada às 23:59.", badge("23:59", Tone::Ok)));
            c = c.push(setting("Commit e push", "Direto na main do vault, um commit por nota gravada. Sem PR.", badge("automático", Tone::Ok)));
            c = c.push(setting("git pull do vault", "Ao abrir o app, a cada conversa nova, antes de gravar uma nota e ao abrir a Memória.", btn("Agora", Kind::Normal, true, None, Some(toast("git pull: já atualizado")))));
            c = c.push(setting("Se você editou a mesma nota no Obsidian", "O Orion pergunta no chat e mostra as duas versões.", badge("perguntar", Tone::Plain)));
            c = c.push(setting("Grafo", "3D no app final, como o Obsidian, sobre o céu do front original.", btn("Abrir", Kind::Normal, true, None, Some(Msg::SetGo(crate::app::View::Memoria)))));
        }
        Sec::Fontes => {
            c = c.push(sh("Fontes"));
            c = c.push(lede("Só o que está ligado é lido."));
            for r in src_rows(app) {
                c = c.push(r);
            }
        }
        Sec::Conectores => {
            let live = app.api.is_some();
            c = c.push(sh("Servidores MCP"));
            c = c.push(lede("Por comando local (stdio) ou URL (HTTP), definidos no mcp.json da pasta do Orion. As ferramentas aparecem no chat com o nome do servidor."));
            if live && app.mcp.is_empty() {
                c = c.push(setting("Nenhum servidor configurado", "Crie o mcp.json na pasta do Orion e reinicie.", badge("vazio", Tone::Plain)));
            }
            for (i, m) in app.mcp.iter().enumerate() {
                let tone = if m.ok { Tone::Ok } else { Tone::Err };
                if live {
                    c = c.push(setting_el(t_badge(&m.t, badge(&m.st, tone)), &format!("{} · {} ferramentas", m.k, m.tools), btn("Testar", Kind::Normal, true, None, Some(Msg::McpTest(i)))));
                } else {
                    let lab = if m.ok { "Gerenciar" } else { "Reconectar" };
                    c = c.push(setting_el(t_badge(&m.t, badge(&m.st, tone)), &format!("{} · {} ferramentas · permissão por ferramenta: perguntar", m.k, m.tools), btn(lab, Kind::Normal, true, None, Some(toast(&format!("{lab} (simulado)"))))));
                }
            }
            if !live {
                c = c.push(setting("Adicionar servidor", "", btn("Adicionar", Kind::Normal, true, Some("plus"), Some(toast("Comando ou URL do servidor (simulado)")))));
            }
            c = c.push(sh("Skills"));
            c = c.push(lede("Pastas com SKILL.md, no formato das skills do Claude."));
            if live && app.skills.is_empty() {
                c = c.push(setting("Nenhuma skill instalada", "", badge("vazio", Tone::Plain)));
            }
            for (i, sk) in app.skills.iter().enumerate() {
                if live {
                    let ctl: El = if sk.ligavel { switch(sk.on, Msg::SkillToggle(i)) } else { badge("fonte desligada", Tone::Plain) };
                    c = c.push(setting_el(txf(sk.name.clone(), 12.0, t::FG, t::MONO).into(), &sk.desc, ctl));
                } else {
                    c = c.push(setting_el(txf(sk.name.clone(), 12.0, t::FG, t::MONO).into(), &sk.desc, swa("skill", i, p.skills.get(i).copied().unwrap_or(true))));
                }
            }
            if !live {
                c = c.push(sh("Plugins"));
                c = c.push(setting("Plugins em Rust/WASM", "Código isolado em sandbox, com permissões declaradas.", btn("Instalar", Kind::Normal, true, None, Some(toast("Instalar plugin .wasm (simulado)")))));
                c = c.push(sh("Importar"));
                c = c.push(setting("Do Claude e do Codex", "Lê mcp.json, configs e skills já instaladas neste PC.", btn("Procurar", Kind::Normal, true, None, Some(toast("Encontrados 3 servidores e 5 skills (simulado)")))));
            }
        }
        Sec::Sistema => {
            c = c.push(sh("Memória RAM"));
            c = c.push(container(row![txf("296 MB", 36.0, t::FG, t::SERIF), container(tx("em uso agora · quanto menos, melhor", 12.5, t::FG3)).padding(pad(14.0, 0.0, 0.0, 8.0))].align_y(Vertical::Bottom)).padding(pad(10.0, 0.0, 0.0, 0.0)));
            let leg = |col: iced::Color, s: &str| -> El<'a> { row![container(sp_w(0.0)).width(Length::Fixed(10.0)).height(Length::Fixed(2.0)).style(surface(Some(col), None, 1.0)), tx(s.to_string(), 12.0, t::FG3)].spacing(6).align_y(Vertical::Center).into() };
            c = c.push(container(row![leg(t::FG2, "Uso nas últimas 18 h"), leg(t::SIGNAL, "Otimização aplicada")].spacing(14)).padding(pad(16.0, 0.0, 4.0, 0.0)));
            let ch = Line { vals: data::RAMS.to_vec(), min: 0.0, max: 1400.0, ticks: vec![0.0, 400.0, 800.0, 1200.0], fmt: |v| if v == 0.0 { "0".into() } else if v >= 1000.0 { format!("{:.1} GB", v / 1000.0).replace('.', ",") } else { format!("{v} MB") }, start: "ontem, 16:00", end: "agora", color: t::FG2, mark: Some((10, "mmap e voz sob demanda")) };
            c = c.push(canvas(ch).width(Length::Fill).height(Length::Fixed(150.0)));
            let seg_bar = row![
                container(sp_w(0.0)).width(Length::FillPortion(28)).height(Length::Fill).style(surface(Some(t::FG2), None, 0.0)),
                container(sp_w(0.0)).width(Length::FillPortion(63)).height(Length::Fill).style(surface(Some(t::mix(t::FG2, 0.45)), None, 0.0)),
                container(sp_w(0.0)).width(Length::FillPortion(8)).height(Length::Fill).style(surface(Some(t::SIGNAL), None, 0.0)),
                sp_w(0.0).into_filler(1)
            ]
            .spacing(2)
            .height(Length::Fixed(8.0));
            c = c.push(container(seg_bar).padding(pad(18.0, 0.0, 12.0, 0.0)));
            let lg = |col: iced::Color, s: &str, v: &str, outline: bool| -> El<'a> {
                let sq = container(sp_w(0.0)).width(Length::Fixed(8.0)).height(Length::Fixed(8.0)).style(move |_| container::Style { background: if outline { None } else { Some(col.into()) }, border: Border { color: t::LINE3, width: if outline { 1.0 } else { 0.0 }, radius: 2.0.into() }, ..Default::default() });
                row![sq, tx(s.to_string(), 12.5, t::FG2), grow(), tx(v.to_string(), 12.5, t::FG)].spacing(8).align_y(Vertical::Center).into()
            };
            c = c.push(column![
                row![container(lg(t::FG2, "Núcleo e interface", "84 MB", false)).width(Length::FillPortion(1)), container(lg(t::mix(t::FG2, 0.45), "Índice vetorial (mmap)", "188 MB", false)).width(Length::FillPortion(1))].spacing(24),
                row![container(lg(t::SIGNAL, "Cache de embeddings", "24 MB", false)).width(Length::FillPortion(1)), container(lg(t::LINE3, "Voz e grafo (descarregados)", "0 MB", true)).width(Length::FillPortion(1))].spacing(24)
            ].spacing(8));
            c = c.push(sh("Janela"));
            c = c.push(setting("Atalho global Ctrl + Alt + Espaço", "Abre a mini barra de qualquer app. Ctrl + Alt + O: captura rápida · Ctrl + Alt + I: Isso? · Ctrl + Alt + P: pânico.", sw(p.shortcut, "shortcut")));
            c = c.push(setting("Fundo Mica", "Transparência do Windows 11 na barra lateral.", sw(p.mica, "mica")));
            c = c.push(setting("Iniciar com o Windows", "Ao ligar o PC, abre minimizado na bandeja, sem janela até ser chamado. Fechar a janela também vai para a bandeja.", sw(p.boot, "boot")));
            c = c.push(setting("Pastas confiáveis", "O Orion pode ler e escrever nessas pastas e nas de dentro.", btn("Gerenciar", Kind::Normal, true, None, Some(toast("C:\\Projetos e C:\\Users\\Antonio\\Vault")))));
            c = c.push(sh("Este PC e o notebook"));
            c = c.push(setting("Sincronia do banco", "PC principal manda; o notebook replica pela Tailscale quando os dois estão ligados.", badge("notebook em dia · 09:10", Tone::Ok)));
            c = c.push(sh("Atualizações e backup"));
            c = c.push(setting("Atualizações", "Baixa do GitHub Releases em segundo plano, avisa e instala ao fechar.", badge("0.6.0 · em dia", Tone::Plain)));
            c = c.push(setting("Backup no Google Drive", "Banco e configurações, criptografados.", badge("ontem, 03:00", Tone::Ok)));
            c = c.push(setting("Exportar ou importar", "Para trocar de PC.", row_c(vec![btn("Exportar", Kind::Normal, true, None, Some(toast("Exportar (simulado)"))), btn("Importar", Kind::Normal, true, None, Some(toast("Importar (simulado)")))])));
            let en = p.english;
            c = c.push(sh(if en { "Advanced" } else { "Avançado" }));
            c = c.push(setting(if en { "Show advanced" } else { "Mostrar avançado" }, if en { "Dangerous or support options. Hidden by default." } else { "Opções perigosas ou de suporte. Ficam escondidas por padrão." }, sw(p.advanced, "advanced")));
            if p.advanced {
                c = c.push(setting(if en { "Debug logs" } else { "Logs de depuração" }, if en { "Writes detailed log files for support." } else { "Gera arquivos de log detalhados para suporte." }, btn(if en { "Soon" } else { "Em breve" }, Kind::Normal, true, None, None)));
                c = c.push(setting(if en { "Reset database" } else { "Resetar banco" }, if en { "Deletes the local history. This cannot be undone." } else { "Apaga o histórico local. Não tem como desfazer." }, btn(if en { "Soon" } else { "Em breve" }, Kind::Normal, true, None, None)));
            }
            let en = p.english;
            c = c.push(sh(if en { "About" } else { "Sobre" }));
            let desc = if en { "Version {v}. One executable, no Python, no open port." } else { "Versão {v}. Um só executável, sem Python e sem porta aberta." };
            c = c.push(setting("Orion", &desc.replace("{v}", env!("CARGO_PKG_VERSION")), badge(env!("CARGO_PKG_VERSION"), Tone::Plain)));
            let (notes, soon) = if en { ("Release notes for this version have not been written yet.", "Soon") } else { ("Notas desta versão ainda não foram escritas.", "Em breve") };
            c = c.push(setting(if en { "What's new" } else { "O que mudou" }, notes, btn(soon, Kind::Normal, true, None, None)));
            let lic = if en { "All rights reserved. Private source code; see the LICENSE file in the repository." } else { "Todos os direitos reservados. Código-fonte privado; veja o arquivo LICENSE do repositório." };
            c = c.push(setting(if en { "License" } else { "Licença" }, lic, badge(if en { "proprietary" } else { "proprietária" }, Tone::Plain)));
        }
        Sec::Locais => {
            let live = app.api.is_some();
            c = c.push(sh("Modelos locais"));
            c = c.push(lede("llama.cpp roda num processo separado que devolve a memória ao terminar. Os modelos só carregam quando usados."));
            for (i, m) in app.locais.iter().enumerate() {
                let gb = format!("{:.1} GB", m.bytes as f64 / 1e9).replace('.', ",");
                let title: El = if m.ativo { t_badge(&m.nome, badge("em uso", Tone::Info)) } else if m.tipo.is_empty() || live { tx(m.nome.clone(), 13.5, t::FG).into() } else { t_badge(&m.nome, badge(&m.tipo, Tone::Info)) };
                let pct = if m.bytes > 0 { (m.baixados * 100 / m.bytes).min(100) } else { 0 };
                let (desc, right): (String, El) = if m.baixando {
                    (format!("{gb} · baixando {pct}%"), badge(&format!("{pct}%"), Tone::Plain).into())
                } else if !m.erro.is_empty() {
                    (format!("{gb} · falhou: {}", m.erro), btn("Tentar de novo", Kind::Normal, true, None, Some(Msg::ModelGet(i))))
                } else if m.instalado {
                    (format!("{gb} no disco · baixado"), btn("Remover", Kind::Normal, !m.ativo, None, if m.ativo { None } else { Some(Msg::ModelDel(i)) }))
                } else {
                    (format!("{gb} · não baixado"), btn("Baixar", Kind::Normal, true, Some("plus"), Some(Msg::ModelGet(i))))
                };
                c = c.push(setting_el(title, &desc, right));
            }
            if !live {
                c = c.push(setting("Baixar modelo", "Do Hugging Face, formato GGUF.", btn("Baixar", Kind::Normal, true, Some("plus"), Some(toast("Procurar no Hugging Face (simulado)")))));
            }
        }
        Sec::Voz => {
            c = c.push(lede(if p.english { "Coming soon: voice arrives with phase F15. The settings below are saved for when it exists." } else { "Em breve: a voz chega com a fase F15. Os ajustes abaixo já ficam salvos para quando ela existir." }));
            c = c.push(sh("Ativação"));
            c = c.push(setting("Palavra “Orion”", "Escuta localmente. Traz a janela para o foco.", sw(p.wake, "wake")));
            c = c.push(setting("Duas palmas", "Também traz a janela para o foco. Em calibração no notebook.", sw(p.clap, "clap")));
            c = c.push(setting("Testar a mini barra", "Barra flutuante no topo da tela enquanto você fala.", btn(if app.voice { "Fechar" } else { "Mostrar" }, Kind::Normal, true, None, Some(Msg::Live))));
            c = c.push(sh("Fala"));
            c = c.push(setting("Resposta falada", "Só quando o pedido foi por voz.", badge("só por voz", Tone::Plain)));
            c = c.push(setting("Idioma", "", sel_pop(["Português (Brasil)", "English (US)"][p.voice_lang].into(), Pop::SetVoiceLang, app, menus::simple_menu(&["Português (Brasil)", "English (US)"], p.voice_lang, "vlang"))));
            c = c.push(setting("Estilo", "", sel_pop(["Masculina, grave", "Masculina, clara"][p.voice_style].into(), Pop::SetVoiceStyle, app, menus::simple_menu(&["Masculina, grave", "Masculina, clara"], p.voice_style, "vstyle"))));
            c = c.push(setting("Velocidade", "", pick_seg(&["Normal", "Rápido"], p.voice_speed, "vspeed")));
        }
        Sec::Camera => {
            c = c.push(lede(if p.english { "Coming soon: the camera arrives with phase F18. Nothing is captured until then." } else { "Em breve: a câmera chega com a fase F18. Nada é capturado até lá." }));
            c = c.push(sh("Modos"));
            c = c.push(setting("Foto sob pedido", "“Olha isso”: tira uma foto e responde.", swa("cam", 0, p.cam[0])));
            c = c.push(setting("Presença contínua", "Local, sem guardar imagem. Ao sair: pausa a voz e bloqueia o Orion. Na volta: cumprimenta com o que aconteceu.", swa("cam", 1, p.cam[1])));
            c = c.push(lede("Com a câmera ativa, aparece um ponto vermelho na barra de título e no ícone da bandeja."));
            c = c.push(sh("O que o Orion faz com a câmera"));
            c = c.push(setting("“Onde eu deixei?”", "Guarda só rótulos (objeto, lugar, hora), sem prazo. Poucos KB por ano.", swa("cam", 2, p.cam[2])));
            c = c.push(setting("Reconhecer você", "Rosto local do dono.", swa("cam", 3, p.cam[3])));
            c = c.push(setting("Legenda ao vivo", "Faixa flutuante na base da tela.", swa("cam", 4, p.cam[4])));
            c = c.push(sh("Objetos lembrados"));
            let mut tags = row![].spacing(6);
            for (i, o) in p.objs.iter().enumerate() {
                tags = tags.push(container(row![tx(o.clone(), 12.5, t::FG2), button(icon("close", 14.0, t::FG3)).padding(0).on_press(Msg::ObjRemove(i)).style(|_, _| button::Style::default())].spacing(4).align_y(Vertical::Center)).height(Length::Fixed(26.0)).center_y(Length::Fixed(26.0)).padding(pad(0.0, 6.0, 0.0, 10.0)).style(surface(None, Some(t::LINE2), 999.0)));
            }
            c = c.push(container(tags.wrap().vertical_spacing(6)).padding(pad(12.0, 0.0, 12.0, 0.0)));
            c = c.push(row![container(text_input("Adicionar objeto", &app.new_obj).on_input(Msg::NewObj).on_submit(Msg::ObjAdd).size(13).font(t::UI).padding(pad(7.0, 10.0, 7.0, 10.0)).style(field_style(t::RAISE, t::LINE2, t::LINE3, 8.0))).width(Length::Fixed(220.0)), btn("Adicionar", Kind::Normal, true, None, Some(Msg::ObjAdd))].spacing(8));
        }
        Sec::Atalhos => {
            c = c.push(sh("Atalhos"));
            for (l, k) in [("Trazer a janela de qualquer lugar (mini barra)", "Ctrl Alt Espaço"), ("Pânico: corta tudo", "Ctrl Alt P"), ("Buscar conversas e notas", "Ctrl K"), ("Nova conversa", "Ctrl Shift O"), ("Recolher barra lateral", "Ctrl B"), ("Configurações", "Ctrl ,"), ("Enviar", "Enter"), ("Nova linha", "Shift Enter"), ("Comandos", "/"), ("Citar nota do vault", "@"), ("Fechar painel", "Esc")] {
                c = c.push(setting(l, "", kbd(k)));
            }
        }
    }
    c.into()
}

trait Filler {
    fn into_filler(self, p: u16) -> El<'static>;
}
impl<'a> Filler for El<'a> {
    fn into_filler(self, p: u16) -> El<'static> {
        container(sp_w(0.0)).width(Length::FillPortion(p)).height(Length::Fill).style(surface(Some(t::LINE3), None, 0.0)).into()
    }
}

fn fmt_n(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

pub fn modal(app: &App) -> El<'_> {
    let mobile = app.is_drawer_mode();
    let found = search(&app.set_q);
    let mut nav = column![].spacing(1);
    let search_in = stack![
        text_input("Procurar", &app.set_q).on_input(Msg::SetQ).size(13).font(t::UI).padding(pad(7.0, 10.0, 7.0, 32.0)).style(|th, st| {
            let mut s = field_style(t::HOVER, iced::Color::TRANSPARENT, t::LINE2, 8.0)(th, st);
            if matches!(st, text_input::Status::Focused { .. }) {
                s.background = t::BG.into();
            }
            s
        }),
        container(icon("search", 14.0, t::FG3)).padding(pad(9.0, 0.0, 0.0, 10.0)),
    ];
    nav = nav.push(container(search_in).padding(pad(0.0, 0.0, 8.0, 0.0)));
    let mut any = false;
    for (gi, (g, items)) in GROUPS.iter().enumerate() {
        let vis: Vec<&Entry> = items.iter().filter(|e| found.contains(&e.0)).collect();
        if vis.is_empty() {
            continue;
        }
        any = true;
        nav = nav.push(container(tx(g.to_string(), 11.5, t::FG3)).padding(pad(if gi == 0 { 6.0 } else { 16.0 }, 10.0, 4.0, 10.0)));
        for (s, ic, l, _) in vis {
            let cur = app.sec == *s;
            nav = nav.push(
                button(container(row![icon(ic, 16.0, if cur { t::FG } else { t::FG2 }), tx(l.to_string(), 13.5, if cur { t::FG } else { t::FG2 })].spacing(11).align_y(Vertical::Center)).center_y(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fixed(32.0))
                    .padding(pad(0.0, 10.0, 0.0, 10.0))
                    .on_press(Msg::SetSec(*s))
                    .style(move |_, st| button::Style { background: if cur { Some(Background::Color(t::SEL)) } else if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: 8.0.into(), ..Default::default() }, shadow: Default::default(), snap: true }),
            );
        }
    }
    if !any {
        nav = nav.push(container(tx("Nada encontrado.", 13.0, t::FG3)).padding(10));
    }
    let nav_panel = container(scrollable(container(nav).padding(pad(12.0, 8.0, 12.0, 8.0))).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill))
        .width(if mobile { Length::Fill } else { Length::Fixed(236.0) })
        .height(Length::Fill)
        .style(|_| container::Style { background: Some(t::SIDE.into()), ..Default::default() });
    let label = all().find(|e| e.0 == app.sec).map(|e| e.2).unwrap_or("");
    let mut main = column![].spacing(0);
    if mobile {
        main = main.push(container(btn("Configurações", Kind::Ghost, false, Some("back"), Some(Msg::SetBack))).padding(pad(10.0, 0.0, 0.0, 0.0)));
    }
    main = main.push(container(txf(label, 26.0, t::FG, t::SERIF).line_height(iced::widget::text::LineHeight::Relative(1.15))).padding(pad(if mobile { 10.0 } else { 30.0 }, 0.0, 2.0, 0.0)));
    main = main.push(section(app));
    let main_panel = scrollable(container(main).padding(pad(0.0, 44.0, 48.0, 44.0)).width(Length::Fill)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).width(Length::Fill).height(Length::Fill);
    let body: El = if mobile {
        if app.set_detail { main_panel.into() } else { nav_panel.into() }
    } else {
        row![nav_panel, vline_full(), main_panel].into()
    };
    let close = container(tip(ib("close", Some(Msg::CloseSettings), false), "Fechar configurações")).padding(pad(10.0, 12.0, 0.0, 0.0)).width(Length::Fill).align_x(iced::alignment::Horizontal::Right);
    let (w, h) = (1060.0f32.min(app.win.width - 48.0), 740.0f32.min(app.win.height - 48.0));
    let dialog = container(stack![body, close])
        .width(if mobile { Length::Fill } else { Length::Fixed(w) })
        .height(if mobile { Length::Fill } else { Length::Fixed(h) })
        .clip(true)
        .style(move |_| container::Style { background: Some(t::BG.into()), border: Border { color: if mobile { iced::Color::TRANSPARENT } else { t::LINE2 }, width: if mobile { 0.0 } else { 1.0 }, radius: if mobile { 0.0 } else { 18.0 }.into() }, shadow: shadow(), ..Default::default() });
    let (mp, sp) = (crate::anim::MODAL.get(), crate::anim::SCRIM.get());
    let dialog = rise(fade(dialog, mp, t::BG), mp, 6.0);
    let scrim = mouse_area(container(opaque(mouse_area(dialog).on_press(Msg::Noop))).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).style(move |_| container::Style { background: Some(t::alpha(t::SCRIM, sp).into()), ..Default::default() })).on_press(Msg::CloseSettings);
    opaque(scrim)
}

fn vline_full<'a>() -> El<'a> {
    container(sp_w(0.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }).into()
}

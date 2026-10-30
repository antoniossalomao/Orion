//! Configurações (§18): modal 1060×740, navegação com busca por palavra-chave.

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
            (Sec::Tools, "keys", "Ferramentas", "permissões computador web abrir apps confirmação autorização"),
            (Sec::Uso, "gauge", "Uso", "limite tokens cotas"),
            (Sec::Painel, "gauge", "Painel", "Cotas, modelos e a semana, direto do servidor."),
            (Sec::Modelos, "cpu", "Modelos", "auto claude codex gemini orion provedores ordem cotas chaves"),
            (Sec::Memoria, "graph", "Memória", "vault fatos grafo pasta commit diário"),
            (Sec::Fontes, "db", "Fontes", "vault calendário email web repositório"),
            (Sec::Automation, "clock", "Automações e pesquisa", "rotinas downloads briefing pesquisa agenda diário revisão"),
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
    all().filter(|(_, _, l, kw)| q.is_empty() || format!("{l} {kw} {} {}", crate::i18n::text(l), crate::i18n::text(kw)).to_lowercase().contains(&q)).map(|e| e.0).collect()
}

fn sh<'a>(s: &'static str) -> El<'a> {
    container(txf(crate::i18n::text(s), 13.0, t::FG, t::UI_SEMI)).padding(pad(30.0, 0.0, 2.0, 0.0)).into()
}
fn lede<'a>(s: &str) -> El<'a> {
    container(tx(crate::i18n::status(s), 12.5, t::FG3)).padding(pad(0.0, 0.0, 6.0, 0.0)).into()
}
fn input<'a>(val: &str, on: impl Fn(String) -> Msg + 'a) -> El<'a> {
    container(text_input("", val).on_input(on).size(13).font(t::UI).padding(pad(6.0, 10.0, 6.0, 10.0)).style(field_style(t::RAISE, t::FIELD_LINE, t::ACCENT, 8.0))).width(Length::Fixed(220.0)).into()
}
/// Seletor simples `.sel` (sem caixa) com popover.
fn sel_pop<'a>(label: String, pop: Pop, app: &App, menu: El<'a>) -> El<'a> {
    let b = button(row![tx(crate::i18n::status(&label), 13.0, t::FG), icon("chevD", 14.0, t::FG3)].spacing(6).align_y(Vertical::Center))
        .height(Length::Fixed(32.0))
        .padding(pad(0.0, 8.0, 0.0, 12.0))
        .on_press(Msg::Pop(pop))
        .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: t::R_CTRL.into(), ..Default::default() }, shadow: Default::default(), snap: true });
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
        let cell = |x: &(&str, &str)| -> El<'a> { row![icon(x.0, 14.0, t::OK), tx(crate::i18n::status(x.1), 13.0, t::FG2)].spacing(8).align_y(Vertical::Center).into() };
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
    if app.api.is_some() {
        match app.sec {
            Sec::Voz=>return crate::resources::settings(app,"voice"),
            Sec::Camera=>return crate::resources::settings(app,"camera"),
            Sec::Automation=>return crate::resources::settings(app,"automation"),
            _=>{}
        }
    }
    match app.sec {
        Sec::Automation => return crate::resources::settings(app,"automation"),
        Sec::Geral => {
            c = c.push(sh(crate::i18n::text("Interface")));
            c = c.push(setting(crate::i18n::text("Idioma"), crate::i18n::text("Português e inglês desde a primeira versão."), pick_seg(&[crate::i18n::text("Português"), "English"], p.english as usize, "lang")));
            let sc = [90, 100, 115, 130, 150].iter().position(|x| *x == p.scale).unwrap_or(1);
            c = c.push(setting(crate::i18n::text("Escala da interface"), crate::i18n::text("Também com Ctrl + e Ctrl −."), pick_seg(&["90%", "100%", "115%", "130%", "150%"], sc, "scale")));
            c = c.push(setting(crate::i18n::text("Densidade"), crate::i18n::text("Altura das linhas na barra lateral e nas listas."), pick_seg(&[crate::i18n::text("Confortável"), crate::i18n::text("Compacta")], p.compact as usize, "density")));
            c = c.push(setting(crate::i18n::text("Movimento"), crate::i18n::text("Animações discretas. Reduzido desliga o texto em tempo real e a rotação do grafo."), pick_seg(&[crate::i18n::text("Sistema"), crate::i18n::text("Reduzido")], p.reduced as usize, "motion")));
            c = c.push(sh(crate::i18n::text("Conversa")));
            let fonts = ["Newsreader", "Geist", crate::i18n::text("Fonte do sistema")];
            let menu = menus::menu_box(vec![menus::item(None, "Newsreader", Some(crate::i18n::text("Serifada, como uma página")), p.font == 0, false, Msg::Pick("font", 0)), menus::item(None, "Geist", Some(crate::i18n::text("Sem serifa, igual à interface")), p.font == 1, false, Msg::Pick("font", 1)), menus::item(None, crate::i18n::text("Fonte do sistema"), Some(crate::i18n::text("Segoe UI no Windows")), p.font == 2, false, Msg::Pick("font", 2))], 260.0);
            c = c.push(setting(crate::i18n::text("Fonte do chat"), crate::i18n::text("Usada nas respostas do Orion."), sel_pop(fonts[p.font].to_string(), Pop::ChatFont, app, menu)));
            c = c.push(setting(crate::i18n::text("Tamanho do texto"), "", pick_seg(&[crate::i18n::text("Pequeno"), crate::i18n::text("Médio"), crate::i18n::text("Grande")], p.size, "size")));
            c = c.push(setting(crate::i18n::text("Largura da transcrição"), "", pick_seg(&[crate::i18n::text("Estreito"), crate::i18n::text("Médio"), crate::i18n::text("Largo")], p.width, "width")));
            c = c.push(setting(crate::i18n::text("Tom das respostas"), crate::i18n::text("Vale para todos os projetos; cada projeto pode ter instruções próprias."), pick_seg(&[crate::i18n::text("Conciso"), "Normal", crate::i18n::text("Detalhado")], p.tone, "tone")));
            let ta = container(text_input(crate::i18n::text("Ex.: sem preâmbulo, código completo, português técnico"), &p.tone_text).on_input(Msg::ToneText).size(13).font(t::UI).padding(pad(10.0, 12.0, 10.0, 12.0)).style(field_style(t::RAISE, t::FIELD_LINE, t::ACCENT, 10.0))).width(Length::Fill).height(Length::Fixed(70.0));
            c = c.push(container(column![tx(crate::i18n::text("Como o Orion deve responder (opcional)"), 12.5, t::FG3), ta].spacing(6)).padding(pad(4.0, 0.0, 12.0, 0.0)));
        }
        Sec::Notificacoes => {
            c = c.push(sh(crate::i18n::text("Notificações do Windows")));
            for (i, l) in ["Aprovação pendente", "Rotina concluída", "Tarefa em segundo plano terminou", "Resposta pronta com a janela em segundo plano"].iter().enumerate() {
                c = c.push(setting(l, "", swa("notif", i, p.notif[i])));
            }
            c = c.push(sh(crate::i18n::text("Sons sutis")));
            c = c.push(setting(crate::i18n::text("Aprovação pendente"), "", swa("sounds", 0, p.sounds[0])));
            c = c.push(setting(crate::i18n::text("Resposta pronta (janela fora de foco)"), "", swa("sounds", 1, p.sounds[1])));
            c = c.push(setting(crate::i18n::text("Rotina concluída"), "", swa("sounds", 2, p.sounds[2])));
            c = c.push(setting(crate::i18n::text("Ativação da voz"), crate::i18n::text("Ao ouvir “Orion” ou duas palmas."), swa("sounds", 3, p.sounds[3])));
            c = c.push(sh(crate::i18n::text("Não perturbe")));
            c = c.push(setting(crate::i18n::text("Não perturbe"), crate::i18n::text("Silencia notificações e sons. Rotinas e Telegram continuam."), sw(p.dnd, "dnd")));
            c = c.push(lede(crate::i18n::text("Com a janela fechada, avisos também chegam pelo Telegram e por um ponto no ícone da bandeja.")));
        }
        Sec::Conta => {
            c = c.push(sh(crate::i18n::text("Perfil")));
            c = c.push(setting(crate::i18n::text("Nome completo"), "", input(&p.name, Msg::NameChanged)));
            c = c.push(setting(crate::i18n::text("Como o Orion deve te chamar"), crate::i18n::text("Aparece na saudação e na barra lateral."), input(&p.call, Msg::CallChanged)));
            c = c.push(setting(crate::i18n::text("Senha deste computador"), crate::i18n::text("Pedida ao abrir o Orion e quando o Windows é bloqueado (Win + L)."), btn(crate::i18n::text("Trocar"), Kind::Normal, true, None, Some(toast("Trocar senha não faz parte do protótipo")))));
            c = c.push(sh(crate::i18n::text("Sessão")));
            c = c.push(setting(crate::i18n::text("Sair deste computador"), crate::i18n::text("Pede a senha de novo para entrar."), btn(crate::i18n::text("Sair"), Kind::Normal, true, None, Some(Msg::Logout))));
        }
        Sec::Tools => {
            c = c.push(sh("Ferramentas"));
            c = c.push(lede("Autorize uma vez neste computador. A escolha fica salva e pode ser alterada aqui a qualquer momento."));
            c = c.push(lede("Sem confirmações por ação, o Orion pode abrir apps, executar comandos, alterar ou apagar arquivos e usar a web, inclusive após ler documentos externos."));
            let values = p.tools.unwrap_or([false; 3]);
            let ready = p.tools.is_some() && !p.tools_busy;
            c = c.push(btn(
                crate::i18n::text(if values.iter().all(|v| *v) { "Desativar todas as ferramentas" } else { "Autorizar todas as ferramentas" }),
                Kind::Primary, false, None, ready.then_some(Msg::ToolPermission(3)),
            ));
            if p.tools.is_none() {
                c = c.push(lede("Carregando permissões…"));
            }
            for (index, (label, description)) in [
                ("Ferramentas do computador", "Arquivos, aplicativos, comandos e automação do computador."),
                ("Ferramentas da web", "Busca e acesso à web com os serviços configurados."),
                ("Sem confirmação a cada ação", "Usa a autorização salva. Desligado, volta a pedir confirmação para ações que exigem aprovação."),
            ].into_iter().enumerate() {
                let control = if ready { switch(values[index], Msg::ToolPermission(index)) } else { badge(crate::i18n::text(if values[index] { "Ativo" } else { "Desligado" }), Tone::Plain) };
                c = c.push(setting(crate::i18n::text(label), crate::i18n::text(description), control));
            }
            c = c.push(lede("As alterações valem imediatamente. Voz, câmera, captura contínua e integrações ainda precisam de configuração própria. Pânico e registros de ações continuam ativos."));
            c = c.push(lede("Alterar o código do próprio Orion continua exigindo aprovação específica."));
        }
        Sec::Privacidade => {
            c = c.push(sh(crate::i18n::text("Modo autônomo")));
            c = c.push(setting(crate::i18n::text("Modo autônomo"), crate::i18n::text("Permite mais passos por pedido. A autorização para agir sem confirmação fica em Ferramentas."), sw(p.auto, "auto")));
            c = c.push(sh(crate::i18n::text("Sempre sem aprovação")));
            c = c.push(never(&[("check", crate::i18n::text("Ler arquivos nas pastas confiáveis")), ("check", crate::i18n::text("Gravar notas no vault + commit na main")), ("check", crate::i18n::text("Busca na web sem dados pessoais")), ("check", crate::i18n::text("Comandos de leitura (git status, cargo check)"))]));
            c = c.push(sh(crate::i18n::text("Nunca sai do computador")));
            c = c.push(never(&[("shield", crate::i18n::text("Senhas e chaves (filtradas antes de enviar)")), ("shield", crate::i18n::text("Documentos pessoais (RG, CPF, banco: mascarados)"))]));
            c = c.push(lede(crate::i18n::text("Cada resposta que usou uma API de fora mostra um ícone discreto; clicando, aparece o que saiu e para qual provedor.")));
            c = c.push(sh(crate::i18n::text("Parada")));
            let ctl: El = match app.panic {
                Panic::Idle => btn(crate::i18n::text("Pausar"), Kind::Normal, true, None, Some(Msg::PanicArm)),
                Panic::Armed => row_c(vec![btn(crate::i18n::text("Cancelar"), Kind::Normal, true, None, Some(Msg::PanicCancel)), btn(crate::i18n::text("Confirmar pausa"), Kind::Danger, true, None, Some(Msg::PanicConfirm))]),
                Panic::Paused => {
                    let mut v: Vec<El> = vec![badge(crate::i18n::text("Pausado"), Tone::Err)];
                    if app.api.is_some() {
                        v.push(container(text_input(crate::i18n::text("Senha para retomar"), &app.resume_pw).id(crate::island::password_field(crate::island::Purpose::Panic)).on_input(Msg::ResumePw).on_submit(Msg::PanicResume).secure(true).size(13).font(t::UI).padding(pad(6.0, 10.0, 6.0, 10.0)).style(field_style(t::RAISE, t::FIELD_LINE, t::ACCENT, 8.0))).width(Length::Fixed(170.0)).into());
                    }
                    if app.face_id_enabled() {
                        v.push(btn(crate::i18n::text("Face ID"), Kind::Ghost, true, None, (!app.island.busy()).then_some(Msg::Face(crate::island::FaceMsg::Start(crate::island::Purpose::Panic)))));
                    }
                    v.push(btn(crate::i18n::text("Retomar"), Kind::Normal, true, None, Some(Msg::PanicResume)));
                    row_c(v)
                }
            };
            c = c.push(setting(crate::i18n::text("Pausar tudo"), crate::i18n::text("Corta ferramentas de escrita, rede e execução até você retomar."), ctl));
            c = c.push(sh(crate::i18n::text("Dados")));
            let ctl: El = if app.wipe { row_c(vec![btn(crate::i18n::text("Cancelar"), Kind::Normal, true, None, Some(Msg::WipeCancel)), btn(&crate::i18n::formatted("Apagar {} conversas", format!("Apagar {} conversas", app.convs.len())), Kind::Danger, true, None, Some(Msg::WipeConfirm))]) } else { btn(crate::i18n::text("Apagar…"), Kind::Danger, true, None, Some(Msg::WipeArm)) };
            c = c.push(setting(crate::i18n::text("Apagar histórico de conversas"), crate::i18n::text("Remove as conversas deste computador. As notas no vault continuam."), ctl));
            if app.api.is_some(){c=c.push(crate::resources::settings(app,"privacy"));}
        }
        Sec::Painel => return crate::views::pages::painel(app),
        Sec::Uso => {
            if app.api.is_some() {
                c = c.push(sh(crate::i18n::text("Cotas das chamadas · semana")));
                let qs: Vec<serde_json::Value> = app.budget.as_ref().and_then(|b| b.get("quotas")).and_then(|q| q.as_array()).cloned().unwrap_or_default();
                if qs.is_empty() {
                    c = c.push(lede(crate::i18n::text("Nenhuma cota configurada.")));
                }
                for q in &qs {
                    let pct = q.get("percent").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
                    let (u, cap) = (q.get("used").and_then(|x| x.as_i64()).unwrap_or(0), q.get("weekly_calls").and_then(|x| x.as_i64()).unwrap_or(0));
                    c = c.push(hbar_sz(q.get("provider").and_then(|x| x.as_str()).unwrap_or("?"), pct, &crate::i18n::formatted("{u} de {cap}", format!("{u} de {cap}")), if pct >= 100.0 { t::DANGER } else { t::FG2 }, 11.5));
                }
            } else {
            c = c.push(sh(crate::i18n::text("Cotas das APIs gratuitas · hoje")));
            for (l, v, s) in [("Gemini (API)", 62.0, "930 de 1.500"), ("Groq", 100.0, "esgotada até 00:00"), ("Cerebras", 18.0, "180 de 1.000"), ("OpenRouter (free)", 40.0, "20 de 50"), ("Mistral", 9.0, "90 de 1.000")] {
                c = c.push(hbar_sz(l, v, s, if v >= 100.0 { t::DANGER } else { t::FG2 }, 11.5));
            }
            c = c.push(sh(crate::i18n::text("Assinaturas")));
            for (l, v, s) in [("Claude (assinatura)", 41.0, "41%"), ("Codex (assinatura)", 23.0, "23%"), ("Gemini CLI", 12.0, "12%")] {
                c = c.push(hbar(l, v, s, t::FG2));
            }
                    }
        }
        Sec::Modelos => {
            c = c.push(sh(crate::i18n::text("Seletor do chat")));
            c = c.push(lede(crate::i18n::text("Auto, Claude, Codex, Gemini e Orion. Claude, Codex e Gemini usam as assinaturas pelas CLIs oficiais, com modelo e esforço.")));
            c = c.push(setting(crate::i18n::text("Padrão para conversas novas"), "", sel_box_pop(p.model_label(), Pop::DefaultModel, app, menus::model_menu(app))));
            c = c.push(setting(crate::i18n::text("Mostrar quem respondeu"), crate::i18n::text("Discreto, na linha da hora de cada resposta."), badge(crate::i18n::text("sempre"), Tone::Ok)));
            c = c.push(sh(crate::i18n::text("Orion · ordem das APIs gratuitas")));
            c = c.push(lede(crate::i18n::text("Quando uma cota acaba, passa para a próxima. Se todas acabarem ou não houver internet, usa o modelo local.")));
            if app.api.is_some() {
                // uso real: só o que o núcleo informa (nada de cotas de exemplo)
                let pn = app.painel.clone().unwrap_or(serde_json::Value::Null);
                let eps = pn["modelos"]["endpoints"].as_array().cloned().unwrap_or_default();
                if app.painel.is_none() {
                    c = c.push(setting(crate::i18n::text("Provedores"), crate::i18n::text("Carregando o estado dos modelos…"), badge("…", Tone::Plain)));
                } else if eps.is_empty() {
                    let dica = if pn["modelos"]["configurado"].as_bool().unwrap_or(false) { crate::i18n::text("Modelos configurados.") } else { crate::i18n::text("Nenhum modelo configurado. Guarde a chave de um provedor abaixo e use na hora, ou entre com uma CLI (Claude, Codex ou Gemini).") };
                    c = c.push(setting(crate::i18n::text("Provedores"), dica, badge(crate::i18n::text("vazio"), Tone::Plain)));
                }
                for (i, e) in eps.iter().enumerate() {
                    let nome = e["nome"].as_str().or(e["name"].as_str()).unwrap_or("modelo").to_string();
                    let model = e["modelo"].as_str().unwrap_or("?");
                    let error = e["ultimo_erro"].as_str().filter(|error| !error.is_empty());
                    let detail = match error {
                        Some(error) => crate::i18n::formatted("Modelo: {model} · Último erro: {error}", format!("Modelo: {model} · Último erro: {error}")),
                        None => crate::i18n::formatted("Modelo: {model}", format!("Modelo: {model}")),
                    };
                    c = c.push(setting_el(row![mono_muted(&(i + 1).to_string()), tx(nome, 13.5, t::FG)].spacing(8).into(), &detail, {
                        // mesmo critério do painel do núcleo: em quarentena (limite ou falhas seguidas) o provedor não está ativo
                        let seg = e["quarentena_s"].as_i64().unwrap_or(0);
                        if seg > 0 {
                            let motivo = if e["motivo"].as_str() == Some("falhas") { crate::i18n::text("pausa por falhas") } else { crate::i18n::text("quarentena") };
                            badge(&crate::i18n::formatted("{motivo} · {}", format!("{motivo} · {}", if seg >= 60 { format!("{} min", seg / 60) } else { format!("{seg} s") })), Tone::Err)
                        } else if error.is_some() {
                            badge(crate::i18n::text("falha na última chamada"), Tone::Err)
                        } else if e["ok"].as_u64().unwrap_or(0) > 0 {
                            badge(crate::i18n::text("ativo"), Tone::Ok)
                        } else {
                            badge(crate::i18n::text("configurado"), Tone::Plain)
                        }
                    }));
                }
                c = c.push(sh(crate::i18n::text("Chaves de API")));
                c = c.push(lede(crate::i18n::text("Guardadas no cofre do sistema (Credential Manager). Chaves de modelos valem na hora.")));
                for (id, label, nome, url) in crate::onboarding::PROVIDERS {
                    if let Some(f) = app.key_form.as_ref().filter(|f| f.nome == *nome) {
                        let mut input = text_input(crate::i18n::text("Cole a chave de API"), &f.value).id("api-key").secure(true).size(13).font(t::UI).padding(pad(6.0, 10.0, 6.0, 10.0)).style(field_style(t::RAISE, t::FIELD_LINE, t::ACCENT, 8.0));
                        if !f.busy {
                            input = input.on_input(Msg::KeyInput).on_submit(Msg::KeySave);
                        }
                        let desc = if f.error.is_empty() { crate::i18n::formatted("Chave de {label}. Não aparece na tela nem em log.", format!("Chave de {label}. Não aparece na tela nem em log.")) } else { crate::i18n::status(&f.error) };
                        c = c.push(setting_el(
                            tx(label.to_string(), 13.5, t::FG).into(),
                            &desc,
                            row_c(vec![
                                container(input).width(Length::Fixed(260.0)).into(),
                                btn(if f.busy { crate::i18n::text("Guardando…") } else { crate::i18n::text("Guardar") }, Kind::Primary, true, None, (!f.busy).then_some(Msg::KeySave)),
                                btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, (!f.busy).then_some(Msg::KeyCancel)),
                            ]),
                        ));
                    } else {
                        let tem = app.keys.get(*nome).copied().unwrap_or(false);
                        let mut desc = if tem { crate::i18n::text("Chave guardada no cofre do sistema.").to_string() } else { crate::i18n::formatted("Sem chave. Obtenha em {url}", format!("Sem chave. Obtenha em {url}")) };
                        // F02.26: aviso de treino com os dados enviados a este provedor.
                        if let Some((_, policy)) = crate::onboarding::TRAINING_POLICY.iter().find(|(tid, _)| tid == id) {
                            desc = format!("{desc} {}: {}", crate::i18n::text("Treino"), crate::i18n::text(policy));
                        }
                        c = c.push(setting_el(
                            tx(label.to_string(), 13.5, t::FG).into(),
                            &desc,
                            btn(if tem { crate::i18n::text("Trocar") } else { crate::i18n::text("Chave") }, Kind::Normal, true, None, Some(Msg::KeyOpen(nome.to_string()))),
                        ));
                    }
                }
                let quotas: Vec<serde_json::Value> = app.budget.as_ref().and_then(|b| b.get("quotas")).and_then(|q| q.as_array()).cloned().unwrap_or_default();
                for q in &quotas {
                    let pct = q.get("percent").and_then(|x| x.as_f64()).unwrap_or(0.0);
                    let nome = q.get("provider").and_then(|x| x.as_str()).unwrap_or("?").to_string();
                    c = c.push(setting(&nome, &if pct >= 100.0 { crate::i18n::text("Cota da semana esgotada").to_string() } else { crate::i18n::formatted("{:.0}% da cota da semana usada", format!("{:.0}% da cota da semana usada", pct)) }, badge(if pct >= 100.0 { crate::i18n::text("esgotada") } else { "ok" }, if pct >= 100.0 { Tone::Err } else { Tone::Ok })));
                }
            } else {
            for (i, (l, v)) in [("Gemini (API)", 62), ("Groq", 100), ("Cerebras", 18), ("OpenRouter (free)", 40), ("Mistral", 9)].iter().enumerate() {
                    let d = if *v >= 100 { crate::i18n::text("Sem cota até 00:00").to_string() } else { crate::i18n::formatted("{}% da cota disponível", format!("{}% da cota disponível", 100 - v)) };
                    c = c.push(setting_el(row![mono_muted(&(i + 1).to_string()), tx(crate::i18n::text(l), 13.5, t::FG)].spacing(8).into(), &d, row_c(vec![tip(ib("up", Some(toast("Reordenar")), false), crate::i18n::text("Subir na ordem")), btn(crate::i18n::text("Chave"), Kind::Normal, true, None, Some(toast(crate::i18n::text("Colar a chave (simulado)"))))])));
                }
            }
            c = c.push(setting_el(row![mono_muted("∞"), tx(crate::i18n::text("Local · Qwen 2.5 7B"), 13.5, t::FG)].spacing(8).into(), crate::i18n::text("Fallback sem internet. Gerenciado em Modelos locais."), badge("fallback", Tone::Plain)));
        }
        Sec::Memoria => {
            c = c.push(sh(crate::i18n::text("Memória no vault")));
            let (n, l) = (app.mem.nodes.len(), app.mem.links.len());
            c = c.push(lede(&crate::i18n::formatted("A memória é o vault: {} notas ligadas por {} wikilinks.", format!("A memória é o vault: {} notas ligadas por {} wikilinks.", fmt_n(n), fmt_n(l)))));
            if app.api.is_some(){
                c=c.push(crate::resources::settings(app,"memory"));
                for (key,label) in [("facts","Usar fatos nas respostas"),("conversations","Usar conversas anteriores")] {
                    let on=app.resources.sources.as_ref().and_then(|v|v[key].as_bool()).unwrap_or(false);
                    c=c.push(setting(crate::i18n::text(label),"",switch(on,Msg::Resources(crate::resources::Event::Source(key)))));
                }
            }else{c=c.push(setting(crate::i18n::text("Usar memória nas respostas"),"",sw(p.mem,"mem")));}
            c = c.push(setting(crate::i18n::text("Pasta das notas do Orion"), crate::i18n::text("Todas as conversas viram resumo aqui, sem perguntar."), txf("03 Recursos/Orion/Memória/", 12.0, t::FG, t::MONO).into()));
            c = c.push(setting(crate::i18n::text("Resumo diário das conversas triviais"), crate::i18n::text("Uma nota por dia em Memória/Diário, gerada às 23:59. O que você fez aparece em Você fez."), badge("23:59", Tone::Ok)));
            c = c.push(setting(crate::i18n::text("Salvar no GitHub"), crate::i18n::text("Cada nota gravada vira um commit direto na main do vault, sem PR."), badge(crate::i18n::text("automático"), Tone::Ok)));
            c = c.push(setting(crate::i18n::text("git pull do vault"), crate::i18n::text("Ao abrir o app, a cada conversa nova, antes de gravar uma nota e ao abrir a Memória."), btn(crate::i18n::text("Agora"), Kind::Normal, true, None, Some(Msg::VaultPull))));
            c = c.push(setting(crate::i18n::text("Se você editou a mesma nota no Obsidian"), crate::i18n::text("A sua versão fica como está e a nova vira uma proposta. Compare as duas aqui e escolha."), badge(if app.conflitos.is_empty() { crate::i18n::text("sem conflitos") } else { crate::i18n::text("revisar") }, if app.conflitos.is_empty() { Tone::Ok } else { Tone::Plain })));
            for (i, (original, _, existe)) in app.conflitos.iter().enumerate() {
                let nome = original.rsplit('/').next().unwrap_or(original);
                c = c.push(setting(nome, crate::i18n::text("Conflito pendente no vault."), btn(crate::i18n::text("Comparar"), Kind::Normal, *existe, None, Some(Msg::ConflitoAbrir(i)))));
            }
            c = c.push(setting(crate::i18n::text("Grafo"), crate::i18n::text("3D no app final, como o Obsidian, sobre o céu do front original."), btn(crate::i18n::text("Abrir"), Kind::Normal, true, None, Some(Msg::SetGo(crate::app::View::Memoria)))));
        }
        Sec::Fontes => {
            c = c.push(sh(crate::i18n::text("Fontes")));
            c = c.push(lede(crate::i18n::text("Só fontes ligadas entram na busca de memória e nas ferramentas relacionadas.")));
            for r in src_rows(app) {
                c = c.push(r);
            }
        }
        Sec::Conectores => {
            let live = app.api.is_some();
            c = c.push(sh(crate::i18n::text("Servidores MCP")));
            c = c.push(lede(crate::i18n::text("Por comando local (stdio) ou URL (HTTP), definidos no mcp.json da pasta do Orion. As ferramentas aparecem no chat com o nome do servidor.")));
            if live && app.mcp.is_empty() {
                c = c.push(setting(crate::i18n::text("Nenhum servidor configurado"), crate::i18n::text("Use Adicionar servidor MCP para configurar e habilitar a conexão."), badge(crate::i18n::text("vazio"), Tone::Plain)));
            }
            for (i, m) in app.mcp.iter().enumerate() {
                let tone = if m.ok { Tone::Ok } else { Tone::Err };
                if live {
                    c = c.push(setting_el(t_badge(&m.t, badge(&m.st, tone)), &crate::i18n::formatted("{} · {} ferramentas", format!("{} · {} ferramentas", m.k, m.tools)), btn(crate::i18n::text("Testar"), Kind::Normal, true, None, Some(Msg::McpTest(i)))));
                } else {
                    let lab = if m.ok { crate::i18n::text("Gerenciar") } else { crate::i18n::text("Reconectar") };
                    c = c.push(setting_el(t_badge(&m.t, badge(&m.st, tone)), &crate::i18n::formatted("{} · {} ferramentas · permissão por ferramenta: perguntar", format!("{} · {} ferramentas · permissão por ferramenta: perguntar", m.k, m.tools)), btn(lab, Kind::Normal, true, None, Some(toast(&format!("{lab} (simulado)"))))));
                }
            }
            if !live {
                c = c.push(setting(crate::i18n::text("Adicionar servidor"), "", btn(crate::i18n::text("Adicionar"), Kind::Normal, true, Some("plus"), Some(toast("Comando ou URL do servidor (simulado)")))));
            }
            c = c.push(sh("Skills"));
            c = c.push(lede(crate::i18n::text("Pastas com SKILL.md, no formato das skills do Claude.")));
            if live && app.skills.is_empty() {
                c = c.push(setting(crate::i18n::text("Nenhuma skill instalada"), "", badge(crate::i18n::text("vazio"), Tone::Plain)));
            }
            for (i, sk) in app.skills.iter().enumerate() {
                if live {
                    let ctl: El = if sk.ligavel { switch(sk.on, Msg::SkillToggle(i)) } else { badge(crate::i18n::text("fonte desligada"), Tone::Plain) };
                    c = c.push(setting_el(txf(sk.name.clone(), 12.0, t::FG, t::MONO).into(), &sk.desc, ctl));
                } else {
                    c = c.push(setting_el(txf(sk.name.clone(), 12.0, t::FG, t::MONO).into(), &sk.desc, swa("skill", i, p.skills.get(i).copied().unwrap_or(true))));
                }
            }
            if !live {
                c = c.push(sh("Plugins"));
                c = c.push(setting(crate::i18n::text("Plugins em Rust/WASM"), crate::i18n::text("Código isolado em sandbox, com permissões declaradas."), btn(crate::i18n::text("Instalar"), Kind::Normal, true, None, Some(toast("Instalar plugin .wasm (simulado)")))));
                c = c.push(sh(crate::i18n::text("Importar")));
                c = c.push(setting(crate::i18n::text("Do Claude e do Codex"), crate::i18n::text("Lê mcp.json, configs e skills já instaladas neste PC."), btn(crate::i18n::text("Procurar"), Kind::Normal, true, None, Some(toast("Encontrados 3 servidores e 5 skills (simulado)")))));
            }
            if app.api.is_some(){c=c.push(crate::resources::settings(app,"connectors"));}
        }
        Sec::Sistema => {
            let live = app.api.is_some();
            if live {
                c = c.push(setting(crate::i18n::text("Recarregar configurações"), crate::i18n::text("Relê o .env e aplica o que dá sem reiniciar: modelos, chaves, guarda de injeção e diário. O resto avisa que precisa reiniciar."), btn(crate::i18n::text("Recarregar"), Kind::Normal, true, None, Some(Msg::RecarregarConfig))));
                // uso real: só o que dá para medir agora (sem gráfico de exemplo)
                c = c.push(sh(crate::i18n::text("Memória RAM")));
                c = c.push(container(row![txf(format!("{} MB", proc_mem_mb()), 36.0, t::FG, t::SERIF), container(tx(crate::i18n::text("deste processo agora · quanto menos, melhor"), 12.5, t::FG3)).padding(pad(12.0, 0.0, 0.0, 8.0))].align_y(Vertical::Bottom)).padding(pad(6.0, 0.0, 0.0, 0.0)));
            } else {
            c = c.push(sh(crate::i18n::text("Memória RAM")));
                c = c.push(container(row![txf("296 MB", 36.0, t::FG, t::SERIF), container(tx(crate::i18n::text("em uso agora · quanto menos, melhor"), 12.5, t::FG3)).padding(pad(12.0, 0.0, 0.0, 8.0))].align_y(Vertical::Bottom)).padding(pad(10.0, 0.0, 0.0, 0.0)));
                let leg = |col: iced::Color, s: &str| -> El<'a> { row![container(sp_w(0.0)).width(Length::Fixed(10.0)).height(Length::Fixed(2.0)).style(surface(Some(col), None, 1.0)), tx(s.to_string(), 12.0, t::FG3)].spacing(6).align_y(Vertical::Center).into() };
                c = c.push(container(row![leg(t::FG2, crate::i18n::text("Uso nas últimas 18 h")), leg(t::SIGNAL, crate::i18n::text("Otimização aplicada"))].spacing(12)).padding(pad(16.0, 0.0, 4.0, 0.0)));
                let ch = Line { vals: data::RAMS.to_vec(), min: 0.0, max: 1400.0, ticks: vec![0.0, 400.0, 800.0, 1200.0], fmt: |v| if v == 0.0 { "0".into() } else if v >= 1000.0 { format!("{:.1} GB", v / 1000.0).replace('.', ",") } else { format!("{v} MB") }, start: crate::i18n::text("ontem, 16:00"), end: crate::i18n::text("agora"), color: t::FG2, mark: Some((10, "mmap e voz sob demanda")) };
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
                    row![sq, tx(crate::i18n::status(s), 12.5, t::FG2), grow(), tx(v.to_string(), 12.5, t::FG)].spacing(8).align_y(Vertical::Center).into()
                };
                c = c.push(column![
                    row![container(lg(t::FG2, crate::i18n::text("Núcleo e interface"), "84 MB", false)).width(Length::FillPortion(1)), container(lg(t::mix(t::FG2, 0.45), crate::i18n::text("Índice vetorial (mmap)"), "188 MB", false)).width(Length::FillPortion(1))].spacing(24),
                    row![container(lg(t::SIGNAL, crate::i18n::text("Cache de embeddings"), "24 MB", false)).width(Length::FillPortion(1)), container(lg(t::LINE3, crate::i18n::text("Voz e grafo (descarregados)"), "0 MB", true)).width(Length::FillPortion(1))].spacing(24)
                ].spacing(8));
            }
            c = c.push(sh(crate::i18n::text("Janela")));
            c = c.push(setting(crate::i18n::text("Atalho global Ctrl + Alt + Espaço"), crate::i18n::text("Abre a mini barra de qualquer app. Ctrl + Alt + O: captura rápida · Ctrl + Alt + I: Isso? · Ctrl + Alt + P: pânico."), sw(p.shortcut, "shortcut")));
            c = c.push(setting(crate::i18n::text("Fundo Mica"), crate::i18n::text("Transparência do Windows 11 na barra lateral."), sw(p.mica, "mica")));
            c = c.push(setting(crate::i18n::text("Iniciar com o Windows"), crate::i18n::text("Ao ligar o PC, abre minimizado na bandeja, sem janela até ser chamado. Fechar a janela também vai para a bandeja."), sw(p.boot, "boot")));
            if !live {
            c = c.push(setting(crate::i18n::text("Pastas confiáveis"), crate::i18n::text("O Orion pode ler e escrever nessas pastas e nas de dentro."), btn(crate::i18n::text("Gerenciar"), Kind::Normal, true, None, Some(toast("C:\\Projetos e C:\\Users\\Antonio\\Vault")))));
            c = c.push(sh(crate::i18n::text("Este PC e o notebook")));
            c = c.push(setting(crate::i18n::text("Sincronia do banco"), crate::i18n::text("PC principal manda; o notebook replica pela Tailscale quando os dois estão ligados."), badge(crate::i18n::text("notebook em dia · 09:10"), Tone::Ok)));
            c = c.push(sh(crate::i18n::text("Atualizações e backup")));
            c = c.push(setting(crate::i18n::text("Atualizações"), crate::i18n::text("Baixa do GitHub Releases em segundo plano, avisa e instala ao fechar."), badge(crate::i18n::text("0.6.0 · em dia"), Tone::Plain)));
            c = c.push(setting(crate::i18n::text("Backup no Google Drive"), crate::i18n::text("Banco e configurações, criptografados."), badge(crate::i18n::text("ontem, 03:00"), Tone::Ok)));
            c = c.push(setting(crate::i18n::text("Exportar ou importar"), crate::i18n::text("Para trocar de PC."), row_c(vec![btn(crate::i18n::text("Exportar"), Kind::Normal, true, None, Some(toast(crate::i18n::text("Exportar (simulado)")))), btn(crate::i18n::text("Importar"), Kind::Normal, true, None, Some(toast(crate::i18n::text("Importar (simulado)"))))])));
            } else {
                c = c.push(setting(crate::i18n::text("Atualizações"), crate::i18n::text("O instalador atualiza o app com o feed assinado (minisign). Manualmente: `orion update`."), badge(env!("CARGO_PKG_VERSION"), Tone::Plain)));
            }
            let en = p.english;
            c = c.push(sh(if en { "Advanced" } else { crate::i18n::text("Avançado") }));
            c = c.push(setting(if en { "Show advanced" } else { crate::i18n::text("Mostrar avançado") }, if en { "Dangerous or support options. Hidden by default." } else { crate::i18n::text("Opções perigosas ou de suporte. Ficam escondidas por padrão.") }, sw(p.advanced, "advanced")));
            if p.advanced {
                c = c.push(setting(if en { "Debug logs" } else { crate::i18n::text("Logs de depuração") }, if en { "Writes detailed log files for support." } else { crate::i18n::text("Gera arquivos de log detalhados para suporte.") }, btn(if en { "Open logs folder" } else { crate::i18n::text("Abrir pasta de logs") }, Kind::Normal, true, None, Some(Msg::OpenLogs))));
                c = c.push(setting(if en { "Reset database" } else { crate::i18n::text("Resetar banco") }, if en { "Deletes the local history. This cannot be undone." } else { crate::i18n::text("Apaga o histórico local. Não tem como desfazer.") }, btn(if en { "Soon" } else { crate::i18n::text("Em breve") }, Kind::Normal, true, None, None)));
            }
            let en = p.english;
            c = c.push(sh(if en { "About" } else { crate::i18n::text("Sobre") }));
            let desc = if en { "Version {v}. One executable, no Python, no open port." } else { crate::i18n::text("Versão {v}. Um só executável, sem Python e sem porta aberta.") };
            c = c.push(setting("Orion", &desc.replace("{v}", env!("CARGO_PKG_VERSION")), badge(env!("CARGO_PKG_VERSION"), Tone::Plain)));
            let (notes, soon) = if en { ("Release notes for this version have not been written yet.", "Soon") } else { (crate::i18n::text("Notas desta versão ainda não foram escritas."), crate::i18n::text("Em breve")) };
            c = c.push(setting(if en { "What's new" } else { crate::i18n::text("O que mudou") }, notes, btn(soon, Kind::Normal, true, None, None)));
            let lic = if en { "All rights reserved. Private source code; see the LICENSE file in the repository." } else { crate::i18n::text("Todos os direitos reservados. Código-fonte privado; veja o arquivo LICENSE do repositório.") };
            c = c.push(setting(if en { "License" } else { crate::i18n::text("Licença") }, lic, badge(if en { "proprietary" } else { crate::i18n::text("proprietária") }, Tone::Plain)));
            if app.api.is_some(){c=c.push(crate::resources::settings(app,"storage"));}
        }
        Sec::Locais => {
            let live = app.api.is_some();
            c = c.push(sh(crate::i18n::text("Modelos locais")));
            c = c.push(lede(crate::i18n::text("llama.cpp roda num processo separado que devolve a memória ao terminar. Os modelos só carregam quando usados.")));
            for (i, m) in app.locais.iter().enumerate() {
                let gb = format!("{:.1} GB", m.bytes as f64 / 1e9).replace('.', ",");
                let title: El = if m.ativo { t_badge(&m.nome, badge(crate::i18n::text("em uso"), Tone::Info)) } else if m.tipo.is_empty() || live { tx(m.nome.clone(), 13.5, t::FG).into() } else { t_badge(&m.nome, badge(&m.tipo, Tone::Info)) };
                let pct = if m.bytes > 0 { (m.baixados * 100 / m.bytes).min(100) } else { 0 };
                let (desc, right): (String, El) = if m.baixando {
                    (crate::i18n::formatted("{gb} · baixando {pct}%", format!("{gb} · baixando {pct}%")), badge(&format!("{pct}%"), Tone::Plain).into())
                } else if !m.erro.is_empty() {
                    (crate::i18n::formatted("{gb} · falhou: {}", format!("{gb} · falhou: {}", m.erro)), btn(crate::i18n::text("Tentar de novo"), Kind::Normal, true, None, Some(Msg::ModelGet(i))))
                } else if m.instalado {
                    (crate::i18n::formatted("{gb} no disco · baixado", format!("{gb} no disco · baixado")), btn(crate::i18n::text("Remover"), Kind::Normal, !m.ativo, None, if m.ativo { None } else { Some(Msg::ModelDel(i)) }))
                } else {
                    (crate::i18n::formatted("{gb} · não baixado", format!("{gb} · não baixado")), btn(crate::i18n::text("Baixar"), Kind::Normal, true, Some("plus"), Some(Msg::ModelGet(i))))
                };
                c = c.push(setting_el(title, &desc, right));
            }
            if !live {
                c = c.push(setting(crate::i18n::text("Baixar modelo"), crate::i18n::text("Do Hugging Face, formato GGUF."), btn(crate::i18n::text("Baixar"), Kind::Normal, true, Some("plus"), Some(toast("Procurar no Hugging Face (simulado)")))));
            }
        }
        Sec::Voz => {
            c = c.push(lede(if p.english { "Coming soon: voice arrives with phase F15. The settings below are saved for when it exists." } else { crate::i18n::text("Em breve: a voz chega com a fase F15. Os ajustes abaixo já ficam salvos para quando ela existir.") }));
            c = c.push(sh(crate::i18n::text("Ativação")));
            c = c.push(setting(crate::i18n::text("Palavra “Orion”"), crate::i18n::text("Escuta localmente. Traz a janela para o foco."), sw(p.wake, "wake")));
            c = c.push(setting(crate::i18n::text("Duas palmas"), crate::i18n::text("Também traz a janela para o foco. Em calibração no notebook."), sw(p.clap, "clap")));
            c = c.push(setting(crate::i18n::text("Testar a mini barra"), crate::i18n::text("Barra flutuante no topo da tela enquanto você fala."), btn(if app.voice { crate::i18n::text("Fechar") } else { crate::i18n::text("Mostrar") }, Kind::Normal, true, None, Some(Msg::Live))));
            c = c.push(sh(crate::i18n::text("Fala")));
            c = c.push(setting(crate::i18n::text("Resposta falada"), crate::i18n::text("Só quando o pedido foi por voz."), badge(crate::i18n::text("só por voz"), Tone::Plain)));
            c = c.push(setting(crate::i18n::text("Idioma"), "", sel_pop(["Português (Brasil)", "English (US)"][p.voice_lang].into(), Pop::SetVoiceLang, app, menus::simple_menu(&["Português (Brasil)", "English (US)"], p.voice_lang, "vlang"))));
            c = c.push(setting(crate::i18n::text("Estilo"), "", sel_pop(["Masculina, grave", "Masculina, clara"][p.voice_style].into(), Pop::SetVoiceStyle, app, menus::simple_menu(&["Masculina, grave", "Masculina, clara"], p.voice_style, "vstyle"))));
            c = c.push(setting(crate::i18n::text("Velocidade"), "", pick_seg(&["Normal", crate::i18n::text("Rápido")], p.voice_speed, "vspeed")));
        }
        Sec::Camera => {
            c = c.push(lede(if p.english { "Coming soon: the camera arrives with phase F18. Nothing is captured until then." } else { crate::i18n::text("Em breve: a câmera chega com a fase F18. Nada é capturado até lá.") }));
            c = c.push(sh(crate::i18n::text("Modos")));
            c = c.push(setting(crate::i18n::text("Foto sob pedido"), crate::i18n::text("“Olha isso”: tira uma foto e responde."), swa("cam", 0, p.cam[0])));
            c = c.push(setting(crate::i18n::text("Presença contínua"), crate::i18n::text("Local, sem guardar imagem. Ao sair: pausa a voz e bloqueia o Orion. Na volta: cumprimenta com o que aconteceu."), swa("cam", 1, p.cam[1])));
            c = c.push(lede(crate::i18n::text("Com a câmera ativa, aparece um ponto vermelho na barra de título e no ícone da bandeja.")));
            c = c.push(sh(crate::i18n::text("O que o Orion faz com a câmera")));
            c = c.push(setting(crate::i18n::text("“Onde eu deixei?”"), crate::i18n::text("Guarda só rótulos (objeto, lugar, hora), sem prazo. Poucos KB por ano."), swa("cam", 2, p.cam[2])));
            c = c.push(setting(crate::i18n::text("Reconhecer você"), crate::i18n::text("Rosto local do dono."), swa("cam", 3, p.cam[3])));
            c = c.push(setting(crate::i18n::text("Legenda ao vivo"), crate::i18n::text("Faixa flutuante na base da tela."), swa("cam", 4, p.cam[4])));
            c = c.push(sh(crate::i18n::text("Objetos lembrados")));
            let mut tags = row![].spacing(6);
            for (i, o) in p.objs.iter().enumerate() {
                tags = tags.push(container(row![tx(o.clone(), 12.5, t::FG2), button(icon("close", 14.0, t::FG3)).padding(0).on_press(Msg::ObjRemove(i)).style(|_, _| button::Style::default())].spacing(4).align_y(Vertical::Center)).height(Length::Fixed(26.0)).center_y(Length::Fixed(26.0)).padding(pad(0.0, 6.0, 0.0, 10.0)).style(surface(None, Some(t::LINE2), 999.0)));
            }
            c = c.push(container(tags.wrap().vertical_spacing(6)).padding(pad(12.0, 0.0, 12.0, 0.0)));
            c = c.push(row![container(text_input(crate::i18n::text("Adicionar objeto"), &app.new_obj).on_input(Msg::NewObj).on_submit(Msg::ObjAdd).size(13).font(t::UI).padding(pad(6.0, 10.0, 6.0, 10.0)).style(field_style(t::RAISE, t::FIELD_LINE, t::ACCENT, 8.0))).width(Length::Fixed(220.0)), btn(crate::i18n::text("Adicionar"), Kind::Normal, true, None, Some(Msg::ObjAdd))].spacing(8));
        }
        Sec::Atalhos => {
            c = c.push(sh(crate::i18n::text("Atalhos")));
            if !app.short_err.is_empty() {
                c = c.push(tx(app.short_err.clone(), 12.5, t::DANGER));
            }
            for a in crate::keys::ACTIONS.iter() {
                let editing = app.short_edit == Some(a.id);
                let shown = if editing { crate::i18n::text("Pressione o novo atalho…").to_string() } else { crate::keys::display(&crate::keys::effective(a.id)) };
                let mut ctl = row![kbd(&shown)].spacing(8).align_y(Vertical::Center);
                if editing {
                    ctl = ctl.push(btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(Msg::ShortcutCancel)));
                } else {
                    ctl = ctl.push(btn(crate::i18n::text("Alterar"), Kind::Normal, true, None, Some(Msg::ShortcutEdit(a.id))));
                    if crate::keys::custom(a.id) {
                        ctl = ctl.push(btn(crate::i18n::text("Restaurar"), Kind::Ghost, true, None, Some(Msg::ShortcutReset(a.id))));
                    }
                }
                c = c.push(setting(a.label, "", ctl.into()));
            }
            // Linhas fixas geradas da lista de reservados, para a tela nunca divergir das regras de conflito.
            for (_, label, shown) in crate::keys::RESERVED.iter().filter(|(_, _, s)| !s.is_empty()) {
                let shown = if *shown == "Ctrl Alt Espaço" { crate::keys::display("Ctrl+Alt+Space") } else { shown.to_string() };
                c = c.push(setting(label, crate::i18n::text("Fixo"), kbd(&shown)));
            }
            c = c.push(setting(crate::keys::EDITING_LABEL, crate::i18n::text("Fixo"), kbd(crate::keys::EDITING_SHOWN)));
            for (l, k) in [("Nova linha", "Shift Enter"), ("Comandos", "/"), ("Citar nota do vault", "@")] {
                c = c.push(setting(l, crate::i18n::text("Fixo"), kbd(k)));
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
        text_input(crate::i18n::text("Buscar"), &app.set_q).on_input(Msg::SetQ).size(13).font(t::UI).padding(pad(6.0, 10.0, 6.0, 32.0)).style(|th, st| {
            let mut s = field_style(t::HOVER, iced::Color::TRANSPARENT, t::LINE2, 8.0)(th, st);
            if matches!(st, text_input::Status::Focused { .. }) {
                s.background = t::BG.into();
            }
            s
        }),
        container(icon("search", 14.0, t::FG3)).padding(pad(8.0, 0.0, 0.0, 10.0)),
    ];
    nav = nav.push(container(search_in).padding(pad(0.0, 0.0, 8.0, 0.0)));
    let mut any = false;
    for (gi, (g, items)) in GROUPS.iter().enumerate() {
        let vis: Vec<&Entry> = items.iter().filter(|e| found.contains(&e.0)).collect();
        if vis.is_empty() {
            continue;
        }
        any = true;
        nav = nav.push(container(tx(crate::i18n::text(g), 11.5, t::FG3)).padding(pad(if gi == 0 { 6.0 } else { 16.0 }, 10.0, 4.0, 10.0)));
        for (s, ic, l, _) in vis {
            let cur = app.sec == *s;
            nav = nav.push(
                button(container(row![icon(ic, 16.0, if cur { t::FG } else { t::FG2 }), tx(crate::i18n::text(l), 13.5, if cur { t::FG } else { t::FG2 })].spacing(11).align_y(Vertical::Center)).center_y(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fixed(32.0))
                    .padding(pad(0.0, 10.0, 0.0, 10.0))
                    .on_press(Msg::SetSec(*s))
                    .style(move |_, st| button::Style { background: if cur { Some(Background::Color(t::SEL)) } else if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: t::R_CTRL.into(), ..Default::default() }, shadow: Default::default(), snap: true }),
            );
        }
    }
    if !any {
        nav = nav.push(container(tx(crate::i18n::text("Nada encontrado."), 13.0, t::FG3)).padding(10));
    }
    let nav_panel = container(scrollable(container(nav).padding(pad(12.0, 8.0, 12.0, 8.0))).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill))
        .width(if mobile { Length::Fill } else { Length::Fixed(236.0) })
        .height(Length::Fill)
        .style(|_| container::Style { background: Some(t::SIDE.into()), ..Default::default() });
    let label = all().find(|e| e.0 == app.sec).map(|e| e.2).unwrap_or("");
    let mut main = column![].spacing(0);
    if mobile {
        main = main.push(container(btn(crate::i18n::text("Configurações"), Kind::Ghost, false, Some("back"), Some(Msg::SetBack))).padding(pad(10.0, 0.0, 0.0, 0.0)));
    }
    main = main.push(container(txf(crate::i18n::text(label), 26.0, t::FG, t::SERIF).line_height(iced::widget::text::LineHeight::Relative(1.15))).padding(pad(if mobile { 10.0 } else { 30.0 }, 0.0, 2.0, 0.0)));
    main = main.push(section(app));
    let main_panel = scrollable(container(main).padding(pad(0.0, 44.0, 48.0, 44.0)).width(Length::Fill)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).width(Length::Fill).height(Length::Fill);
    let body: El = if mobile {
        if app.set_detail { main_panel.into() } else { nav_panel.into() }
    } else {
        row![nav_panel, vline_full(), main_panel].into()
    };
    let close = container(tip(ib("close", Some(Msg::CloseSettings), false), crate::i18n::text("Fechar configurações"))).padding(pad(10.0, 12.0, 0.0, 0.0)).width(Length::Fill).align_x(iced::alignment::Horizontal::Right);
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

/// Memória residente do próprio processo, em MB, atualizada no máximo a cada 2 s (a tela redesenha com frequência).
fn proc_mem_mb() -> u64 {
    use std::sync::Mutex;
    static CACHE: Mutex<Option<(std::time::Instant, u64)>> = Mutex::new(None);
    let mut g = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t0, v)) = *g {
        if t0.elapsed().as_secs() < 2 {
            return v;
        }
    }
    let mut sys = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(std::process::id());
    sys.refresh_processes_specifics(sysinfo::ProcessesToUpdate::Some(&[pid]), true, sysinfo::ProcessRefreshKind::new().with_memory());
    let mb = sys.process(pid).map(|p| p.memory() / (1024 * 1024)).unwrap_or(0);
    *g = Some((std::time::Instant::now(), mb));
    mb
}

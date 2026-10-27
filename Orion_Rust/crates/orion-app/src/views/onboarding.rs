//! Primeiro uso: assistente em etapas no desenho do instalador (céu à esquerda, painel à direita,
//! barra de etapas em segmentos, Voltar/Avançar). Textos curtos; o resto fica para Configurações.
use crate::{
    app::{App, Msg},
    onboarding::{Event, Field, Form, PROVIDERS, STEPS},
    theme as t,
    ui::*,
};
use iced::{
    alignment::Vertical,
    widget::{checkbox, column, container, row, scrollable, stack, text_input},
    Length,
};

const STARS: [&str; STEPS + 1] = ["Betelgeuse", "Bellatrix", "Alnitak", "Alnilam", "Mintaka", "Saiph", "Rigel"];
const OK_TEXT: &str = "Conexão verificada";

fn event(e: Event) -> Msg {
    Msg::Onboarding(e)
}
fn english(f: &Form) -> bool {
    f.language == "en"
}
fn tr<'a>(f: &Form, pt: &'a str, en: &'a str) -> &'a str {
    if english(f) {
        en
    } else {
        pt
    }
}
/// Campos ficam inertes enquanto algo roda: nada digitado se perde em silêncio.
fn idle(f: &Form) -> bool {
    !f.busy && f.testing.is_none()
}
fn field<'a>(f: &'a Form, key: Field, label: &'a str, hint: &'a str, secret: bool) -> El<'a> {
    let mut input = text_input(hint, f.get(key))
        .size(16)
        .font(t::UI)
        .padding(pad(13.0, 16.0, 13.0, 16.0))
        .secure(secret)
        .style(super::login::field_style(t::alpha(t::BG, 0.55), t::LINE, t::ACCENT, 10.0));
    if idle(f) {
        input = input.on_input(move |s| event(Event::Input(key, s)));
    }
    column![tx(label, 13.5, t::FG2), input].spacing(7).width(Length::Fill).into()
}
fn folder<'a>(f: &'a Form, key: Field, label: &'a str, hint: &'a str) -> El<'a> {
    row![
        field(f, key, label, hint, false),
        container(btn(tr(f, crate::i18n::text("Escolher"), "Choose"), Kind::Normal, false, Some("folder"), idle(f).then_some(event(Event::Choose(key)))))
            .padding(pad(27.0, 0.0, 0.0, 0.0))
    ]
    .spacing(10)
    .align_y(Vertical::Bottom)
    .into()
}
fn check<'a>(f: &'a Form, label: &'a str, value: bool, make: fn(bool) -> Event) -> El<'a> {
    let c = checkbox(value).label(label).size(18).text_size(15);
    if idle(f) {
        c.on_toggle(move |b| event(make(b))).into()
    } else {
        c.into()
    }
}
fn note<'a>(s: impl iced::widget::text::IntoFragment<'a>) -> El<'a> {
    tx(s, 13.5, t::FG3).into()
}
fn chip<'a>(label: &str, on: bool, msg: Option<Msg>) -> El<'a> {
    btn(label, if on { Kind::Primary } else { Kind::Normal }, true, None, msg)
}
fn head<'a>(title: &'a str, sub: &'a str) -> El<'a> {
    column![txf(title, 32.0, t::FG, t::SERIF), tx(sub, 15.0, t::FG2)].spacing(6).into()
}

fn step_account<'a>(f: &'a Form) -> El<'a> {
    column![
        head(tr(f, crate::i18n::text("Sua conta"), "Your account"), tr(f, crate::i18n::text("Só neste computador."), "Only on this computer.")),
        sp_h(8.0),
        field(f, Field::User, tr(f, crate::i18n::text("Usuário"), "Username"), "seu_usuario", false),
        field(f, Field::Name, tr(f, crate::i18n::text("Como devo chamar você?"), "What should I call you?"), tr(f, crate::i18n::text("Seu nome"), "Your name"), false),
        check(f, tr(f, crate::i18n::text("Iniciar com o Windows (minimizado na bandeja)"), "Start with Windows (minimized to tray)"), f.autostart, Event::Autostart),
        column![
            tx(tr(f, crate::i18n::text("Idioma"), "Language"), 12.0, t::FG2),
            row![
                chip(crate::i18n::text("Português"), !english(f), idle(f).then_some(event(Event::Input(Field::Language, "pt-BR".into())))),
                chip("English", english(f), idle(f).then_some(event(Event::Input(Field::Language, "en".into()))))
            ]
            .spacing(8)
        ]
        .spacing(6)
    ]
    .spacing(16)
    .into()
}

fn step_security<'a>(f: &'a Form) -> El<'a> {
    let hello: El = if f.hello_available {
        column![
            check(f, tr(f, crate::i18n::text("Entrar com o Windows Hello"), "Sign in with Windows Hello"), f.hello, Event::Hello),
            note(tr(
                f,
                crate::i18n::text("A senha fica guardada no cofre do Windows e é liberada após o Hello."),
                "Your password is kept in the Windows vault and released after Hello."
            ))
        ]
        .spacing(4)
        .into()
    } else {
        note(tr(f, crate::i18n::text("Windows Hello indisponível neste computador."), "Windows Hello is unavailable on this computer."))
    };
    column![
        head(tr(f, crate::i18n::text("Segurança"), "Security"), tr(f, crate::i18n::text("Uma senha abre sua conta e seus dados."), "One password opens your account and data.")),
        sp_h(4.0),
        row![
            field(f, Field::Password, tr(f, crate::i18n::text("Senha"), "Password"), tr(f, crate::i18n::text("12 caracteres ou mais"), "12 characters or more"), true),
            field(f, Field::Confirmation, tr(f, crate::i18n::text("Confirmar"), "Confirm"), tr(f, crate::i18n::text("Repita a senha"), "Repeat the password"), true)
        ]
        .spacing(14),
        column![
            check(f, tr(f, crate::i18n::text("Cifrar meu banco com esta senha"), "Encrypt my database with this password"), f.encrypted, Event::Encrypt),
            if f.cipher_available {
                tx("", 1.0, t::FG3)
            } else {
                tx(tr(f, crate::i18n::text("Esta versão não inclui cifra; desmarque para continuar."), "This build has no encryption; uncheck to continue."), 12.0, t::SIGNAL)
            }
        ]
        .spacing(2),
        hello,
        hline(),
        folder(
            f,
            Field::Recovery,
            tr(f, crate::i18n::text("Cópia do código de recuperação (opcional)"), "Recovery code copy (optional)"),
            tr(f, crate::i18n::text("Um pen drive, por exemplo"), "A USB drive, for example")
        ),
        note(tr(
            f,
            crate::i18n::text("O código aparece na tela ao final. Guarde-o fora da pasta de dados."),
            "The code is shown at the end. Keep it outside the data folder."
        ))
    ]
    .spacing(14)
    .into()
}

fn step_notes<'a>(f: &'a Form) -> El<'a> {
    column![
        head(tr(f, crate::i18n::text("Notas e projetos"), "Notes and projects"), tr(f, crate::i18n::text("Tudo opcional."), "All optional.")),
        sp_h(8.0),
        folder(f, Field::Vault, tr(f, crate::i18n::text("Vault do Obsidian"), "Obsidian vault"), tr(f, crate::i18n::text("Pasta do vault"), "Vault folder")),
        folder(f, Field::Projects, tr(f, crate::i18n::text("Pasta dos projetos"), "Projects folder"), tr(f, crate::i18n::text("Seus repositórios"), "Your repositories")),
        field(f, Field::Secret("ORION_GITHUB_TOKEN"), tr(f, crate::i18n::text("Token do GitHub"), "GitHub token"), tr(f, crate::i18n::text("Salvo no cofre do sistema"), "Saved in the system vault"), true),
        btn(tr(f, crate::i18n::text("Criar token ↗"), "Create token ↗"), Kind::Ghost, true, None, Some(event(Event::Link("https://github.com/settings/tokens"))))
    ]
    .spacing(14)
    .into()
}

fn step_keys<'a>(f: &'a Form) -> El<'a> {
    let mut list = column![].spacing(16);
    for &(id, name, key, url) in PROVIDERS {
        let status = f.tests.get(key);
        let color = if status.is_some_and(|s| s == OK_TEXT) { t::OK } else { t::SIGNAL };
        let mut input = text_input(tr(f, crate::i18n::text("Chave de API"), "API key"), f.get(Field::Secret(key)))
            .size(15)
            .font(t::UI)
            .padding(pad(11.0, 14.0, 11.0, 14.0))
            .secure(true)
            .style(super::login::field_style(t::alpha(t::BG, 0.55), t::LINE, t::ACCENT, 9.0));
        if idle(f) {
            input = input.on_input(move |s| event(Event::Input(Field::Secret(key), s)));
        }
        list = list.push(
            column![
                row![
                    container(tx(name, 15.0, t::FG)).width(Length::Fixed(150.0)),
                    input,
                    btn(tr(f, crate::i18n::text("Obter ↗"), "Get ↗"), Kind::Ghost, false, None, Some(event(Event::Link(url)))),
                    btn(
                        if f.testing == Some(key) { tr(f, crate::i18n::text("Testando…"), "Testing…") } else { tr(f, crate::i18n::text("Testar"), "Test") },
                        Kind::Normal,
                        false,
                        None,
                        (idle(f) && !f.get(Field::Secret(key)).is_empty()).then_some(event(Event::Test(id, key)))
                    )
                ]
                .spacing(8)
                .align_y(Vertical::Center),
                match status {
                    Some(s) => container(tx(s.as_str(), 12.5, color)).padding(pad(0.0, 0.0, 0.0, 158.0)),
                    None => container(sp_h(0.0)),
                }
            ]
            .spacing(2),
        );
    }
    column![
        head(tr(f, crate::i18n::text("Chaves de IA"), "AI keys"), tr(f, crate::i18n::text("Só as que você já usa. O resto fica para depois."), "Only the ones you already use. The rest can wait.")),
        sp_h(4.0),
        list,
        note(tr(f, crate::i18n::text("Cada teste consulta apenas o provedor escolhido."), "Each test contacts only the chosen provider."))
    ]
    .spacing(12)
    .into()
}

fn step_links<'a>(f: &'a Form) -> El<'a> {
    let telegram_key = "ORION_TELEGRAM_TOKEN";
    let drive: El = {
        let label = if f.drive_connected {
            tr(f, crate::i18n::text("Drive conectado"), "Drive connected")
        } else if f.testing == Some("drive") {
            tr(f, crate::i18n::text("Aguardando o Google…"), "Waiting for Google…")
        } else {
            tr(f, crate::i18n::text("Conectar Google Drive"), "Connect Google Drive")
        };
        let action: El = if f.testing == Some("drive") {
            btn(tr(f, crate::i18n::text("Cancelar"), "Cancel"), Kind::Ghost, false, None, Some(event(Event::CancelDrive)))
        } else if f.drive_connected {
            btn(tr(f, crate::i18n::text("Só cópia local"), "Local copies only"), Kind::Ghost, false, None, Some(event(Event::DisconnectDrive)))
        } else {
            btn(
                label,
                Kind::Normal,
                false,
                None,
                (idle(f) && !f.drive_client.is_empty()).then_some(event(Event::ConnectDrive)),
            )
        };
        column![
            row![
                txf("Google Drive", 14.0, t::FG, t::UI_MED),
                grow(),
                if f.testing == Some("drive") { tx(label, 12.0, t::FG3).into() } else { sp_w(0.0) },
                action
            ]
            .spacing(10)
            .align_y(Vertical::Center),
            if f.drive_connected || f.testing == Some("drive") {
                note(tr(f, crate::i18n::text("Autorize no navegador; a espera termina em 3 minutos."), "Authorize in your browser; waiting ends in 3 minutes."))
            } else {
                column![
                    row![
                        field(f, Field::DriveClient, tr(f, crate::i18n::text("ID do cliente OAuth"), "OAuth client ID"), "…apps.googleusercontent.com", false),
                        field(f, Field::DriveSecret, tr(f, crate::i18n::text("Segredo"), "Secret"), "", true)
                    ]
                    .spacing(12),
                    btn(tr(f, crate::i18n::text("Configurar OAuth ↗"), "Set up OAuth ↗"), Kind::Ghost, true, None, Some(event(Event::Link("https://console.cloud.google.com/apis/credentials"))))
                ]
                .spacing(8)
                .into()
            },
            match f.tests.get("drive") {
                Some(s) if s != OK_TEXT => tx(s.as_str(), 12.0, t::SIGNAL),
                _ => tx("", 1.0, t::FG3),
            }
        ]
        .spacing(8)
        .into()
    };
    column![
        head(tr(f, crate::i18n::text("Telegram e backup"), "Telegram and backup"), tr(f, crate::i18n::text("Tudo opcional."), "All optional.")),
        sp_h(2.0),
        row![
            field(f, Field::Secret(telegram_key), tr(f, crate::i18n::text("Token do bot"), "Bot token"), "BotFather", true),
            field(f, Field::TelegramUsers, tr(f, crate::i18n::text("Seu ID"), "Your ID"), "123456789", false),
            container(btn(
                tr(f, crate::i18n::text("Testar"), "Test"),
                Kind::Normal,
                false,
                None,
                (idle(f) && !f.get(Field::Secret(telegram_key)).is_empty()).then_some(event(Event::Test("telegram", telegram_key)))
            ))
            .padding(pad(27.0, 0.0, 0.0, 0.0))
        ]
        .spacing(10)
        .align_y(Vertical::Bottom),
        check(f, tr(f, crate::i18n::text("Pedir código do Telegram ao entrar"), "Ask for a Telegram code at sign-in"), f.telegram_2fa, Event::Telegram2fa),
        match f.tests.get(telegram_key) {
            Some(s) => tx(s.as_str(), 12.0, if s == OK_TEXT { t::OK } else { t::SIGNAL }),
            None => tx("", 1.0, t::FG3),
        },
        hline(),
        folder(f, Field::Backup, tr(f, crate::i18n::text("Pasta das cópias locais"), "Local backup folder"), tr(f, crate::i18n::text("Cifradas com sua senha · últimas 7"), "Encrypted with your password · last 7")),
        drive
    ]
    .spacing(12)
    .into()
}

fn step_imports<'a>(f: &'a Form) -> El<'a> {
    let kind = |id: &'static str, label: &'static str| chip(label, f.import_kind == id, idle(f).then_some(event(Event::Input(Field::ImportKind, id.into()))));
    let mut col = column![
        head(tr(f, crate::i18n::text("Ferramentas e importação"), "Tools and import"), tr(f, crate::i18n::text("Você escolhe uma vez; pode mudar depois nas Configurações."), "Choose once; change it later in Settings.")),
        check(f, tr(f, crate::i18n::text("Autorizar todas as ferramentas sem confirmação a cada ação"), "Authorize all tools without confirming each action"), f.authorize_tools, Event::AuthorizeTools),
        note(tr(f, crate::i18n::text("Permite abrir apps, executar comandos, ler arquivos sensíveis, alterar ou apagar arquivos e usar a web, inclusive após ler conteúdo externo. Vale neste computador até você desligar em Configurações → Ferramentas."), "Allows opening apps, running commands, reading sensitive files, changing or deleting files and using the web, including after reading external content. Applies on this computer until you turn it off in Settings → Tools.")),
        note(tr(f, crate::i18n::text("Não ativa voz, câmera, captura contínua nem integrações sem configuração. Pânico e registros de ações continuam ativos."), "Does not turn on voice, camera, continuous capture or unconfigured integrations. Panic and action logs stay active.")),
        hline(),
        head(tr(f, crate::i18n::text("Importar"), "Import"), tr(f, crate::i18n::text("Traga o que já existe. Nada é executado nem ativado."), "Bring what already exists. Nothing runs or turns on.")),
        sp_h(4.0),
        row![kind("history", "Histórico"), kind("mcp", "MCP"), kind("plugin", "Plugin ZIP"), kind("skills", "Skills")].spacing(8),
    ]
    .spacing(12);
    if f.import_kind == "history" {
        let src = |n: &'static str| chip(n, f.import_source == n, idle(f).then_some(event(Event::Input(Field::ImportSource, n.into()))));
        col = col.push(row![src("Claude"), src("Codex"), src("ChatGPT")].spacing(8));
        col = col.push(field(f, Field::ImportProject, tr(f, crate::i18n::text("Projeto existente no vault"), "Existing project in the vault"), "Orion", false));
    }
    col = col.push(folder(f, Field::ImportPath, tr(f, crate::i18n::text("Arquivo ou pasta"), "File or folder"), tr(f, crate::i18n::text("Escolha o que importar"), "Choose what to import")));
    for (i, item) in f.detected.iter().enumerate() {
        col = col.push(btn(
            &format!("{} · {}", item["source"].as_str().unwrap_or(""), item["kind"].as_str().unwrap_or("")),
            Kind::Ghost,
            true,
            None,
            idle(f).then_some(event(Event::UseFound(i))),
        ));
    }
    col = col.push(btn(
        if f.testing == Some("import") { tr(f, crate::i18n::text("Conferindo…"), "Checking…") } else { tr(f, crate::i18n::text("Conferir e adicionar"), "Review and add") },
        Kind::Normal,
        false,
        None,
        (idle(f) && !f.import_path.is_empty()).then_some(event(Event::PreviewImport)),
    ));
    for (i, item) in f.imports.iter().enumerate() {
        col = col.push(
            container(
                row![
                    column![
                        tx(format!("{} · {} {}", item["kind"].as_str().unwrap_or(""), item["count"], tr(f, crate::i18n::text("itens"), "items")), 13.0, t::FG),
                        tx(item["description"].as_str().unwrap_or(""), 11.0, t::FG3)
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    btn(tr(f, crate::i18n::text("Remover"), "Remove"), Kind::Ghost, true, None, (!f.busy).then_some(event(Event::RemoveImport(i))))
                ]
                .spacing(8)
                .align_y(Vertical::Center),
            )
            .padding(12)
            .width(Length::Fill)
            .style(surface(Some(t::alpha(t::BG, 0.55)), Some(t::LINE), 10.0)),
        );
    }
    match f.tests.get("import") {
        Some(s) if s != OK_TEXT => col = col.push(tx(s.as_str(), 12.0, t::SIGNAL)),
        _ => {}
    }
    col.into()
}

/// Depois do envio: o código de recuperação aparece uma única vez, só em memória.
fn step_done<'a>(f: &'a Form) -> El<'a> {
    let code = f.code.as_deref().unwrap_or("");
    column![
        head(tr(f, crate::i18n::text("Tudo pronto"), "All set"), tr(f, crate::i18n::text("Guarde seu código de recuperação."), "Keep your recovery code.")),
        sp_h(10.0),
        container(txf(code.to_string(), 20.0, t::FG, t::MONO))
            .padding(pad(18.0, 20.0, 18.0, 20.0))
            .width(Length::Fill)
            .style(surface(Some(t::alpha(t::BG, 0.6)), Some(t::LINE2), 12.0)),
        note(tr(
            f,
            crate::i18n::text("Ele abre seus dados se você esquecer a senha. Anote ou guarde fora deste computador — não será mostrado de novo."),
            "It opens your data if you forget the password. Write it down or keep it off this computer — it won't be shown again."
        )),
        if f.warnings > 0 {
            tx(tr(f, crate::i18n::text("Algumas chaves não foram salvas; cadastre depois em Configurações."), "Some keys were not saved; add them later in Settings."), 12.0, t::SIGNAL)
        } else {
            tx("", 1.0, t::FG3)
        }
    ]
    .spacing(12)
    .into()
}

pub fn view(app: &App) -> El<'_> {
    let f = app.onboarding.as_ref().expect("primeiro uso");
    let done = f.step >= STEPS;
    let p = crate::anim::progress(Some(f.step_at), app.now, 420, app.p.reduced);

    let stage: El = match f.step {
        0 => step_account(f),
        1 => step_security(f),
        2 => step_notes(f),
        3 => step_keys(f),
        4 => step_links(f),
        5 => step_imports(f),
        _ => step_done(f),
    };

    let mut bar = row![].spacing(6);
    for i in 0..STEPS {
        let color = if i < f.step { t::ACCENT } else if i == f.step { t::SIGNAL } else { t::LINE };
        bar = bar.push(
            container(sp_h(3.0))
                .width(Length::Fill)
                .height(Length::Fixed(3.0))
                .style(move |_| container::Style { background: Some(color.into()), border: iced::Border { radius: 2.0.into(), ..Default::default() }, ..Default::default() }),
        );
    }

    let error: El = if f.error.is_empty() {
        sp_h(0.0)
    } else {
        container(tx(crate::i18n::status(&f.error), 12.0, t::DANGER))
            .padding(pad(8.0, 12.0, 8.0, 12.0))
            .width(Length::Fill)
            .style(surface(Some(t::DANGER_SOFT), Some(t::DANGER), 9.0))
            .into()
    };

    let last = f.step + 1 == STEPS;
    let next: El = if done {
        btn(tr(f, crate::i18n::text("Começar a usar"), "Start using Orion"), Kind::Primary, false, Some("chevR"), Some(event(Event::Finish)))
    } else if last {
        btn(
            if f.busy { tr(f, crate::i18n::text("Preparando…"), "Preparing…") } else { tr(f, crate::i18n::text("Concluir"), "Finish") },
            Kind::Primary,
            false,
            Some("chevR"),
            idle(f).then_some(event(Event::Save)),
        )
    } else {
        btn(tr(f, crate::i18n::text("Avançar"), "Next"), Kind::Primary, false, Some("chevR"), idle(f).then_some(event(Event::Next)))
    };
    let caption = if done {
        tr(f, crate::i18n::text("Seu Orion está configurado"), "Your Orion is set up")
    } else if matches!(f.step, 2 | 3 | 4 | 5) {
        tr(f, crate::i18n::text("Pode pular — ajuste depois em Configurações"), "You can skip — adjust later in Settings")
    } else {
        ""
    };
    let footer = column![
        error,
        hline(),
        sp_h(10.0),
        row![
            txf(caption, 11.0, t::FG3, t::MONO),
            grow(),
            if f.step > 0 && !done { btn(tr(f, crate::i18n::text("Voltar"), "Back"), Kind::Ghost, false, None, idle(f).then_some(event(Event::Back))) } else { sp_w(0.0) },
            next
        ]
        .spacing(8)
        .align_y(Vertical::Center)
    ]
    .spacing(10);

    let pane = container(
        column![
            bar,
            sp_h(18.0 + 10.0 * (1.0 - p)),
            scrollable(container(stage).padding(pad(0.0, 8.0, 0.0, 0.0)).width(Length::Fill))
                .direction(scrollable::Direction::Vertical(thin_bar()))
                .style(thin_scroll)
                .height(Length::Fill),
            footer
        ]
        .height(Length::Fill),
    )
    .padding(pad(36.0, 56.0, 28.0, 56.0))
    .width(Length::Fixed(760.0))
    .height(Length::Fill)
    .style(|_| container::Style { background: Some(t::SIDE.into()), ..Default::default() });

    let counter = if done { crate::i18n::text("PRONTO").to_string() } else { crate::i18n::formatted("PASSO {} DE {}", format!("PASSO {} DE {}", f.step + 1, STEPS)) };
    let star = STARS[f.step.min(STEPS)];
    let sky_side = iced::widget::image(f.sky_img.clone()).content_fit(iced::ContentFit::Fill).width(Length::Fill).height(Length::Fill);
    let side_text = container(
        column![
            row![belt_fg(false), grow()],
            grow(),
            row![
                txf("Orion", 28.0, t::FG, t::UI_MED),
                grow(),
                column![txf(counter, 12.0, t::FG3, t::MONO), txf(star.to_uppercase(), 12.0, t::SIGNAL, t::MONO)]
                    .spacing(3)
                    .align_x(iced::alignment::Horizontal::Right)
            ]
            .align_y(Vertical::Bottom)
        ]
        .height(Length::Fill),
    )
    .padding(pad(28.0, 32.0, 26.0, 32.0))
    .width(Length::Fill)
    .height(Length::Fill);
    let side = container(stack![sky_side, side_text])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(surface(Some(t::alpha(t::BG, 0.9)), None, 0.0));

    // Janela inteira, como o instalador, sem moldura nem tela dentro de tela.
    container(row![side, vline_fill(), pane])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(surface(Some(t::BG), None, 0.0))
        .into()
}

fn vline_fill<'a>() -> El<'a> {
    container(sp_w(1.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }).into()
}

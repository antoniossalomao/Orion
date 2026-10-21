//! Primeiro uso: tela única rolável, céu, fontes e componentes Grafite do app.
use crate::{
    app::{App, Msg},
    onboarding::{Event, Field, Form, PROVIDERS},
    sky::Sky,
    theme as t,
    ui::*,
};
use iced::{
    widget::{canvas, checkbox, column, container, row, scrollable, stack, text_input},
    Length,
};

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
fn field<'a>(f: &'a Form, key: Field, label: &'a str, hint: &'a str, secret: bool) -> El<'a> {
    let mut input = text_input(hint, f.get(key))
        .size(14)
        .font(t::UI)
        .padding(12)
        .secure(secret)
        .style(super::login::field_style(t::SIDE, t::LINE, t::ACCENT, 10.0));
    if !f.busy {
        input = input.on_input(move |s| event(Event::Input(key, s)));
    }
    column![tx(label, 12.0, t::FG2), input]
        .spacing(6)
        .width(Length::Fill)
        .into()
}
fn folder<'a>(f: &'a Form, key: Field, label: &'a str, hint: &'a str) -> El<'a> {
    row![
        field(f, key, label, hint, false),
        container(btn(
            tr(f, "Escolher", "Choose"),
            Kind::Normal,
            false,
            Some("folder"),
            (!f.busy).then_some(event(Event::Choose(key)))
        ))
        .padding(pad(24.0, 0.0, 0.0, 0.0))
    ]
    .spacing(10)
    .align_y(iced::alignment::Vertical::Bottom)
    .into()
}
fn card<'a>(number: &str, title: &str, description: &str, body: El<'a>) -> El<'a> {
    container(
        column![
            row![
                txf(number.to_string(), 11.0, t::ACCENT, t::MONO),
                txf(title.to_string(), 18.0, t::FG, t::UI_MED)
            ]
            .spacing(12),
            tx(description.to_string(), 12.0, t::FG3),
            sp_h(6.0),
            body
        ]
        .spacing(10),
    )
    .padding(24)
    .width(Length::Fill)
    .style(surface(Some(t::alpha(t::BG, 0.95)), Some(t::LINE), 16.0))
    .into()
}
fn check<'a>(f: &'a Form, label: &'a str, value: bool, make: fn(bool) -> Event) -> El<'a> {
    let c = checkbox(value).label(label).size(16).text_size(13);
    if f.busy {
        c.into()
    } else {
        c.on_toggle(move |b| event(make(b))).into()
    }
}

pub fn view(app: &App) -> El<'_> {
    let f = app.onboarding.as_ref().expect("primeiro uso");
    let identity = column![
        row![
            field(f, Field::User, tr(f, "Usuário", "Username"), "antoniosalomao", false),
            field(
                f,
                Field::Name,
                tr(f, "Como devo chamar você?", "What should I call you?"),
                tr(f, "Seu nome", "Your name"),
                false
            )
        ]
        .spacing(16),
        row![
            field(
                f,
                Field::Password,
                tr(f, "Senha", "Password"),
                tr(f, "Pelo menos 12 caracteres", "At least 12 characters"),
                true
            ),
            field(
                f,
                Field::Confirmation,
                tr(f, "Confirmar senha", "Confirm password"),
                tr(f, "Repita a senha", "Repeat your password"),
                true
            )
        ]
        .spacing(16),
        row![
            tx(tr(f, "Idioma", "Language"), 12.0, t::FG2),
            grow(),
            btn(
                "Português",
                if !english(f) { Kind::Primary } else { Kind::Normal },
                true,
                None,
                (!f.busy).then_some(event(Event::Input(Field::Language, "pt-BR".into())))
            ),
            btn(
                "English",
                if english(f) { Kind::Primary } else { Kind::Normal },
                true,
                None,
                (!f.busy).then_some(event(Event::Input(Field::Language, "en".into())))
            )
        ]
        .spacing(8)
        .align_y(iced::alignment::Vertical::Center),
        check(
            f,
            tr(f, "Cifrar meu banco com esta senha", "Encrypt my database with this password"),
            f.encrypted,
            Event::Encrypt
        ),
        tx(
            if f.cipher_available {
                tr(
                    f,
                    "A senha protege a conta e abre o banco cifrado.",
                    "Your password protects the account and unlocks the encrypted database.",
                )
            } else {
                tr(
                    f,
                    "Esta versão não inclui cifra. Instale o pacote completo ou desmarque a opção para continuar.",
                    "This version does not include encryption. Install the full package or explicitly uncheck it to continue.",
                )
            },
            12.0,
            if f.cipher_available { t::FG3 } else { t::SIGNAL }
        ),
        folder(
            f,
            Field::Recovery,
            tr(f, "Salvar chave de recuperação em", "Save recovery key to"),
            tr(f, "Escolha um arquivo novo, de preferência fora deste PC", "Choose a new file, preferably outside this PC")
        ),
        tx(
            tr(
                f,
                "O arquivo é criado uma única vez. Guarde uma cópia em local seguro.",
                "The file is created once. Keep a copy somewhere safe."
            ),
            12.0,
            t::FG3
        ),
        if f.hello_available {
            check(
                f,
                tr(
                    f,
                    "Usar Windows Hello para entrar e como reserva do 2FA",
                    "Use Windows Hello to sign in and as a 2FA fallback",
                ),
                f.hello,
                Event::Hello,
            )
        } else {
            tx(
                tr(
                    f,
                    "Windows Hello não está disponível neste computador. Você pode usar sua senha.",
                    "Windows Hello is not available on this computer. You can use your password.",
                ),
                12.0,
                t::FG3,
            )
            .into()
        }
    ]
    .spacing(14);
    let folders = column![
        folder(
            f,
            Field::Vault,
            tr(f, "Vault do Obsidian", "Obsidian vault"),
            tr(
                f,
                "Opcional — escolha uma pasta existente",
                "Optional — choose an existing folder"
            )
        ),
        folder(
            f,
            Field::Projects,
            tr(f, "Pasta dos projetos", "Projects folder"),
            tr(
                f,
                "Opcional — seus repositórios",
                "Optional — your repositories"
            )
        ),
        field(
            f,
            Field::Secret("ORION_GITHUB_TOKEN"),
            tr(f, "Token do GitHub", "GitHub token"),
            tr(
                f,
                "Opcional — salvo no cofre do sistema",
                "Optional — saved in the system vault"
            ),
            true
        ),
        btn(
            tr(f, "Criar token no GitHub ↗", "Create a GitHub token ↗"),
            Kind::Ghost,
            true,
            None,
            Some(event(Event::Link("https://github.com/settings/tokens")))
        )
    ]
    .spacing(14);
    let mut providers = column![tx(
        tr(
            f,
            "Preencha só as chaves que você já usa. Você pode adicionar as outras depois. Os testes consultam o provedor escolhido.",
            "Add only the keys you already use. You can add others later. Tests contact the selected provider."
        ),
        12.0,
        t::FG3
    )]
    .spacing(14);
    for &(id, name, key, url) in PROVIDERS {
        let controls = row![
            field(
                f,
                Field::Secret(key),
                name,
                tr(f, "Opcional — chave de API", "Optional — API key"),
                true
            ),
            container(
                row![
                    btn(
                        tr(f, "Obter ↗", "Get key ↗"),
                        Kind::Ghost,
                        true,
                        None,
                        Some(event(Event::Link(url)))
                    ),
                    btn(
                        if f.testing == Some(key) {
                            tr(f, "Testando…", "Testing…")
                        } else {
                            tr(f, "Testar", "Test")
                        },
                        Kind::Normal,
                        true,
                        None,
                        (!f.busy && f.testing.is_none() && !f.get(Field::Secret(key)).is_empty())
                            .then_some(event(Event::Test(id, key)))
                    )
                ]
                .spacing(6)
            )
            .padding(pad(26.0, 0.0, 0.0, 0.0))
        ]
        .spacing(12);
        providers = providers.push(controls);
        if let Some(status) = f.tests.get(key) {
            providers = providers.push(tx(
                status,
                12.0,
                if status == "Conexão verificada" {
                    t::OK
                } else {
                    t::SIGNAL
                },
            ));
        }
    }
    providers = providers.push(tx(
        tr(
            f,
            "As políticas de retenção e uso para treino variam por provedor. Consulte os termos antes de enviar seus dados.",
            "Retention and training policies vary by provider. Review their terms before sending your data.",
        ),
        12.0,
        t::FG3,
    ));
    let telegram_key = "ORION_TELEGRAM_TOKEN";
    let mut telegram = column![
        field(
            f,
            Field::Secret(telegram_key),
            tr(f, "Token do bot", "Bot token"),
            tr(
                f,
                "Opcional — obtido no BotFather",
                "Optional — from BotFather"
            ),
            true
        ),
        field(
            f,
            Field::TelegramUsers,
            tr(f, "Seu ID do Telegram", "Your Telegram ID"),
            tr(f, "IDs separados por vírgula", "IDs separated by commas"),
            false
        ),
        check(
            f,
            tr(
                f,
                "Pedir código do Telegram ao entrar",
                "Ask for a Telegram code when signing in"
            ),
            f.telegram_2fa,
            Event::Telegram2fa
        ),
        row![
            btn(
                "BotFather ↗",
                Kind::Ghost,
                true,
                None,
                Some(event(Event::Link("https://t.me/BotFather")))
            ),
            btn(
                tr(f, "Testar bot", "Test bot"),
                Kind::Normal,
                true,
                None,
                (!f.busy && f.testing.is_none() && !f.get(Field::Secret(telegram_key)).is_empty())
                    .then_some(event(Event::Test("telegram", telegram_key)))
            )
        ]
        .spacing(8)
    ]
    .spacing(14);
    if let Some(status) = f.tests.get(telegram_key) {
        telegram = telegram.push(tx(status, 12.0, t::FG2));
    }
    let mut backup = column![
        folder(
            f,
            Field::Backup,
            tr(f, "Pasta das cópias locais", "Local backup folder"),
            tr(f, "Opcional — pasta de backup", "Optional — backup folder")
        ),
        tx(
            tr(
                f,
                "Banco e perfil cifrados com sua senha. Últimas 7 cópias, sem tokens nem credenciais de login.",
                "Database and profile encrypted with your password. Last 7 copies, without tokens or sign-in credentials."
            ),
            12.0,
            t::FG3
        ),
        hline(),
        txf("Google Drive", 15.0, t::FG, t::UI_MED),
        tx(
            tr(
                f,
                "Opcional. Autorize o Orion no navegador para guardar também suas cópias no Drive.",
                "Optional. Authorize Orion in your browser to keep copies in Drive too."
            ),
            12.0,
            t::FG3
        ),
        field(
            f,
            Field::DriveClient,
            tr(f, "ID OAuth do Google (aplicativo para computador)", "Google OAuth client ID (Desktop app)"),
            "…apps.googleusercontent.com",
            false
        ),
        field(
            f,
            Field::DriveSecret,
            tr(f, "Segredo do cliente OAuth", "OAuth client secret"),
            tr(f, "Obtido com o ID no Google Cloud", "From Google Cloud, along with the ID"),
            true
        ),
        row![
            btn(
                tr(f, "Configurar OAuth ↗", "Set up OAuth ↗"),
                Kind::Ghost,
                true,
                None,
                Some(event(Event::Link("https://console.cloud.google.com/apis/credentials")))
            ),
            grow(),
            btn(
                if f.testing == Some("drive") {
                    tr(f, "Aguardando o Google…", "Waiting for Google…")
                } else if f.drive_connected {
                    tr(f, "Drive conectado", "Drive connected")
                } else {
                    tr(f, "Conectar Google Drive", "Connect Google Drive")
                },
                Kind::Normal,
                false,
                None,
                (!f.busy && f.testing.is_none() && !f.drive_client.is_empty() && !f.drive_connected).then_some(event(Event::ConnectDrive))
            )
        ]
        .spacing(8)
    ]
    .spacing(14);
    if f.drive_connected {
        backup = backup.push(btn(
            tr(f, "Usar só cópia local", "Use local copies only"),
            Kind::Ghost,
            true,
            None,
            Some(event(Event::DisconnectDrive)),
        ));
    }
    if let Some(status) = f.tests.get("drive") {
        backup = backup.push(tx(status, 12.0, t::FG2));
    }
    let mut imports = column![
        tx(
            tr(
                f,
                "Escolha exports JSON/JSONL, configurações MCP em JSON ou plugins ZIP. Nada é executado ou ativado automaticamente.",
                "Choose JSON/JSONL exports, MCP JSON settings, or ZIP plugins. Nothing is executed or enabled automatically."
            ),
            12.0,
            t::FG3
        ),
        row![
            btn("Histórico", Kind::Normal, true, None, Some(event(Event::Input(Field::ImportKind, "history".into())))),
            btn("MCP", Kind::Normal, true, None, Some(event(Event::Input(Field::ImportKind, "mcp".into())))),
            btn(
                "Skills / plugin ZIP",
                Kind::Normal,
                true,
                None,
                Some(event(Event::Input(Field::ImportKind, "plugin".into())))
            )
        ]
        .spacing(8),
        tx(format!("{}: {}", tr(f, "Tipo selecionado", "Selected type"), match f.import_kind.as_str() {
            "history" => tr(f, "Histórico", "History"),
            "mcp" => "MCP",
            "skills" => tr(f, "Pasta de skills", "Skills folder"),
            _ => "Skills / plugin ZIP",
        }), 12.0, t::FG2),
        folder(
            f,
            Field::ImportPath,
            tr(f, "Arquivo para importar", "File to import"),
            tr(f, "Opcional — escolha o arquivo exportado", "Optional — choose an exported file")
        )
    ]
    .spacing(14);
    imports = imports.push(btn(
        tr(f, "Pasta de skills", "Skills folder"),
        Kind::Normal,
        true,
        None,
        Some(event(Event::Input(Field::ImportKind, "skills".into()))),
    ));
    for (i, item) in f.detected.iter().enumerate() {
        imports = imports.push(btn(
            &format!(
                "{} · {} · {}",
                item["source"].as_str().unwrap_or(""),
                item["kind"].as_str().unwrap_or(""),
                item["path"].as_str().unwrap_or("")
            ),
            Kind::Ghost,
            true,
            None,
            (!f.busy && f.testing.is_none()).then_some(event(Event::UseFound(i))),
        ));
    }
    if f.import_kind == "history" {
        imports = imports.push(
            row![
                btn(
                    "Claude",
                    Kind::Normal,
                    true,
                    None,
                    Some(event(Event::Input(Field::ImportSource, "Claude".into())))
                ),
                btn(
                    "Codex",
                    Kind::Normal,
                    true,
                    None,
                    Some(event(Event::Input(Field::ImportSource, "Codex".into())))
                ),
                btn(
                    "ChatGPT",
                    Kind::Normal,
                    true,
                    None,
                    Some(event(Event::Input(Field::ImportSource, "ChatGPT".into())))
                )
            ]
            .spacing(8),
        );
        imports = imports.push(tx(
            format!("Fonte selecionada: {}", f.import_source),
            12.0,
            t::FG2,
        ));
        imports = imports.push(field(
            f,
            Field::ImportProject,
            tr(f, "Projeto existente no vault", "Existing project in vault"),
            "Orion",
            false,
        ));
    }
    imports = imports.push(btn(
        tr(f, "Conferir e adicionar", "Review and add"),
        Kind::Normal,
        false,
        None,
        (!f.busy && f.testing.is_none() && !f.import_path.is_empty())
            .then_some(event(Event::PreviewImport)),
    ));
    for (i, item) in f.imports.iter().enumerate() {
        imports = imports.push(
            container(
                column![
                    row![
                        tx(
                            format!(
                                "{} · {} itens",
                                item["kind"].as_str().unwrap_or(""),
                                item["count"]
                            ),
                            13.0,
                            t::FG
                        ),
                        grow(),
                        btn(
                            tr(f, "Remover", "Remove"),
                            Kind::Ghost,
                            true,
                            None,
                            (!f.busy).then_some(event(Event::RemoveImport(i)))
                        )
                    ]
                    .spacing(8),
                    tx(item["path"].as_str().unwrap_or(""), 12.0, t::FG3),
                    tx(item["description"].as_str().unwrap_or(""), 12.0, t::FG2)
                ]
                .spacing(8),
            )
            .padding(12)
            .style(surface(Some(t::SIDE), Some(t::LINE), 10.0)),
        );
    }
    if let Some(status) = f.tests.get("import") {
        if status != "Conexão verificada" {
            imports = imports.push(tx(status, 12.0, t::SIGNAL));
        }
    }
    let header = row![
        column![
            belt_fg(false),
            txf(
                tr(f, "Seu Orion começa aqui", "Your Orion starts here"),
                32.0,
                t::FG,
                t::SERIF
            ),
            tx(
                tr(
                    f,
                    "Uma conta, suas notas e as conexões que você escolher.",
                    "One account, your notes, and the connections you choose."
                ),
                14.0,
                t::FG2
            )
        ]
        .spacing(12),
        grow(),
        btn(
            "×",
            Kind::Ghost,
            false,
            None,
            Some(Msg::Acao(crate::tray::Acao::Exit))
        )
    ];
    let mut body = column![
        header,
        card(
            "01",
            tr(f, "Sua conta", "Your account"),
            tr(
                f,
                "Só neste computador. Você decide o que conectar.",
                "On this computer. You choose what to connect."
            ),
            identity.into()
        ),
        card(
            "02",
            tr(f, "Notas e projetos", "Notes and projects"),
            tr(
                f,
                "Confirme a pasta detectada ou escolha outra.",
                "Confirm the detected folder or choose another."
            ),
            folders.into()
        ),
        card(
            "03",
            tr(f, "Chaves de API", "API keys"),
            tr(
                f,
                "Opcionais. Salvas no cofre do sistema.",
                "Optional. Saved in the system vault."
            ),
            providers.into()
        ),
        card(
            "04",
            "Telegram",
            tr(
                f,
                "Acesso só para os IDs que você autorizar.",
                "Access only for the IDs you authorize."
            ),
            telegram.into()
        ),
        card(
            "05",
            tr(f, "Backup", "Backup"),
            tr(
                f,
                "Escolha onde manter suas cópias locais.",
                "Choose where to keep local copies."
            ),
            backup.into()
        ),
        card(
            "06",
            tr(f, "Importações", "Imports"),
            tr(
                f,
                "Suas conexões existentes, com revisão.",
                "Your existing connections, with review."
            ),
            imports.into()
        )
    ]
    .spacing(20)
    .max_width(860)
    .width(Length::Fill);
    if !f.error.is_empty() {
        body = body.push(
            container(tx(&f.error, 13.0, t::DANGER))
                .padding(16)
                .width(Length::Fill)
                .style(surface(Some(t::DANGER_SOFT), Some(t::DANGER), 10.0)),
        );
    }
    body = body.push(
        row![
            tx(
                tr(
                    f,
                    "Você pode ajustar tudo depois em Configurações.",
                    "You can adjust everything later in Settings."
                ),
                12.0,
                t::FG3
            ),
            grow(),
            btn(
                if f.busy {
                    tr(f, "Preparando seu Orion…", "Preparing your Orion…")
                } else {
                    tr(f, "Começar a usar", "Start using Orion")
                },
                Kind::Primary,
                false,
                Some("chevR"),
                (!f.busy && f.testing.is_none()).then_some(event(Event::Save))
            )
        ]
        .spacing(16)
        .align_y(iced::alignment::Vertical::Center),
    );
    let content = scrollable(
        container(body)
            .padding(pad(40.0, 28.0, 56.0, 28.0))
            .center_x(Length::Fill),
    )
    .direction(scrollable::Direction::Vertical(thin_bar()))
    .style(thin_scroll)
    .height(Length::Fill);
    let sky = canvas(Sky {
        alpha: 0.75,
        constellation: true,
        belt: 0.0,
    })
    .width(Length::Fill)
    .height(Length::Fill);
    container(stack![sky, content])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(surface(Some(t::BG), None, 0.0))
        .into()
}

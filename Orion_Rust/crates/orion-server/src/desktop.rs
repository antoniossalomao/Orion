//! Controle Windows (REGRAS.md: toda rota da API exige login; pânico é corte; não perturbe só segura).
use crate::estado::Estado;
use iced::{
    widget::{button, column, container, pick_list, progress_bar, row, scrollable, text, text_input},
    window, Element, Subscription, Task,
};
use orion_core::config::Settings;
use orion_platform::{DesktopAction, NativeShell};
use serde_json::json;
use std::{sync::atomic::Ordering, time::Duration};

#[derive(Clone)]
enum Message {
    Tick,
    Position(window::Id, Option<iced::Size>),
    Window(Option<window::Id>),
    Hide(window::Id),
    User(String),
    Password(String),
    Code(String),
    Login,
    Logged(Result<LoginResult, String>),
    Logout,
    Input(String),
    Send,
    Answer(String),
    Open(DesktopAction),
    Resume,
    VoicePause,
    Provider(String),
    Limit(String),
    Reset(String),
    WindowLimit(String),
    SaveBudget,
    Routine(String),
    RunRoutine,
    Autostart,
}
#[derive(Clone)]
struct LoginResult {
    token: Option<String>,
    challenge: Option<String>,
}
// REGRAS.md: segredos nunca em log, audit ou processo filho. Debug nunca mostra senha/código/token.
impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Evento do controle {:?}", std::mem::discriminant(self))
    }
}
struct Desktop {
    state: Estado,
    shell: Option<NativeShell>,
    shell_attempted: bool,
    window: Option<window::Id>,
    token: Option<String>,
    challenge: Option<String>,
    user: String,
    password: String,
    code: String,
    input: String,
    output: String,
    status: String,
    busy: bool,
    panel: DesktopAction,
    provider: String,
    limit: String,
    reset: String,
    window_limit: String,
    routine: String,
    transcript: String,
}
impl Desktop {
    fn authenticated(&self) -> bool {
        self.state.auth.validate(self.token.as_deref()).is_some()
    }
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Window(id) => {
                self.window = id;
                if self.shell_attempted && self.shell.is_none() {
                    if let Some(id) = id {
                        return window::set_mode(id, window::Mode::Windowed);
                    }
                }
            }
            Message::Position(id, Some(size)) => return window::move_to(id, iced::Point::new(((size.width - 660.0) / 2.0).max(0.0), 16.0)),
            Message::Position(_, None) => {}
            Message::Tick => {
                if !self.shell_attempted {
                    self.shell_attempted = true;
                    match NativeShell::new() {
                        Ok(s) => self.shell = Some(s),
                        Err(e) => {
                            self.status = e.to_string();
                            if let Some(id) = self.window {
                                return window::set_mode(id, window::Mode::Windowed);
                            }
                        }
                    }
                }
                let mut actions = self.shell.as_ref().map(|s| s.poll()).unwrap_or_default();
                if self.state.desktop_wake.swap(false, Ordering::SeqCst) {
                    actions.push(DesktopAction::MiniBar)
                }
                if self.authenticated() {
                    if let Some(e) = &self.state.escuta {
                        self.transcript = e.transcript.lock().clone();
                    }
                }
                if self.token.is_some() && !self.authenticated() {
                    self.token = None;
                    self.output.clear();
                }
                return Task::batch(actions.into_iter().map(|a| Task::done(Message::Open(a))));
            }
            Message::Open(DesktopAction::Exit) => {
                self.state.auth.logout(self.token.as_deref());
                if let Some(e) = &self.state.escuta {
                    e.stop()
                }
                return iced::exit();
            }
            Message::Open(DesktopAction::Panic) => {
                self.status = match self.state.modos.entrar_panico("atalho_desktop") {
                    Ok(_) => "Pânico ligado. Entre na conta e clique em Retomar para voltar.".into(),
                    Err(e) => e.to_string(),
                };
            }
            Message::Open(panel) => {
                self.panel = panel;
                if let Some(id) = self.window {
                    return Task::batch([
                        window::set_mode(id, window::Mode::Windowed),
                        window::resize(
                            id,
                            iced::Size::new(
                                if panel == DesktopAction::MiniBar { 660.0 } else { 850.0 },
                                if panel == DesktopAction::MiniBar { 280.0 } else { 700.0 },
                            ),
                        ),
                        window::set_level(id, if panel == DesktopAction::MiniBar { window::Level::AlwaysOnTop } else { window::Level::Normal }),
                        window::gain_focus(id),
                        if panel == DesktopAction::MiniBar { window::monitor_size(id).map(move |size| Message::Position(id, size)) } else { Task::none() },
                    ]);
                }
            }
            Message::Hide(id) => {
                if self.shell.is_none() {
                    return Task::done(Message::Open(DesktopAction::Exit));
                }
                return window::set_mode(id, window::Mode::Hidden);
            }
            Message::User(v) => self.user = v,
            Message::Password(v) => self.password = v,
            Message::Code(v) => self.code = v,
            Message::Login => {
                if self.busy {
                    return Task::none();
                }
                self.busy = true;
                let url = format!("http://127.0.0.1:{}", self.state.settings.port);
                let challenge = self.challenge.clone();
                let body = if let Some(c) = &challenge {
                    json!({"challenge":c,"code":std::mem::take(&mut self.code)})
                } else {
                    json!({"usuario":self.user,"senha":std::mem::take(&mut self.password)})
                };
                return Task::perform(
                    async move {
                        let response = reqwest::Client::builder()
                            .no_proxy()
                            .build()
                            .map_err(|_| "Cliente local indisponível".to_string())?
                            .post(format!("{url}/auth/{}", if challenge.is_some() { "2fa" } else { "login" }))
                            .json(&body)
                            .timeout(Duration::from_secs(45))
                            .send()
                            .await
                            .map_err(|_| "Servidor indisponível".to_string())?;
                        let status = response.status();
                        let token = response
                            .headers()
                            .get("set-cookie")
                            .and_then(|h| h.to_str().ok())
                            .and_then(|s| s.split(';').next())
                            .and_then(|s| s.strip_prefix("orion_session="))
                            .map(str::to_string);
                        let value: serde_json::Value = response.json().await.map_err(|_| "Resposta inválida".to_string())?;
                        if !status.is_success() {
                            return Err(value["detail"].as_str().unwrap_or("Não foi possível entrar. Confira os dados e tente novamente.").to_string());
                        }
                        Ok(LoginResult { token, challenge: value["challenge"].as_str().map(str::to_string) })
                    },
                    Message::Logged,
                );
            }
            Message::Logged(result) => {
                self.busy = false;
                match result {
                    Ok(r) => {
                        self.token = r.token;
                        self.challenge = r.challenge;
                        self.status = if self.challenge.is_some() { "Digite o código recebido no Telegram" } else { "Conectado" }.into();
                    }
                    Err(e) => {
                        self.status = e;
                        self.challenge = None;
                    }
                }
            }
            Message::Logout => {
                self.state.auth.logout(self.token.as_deref());
                self.token = None;
                self.output.clear();
                self.challenge = None;
            }
            Message::Input(v) => self.input = v,
            Message::Answer(v) => {
                self.busy = false;
                self.output = v;
            }
            Message::Send if self.authenticated() && !self.busy => {
                if self.input.trim().is_empty() {
                    return Task::none();
                }
                let Some(agent) = self.state.agent.clone() else {
                    self.status = "Configure um provedor para conversar".into();
                    return Task::none();
                };
                let input = std::mem::take(&mut self.input);
                self.busy = true;
                return Task::perform(
                    async move {
                        use futures::StreamExt;
                        let mut stream = agent.run(orion_core::agent::TurnRequest { channel: "desktop".into(), text: input, ..Default::default() });
                        let mut output = String::new();
                        while let Some(event) = stream.next().await {
                            match event.kind {
                                "text" => output.push_str(event.data["text"].as_str().unwrap_or("")),
                                "approval" => output.push_str("\nAprovação pendente: revise o cartão no Telegram ou no painel do app."),
                                "error" => output.push_str(event.data["message"].as_str().unwrap_or("Falha")),
                                _ => {}
                            }
                        }
                        output
                    },
                    Message::Answer,
                );
            }
            Message::Resume if self.authenticated() => {
                self.status = match self.state.modos.sair_panico("botao_desktop") {
                    Ok(_) => {
                        if let Some(e) = &self.state.escuta {
                            e.set_suspended(false)
                        }
                        "Pânico desligado".into()
                    }
                    Err(e) => e.to_string(),
                };
            }
            Message::VoicePause if self.authenticated() => {
                if let Some(e) = &self.state.escuta {
                    e.pausar(!e.pausada())
                }
            }
            Message::Provider(v) => self.provider = v,
            Message::Limit(v) => self.limit = v,
            Message::Reset(v) => self.reset = v,
            Message::WindowLimit(v) => self.window_limit = v,
            Message::SaveBudget if self.authenticated() => {
                let result = (|| -> anyhow::Result<()> {
                    let reset = chrono::NaiveDateTime::parse_from_str(&self.reset, "%Y-%m-%d %H:%M")?.and_utc().timestamp();
                    let quota = orion_ai::budget::Quota {
                        weekly_calls: self.limit.parse()?,
                        reset_at: reset,
                        five_hour_calls: if self.window_limit.is_empty() { None } else { Some(self.window_limit.parse()?) },
                    };
                    let mut configs = self.state.budget.config();
                    configs.insert(self.provider.clone(), quota);
                    self.state.budget.configure(configs)
                })();
                self.status = match result {
                    Ok(()) => "Orçamento salvo".into(),
                    Err(e) => format!("Confira os limites e a data: {e}"),
                };
            }
            Message::Routine(v) => self.routine = v,
            Message::RunRoutine if self.authenticated() && !self.busy => {
                if let Some(executor) = self.state.executor_rotinas.clone() {
                    let name = self.routine.clone();
                    self.busy = true;
                    return Task::perform(
                        async move {
                            let r = executor.executar(&name, "desktop").await;
                            format!("Rotina: {} — {}", r.status, r.motivo)
                        },
                        Message::Answer,
                    );
                }
            }
            Message::Autostart if self.authenticated() => {
                let result = (|| -> anyhow::Result<()> {
                    let exe = std::env::current_exe()?;
                    let project = orion_core::config::project_root();
                    let home = orion_core::downloads::dirs_home();
                    let spec =
                        orion_core::autostart::render("windows", &exe, &project, &self.state.settings.data_dir, &home, false).map_err(anyhow::Error::msg)?;
                    if spec.arquivo.exists() {
                        std::fs::remove_file(spec.arquivo)?;
                        self.status = "Início automático desligado".into();
                    } else {
                        orion_core::autostart::instalar(&spec, None, false).map_err(anyhow::Error::msg)?;
                        self.status = "Início automático ligado".into();
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    self.status = e.to_string()
                }
            }
            _ => {}
        }
        Task::none()
    }
    fn view(&self) -> Element<'_, Message> {
        if !self.authenticated() {
            let mut form = column![text("Orion").size(28), text(&self.status)].spacing(12);
            if self.challenge.is_some() {
                form = form.push(text_input("Código do Telegram", &self.code).on_input(Message::Code).secure(true).on_submit(Message::Login));
            } else {
                form = form
                    .push(text_input("Usuário", &self.user).on_input(Message::User))
                    .push(text_input("Senha", &self.password).on_input(Message::Password).secure(true).on_submit(Message::Login));
            }
            return container(form.push(button(if self.busy { "Aguarde…" } else { "Entrar" }).on_press(Message::Login))).padding(20).into();
        }
        let mut body = column![
            row![
                text("Orion").size(26),
                button("Mini barra").on_press(Message::Open(DesktopAction::MiniBar)),
                button("Orçamento e rotinas").on_press(Message::Open(DesktopAction::Budget)),
                button("Sair da conta").on_press(Message::Logout)
            ]
            .spacing(10),
            text(&self.status)
        ]
        .spacing(12);
        if self.state.modos.panico() {
            body = body.push(button("Retomar após pânico").on_press(Message::Resume));
        }
        if self.panel == DesktopAction::Budget {
            if self.state.budget.summary().is_empty() {
                body = body.push(text("Configure as cotas abaixo para aplicar os tetos semanais."));
            }
            body = body.push(text("Estimativa local em chamadas · tetos: 95% semanal / 20% por tarefa").size(16));
            for q in self.state.budget.summary() {
                body = body
                    .push(text(format!(
                        "{} · {} / {} chamadas · {:.1}% · reset {}",
                        q["provider"].as_str().unwrap_or(""),
                        q["used"],
                        q["weekly_calls"],
                        q["percent"].as_f64().unwrap_or(0.0),
                        chrono::DateTime::from_timestamp(q["reset_at"].as_i64().unwrap_or(0), 0).map(|d| d.to_rfc3339()).unwrap_or_default()
                    )))
                    .push(progress_bar(0.0..=100.0, q["percent"].as_f64().unwrap_or(0.0) as f32));
            }
            let mut providers = vec!["cli:claude".to_string(), "cli:codex".into(), "cli:gemini".into()];
            if let Some(g) = &self.state.gateway {
                providers.extend(g.endpoints.iter().filter(|e| e.tier != "local").map(|e| e.provider()))
            }
            body = body
                .push(pick_list(providers, Some(self.provider.clone()), Message::Provider))
                .push(
                    row![
                        text_input("Cota semanal em chamadas", &self.limit).on_input(Message::Limit),
                        text_input("Próximo reset UTC: AAAA-MM-DD HH:MM", &self.reset).on_input(Message::Reset)
                    ]
                    .spacing(8),
                )
                .push(text_input("Limite em 5 h (opcional)", &self.window_limit).on_input(Message::WindowLimit))
                .push(button("Salvar orçamento").on_press(Message::SaveBudget));
            let names: Vec<String> = self.state.rotinas.listar().iter().map(|r| r.nome.clone()).collect();
            body = body
                .push(text("Rotinas concedidas"))
                .push(pick_list(names, Some(self.routine.clone()), Message::Routine))
                .push(button("Executar rotina selecionada").on_press(Message::RunRoutine))
                .push(button("Ligar / desligar início com Windows").on_press(Message::Autostart));
            if self.state.escuta.is_some() {
                body = body.push(button("Pausar / retomar escuta").on_press(Message::VoicePause));
            }
        }
        if !self.transcript.is_empty() {
            body = body.push(text(format!("Entendi: {}", self.transcript)));
        }
        body = body
            .push(
                row![
                    text_input("Fale com Orion…", &self.input).on_input(Message::Input).on_submit(Message::Send),
                    button(if self.busy { "Aguarde…" } else { "Enviar" }).on_press(Message::Send)
                ]
                .spacing(8),
            )
            .push(text(&self.output));
        container(scrollable(body)).padding(16).into()
    }
}
pub fn run(settings: Settings, background: bool) -> anyhow::Result<i32> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let state = runtime.block_on(crate::estado::montar(settings))?;
    let listener = runtime.block_on(tokio::net::TcpListener::bind((state.settings.host.as_str(), state.settings.port)))?;
    let shutdown_state = state.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let mut task = runtime.spawn(crate::serve_state(state.clone(), listener, async {
        let _ = shutdown_rx.await;
    }));
    let result = iced::application(
        move || {
            (
                Desktop {
                    state: state.clone(),
                    shell: None,
                    shell_attempted: false,
                    window: None,
                    token: None,
                    challenge: None,
                    user: "admin".into(),
                    password: String::new(),
                    code: String::new(),
                    input: String::new(),
                    output: String::new(),
                    status: "Entre na sua conta".into(),
                    busy: false,
                    panel: DesktopAction::MiniBar,
                    provider: "cli:codex".into(),
                    limit: "100".into(),
                    reset: (chrono::Utc::now() + chrono::Duration::days(7)).format("%Y-%m-%d %H:%M").to_string(),
                    window_limit: String::new(),
                    routine: String::new(),
                    transcript: String::new(),
                },
                window::oldest().map(Message::Window),
            )
        },
        Desktop::update,
        Desktop::view,
    )
    .title("Orion")
    .theme(iced::Theme::Dark)
    .window(window::Settings { size: iced::Size::new(660.0, 280.0), visible: !background, ..Default::default() })
    .exit_on_close_request(false)
    .subscription(|_| Subscription::batch([iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick), window::close_requests().map(Message::Hide)]))
    .run();
    if let Some(e) = &shutdown_state.escuta {
        e.stop()
    }
    let _ = shutdown_tx.send(());
    runtime.block_on(async {
        if tokio::time::timeout(Duration::from_secs(10), &mut task).await.is_err() {
            task.abort();
        }
    });
    result?;
    Ok(0)
}

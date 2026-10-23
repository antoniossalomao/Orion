//! Bandeja do Windows, atalhos globais e janela escondida: o app continua rodando em segundo plano.
//! Fora do Windows não há bandeja (`Bandeja::nova` devolve `None` e fechar a janela encerra o app).
pub use orion_platform::DesktopAction as Acao;

pub struct Bandeja {
    #[cfg(windows)]
    shell: orion_platform::NativeShell,
}

impl Bandeja {
    pub fn nova() -> Option<Self> {
        #[cfg(windows)]
        {
            orion_platform::NativeShell::new_with_labels([crate::i18n::text("Abrir mini barra"), crate::i18n::text("Captura rápida (Ctrl+Alt+O)"), crate::i18n::text("Isso? (Ctrl+Alt+I)"), crate::i18n::text("Orçamento e rotinas"), crate::i18n::text("Pânico — cortar tudo"), crate::i18n::text("Sair do Orion")]).ok().map(|shell| Bandeja { shell })
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub fn consultar(&self) -> Vec<Acao> {
        #[cfg(windows)]
        {
            self.shell.poll()
        }
        #[cfg(not(windows))]
        {
            vec![]
        }
    }
}

/// Consulta eventos fora da thread da interface e só a acorda quando há uma ação.
#[derive(Clone)]
pub struct Monitor(std::sync::Arc<MonitorInner>);
struct MonitorInner {
    api: Option<crate::net::Api>,
    poll: std::sync::Arc<dyn Fn() -> Vec<Acao> + Send + Sync>,
}
impl std::hash::Hash for Monitor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) { std::hash::Hash::hash(&std::sync::Arc::as_ptr(&self.0), state); }
}
impl Monitor {
    pub fn new(tray: Option<&Bandeja>, api: Option<crate::net::Api>) -> Self {
        #[cfg(windows)] let poll = tray.map(|t| t.shell.poller()).unwrap_or_else(|| std::sync::Arc::new(Vec::new));
        #[cfg(not(windows))] let poll = { let _ = tray; std::sync::Arc::new(Vec::new) as std::sync::Arc<dyn Fn() -> Vec<Acao> + Send + Sync> };
        Self(std::sync::Arc::new(MonitorInner { api, poll }))
    }
    pub fn subscription(&self) -> iced::Subscription<crate::app::Msg> {
        iced::Subscription::run_with(self.clone(), |monitor| {
            let monitor = monitor.clone();
            async_stream::stream! {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
                loop {
                    interval.tick().await;
                    for action in (monitor.0.poll)() { yield crate::app::Msg::Acao(action); }
                    match monitor.0.api.as_ref().and_then(|api| api.acordar()) {
                        Some(crate::net::Acordar::Voz) => yield crate::app::Msg::Acao(Acao::MiniBar),
                        Some(crate::net::Acordar::Janela) => yield crate::app::Msg::Mostrar,
                        None => {}
                    }
                }
            }
        })
    }
}

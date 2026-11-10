//! Recuperação da conta pela janela (F02.10): código de recuperação → nova senha → novo código.
//! O código usado deixa de valer e o novo aparece uma única vez, só em memória; dá para imprimi-lo.
//! Nada disto vai para log (REGRAS.md: segredos nunca em log).
use crate::app::{App, Msg};
use crate::theme as t;
use crate::ui::*;
use iced::alignment::Horizontal;
use iced::widget::{column, container, text_input};
use iced::{Length, Task};
use zeroize::{Zeroize, Zeroizing};

/// Mesmos limites da senha do núcleo (`orion_core::auth::SENHA_MIN` e `SENHA_MAX`).
const SENHA_MIN: usize = 12;
const SENHA_MAX: usize = 256;

#[derive(Default)]
pub struct Form {
    pub code: String,
    pub pw: String,
    pub pw2: String,
    pub busy: bool,
    pub err: String,
    /// Novo código, mostrado uma vez depois de recuperar.
    pub new_code: Option<String>,
}

impl Drop for Form {
    fn drop(&mut self) {
        apagar_paginas();
        self.code.zeroize();
        self.pw.zeroize();
        self.pw2.zeroize();
        if let Some(c) = self.new_code.as_mut() {
            c.zeroize();
        }
    }
}

#[derive(Debug, Clone)]
pub enum Event {
    Open,
    Close,
    Code(String),
    Pw(String),
    Pw2(String),
    Submit,
    Done(Result<String, String>),
    Print,
    /// Imprime o código mostrado no fim do primeiro uso (o código não viaja na mensagem).
    PrintOnboarding,
}

impl App {
    pub fn update_recov(&mut self, ev: Event) -> Task<Msg> {
        match ev {
            Event::Open => {
                self.recov = Some(Form::default());
                self.gate_err.clear();
            }
            // Fechar com a recuperação em andamento perderia o código novo: espera terminar.
            Event::Close => {
                if self.recov.as_ref().is_some_and(|f| !f.busy) {
                    self.recov = None;
                }
            }
            Event::Code(s) => {
                if let Some(f) = self.recov.as_mut() {
                    f.code = s.chars().take(80).collect();
                    f.err.clear();
                }
            }
            Event::Pw(s) => {
                if let Some(f) = self.recov.as_mut() {
                    f.pw = s.chars().take(SENHA_MAX + 1).collect();
                    f.err.clear();
                }
            }
            Event::Pw2(s) => {
                if let Some(f) = self.recov.as_mut() {
                    f.pw2 = s.chars().take(SENHA_MAX + 1).collect();
                    f.err.clear();
                }
            }
            Event::Submit => {
                let Some(api) = self.api.clone() else { return Task::none() };
                let Some(f) = self.recov.as_mut() else { return Task::none() };
                if f.busy || f.new_code.is_some() {
                    return Task::none();
                }
                if f.code.trim().is_empty() {
                    f.err = "Digite o código de recuperação.".into();
                    return Task::none();
                }
                if !(SENHA_MIN..=SENHA_MAX).contains(&f.pw.chars().count()) {
                    f.err = "A nova senha precisa ter entre 12 e 256 caracteres.".into();
                    return Task::none();
                }
                if f.pw != f.pw2 {
                    f.err = "As senhas não coincidem.".into();
                    return Task::none();
                }
                f.busy = true;
                f.err.clear();
                let code = std::mem::take(&mut f.code);
                let pw = std::mem::take(&mut f.pw);
                f.pw2.zeroize();
                return Task::perform(async move { api.recuperar_conta(code, pw).await }, |r| Msg::Recov(Event::Done(r)));
            }
            Event::Done(r) => {
                if let Some(f) = self.recov.as_mut() {
                    f.busy = false;
                    match r {
                        Ok(novo) => f.new_code = Some(novo),
                        Err(e) => f.err = e,
                    }
                }
            }
            Event::PrintOnboarding => {
                let code = self.onboarding.as_ref().and_then(|f| f.code.clone()).map(Zeroizing::new);
                // no primeiro uso vale o idioma escolhido ali, que ainda pode não ser o da interface
                let english = self.onboarding.as_ref().map(|f| f.language == "en");
                if let Some(code) = code {
                    if let Err(e) = print(&code, english) {
                        self.toast(e);
                    }
                }
            }
            Event::Print => {
                let code = self.recov.as_ref().and_then(|f| f.new_code.clone()).map(Zeroizing::new);
                if let Some(code) = code {
                    if let Err(e) = print(&code, None) {
                        self.toast(e);
                    }
                }
            }
        }
        Task::none()
    }
}

const PREFIXO: &str = "orion-recuperacao-";
/// Páginas de impressão criadas nesta execução, para apagar assim que o formulário fecha.
static PAGINAS: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Apaga as páginas de impressão desta execução (o navegador já carregou a página; o arquivo não
/// precisa ficar).
pub fn apagar_paginas() {
    if let Ok(mut g) = PAGINAS.lock() {
        for p in g.drain(..) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Na abertura do app: apaga páginas que sobraram de execuções que terminaram antes da limpeza
/// (o código impresso nunca fica em disco além disso).
pub fn limpar_restos() {
    let Ok(entradas) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for e in entradas.flatten() {
        let nome = e.file_name();
        let nome = nome.to_string_lossy();
        if nome.starts_with(PREFIXO) && nome.ends_with(".html") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Imprime o código: grava uma página mínima na pasta temporária (só o dono lê), abre no navegador
/// padrão, que já chama a janela de impressão, e apaga o arquivo em 60 segundos, quando o formulário
/// fecha ou na próxima abertura do app, o que vier primeiro.
pub fn print(code: &str, english: Option<bool>) -> Result<(), String> {
    if code.is_empty() || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Código inválido para impressão.".into());
    }
    let path = std::env::temp_dir().join(format!("{PREFIXO}{}.html", std::process::id()));
    // textos pelo catálogo Fluent, no idioma pedido (o da interface, ou o escolhido no primeiro uso)
    let anterior = crate::i18n::english();
    if let Some(en) = english {
        crate::i18n::set_english(en);
    }
    let (en, titulo, instrucoes) = (
        crate::i18n::english(),
        crate::i18n::text("Código de recuperação do Orion"),
        crate::i18n::text("Guarde esta folha fora do computador. Ele abre seus dados se você esquecer a senha e só vale uma vez: depois de usado, o Orion gera outro."),
    );
    crate::i18n::set_english(anterior);
    let html = format!(
        "<!doctype html><html lang={}><meta charset=utf-8><title>{titulo}</title>\
         <style>body{{font:16px system-ui,sans-serif;margin:48px}}code{{font:24px ui-monospace,Consolas,monospace;letter-spacing:.06em}}</style>\
         <h1>{titulo}</h1>\
         <p>{instrucoes}</p>\
         <p><code>{code}</code></p>\
         <script>addEventListener('load',()=>setTimeout(()=>print(),300))</script></html>",
        if en { "en" } else { "pt-BR" }
    );
    {
        use std::io::Write;
        let mut opt = std::fs::OpenOptions::new();
        opt.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opt.mode(0o600);
        }
        let mut file = opt.open(&path).map_err(|_| "Não consegui preparar a página de impressão.".to_string())?;
        file.write_all(html.as_bytes()).map_err(|_| "Não consegui preparar a página de impressão.".to_string())?;
    }
    #[cfg(windows)]
    let aberto = std::process::Command::new("cmd").args(["/C", "start", ""]).arg(&path).spawn();
    #[cfg(not(windows))]
    let aberto = std::process::Command::new("xdg-open").arg(&path).spawn();
    if let Ok(mut g) = PAGINAS.lock() {
        g.push(path.clone());
    }
    let apagar = path.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        let _ = std::fs::remove_file(apagar);
    });
    aberto.map(|_| ()).map_err(|_| {
        let _ = std::fs::remove_file(&path);
        "Não consegui abrir o navegador para imprimir.".to_string()
    })
}

fn campo<'a>(rotulo: &'static str, v: &str, f: fn(String) -> Msg, secure: bool, submit: bool) -> El<'a> {
    let mut entrada = text_input("", v)
        .on_input(f)
        .secure(secure)
        .size(14)
        .font(t::UI)
        .padding(pad(t::SP_3, t::SP_3, t::SP_3, t::SP_3))
        .style(crate::views::login::field_style(t::alpha(t::BG, 0.55), t::FIELD_LINE, t::ACCENT, 10.0));
    if submit {
        entrada = entrada.on_submit(Msg::Recov(Event::Submit));
    }
    column![tx(crate::i18n::text(rotulo), 12.0, t::FG2), entrada].spacing(t::SP_15).into()
}

/// Conteúdo do cartão de entrada enquanto a recuperação está aberta.
pub fn view<'a>(f: &'a Form, cab: El<'a>) -> El<'a> {
    if let Some(novo) = f.new_code.as_deref() {
        return column![
            cab,
            sp_h(t::SP_25),
            tx(crate::i18n::text("Acesso recuperado"), 14.0, t::FG),
            container(txf(novo.to_string(), 17.0, t::FG, t::MONO))
                .padding(pad(t::SP_4, t::SP_35, t::SP_4, t::SP_35))
                .width(Length::Fill)
                .style(surface(Some(t::alpha(t::BG, 0.6)), Some(t::LINE2), 12.0)),
            tx(crate::i18n::text("O código antigo deixou de valer. Guarde este fora do computador: ele não será mostrado de novo."), 12.0, t::FG3),
            btn(crate::i18n::text("Imprimir código"), Kind::Normal, false, None, Some(Msg::Recov(Event::Print))),
            btn(crate::i18n::text("Entrar com a nova senha"), Kind::Primary, false, None, Some(Msg::Recov(Event::Close))),
        ]
        .spacing(t::SP_25)
        .align_x(Horizontal::Center)
        .width(Length::Fill)
        .into();
    }
    let aviso = container(tx(if f.busy { crate::i18n::text("Recuperando…").to_string() } else { crate::i18n::status(&f.err) }, 12.0, if f.err.is_empty() { t::FG3 } else { t::DANGER })).height(Length::Fixed(16.0));
    column![
        cab,
        sp_h(t::SP_25),
        campo("Código de recuperação", &f.code, |s| Msg::Recov(Event::Code(s)), false, false),
        campo("Nova senha", &f.pw, |s| Msg::Recov(Event::Pw(s)), true, false),
        campo("Confirme a nova senha", &f.pw2, |s| Msg::Recov(Event::Pw2(s)), true, true),
        aviso,
        btn(crate::i18n::text("Recuperar acesso"), Kind::Primary, false, None, if f.busy { None } else { Some(Msg::Recov(Event::Submit)) }),
        btn(crate::i18n::text("Voltar ao login"), Kind::Ghost, true, None, if f.busy { None } else { Some(Msg::Recov(Event::Close)) }),
    ]
    .spacing(t::SP_25)
    .align_x(Horizontal::Center)
    .width(Length::Fill)
    .into()
}

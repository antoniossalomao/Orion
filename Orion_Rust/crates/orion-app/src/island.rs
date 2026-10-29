//! Estado e fluxo do Face ID na janela: a Dynamic Island no topo (desenho em `views::island`) e o
//! que acontece em cada resultado da leitura (REGRAS.md: o Face ID é opt-in, só vale na janela
//! nativa e a senha continua sempre como alternativa).
//!
//! A leitura em si roda fora da thread da interface: `views::login::varredura` chama o núcleo a cada
//! `INTERVALO` e cada análise roda num `spawn_blocking`. Aqui só mudam fases e marcas de tempo.

use crate::app::{App, Msg};
use crate::net::{FaceEvent, FaceFailure, FaceHint, FacePreview};
use iced::Task;
use std::time::{Duration, Instant};

/// Para que serve a leitura em andamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Login,
    Autonomous,
    Panic,
}

impl Purpose {
    pub fn key(self) -> &'static str {
        match self {
            Purpose::Login => "login",
            Purpose::Autonomous => "autonomo",
            Purpose::Panic => "panico",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Pílula discreta: Face ID disponível.
    Standby,
    /// Câmera ligada, lendo o rosto.
    Reading,
    /// Cadastrando o rosto (Configurações).
    Enrolling,
    Success,
    Error,
}

/// Duração da transição de tamanho entre fases.
pub const MORPH_MS: u64 = 520;
/// Tempo do brilho de confirmação antes de seguir para a Home (ou fechar o pedido).
pub const SUCCESS_MS: u64 = 760;
/// Quanto o aviso de erro fica aberto antes de voltar a pílula.
pub const ERROR_MS: u64 = 3200;

#[derive(Debug, Clone)]
pub struct Island {
    pub phase: Phase,
    /// Início da fase atual (animação de forma e de brilho).
    pub since: Instant,
    /// Tamanho de onde a forma parte (o tamanho visível no instante da troca de fase).
    pub from: (f32, f32),
    pub purpose: Purpose,
    pub session: Option<String>,
    /// Geração da leitura: eventos de leituras antigas são ignorados.
    pub generation: u64,
    pub hint: FaceHint,
    pub failure: Option<FaceFailure>,
    /// Senha digitada para cadastrar o Face ID (None = pedido fechado).
    pub enroll_password: Option<String>,
    /// Só nas capturas de tela (`snap`): finge Face ID ativo sem núcleo.
    pub demo: bool,
    /// Último quadro do HUD da câmera e a imagem pronta para desenhar.
    pub preview: Option<(FacePreview, iced::widget::image::Handle)>,
    /// A fase atual veio de um cadastro (sucesso/erro do cadastro também mostram o HUD).
    pub enrolled_last: bool,
}

impl Island {
    pub fn new(now: Instant) -> Self {
        Self { phase: Phase::Standby, since: now, from: size(Phase::Standby), purpose: Purpose::Login, session: None, generation: 0, hint: FaceHint::Starting, failure: None, enroll_password: None, demo: false, preview: None, enrolled_last: false }
    }

    pub fn busy(&self) -> bool {
        matches!(self.phase, Phase::Reading | Phase::Enrolling)
    }

    /// Precisa de quadros de animação: lendo (anel girando) ou ainda dentro de uma transição/brilho.
    pub fn animating(&self, now: Instant) -> bool {
        self.busy() || now.saturating_duration_since(self.since) < Duration::from_millis(MORPH_MS.max(SUCCESS_MS) + 120)
    }

    /// Tamanho visível agora (interpolado entre `de` e o alvo da fase).
    pub fn current_size(&self, now: Instant, reduced: bool) -> (f32, f32) {
        let p = crate::anim::progress(Some(self.since), now, MORPH_MS, reduced);
        let (a, b) = (self.from, size(self.phase));
        (a.0 + (b.0 - a.0) * p, a.1 + (b.1 - a.1) * p)
    }

    /// O painel do HUD aparece no login e no cadastro (as confirmações de senha ficam só na ilha).
    pub fn shows_hud(&self) -> bool {
        self.phase != Phase::Standby && (self.purpose == Purpose::Login || self.phase == Phase::Enrolling || self.enrolled_last)
    }

    pub fn set_phase(&mut self, phase: Phase, now: Instant, reduced: bool) {
        if phase == Phase::Standby {
            self.preview = None;
        }
        self.enrolled_last = phase != Phase::Standby && (phase == Phase::Enrolling || (self.phase == Phase::Enrolling && self.enrolled_last));
        self.from = self.current_size(now, reduced);
        self.phase = phase;
        self.since = now;
    }
}

/// Alvo de tamanho de cada fase (largura, altura).
pub fn size(phase: Phase) -> (f32, f32) {
    match phase {
        Phase::Standby => (132.0, 32.0),
        Phase::Reading | Phase::Enrolling => (318.0, 64.0),
        Phase::Success => (296.0, 64.0),
        Phase::Error => (348.0, 72.0),
    }
}

#[derive(Debug, Clone)]
pub enum FaceMsg {
    /// Começa uma leitura (toque na pílula, tela de entrada ou pedido de senha).
    Start(Purpose),
    Started(u64, Result<String, FaceFailure>),
    Event(u64, FaceEvent),
    /// Fim do brilho de sucesso: segue para a Home ou conclui o pedido.
    Finish(u64),
    /// Fim do aviso de erro: volta à pílula.
    Collapse(u64),
    Cancel,
    // cadastro (Configurações › Câmera)
    EnrollAsk,
    EnrollPassword(String),
    EnrollClose,
    EnrollSubmit,
    Enrolled(Result<usize, String>),
    Disable,
    Disabled(Result<(), String>),
    /// Pulso de 250 ms do HUD: pede o último quadro ao núcleo.
    PreviewTick,
    Preview(u64, Option<FacePreview>),
}

/// Id do campo de senha que recebe o foco quando o rosto falha.
pub fn password_field(p: Purpose) -> &'static str {
    match p {
        Purpose::Login => "login-password",
        Purpose::Autonomous => "auto-password",
        Purpose::Panic => "panic-password",
    }
}

pub fn failure_text(f: &FaceFailure) -> &'static str {
    match f {
        FaceFailure::NoCamera => "Câmera indisponível",
        FaceFailure::LowLight => "Pouca luz para o Face ID",
        FaceFailure::NotRecognized => "Rosto não reconhecido",
        FaceFailure::Locked => "Face ID travado: use a senha",
        FaceFailure::Inactive => "Face ID não está ativado",
        FaceFailure::Other(_) => "Face ID indisponível agora",
    }
}

fn after(ms: u64, msg: Msg) -> Task<Msg> {
    Task::perform(async move { tokio::time::sleep(Duration::from_millis(ms)).await }, move |_| msg.clone())
}

impl App {
    pub fn face_id_enabled(&self) -> bool {
        self.island.demo || self.api.as_ref().is_some_and(|a| a.face_id_enabled())
    }

    /// A pílula aparece na tela de entrada com Face ID ativo e, já dentro, só durante uma leitura.
    pub fn island_visible(&self) -> bool {
        if self.onboarding.is_some() || self.loading.is_some() {
            return false;
        }
        if !self.logged {
            return self.face_id_enabled();
        }
        self.island.phase != Phase::Standby || crate::anim::active(Some(self.island.since), self.now, MORPH_MS)
    }

    fn cancel_scan(&mut self) {
        if let (Some(id), Some(api)) = (self.island.session.take(), self.api.as_ref()) {
            api.face_id_cancel(&id);
        }
        self.island.generation += 1;
    }

    pub fn update_face(&mut self, m: FaceMsg) -> Task<Msg> {
        let (now, reduced) = (Instant::now(), self.p.reduced);
        self.now = now;
        match m {
            FaceMsg::Start(purpose) => {
                let Some(api) = self.api.clone() else { return Task::none() };
                if self.island.busy() || !api.face_id_enabled() {
                    return Task::none();
                }
                if purpose == Purpose::Login && (self.logged || self.challenge.is_some()) {
                    return Task::none();
                }
                self.cancel_scan();
                self.island.purpose = purpose;
                self.island.hint = FaceHint::Starting;
                self.island.failure = None;
                self.island.set_phase(Phase::Reading, now, reduced);
                crate::views::login::face_scan(api, purpose, self.island.generation)
            }
            FaceMsg::Started(g, r) => {
                if g != self.island.generation {
                    // leitura cancelada enquanto a câmera abria: solta já
                    if let (Ok(id), Some(api)) = (r, self.api.as_ref()) {
                        api.face_id_cancel(&id);
                    }
                    return Task::none();
                }
                match r {
                    Ok(id) => {
                        self.island.session = Some(id);
                        Task::none()
                    }
                    Err(f) => self.face_failed(f),
                }
            }
            FaceMsg::Event(g, _) if g != self.island.generation => Task::none(),
            FaceMsg::Event(_, ev) => match ev {
                FaceEvent::Searching(d) => {
                    self.island.hint = d;
                    Task::none()
                }
                FaceEvent::LoggedIn => {
                    self.island.session = None;
                    self.island.set_phase(Phase::Success, now, reduced);
                    self.pw.clear();
                    after(SUCCESS_MS, Msg::Face(FaceMsg::Finish(self.island.generation)))
                }
                FaceEvent::Approved(ticket) => {
                    self.island.session = None;
                    self.island.set_phase(Phase::Success, now, reduced);
                    let g = self.island.generation;
                    let Some(api) = self.api.clone() else { return Task::none() };
                    let request = match self.island.purpose {
                        Purpose::Autonomous => {
                            self.auto_senha = None;
                            Task::perform(async move { api.post("/modo/autonomo", serde_json::json!({"ativo": true, "faceid": ticket})).await }, |r| Msg::L(crate::live::Lm::Modo(r)))
                        }
                        Purpose::Panic => {
                            self.resume_pw.clear();
                            Task::perform(async move { api.post("/modo/panico", serde_json::json!({"ativo": false, "faceid": ticket})).await }, |r| match r {
                                Ok(_) => Msg::L(crate::live::Lm::Acao(Ok("Retomado.".into()), true)),
                                Err(e) => Msg::L(crate::live::Lm::Acao(Err(e), true)),
                            })
                        }
                        Purpose::Login => Task::none(),
                    };
                    Task::batch([request, after(SUCCESS_MS, Msg::Face(FaceMsg::Finish(g)))])
                }
                FaceEvent::Failed(f) => {
                    self.island.session = None;
                    self.face_failed(f)
                }
            },
            FaceMsg::Finish(g) if g == self.island.generation && self.island.phase == Phase::Success => {
                self.island.set_phase(Phase::Standby, now, reduced);
                if self.island.purpose == Purpose::Login {
                    self.loading = Some("Abrindo o Orion…".into());
                    return Task::done(Msg::L(crate::live::Lm::Desbloqueado(Ok(()))));
                }
                Task::none()
            }
            FaceMsg::Collapse(g) if g == self.island.generation && self.island.phase == Phase::Error => {
                self.island.set_phase(Phase::Standby, now, reduced);
                Task::none()
            }
            FaceMsg::Finish(_) | FaceMsg::Collapse(_) => Task::none(),
            FaceMsg::Cancel => {
                self.cancel_scan();
                if self.island.phase != Phase::Standby {
                    self.island.set_phase(Phase::Standby, now, reduced);
                }
                iced::widget::operation::focus(password_field(self.island.purpose))
            }
            FaceMsg::EnrollAsk => {
                self.island.enroll_password = Some(String::new());
                iced::widget::operation::focus("faceid-password")
            }
            FaceMsg::EnrollPassword(s) => {
                self.island.enroll_password = Some(s);
                Task::none()
            }
            FaceMsg::EnrollClose => {
                self.island.enroll_password = None;
                Task::none()
            }
            FaceMsg::EnrollSubmit => {
                let Some(api) = self.api.clone() else { return Task::none() };
                let Some(password) = self.island.enroll_password.take().filter(|s| !s.is_empty()) else {
                    self.island.enroll_password = Some(String::new());
                    return Task::none();
                };
                if self.island.busy() {
                    return Task::none();
                }
                self.cancel_scan();
                self.island.failure = None;
                self.island.hint = FaceHint::Starting;
                self.island.set_phase(Phase::Enrolling, now, reduced);
                Task::perform(async move { api.face_id_enroll(password).await }, |r| Msg::Face(FaceMsg::Enrolled(r)))
            }
            FaceMsg::Enrolled(r) => match r {
                Ok(_) => {
                    self.island.purpose = Purpose::Autonomous;
                    self.island.set_phase(Phase::Success, now, reduced);
                    self.toast("Face ID ativado. Entrar e confirmar pedidos de senha agora aceitam o seu rosto; a senha continua valendo.");
                    after(SUCCESS_MS, Msg::Face(FaceMsg::Finish(self.island.generation)))
                }
                Err(e) => {
                    self.island.failure = Some(FaceFailure::Other(e.clone()));
                    self.island.set_phase(Phase::Error, now, reduced);
                    self.toast(&e);
                    after(ERROR_MS, Msg::Face(FaceMsg::Collapse(self.island.generation)))
                }
            },
            FaceMsg::Disable => {
                let Some(api) = self.api.clone() else { return Task::none() };
                self.cancel_scan();
                Task::perform(async move { api.face_id_disable().await }, |r| Msg::Face(FaceMsg::Disabled(r)))
            }
            FaceMsg::PreviewTick => {
                let Some(api) = self.api.clone() else { return Task::none() };
                if !self.island.shows_hud() || !self.island.busy() {
                    return Task::none();
                }
                let session = if self.island.phase == Phase::Enrolling { Some("enroll".to_string()) } else { self.island.session.clone() };
                let Some(session) = session else { return Task::none() };
                let g = self.island.generation;
                Task::perform(async move { api.face_id_preview(&session).await }, move |p| Msg::Face(FaceMsg::Preview(g, p)))
            }
            FaceMsg::Preview(g, Some(p)) if g == self.island.generation && self.island.busy() => {
                let handle = iced::widget::image::Handle::from_rgba(p.w, p.h, p.rgba.clone());
                self.island.preview = Some((p, handle));
                Task::none()
            }
            FaceMsg::Preview(..) => Task::none(),
            FaceMsg::Disabled(r) => {
                match r {
                    Ok(()) => self.toast("Face ID desligado. O modelo do rosto e a credencial foram apagados."),
                    Err(e) => self.toast(&e),
                }
                Task::none()
            }
        }
    }

    /// Falha: a ilha abre o aviso e o campo de senha recebe o foco (a senha mestre é o caminho).
    fn face_failed(&mut self, f: FaceFailure) -> Task<Msg> {
        let (now, reduced) = (Instant::now(), self.p.reduced);
        if let FaceFailure::Other(e) = &f {
            tracing::info!("Face ID: {e}");
        }
        self.island.failure = Some(f);
        self.island.set_phase(Phase::Error, now, reduced);
        let g = self.island.generation;
        let focus = match self.island.purpose {
            // pedido de senha do autônomo: garante que o campo existe
            Purpose::Autonomous if self.auto_senha.is_none() => {
                self.auto_senha = Some(String::new());
                iced::widget::operation::focus(password_field(Purpose::Autonomous))
            }
            p => iced::widget::operation::focus(password_field(p)),
        };
        Task::batch([focus, after(ERROR_MS, Msg::Face(FaceMsg::Collapse(g)))])
    }
}

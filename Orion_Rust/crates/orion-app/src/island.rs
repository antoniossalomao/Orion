//! Estado e fluxo do Face ID na janela: a Dynamic Island no topo (desenho em `views::island`) e o
//! que acontece em cada resultado da leitura (REGRAS.md: o Face ID é opt-in, só vale na janela
//! nativa e a senha continua sempre como alternativa).
//!
//! A leitura em si roda fora da thread da interface: `views::login::varredura` chama o núcleo a cada
//! `INTERVALO` e cada análise roda num `spawn_blocking`. Aqui só mudam fases e marcas de tempo.

use crate::app::{App, Msg};
use crate::net::{FaceDica, FaceEvento, FaceFalha};
use iced::Task;
use std::time::{Duration, Instant};

/// Para que serve a leitura em andamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proposito {
    Login,
    Autonomo,
    Panico,
}

impl Proposito {
    pub fn chave(self) -> &'static str {
        match self {
            Proposito::Login => "login",
            Proposito::Autonomo => "autonomo",
            Proposito::Panico => "panico",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fase {
    /// Pílula discreta: Face ID disponível.
    Standby,
    /// Câmera ligada, lendo o rosto.
    Lendo,
    /// Cadastrando o rosto (Configurações).
    Cadastro,
    Sucesso,
    Erro,
}

/// Duração da transição de tamanho entre fases.
pub const MORPH_MS: u64 = 520;
/// Tempo do brilho de confirmação antes de seguir para a Home (ou fechar o pedido).
pub const SUCESSO_MS: u64 = 760;
/// Quanto o aviso de erro fica aberto antes de voltar a pílula.
pub const ERRO_MS: u64 = 3200;

#[derive(Debug, Clone)]
pub struct Island {
    pub fase: Fase,
    /// Início da fase atual (animação de forma e de brilho).
    pub desde: Instant,
    /// Tamanho de onde a forma parte (o tamanho visível no instante da troca de fase).
    pub de: (f32, f32),
    pub proposito: Proposito,
    pub sessao: Option<String>,
    /// Geração da leitura: eventos de leituras antigas são ignorados.
    pub geracao: u64,
    pub dica: FaceDica,
    pub falha: Option<FaceFalha>,
    /// Senha digitada para cadastrar o Face ID (None = pedido fechado).
    pub senha_cadastro: Option<String>,
    /// Só nas capturas de tela (`snap`): finge Face ID ativo sem núcleo.
    pub demo: bool,
}

impl Island {
    pub fn new(now: Instant) -> Self {
        Self { fase: Fase::Standby, desde: now, de: tamanho(Fase::Standby), proposito: Proposito::Login, sessao: None, geracao: 0, dica: FaceDica::Iniciando, falha: None, senha_cadastro: None, demo: false }
    }

    pub fn ocupada(&self) -> bool {
        matches!(self.fase, Fase::Lendo | Fase::Cadastro)
    }

    /// Precisa de quadros de animação: lendo (anel girando) ou ainda dentro de uma transição/brilho.
    pub fn animando(&self, now: Instant) -> bool {
        self.ocupada() || now.saturating_duration_since(self.desde) < Duration::from_millis(MORPH_MS.max(SUCESSO_MS) + 120)
    }

    /// Tamanho visível agora (interpolado entre `de` e o alvo da fase).
    pub fn tamanho_atual(&self, now: Instant, reduced: bool) -> (f32, f32) {
        let p = crate::anim::progress(Some(self.desde), now, MORPH_MS, reduced);
        let (a, b) = (self.de, tamanho(self.fase));
        (a.0 + (b.0 - a.0) * p, a.1 + (b.1 - a.1) * p)
    }

    pub fn mudar(&mut self, fase: Fase, now: Instant, reduced: bool) {
        self.de = self.tamanho_atual(now, reduced);
        self.fase = fase;
        self.desde = now;
    }
}

/// Alvo de tamanho de cada fase (largura, altura).
pub fn tamanho(fase: Fase) -> (f32, f32) {
    match fase {
        Fase::Standby => (132.0, 32.0),
        Fase::Lendo | Fase::Cadastro => (318.0, 64.0),
        Fase::Sucesso => (296.0, 64.0),
        Fase::Erro => (348.0, 72.0),
    }
}

#[derive(Debug, Clone)]
pub enum FaceMsg {
    /// Começa uma leitura (toque na pílula, tela de entrada ou pedido de senha).
    Iniciar(Proposito),
    Iniciada(u64, Result<String, FaceFalha>),
    Evento(u64, FaceEvento),
    /// Fim do brilho de sucesso: segue para a Home ou conclui o pedido.
    Concluir(u64),
    /// Fim do aviso de erro: volta à pílula.
    Recolher(u64),
    Cancelar,
    // cadastro (Configurações › Câmera)
    CadastroPedir,
    CadastroSenha(String),
    CadastroFechar,
    CadastroOk,
    Cadastrado(Result<usize, String>),
    Desligar,
    Desligado(Result<(), String>),
}

/// Id do campo de senha que recebe o foco quando o rosto falha.
pub fn campo_senha(p: Proposito) -> &'static str {
    match p {
        Proposito::Login => "login-password",
        Proposito::Autonomo => "auto-password",
        Proposito::Panico => "panic-password",
    }
}

pub fn texto_falha(f: &FaceFalha) -> &'static str {
    match f {
        FaceFalha::SemCamera => "Câmera indisponível",
        FaceFalha::PoucaLuz => "Pouca luz para o Face ID",
        FaceFalha::NaoReconhecido => "Rosto não reconhecido",
        FaceFalha::Travado => "Face ID travado: use a senha",
        FaceFalha::Inativo => "Face ID não está ativado",
        FaceFalha::Outro(_) => "Face ID indisponível agora",
    }
}

fn depois(ms: u64, msg: Msg) -> Task<Msg> {
    Task::perform(async move { tokio::time::sleep(Duration::from_millis(ms)).await }, move |_| msg.clone())
}

impl App {
    pub fn face_id_ativo(&self) -> bool {
        self.island.demo || self.api.as_ref().is_some_and(|a| a.face_id_ativo())
    }

    /// A pílula aparece na tela de entrada com Face ID ativo e, já dentro, só durante uma leitura.
    pub fn island_visivel(&self) -> bool {
        if self.onboarding.is_some() || self.loading.is_some() {
            return false;
        }
        if !self.logged {
            return self.face_id_ativo();
        }
        self.island.fase != Fase::Standby || crate::anim::active(Some(self.island.desde), self.now, MORPH_MS)
    }

    fn cancelar_leitura(&mut self) {
        if let (Some(id), Some(api)) = (self.island.sessao.take(), self.api.as_ref()) {
            api.face_id_cancelar(&id);
        }
        self.island.geracao += 1;
    }

    pub fn update_face(&mut self, m: FaceMsg) -> Task<Msg> {
        let (now, reduced) = (Instant::now(), self.p.reduced);
        self.now = now;
        match m {
            FaceMsg::Iniciar(proposito) => {
                let Some(api) = self.api.clone() else { return Task::none() };
                if self.island.ocupada() || !api.face_id_ativo() {
                    return Task::none();
                }
                if proposito == Proposito::Login && (self.logged || self.challenge.is_some()) {
                    return Task::none();
                }
                self.cancelar_leitura();
                self.island.proposito = proposito;
                self.island.dica = FaceDica::Iniciando;
                self.island.falha = None;
                self.island.mudar(Fase::Lendo, now, reduced);
                crate::views::login::varredura(api, proposito, self.island.geracao)
            }
            FaceMsg::Iniciada(g, r) => {
                if g != self.island.geracao {
                    // leitura cancelada enquanto a câmera abria: solta já
                    if let (Ok(id), Some(api)) = (r, self.api.as_ref()) {
                        api.face_id_cancelar(&id);
                    }
                    return Task::none();
                }
                match r {
                    Ok(id) => {
                        self.island.sessao = Some(id);
                        Task::none()
                    }
                    Err(f) => self.face_falhou(f),
                }
            }
            FaceMsg::Evento(g, _) if g != self.island.geracao => Task::none(),
            FaceMsg::Evento(_, ev) => match ev {
                FaceEvento::Procurando(d) => {
                    self.island.dica = d;
                    Task::none()
                }
                FaceEvento::Entrou => {
                    self.island.sessao = None;
                    self.island.mudar(Fase::Sucesso, now, reduced);
                    self.pw.clear();
                    depois(SUCESSO_MS, Msg::Face(FaceMsg::Concluir(self.island.geracao)))
                }
                FaceEvento::Aprovado(tiquete) => {
                    self.island.sessao = None;
                    self.island.mudar(Fase::Sucesso, now, reduced);
                    let g = self.island.geracao;
                    let Some(api) = self.api.clone() else { return Task::none() };
                    let pedido = match self.island.proposito {
                        Proposito::Autonomo => {
                            self.auto_senha = None;
                            Task::perform(async move { api.post("/modo/autonomo", serde_json::json!({"ativo": true, "faceid": tiquete})).await }, |r| Msg::L(crate::live::Lm::Modo(r)))
                        }
                        Proposito::Panico => {
                            self.resume_pw.clear();
                            Task::perform(async move { api.post("/modo/panico", serde_json::json!({"ativo": false, "faceid": tiquete})).await }, |r| match r {
                                Ok(_) => Msg::L(crate::live::Lm::Acao(Ok("Retomado.".into()), true)),
                                Err(e) => Msg::L(crate::live::Lm::Acao(Err(e), true)),
                            })
                        }
                        Proposito::Login => Task::none(),
                    };
                    Task::batch([pedido, depois(SUCESSO_MS, Msg::Face(FaceMsg::Concluir(g)))])
                }
                FaceEvento::Falhou(f) => {
                    self.island.sessao = None;
                    self.face_falhou(f)
                }
            },
            FaceMsg::Concluir(g) if g == self.island.geracao && self.island.fase == Fase::Sucesso => {
                self.island.mudar(Fase::Standby, now, reduced);
                if self.island.proposito == Proposito::Login {
                    self.loading = Some("Abrindo o Orion…".into());
                    return Task::done(Msg::L(crate::live::Lm::Desbloqueado(Ok(()))));
                }
                Task::none()
            }
            FaceMsg::Recolher(g) if g == self.island.geracao && self.island.fase == Fase::Erro => {
                self.island.mudar(Fase::Standby, now, reduced);
                Task::none()
            }
            FaceMsg::Concluir(_) | FaceMsg::Recolher(_) => Task::none(),
            FaceMsg::Cancelar => {
                self.cancelar_leitura();
                if self.island.fase != Fase::Standby {
                    self.island.mudar(Fase::Standby, now, reduced);
                }
                iced::widget::operation::focus(campo_senha(self.island.proposito))
            }
            FaceMsg::CadastroPedir => {
                self.island.senha_cadastro = Some(String::new());
                iced::widget::operation::focus("faceid-password")
            }
            FaceMsg::CadastroSenha(s) => {
                self.island.senha_cadastro = Some(s);
                Task::none()
            }
            FaceMsg::CadastroFechar => {
                self.island.senha_cadastro = None;
                Task::none()
            }
            FaceMsg::CadastroOk => {
                let Some(api) = self.api.clone() else { return Task::none() };
                let Some(senha) = self.island.senha_cadastro.take().filter(|s| !s.is_empty()) else {
                    self.island.senha_cadastro = Some(String::new());
                    return Task::none();
                };
                if self.island.ocupada() {
                    return Task::none();
                }
                self.cancelar_leitura();
                self.island.falha = None;
                self.island.dica = FaceDica::Iniciando;
                self.island.mudar(Fase::Cadastro, now, reduced);
                Task::perform(async move { api.face_id_cadastrar(senha).await }, |r| Msg::Face(FaceMsg::Cadastrado(r)))
            }
            FaceMsg::Cadastrado(r) => match r {
                Ok(_) => {
                    self.island.proposito = Proposito::Autonomo;
                    self.island.mudar(Fase::Sucesso, now, reduced);
                    self.toast("Face ID ativado. Entrar e confirmar pedidos de senha agora aceitam o seu rosto; a senha continua valendo.");
                    depois(SUCESSO_MS, Msg::Face(FaceMsg::Concluir(self.island.geracao)))
                }
                Err(e) => {
                    self.island.falha = Some(FaceFalha::Outro(e.clone()));
                    self.island.mudar(Fase::Erro, now, reduced);
                    self.toast(&e);
                    depois(ERRO_MS, Msg::Face(FaceMsg::Recolher(self.island.geracao)))
                }
            },
            FaceMsg::Desligar => {
                let Some(api) = self.api.clone() else { return Task::none() };
                self.cancelar_leitura();
                Task::perform(async move { api.face_id_desligar().await }, |r| Msg::Face(FaceMsg::Desligado(r)))
            }
            FaceMsg::Desligado(r) => {
                match r {
                    Ok(()) => self.toast("Face ID desligado. O modelo do rosto e a credencial foram apagados."),
                    Err(e) => self.toast(&e),
                }
                Task::none()
            }
        }
    }

    /// Falha: a ilha abre o aviso e o campo de senha recebe o foco (a senha mestre é o caminho).
    fn face_falhou(&mut self, f: FaceFalha) -> Task<Msg> {
        let (now, reduced) = (Instant::now(), self.p.reduced);
        if let FaceFalha::Outro(e) = &f {
            tracing::info!("Face ID: {e}");
        }
        self.island.falha = Some(f);
        self.island.mudar(Fase::Erro, now, reduced);
        let g = self.island.geracao;
        let foco = match self.island.proposito {
            // pedido de senha do autônomo: garante que o campo existe
            Proposito::Autonomo if self.auto_senha.is_none() => {
                self.auto_senha = Some(String::new());
                iced::widget::operation::focus(campo_senha(Proposito::Autonomo))
            }
            p => iced::widget::operation::focus(campo_senha(p)),
        };
        Task::batch([foco, depois(ERRO_MS, Msg::Face(FaceMsg::Recolher(g)))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forma_interpola_entre_fases() {
        let t0 = Instant::now();
        let mut i = Island::new(t0);
        assert_eq!(i.tamanho_atual(t0, false), tamanho(Fase::Standby));
        i.mudar(Fase::Lendo, t0, false);
        let meio = i.tamanho_atual(t0 + Duration::from_millis(MORPH_MS / 4), false);
        assert!(meio.0 > tamanho(Fase::Standby).0 && meio.0 < tamanho(Fase::Lendo).0);
        assert_eq!(i.tamanho_atual(t0 + Duration::from_millis(MORPH_MS + 1), false), tamanho(Fase::Lendo));
        // movimento reduzido: salta direto
        i.mudar(Fase::Erro, t0, true);
        assert_eq!(i.tamanho_atual(t0, true), tamanho(Fase::Erro));
    }

    #[test]
    fn so_anima_quando_precisa() {
        let t0 = Instant::now();
        let mut i = Island::new(t0);
        assert!(!i.animando(t0 + Duration::from_secs(5)));
        i.mudar(Fase::Lendo, t0, false);
        assert!(i.animando(t0 + Duration::from_secs(5)));
    }
}

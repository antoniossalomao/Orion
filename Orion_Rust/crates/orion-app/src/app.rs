//! Estado, mensagens e laço de atualização (arquitetura Elm do iced).

use crate::data::{self, Act, ApprState, ChatMsg, Conv, ConvState, Rot};
use crate::{theme as t, views};
use iced::widget::text_editor;
use iced::{window, Element, Size, Subscription, Task};
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Home,
    Chat,
    Projetos,
    Memoria,
    Aprovacoes,
    Rotinas,
    Resultados,
    Nuvem,
    Fontes,
    Conhecimento,
    Painel,
    Arquivadas,
}

impl View {
    pub fn title(self) -> &'static str {
        match self {
            View::Home | View::Chat => "",
            View::Projetos => crate::i18n::text("Projetos"),
            View::Memoria => crate::i18n::text("Memória"),
            View::Aprovacoes => crate::i18n::text("Aprovações"),
            View::Rotinas => crate::i18n::text("Rotinas"),
            View::Resultados => crate::i18n::text("Resultados"),
            View::Nuvem => crate::i18n::text("Segundo plano"),
            View::Fontes => crate::i18n::text("Fontes"),
            View::Conhecimento => crate::i18n::text("Conhecimento"),
            View::Painel => crate::i18n::text("Painel"),
            View::Arquivadas => crate::i18n::text("Arquivadas"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CtxTab {
    Contexto,
    Arquivos,
    Diff,
    Terminal,
    Artefatos,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pop {
    Model,
    Project,
    Account,
    ConvMenu,
    ChatFont,
    SetLang,
    SetVoiceLang,
    SetVoiceStyle,
    DefaultModel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sec {
    Geral,
    Notificacoes,
    Conta,
    Privacidade,
    Uso,
    Modelos,
    Memoria,
    Fontes,
    Conectores,
    Sistema,
    Locais,
    Voz,
    Camera,
    Atalhos,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panic {
    Idle,
    Armed,
    Paused,
}

#[derive(Clone, Debug)]
pub struct Prefs {
    pub font: usize, // 0 Newsreader, 1 Geist, 2 sistema
    pub size: usize, // 0 P, 1 M, 2 G
    pub width: usize,
    pub reduced: bool,
    pub english: bool,
    pub scale: u32,
    pub compact: bool,
    pub tone: usize,
    pub tone_text: String,
    pub dnd: bool,
    pub auto: bool,
    pub notif: [bool; 4],
    pub sounds: [bool; 4],
    pub src: [bool; 5],
    pub mem: bool,
    pub cam: [bool; 5], // pedido, presença, onde, rosto, legenda
    pub wake: bool,
    pub clap: bool,
    pub voice_lang: usize,
    pub voice_style: usize,
    pub voice_speed: usize,
    pub boot: bool,
    pub shortcut: bool,
    pub mica: bool,
    pub name: String,
    pub call: String,
    pub prov: usize, // 0 Auto 1 Claude 2 Codex 3 Gemini 4 Orion
    pub model: usize,
    pub effort: usize,
    pub proj: usize,
    pub objs: Vec<String>,
    pub skills: [bool; 3],
    pub sync: bool,
    /// Opções avançadas de Sistema (F07.04): logs de depuração e reset do banco.
    pub advanced: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            font: 0,
            size: 1,
            width: 1,
            reduced: false,
            english: false,
            scale: 100,
            compact: false,
            tone: 0,
            tone_text: String::new(),
            dnd: false,
            auto: false,
            notif: [true, true, true, false],
            sounds: [true; 4],
            src: [true; 5],
            mem: true,
            cam: [true, false, true, true, false],
            wake: true,
            clap: true,
            voice_lang: 0,
            voice_style: 0,
            voice_speed: 0,
            boot: true,
            shortcut: true,
            mica: true,
            name: "Antônio Salomão".into(),
            call: "Antônio".into(),
            prov: 0,
            model: 1,
            effort: 1,
            proj: 0,
            objs: data::OBJ.iter().map(|s| s.to_string()).collect(),
            skills: [true; 3],
            sync: true,
            advanced: false,
        }
    }
}

pub const PROV_NAMES: [&str; 5] = ["Auto", "Claude", "Codex", "Gemini", "Orion"];
pub const PROV_MODELS: [&[&str]; 5] = [&[], &["Opus", "Sonnet", "Haiku"], &["Padrão", "Mini"], &["Pro", "Flash"], &[]];
pub const EFFORTS: [&str; 3] = ["Baixo", "Médio", "Alto"];

impl Prefs {
    pub fn model_label(&self) -> String {
        let p = PROV_NAMES[self.prov];
        match PROV_MODELS[self.prov].get(self.model) {
            Some(m) => format!("{p} {}", crate::i18n::status(m)),
            None => p.to_string(),
        }
    }
    pub fn has_effort(&self) -> bool {
        matches!(self.prov, 1..=3)
    }
    pub fn chat_fs(&self) -> f32 {
        [15.0, 16.5, 18.5][self.size]
    }
    pub fn thread_w(&self) -> f32 {
        // Médio é o padrão: 760 px de transcrição (F07.10).
        [40.0, 47.5, 58.0][self.width] * 16.0
    }
}

pub struct Stream {
    pub conv: usize,
    pub words: usize,
    pub total: usize,
}

pub struct PalState {
    pub open: bool,
    pub q: String,
    pub sel: usize,
}

#[derive(Default)]
pub struct ProjectForm {
    pub name: String,
    pub instructions: String,
    pub root: String,
    pub busy: bool,
    pub error: String,
    pub request_id: u64,
}

/// Cadastro de uma chave de provedor nas Configurações (o valor só vive em memória até salvar).
pub struct KeyForm {
    pub nome: String,
    pub label: String,
    pub value: String,
    pub busy: bool,
    pub error: String,
}

impl Drop for KeyForm {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.value.zeroize();
    }
}

pub struct RenameForm {
    pub session_id: String,
    pub title: String,
    pub busy: bool,
    pub error: String,
    pub request_id: u64,
}

pub struct EditorState {
    pub path: String,
    pub name: String,
    pub content: text_editor::Content,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CapKind {
    Tarefa,
    Lembrete,
    Gasto,
    Nota,
}

pub struct QCap {
    pub text: String,
    pub kind: CapKind,
    pub manual: bool,
}

pub struct Isso {
    pub q: String,
    pub answered: bool,
}

/// Marcas de tempo das transições (§26.1).
#[derive(Default, Clone)]
pub struct Marks {
    pub page: Option<Instant>,
    pub pop: Option<Instant>,
    pub modal: Option<Instant>,
    pub scrim: Option<Instant>,
    pub over: Option<Instant>,
    pub drawer: Option<Instant>,
    pub toast: Option<Instant>,
    pub more: Option<Instant>,
    pub tools: Option<Instant>,
    pub switch: Option<(String, Instant)>,
}

pub struct App {
    pub p: Prefs,
    pub logged: bool,
    pub pw: String,
    /// Usuário da entrada (`ORION_USER`; padrão `antoniosalomao`).
    pub user: String,
    pub window_active: bool,
    pub loading: Option<String>,
    pub message_heights: std::collections::HashMap<(String, usize), f32>,
    pub gate_err: String,
    pub view: View,
    pub conv: Option<usize>,
    pub convs: Vec<Conv>,
    pub collapsed: bool,
    pub more: bool,
    pub side_q: String,
    pub drawer: bool,
    pub composer: text_editor::Content,
    pub attach: Vec<(String, String)>,
    pub attachment_ids: Vec<String>,
    pub attachment_generation: u64,
    pub attaching: bool,
    pub attach_n: usize,
    pub streaming: Option<Stream>,
    pub ctx_override: Option<bool>,
    pub ctx_tab: CtxTab,
    pub tools_open: HashSet<(usize, usize)>,
    pub hist: Vec<(View, Option<usize>)>,
    pub fwd: Vec<(View, Option<usize>)>,
    pub rots: Vec<Rot>,
    pub rot_pend: bool,
    pub acts: Vec<Act>,
    pub act_filter: usize,
    pub proj_q: String,
    pub project_form: Option<ProjectForm>,
    pub rename_form: Option<RenameForm>,
    pub key_form: Option<KeyForm>,
    /// A conversa está colada no fim? (esconde o botão "ir ao fim")
    pub chat_end: bool,
    /// Conversas em que o usuário pediu para ver todas as mensagens (por padrão só as mais recentes são montadas).
    /// Mensagens montadas por conversa (janela que cresce ao rolar para o topo).
    pub chat_win: std::collections::HashMap<usize, usize>,
    /// Trecho visível da conversa (início, tamanho), de 0 a 1, para o minimapa.
    pub chat_vp: (f32, f32),
    /// Artefato HTML com a prévia aberta, no painel direito.
    pub art_preview: Option<usize>,
    pub artifact_records: Vec<serde_json::Value>,
    pub artifact_detail: Option<serde_json::Value>,
    pub artifact_busy: bool,
    /// Imagens geradas (`/imagens/…`) pedidas ao núcleo e ainda a caminho.
    pub imgs_pedidas: std::collections::HashSet<String>,
    /// Quais chaves de provedor já estão no cofre (só sim/não).
    pub keys: std::collections::BTreeMap<String, bool>,
    pub request_id: u64,
    pub facts: Vec<data::Fact>,
    pub edit_fact: Option<(usize, String)>,
    pub archived: Vec<(String, String, String)>,
    pub projects: Vec<data::Project>,
    pub cloud: Vec<data::CloudTask>,
    /// Tarefas na nuvem pelas CLIs oficiais (`/nuvem`); vazio com a nuvem desligada.
    pub nuvem: Vec<data::NuvemItem>,
    pub mcp: Vec<data::Mcp>,
    pub results: Vec<(String, String, String, String, String)>,
    pub cloud_extra: Vec<(String, String)>,
    pub skills: Vec<data::SkillRow>,
    pub locais: Vec<data::LocalModel>,
    pub set_open: bool,
    pub sec: Sec,
    pub set_q: String,
    pub set_detail: bool,
    pub panic: Panic,
    pub wipe: bool,
    pub pal: PalState,
    pub editor: Option<EditorState>,
    pub qcap: Option<QCap>,
    /// Senha digitada para ligar o modo autônomo (None = pedido fechado).
    pub auto_senha: Option<String>,
    pub isso: Option<Isso>,
    pub voice: bool,
    pub mic_rec: bool,
    pub pop: Option<Pop>,
    pub toast: Option<(String, Instant)>,
    pub win: Size,
    pub now: Instant,
    /// Início da tela de entrada (animação de entrada e respiração do brilho).
    pub login_at: Instant,
    pub spin: f32,
    pub term_out: Vec<String>,
    pub term_in: String,
    pub new_obj: String,
    pub sug_sel: usize,
    pub pending_resolved: HashSet<(usize, usize)>,
    pub mem: crate::graph::Graph,
    pub mem_q: String,
    pub mem_tween: Option<(Instant, (f32, f32, f32), (f32, f32, f32))>,
    pub mem_tip: (f32, f32),
    pub hover: Option<(u8, usize)>,
    pub api: Option<crate::net::Api>,
    pub challenge: Option<String>,
    pub pending_send: Option<String>,
    pub abort: Option<iced::task::Handle>,
    pub win_id: Option<window::Id>,
    /// Mini barra: a janela está encolhida no topo da tela.
    pub mini: bool,
    pub mini_prev: Size,
    /// Banco cifrado trancado: a tela de entrada pede a senha do banco antes de abrir o núcleo.
    pub db_locked: bool,
    pub onboarding: Option<crate::onboarding::Form>,
    pub fundo: bool,
    pub bandeja: Option<crate::tray::Bandeja>,
    pub monitor: Option<crate::tray::Monitor>,
    pub bandeja_tentou: bool,
    pub painel: Option<serde_json::Value>,
    pub budget: Option<serde_json::Value>,
    pub resume_pw: String,
    pub cur_sid: Option<String>,
    pub at: Marks,
    pub t0: Instant,
    pub hour: Option<u32>,
    pub toast_pinned: bool,
    /// Aviso de novidades ainda não mostrado nesta versão (novidades.rs).
    pub news_pending: bool,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Id da janela principal (primeira tarefa do boot).
    Win(Option<window::Id>),
    /// Pulso do segundo plano: bandeja, atalhos, palavra de ativação, outra instância.
    Bandeja,
    Acao(crate::tray::Acao),
    CloseReq(window::Id),
    /// Liga ou desliga a mini barra.
    Mini(bool),
    Mostrar,
    MiniPos(window::Id, Option<Size>),
    // entrada
    PwChanged(String),
    UserChanged(String),
    Login,
    Hello,
    LoginCancel,
    Onboarding(crate::onboarding::Event),
    Logout,
    // navegação
    Go(View),
    NewChat,
    OpenConv(usize),
    Back,
    Fwd,
    Collapse,
    ToggleMore,
    SideQ(String),
    Drawer(bool),
    // composer
    Edit(text_editor::Action),
    Send,
    Stop,
    RepeatLast,
    RepeatLastIfEmpty,
    Attach,
    Unattach(usize),
    Mic,
    Live,
    Sugg(String),
    SugPick,
    Fill(&'static str),
    // pop
    Pop(Pop),
    ClosePop,
    SetProv(usize),
    SetModelFull(usize, usize),
    SetModel(usize),
    SetEffort(usize),
    SetProj(usize),
    // chat
    ToggleTools(usize, usize),
    Approve(usize, usize),
    Deny(usize, usize),
    ShowDiff,
    CtxToggle,
    CtxTab(CtxTab),
    TermIn(String),
    TermRun,
    PinToggle,
    ArchiveConv,
    DeleteConv,
    CopyText(String),
    OpenUrl(String),
    /// `git pull` do vault (uso real; nos dados de exemplo é só um aviso).
    VaultPull,
    RenameStart,
    RenameText(String),
    RenameSave,
    ChatScrolled(iced::widget::scrollable::Viewport),
    ChatEnd,
    /// Grava o artefato `n` (bloco de código da conversa aberta) em Documentos/Orion/Artefatos.
    ArtifactSave(usize),
    ArtifactPreview(usize),
    ArtifactRecord(usize),
    ArtifactUpdate(usize, String),
    ArtifactRead(String, i64),
    ShowAll(usize),
    ChatJump(f32),
    KeyOpen(String),
    KeyInput(String),
    KeyCancel,
    KeySave,
    RenameClose,
    Toast(String),
    // páginas
    ProjQ(String),
    OpenProject(usize),
    ProjectStart,
    ProjectName(String),
    ProjectInstructions(String),
    ProjectRoot(String),
    ProjectSave,
    ProjectClose,
    ActFilter(usize),
    Undo(usize),
    ApproveRot,
    DenyRot,
    RotToggle(usize),
    RotRun(usize),
    FactEdit(usize),
    FactText(String),
    FactSave,
    FactForget(usize),
    Unarchive(usize),
    DeleteArchived(usize),
    CloudNew,
    CloudCancel(usize),
    McpTest(usize),
    SkillToggle(usize),
    ModelGet(usize),
    ModelDel(usize),
    Refresh,
    // configurações
    OpenSettings(Option<Sec>),
    CloseSettings,
    OpenLogs,
    SetSec(Sec),
    SetQ(String),
    SetBack,
    SetGo(View),
    Toggle(&'static str),
    TogA(&'static str, usize),
    Pick(&'static str, usize),
    NameChanged(String),
    CallChanged(String),
    ToneText(String),
    PanicArm,
    PanicCancel,
    PanicConfirm,
    PanicResume,
    WipeArm,
    WipeCancel,
    WipeConfirm,
    ObjRemove(usize),
    NewObj(String),
    ObjAdd,
    // overlays
    PalOpen,
    PalClose,
    PalQ(String),
    PalMove(i32),
    PalRun(usize),
    QcapOpen,
    QcapClose,
    AutoSenhaText(String),
    AutoSenhaOk,
    AutoSenhaClose,
    NuvemTrazer(usize),
    NuvemDescartar(usize),
    QcapText(String),
    QcapKind(usize),
    QcapSave,
    IssoOpen,
    IssoClose,
    IssoQ(String),
    IssoAsk,
    EditorOpen(String),
    EditorEdit(text_editor::Action),
    EditorClose,
    EditorSave,
    // grafo
    MemQ(String),
    MemFilter(usize),
    MemRead,
    MemZoom(f32),
    MemFit,
    MemSelect(Option<usize>),
    MemHover(Option<usize>, f32, f32),
    MemHalt,
    MemPan(f32, f32),
    MemZoomAt(f32, f32, f32),
    MemDrag(usize, f32, f32),
    MemCut(f32),
    MemOpenNote(usize),
    MemLocal(usize),
    // janela
    Tick(Instant),
    WindowActive(bool),
    BootProgress,
    MessageSize(String, usize, f32),
    Resized(Size),
    DragWindow,
    Minimize,
    Fullscreen,
    CloseWindow,
    Bg,
    Esc,
    Hover(Option<(u8, usize)>),
    L(crate::live::Lm),
    Poll,
    ResumePw(String),
    Noop,
}

impl App {
    pub fn new() -> Self {
        App {
            p: Prefs::default(),
            logged: false,
            pw: String::new(),
            user: std::env::var("ORION_USER").ok().filter(|s| !s.trim().is_empty()).unwrap_or_default(),
            window_active: true,
            loading: None,
            message_heights: Default::default(),
            gate_err: String::new(),
            view: View::Home,
            conv: None,
            convs: data::convs(),
            collapsed: false,
            more: false,
            side_q: String::new(),
            drawer: false,
            composer: text_editor::Content::new(),
            attach: vec![],
            attachment_ids: vec![],
            attachment_generation: 0,
            attaching: false,
            attach_n: 0,
            streaming: None,
            ctx_override: None,
            ctx_tab: CtxTab::Contexto,
            tools_open: HashSet::new(),
            hist: vec![],
            fwd: vec![],
            rots: data::rots(),
            rot_pend: true,
            acts: data::acts(),
            act_filter: 0,
            proj_q: String::new(),
            project_form: None,
            rename_form: None,
            key_form: None,
            chat_end: true,
            chat_win: std::collections::HashMap::new(),
            chat_vp: (0.0, 1.0),
            art_preview: None,
            artifact_records: vec![],
            artifact_detail: None,
            artifact_busy: false,
            imgs_pedidas: std::collections::HashSet::new(),
            keys: std::collections::BTreeMap::new(),
            request_id: 0,
            facts: data::facts(),
            edit_fact: None,
            archived: data::arch(),
            projects: data::projects(),
            cloud: data::cloud(),
            nuvem: vec![],
            mcp: data::mcp(),
            results: data::results(),
            cloud_extra: vec![],
            skills: data::skills_demo(),
            locais: data::locais_demo(),
            set_open: false,
            sec: Sec::Geral,
            set_q: String::new(),
            set_detail: false,
            panic: Panic::Idle,
            wipe: false,
            pal: PalState { open: false, q: String::new(), sel: 0 },
            editor: None,
            qcap: None,
            auto_senha: None,
            isso: None,
            voice: false,
            mic_rec: false,
            pop: None,
            toast: None,
            win: Size::new(1440.0, 900.0),
            now: Instant::now(),
            login_at: Instant::now(),
            spin: 0.0,
            term_out: data::TERM.iter().map(|s| s.to_string()).collect(),
            term_in: String::new(),
            new_obj: String::new(),
            sug_sel: 0,
            pending_resolved: HashSet::new(),
            mem: crate::graph::Graph::load(),
            mem_q: String::new(),
            mem_tween: None,
            mem_tip: (0.0, 0.0),
            hover: None,
            api: None,
            challenge: None,
            pending_send: None,
            abort: None,
            win_id: None,
            mini: false,
            mini_prev: Size::new(1440.0, 900.0),
            db_locked: false,
            onboarding: None,
            fundo: false,
            bandeja: None,
            monitor: None,
            bandeja_tentou: false,
            painel: None,
            budget: None,
            resume_pw: String::new(),
            cur_sid: None,
            at: Marks::default(),
            t0: Instant::now(),
            hour: None,
            toast_pinned: false,
            news_pending: crate::novidades::pending(),
        }
    }

    pub fn boot() -> (Self, Task<Msg>) {
        (Self::new(), Task::none())
    }

    pub fn title(&self) -> String {
        match (self.view, self.conv) {
            (View::Chat, Some(i)) => format!("{} · Orion", self.convs[i].title),
            _ => "Orion".into(),
        }
    }

    pub fn theme(&self) -> iced::Theme {
        iced::Theme::custom(
            "Grafite".to_string(),
            iced::theme::Palette { background: t::BG, text: t::FG, primary: t::PRIMARY, success: t::OK, warning: t::SIGNAL, danger: t::DANGER },
        )
    }

    pub fn scale_factor(&self) -> f32 {
        self.p.scale as f32 / 100.0
    }

    pub fn subscription(&self) -> Subscription<Msg> {
        let a = &self.at;
        let entrada = (!self.logged && crate::anim::active(Some(self.login_at), self.now, 1500)) || self.onboarding.as_ref().is_some_and(|f| crate::anim::active(Some(f.step_at), self.now, 450));
        let busy = entrada || [a.page, a.pop, a.modal, a.scrim, a.over, a.drawer, a.toast, a.more, a.tools].iter().any(|m| crate::anim::active(*m, self.now, 450)) || a.switch.as_ref().map(|s| crate::anim::active(Some(s.1), self.now, 300)).unwrap_or(false) || self.mem_tween.is_some();
        let moving = busy || self.streaming.is_some() || self.toast.is_some() || self.news_pending;
        let tick = if self.window_active && moving { iced::time::every(Duration::from_millis(42)).map(Msg::Tick) } else { Subscription::none() };
        let progress = if self.loading.is_some() { iced::time::every(Duration::from_millis(150)).map(|_| Msg::BootProgress) } else { Subscription::none() };
        let focus = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(window::Event::Focused) => Some(Msg::WindowActive(true)),
            iced::Event::Window(window::Event::Unfocused) => Some(Msg::WindowActive(false)),
            _ => None,
        });
        let poll = if self.window_active && self.api.is_some() && self.logged && self.streaming.is_none() { iced::time::every(Duration::from_secs(6)).map(|_| Msg::Poll) } else { Subscription::none() };
        let resize = window::resize_events().map(|(_, s)| Msg::Resized(s));
        let keys = iced::keyboard::listen().filter_map(crate::keys::map);
        let fundo = match &self.monitor {
            Some(monitor) => monitor.subscription(),
            None => Subscription::run(|| futures::stream::once(async { Msg::Bandeja })),
        };
        let fechar = window::close_requests().map(Msg::CloseReq);
        Subscription::batch([tick, resize, keys, poll, fundo, fechar, focus, progress])
    }

    pub fn pend_list(&self) -> Vec<(usize, usize)> {
        let mut out = vec![];
        for (ci, c) in self.convs.iter().enumerate() {
            for (mi, m) in c.msgs.iter().enumerate() {
                if let ChatMsg::Orion(o) = m {
                    if let Some(a) = &o.approval {
                        if a.state == ApprState::Open {
                            out.push((ci, mi));
                        }
                    }
                }
            }
        }
        out
    }
    pub fn pend_count(&self) -> usize {
        self.pend_list().len() + usize::from(self.rot_pend)
    }

    pub fn is_drawer_mode(&self) -> bool {
        self.win.width <= t::DRAWER_MAX
    }
    pub fn show_ctx(&self) -> bool {
        if self.view != View::Chat || self.win.width <= t::CTX_MIN_W {
            return false;
        }
        match self.ctx_override {
            Some(v) => v,
            None => self.conv.map(|i| self.convs[i].state == ConvState::Pending || self.convs[i].msgs.iter().any(|m| matches!(m, ChatMsg::Orion(o) if matches!(&o.approval, Some(a) if a.state == ApprState::Open)))).unwrap_or(false),
        }
    }

    pub fn toast(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), Instant::now()));
    }

    fn navigate(&mut self, v: View, conv: Option<usize>) {
        if (self.view, self.conv) != (v, conv) {
            self.hist.push((self.view, self.conv));
            self.fwd.clear();
        }
        self.enter(v, conv);
    }
    fn enter(&mut self, v: View, conv: Option<usize>) {
        self.view = v;
        self.conv = conv;
        self.ctx_tab = CtxTab::Contexto;
        self.ctx_override = None;
        self.edit_fact = None;
        self.artifact_records.clear();
        self.artifact_detail = None;
        self.drawer = false;
        self.pop = None;
        self.composer = text_editor::Content::new();
        self.attach.clear();
        self.attachment_ids.clear();
        self.attachment_generation = self.attachment_generation.wrapping_add(1);
        if v == View::Rotinas {
            for r in self.rots.iter_mut() {
                r.news = false;
            }
        }
    }

    pub fn draft(&self) -> String {
        self.composer.text().trim_end_matches('\n').to_string()
    }

    pub fn update(&mut self, msg: Msg) -> Task<Msg> {
        let _timing = Timing::new("update", self.view);
        if !matches!(&msg, Msg::Tick(_) | Msg::Bandeja | Msg::BootProgress) { self.now = Instant::now(); }
        if let Msg::Onboarding(event) = msg { return self.update_onboarding(event); }

        if let Msg::L(m) = msg {
            return self.lm(m);
        }
        match &msg {
            // ligar o autônomo pede a senha de novo; desligar é direto
            Msg::Toggle("auto") if !self.p.auto => {
                self.auto_senha = Some(String::new());
                return Task::none();
            }
            Msg::AutoSenhaText(s) => {
                self.auto_senha = Some(s.clone());
                return Task::none();
            }
            Msg::AutoSenhaClose => {
                self.auto_senha = None;
                return Task::none();
            }
            Msg::AutoSenhaOk => {
                let Some(senha) = self.auto_senha.take() else { return Task::none() };
                if senha.is_empty() {
                    self.auto_senha = Some(senha);
                    return Task::none();
                }
                if let Some(api) = self.api.clone() {
                    return Task::perform(async move { api.post("/modo/autonomo", serde_json::json!({"ativo": true, "senha": senha})).await }, |r| Msg::L(crate::live::Lm::Modo(r)));
                }
                self.p.auto = true;
                self.toast("Modo autônomo ligado. Ações que podem quebrar o PC ou o projeto continuam pedindo confirmação.");
                return Task::none();
            }
            _ => {}
        }
        let extra = self.live_before(&msg);
        if let Some((t, false)) = extra {
            return t;
        }
        let task = self.update_local(msg);
        match extra {
            Some((t, true)) => Task::batch([task, t]),
            _ => task,
        }
    }

    fn update_local(&mut self, msg: Msg) -> Task<Msg> {
        let before = (self.view, self.conv, self.pop, self.set_open, self.pal.open, self.qcap.is_some(), self.isso.is_some(), self.editor.is_some(), self.drawer, self.more, self.tools_open.len(), self.toast.as_ref().map(|t| t.1));
        let key = matches!(msg, Msg::Toggle(_) | Msg::TogA(..) | Msg::RotToggle(_)).then(|| format!("{msg:?}"));
        let task = self.update_inner(msg);
        let now = Instant::now();
        let a = &mut self.at;
        if (self.view, self.conv) != (before.0, before.1) {
            a.page = Some(now);
        }
        if self.pop != before.2 && self.pop.is_some() {
            a.pop = Some(now);
        }
        if (self.set_open && !before.3) || (self.editor.is_some() && !before.7) {
            a.modal = Some(now);
            a.scrim = Some(now);
        }
        if (self.pal.open && !before.4) || (self.qcap.is_some() && !before.5) || (self.isso.is_some() && !before.6) {
            a.over = Some(now);
            a.scrim = Some(now);
        }
        if self.drawer != before.8 {
            a.drawer = Some(now);
        }
        if self.more != before.9 {
            a.more = Some(now);
        }
        if self.tools_open.len() != before.10 {
            a.tools = Some(now);
        }
        if self.toast.as_ref().map(|t| t.1) != before.11 && self.toast.is_some() {
            a.toast = Some(now);
        }
        if let Some(k) = key {
            a.switch = Some((k, now));
        }
        task
    }

    fn update_inner(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Tick(now) => {
                self.now = now;

                crate::views::memory::tick(self, now);
                self.spin = (self.spin + 0.35) % std::f32::consts::TAU;
                if self.news_pending && !crate::novidades::started_in_background() {
                    self.news_pending = false;
                    crate::novidades::mark_seen();
                    self.toast(crate::novidades::text(self.p.english));
                }
                if let Some((_, at)) = &self.toast {
                    if now.duration_since(*at) > Duration::from_millis(2600) {
                        self.toast = None;
                    }
                }
                if let Some(s) = self.streaming.as_mut().filter(|s| s.total != usize::MAX) {
                    if s.words < s.total {
                        s.words += 1;
                    } else {
                        let ci = s.conv;
                        self.streaming = None;
                        if self.convs[ci].state == ConvState::Running {
                            self.convs[ci].state = ConvState::Normal;
                        }
                    }
                }
            }
            Msg::MessageSize(session, index, height) => { if self.message_heights.len() > 10000 { self.message_heights.clear(); } self.message_heights.insert((session, index), height); }
            Msg::BootProgress => { if let Some(message) = self.api.as_ref().and_then(|api| api.progress()) { self.loading = Some(message); } }
            Msg::WindowActive(active) => { self.window_active = active; self.now = Instant::now(); }
            Msg::Resized(s) => { self.win = s; if let Some(f) = self.onboarding.as_mut() { f.settle_sky(f.step, s); } },
            Msg::Hover(h) => self.hover = h,
            Msg::PwChanged(s) => self.pw = s,
            Msg::UserChanged(s) => self.user = s,
            Msg::Onboarding(_) | Msg::Hello | Msg::LoginCancel => {},
            Msg::Login => {
                if self.pw.trim().is_empty() {
                    self.gate_err = "Digite a senha para entrar.".into();
                } else {
                    self.logged = true;
                    self.gate_err.clear();
                    self.pw.clear();
                    self.navigate(View::Chat, Some(0));
                    self.hist.clear();
                    self.toast("git pull do vault: 3 notas novas");
                }
            }
            Msg::Logout => {
                self.logged = false;
                self.pop = None;
                self.set_open = false;
                self.pal.open = false;
                self.editor = None;
                self.voice = false;
            }
            Msg::Go(v) => {
                self.navigate(v, None);
                if v == View::Memoria && !self.mem.fitted {
                    let (w, h) = crate::views::memory::stage_size(self);
                    self.mem.fit(w, h);
                    self.mem.fitted = true;
                }
            }
            Msg::NewChat => self.navigate(View::Home, None),
            Msg::OpenConv(i) => {
                if self.convs[i].state == ConvState::Unread {
                    self.convs[i].state = ConvState::Normal;
                }
                self.navigate(View::Chat, Some(i));
            }
            Msg::Back => {
                if let Some((v, c)) = self.hist.pop() {
                    self.fwd.push((self.view, self.conv));
                    self.enter(v, c);
                }
            }
            Msg::Fwd => {
                if let Some((v, c)) = self.fwd.pop() {
                    self.hist.push((self.view, self.conv));
                    self.enter(v, c);
                }
            }
            Msg::Collapse => self.collapsed = !self.collapsed,
            Msg::ToggleMore => self.more = !self.more,
            Msg::SideQ(s) => self.side_q = s,
            Msg::Drawer(b) => self.drawer = b,
            Msg::Pop(p) => self.pop = if self.pop == Some(p) { None } else { Some(p) },
            Msg::ClosePop | Msg::Bg => {
                if matches!(msg, Msg::ClosePop) || self.pop.is_some() {
                    self.pop = None;
                }
            }
            Msg::SetProv(i) => {
                self.p.prov = i;
                self.p.model = if PROV_MODELS[i].is_empty() { 0 } else { self.p.model.min(PROV_MODELS[i].len() - 1) };
                self.pop = None;
            }
            Msg::SetModelFull(p, m) => {
                self.p.prov = p;
                self.p.model = m;
                self.pop = None;
            }
            Msg::SetModel(m) => {
                self.p.model = m;
                self.pop = None;
            }
            Msg::SetEffort(e) => {
                self.p.effort = e;
                self.pop = None;
            }
            Msg::SetProj(i) => {
                self.p.proj = i;
                self.pop = None;
            }
            Msg::Toast(s) => {
                self.pop = None;
                self.toast(s);
            }
            Msg::ChatScrolled(vp) => {
                // a lista é ancorada no fim: absolute_offset conta a distância até o fim; o _reversed, até o topo
                self.chat_end = vp.absolute_offset().y < 48.0;
                let (h, vh) = (vp.content_bounds().height.max(1.0), vp.bounds().height);
                self.chat_vp = ((vp.absolute_offset_reversed().y / h).clamp(0.0, 1.0), (vh / h).clamp(0.0, 1.0));
                // Janela virtual: só monta as mensagens que cobrem (distância até o fim + tela + 2400 px de folga).
                // A lista é ancorada no fim, então tirar mensagens do topo não move o que está na tela, e
                // incluir mais ao chegar perto do topo também não. Alturas vêm dos sensores de layout.
                if let Some(ci) = self.conv {
                    let total = self.convs.get(ci).map_or(0, |c| c.msgs.len());
                    let atual = self.chat_win.get(&ci).copied().unwrap_or(crate::views::chat::JANELA).clamp(1, total.max(1));
                    let session = &self.convs[ci].id;
                    let mut pixels = 0.0;
                    let needed = vp.absolute_offset().y + vh + 2400.0;
                    let count = (0..total).rev().take_while(|index| {
                        if pixels >= needed { return false; }
                        pixels += self.message_heights.get(&(session.clone(), *index)).copied().unwrap_or(80.0) + 30.0;
                        true
                    }).count();
                    let alvo = count.clamp(crate::views::chat::JANELA, total.max(crate::views::chat::JANELA));
                    // histerese: cresce já, encolhe só quando sobram mais de 20 mensagens (evita vaivém)
                    if alvo > atual || alvo + 20 < atual {
                        self.chat_win.insert(ci, alvo);
                    }
                }
            }
            Msg::ArtifactRecord(_) | Msg::ArtifactUpdate(..) | Msg::ArtifactRead(..) => {},
            Msg::ArtifactPreview(n) => {
                self.art_preview = if self.art_preview == Some(n) { None } else { Some(n) };
            }
            Msg::ArtifactSave(n) => {
                let Some(c) = self.conv.and_then(|i| self.convs.get(i)) else { return Task::none() };
                let Some((lang, body)) = crate::views::ctx::artifacts(c).into_iter().nth(n) else { return Task::none() };
                let ext = crate::views::ctx::ext_of(&lang);
                let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let conv_id = self.conv.and_then(|i| self.convs.get(i)).map(|c| c.id.chars().take(8).collect::<String>()).unwrap_or_default();
                let base = format!("artefato-{conv_id}-{}-{ts}", n + 1);
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || -> Result<String, String> {
                            let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).ok_or("pasta do usuário desconhecida")?;
                            let dir = std::path::PathBuf::from(home).join("Documents").join("Orion").join("Artefatos");
                            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                            // criação exclusiva: salvar de novo nunca sobrescreve um export anterior (tenta -2, -3, ...)
                            use std::io::Write;
                            for k in 1..=99 {
                                let nome = if k == 1 { format!("{base}.{ext}") } else { format!("{base}-{k}.{ext}") };
                                let alvo = dir.join(nome);
                                match std::fs::OpenOptions::new().write(true).create_new(true).open(&alvo) {
                                    Ok(mut f) => {
                                        f.write_all(body.as_bytes()).map_err(|e| e.to_string())?;
                                        return Ok(alvo.display().to_string());
                                    }
                                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                                    Err(e) => return Err(e.to_string()),
                                }
                            }
                            Err("nomes de arquivo esgotados".to_string())
                        })
                        .await
                        .map_err(|e| e.to_string())?
                    },
                    |r| Msg::Toast(match r { Ok(p) => format!("Salvo em {p}"), Err(e) => format!("Não foi possível salvar: {e}") }),
                );
            }
            Msg::ChatEnd => {
                self.chat_end = true;
                return iced::widget::operation::snap_to("chat-scroll", iced::widget::scrollable::RelativeOffset::START);
            }
            Msg::ShowAll(i) => {
                let atual = self.chat_win.get(&i).copied().unwrap_or(crate::views::chat::JANELA);
                self.chat_win.insert(i, atual + crate::views::chat::JANELA * 2);
            }
            Msg::ChatJump(f) => {
                // a lista é ancorada no fim: o deslocamento relativo é contado de baixo para cima
                let y = (1.0 - f).clamp(0.0, 1.0);
                return iced::widget::operation::snap_to("chat-scroll", iced::widget::scrollable::RelativeOffset { x: 0.0, y });
            }
            Msg::KeyOpen(nome) => {
                if self.key_form.as_ref().is_some_and(|f| f.busy) { return Task::none(); }
                let label = crate::onboarding::PROVIDERS.iter().find(|p| p.2 == nome).map(|p| p.1.to_string()).unwrap_or_else(|| nome.clone());
                self.key_form = Some(KeyForm { nome, label, value: String::new(), busy: false, error: String::new() });
                return iced::widget::operation::focus("api-key");
            }
            Msg::KeyInput(v) => {
                if let Some(f) = self.key_form.as_mut().filter(|f| !f.busy) { f.value = v; f.error.clear(); }
            }
            Msg::KeyCancel => {
                if self.key_form.as_ref().is_some_and(|f| !f.busy) { self.key_form = None; }
            }
            Msg::OpenLogs => {
                if let Some(path) = self.api.as_ref().and_then(|a| a.logs_dir()) {
                    return Task::perform(async move { tokio::task::spawn_blocking(move || {
                        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
                        #[cfg(windows)] let mut cmd = crate::onboarding::system_command("explorer.exe");
                        #[cfg(target_os = "macos")] let mut cmd = crate::onboarding::system_command("open");
                        #[cfg(all(not(windows), not(target_os = "macos")))] let mut cmd = crate::onboarding::system_command("xdg-open");
                        cmd.arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
                    }).await.map_err(|e| e.to_string()).and_then(|r| r) }, |result| match result { Ok(()) => Msg::Noop, Err(e) => Msg::Toast(e) });
                }
            }
            Msg::KeySave => self.toast("Guardar chave: só no uso real"),
            Msg::VaultPull => self.toast("git pull: já atualizado"),
            Msg::OpenUrl(url) if url.starts_with("orion-file:") => {
                let alvo = url.trim_start_matches("orion-file:").to_string();
                match crate::md::file_ref(&alvo) {
                    Some((path, line)) if std::path::Path::new(&path).is_absolute() && std::path::Path::new(&path).is_file() => {
                        let target = match line { Some(l) => format!("{path}:{l}"), None => path };
                        return Task::perform(async move { tokio::task::spawn_blocking(move || crate::onboarding::open_in_editor(&target)).await.ok(); }, |_| Msg::Noop);
                    }
                    Some((path, _)) if std::path::Path::new(&path).is_absolute() => self.toast(if self.p.english { "File not found" } else { "Arquivo não encontrado" }),
                    _ => self.toast(if self.p.english { "Relative path: open it from the project folder" } else { "Caminho relativo: abra pela pasta do projeto" }),
                }
            }
            Msg::OpenUrl(url) => {
                // só http(s): link vindo de resposta do modelo nunca abre arquivo nem programa
                if crate::md::is_web(&url) {
                    return Task::perform(async move { tokio::task::spawn_blocking(move || crate::onboarding::open_link(&url)).await.ok(); }, |_| Msg::Noop);
                }
                self.toast(if self.p.english { "Only web links can be opened" } else { "Só links da web podem ser abertos" });
            }
            Msg::CopyText(text) => {
                self.toast(if self.p.english { "Copied" } else { "Copiado" });
                return iced::clipboard::write(text);
            }
            Msg::RenameStart => {
                self.pop = None;
                if self.rename_form.as_ref().is_some_and(|f| f.busy) { return Task::none(); }
                if let Some(c) = self.conv.and_then(|i| self.convs.get(i)) {
                    self.rename_form = Some(RenameForm { session_id: c.id.clone(), title: c.title.clone(), busy: false, error: String::new(), request_id: 0 });
                    return iced::widget::operation::focus("conversation-title");
                }
            }
            Msg::RenameText(value) => {
                if let Some(form) = self.rename_form.as_mut().filter(|f| !f.busy) { form.title = value; form.error.clear(); }
            }
            Msg::RenameClose => {
                if self.rename_form.as_ref().is_some_and(|f| !f.busy) { self.rename_form = None; }
            }
            Msg::RenameSave => {
                let Some(form) = self.rename_form.as_mut().filter(|f| !f.busy) else { return Task::none() };
                let title = form.title.trim().to_string();
                if !(1..=120).contains(&title.chars().count()) {
                    form.error = "Use um título entre 1 e 120 caracteres.".into();
                    return Task::none();
                }
                let Some(api) = self.api.clone() else {
                    form.error = "Renomear exige conexão com o núcleo.".into();
                    return Task::none();
                };
                self.request_id = self.request_id.wrapping_add(1);
                let request_id = self.request_id;
                form.request_id = request_id;
                form.busy = true;
                form.error.clear();
                let sid = form.session_id.clone();
                return Task::perform(async move {
                    let result = api.patch(&format!("/sessoes/{sid}"), serde_json::json!({"titulo": title})).await;
                    (sid, result)
                }, move |(sid, result)| Msg::L(crate::live::Lm::Renamed(request_id, sid, result)));
            }
            Msg::ProjectStart => {
                if self.project_form.is_none() {
                    self.project_form = Some(ProjectForm::default());
                    return iced::widget::operation::focus("project-name");
                }
            }
            Msg::ProjectName(value) => {
                if let Some(form) = self.project_form.as_mut().filter(|f| !f.busy) { form.name = value; form.error.clear(); }
            }
            Msg::ProjectInstructions(value) => {
                if let Some(form) = self.project_form.as_mut().filter(|f| !f.busy) { form.instructions = value; form.error.clear(); }
            }
            Msg::ProjectRoot(value) => {
                if let Some(form) = self.project_form.as_mut().filter(|f| !f.busy) { form.root = value; form.error.clear(); }
            }
            Msg::ProjectClose => {
                if self.project_form.as_ref().is_some_and(|f| !f.busy) { self.project_form = None; }
            }
            Msg::ProjectSave => {
                let Some(form) = self.project_form.as_mut().filter(|f| !f.busy) else { return Task::none() };
                let name = form.name.trim().to_string();
                if !(1..=120).contains(&name.chars().count()) || form.instructions.chars().count() > 16000 || form.root.chars().count() > 4096 {
                    form.error = "Use nome de 1 a 120 caracteres, instruções de até 16000 e caminho de até 4096.".into();
                    return Task::none();
                }
                let Some(api) = self.api.clone() else {
                    form.error = "Criar projeto exige conexão com o núcleo.".into();
                    return Task::none();
                };
                let root = form.root.trim();
                let body = serde_json::json!({"name": name, "instructions": form.instructions, "root": if root.is_empty() { None } else { Some(root) }, "share_personal": false});
                self.request_id = self.request_id.wrapping_add(1);
                let request_id = self.request_id;
                form.request_id = request_id;
                form.busy = true;
                form.error.clear();
                return Task::perform(async move { api.post("/projects", body).await }, move |r| Msg::L(crate::live::Lm::ProjectCreated(request_id, r)));
            }
            Msg::Edit(a) => {
                self.composer.perform(a);
                self.pop = None;
            }
            Msg::Send => return self.send(),
            Msg::Stop => {
                if let Some(s) = self.streaming.take() {
                    if self.convs[s.conv].state == ConvState::Running {
                        self.convs[s.conv].state = ConvState::Normal;
                    }
                }
            }
            Msg::RepeatLastIfEmpty => {
                if self.draft().is_empty() {
                    return self.update(Msg::RepeatLast);
                }
                self.composer.perform(text_editor::Action::Move(text_editor::Motion::Up));
            }
            Msg::RepeatLast => {
                if let Some(i) = self.conv {
                    if let Some(ChatMsg::User(s)) = self.convs[i].msgs.iter().rev().find(|m| matches!(m, ChatMsg::User(_))) {
                        self.composer = text_editor::Content::with_text(s);
                    }
                }
            }
            Msg::Attach => {
                let files = [("relatorio-ram.pdf", "1,2 MB"), ("captura.png", "340 KB"), ("notas.txt", "4 KB"), ("reuniao.m4a", "3,8 MB")];
                if self.attach_n < files.len() {
                    let f = files[self.attach_n];
                    self.attach.push((f.0.into(), f.1.into()));
                    self.attach_n += 1;
                } else {
                    self.toast("Até 20 MB por arquivo. Maior que isso: ponha numa pasta e passe o caminho.");
                }
            }
            Msg::Unattach(i) => {
                if i < self.attach.len() {
                    self.attach.remove(i);
                    if i < self.attachment_ids.len() { self.attachment_ids.remove(i); }
                }
            }
            Msg::Mic => {
                self.mic_rec = !self.mic_rec;
                if self.mic_rec {
                    self.toast("Gravando… clique de novo para enviar");
                }
            }
            Msg::Live => self.voice = !self.voice,
            Msg::Fill(s) => {
                self.composer = text_editor::Content::with_text(s);
            }
            Msg::SugPick => {
                let items = crate::views::composer::suggestions(self);
                if let Some((ins, _, _)) = items.get(self.sug_sel).cloned() {
                    return self.update(Msg::Sugg(ins));
                }
            }
            Msg::Sugg(s) => {
                let cur = self.draft();
                let new = if let Some(p) = cur.rfind(['/', '@']) { format!("{}{}", &cur[..p], s) } else { s };
                self.composer = text_editor::Content::with_text(&new);
                self.composer.perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
            }
            Msg::ToggleTools(c, m) => {
                if !self.tools_open.remove(&(c, m)) {
                    self.tools_open.insert((c, m));
                }
            }
            Msg::Approve(c, m) => self.resolve(c, m, true),
            Msg::Deny(c, m) => self.resolve(c, m, false),
            Msg::ShowDiff => {
                self.ctx_override = Some(true);
                self.ctx_tab = CtxTab::Diff;
            }
            Msg::CtxToggle => self.ctx_override = Some(!self.show_ctx()),
            Msg::CtxTab(tb) => self.ctx_tab = tb,
            Msg::TermIn(s) => self.term_in = s,
            Msg::TermRun => {
                let cmd = std::mem::take(&mut self.term_in);
                if !cmd.trim().is_empty() {
                    self.term_out.push(format!("PS> {cmd}"));
                    self.term_out.push(format!("(simulado) {cmd}"));
                }
            }
            Msg::PinToggle => {
                if let Some(i) = self.conv {
                    self.convs[i].pinned = !self.convs[i].pinned;
                    let m = if self.convs[i].pinned { "Conversa fixada" } else { "Conversa desafixada" };
                    self.toast(m);
                }
                self.pop = None;
            }
            Msg::ArchiveConv | Msg::DeleteConv => {
                if let Some(i) = self.conv {
                    let c = self.convs.remove(i);
                    if matches!(msg, Msg::ArchiveConv) {
                        self.archived.push((c.title, c.proj, "outubro".into()));
                        self.toast("Conversa arquivada");
                    } else {
                        self.toast("Conversa apagada");
                    }
                    self.navigate(View::Home, None);
                }
            }
            Msg::ProjQ(s) => self.proj_q = s,
            Msg::OpenProject(i) => {
                let name = self.projects[i].n.clone();
                if let Some(ci) = self.convs.iter().position(|c| c.proj == name && !c.msgs.is_empty()) {
                    self.navigate(View::Chat, Some(ci));
                } else {
                    self.p.proj = i;
                    self.navigate(View::Home, None);
                }
            }
            Msg::ActFilter(i) => self.act_filter = i,
            Msg::Undo(i) => {
                if let Some(a) = self.acts.get_mut(i) {
                    a.undone = true;
                    a.label = "desfeito".into();
                    let has_commit = a.text.contains("commit");
                    let text = format!("Desfeito: {}", a.text);
                    self.acts.insert(0, Act { when: "agora".into(), text, kind: "ok".into(), label: "revert".into(), undone: true, undoable: false });
                    self.toast(if has_commit { "git revert feito e push na main do vault" } else { "Ação revertida" });
                }
            }
            Msg::ApproveRot => {
                self.rot_pend = false;
                self.toast("Rotina “Organizar Downloads” aprovada");
            }
            Msg::DenyRot => {
                self.rot_pend = false;
                self.toast("Proposta negada");
            }
            Msg::RotToggle(i) => {
                self.rots[i].on = !self.rots[i].on;
                let m = format!("{} {}", self.rots[i].t, if self.rots[i].on { "ativada" } else { "pausada" });
                self.toast(m);
            }
            Msg::RotRun(i) => {
                let m = format!("{}: rodou agora. Saída: {}", self.rots[i].t, self.rots[i].out);
                self.toast(m);
            }
            Msg::FactEdit(i) => self.edit_fact = Some((i, self.facts[i].t.clone())),
            Msg::FactText(s) => {
                if let Some(e) = &mut self.edit_fact {
                    e.1 = s;
                }
            }
            Msg::FactSave => {
                if let Some((i, s)) = self.edit_fact.take() {
                    if !s.trim().is_empty() {
                        self.facts[i].t = s;
                        self.facts[i].src = "corrigido por você · hoje".into();
                        self.toast("Fato corrigido; commit na main");
                    }
                }
            }
            Msg::FactForget(i) => {
                self.facts.remove(i);
                self.toast("Fato esquecido; nota no vault atualizada");
            }
            Msg::Unarchive(i) => {
                self.archived.remove(i);
                self.toast("Conversa desarquivada");
            }
            Msg::DeleteArchived(i) => {
                self.archived.remove(i);
                self.toast("Conversa apagada");
            }
            Msg::CloudNew => {
                self.composer = text_editor::Content::with_text("/tarefa ");
                self.navigate(View::Home, None);
            }
            Msg::CloudCancel(i) => {
                if let Some(c) = self.cloud.get_mut(i) {
                    c.running = false;
                    c.proj = "cancelada".into();
                }
                self.toast("Tarefa cancelada");
            }
            Msg::McpTest(_) => self.toast("Teste do servidor (exemplo)"),
            Msg::SkillToggle(i) => {
                if let Some(k) = self.skills.get_mut(i) {
                    k.on = !k.on;
                }
            }
            Msg::ModelGet(_) => self.toast("Baixar modelo (exemplo)"),
            Msg::ModelDel(_) => self.toast("Remover modelo (exemplo)"),
            Msg::Refresh => self.toast("Painel atualizado"),
            Msg::OpenSettings(sec) => {
                self.set_open = true;
                self.drawer = false;
                self.pop = None;
                self.wipe = false;
                if self.panic == Panic::Armed {
                    self.panic = Panic::Idle;
                }
                if let Some(s) = sec {
                    self.sec = s;
                    self.set_detail = true;
                }
            }
            Msg::CloseSettings => {
                self.set_open = false;
                self.pop = None;
            }
            Msg::SetSec(s) => {
                self.sec = s;
                self.set_detail = true;
                self.pop = None;
            }
            Msg::SetQ(s) => {
                self.set_q = s;
                if let Some(first) = crate::views::settings::search(&self.set_q).first() {
                    self.sec = *first;
                }
            }
            Msg::SetBack => self.set_detail = false,
            Msg::SetGo(v) => {
                self.set_open = false;
                return self.update(Msg::Go(v));
            }
            Msg::Toggle(k) => self.toggle(k),
            Msg::TogA(arr, i) => {
                let p = &mut self.p;
                match arr {
                    "notif" => p.notif[i] = !p.notif[i],
                    "sounds" => p.sounds[i] = !p.sounds[i],
                    "src" => p.src[i] = !p.src[i],
                    "cam" => p.cam[i] = !p.cam[i],
                    "skill" => p.skills[i] = !p.skills[i],
                    _ => {}
                }
            }
            Msg::Pick(k, v) => self.pick(k, v),
            Msg::NameChanged(s) => self.p.name = s,
            Msg::CallChanged(s) => self.p.call = s,
            Msg::ToneText(s) => self.p.tone_text = s,
            Msg::PanicArm => self.panic = Panic::Armed,
            Msg::PanicCancel => self.panic = Panic::Idle,
            Msg::PanicConfirm => {
                self.panic = Panic::Paused;
                self.p.auto = false;
                self.p.cam[1] = false;
                self.voice = false;
                self.toast("Tudo pausado. Nenhuma ferramenta de escrita, rede ou execução roda.");
            }
            Msg::PanicResume => self.panic = Panic::Idle,
            Msg::WipeArm => self.wipe = true,
            Msg::WipeCancel => self.wipe = false,
            Msg::WipeConfirm => {
                self.wipe = false;
                self.convs.clear();
                self.hist.clear();
                self.fwd.clear();
                self.set_open = false;
                self.view = View::Home;
                self.conv = None;
                self.toast("Histórico apagado");
            }
            Msg::ObjRemove(i) => {
                if i < self.p.objs.len() {
                    self.p.objs.remove(i);
                }
            }
            Msg::NewObj(s) => self.new_obj = s,
            Msg::ObjAdd => {
                let o = self.new_obj.trim().to_string();
                if !o.is_empty() && !self.p.objs.contains(&o) {
                    self.p.objs.push(o);
                }
                self.new_obj.clear();
            }
            Msg::PalOpen => {
                if !self.set_open {
                    self.pal = PalState { open: !self.pal.open, q: String::new(), sel: 0 };
                }
            }
            Msg::PalClose => self.pal.open = false,
            Msg::PalQ(s) => {
                self.pal.q = s;
                self.pal.sel = 0;
            }
            Msg::PalMove(d) if !self.pal.open => {
                let n = crate::views::composer::suggestions(self).len() as i32;
                if n > 0 {
                    self.sug_sel = ((self.sug_sel as i32 + d).rem_euclid(n)) as usize;
                }
            }
            Msg::PalMove(d) => {
                let n = crate::views::overlays::pal_items(self).len().max(1) as i32;
                self.pal.sel = ((self.pal.sel as i32 + d).rem_euclid(n)) as usize;
            }
            Msg::PalRun(i) => return crate::views::overlays::pal_run(self, i),
            Msg::QcapOpen => self.qcap = Some(QCap { text: String::new(), kind: CapKind::Tarefa, manual: false }),
            Msg::QcapClose => self.qcap = None,
            Msg::QcapText(s) => {
                if let Some(q) = &mut self.qcap {
                    if !q.manual {
                        q.kind = detect_kind(&s);
                    }
                    q.text = s;
                }
            }
            Msg::QcapKind(k) => {
                if let Some(q) = &mut self.qcap {
                    q.kind = [CapKind::Tarefa, CapKind::Lembrete, CapKind::Gasto, CapKind::Nota][k];
                    q.manual = true;
                }
            }
            Msg::QcapSave => {
                if let Some(q) = self.qcap.take() {
                    if !q.text.trim().is_empty() {
                        let name = match q.kind {
                            CapKind::Tarefa => "Tarefa",
                            CapKind::Lembrete => "Lembrete",
                            CapKind::Gasto => "Gasto",
                            CapKind::Nota => "Nota",
                        };
                        self.acts.insert(0, Act { when: "agora".into(), text: format!("Captura rápida ({}): {}", name.to_lowercase(), q.text), kind: "ok".into(), label: "Inbox".into(), undone: false, undoable: true });
                        self.toast(format!("{name} salva em 00 Inbox · commit na main"));
                    }
                }
            }
            Msg::IssoOpen => self.isso = Some(Isso { q: String::new(), answered: false }),
            Msg::IssoClose => self.isso = None,
            Msg::IssoQ(s) => {
                if let Some(i) = &mut self.isso {
                    i.q = s;
                }
            }
            Msg::IssoAsk => {
                if let Some(i) = &mut self.isso {
                    i.answered = true;
                }
            }
            Msg::EditorOpen(path) => {
                let name = path.rsplit('/').next().unwrap_or(&path).trim_end_matches(".md").to_string();
                self.editor = Some(EditorState { path, name, content: text_editor::Content::with_text(data::NOTE_SAMPLE) });
                self.pal.open = false;
            }
            Msg::EditorEdit(a) => {
                if let Some(e) = &mut self.editor {
                    e.content.perform(a);
                }
            }
            Msg::EditorClose => self.editor = None,
            Msg::EditorSave => {
                self.editor = None;
                self.toast("Nota salva; commit e push na main do vault");
            }
            Msg::Win(id) => {
                self.win_id = id;
            }
            Msg::Bandeja => {
                let mut acoes = vec![];
                if !self.bandeja_tentou {
                    self.bandeja_tentou = true;
                    self.bandeja = crate::tray::Bandeja::nova();
                    self.monitor = Some(crate::tray::Monitor::new(self.bandeja.as_ref(), self.api.clone()));
                    // sem bandeja não há como reabrir uma janela escondida: abre mesmo em segundo plano
                    if self.bandeja.is_none() && self.fundo {
                        acoes.push(crate::tray::Acao::MiniBar);
                    }
                }
                if let Some(b) = &self.bandeja {
                    acoes.extend(b.consultar());
                }
                let mut tarefas: Vec<Task<Msg>> = vec![];
                match self.api.as_ref().and_then(|a| a.acordar()) {
                    Some(crate::net::Acordar::Voz) => acoes.push(crate::tray::Acao::MiniBar),
                    Some(crate::net::Acordar::Janela) => tarefas.push(Task::done(Msg::Mostrar)),
                    None => {}
                }
                tarefas.extend(acoes.into_iter().map(|x| Task::done(Msg::Acao(x))));
                return Task::batch(tarefas);
            }
            Msg::Acao(acao) => {
                use crate::tray::Acao;
                match acao {
                    Acao::Exit => return window::oldest().and_then(|id| window::close(id).chain(iced::exit())),
                    Acao::Panic => {
                        let r = self.api.as_ref().map(|a| a.panico()).unwrap_or_else(|| Err("pânico indisponível".into()));
                        self.toast(r.unwrap_or_else(|e| e));
                    }
                    Acao::MiniBar => return self.update_inner(Msg::Mini(true)),
                    Acao::QuickCapture => {
                        let t = self.update_inner(Msg::Mostrar);
                        let _ = self.update_inner(Msg::QcapOpen);
                        return t;
                    }
                    Acao::Isso => {
                        let t = self.update_inner(Msg::Mostrar);
                        let _ = self.update_inner(Msg::IssoOpen);
                        return t;
                    }
                    Acao::Budget => {
                        let mut ts = vec![self.update_inner(Msg::Mostrar)];
                        let _ = self.update_inner(Msg::OpenSettings(Some(Sec::Uso)));
                        ts.push(Task::none());
                        return Task::batch(ts);
                    }
                }
            }
            Msg::CloseReq(id) => {
                self.window_active = false;
                // com bandeja o app segue vivo, só esconde a janela; sem bandeja, fechar encerra
                if self.bandeja.is_none() {
                    return window::close(id).chain(iced::exit());
                }
                let mut ts = vec![window::set_mode(id, window::Mode::Hidden)];
                if self.mini {
                    self.mini = false;
                    ts.push(window::resize(id, self.mini_prev));
                    ts.push(window::set_level(id, window::Level::Normal));
                }
                return Task::batch(ts);
            }
            Msg::Mostrar => {
                self.window_active = true;
                self.now = Instant::now();
                let Some(id) = self.win_id else { return Task::none() };
                let mut ts = vec![window::set_mode(id, window::Mode::Windowed), window::minimize(id, false), window::gain_focus(id)];
                if self.mini {
                    self.mini = false;
                    ts.push(window::resize(id, self.mini_prev));
                    ts.push(window::set_level(id, window::Level::Normal));
                }
                return Task::batch(ts);
            }
            Msg::Mini(on) => {
                let Some(id) = self.win_id else { return Task::none() };
                if on == self.mini {
                    return window::gain_focus(id);
                }
                self.mini = on;
                self.pop = None;
                return if on {
                    self.mini_prev = self.win;
                    Task::batch([
                        window::set_mode(id, window::Mode::Windowed),
                        window::minimize(id, false),
                        window::resize(id, Size::new(660.0, 340.0)),
                        window::set_level(id, window::Level::AlwaysOnTop),
                        window::gain_focus(id),
                        window::monitor_size(id).map(move |s| Msg::MiniPos(id, s)),
                    ])
                } else {
                    Task::batch([window::resize(id, self.mini_prev), window::set_level(id, window::Level::Normal)])
                };
            }
            Msg::MiniPos(id, size) => {
                if let Some(s) = size {
                    return window::move_to(id, iced::Point::new(((s.width - 660.0) / 2.0).max(0.0), 16.0));
                }
            }
            Msg::DragWindow => return window::oldest().and_then(window::drag),
            Msg::Minimize => { self.window_active = false; return window::oldest().and_then(|id| window::minimize(id, true)); },
            Msg::Fullscreen => return window::oldest().and_then(window::toggle_maximize),
            Msg::CloseWindow => return window::oldest().and_then(|id| Task::done(Msg::CloseReq(id))),
            Msg::Esc => {
                if self.mini {
                    return self.update_inner(Msg::CloseWindow);
                }
                if self.pop.is_some() {
                    self.pop = None;
                } else if self.isso.is_some() {
                    self.isso = None;
                } else if self.qcap.is_some() {
                    self.qcap = None;
                } else if self.editor.is_some() {
                    self.editor = None;
                } else if self.pal.open {
                    self.pal.open = false;
                } else if self.set_open {
                    if self.set_detail && self.is_drawer_mode() { self.set_detail = false } else { self.set_open = false }
                } else if self.drawer {
                    self.drawer = false;
                } else if self.view == View::Memoria {
                    self.mem.sel = None;
                }
            }
            Msg::ResumePw(s) => self.resume_pw = s,
            Msg::Poll => {}
            Msg::Noop => {}
            m => crate::views::memory::update(self, m),
        }
        Task::none()
    }

    fn toggle(&mut self, k: &'static str) {
        let p = &mut self.p;
        match k {
            "auto" => {
                p.auto = !p.auto;
                let m = if p.auto { "Modo autônomo ligado. Ações que podem quebrar o PC ou o projeto continuam pedindo confirmação." } else { "Modo autônomo desligado" };
                self.toast(m);
            }
            "dnd" => p.dnd = !p.dnd,
            "mem" => p.mem = !p.mem,
            "wake" => p.wake = !p.wake,
            "clap" => p.clap = !p.clap,
            "boot" => p.boot = !p.boot,
            "shortcut" => p.shortcut = !p.shortcut,
            "mica" => p.mica = !p.mica,
            "advanced" => p.advanced = !p.advanced,
            "sync" => p.sync = !p.sync,
            "reduced" => p.reduced = !p.reduced,
            _ => {
                if let Some((arr, i)) = k.split_once(':') {
                    let i: usize = i.parse().unwrap_or(0);
                    match arr {
                        "notif" => p.notif[i] = !p.notif[i],
                        "sounds" => p.sounds[i] = !p.sounds[i],
                        "src" => p.src[i] = !p.src[i],
                        "cam" => p.cam[i] = !p.cam[i],
                        "skill" => p.skills[i] = !p.skills[i],
                        _ => {}
                    }
                }
            }
        }
    }

    fn pick(&mut self, k: &'static str, v: usize) {
        match k {
            "lang" => {
                self.p.english = v == 1;
                crate::i18n::set_english(self.p.english);
                self.bandeja = None;
                self.monitor = None;
                self.bandeja_tentou = false;
                self.mem.cache.clear();
                if v == 1 {
                    self.toast("Language updated.");
                }
            }
            "scale" => self.p.scale = [90, 100, 115, 130, 150][v],
            "density" => self.p.compact = v == 1,
            "motion" => self.p.reduced = v == 1,
            "font" => self.p.font = v,
            "size" => self.p.size = v,
            "width" => self.p.width = v,
            "tone" => self.p.tone = v,
            "vlang" => self.p.voice_lang = v,
            "vstyle" => self.p.voice_style = v,
            "vspeed" => self.p.voice_speed = v,
            _ => {}
        }
        self.pop = None;
    }

    fn resolve(&mut self, c: usize, m: usize, ok: bool) {
        let hm = chrono_hm();
        if let Some(ChatMsg::Orion(o)) = self.convs[c].msgs.get_mut(m) {
            if let Some(a) = &mut o.approval {
                a.state = if ok { ApprState::Approved(format!("Aprovado às {hm}. Executando e mostrando o resultado aqui.")) } else { ApprState::Denied(format!("Negado às {hm}. Nada foi alterado.")) };
            }
        }
        if !self.convs[c].msgs.iter().any(|x| matches!(x, ChatMsg::Orion(o) if matches!(&o.approval, Some(a) if a.state == ApprState::Open))) && self.convs[c].state == ConvState::Pending {
            self.convs[c].state = ConvState::Normal;
        }
        if self.show_ctx() && self.win.width <= t::CTX_MIN_W {
            self.toast("+176 −0 em 3 arquivos");
        }
    }

    fn send(&mut self) -> Task<Msg> {
        let text = self.draft();
        if text.trim().is_empty() || self.streaming.is_some() {
            return Task::none();
        }
        if let Some(cmd) = text.strip_prefix('/') {
            let word = cmd.split_whitespace().next().unwrap_or("");
            self.composer = text_editor::Content::new();
            return crate::views::composer::run_command(self, word, cmd);
        }
        if self.api.is_some() {
            return self.live_send(text);
        }
        let ci = match self.conv {
            Some(i) if self.view == View::Chat => i,
            _ => {
                let proj = self.projects.get(self.p.proj).map(|p| p.n.clone()).unwrap_or_else(|| "Sem projeto".into());
                self.convs.insert(0, Conv { id: "novo".into(), title: "Nova conversa".into(), proj, origin: None, day: "Hoje".into(), state: ConvState::Running, ctx: 2, branch: None, pinned: false, note: None, pin: None, banner: None, msgs: vec![] });
                for h in self.hist.iter_mut().chain(self.fwd.iter_mut()) {
                    if let Some(c) = &mut h.1 {
                        *c += 1;
                    }
                }
                0
            }
        };
        let mut shown = text.clone();
        if !self.attach.is_empty() {
            shown.push_str("\n\nAnexos: ");
            shown.push_str(&self.attach.iter().map(|a| a.0.clone()).collect::<Vec<_>>().join(", "));
        }
        self.convs[ci].msgs.push(ChatMsg::User(shown));
        if self.convs[ci].title == "Nova conversa" {
            let t: String = text.chars().take(40).collect();
            self.convs[ci].title = t;
        }
        let body = "Posso ajudar com isso. No app final a resposta vem do modelo escolhido; aqui é o texto de exemplo do protótipo, que aparece palavra por palavra.";
        let total = body.split_whitespace().count();
        let via = crate::views::chat::via_label(&self.p);
        self.convs[ci].msgs.push(ChatMsg::Orion(data::OrionMsg { time: "agora".into(), via, out: String::new(), tools_label: String::new(), tools: vec![], body: body.into(), saved: true, approval: None }));
        self.convs[ci].state = ConvState::Running;
        self.streaming = Some(Stream { conv: ci, words: 0, total });
        self.composer = text_editor::Content::new();
        self.attach.clear();
        self.attachment_ids.clear();
        self.attachment_generation = self.attachment_generation.wrapping_add(1);
        self.navigate(View::Chat, Some(ci));
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let _timing = Timing::new("view", self.view);
        crate::i18n::set_english(self.onboarding.as_ref().map(|f| f.language == "en").unwrap_or(self.p.english));
        crate::views::sync_anim(self);
        views::root(self)
    }
}

pub fn chrono_hm() -> String {
    crate::live::hm(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0))
}

pub fn detect_kind(s: &str) -> CapKind {
    let l = s.to_lowercase();
    let has_money = l.contains("r$") || l.contains("gastei") || l.contains("paguei") || l.split_whitespace().any(|w| w.len() >= 4 && w.contains(',') && w.chars().filter(|c| c.is_ascii_digit()).count() >= 3 && w.rsplit(',').next().map(|d| d.len() == 2).unwrap_or(false));
    if has_money {
        CapKind::Gasto
    } else if ["amanh", "hoje", "lembra"].iter().any(|k| l.contains(k)) || l.contains(" às ") {
        CapKind::Lembrete
    } else if ["comprar", "fazer", "ligar", "mandar", "revisar"].iter().any(|k| l.starts_with(k)) {
        CapKind::Tarefa
    } else {
        CapKind::Nota
    }
}

struct Timing { stage: &'static str, view: View, started: Option<Instant> }
impl Timing {
    fn new(stage: &'static str, view: View) -> Self { Self { stage, view, started: tracing::enabled!(target: "orion.performance", tracing::Level::DEBUG).then(Instant::now) } }
}
impl Drop for Timing {
    fn drop(&mut self) { if let Some(started) = self.started { tracing::debug!(target: "orion.performance", stage = self.stage, screen = ?self.view, elapsed_us = started.elapsed().as_micros() as u64); } }
}

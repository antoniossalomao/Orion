//! Controles reais dos recursos nativos (REGRAS.md: opt-in não liga sozinho; portabilidade).
use crate::{
    app::{App, Msg},
    net::Api,
    theme as t,
    ui::*,
};
use iced::{
    widget::{column, container, row, text_editor, text_input},
    Length, Task,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct ResourceState {
    /// F02.05: texto do settings.toml (espelho das Configurações) editado no app.
    pub toml: text_editor::Content,
    pub lists: BTreeMap<&'static str, Value>,
    pub snapshot: Option<Value>,
    pub draft: BTreeMap<&'static str, String>,
    /// F07.02: campo da última mudança salva sozinha e o valor anterior, para "Desfazer".
    pub undo: Option<(&'static str, String)>,
    /// F07.03: valores da primeira execução ("valor da instalação"), para restaurar.
    pub defaults: Option<Value>,
    pub busy: bool,
    pub error: String,
    pub sources: Option<Value>,
    pub source_busy: bool,
    pub source_changed: Option<std::time::Instant>,
    pub audio_id: Option<String>,
    pub audio: Option<Value>,
    pub audio_busy: bool,
    pub audio_session: Option<String>,
    pub result: Option<String>,
    pub captions: Vec<String>,
    pub form: Option<Form>,
}
impl Default for ResourceState {
    fn default() -> Self {
        Self {
            toml: text_editor::Content::new(),
            lists: BTreeMap::new(),
            snapshot: None,
            draft: BTreeMap::new(),
            undo: None,
            defaults: None,
            busy: false,
            error: String::new(),
            sources: None,
            source_busy: false,
            source_changed: None,
            audio_id: None,
            audio: None,
            audio_busy: false,
            audio_session: None,
            result: None,
            captions: Vec::new(),
            form: None,
        }
    }
}
#[derive(Clone)]
pub enum Event {
    Load,
    Loaded(Result<Value, String>),
    Input(&'static str, String),
    Toggle(&'static str),
    /// F04.31: liga ou desliga um grupo de avisos no Telegram (índice em `GRUPOS_TELEGRAM`).
    TelegramGrupo(usize),
    Browse(&'static str, bool),
    Chosen(&'static str, Result<Option<String>, String>),
    Save,
    Undo,
    Defaults(Result<Value, String>),
    TomlLoaded(Result<Value, String>),
    TomlEdit(text_editor::Action),
    TomlSave,
    TomlSaved(Result<Value, String>),
    RestoreOption(&'static str),
    RestoreGroup(String),
    Saved(Result<Value, String>, Vec<&'static str>),
    Source(&'static str),
    Sources(std::time::Instant, Result<Value, String>),
    Action(&'static str, String),
    ActionDone(Result<Value, String>),
    Manage(&'static str),
    Managed(&'static str, Result<Value, String>),
    Post(String, Value),
    Review(String, Value),
    Authorized(Result<Value, String>),
    McpEdit(String),
    McpLoaded(String, Result<Value, String>),
    FormBrowse(&'static str, bool),
    FormChosen(&'static str, Result<Option<String>, String>),
    Open(FormKind),
    FormInput(&'static str, String),
    FormSubmit,
    FormRead,
    FormReadDone(Result<Value, String>),
    FormDone(Result<Value, String>),
    Close,
    StartupDone(Result<bool, String>),
    AudioStart(bool),
    AudioStop(bool),
    AudioPoll,
    AudioDone(&'static str, Result<Value, String>),
    AudioStatus(Result<Value, String>),
    Captions(Result<Value, String>),
    TerminalDone(Result<Value, String>),
    Editor(iced::widget::text_editor::Action),
}
impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Recurso([conteúdo oculto])")
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    Mcp,
    Account,
    Calendar,
    Routine,
    Downloads,
    Plugin,
    BackupExport,
    BackupRestore,
    Review,
}
pub struct Form {
    pub method: &'static str,
    pub path: Option<String>,
    pub kind: FormKind,
    pub values: BTreeMap<&'static str, String>,
    pub editor: iced::widget::text_editor::Content,
    pub busy: bool,
    pub error: String,
}
#[derive(Clone, Copy)]
enum InputKind {
    Bool,
    Text,
    Number,
    File,
    Folder,
    List,
}
use InputKind::*;
type Field = (&'static str, &'static str, &'static str, InputKind);
const FIELDS: &[Field] = &[
    ("voice_enabled", "voice", "Voz por clique", Bool),
    ("local_stt_model", "voice", "Modelo Whisper local", File),
    ("local_stt_gpu", "voice", "GPU para transcrição local", Bool),
    ("voice_speak", "voice", "Resposta falada", Bool),
    ("voice_tts_voice", "voice", "Voz do TTS", Text),
    (
        "voice_barge_in",
        "voice",
        "Interromper a fala ao conversar",
        Bool,
    ),
    (
        "wake_device",
        "voice",
        "Nome do microfone (vazio: padrão)",
        Text,
    ),
    ("wake_enabled", "voice", "Palavra “Orion”", Bool),
    (
        "wake_model",
        "voice",
        "Bundle validado da palavra Orion",
        File,
    ),
    (
        "wake_threshold",
        "voice",
        "Sensibilidade da palavra",
        Number,
    ),
    (
        "wake_max_s",
        "voice",
        "Duração máxima da gravação (segundos)",
        Number,
    ),
    (
        "wake_silence_s",
        "voice",
        "Silêncio para encerrar a fala (segundos)",
        Number,
    ),
    (
        "wake_max_per_hour",
        "voice",
        "Limite de ativações por hora",
        Number,
    ),
    ("clap_enabled", "voice", "Duas palmas", Bool),
    ("clap_ratio", "voice", "Sensibilidade das palmas", Number),
    (
        "clap_action",
        "voice",
        "Ação das palmas (abrir ou rotina:nome)",
        Text,
    ),
    (
        "voice_live_enabled",
        "voice",
        "Conversa por voz ao vivo",
        Bool,
    ),
    (
        "voice_live_max_min",
        "voice",
        "Duração máxima ao vivo (minutos)",
        Number,
    ),
    ("vision_tools", "camera", "Descrever imagens e tela", Bool),
    ("camera", "camera", "Câmera sob pedido", Bool),
    ("camera_device", "camera", "Índice da câmera", Number),
    (
        "camera_watch",
        "camera",
        "Lembrar objetos pela câmera",
        Bool,
    ),
    ("object_model", "camera", "Modelo YOLO ONNX", File),
    ("presence_enabled", "camera", "Detecção de presença", Bool),
    (
        "presence_away_min",
        "camera",
        "Ausência antes de pausar (minutos)",
        Number,
    ),
    (
        "presence_bt",
        "camera",
        "Dispositivo Bluetooth de presença",
        Text,
    ),
    (
        "presence_lock",
        "camera",
        "Permitir bloqueio ao se ausentar",
        Bool,
    ),
    ("subtitles", "camera", "Legenda ao vivo", Bool),
    ("subtitle_model", "camera", "Modelo local da legenda", File),
    (
        "subtitle_device",
        "camera",
        "Dispositivo de áudio da legenda",
        Text,
    ),
    ("screen_memory", "privacy", "Memória local da tela", Bool),
    (
        "screen_interval_s",
        "privacy",
        "Intervalo da memória da tela (segundos)",
        Number,
    ),
    (
        "screen_retention_days",
        "privacy",
        "Retenção da memória da tela (dias)",
        Number,
    ),
    (
        "screen_exclude",
        "privacy",
        "Títulos excluídos (separados por ponto e vírgula)",
        Text,
    ),
    (
        "screen_exclude_apps",
        "privacy",
        "Aplicativos excluídos",
        Text,
    ),
    ("screen_ocr_langs", "privacy", "Idiomas do OCR", Text),
    (
        "clipboard_days",
        "privacy",
        "Histórico da área de transferência (dias; 0 desliga)",
        Number,
    ),
    (
        "command_history",
        "privacy",
        "Histórico local de comandos",
        Bool,
    ),
    (
        "jobs_enabled",
        "automation",
        "Trabalhos em segundo plano",
        Bool,
    ),
    ("briefing_at", "automation", "Horário do briefing", Text),
    (
        "research_at",
        "automation",
        "Horário da pesquisa noturna",
        Text,
    ),
    (
        "research_topics",
        "automation",
        "Assuntos da pesquisa (separados por ponto e vírgula)",
        Text,
    ),
    (
        "sleep_at",
        "automation",
        "Horário da revisão da memória",
        Text,
    ),
    ("weekly_ai", "automation", "Revisão semanal com IA", Bool),
    (
        "consolidate",
        "automation",
        "Consolidar fatos das conversas",
        Bool,
    ),
    ("diary_at", "automation", "Horário do diário", Text),
    ("water_every_min", "automation", "Lembrete de água (minutos)", Number),
    ("bedtime_at", "automation", "Hora de dormir", Text),
    ("wake_pc", "automation", "Acordar o PC para agendamentos", Bool),
    (
        "weekly_review_at",
        "automation",
        "Horário da revisão semanal",
        Text,
    ),
    ("nuvem", "automation", "Tarefas nas CLIs de nuvem", Bool),
    ("codex_env", "automation", "Ambiente do Codex Cloud", Text),
    (
        "nuvem_dir",
        "automation",
        "Pasta de trabalho das tarefas",
        Folder,
    ),
    ("vault_dir", "storage", "Pasta do vault", Folder),
    ("backup_dir", "storage", "Pasta dos backups", Folder),
    (
        "backup_keep",
        "storage",
        "Quantidade de backups mantidos",
        Number,
    ),
    (
        "repos",
        "storage",
        "Repositórios (separados por ponto e vírgula)",
        List,
    ),
    (
        "extra_safe_roots",
        "storage",
        "Pastas confiáveis (separadas por ponto e vírgula)",
        List,
    ),
    (
        "rerank",
        "memory",
        "Reordenar resultados com modelo local",
        Bool,
    ),
    (
        "rerank_model_dir",
        "memory",
        "Pasta do reranker ONNX",
        Folder,
    ),
    (
        "mcp_enabled",
        "connectors",
        "Servidores MCP habilitados",
        Bool,
    ),
    (
        "mcp_config",
        "connectors",
        "Arquivo dos servidores MCP",
        File,
    ),
    ("skills_enabled", "connectors", "Skills habilitadas", Bool),
    ("skills_dir", "connectors", "Pasta de skills", Folder),
    ("plugins_enabled", "connectors", "Plugins habilitados", Bool),
    ("plugins_dir", "connectors", "Pasta de plugins", Folder),
    ("telegram_events", "connectors", "Avisos enviados ao Telegram", Text),
];
pub fn load_management(api: &Api, groups: &[&'static str]) -> Task<Msg> {
    Task::batch(groups.iter().map(|&group| {
        let api = api.clone();
        Task::perform(async move { api.get(list_path(group)).await }, move |r| {
            Msg::Resources(Event::Managed(group, r))
        })
    }))
}
pub fn load(api: &Api) -> Task<Msg> {
    let a = api.clone();
    let b = api.clone();
    let c = api.clone();
    let d = api.clone();
    let start = std::time::Instant::now();
    Task::batch([
        Task::perform(async move { a.get("/config/runtime").await }, |r| {
            Msg::Resources(Event::Loaded(r))
        }),
        Task::perform(async move { b.get("/config/sources").await }, move |r| {
            Msg::Resources(Event::Sources(start, r))
        }),
        Task::perform(async move { c.get("/config/runtime/padrao").await }, |r| {
            Msg::Resources(Event::Defaults(r))
        }),
        Task::perform(async move { d.get("/config/toml").await }, |r| {
            Msg::Resources(Event::TomlLoaded(r))
        }),
    ])
}
fn value_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        text.into()
    } else if let Some(array) = value.as_array() {
        array
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("; ")
    } else if value.is_null() {
        String::new()
    } else {
        value.to_string()
    }
}
/// F04.31: grupos de avisos que podem ir ao Telegram e os tipos de cada um (`ops.notify`).
pub const GRUPOS_TELEGRAM: [(&str, &[&str]); 8] = [
    ("Lembretes e timers", &["lembrete"]),
    ("Agendamentos", &["agendamento"]),
    ("Rotinas", &["rotina"]),
    ("Tarefas em segundo plano", &["tarefa_bg"]),
    ("Cota e custos", &["cota"]),
    ("Briefing e resumos", &["briefing", "resumo_diario", "semanal"]),
    ("Água e hora de dormir", &["bem_estar"]),
    ("Memória e outros avisos", &["sono", "sonho", "agente", "gateway", "processo", "proposta", "preco", "vigilancia"]),
];

/// Filtro do Telegram: (só estes, todos menos estes). Vazio nos dois = todos.
fn filtro_telegram(app: &App) -> (Vec<String>, Vec<String>) {
    let v = input_value(app, "telegram_events");
    let tipos = v.split(',').map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty());
    let (fora, so): (Vec<String>, Vec<String>) = tipos.partition(|t| t.starts_with('-'));
    (so, fora.into_iter().map(|t| t[1..].to_string()).collect())
}

/// O grupo vai ao Telegram (todos os tipos dele passam no filtro).
pub fn grupo_no_telegram(app: &App, i: usize) -> bool {
    let (so, fora) = filtro_telegram(app);
    GRUPOS_TELEGRAM.get(i).is_some_and(|g| g.1.iter().all(|t| (so.is_empty() || so.iter().any(|x| x == t)) && !fora.iter().any(|x| x == t)))
}

fn input_value(app: &App, key: &'static str) -> String {
    app.resources.draft.get(key).cloned().unwrap_or_else(|| {
        value_text(
            &app.resources
                .snapshot
                .as_ref()
                .map(|v| v["saved"][key].clone())
                .unwrap_or(Value::Null),
        )
    })
}
/// F07.03: descrição curta de cada opção de Configurações (texto pt-BR; a interface traduz via catálogo).
const DESCRIPTIONS: &[(&str, &str)] = &[
    ("voice_enabled", "Ativa o botão de microfone para ditar mensagens."),
    ("local_stt_model", "Modelo local que transcreve a fala neste computador."),
    ("local_stt_gpu", "Usa a GPU na transcrição local, quando disponível."),
    ("voice_speak", "O Orion responde em voz alta."),
    ("voice_tts_voice", "Voz usada na resposta falada."),
    ("voice_barge_in", "Permite interromper a fala do Orion falando por cima."),
    ("wake_device", "Microfone da palavra de ativação. Vazio usa o padrão."),
    ("wake_enabled", "Escuta local da palavra “Orion” para começar a falar."),
    ("wake_model", "Arquivo do modelo validado que reconhece a palavra."),
    ("wake_threshold", "Quanto maior, mais difícil a palavra disparar sem querer."),
    ("wake_max_s", "Tempo máximo gravado depois da palavra."),
    ("wake_silence_s", "Quanto silêncio encerra a fala."),
    ("wake_max_per_hour", "Quantas ativações por hora são permitidas."),
    ("clap_enabled", "Duas palmas abrem o Orion."),
    ("clap_ratio", "Quanto maior, menos sensível às palmas."),
    ("clap_action", "O que as palmas fazem: abrir o Orion ou rodar uma rotina (rotina:nome)."),
    ("voice_live_enabled", "Permite conversar por voz em tempo real."),
    ("voice_live_max_min", "Tempo máximo de uma conversa por voz ao vivo."),
    ("vision_tools", "Permite ao Orion descrever imagens e a tela."),
    ("camera", "A câmera só é usada quando você pede."),
    ("camera_device", "Qual câmera usar, pelo índice (0 é a padrão)."),
    ("camera_watch", "Guarda objetos vistos pela câmera para lembrar depois."),
    ("object_model", "Modelo ONNX que detecta objetos nas imagens."),
    ("presence_enabled", "Detecta se você está presente pelo computador."),
    ("presence_away_min", "Minutos de ausência antes de pausar."),
    ("presence_bt", "Dispositivo Bluetooth que indica a presença."),
    ("presence_lock", "Permite bloquear o computador ao se ausentar."),
    ("subtitles", "Mostra legenda ao vivo do áudio do computador."),
    ("subtitle_model", "Modelo local usado nas legendas."),
    ("subtitle_device", "Dispositivo de áudio capturado para a legenda."),
    ("screen_memory", "Guarda o texto da tela só neste computador."),
    ("screen_interval_s", "Intervalo entre capturas da tela, em segundos."),
    ("screen_retention_days", "Por quantos dias a memória da tela é mantida."),
    ("screen_exclude", "Títulos de janela que não entram na memória da tela."),
    ("screen_exclude_apps", "Aplicativos que não entram na memória da tela."),
    ("screen_ocr_langs", "Idiomas usados ao ler texto da tela."),
    ("clipboard_days", "Por quantos dias o histórico fica guardado. 0 desliga."),
    ("command_history", "Guarda o histórico de comandos só neste computador."),
    ("jobs_enabled", "Liga as tarefas que rodam em segundo plano."),
    ("briefing_at", "Horário do resumo diário."),
    ("research_at", "Horário da pesquisa noturna sobre os assuntos."),
    ("research_topics", "Assuntos pesquisados à noite."),
    ("sleep_at", "Horário em que a memória é revisada."),
    ("weekly_ai", "Faz uma revisão semanal com IA."),
    ("consolidate", "Consolida os fatos extraídos das conversas."),
    ("diary_at", "Horário em que o diário da conversa é gravado no vault."),
    ("weekly_review_at", "Horário da revisão semanal."),
    ("nuvem", "Permite rodar tarefas nas CLIs de nuvem."),
    ("codex_env", "Ambiente usado pelo Codex Cloud."),
    ("nuvem_dir", "Pasta onde as tarefas de nuvem trabalham."),
    ("vault_dir", "Pasta do vault do Obsidian. Vale depois de reiniciar o Orion."),
    ("backup_dir", "Pasta onde os backups são guardados."),
    ("backup_keep", "Quantos backups antigos manter."),
    ("repos", "Repositórios que o Orion pode ler."),
    ("extra_safe_roots", "Pastas onde o Orion pode ler e escrever."),
    ("rerank", "Reordena resultados de busca com um modelo local."),
    ("rerank_model_dir", "Pasta do modelo ONNX de reordenação."),
    ("mcp_enabled", "Liga os servidores MCP configurados."),
    ("water_every_min", "Lembra de beber água nesse intervalo, das 8h às 22h. 0 desliga. Não perturbe segura o aviso."),
    ("bedtime_at", "Avisa 30 minutos antes deste horário (HH:MM). Vazio desliga."),
    ("wake_pc", "Tira o PC da suspensão um minuto antes do próximo agendamento ou lembrete. Precisa dos timers de despertar permitidos no plano de energia."),
    ("telegram_events", "Tipos separados por vírgula (lembrete, agendamento, cota…); com \"-\" na frente, todos menos esse (-rotina). Vazio envia todos. Também dá para escolher em Notificações."),
    ("mcp_config", "Arquivo com a configuração dos servidores MCP."),
    ("skills_enabled", "Liga as skills instaladas."),
    ("skills_dir", "Pasta onde ficam as skills."),
    ("plugins_enabled", "Liga os plugins instalados."),
    ("plugins_dir", "Pasta onde ficam os plugins."),
];

/// Descrição curta da opção, já traduzida.
fn description(key: &str) -> &'static str {
    DESCRIPTIONS.iter().find(|(k, _)| *k == key).map(|(_, text)| crate::i18n::text(text)).unwrap_or("")
}

pub fn settings<'a>(app: &'a App, group: &str) -> El<'a> {
    let mut col = column![].spacing(10);
    if group == "storage" {
        col = col.push(tx(crate::i18n::text("settings.toml: espelho das Configurações, sem segredos. Salvar aplica na hora; editar o arquivo à mão também aplica."), 12.5, t::FG3));
        col = col.push(
            text_editor(&app.resources.toml)
                .on_action(|action| Msg::Resources(Event::TomlEdit(action)))
                .font(t::MONO_MED)
                .size(12.5)
                .height(Length::Fixed(240.0))
                .padding(10),
        );
        col = col.push(btn(
            crate::i18n::text("Salvar settings.toml"),
            Kind::Primary,
            false,
            None,
            (!app.resources.busy).then_some(Msg::Resources(Event::TomlSave)),
        ));
    }
    col=col.push(tx(crate::i18n::text("As escolhas ficam salvas neste computador. Use Salvar e aplicar para atualizar os recursos."),12.5,t::FG3));
    if app.resources.snapshot.is_none() {
        return column![
            tx(crate::i18n::text("Carregando recursos…"), 13.0, t::FG3),
            btn(
                crate::i18n::text("Atualizar"),
                Kind::Normal,
                true,
                None,
                Some(Msg::Resources(Event::Load))
            )
        ]
        .spacing(10)
        .into();
    }
    col = col.push(btn(
        crate::i18n::text(if app.resources.busy {
            "Aplicando…"
        } else {
            "Salvar e aplicar"
        }),
        Kind::Primary,
        false,
        None,
        (!app.resources.busy).then_some(Msg::Resources(Event::Save)),
    ));
    if app.resources.undo.is_some() {
        col = col.push(btn(
            crate::i18n::text("Desfazer última mudança"),
            Kind::Ghost,
            false,
            None,
            (!app.resources.busy).then_some(Msg::Resources(Event::Undo)),
        ));
    }
    if app.resources.defaults.is_some() {
        col = col.push(btn(
            crate::i18n::text("Restaurar padrão desta seção"),
            Kind::Ghost,
            false,
            None,
            (!app.resources.busy).then(|| Msg::Resources(Event::RestoreGroup(group.to_string()))),
        ));
    }
    for &(key, _, label, kind) in FIELDS.iter().filter(|f| f.1 == group) {
        let text = input_value(app, key);
        let control: El = match kind {
            Bool if key == "local_stt_gpu"
                && !app.resources.snapshot.as_ref().unwrap()["availability"]["stt_gpu"]
                    .as_bool()
                    .unwrap_or(false) =>
            {
                badge(
                    crate::i18n::text("GPU de transcrição indisponível neste pacote"),
                    Tone::Plain,
                )
            }
            Bool => switch(text == "true", Msg::Resources(Event::Toggle(key))),
            _ => {
                let mut input = text_input("", &text).size(13).font(t::UI).padding(8).style(
                    crate::views::login::field_style(t::RAISE, t::LINE2, t::ACCENT, 8.0),
                );
                if !app.resources.busy {
                    input = input
                        .on_input(move |text| Msg::Resources(Event::Input(key, text)))
                        .on_submit(Msg::Resources(Event::Save));
                }
                let mut controls = row![container(input).width(Length::Fixed(230.0))].spacing(6);
                if matches!(kind, File | Folder) {
                    controls = controls.push(btn(
                        crate::i18n::text("Escolher"),
                        Kind::Normal,
                        true,
                        None,
                        (!app.resources.busy)
                            .then_some(Msg::Resources(Event::Browse(key, matches!(kind, Folder)))),
                    ));
                }
                controls.into()
            }
        };
        // F07.03: "Restaurar" por opção só aparece quando o valor difere da primeira execução.
        let default_value = app.resources.defaults.as_ref().and_then(|d| d.get(key));
        let differs = default_value.is_some_and(|v| value_text(v) != input_value(app, key));
        let control: El = if differs {
            row![control, btn(crate::i18n::text("Restaurar"), Kind::Ghost, false, None, (!app.resources.busy).then_some(Msg::Resources(Event::RestoreOption(key))))].spacing(6).align_y(iced::alignment::Vertical::Center).into()
        } else {
            control
        };
        col = col.push(setting(crate::i18n::text(label), description(key), control));
    }
    let applied = app.resources.snapshot.as_ref().unwrap();
    if applied["saved"] != applied["active"] {
        col = col.push(tx(
            crate::i18n::text("Há alterações salvas aguardando aplicação."),
            13.0,
            t::SIGNAL,
        ));
    }
    let availability = &applied["availability"];
    for (key, label) in match group {
        "voice" => vec![
            ("voice", "Transcrição"),
            ("speaker", "Resposta falada"),
            ("wake", "Palavra de ativação"),
        ],
        "camera" => vec![("camera", "Descrição da câmera")],
        "automation" => vec![("jobs", "Agendas"), ("research", "Pesquisa noturna")],
        _ => vec![],
    } {
        col = col.push(setting(
            crate::i18n::text(label),
            "",
            badge(
                crate::i18n::text(if availability[key].as_bool().unwrap_or(false) {
                    "Disponível"
                } else {
                    "Falta configurar"
                }),
                Tone::Plain,
            ),
        ));
    }
    let action = |label: &'static str, method, path: &'static str| {
        btn(
            crate::i18n::text(label),
            Kind::Normal,
            true,
            None,
            Some(Msg::Resources(Event::Action(method, path.into()))),
        )
    };
    let form = |label: &'static str, kind| {
        btn(
            crate::i18n::text(label),
            Kind::Normal,
            true,
            None,
            Some(Msg::Resources(Event::Open(kind))),
        )
    };
    match group {
        "voice" => {
            col=col.push(tx(crate::i18n::text("Transcrição exige Whisper local ou fallback Groq configurado. A palavra Orion exige bundle validado. A fala usa uma voz do Windows ou edge-tts."),12.5,t::FG3));
            col = col.push(
                row![
                    btn(
                        crate::i18n::text("Testar microfone"),
                        Kind::Normal,
                        true,
                        None,
                        Some(Msg::Mic)
                    ),
                    btn(
                        crate::i18n::text("Conversa ao vivo"),
                        Kind::Normal,
                        true,
                        None,
                        Some(Msg::Live)
                    )
                ]
                .spacing(8),
            );
        }
        "camera" => {
            col = col.push(
                row![
                    btn(if app.camera.is_some() { crate::i18n::text("Desligar a câmera") } else { crate::i18n::text("Abrir a câmera") }, Kind::Primary, true, None, Some(Msg::Camera(if app.camera.is_some() { crate::camera_ui::CameraMsg::Close } else { crate::camera_ui::CameraMsg::Open }))),
                    action("Cadastrar meu rosto", "POST", "/resources/face/enroll"),
                    action("Apagar meu rosto", "POST", "/resources/face/delete")
                ]
                .spacing(8),
            );
            col=col.push(tx(crate::i18n::text("O cadastro tira cinco fotos e guarda somente o vetor cifrado. Este rosto serve para presença; não aprova ações."),12.5,t::FG3));
            // Face ID: entrar e confirmar pedidos de senha pelo rosto (opt-in; a senha segue valendo)
            let face_on = app.face_id_enabled();
            col = col.push(
                row![
                    if face_on {
                        btn(crate::i18n::text("Cadastrar o Face ID de novo"), Kind::Normal, true, None, (!app.island.busy()).then_some(Msg::Face(crate::island::FaceMsg::EnrollAsk)))
                    } else {
                        btn(crate::i18n::text("Ativar Face ID"), Kind::Primary, true, None, (!app.island.busy()).then_some(Msg::Face(crate::island::FaceMsg::EnrollAsk)))
                    },
                    if face_on { btn(crate::i18n::text("Desligar Face ID"), Kind::Normal, true, None, Some(Msg::Face(crate::island::FaceMsg::Disable))) } else { sp_w(0.0) }
                ]
                .spacing(8),
            );
            col=col.push(tx(crate::i18n::text("Face ID: entra no Orion e confirma pedidos de senha (modo autônomo, sair do pânico) olhando para a câmera. Tudo roda neste computador; com câmera indisponível ou pouca luz, a senha continua valendo. Aprovar ferramentas segue sendo só por botão."),12.5,t::FG3));
            col = col.push(
                row![
                    action("Fotografar e descrever", "POST", "/camera/ver"),
                    action("Descrever a tela", "POST", "/resources/screen")
                ]
                .spacing(8),
            );
        }
        "automation" => {
            col = col
                .push(management(app, "routines"))
                .push(management(app, "downloads"));
            col = col.push(
                row![
                    form("Criar rotina", FormKind::Routine),
                    form("Regras de Downloads", FormKind::Downloads)
                ]
                .spacing(8),
            );
            col=col.push(tx(crate::i18n::text("Horários usam HH:MM; vazio desliga a agenda. Pesquisa noturna precisa do vault e de ferramentas web."),12.5,t::FG3));
        }
        "privacy" => {
            col = col.push(
                row![
                    action(
                        "Ver histórico da área de transferência",
                        "GET",
                        "/area-transferencia"
                    ),
                    action("Ver histórico de comandos", "GET", "/comandos")
                ]
                .spacing(8),
            );
        }
        "storage" => {
            col = col.push(
                row![
                    form("Exportar backup", FormKind::BackupExport),
                    form("Restaurar backup", FormKind::BackupRestore)
                ]
                .spacing(8),
            );
        }
        "connectors" => {
            for group in ["mcp", "accounts", "calendar", "plugins"] {
                col = col.push(management(app, group));
            }
            col = col.push(
                row![
                    form("Adicionar servidor MCP", FormKind::Mcp),
                    form("Conectar conta", FormKind::Account)
                ]
                .spacing(8),
            );
            col = col.push(
                row![
                    form("Vincular calendário", FormKind::Calendar),
                    form("Importar plugin", FormKind::Plugin)
                ]
                .spacing(8),
            );
        }
        _ => {}
    }
    if !app.resources.error.is_empty() {
        col = col.push(tx(
            crate::i18n::status(&app.resources.error),
            13.0,
            t::DANGER,
        ));
    }
    col.push(btn(
        crate::i18n::text(if app.resources.busy {
            "Aplicando…"
        } else {
            "Salvar e aplicar"
        }),
        Kind::Primary,
        false,
        None,
        (!app.resources.busy).then_some(Msg::Resources(Event::Save)),
    ))
    .into()
}
impl App {
    /// Envia ao servidor o rascunho, ou só as chaves pedidas (autosave salva apenas o campo alterado).
    fn save_draft(&mut self, only: Option<&[&'static str]>) -> Task<Msg> {
        let Some(api) = self.api.clone() else {
            self.resources.error = "Configuração disponível somente no app conectado.".into();
            return Task::none();
        };
        let mut patch = serde_json::Map::new();
        let mut sent: Vec<&'static str> = Vec::new();
        for (&key, text) in &self.resources.draft {
            if only.is_some_and(|keys| !keys.contains(&key)) {
                continue;
            }
            let Some(field) = FIELDS.iter().find(|f| f.0 == key) else {
                continue;
            };
            let value = match field.3 {
                Bool => json!(text == "true"),
                Number => match text.parse::<serde_json::Number>() {
                    Ok(n) => Value::Number(n),
                    Err(_) => {
                        self.resources.error = format!("{}: valor numérico inválido", field.2);
                        return Task::none();
                    }
                },
                File if text.trim().is_empty() => Value::Null,
                Folder if text.trim().is_empty() && !["nuvem_dir", "rerank_model_dir"].contains(&key) => Value::Null,
                List => json!(text.split(';').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>()),
                _ => json!(text.trim()),
            };
            patch.insert(key.into(), value);
            sent.push(key);
        }
        self.resources.busy = true;
        self.resources.error.clear();
        // A pasta do vault só vale depois de reiniciar o Orion: o aviso sai junto com os outros.
        let vault_changed = patch.contains_key("vault_dir");
        Task::perform(
            async move {
                api.put("/config/runtime", Value::Object(patch)).await?;
                let applied = api.reconfigure().await.map_err(|e| format!("Configuração salva. {e}"))?;
                let mut snapshot = api.get("/config/runtime").await?;
                let mut warnings = applied["warnings"].as_array().cloned().unwrap_or_default();
                if vault_changed {
                    warnings.push(Value::String("Pasta do vault salva. Reinicie o Orion para usar a nova pasta.".into()));
                }
                snapshot["warnings"] = Value::Array(warnings);
                Ok(snapshot)
            },
            move |r| Msg::Resources(Event::Saved(r, sent)),
        )
    }

    fn sync_resources(&mut self, value: &Value) {
        let a = &value["active"];
        self.p.wake = a["wake_enabled"].as_bool().unwrap_or(false);
        self.p.clap = a["clap_enabled"].as_bool().unwrap_or(false);
        for (i, key) in [
            "camera",
            "presence_enabled",
            "camera_watch",
            "presence_enabled",
            "subtitles",
        ]
        .iter()
        .enumerate()
        {
            self.p.cam[i] = a[*key].as_bool().unwrap_or(false);
        }
    }

    pub fn update_resources(&mut self, event: Event) -> Task<Msg> {
        match event {
            Event::StartupDone(result) => match result {
                Ok(on) => self.p.boot = on,
                Err(e) => self.toast(&e),
            },
            Event::AudioStart(live) => {
                if self.resources.audio_busy {
                    return Task::none();
                }
                let Some(api) = self.api.clone() else {
                    return Task::none();
                };
                self.resources.audio_busy = true;
                self.resources.audio_session = self.session_id();
                return Task::perform(
                    async move {
                        api.post("/resources/audio/start", json!({"live":live}))
                            .await
                    },
                    |r| Msg::Resources(Event::AudioDone("start", r)),
                );
            }
            Event::AudioStop(cancel) => {
                if self.resources.audio_busy && !cancel {
                    return Task::none();
                }
                let Some(api) = self.api.clone() else {
                    return Task::none();
                };
                self.resources.audio_busy = true;
                return Task::perform(
                    async move {
                        api.post(
                            if cancel {
                                "/resources/audio/cancel"
                            } else {
                                "/resources/audio/finish"
                            },
                            json!({}),
                        )
                        .await
                    },
                    move |r| {
                        Msg::Resources(Event::AudioDone(
                            if cancel { "cancel" } else { "finish" },
                            r,
                        ))
                    },
                );
            }
            Event::AudioDone(action, result) => {
                if !self.logged {
                    self.resources.audio_busy = false;
                    self.mic_rec = false;
                    self.voice = false;
                    if action == "start" {
                        return self.update_resources(Event::AudioStop(true));
                    }
                    return Task::none();
                }
                self.resources.audio_busy = false;
                match result {
                    Ok(value) => {
                        if action == "start" {
                            self.resources.audio_id =
                                value["capture_id"].as_str().map(String::from);
                        } else if action == "cancel" {
                            self.resources.audio_id = None;
                        }
                        if action != "finish" {
                            self.mic_rec =
                                action == "start" && !value["live"].as_bool().unwrap_or(false);
                        }
                        self.voice = action == "start" && value["live"].as_bool().unwrap_or(false);
                        if action == "finish" {
                            return self.update_resources(Event::AudioStatus(Ok(value)));
                        }
                        self.resources.audio = Some(value);
                    }
                    Err(e) => {
                        self.mic_rec = false;
                        self.voice = false;
                        self.resources.error = e.clone();
                        self.toast(&e);
                    }
                }
            }
            Event::AudioPoll => {
                let Some(api) = self.api.clone() else {
                    return Task::none();
                };
                let captions = self.p.cam[4];
                let live = self.voice || self.mic_rec;
                let a = api.clone();
                return Task::batch([
                    if live {
                        Task::perform(async move { a.get("/resources/audio").await }, |r| {
                            Msg::Resources(Event::AudioStatus(r))
                        })
                    } else {
                        Task::none()
                    },
                    if captions {
                        Task::perform(async move { api.get("/legenda").await }, |r| {
                            Msg::Resources(Event::Captions(r))
                        })
                    } else {
                        Task::none()
                    },
                ]);
            }
            Event::AudioStatus(result) => match result {
                Ok(value) => {
                    if !self.logged
                        || self.resources.audio_id.as_deref() != value["capture_id"].as_str()
                    {
                        return Task::none();
                    }
                    let status = value["status"].as_str().unwrap_or("");
                    if status == "done" && self.mic_rec {
                        self.mic_rec = false;
                        if let Some(text) = value["transcript"].as_str().filter(|s| !s.is_empty()) {
                            if self.resources.audio_session == self.session_id() {
                                let draft = self.draft();
                                self.composer =
                                    iced::widget::text_editor::Content::with_text(&format!(
                                        "{}{}{}",
                                        draft,
                                        if draft.is_empty() { "" } else { " " },
                                        text
                                    ));
                            } else {
                                self.resources.result = Some(text.into());
                            }
                        }
                    }
                    if matches!(status, "error" | "stopped" | "idle") {
                        self.mic_rec = false;
                        self.voice = false;
                        if let Some(error) = value["error"].as_str() {
                            self.resources.error = error.into();
                            self.toast(error);
                        }
                    }
                    self.resources.audio = Some(value);
                }
                Err(e) => {
                    self.resources.error = e;
                }
            },
            Event::Load => return self.api.as_ref().map(load).unwrap_or(Task::none()),
            Event::Loaded(result) => match result {
                Ok(value) => {
                    self.sync_resources(&value);
                    self.resources.snapshot = Some(value)
                }
                Err(e) => self.resources.error = e,
            },
            Event::Input(key, text) if !self.resources.busy => {
                self.resources.draft.insert(key, text);
            }
            Event::Toggle(key) if !self.resources.busy => {
                // F07.02: o toggle salva sozinho e guarda o valor anterior para desfazer.
                let on = input_value(self, key) == "true";
                self.resources.undo = Some((key, on.to_string()));
                self.resources.draft.insert(key, (!on).to_string());
                return self.save_draft(Some(&[key]));
            }
            Event::TelegramGrupo(i) if !self.resources.busy => {
                let Some((_, grupo)) = GRUPOS_TELEGRAM.get(i) else { return Task::none() };
                let ligado = grupo_no_telegram(self, i);
                let anterior = input_value(self, "telegram_events");
                let (mut so, mut fora) = filtro_telegram(self);
                let novo = if so.is_empty() {
                    // lista de exclusão: tipo que não está na grade (conta, foco, presença…) continua indo
                    fora.retain(|t| !grupo.contains(&t.as_str()));
                    if ligado {
                        fora.extend(grupo.iter().map(|t| t.to_string()));
                    }
                    fora.iter().map(|t| format!("-{t}")).collect::<Vec<_>>().join(",")
                } else {
                    // lista "só estes" escrita à mão: continua nesse formato
                    so.retain(|t| !grupo.contains(&t.as_str()));
                    if !ligado {
                        so.extend(grupo.iter().map(|t| t.to_string()));
                    }
                    if so.is_empty() {
                        so.push("nenhum".into());
                    }
                    // as exclusões escritas junto continuam valendo
                    so.into_iter().chain(fora.iter().map(|t| format!("-{t}"))).collect::<Vec<_>>().join(",")
                };
                self.resources.undo = Some(("telegram_events", anterior));
                self.resources.draft.insert("telegram_events", novo);
                return self.save_draft(Some(&["telegram_events"]));
            }
            Event::Defaults(Ok(values)) => self.resources.defaults = Some(values),
            Event::Defaults(Err(_)) => {}
            Event::TomlLoaded(Ok(body)) => {
                let text = body["content"].as_str().unwrap_or_default();
                self.resources.toml = text_editor::Content::with_text(text);
            }
            Event::TomlLoaded(Err(_)) => {}
            Event::TomlEdit(action) => self.resources.toml.perform(action),
            Event::TomlSave if !self.resources.busy => {
                let Some(api) = self.api.clone() else {
                    self.resources.error = "Configuração disponível somente no app conectado.".into();
                    return Task::none();
                };
                let content = self.resources.toml.text();
                self.resources.busy = true;
                // Salvar o TOML aplica na hora: grava e depois reconfigura, como o botão Salvar e aplicar.
                return Task::perform(
                    async move {
                        api.put("/config/toml", json!({"content": content})).await?;
                        api.reconfigure().await.map(|_| Value::Null)
                    },
                    |r| Msg::Resources(Event::TomlSaved(r)),
                );
            }
            Event::TomlSaved(Ok(_)) => {
                self.resources.busy = false;
                self.toast(crate::i18n::text("Salvo"));
                return Task::done(Msg::Resources(Event::Load));
            }
            Event::TomlSaved(Err(e)) => {
                self.resources.busy = false;
                self.resources.error = e;
            }
            Event::RestoreOption(key) if !self.resources.busy => {
                if let Some(default_value) = self.resources.defaults.as_ref().and_then(|d| d.get(key)).cloned() {
                    let previous = input_value(self, key);
                    self.resources.undo = Some((key, previous));
                    self.resources.draft.insert(key, value_text(&default_value));
                    return self.save_draft(Some(&[key]));
                }
            }
            Event::RestoreGroup(group) if !self.resources.busy => {
                if let Some(defaults) = self.resources.defaults.clone() {
                    let mut keys = Vec::new();
                    for &(key, _, _, _) in FIELDS.iter().filter(|f| f.1 == group.as_str()) {
                        if let Some(default_value) = defaults.get(key) {
                            self.resources.draft.insert(key, value_text(default_value));
                            keys.push(key);
                        }
                    }
                    self.resources.undo = None;
                    return self.save_draft(Some(&keys));
                }
            }
            Event::Undo if !self.resources.busy => {
                if let Some((key, previous)) = self.resources.undo.take() {
                    self.resources.draft.insert(key, previous);
                    return self.save_draft(Some(&[key]));
                }
            }
            Event::Browse(key, folder) if !self.resources.busy => {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::onboarding::choose(if folder {
                                crate::onboarding::Field::Vault
                            } else {
                                crate::onboarding::Field::ResourceFile
                            })
                        })
                        .await
                        .map_err(|_| "Não foi possível abrir o seletor.".to_string())
                        .and_then(|r| r)
                    },
                    move |r| Msg::Resources(Event::Chosen(key, r)),
                )
            }
            Event::Chosen(key, Ok(Some(path))) => {
                // F07.02: pasta ou arquivo escolhido salva sozinho.
                let previous = input_value(self, key);
                self.resources.undo = Some((key, previous));
                self.resources.draft.insert(key, path);
                return self.save_draft(Some(&[key]));
            }
            Event::Chosen(_, Err(e)) => self.resources.error = e,
            Event::Save if !self.resources.busy => return self.save_draft(None),
            Event::Saved(result, sent) => {
                self.resources.busy = false;
                match result {
                    Ok(value) => {
                        self.sync_resources(&value);
                        self.toast(crate::i18n::text("Salvo"));
                        if let Some(warnings) = value["warnings"].as_array() {
                            self.resources.error = warnings
                                .iter()
                                .filter_map(Value::as_str)
                                .collect::<Vec<_>>()
                                .join("\n");
                        }
                        self.resources.snapshot = Some(value);
                        // Só sai do rascunho o que foi enviado; edições em outros campos continuam.
                        for key in sent {
                            self.resources.draft.remove(key);
                        }
                        self.toast("Recursos aplicados.");
                        let mut tasks = Vec::new();
                        if let Some(api) = self.api.clone() {
                            // Recarrega também o editor do settings.toml, para ele não guardar valores antigos.
                            tasks.push(load(&api));
                            if self.sec == crate::app::Sec::Conectores {
                                tasks.push(crate::live::pull_sec(&api, self.sec));
                            }
                        }
                        return Task::batch(tasks);
                    }
                    Err(e) => self.resources.error = e,
                }
            }
            Event::Source(key) => {
                if self.resources.source_busy {
                    return Task::none();
                }
                let Some(api) = self.api.clone() else {
                    return Task::none();
                };
                let Some(current) = &self.resources.sources else {
                    return load(&api);
                };
                let next = !current[key].as_bool().unwrap_or(true);
                let start = std::time::Instant::now();
                self.resources.source_busy = true;
                self.resources.source_changed = Some(start);
                return Task::perform(
                    async move { api.put("/config/sources", json!({(key):next})).await },
                    move |r| Msg::Resources(Event::Sources(start, r)),
                );
            }
            Event::Sources(start, result) => {
                if self
                    .resources
                    .source_changed
                    .is_some_and(|changed| start < changed)
                {
                    return Task::none();
                }
                if self.resources.source_busy && self.resources.source_changed != Some(start) {
                    return Task::none();
                }
                self.resources.source_changed = Some(std::time::Instant::now());
                self.resources.source_busy = false;
                match result {
                    Ok(value) => {
                        for (i, key) in ["vault", "repo", "calendar", "email", "web"]
                            .into_iter()
                            .enumerate()
                        {
                            self.p.src[i] = value[key].as_bool().unwrap_or(false);
                        }
                        self.resources.sources = Some(value);
                    }
                    Err(e) => {
                        self.resources.error = e.clone();
                        self.toast(&e)
                    }
                }
            }
            Event::Action(method, path) => {
                let Some(api) = self.api.clone() else {
                    return Task::none();
                };
                return Task::perform(
                    async move {
                        if method == "GET" {
                            api.get(&path).await
                        } else {
                            api.post(&path, json!({})).await
                        }
                    },
                    |r| Msg::Resources(Event::ActionDone(r)),
                );
            }
            Event::ActionDone(result) => match result {
                Ok(value) => {
                    self.resources.result =
                        Some(serde_json::to_string_pretty(&value).unwrap_or_default())
                }
                Err(e) => self.resources.error = e,
            },
            Event::TerminalDone(result) => {
                self.resources.busy = false;
                match result {
                    Ok(value) => {
                        for key in ["stdout", "stderr", "saida", "erro"] {
                            if let Some(text) = value[key].as_str() {
                                self.term_out.extend(text.lines().map(str::to_string));
                            }
                        }
                        if value.get("codigo").is_some() {
                            self.term_out.push(format!("Exit: {}", value["codigo"]));
                        }
                    }
                    Err(e) => self.term_out.push(e),
                }
            }
            Event::Manage(group) => {
                let Some(api) = self.api.clone() else {
                    return Task::none();
                };
                let path = list_path(group);
                return Task::perform(async move { api.get(path).await }, move |r| {
                    Msg::Resources(Event::Managed(group, r))
                });
            }
            Event::Managed(group, result) => match result {
                Ok(value) => {
                    self.resources.lists.insert(group, value);
                }
                Err(e) => self.resources.error = e,
            },
            Event::Post(path, body) => {
                let Some(api) = self.api.clone() else {
                    return Task::none();
                };
                let authorize = path.starts_with("/accounts/") && path.ends_with("/authorize");
                return Task::perform(async move { api.post(&path, body).await }, move |r| {
                    Msg::Resources(if authorize {
                        Event::Authorized(r)
                    } else {
                        Event::ActionDone(r)
                    })
                });
            }
            Event::Review(path, value) => {
                self.resources.form = Some(Form {
                    method: "POST",
                    path: Some(path),
                    kind: FormKind::Review,
                    values: Default::default(),
                    editor: iced::widget::text_editor::Content::with_text(
                        &serde_json::to_string_pretty(&value).unwrap_or_default(),
                    ),
                    busy: false,
                    error: String::new(),
                });
            }
            Event::Authorized(result) => match result {
                Ok(value) => {
                    if let Some(url) = value["authorization_url"]
                        .as_str()
                        .and_then(|s| reqwest::Url::parse(s).ok())
                        .filter(|u| {
                            u.scheme() == "https"
                                && u.host_str().is_some()
                                && u.username().is_empty()
                                && u.password().is_none()
                        })
                    {
                        crate::onboarding::open_link(url.as_str());
                    }
                    self.resources.result =
                        Some(serde_json::to_string_pretty(&value).unwrap_or_default());
                }
                Err(e) => self.resources.error = e,
            },
            Event::McpEdit(id) => {
                let Some(api) = self.api.clone() else {
                    return Task::none();
                };
                return Task::perform(
                    async move {
                        let value = api.get(&format!("/resources/mcp/{id}")).await;
                        (id, value)
                    },
                    |(id, r)| Msg::Resources(Event::McpLoaded(id, r)),
                );
            }
            Event::McpLoaded(id, result) => match result {
                Ok(value) => {
                    let _ = self
                        .update_resources(Event::Review(format!("/mcp/connections/{id}"), value));
                    if let Some(form) = &mut self.resources.form {
                        form.method = "PUT";
                    }
                }
                Err(e) => self.resources.error = e,
            },
            Event::FormBrowse(key, folder) => {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::onboarding::choose(if folder {
                                crate::onboarding::Field::Vault
                            } else {
                                crate::onboarding::Field::ResourceFile
                            })
                        })
                        .await
                        .map_err(|_| "Seletor indisponível.".to_string())
                        .and_then(|r| r)
                    },
                    move |r| Msg::Resources(Event::FormChosen(key, r)),
                )
            }
            Event::FormChosen(key, Ok(Some(path))) => {
                if let Some(form) = &mut self.resources.form {
                    form.values.insert(key, path);
                }
            }
            Event::FormChosen(_, Err(e)) => {
                if let Some(form) = &mut self.resources.form {
                    form.error = e;
                }
            }
            Event::Open(kind) => {
                self.resources.result = None;
                let (body, _, _) = form_spec(kind);
                let mut values = BTreeMap::new();
                values.insert(
                    "name",
                    if kind == FormKind::Downloads {
                        "downloads".into()
                    } else {
                        String::new()
                    },
                );
                self.resources.form = Some(Form {
                    method: "POST",
                    path: None,
                    kind,
                    values,
                    editor: iced::widget::text_editor::Content::with_text(&body),
                    busy: false,
                    error: String::new(),
                });
            }
            Event::FormInput(key, value) => {
                if let Some(form) = self.resources.form.as_mut().filter(|f| !f.busy) {
                    form.values.insert(key, value);
                }
            }
            Event::Editor(action) => {
                if let Some(form) = self.resources.form.as_mut().filter(|f| !f.busy) {
                    form.editor.perform(action);
                }
            }
            Event::FormRead => {
                let (Some(api), Some(form)) = (self.api.clone(), self.resources.form.as_mut())
                else {
                    return Task::none();
                };
                if form.busy {
                    return Task::none();
                }
                form.busy = true;
                let name = form.values.get("name").cloned().unwrap_or_default();
                return Task::perform(
                    async move {
                        let mut url =
                            reqwest::Url::parse("http://127.0.0.1/resources/automations/").unwrap();
                        url.path_segments_mut().unwrap().push(&name);
                        api.get(url.path()).await
                    },
                    |r| Msg::Resources(Event::FormReadDone(r)),
                );
            }
            Event::FormReadDone(result) => {
                if let Some(form) = self.resources.form.as_mut() {
                    form.busy = false;
                    match result {
                        Ok(value) => {
                            form.editor = iced::widget::text_editor::Content::with_text(
                                value["text"].as_str().unwrap_or(""),
                            );
                            form.values
                                .insert("hash", value["hash"].as_str().unwrap_or("").into());
                        }
                        Err(e) => form.error = e,
                    }
                }
            }
            Event::FormSubmit => {
                let (Some(api), Some(form)) = (self.api.clone(), self.resources.form.as_mut())
                else {
                    return Task::none();
                };
                if form.busy {
                    return Task::none();
                }
                let (_, _, default_path) = form_spec(form.kind);
                let path = form.path.clone().unwrap_or(default_path.into());
                let method = form.method;
                let body = if matches!(form.kind, FormKind::Plugin | FormKind::BackupExport) {
                    if form.kind == FormKind::Plugin {
                        json!({"folder":form.values.get("folder").cloned().unwrap_or_default(),"update":form.values.get("update").is_some_and(|v|v=="true")})
                    } else {
                        json!({"folder":form.values.get("folder").cloned().unwrap_or_default()})
                    }
                } else if form.kind == FormKind::BackupRestore {
                    json!({"file":form.values.get("file"),"folder":form.values.get("folder"),"password":form.values.get("password"),"new_password":form.values.get("new_password"),"recovery":form.values.get("recovery").is_some_and(|v|v=="true")})
                } else if matches!(form.kind, FormKind::Routine | FormKind::Downloads) {
                    json!({"name":form.values.get("name").cloned().unwrap_or_default(),"text":form.editor.text(),"expected_hash":form.values.get("hash").cloned().unwrap_or_default()})
                } else {
                    match serde_json::from_str::<Value>(&form.editor.text()) {
                        Ok(v) => v,
                        Err(e) => {
                            form.error = format!("JSON inválido: {e}");
                            return Task::none();
                        }
                    }
                };
                form.busy = true;
                form.error.clear();
                return Task::perform(
                    async move {
                        if method == "PUT" {
                            api.put(&path, body).await
                        } else {
                            api.post(&path, body).await
                        }
                    },
                    |r| Msg::Resources(Event::FormDone(r)),
                );
            }
            Event::FormDone(result) => {
                if let Some(form) = self.resources.form.as_mut() {
                    form.busy = false;
                    match result {
                        Ok(value) => {
                            if let Some(hash) = value["hash"].as_str() {
                                form.values.insert("hash", hash.into());
                            }
                            self.resources.result =
                                Some(serde_json::to_string_pretty(&value).unwrap_or_default());
                        }
                        Err(e) => form.error = e,
                    }
                }
            }
            Event::Close => {
                self.resources.form = None;
                self.resources.result = None;
            }
            Event::Captions(Ok(value)) => {
                self.resources.captions = value["linhas"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| {
                                v["texto"]
                                    .as_str()
                                    .or_else(|| v["text"].as_str())
                                    .map(str::to_string)
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Event::Captions(Err(e)) => self.resources.error = e,
            _ => {}
        }
        Task::none()
    }
}

fn form_spec(kind: FormKind) -> (String, &'static str, &'static str) {
    let (value, title, path) = match kind {
        FormKind::Review => (json!({}), "Revisar e ativar", ""),
        FormKind::Mcp => (
            json!({"id":"meu-servidor","transport":"http","url":"https://servidor.exemplo/mcp","scope":"personal","enabled":true,"authorized":true,"resources":[],"prompts":[],"classifications":{}}),
            "Adicionar servidor MCP",
            "/mcp/connections",
        ),
        FormKind::Account => (
            json!({"name":"Minha conta","url":"https://servidor.exemplo/mcp","scope":"personal","scopes":["read"]}),
            "Conectar conta",
            "/accounts",
        ),
        FormKind::Calendar => (
            json!({"scope":"personal","connection_id":"meu-servidor","account":"principal","calendar_id":"primary","timezone":"America/Sao_Paulo","reviewed_read_only":true}),
            "Vincular calendário",
            "/calendar/bind",
        ),
        FormKind::Plugin => (
            json!({"folder":"","update":false}),
            "Importar plugin",
            "/plugins/install",
        ),
        FormKind::BackupExport => (
            json!({"folder":""}),
            "Exportar backup",
            "/resources/backup/export",
        ),
        FormKind::BackupRestore => (
            json!({"file":"","folder":"","password":"","new_password":"","recovery":false}),
            "Restaurar backup",
            "/resources/backup/restore",
        ),
        FormKind::Routine => {
            return (
                "descricao: Minha rotina\ngatilhos: []\nliberar: []\npassos:\n  - briefing: true\n"
                    .into(),
                "Criar rotina",
                "/resources/automations",
            )
        }
        FormKind::Downloads => {
            return (
                "regras:\n  - extensoes: [pdf]\n    destino: ''\n".into(),
                "Regras de Downloads",
                "/resources/automations",
            )
        }
    };
    (
        serde_json::to_string_pretty(&value).unwrap_or_default(),
        title,
        path,
    )
}
pub fn overlay(app: &App) -> Option<El<'_>> {
    use iced::widget::{mouse_area, opaque, scrollable, text_editor};
    if app.resources.form.is_none() && app.resources.result.is_none() {
        return None;
    }
    let close = Msg::Resources(Event::Close);
    let mut col = column![].spacing(12);
    if let Some(form) = &app.resources.form {
        let (_, title, _) = form_spec(form.kind);
        col = col.push(txf(crate::i18n::text(title), 22.0, t::FG, t::UI_SEMI));
        if matches!(form.kind, FormKind::Routine | FormKind::Downloads) {
            col = col.push(
                text_input(
                    crate::i18n::text("Nome"),
                    form.values.get("name").map(String::as_str).unwrap_or(""),
                )
                .on_input(|s| Msg::Resources(Event::FormInput("name", s))),
            );
            col = col.push(btn(
                crate::i18n::text("Carregar arquivo existente"),
                Kind::Normal,
                true,
                None,
                (!form.busy).then_some(Msg::Resources(Event::FormRead)),
            ));
            col=col.push(tx(crate::i18n::text("Salvar revoga a concessão anterior. Revise e conceda a automação para executar."),12.5,t::FG3));
        } else {
            let description=match form.kind{
                FormKind::BackupRestore=>"Restaura em uma nova pasta. A senha atual abre o arquivo; a nova senha protege o perfil restaurado.",
                FormKind::BackupExport=>"Exporta banco e perfil cifrados. Contas, tokens e permissões deste computador ficam fora do arquivo.",
                _=>"Revise os dados antes de salvar. Conexões habilitadas podem iniciar processos locais ou acessar o endereço informado.",
            };
            col = col.push(tx(crate::i18n::text(description), 12.5, t::FG3));
        }
        // Senhas de restauração têm controles próprios; nunca aparecem no editor JSON.
        if form.kind == FormKind::BackupRestore {
            for (key, label) in [
                ("file", "Arquivo do backup"),
                ("folder", "Nova pasta de dados"),
                ("password", "Senha ou chave de recuperação"),
                ("new_password", "Nova senha"),
            ] {
                let field = text_input(
                    crate::i18n::text(label),
                    form.values.get(key).map(String::as_str).unwrap_or(""),
                )
                .secure(key.contains("password"))
                .on_input(move |v| Msg::Resources(Event::FormInput(key, v)));
                if !key.contains("password") {
                    col = col.push(
                        row![
                            field,
                            btn(
                                crate::i18n::text("Escolher"),
                                Kind::Normal,
                                true,
                                None,
                                Some(Msg::Resources(Event::FormBrowse(key, key == "folder")))
                            )
                        ]
                        .spacing(8),
                    );
                } else {
                    col = col.push(field);
                }
            }
            col = col.push(setting(
                crate::i18n::text("Usar chave de recuperação"),
                "",
                switch(
                    form.values.get("recovery").is_some_and(|v| v == "true"),
                    Msg::Resources(Event::FormInput(
                        "recovery",
                        (!form.values.get("recovery").is_some_and(|v| v == "true")).to_string(),
                    )),
                ),
            ));
        } else if matches!(form.kind, FormKind::Plugin | FormKind::BackupExport) {
            col = col.push(
                row![
                    text_input(
                        crate::i18n::text("Pasta"),
                        form.values.get("folder").map(String::as_str).unwrap_or("")
                    )
                    .on_input(|s| Msg::Resources(Event::FormInput("folder", s))),
                    btn(
                        crate::i18n::text("Escolher"),
                        Kind::Normal,
                        true,
                        None,
                        Some(Msg::Resources(Event::FormBrowse("folder", true)))
                    )
                ]
                .spacing(8),
            );
            if form.kind == FormKind::Plugin {
                col = col.push(setting(
                    crate::i18n::text("Atualizar plugin existente"),
                    "",
                    switch(
                        form.values.get("update").is_some_and(|v| v == "true"),
                        Msg::Resources(Event::FormInput(
                            "update",
                            (!form.values.get("update").is_some_and(|v| v == "true")).to_string(),
                        )),
                    ),
                ));
            }
        } else {
            col = col.push(
                text_editor(&form.editor)
                    .height(260)
                    .font(t::MONO)
                    .size(13)
                    .on_action(|a| Msg::Resources(Event::Editor(a))),
            );
        }
        if !form.error.is_empty() {
            col = col.push(tx(form.error.clone(), 13.0, t::DANGER));
        }
        col = col.push(btn(
            crate::i18n::text(if form.busy {
                "Salvando…"
            } else {
                match form.kind {
                    FormKind::BackupRestore => "Restaurar",
                    FormKind::BackupExport => "Exportar",
                    FormKind::Plugin => "Instalar",
                    _ => "Salvar",
                }
            }),
            Kind::Primary,
            false,
            None,
            (!form.busy).then_some(Msg::Resources(Event::FormSubmit)),
        ));
    }
    if let Some(result) = &app.resources.result {
        col = col.push(
            container(scrollable(txf(result.clone(), 12.5, t::FG2, t::MONO))).max_height(220),
        );
    }
    col = col.push(btn(
        crate::i18n::text("Fechar"),
        Kind::Normal,
        true,
        None,
        Some(close.clone()),
    ));
    let card =
        container(col)
            .padding(24)
            .width(700)
            .style(surface(Some(t::BG), Some(t::LINE2), 14.0));
    Some(opaque(
        mouse_area(
            container(opaque(card))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(surface(Some(t::SCRIM), None, 0.0)),
        )
        .on_press(Msg::Bg),
    ))
}

fn list_path(group: &str) -> &'static str {
    match group {
        "mcp" => "/mcp/connections",
        "accounts" => "/accounts",
        "calendar" => "/calendar",
        "plugins" => "/plugins",
        "routines" => "/rotinas",
        _ => "/downloads",
    }
}
pub fn management<'a>(app: &'a App, group: &'static str) -> El<'a> {
    let title = match group {
        "mcp" => "Servidores MCP",
        "accounts" => "Contas conectadas",
        "calendar" => "Calendários vinculados",
        "plugins" => "Plugins instalados",
        "routines" => "Rotinas",
        _ => "Downloads",
    };
    let mut col = column![row![
        txf(crate::i18n::text(title), 14.0, t::FG, t::UI_SEMI),
        btn(
            crate::i18n::text("Atualizar"),
            Kind::Normal,
            true,
            None,
            Some(Msg::Resources(Event::Manage(group)))
        )
    ]
    .spacing(8)]
    .spacing(8);
    let action = |label: &'static str, path: String| {
        btn(
            crate::i18n::text(label),
            Kind::Normal,
            true,
            None,
            Some(Msg::Resources(Event::Post(path, json!({})))),
        )
    };
    if let Some(value) = app.resources.lists.get(group) {
        let entries = value.as_array().or_else(|| value["rotinas"].as_array());
        if let Some(entries) = entries {
            if entries.is_empty() {
                col = col.push(tx(
                    crate::i18n::text("Nenhum item configurado."),
                    12.5,
                    t::FG3,
                ));
            }
            for item in entries.iter().take(32) {
                let id = item["id"]
                    .as_str()
                    .or_else(|| item["nome"].as_str())
                    .unwrap_or("");
                let label = item["name"]
                    .as_str()
                    .or_else(|| item["nome"].as_str())
                    .unwrap_or(id);
                let status = item["state"]
                    .as_str()
                    .or_else(|| item["estado"].as_str())
                    .unwrap_or("");
                let mut controls = row![tx(format!("{label} · {status}"), 13.0, t::FG2)].spacing(6);
                match group {
                    "mcp" => {
                        controls = controls
                            .push(btn(
                                crate::i18n::text("Editar e habilitar"),
                                Kind::Normal,
                                true,
                                None,
                                Some(Msg::Resources(Event::McpEdit(id.into()))),
                            ))
                            .push(action("Testar", format!("/mcp/connections/{id}/test")))
                            .push(action(
                                "Desativar",
                                format!("/mcp/connections/{id}/disable"),
                            ));
                    }
                    "accounts" => {
                        controls = controls
                            .push(action("Autorizar", format!("/accounts/{id}/authorize")))
                            .push(action("Revogar", format!("/accounts/{id}/revoke")));
                    }
                    "plugins" => {
                        let review = json!({"digest":item["selected_digest"],"capabilities":item["capabilities"],"trusted_local":false,"authorized_remote":false,"classifications":{},"scope":"personal"});
                        controls = controls
                            .push(btn(
                                crate::i18n::text("Revisar e ativar"),
                                Kind::Normal,
                                true,
                                None,
                                Some(Msg::Resources(Event::Review(
                                    format!("/plugins/{id}/activate"),
                                    review,
                                ))),
                            ))
                            .push(action("Desativar", format!("/plugins/{id}/deactivate")));
                    }
                    "routines" => {
                        controls = controls
                            .push(action("Conceder", format!("/rotinas/{id}/conceder")))
                            .push(action("Revogar", format!("/rotinas/{id}/revogar")))
                            .push(action("Executar", format!("/rotinas/{id}/executar")));
                    }
                    _ => {}
                }
                col = col.push(controls.wrap());
            }
        } else {
            col = col.push(txf(
                serde_json::to_string_pretty(value).unwrap_or_default(),
                12.0,
                t::FG3,
                t::MONO,
            ));
        }
    }
    if group == "downloads" {
        col = col.push(
            row![
                action("Conceder", "/downloads/conceder".into()),
                action("Revogar", "/downloads/revogar".into()),
                btn(
                    crate::i18n::text("Simular"),
                    Kind::Normal,
                    true,
                    None,
                    Some(Msg::Resources(Event::Post(
                        "/downloads/rodar?simular=true".into(),
                        json!({})
                    )))
                ),
                btn(
                    crate::i18n::text("Executar"),
                    Kind::Normal,
                    true,
                    None,
                    Some(Msg::Resources(Event::Post(
                        "/downloads/rodar?simular=false".into(),
                        json!({})
                    )))
                ),
                btn(
                    crate::i18n::text("Desfazer último"),
                    Kind::Normal,
                    true,
                    None,
                    Some(Msg::Resources(Event::Post(
                        "/downloads/desfazer".into(),
                        json!({"escopo":"ultimo"})
                    )))
                ),
            ]
            .spacing(6)
            .wrap(),
        );
    }
    col.into()
}

impl Drop for Form {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        for (key, value) in &mut self.values {
            if key.contains("password") {
                value.zeroize();
            }
        }
    }
}

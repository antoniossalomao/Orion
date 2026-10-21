//! Estado do servidor: tudo que as rotas usam, montado uma vez na subida (o `lifespan` do
//! Python).

use orion_ai::costs::{cotas_de, Custos};
use orion_ai::delegate::Delegator;
use orion_ai::gateway::{Endpoint, CAMADA_LOCAL};
use orion_ai::secrets::get_secret;
use orion_ai::ChatGateway;
use orion_core::agent::{Agent, AgentConfig};
use orion_core::auth::AuthService;
use orion_core::config::{project_root, Settings};
use orion_core::mcp_indice::Indice;
use orion_core::memory::compact::Compactor;
use orion_core::modos::Modos;
use orion_core::resultados::Library;
use orion_core::skills::{skill_tool, SkillCatalog};
use orion_core::tools::builtin::{default_registry, Opcoes};
use orion_core::tools::processes::{process_tools, ProcessManager};
use orion_core::tools::web::WebOpcoes;
use orion_policy::{ApprovalStore, PathGuard, PolicyEngine};
use orion_store::ops::Operations;
use orion_store::Store;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;

#[allow(dead_code)] // gateway, custos, delegator e processos alimentam o painel
pub struct AppState {
    pub settings: Settings,
    pub memory: Arc<Store>,
    pub policy: Arc<PolicyEngine>,
    pub started_at: f64,
    pub ops: Arc<Operations>,
    pub auth: Arc<AuthService>,
    pub two_factor: orion_core::two_factor::TwoFactor,
    /// `None` enquanto o gateway não está configurado.
    pub agent: Option<Arc<Agent>>,
    pub accounts: Option<Arc<orion_core::accounts::Accounts>>,
    pub ponte: Arc<orion_core::ponte::PonteHub>,
    pub imagens_ponte: Arc<orion_core::ponte::Imagens>,
    pub mcp_host: Option<Arc<orion_core::extensions::host::MCPHost>>,
    pub catalog: Option<Arc<orion_core::extensions::catalog::Catalog>>,
    pub calendar: Option<Arc<orion_core::calendar::Calendar>>,
    pub events: Option<Arc<orion_core::calendar_events::Events>>,
    pub file_plans: Option<Arc<orion_core::file_plans::FilePlans>>,
    /// Skills de extensões (pacotes de plugins e fontes `ORION_SKILL_SOURCES`), com escopo por turno.
    pub skill_runtime: Arc<orion_core::extensions::skill_runtime::SkillRuntime>,
    pub extensions: Option<Arc<orion_core::extensions::manager::PluginManager>>,
    pub gateway: Option<Arc<ChatGateway>>,
    pub biblioteca: Arc<Library>,
    /// Pânico e não perturbe (REGRAS.md: pânico é corte; não perturbe só segura).
    pub modos: Arc<Modos>,
    /// Cota gratuita (REGRAS.md: custo zero (sem API paga)).
    pub custos: Arc<Custos>,
    pub budget: Arc<orion_ai::budget::Budget>,
    pub delegator: Option<Arc<Delegator>>,
    pub processos: Option<Arc<ProcessManager>>,
    pub skills: Option<Arc<RwLock<SkillCatalog>>>,
    pub painel: Arc<orion_core::painel::Painel>,
    /// `None` com `ORION_JOBS_ENABLED=false`.
    pub jobs: Option<Arc<orion_core::jobs::JobRunner>>,
    /// `None` sem mcp.json, sem servidor habilitado ou sem gateway.
    pub mcp: Option<Arc<orion_core::mcp_client::McpManager>>,
    pub ler_depois: Arc<orion_core::ler_depois::LerDepois>,
    pub desfazer: orion_core::captura_rapida::Desfazer,
    pub export_credentials: Arc<orion_core::export_credentials::ExportCredentials>,
    pub export: Arc<orion_core::mcp_export::Export>,
    /// `None` sem agente, ORION_WEB_TOOLS ou vault.
    pub pesquisa: Option<Arc<orion_core::research::NightResearch>>,
    /// Memória da tela (REGRAS.md: memória da tela: só texto, local e com opt-in).
    pub tela: Option<Arc<orion_core::screen_memory::ScreenMemory>>,
    /// `None` sem token, sem usuários ou sem gateway.
    pub telegram: Option<Arc<orion_core::channels::telegram::TelegramChannel>>,
    pub financas: Arc<orion_core::vida::Financas>,
    pub precos: Arc<orion_core::vida::Precos>,
    pub foco: Arc<orion_core::vida::Foco>,
    pub vida_canal: Arc<orion_core::vida::VidaCanal>,
    /// Só o modelo deste computador (REGRAS.md: modelo local é reserva sem ferramentas).
    pub gateway_local: Option<Arc<dyn orion_core::memory::Completer>>,
    pub historicos: Arc<orion_core::historicos::Historicos>,
    pub downloads: Arc<orion_core::downloads::Downloads>,
    pub rotinas: Arc<orion_core::rotinas::RotinaStore>,
    pub executor_rotinas: Option<Arc<orion_core::rotinas::Executor>>,
    pub tarefas_bg: Option<Arc<orion_core::tarefas_bg::TarefasBg>>,
    /// `None` sem `ORION_NUVEM=true` (opt-in).
    pub nuvem: Option<Arc<orion_core::nuvem::Nuvem>>,
    pub camera_descrever: Option<CameraDescrever>,
    pub vigia: Option<Arc<orion_core::vigia::Vigia>>,
    pub legenda: Option<Arc<orion_core::legenda::Legenda>>,
    pub speaker: Option<Arc<dyn orion_core::voice::Speaker>>,
    pub transcriber: Option<Arc<orion_ai::transcribe::Transcriber>>,
    pub voz: Arc<orion_core::voice::VozStats>,
    pub estado_voz: Arc<orion_core::vida::EstadoVoz>,
    pub escuta: Option<Arc<orion_core::wake::WakeListener>>,
    pub desktop_wake: Arc<std::sync::atomic::AtomicBool>,
}

pub type CameraDescrever = Arc<dyn Fn(&str) -> serde_json::Value + Send + Sync>;
pub type Estado = Arc<AppState>;

/// Política com audit no banco: falha de gravação nega o que não for leitura
/// (REGRAS.md: falha de auditoria nega a ação).
pub fn build_policy(settings: &Settings, ops: Option<Arc<Operations>>) -> PolicyEngine {
    let mut safe = orion_policy::paths::default_safe_roots();
    safe.extend(settings.extra_safe_roots.iter().cloned());
    let guard = PathGuard { protected_roots: vec![project_root()], safe_roots: safe, system_roots: orion_policy::paths::system_roots() };
    let approvals = Arc::new(ApprovalStore::new(settings.approval_ttl_s, orion_policy::wall_clock()));
    PolicyEngine::new(guard).with_approvals(approvals).with_audit(Arc::new(move |evento| {
        tracing::info!(target: "orion.audit", "tool_decision {}", evento);
        match &ops {
            Some(o) => o.audit_add(evento).map(|_| ()).map_err(|e| e.to_string()),
            None => Ok(()),
        }
    }))
}

fn espera(camada: &str) -> f64 {
    match camada {
        "pesado" => 120.0,
        "visao" => 90.0,
        _ => 60.0,
    }
}

/// Um endpoint por modelo configurado em `ORION_PROVEDORES`, na ordem da lista. `camadas` filtra;
/// provedor sem chave fica de fora, com aviso no log.
pub fn endpoints_dos_provedores(settings: &Settings, camadas: Option<&[&str]>) -> Vec<Endpoint> {
    let mut eps = vec![];
    for p in &settings.provedores {
        let env = orion_ai::provedores::chave_env(&p.id);
        let Some(chave) = get_secret(&env) else {
            tracing::warn!("provedor {} sem chave ({env}): fica de fora", p.id);
            continue;
        };
        for (camada, modelo) in p.modelos() {
            if camadas.is_some_and(|c| !c.contains(&camada.as_str())) {
                continue;
            }
            let nome = if camada.is_empty() { p.id.clone() } else { format!("{}:{camada}", p.id) };
            eps.push(Endpoint {
                name: nome,
                base_url: p.base_url(),
                model: modelo,
                api_key: Some(chave.clone()),
                timeout_s: espera(&camada),
                tier: camada,
                tools: true,
                provedor: p.id.clone(),
            });
        }
    }
    eps
}

fn local(settings: &Settings, timeout: f64) -> Endpoint {
    Endpoint {
        name: "local".into(),
        base_url: if settings.local_model_path.is_some() { "embedded://llama.cpp".into() } else { settings.local_url.clone() },
        model: settings.local_model_path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned()).unwrap_or_else(|| settings.local_model.clone()),
        api_key: None,
        timeout_s: timeout,
        tier: CAMADA_LOCAL.into(),
        tools: false,
        provedor: String::new(),
    }
}

fn has_local(settings: &Settings) -> bool {
    settings.local_model_path.is_some() || !settings.local_model.is_empty()
}

fn install_local_backend(gateway: ChatGateway, settings: &Settings) -> ChatGateway {
    if let Some(path) = &settings.local_model_path {
        *gateway.embedded_local.lock() = Some(crate::embedded::generator(path.clone(), settings.local_model_gpu));
    }
    gateway
}

fn local_gateway(settings: &Settings, timeout: f64) -> Option<ChatGateway> {
    if !has_local(settings) { return None; }
    ChatGateway::new(vec![local(settings, timeout)]).ok().map(|g| install_local_backend(g, settings))
}

pub fn gateway_from_settings(settings: &Settings) -> Option<ChatGateway> {
    let mut eps = endpoints_dos_provedores(settings, None);
    if !settings.gateway_url.is_empty() && !settings.gateway_model.is_empty() {
        let chave = Some(settings.gateway_api_key.clone()).filter(|k| !k.is_empty()).or_else(|| get_secret("ORION_GATEWAY_API_KEY"));
        let url = &settings.gateway_url;
        let mut ep = Endpoint::new("gateway", url, &settings.gateway_model);
        ep.api_key = chave.clone();
        eps.push(ep);
        // camadas do roteamento: só entram as que têm modelo próprio; o "gateway" fica de reserva
        for (camada, modelo, t) in
            [("rapido", &settings.gateway_model_fast, 60.0), ("pesado", &settings.gateway_model_heavy, 120.0), ("visao", &settings.vision_model, 90.0)]
        {
            if !modelo.is_empty() && *modelo != settings.gateway_model {
                let mut ep = Endpoint::new(camada, url, modelo);
                ep.api_key = chave.clone();
                ep.timeout_s = t;
                ep.tier = camada.into();
                eps.push(ep);
            }
        }
    }
    if has_local(settings) {
        // REGRAS.md: o modelo deste computador entra por último e nunca recebe ferramentas
        eps.push(local(settings, 180.0));
    }
    if eps.is_empty() {
        return None;
    }
    ChatGateway::new(eps).ok().map(|g| install_local_backend(g, settings))
}

/// Auto-compact só com o modelo local, num gateway próprio: o texto da conversa nunca sai do
/// computador para ser resumido (REGRAS.md: resumo da conversa nunca substitui o registro).
pub fn compactor_from_settings(settings: &Settings, memory: Arc<Store>) -> Option<Arc<Compactor>> {
    if settings.compact_at == 0 || !has_local(settings) {
        return None;
    }
    let gw = local_gateway(settings, 300.0)?;
    Some(Compactor::new(memory, Arc::new(gw), settings.compact_at, 30, &format!("local:{}", local(settings, 300.0).model)))
}

/// O roteamento só liga se alguma camada tem modelo próprio.
pub fn roteamento_ligado(settings: &Settings) -> bool {
    !settings.gateway_model_fast.is_empty()
        || !settings.gateway_model_heavy.is_empty()
        || !settings.vision_model.is_empty()
        || settings.provedores.iter().any(|p| !p.rapido.is_empty() || !p.pesado.is_empty() || !p.visao.is_empty())
}

/// Visão só com `ORION_VISION_TOOLS` e gateway configurado.
pub fn vision_from_settings(settings: &Settings) -> Option<orion_ai::vision::Vision> {
    if !settings.vision_tools {
        return None;
    }
    let mut eps = endpoints_dos_provedores(settings, Some(&["visao"]));
    let com_visao: Vec<String> = eps.iter().map(|e| e.provedor.clone()).collect();
    eps.extend(endpoints_dos_provedores(settings, Some(&[""])).into_iter().filter(|e| !com_visao.contains(&e.provedor)));
    if !settings.gateway_url.is_empty() && !settings.gateway_model.is_empty() {
        let modelo = if settings.vision_model.is_empty() { &settings.gateway_model } else { &settings.vision_model };
        let mut ep = Endpoint::new("gateway", &settings.gateway_url, modelo);
        ep.api_key = Some(settings.gateway_api_key.clone()).filter(|k| !k.is_empty()).or_else(|| get_secret("ORION_GATEWAY_API_KEY"));
        ep.timeout_s = 90.0;
        eps.push(ep);
    }
    orion_ai::vision::Vision::new(eps).ok()
}

pub fn transcriber_from_settings(settings: &Settings) -> Option<orion_ai::transcribe::Transcriber> {
    let fallback = if settings.transcribe_groq_fallback {
        Some(settings.transcribe_api_key.clone())
            .filter(|k| !k.is_empty())
            .or_else(|| get_secret("ORION_TRANSCRIBE_API_KEY"))
            .and_then(|k| orion_ai::transcribe::Transcriber::new(&k, &settings.transcribe_url, &settings.transcribe_model).ok())
    } else {
        None
    };
    #[cfg(windows)]
    if let Some(model) = &settings.local_stt_model {
        match orion_core::native_audio::transcriber(model, settings.local_stt_gpu, fallback.clone()) {
            Ok(t) => return Some(t),
            Err(_) => tracing::warn!("Whisper local indisponível; confira o modelo e a GPU"),
        }
    }
    fallback
}

/// Sem chave de API, sem embeddings: a busca segue só por palavra-chave.
pub fn embedder_from_settings(settings: &Settings) -> Option<Arc<dyn orion_store::Embedder>> {
    let chave = Some(settings.embed_api_key.clone()).filter(|k| !k.is_empty()).or_else(|| get_secret("ORION_EMBED_API_KEY"))?;
    orion_ai::embedders::GeminiEmbedder::new(&chave, &settings.embed_model, settings.embed_dim).ok().map(|e| Arc::new(e) as Arc<dyn orion_store::Embedder>)
}

pub fn memory_from_settings(settings: &Settings) -> anyhow::Result<Store> {
    let embedder = embedder_from_settings(settings);
    let tem = embedder.is_some();
    let s = match orion_core::dbopen::open_store_with(&settings.db_path(), embedder, orion_policy::wall_clock()) {
        Ok(s) => s,
        Err(e) if tem => {
            // trocou o modelo/dimensão sem `reset_vectors()`: sobe sem vetores em vez de não subir
            tracing::error!("embeddings desligados: {e}");
            orion_core::dbopen::open_store(&settings.db_path())?
        }
        Err(e) => return Err(e.into()),
    };
    if settings.rerank {
        let dir = if settings.rerank_model_dir.trim().is_empty() {
            settings.data_dir.join("modelos").join("rerank")
        } else {
            PathBuf::from(&settings.rerank_model_dir)
        };
        *s.reranker.lock() = orion_core::memory::rerank::reranker_from_dir(Some(&dir));
    }
    Ok(s)
}

pub fn speaker_from_settings(settings: &Settings) -> Option<Arc<dyn orion_core::voice::Speaker>> {
    if !((settings.voice_enabled && settings.voice_speak) || settings.telegram_voice_reply) {
        return None;
    }
    Some(Arc::new(orion_core::voice::EdgeSpeaker::new(&settings.voice_tts_voice, 30.0)))
}

pub fn camera_from_settings(
    settings: &Settings,
    memory: Arc<Store>,
    vision: Option<Arc<orion_ai::vision::Vision>>,
) -> (
    Vec<orion_core::tools::registry::Tool>,
    Option<Arc<orion_core::visao_local::Detector>>,
    Option<Arc<orion_core::visao_local::Avistamentos>>,
    Option<CameraDescrever>,
) {
    if !settings.camera {
        return (vec![], None, None, None);
    }
    let detector = settings.object_model.as_ref().map(|p| Arc::new(orion_core::visao_local::Detector::new(Some(p.clone()), 0.35)));
    let avistamentos = settings.camera_watch.then(|| Arc::new(orion_core::visao_local::Avistamentos::new(memory, 30)));
    let dev = settings.camera_device as i32;
    let tools = orion_core::tools::camera::camera_tools(
        dev,
        &settings.vision_order,
        vision.clone(),
        None,
        detector.clone(),
        avistamentos.clone(),
        &settings.translator_langs,
        false,
    );
    let ord = settings.vision_order.clone();
    let (det2, av2) = (detector.clone(), avistamentos.clone());
    let descrever: CameraDescrever = Arc::new(move |pergunta: &str| {
        orion_core::tools::camera::descrever_camera(dev, pergunta, &ord, vision.as_deref(), None, det2.as_deref(), av2.as_deref())
    });
    (tools, detector, avistamentos, Some(descrever))
}

pub fn vigia_from_settings(
    settings: &Settings,
    memory: Arc<Store>,
    tela: Option<Arc<orion_core::screen_memory::ScreenMemory>>,
    ops: Arc<Operations>,
    detector: Option<Arc<orion_core::visao_local::Detector>>,
    avistamentos: Option<Arc<orion_core::visao_local::Avistamentos>>,
) -> Option<Arc<orion_core::vigia::Vigia>> {
    if !(settings.presence_enabled || (settings.camera && settings.camera_watch)) {
        return None;
    }
    use orion_core::presenca::Presenca;
    use orion_core::rosto::{Rostos, LIMIAR};
    use orion_core::vigia::Vigia;
    let presenca = settings.presence_enabled.then(|| Arc::new(Presenca::new(settings.presence_away_min)));
    let rostos = if settings.face_model.is_some() || (settings.presence_enabled && settings.camera) {
        Some(Arc::new(Rostos::new(memory.clone(), settings.face_model.clone(), LIMIAR, None)))
    } else {
        None
    };
    let watch_s = if settings.camera && settings.camera_watch { 60 } else { 0 };
    Some(Arc::new(Vigia::new(
        memory,
        Some(ops),
        tela,
        settings.camera_device as i32,
        watch_s,
        detector,
        avistamentos,
        presenca,
        rostos,
        &settings.presence_bt,
        settings.presence_lock,
    )))
}

fn chave_de(nomes_settings: Vec<String>, segredos: &'static [&'static str]) -> orion_core::tools::web::Chave {
    Arc::new(move || nomes_settings.iter().find(|v| !v.is_empty()).cloned().or_else(|| segredos.iter().find_map(|s| get_secret(s))))
}

/// Servidores MCP do `mcp.json`; configuração inválida desliga o MCP, não o Orion.
pub fn mcp_from_settings(settings: &Settings) -> Option<Arc<orion_core::mcp_client::McpManager>> {
    if !settings.mcp_enabled {
        return None;
    }
    let extras =
        if settings.plugins_enabled { orion_core::plugins_locais::PluginStore::new(settings.effective_plugins_dir()).servidores_mcp() } else { vec![] };
    match orion_core::mcp_client::manager_from_file(&settings.effective_mcp_config(), extras) {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("MCP desligado: {e}");
            None
        }
    }
}

fn alguma_cli() -> bool {
    ["claude", "codex", "gemini"].iter().any(|c| orion_ai::delegate::which(c).is_some())
}

/// Sobe tudo. Recusa subir exposto na rede sem login ou com a senha de fábrica
/// (REGRAS.md: acesso de fora só com login).
pub async fn montar(settings: Settings) -> anyhow::Result<Estado> {
    std::fs::create_dir_all(&settings.data_dir)?;
    let s2 = settings.clone();
    let memory = Arc::new(tokio::task::spawn_blocking(move || memory_from_settings(&s2)).await??);
    // registro de saída (REGRAS.md: toda saída de dado fica registrada, sem conteúdo)
    orion_ai::saidas::definir_destino(Some(orion_ai::saidas::destino_store(memory.clone())));
    let ops = Arc::new(Operations::new(memory.clone()));
    orion_core::documents::recuperar_interrompidos(&memory);
    let ops_audit = ops.clone();
    let modos = Arc::new(Modos::new(memory.clone(), &settings.dnd_at, Some(Arc::new(move |e| ops_audit.audit_add(e).map(|_| ()).map_err(Into::into)))));
    {
        let m = modos.clone();
        *ops.segurar.write() = Arc::new(move || m.nao_perturbe()); // não perturbe segura os avisos
    }
    let custos = Arc::new(Custos::new(memory.clone(), cotas_de(&settings.quotas(), &settings.provedores), Some(ops.clone())));
    let policy = Arc::new(build_policy(&settings, Some(ops.clone())));
    let auth = Arc::new(AuthService::open(&settings.auth_db_path(), &settings.auth_user, settings.session_ttl_h as f64 * 3600.0)?);
    if settings.seed_default_password && auth.seed_default()? {
        tracing::warn!("SENHA DE FÁBRICA ativa (usuário '{}'): troque em Configurações", settings.auth_user);
    }
    let fora = settings.hosts_de_fora();
    if !fora.is_empty() && auth.uses_default_password() {
        anyhow::bail!(
            "host {} em ORION_ALLOWED_HOSTS com a senha de fábrica ainda ativa: troque a senha antes de expor o Orion na rede (`orion set-password` ou Configurações › Conexão)",
            fora.join(", ")
        );
    }
    if !fora.is_empty() && settings.admin_token.is_empty() && !auth.has_password() {
        anyhow::bail!(
            "host {} em ORION_ALLOWED_HOSTS exige login: defina a senha (`orion set-password`) ou ORION_ADMIN_TOKEN (REGRAS.md: acesso de fora só com login)",
            fora.join(", ")
        );
    }
    let gateway = gateway_from_settings(&settings).map(Arc::new);
    if let Some(g) = &gateway {
        // orçamento diário por provedor (`limite_dia`): fim da fila
        let c = custos.clone();
        *g.orcamento.lock() = Some(Arc::new(move |ep: &Endpoint| c.no_orcamento(&ep.provedor)));
    }
    let biblioteca = Arc::new(Library::new(memory.clone(), settings.data_dir.join("resultados")));
    let historicos = Arc::new(orion_core::historicos::Historicos::new(memory.clone()));
    let transcriber = transcriber_from_settings(&settings).map(Arc::new);
    let speaker = speaker_from_settings(&settings);
    let vision = vision_from_settings(&settings).map(Arc::new);
    let (camera_tools, detector, avistamentos, camera_descrever) = camera_from_settings(&settings, memory.clone(), vision.clone());
    let tela = settings.screen_memory.then(|| {
        use orion_core::screen_memory::*;
        Arc::new(ScreenMemory::new(
            memory.clone(),
            settings.screen_interval_s,
            settings.screen_retention_days,
            parse_lista(&settings.screen_exclude),
            parse_lista(&settings.screen_exclude_apps),
            settings.screen_allow_unknown_title,
            &settings.screen_ocr_langs,
        ))
    });
    let mut agent = None;
    let mut delegator = None;
    let mut processos = None;
    let mut skills_cat = None;
    let mut mcp_geral = None;
    if let Some(gw) = &gateway {
        // `delegar` só aparece se alguma CLI oficial estiver instalada
        if alguma_cli() {
            delegator = Some(Arc::new(Delegator::new(memory.clone(), orion_ai::delegate::default_agents(), 600.0, 20_000)));
        }
        let mut extra = vec![];
        let mcp = mcp_from_settings(&settings);
        let mut mcp_tools = vec![];
        if let Some(m) = &mcp {
            mcp_tools = m.start().await;
            for spec in m.specs().into_values() {
                let _ = policy.register_tool(spec, None);
            }
        }
        let indice = Indice::new(&mcp_tools, settings.mcp_index_at); // só as de servidor MCP (E7.4)
        extra.extend(mcp_tools);
        if indice.ativo() {
            let _ = policy.register_tool(orion_core::mcp_indice::spec(), None);
            extra.push(indice.tool());
        }
        if let Some(m) = mcp.as_ref().filter(|m| m.recursos_ativos()) {
            let _ = policy.register_tool(orion_core::mcp_client::spec_recurso(), None);
            extra.push(m.tool_recurso());
        }
        mcp_geral = mcp.clone();
        if settings.skills_enabled {
            let cat = SkillCatalog::new(
                &settings.effective_skills_dir(),
                if settings.plugins_enabled {
                    orion_core::plugins_locais::PluginStore::new(settings.effective_plugins_dir()).pastas_de_skills()
                } else {
                    vec![]
                },
            );
            if !cat.skills.is_empty() {
                let _ = policy.register_tool(orion_core::skills::tool_spec(), None);
                let arc = Arc::new(RwLock::new(cat));
                extra.push(skill_tool(arc.clone()));
                skills_cat = Some(arc);
            }
        }
        if let Some(t) = &tela {
            let _ = t;
            let _ =
                policy.register_tool(orion_policy::ToolSpec { external: true, ..orion_policy::ToolSpec::new("buscar_tela", orion_policy::Risk::Read) }, None);
            extra.push(orion_core::screen_memory::screen_tool(memory.clone()));
        }
        extra.extend(orion_core::historicos::ferramentas(historicos.clone(), &policy, settings.clipboard_days > 0, settings.command_history));
        let mut o = Opcoes { ops: Some(ops.clone()), delegator: delegator.clone(), camera: camera_tools, extra, ..Default::default() };
        if settings.desktop_tools {
            // opt-in (ORION_DESKTOP_TOOLS): age no computador, sempre sob a política
            use orion_core::tools::*;
            o.desktop.extend(desktop::desktop_tools(60.0, 8000));
            o.desktop.extend(fs_tools::fs_tools());
            o.desktop.extend(system_tools::system_tools(15.0));
            o.desktop.extend(documents::document_tools());
            o.desktop.extend(media::media_tools(15.0));
            if let Some(v) = &vision {
                o.desktop.extend(vision::vision_tools(v.clone(), settings.data_dir.join("capturas"), 20.0));
            }
            if let Some(t) = transcriber_from_settings(&settings) {
                o.desktop.extend(audio::audio_tools(t));
            }
            let pm = Arc::new(ProcessManager::new(settings.data_dir.join("processos")));
            o.desktop.extend(process_tools(pm.clone(), ops.clone()));
            processos = Some(pm);
        }
        if settings.web_tools {
            // opt-in (ORION_WEB_TOOLS): rede, conteúdo externo
            o.web = orion_core::tools::web::web_tools(WebOpcoes {
                search_key: chave_de(vec![settings.search_api_key.clone(), settings.embed_api_key.clone()], &["ORION_SEARCH_API_KEY", "ORION_EMBED_API_KEY"]),
                search_model: settings.search_model.clone(),
                brave_key: chave_de(vec![settings.brave_api_key.clone()], &["ORION_BRAVE_API_KEY"]),
                image_key: chave_de(
                    vec![settings.image_api_key.clone(), settings.search_api_key.clone(), settings.embed_api_key.clone()],
                    &["ORION_IMAGE_API_KEY", "ORION_SEARCH_API_KEY", "ORION_EMBED_API_KEY"],
                ),
                image_model: settings.image_model.clone(),
                image_dir: Some(settings.data_dir.join("imagens")),
                cidade_padrao: settings.weather_city.clone(),
            });
        }
        let webhooks = orion_core::tools::n8n::parse_webhooks(&settings.n8n_webhooks).map_err(|e| anyhow::anyhow!(e))?;
        if !webhooks.is_empty() {
            let _ = policy.register_tool(orion_core::tools::n8n::tool_spec(), None);
            o.extra.push(orion_core::tools::n8n::n8n_tool(webhooks));
        }
        let registro = default_registry(memory.clone(), o)?;
        let mut a =
            Agent::new(gw.clone(), registro, policy.clone(), memory.clone(), AgentConfig { routing: roteamento_ligado(&settings), persona: {
                let mut persona = orion_core::agent::AgentConfig::default().persona;
                if let Some(p) = orion_core::onboarding::Profile::load(&settings.data_dir)? { persona.push_str(&p.context()); }
                persona
            }, ..Default::default() });
        a.ops = Some(ops.clone());
        if let Some(m) = &mcp {
            let m = m.clone();
            a.servidor_mcp = Some(Arc::new(move |n: &str| m.servidor_de(n)));
        }
        a.skills = skills_cat.clone();
        a.library = Some(biblioteca.clone());
        a.modos = Some(modos.clone());
        a.compactor = compactor_from_settings(&settings, memory.clone());
        *a.indice.write() = Some(indice);
        agent = Some(Arc::new(a));
    }
    let started_at = orion_core::util::agora();
    // conexões MCP revisadas (host) + catálogo + agenda (leitura) + propostas + planos de arquivos
    let accounts = orion_core::accounts::Accounts::new(memory.clone());
    let configs = settings
        .mcp_connections
        .iter()
        .map(orion_core::extensions::host::parse_config)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("ORION_MCP_CONNECTIONS inválido: {e}"))?;
    let mcp_host = Arc::new(
        orion_core::extensions::host::MCPHost::new(configs, std::time::Duration::from_secs(5), Some(accounts.factory())).map_err(|e| anyhow::anyhow!(e))?,
    );
    *accounts.host.write() = Some(mcp_host.clone());
    mcp_host.start().await;
    let registro_ext = agent.as_ref().map(|a| a.tools.clone()).unwrap_or_default();
    let catalog = Arc::new(orion_core::extensions::catalog::Catalog::new(mcp_host.clone(), registro_ext.clone(), policy.clone()));
    if let Some(a) = &agent {
        let c = catalog.clone();
        *a.refresh_tools.write() = Some(Arc::new(move || {
            let c = c.clone();
            Box::pin(async move {
                let _ = c.refresh().await;
            })
        }));
    }
    if catalog.refresh().await.is_err() {
        tracing::warn!("mcp_catalog_unavailable");
    }
    let calendar = orion_core::calendar::Calendar::new(memory.clone(), mcp_host.clone(), registro_ext, policy.clone());
    let events = orion_core::calendar_events::Events::new(calendar.clone());
    let file_plans = orion_core::file_plans::FilePlans::new(events.clone());
    orion_core::development::Development::registrar(memory.clone(), &catalog.registry, policy.clone());
    let fontes = settings
        .skill_sources
        .iter()
        .map(orion_core::extensions::skill_runtime::SkillSource::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("ORION_SKILL_SOURCES inválido: {e}"))?;
    let skill_runtime = orion_core::extensions::skill_runtime::SkillRuntime::new(&fontes);
    if let Some(ids) = memory.meta_get("skills:pausadas").and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok()) {
        for id in ids {
            skill_runtime.pausar(&id);
        }
    }
    skill_runtime.attach_tools(&catalog.registry, &policy);
    let extensions = match orion_core::extensions::manager::PluginManager::new(
        &settings.data_dir.join("extensions"),
        skill_runtime.clone(),
        mcp_host.clone(),
        catalog.clone(),
        policy.clone(),
    ) {
        Ok(m) => {
            let urls = orion_core::extensions::catalogo::urls_validas(&settings.plugin_catalogs).map_err(|e| anyhow::anyhow!("ORION_PLUGIN_CATALOGS: {e}"))?;
            *m.catalogos.write() = Some(Arc::new(orion_core::extensions::catalogo::Catalogos::new(settings.effective_catalog_dir(), urls)));
            Some(Arc::new(m))
        }
        Err(e) => {
            tracing::warn!("extensões indisponíveis: {}", e.0);
            None
        }
    };
    let export_credentials = Arc::new(orion_core::export_credentials::ExportCredentials::new(memory.clone()));
    orion_core::mcp_export::registrar_specs(&policy);
    let export = Arc::new(orion_core::mcp_export::Export { memory: memory.clone(), policy: policy.clone(), creds: export_credentials.clone() });
    let downloads = Arc::new(orion_core::downloads::Downloads::new(settings.data_dir.join("regras_downloads.yaml"), memory.clone(), policy.path_guard.clone()));
    let rotinas = Arc::new(orion_core::rotinas::RotinaStore::new(settings.data_dir.join("rotinas"), memory.clone()));
    let (executor_rotinas, tarefas_bg) = match &agent {
        Some(a) => {
            let mut ex = orion_core::rotinas::Executor::new(rotinas.clone(), a.tools.clone(), policy.clone(), memory.clone(), ops.clone());
            ex.modos = Some(modos.clone());
            let (o, b) = (ops.clone(), settings.briefing_calendar_email.clone());
            let _ = b;
            ex.briefing = Some(Arc::new(move || orion_core::briefing::build_briefing(&o, orion_core::util::agora(), "Antônio", None)));
            let tb = orion_core::tarefas_bg::TarefasBg::new(memory.clone(), ops.clone(), a.clone(), Some(custos.clone()), Some(modos.clone()));
            (Some(Arc::new(ex)), Some(Arc::new(tb)))
        }
        None => (None, None),
    };
    let ler_depois = Arc::new(orion_core::ler_depois::LerDepois::new(
        memory.clone(),
        settings.vault_dir.clone(),
        settings.web_tools,
        gateway.clone().map(|g| g as Arc<dyn orion_core::memory::Completer>),
        Some(modos.clone()),
    ));
    let pesquisa = match (&agent, &settings.vault_dir) {
        (Some(a), Some(v)) if settings.web_tools => {
            let cap = orion_core::capture::Capturer::new(v.clone(), &settings.capture_folder);
            match orion_core::research::NightResearch::new(memory.clone(), a.clone(), cap, &settings.research_topics, &settings.research_at) {
                Ok(p) => Some(Arc::new(p)),
                Err(e) => anyhow::bail!(e),
            }
        }
        _ => {
            if !settings.research_at.is_empty() {
                tracing::warn!("pesquisa noturna pedida, mas faltam gateway, ORION_WEB_TOOLS ou o vault");
            }
            None
        }
    };
    let financas = Arc::new(orion_core::vida::Financas::new(memory.clone()));
    let precos = Arc::new(orion_core::vida::Precos::new(memory.clone()));
    let foco = Arc::new(orion_core::vida::Foco::new(settings.focus_distractions.split(';').filter(|d| !d.trim().is_empty()).map(String::from).collect()));
    let revisao = Arc::new(orion_core::vida::Revisao::new(settings.vault_dir.clone()));
    if let Some(t) = &tela {
        // escudo de foco: só pergunta, nunca fecha nada (REGRAS.md: o escudo de foco pergunta; nunca bloqueia, fecha nem segura nada)
        let (f, o) = (foco.clone(), ops.clone());
        *t.on_capture.lock() = Some(Arc::new(move |alvo: &str| {
            if f.pergunta_permitida(alvo) {
                let _ = o.notify(
                    "foco",
                    &format!("isso é do marco da semana? ({})", alvo.chars().take(80).collect::<String>()),
                    Some(&format!("foco:{}", orion_core::util::agora() as i64)),
                    false,
                );
            }
        }));
    }
    let gateway_local: Option<Arc<dyn orion_core::memory::Completer>> = local_gateway(&settings, 60.0).map(|g| Arc::new(g) as Arc<dyn orion_core::memory::Completer>);
    let vida_canal =
        Arc::new(orion_core::vida::VidaCanal::new(memory.clone(), financas.clone(), precos.clone(), foco.clone(), revisao, ops.clone(), gateway_local.clone()));
    let vigia = vigia_from_settings(&settings, memory.clone(), tela.clone(), ops.clone(), detector.clone(), avistamentos.clone());
    let legenda = if settings.subtitles {
        settings.subtitle_model.as_ref().and_then(|modelo| match orion_core::legenda::criar_transcritor(&modelo.to_string_lossy()) {
            Ok(tr) => {
                let leg = Arc::new(orion_core::legenda::Legenda::new(tr, 200.0));
                let _ = orion_core::legenda::iniciar_thread(leg.clone(), &settings.subtitle_device);
                Some(leg)
            }
            Err(e) => {
                tracing::warn!("legenda desligada: {e}");
                None
            }
        })
    } else {
        None
    };
    let telegram = match (&agent, Some(settings.telegram_token.clone()).filter(|t| !t.is_empty()).or_else(|| get_secret("ORION_TELEGRAM_TOKEN"))) {
        (Some(a), Some(token)) if !settings.telegram_allowed_users.is_empty() => {
            let cap = settings.vault_dir.as_ref().map(|v| Arc::new(orion_core::capture::Capturer::new(v.clone(), &settings.capture_folder)));
            match orion_core::channels::telegram::TelegramChannel::new(
                &token,
                &settings.telegram_allowed_users,
                a.clone(),
                memory.clone(),
                a.policy.approvals.clone(),
                ops.clone(),
                transcriber.clone(),
                cap,
            ) {
                Ok(t) => {
                    *t.modos.lock() = Some(modos.clone());
                    if settings.telegram_voice_reply {
                        *t.speaker.lock() = speaker.clone();
                    }
                    let (tl, mem) = (tela.clone(), memory.clone());
                    *t.tela.lock() = Some(Arc::new(move |acao: &str| orion_core::screen_memory::comando_tela(tl.as_deref(), &mem, acao)));
                    *t.ler_depois.lock() = Some(ler_depois.clone());
                    *t.vida.lock() = Some(vida_canal.clone());
                    *t.rotinas.lock() = Some(rotinas.clone());
                    *t.executor_rotinas.lock() = executor_rotinas.clone();
                    if let Some(cd) = &camera_descrever {
                        let cd = cd.clone();
                        *t.camera.lock() = Some(Arc::new(move |pergunta: &str| {
                            let r = cd(pergunta);
                            if r["ok"] == serde_json::json!(true) {
                                r["descricao"].as_str().unwrap_or("").to_string()
                            } else {
                                format!("Não consegui olhar: {}", r["erro"].as_str().unwrap_or("falha"))
                            }
                        }));
                    }
                    {
                        let (repos, bdir, tb, dl) = (settings.repos.clone(), settings.effective_backup_dir(), tarefas_bg.clone(), downloads.clone());
                        *t.desligar.lock() = Some(Arc::new(move || {
                            let pasta =
                                dl.estado()["pasta"].as_str().map(PathBuf::from).unwrap_or_else(|| orion_core::downloads::dirs_home().join("Downloads"));
                            let tarefas = tb.as_ref().and_then(|x| x.listar(50).ok()).unwrap_or_default();
                            orion_core::desligar::texto(&orion_core::desligar::posso_desligar(&repos, Some(&bdir), &pasta, &tarefas))
                        }));
                    }
                    Some(Arc::new(t))
                }
                Err(e) => {
                    tracing::error!("telegram desligado: {e}");
                    None
                }
            }
        }
        (None, Some(_)) => {
            tracing::error!("telegram desligado: configure os modelos (ORION_PROVEDORES ou ORION_GATEWAY_URL/MODEL)");
            None
        }
        (_, Some(_)) => {
            tracing::error!("telegram desligado: defina ORION_TELEGRAM_ALLOWED_USERS (default-deny)");
            None
        }
        _ => None,
    };
    let jobs = if settings.jobs_enabled {
        use orion_core::jobs::{JobRunner, JobsConfig};
        let cfg = JobsConfig {
            backup_dir: settings.backup_dir.clone(),
            backup_keep: settings.backup_keep,
            vault_dir: settings.vault_dir.clone(),
            audit_days: settings.audit_retention_days,
            briefing_at: settings.briefing_at.clone(),
            gateway_down_min: settings.gateway_down_min,
            clipboard_days: settings.clipboard_days as i64,
            command_history: settings.command_history,
            ..Default::default()
        };
        let mut j = JobRunner::new(memory.clone(), ops.clone(), cfg)
            .map_err(|e| anyhow::anyhow!(e))?
            .com_custos(custos.clone())
            .com_modos(modos.clone())
            .com_downloads(downloads.clone())
            .com_historicos(historicos.clone());
        if let Some(vg) = &vigia {
            j = j.com_vigia(vg.clone());
        }
        if let Some(gw) = &gateway {
            if settings.consolidate {
                j = j.com_consolidator(Arc::new(orion_core::memory::consolidate::Consolidator::new(memory.clone(), gw.clone())));
            }
            if settings.weekly_ai {
                j = j.com_leitura_semanal(gw.clone());
            }
        }
        if let Some(pm) = &processos {
            j = j.com_processos(pm.clone());
        }
        if !settings.sleep_at.is_empty() {
            let gw_c = gateway.clone().map(|g| g as Arc<dyn orion_core::memory::Completer>);
            let mut sono = orion_core::memory::sleep::SleepCycle::new(memory.clone(), ops.clone(), gw_c, &settings.sleep_at).map_err(|e| anyhow::anyhow!(e))?;
            sono.sonhos = true; // E10.9: o sono também sonha uma ideia (fica pendente até o 'boa')
            j = j.com_sono(Arc::new(sono));
        }
        if let Some(p) = &pesquisa {
            j = j.com_pesquisa(p.clone());
        }
        if let Some(t) = &tela {
            j = j.com_tela(t.clone());
        }
        {
            let mut v = orion_core::vida::VidaJobs::new(
                memory.clone(),
                financas.clone(),
                ops.clone(),
                settings.vault_dir.clone(),
                settings.repos.clone(),
                &settings.diary_at,
                &settings.weekly_review_at,
            );
            if let Some(a) = &agent {
                // E10.5: usa a pesquisa da web; a cota já foi checada no job
                let (tools, h) = (a.tools.clone(), tokio::runtime::Handle::current());
                v = v.com_precos(
                    precos.clone(),
                    Arc::new(move |consulta: &str| {
                        let t = tools.read().get("pesquisar_internet").cloned()?;
                        let mut args = serde_json::Map::new();
                        args.insert("query".into(), serde_json::json!(consulta));
                        args.insert("max_resultados".into(), serde_json::json!(5));
                        Some(h.block_on(t.run(args)))
                    }),
                );
            }
            j = j.com_vida(Arc::new(v));
        }
        Some(Arc::new(j))
    } else {
        None
    };
    let budget = Arc::new(orion_ai::budget::Budget::load(memory.clone())?);
    if let Some(g) = &gateway {
        *g.weekly_budget.lock() = Some(budget.clone());
    }
    if let Some(d) = &delegator {
        *d.weekly_budget.lock() = Some(budget.clone());
    }
    let voz = Arc::new(orion_core::voice::VozStats::default());
    let estado_voz = Arc::new(orion_core::vida::EstadoVoz::default());
    let desktop_wake = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let escuta = listen_from_settings(&settings, agent.clone(), transcriber.clone(), speaker.clone(), memory.clone(), modos.clone(), desktop_wake.clone());
    let voz_painel = {
        let vz = voz.clone();
        let esc = escuta.clone();
        let clique = settings.voice_enabled && agent.is_some() && transcriber.is_some();
        let fala = settings.voice_enabled && settings.voice_speak && speaker.is_some();
        let wake_pedida = settings.wake_enabled;
        let clap_pedida = settings.clap_enabled;
        let clap_acao = settings.clap_action.clone();
        Arc::new(move || {
            let mut v = vz.estado(clique, fala, false);
            v["escuta"] = esc.as_ref().map(|e| e.estado()).unwrap_or_else(|| {
                serde_json::json!({
                    "pedida": wake_pedida, "ouvindo": false, "pausada": false,
                    "ativacoes": 0, "ultima": null, "ultimo_erro": null,
                })
            });
            v["palmas"] = serde_json::json!({
                "pedida": clap_pedida,
                "ouvindo": esc.as_ref().is_some_and(|e| clap_pedida && e.estado()["escutando"] == serde_json::json!(true)),
                "acao": clap_acao,
                "acionadas": esc.as_ref().map(|e| e.stats.lock().palmas).unwrap_or(0),
                "recusadas": 0,
                "picos_ultima_hora": 0,
            });
            v
        }) as orion_core::painel::Fonte
    };
    let painel = Arc::new(orion_core::painel::Painel {
        started_at,
        memory: memory.clone(),
        ops: ops.clone(),
        policy: policy.clone(),
        agent: agent.clone(),
        delegator: delegator.clone(),
        modos: Some(modos.clone()),
        custos: Some(custos.clone()),
        jobs: jobs.clone().map(|j| Arc::new(move || j.estado()) as orion_core::painel::Fonte),
        mcp: mcp_geral.clone().map(|m| Arc::new(move || serde_json::json!(m.status())) as orion_core::painel::Fonte),
        telegram_ativo: Some({
            let ativo = telegram.is_some();
            Arc::new(move || ativo)
        }),
        tela: tela.clone().map(|t| Arc::new(move || t.estado()) as orion_core::painel::Fonte),
        voz: Some(voz_painel),
    });
    if let Some(t) = &telegram {
        let p = painel.clone();
        *t.painel.lock() = Some(Arc::new(move || orion_core::painel::texto_do_painel(&p.montar())));
    }
    Ok(Arc::new(AppState {
        accounts: Some(accounts),
        ponte: Default::default(),
        imagens_ponte: Default::default(),
        mcp_host: Some(mcp_host),
        catalog: Some(catalog),
        calendar: Some(calendar),
        events: Some(events),
        file_plans: Some(file_plans),
        skill_runtime,
        extensions,
        started_at,
        painel,
        jobs,
        mcp: mcp_geral,
        ler_depois,
        desfazer: Default::default(),
        export_credentials,
        export,
        pesquisa,
        tela,
        telegram,
        financas,
        precos,
        foco,
        vida_canal,
        gateway_local,
        historicos,
        downloads,
        rotinas,
        executor_rotinas,
        tarefas_bg,
        nuvem: settings.nuvem.then(|| {
            let pasta = if settings.nuvem_dir.trim().is_empty() { std::env::current_dir().unwrap_or_default() } else { std::path::PathBuf::from(settings.nuvem_dir.trim()) };
            Arc::new(orion_core::nuvem::Nuvem::new(memory.clone(), pasta, settings.codex_env.clone(), 180.0))
        }),
        camera_descrever,
        vigia,
        legenda,
        speaker,
        transcriber,
        voz,
        estado_voz,
        escuta,
        desktop_wake,
        settings,
        memory,
        policy,
        ops,
        auth,
        two_factor: Default::default(),
        agent,
        gateway,
        biblioteca,
        modos,
        custos,
        budget,
        delegator,
        processos,
        skills: skills_cat,
    }))
}

pub fn front_dir() -> PathBuf {
    project_root().join("Orion_Core").join("Front_end_Orion")
}

#[allow(clippy::too_many_arguments)]
fn listen_from_settings(
    settings: &Settings,
    agent: Option<Arc<Agent>>,
    transcriber: Option<Arc<orion_ai::transcribe::Transcriber>>,
    speaker: Option<Arc<dyn orion_core::voice::Speaker>>,
    memory: Arc<Store>,
    modos: Arc<Modos>,
    open: Arc<std::sync::atomic::AtomicBool>,
) -> Option<Arc<orion_core::wake::WakeListener>> {
    #[cfg(windows)]
    {
        use orion_core::wake::*;
        if !settings.wake_enabled && !settings.clap_enabled {
            return None;
        }
        let detector: Box<dyn Detector> = if settings.wake_enabled {
            if settings.wake_engine != "openwakeword" {
                tracing::warn!("escuta Rust exige ORION_WAKE_ENGINE=openwakeword");
                return None;
            }
            let Some(model) = &settings.wake_model else {
                tracing::warn!("escuta desligada: falta modelo ONNX treinado para Orion");
                return None;
            };
            match orion_core::native_audio::OpenWakeWord::load(model, settings.wake_threshold as f32) {
                Ok(d) => Box::new(d),
                Err(_) => {
                    tracing::warn!("escuta desligada: modelo openWakeWord inválido");
                    return None;
                }
            }
        } else {
            Box::new(DetectorInerte)
        };
        let device = settings.wake_device.clone();
        let source: SourceFactory = Arc::new(move || orion_core::native_audio::CpalSource::open(&device).map(|s| Box::new(s) as Box<dyn AudioSource>));
        let config = WakeConfig {
            clap_enabled: settings.clap_enabled,
            silence_s: settings.wake_silence_s,
            max_record_s: settings.wake_max_s,
            max_per_hour: settings.wake_max_per_hour as usize,
            clap_max_per_hour: settings.clap_max_per_hour as usize,
            calibrate_frames: 64,
            clap_ratio: settings.clap_ratio,
            ..Default::default()
        };
        let listener = if settings.wake_enabled {
            let (Some(agent), Some(transcriber)) = (agent, transcriber) else {
                tracing::warn!("voz desligada: configure agente e transcrição local");
                return None;
            };
            WakeListener::from_runtime(detector, source, agent, transcriber, speaker, memory, config, None, None, Some(modos.clone()))
        } else {
            Arc::new(WakeListener::new(detector, source, Arc::new(|_| {}), Some(memory), config, None))
        };
        if settings.wake_enabled && settings.voice_barge_in {
            let device = settings.wake_device.clone();
            let delay = settings.voice_aec_delay_ms;
            *listener.interruptible_player.lock() = Some(Arc::new(move |mp3, noise_threshold, abort| {
                orion_voice::play_interruptible(mp3, &device, delay, noise_threshold, abort).map_err(|_| "reprodução com AEC3 indisponível".into())
            }));
        }
        *listener.modos.lock() = Some(modos);
        *listener.on_activate.lock() = Some(Arc::new(move || {
            open.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        Some(listener)
    }
    #[cfg(not(windows))]
    {
        let _ = (settings, agent, transcriber, speaker, memory, modos, open);
        None
    }
}

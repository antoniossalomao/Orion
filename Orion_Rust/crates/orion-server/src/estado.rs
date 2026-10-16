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
    /// `None` enquanto o gateway não está configurado.
    pub agent: Option<Arc<Agent>>,
    pub gateway: Option<Arc<ChatGateway>>,
    pub biblioteca: Arc<Library>,
    /// Pânico e não perturbe (REGRAS.md: pânico é corte; não perturbe só segura).
    pub modos: Arc<Modos>,
    /// Cota gratuita (REGRAS.md: custo zero (sem API paga)).
    pub custos: Arc<Custos>,
    pub delegator: Option<Arc<Delegator>>,
    pub processos: Option<Arc<ProcessManager>>,
    pub skills: Option<Arc<RwLock<SkillCatalog>>>,
}

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
        base_url: settings.local_url.clone(),
        model: settings.local_model.clone(),
        api_key: None,
        timeout_s: timeout,
        tier: CAMADA_LOCAL.into(),
        tools: false,
        provedor: String::new(),
    }
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
        for (camada, modelo, t) in [
            ("rapido", &settings.gateway_model_fast, 60.0),
            ("pesado", &settings.gateway_model_heavy, 120.0),
            ("visao", &settings.vision_model, 90.0),
        ] {
            if !modelo.is_empty() && *modelo != settings.gateway_model {
                let mut ep = Endpoint::new(camada, url, modelo);
                ep.api_key = chave.clone();
                ep.timeout_s = t;
                ep.tier = camada.into();
                eps.push(ep);
            }
        }
    }
    if !settings.local_model.is_empty() {
        // REGRAS.md: o modelo deste computador entra por último e nunca recebe ferramentas
        eps.push(local(settings, 180.0));
    }
    if eps.is_empty() {
        return None;
    }
    ChatGateway::new(eps).ok()
}

/// Auto-compact só com o modelo local, num gateway próprio: o texto da conversa nunca sai do
/// computador para ser resumido (REGRAS.md: resumo da conversa nunca substitui o registro).
pub fn compactor_from_settings(settings: &Settings, memory: Arc<Store>) -> Option<Arc<Compactor>> {
    if settings.compact_at == 0 || settings.local_model.is_empty() {
        return None;
    }
    let gw = ChatGateway::new(vec![local(settings, 300.0)]).ok()?;
    Some(Compactor::new(memory, Arc::new(gw), settings.compact_at, 30, &format!("local:{}", settings.local_model)))
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
    let chave = Some(settings.transcribe_api_key.clone()).filter(|k| !k.is_empty()).or_else(|| get_secret("ORION_TRANSCRIBE_API_KEY"))?;
    orion_ai::transcribe::Transcriber::new(&chave, &settings.transcribe_url, &settings.transcribe_model).ok()
}

/// Sem chave de API, sem embeddings: a busca segue só por palavra-chave.
pub fn embedder_from_settings(settings: &Settings) -> Option<Arc<dyn orion_store::Embedder>> {
    let chave = Some(settings.embed_api_key.clone()).filter(|k| !k.is_empty()).or_else(|| get_secret("ORION_EMBED_API_KEY"))?;
    orion_ai::embedders::GeminiEmbedder::new(&chave, &settings.embed_model, settings.embed_dim).ok().map(|e| Arc::new(e) as Arc<dyn orion_store::Embedder>)
}

pub fn memory_from_settings(settings: &Settings) -> anyhow::Result<Store> {
    let embedder = embedder_from_settings(settings);
    let tem = embedder.is_some();
    match Store::open_with(settings.db_path(), embedder, orion_policy::wall_clock()) {
        Ok(s) => Ok(s),
        Err(e) if tem => {
            // trocou o modelo/dimensão sem `reset_vectors()`: sobe sem vetores em vez de não subir
            tracing::error!("embeddings desligados: {e}");
            Ok(Store::open(settings.db_path())?)
        }
        Err(e) => Err(e.into()),
    }
}

fn chave_de(nomes_settings: Vec<String>, segredos: &'static [&'static str]) -> orion_core::tools::web::Chave {
    Arc::new(move || {
        nomes_settings.iter().find(|v| !v.is_empty()).cloned().or_else(|| segredos.iter().find_map(|s| get_secret(s)))
    })
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
    let ops_audit = ops.clone();
    let modos = Arc::new(Modos::new(
        memory.clone(),
        &settings.dnd_at,
        Some(Arc::new(move |e| ops_audit.audit_add(e).map(|_| ()).map_err(Into::into))),
    ));
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
    let mut agent = None;
    let mut delegator = None;
    let mut processos = None;
    let mut skills_cat = None;
    if let Some(gw) = &gateway {
        // `delegar` só aparece se alguma CLI oficial estiver instalada
        if alguma_cli() {
            delegator = Some(Arc::new(Delegator::new(memory.clone(), orion_ai::delegate::default_agents(), 600.0, 20_000)));
        }
        let mut extra = vec![];
        let indice = Indice::new(&[], settings.mcp_index_at);
        if settings.skills_enabled {
            let cat = SkillCatalog::new(&settings.effective_skills_dir(), vec![]);
            if !cat.skills.is_empty() {
                let _ = policy.register_tool(orion_core::skills::tool_spec(), None);
                let arc = Arc::new(RwLock::new(cat));
                extra.push(skill_tool(arc.clone()));
                skills_cat = Some(arc);
            }
        }
        let mut o = Opcoes { ops: Some(ops.clone()), delegator: delegator.clone(), extra, ..Default::default() };
        if settings.desktop_tools {
            // opt-in (ORION_DESKTOP_TOOLS): age no computador, sempre sob a política
            use orion_core::tools::*;
            o.desktop.extend(desktop::desktop_tools(60.0, 8000));
            o.desktop.extend(fs_tools::fs_tools());
            o.desktop.extend(system_tools::system_tools(15.0));
            o.desktop.extend(documents::document_tools());
            o.desktop.extend(media::media_tools(15.0));
            if let Some(v) = vision_from_settings(&settings) {
                o.desktop.extend(vision::vision_tools(Arc::new(v), settings.data_dir.join("capturas"), 20.0));
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
        let mut a = Agent::new(
            gw.clone(),
            registro,
            policy.clone(),
            memory.clone(),
            AgentConfig { routing: roteamento_ligado(&settings), ..Default::default() },
        );
        a.ops = Some(ops.clone());
        a.skills = skills_cat.clone();
        a.library = Some(biblioteca.clone());
        a.modos = Some(modos.clone());
        a.compactor = compactor_from_settings(&settings, memory.clone());
        *a.indice.write() = Some(indice);
        agent = Some(Arc::new(a));
    }
    Ok(Arc::new(AppState {
        started_at: orion_core::util::agora(),
        settings,
        memory,
        policy,
        ops,
        auth,
        agent,
        gateway,
        biblioteca,
        modos,
        custos,
        delegator,
        processos,
        skills: skills_cat,
    }))
}

pub fn front_dir() -> PathBuf {
    project_root().join("Orion_Core").join("Front_end_Orion")
}

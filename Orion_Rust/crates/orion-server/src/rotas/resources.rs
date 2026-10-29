//! Configurações reais de recursos, fontes e ações nativas da interface autenticada.
use crate::{estado::Estado, http::*};
use axum::{extract::State, Json};
use serde_json::{json, Value};

pub async fn runtime_get(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    Ok(Json(
        json!({"saved":*st.runtime_preferences.lock(), "active":orion_core::runtime_preferences::RuntimePreferences::from_settings(&st.settings),
        "availability":{
            "voice":st.transcriber.is_some(), "speaker":st.speaker.is_some(), "wake":st.escuta.is_some(),
            "camera":st.camera_descrever.is_some(), "screen":st.tela.is_some(), "research":st.pesquisa.is_some(),
            "jobs":st.jobs.is_some(), "native_audio":cfg!(windows),"stt_gpu":cfg!(all(windows,feature="local-vulkan")), "backup":crate::onboarding::BACKUP_KEY.lock().is_some(),
            "ffmpeg":orion_core::util::which("ffmpeg").is_some(), "tts":orion_core::util::which("edge-tts").is_some(),
        }}),
    ))
}

/// Valida e grava as preferências (`PUT /config/runtime` e `settings.toml` usam o mesmo path) e atualiza o espelho.
pub fn apply_preferences(st: &Estado, patch: Value) -> ApiResult<Value> {
    let topics = patch
        .get("research_topics")
        .and_then(Value::as_str)
        .map(|text| {
            text.split(';')
                .map(str::to_string)
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
        });
    if let Some(topics) = &topics {
        orion_core::research::validar_assuntos(topics).map_err(|e| ApiError::new(422, e))?;
    }
    let mut saved = st.runtime_preferences.lock();
    let updated = saved
        .updated(patch)
        .map_err(|e| ApiError::new(422, e.to_string()))?;
    st.ops
        .audit_add(&json!({"tool":"runtime_preferences", "action":"configure"}))?;
    updated.save(&st.settings.data_dir)?;
    if let Some(topics) = topics {
        orion_core::research::salvar_assuntos(&st.memory, &topics)
            .map_err(|e| ApiError::new(422, e))?;
    }
    *saved = updated;
    write_mirror(&st.settings.data_dir, &saved)?;
    Ok(json!({"saved":*saved,"restart_required":true}))
}

/// F07.03: valores da primeira execução, para "Restaurar padrão".

pub async fn runtime_save(
    _: Admin,
    State(st): State<Estado>,
    Json(patch): Json<Value>,
) -> ApiResult<Json<Value>> {
    Ok(Json(apply_preferences(&st, patch)?))
}

pub async fn runtime_defaults(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    Ok(Json(orion_core::runtime_preferences::load_defaults(&st.settings.data_dir)?))
}

pub async fn sources_get(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    Ok(Json(
        serde_json::to_value(st.memory.source_preferences()?).map_err(anyhow::Error::from)?,
    ))
}
pub async fn sources_save(
    _: Admin,
    State(st): State<Estado>,
    Json(patch): Json<Value>,
) -> ApiResult<Json<Value>> {
    let updated = st.memory.source_preferences()?.updated(patch)?;
    st.ops.audit_add(
        &json!({"tool":"sources_preferences", "action":"configure", "sources":updated}),
    )?;
    st.memory.save_source_preferences(&updated)?;
    Ok(Json(
        serde_json::to_value(updated).map_err(anyhow::Error::from)?,
    ))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalCommand {
    command: String,
    #[serde(default)]
    directory: String,
}
pub async fn terminal_run(
    _: Admin,
    State(st): State<Estado>,
    Json(request): Json<TerminalCommand>,
) -> ApiResult<Json<Value>> {
    if st.modos.panico() {
        return erro(409, "modo pânico");
    }
    if request.command.trim().is_empty() || request.command.len() > 8000 {
        return erro(422, "comando inválido");
    }
    if !st.tool_permissions.lock().desktop_tools {
        return erro(409, "habilite Ferramentas do computador nas Configurações");
    }
    // O comando digitado no terminal é ação explícita da pessoa autenticada, não do modelo.
    st.ops
        .audit_add(&json!({"tool":"terminal_ui", "action":"execute"}))?;
    let agent = st
        .agent
        .as_ref()
        .ok_or_else(|| ApiError::new(409, "núcleo indisponível"))?;
    let tool = agent
        .tools
        .read()
        .get("executar_comando")
        .cloned()
        .ok_or_else(|| ApiError::new(409, "terminal indisponível"))?;
    let args = json!({"cmd":request.command,"pasta":request.directory})
        .as_object()
        .unwrap()
        .clone();
    let result = tokio::time::timeout(std::time::Duration::from_secs(65), tool.run(args))
        .await
        .map_err(|_| ApiError::new(504, "tempo do comando esgotado"))?;
    Ok(Json(
        serde_json::from_str(&result).map_err(anyhow::Error::from)?,
    ))
}

/// Salva apenas arquivos de automação conhecidos; editar revoga a concessão anterior.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationFile {
    name: String,
    text: String,
    #[serde(default)]
    expected_hash: String,
}
pub async fn automation_read(
    _: Admin,
    State(st): State<Estado>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> ApiResult<Json<Value>> {
    let path = automation_path(&st, &name)?;
    if !path.exists() {
        return Ok(Json(json!({"name":name,"text":"","hash":""})));
    }
    let meta = std::fs::symlink_metadata(&path)?;
    if meta.file_type().is_symlink() || meta.len() > 16384 {
        return erro(422, "arquivo inválido");
    }
    Ok(Json(
        json!({"name":name,"text":std::fs::read_to_string(&path)?,"hash":orion_core::automacao::hash_do_arquivo(&path)?}),
    ))
}
fn automation_path(st: &Estado, name: &str) -> ApiResult<std::path::PathBuf> {
    if name == "downloads" {
        return Ok(st.downloads.arquivo.clone());
    }
    conferir(name, r"^[a-z0-9][a-z0-9_-]{0,31}$", "name")?;
    Ok(st.rotinas.pasta.join(format!("{name}.yaml")))
}
pub async fn automation_save(
    _: Admin,
    State(st): State<Estado>,
    Json(file): Json<AutomationFile>,
) -> ApiResult<Json<Value>> {
    if st.modos.panico() {
        return erro(409, "modo pânico");
    }
    if file.text.len() > 16384 {
        return erro(422, "arquivo grande demais");
    }
    let path = automation_path(&st, &file.name)?;
    let value =
        orion_core::automacao::yaml_para_json(&file.text).map_err(|e| ApiError::new(422, e))?;
    if file.name == "downloads" {
        let mut rules: orion_core::downloads::Regras =
            serde_json::from_value(value).map_err(|e| ApiError::new(422, e.to_string()))?;
        if rules.regras.is_empty()
            || rules.regras.len() > 50
            || rules.pasta.as_ref().is_some_and(|p| p.len() > 400)
        {
            return erro(422, "regras inválidas");
        }
        for rule in &mut rules.regras {
            rule.validar().map_err(|e| ApiError::new(422, e))?;
        }
    } else {
        let routine: orion_core::rotinas::Rotina =
            serde_json::from_value(value).map_err(|e| ApiError::new(422, e.to_string()))?;
        routine.validar().map_err(|e| ApiError::new(422, e))?;
    }
    if path.exists() {
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || meta.len() > 16384 {
            return erro(422, "arquivo inválido");
        }
        if orion_core::automacao::hash_do_arquivo(&path)? != file.expected_hash {
            return erro(409, "arquivo alterado: carregue novamente antes de salvar");
        }
    } else if !file.expected_hash.is_empty() {
        return erro(409, "arquivo removido: carregue novamente");
    }
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::new(422, "pasta inválida"))?;
    std::fs::create_dir_all(parent)?;
    st.ops
        .audit_add(&json!({"tool":"automation_file","action":"save","name":file.name}))?;
    if file.name == "downloads" {
        st.downloads
            .concessoes
            .revogar(orion_core::downloads::TIPO, orion_core::downloads::NOME)?;
    } else {
        st.rotinas
            .concessoes
            .revogar(orion_core::rotinas::TIPO, &file.name)?;
    }
    use std::io::Write;
    let mut out = tempfile::NamedTempFile::new_in(parent)?;
    out.write_all(file.text.as_bytes())?;
    out.as_file().sync_all()?;
    out.persist(&path).map_err(anyhow::Error::from)?;
    Ok(Json(
        json!({"name":file.name,"text":file.text,"hash":orion_core::automacao::hash_do_arquivo(&path)?,"state":"sem_concessao"}),
    ))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupRequest {
    folder: std::path::PathBuf,
}
pub async fn backup_export(
    _: Admin,
    State(st): State<Estado>,
    Json(request): Json<BackupRequest>,
) -> ApiResult<Json<Value>> {
    if st.modos.panico() {
        return erro(409, "modo pânico");
    }
    if !request.folder.is_absolute() {
        return erro(422, "escolha uma pasta absoluta");
    }
    let key = crate::onboarding::BACKUP_KEY
        .lock()
        .clone()
        .ok_or_else(|| ApiError::new(409, "entre novamente para desbloquear a chave do backup"))?;
    st.ops
        .audit_add(&json!({"tool":"backup_ui","action":"export"}))?;
    let path = tokio::task::spawn_blocking(move || {
        orion_core::onboarding_backup::export(
            &st.memory,
            &st.settings.data_dir,
            &request.folder,
            &key,
        )
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(|e| ApiError::new(422, e.to_string()))?;
    Ok(Json(json!({"file":path})))
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreRequest {
    file: std::path::PathBuf,
    folder: std::path::PathBuf,
    password: String,
    new_password: String,
    #[serde(default)]
    recovery: bool,
}
pub async fn backup_restore(
    _: Admin,
    State(st): State<Estado>,
    Json(mut request): Json<RestoreRequest>,
) -> ApiResult<Json<Value>> {
    use zeroize::Zeroize;
    if st.modos.panico() {
        return erro(409, "modo pânico");
    }
    if !request.file.is_absolute() || !request.folder.is_absolute() {
        return erro(422, "use caminhos absolutos");
    }
    st.ops
        .audit_add(&json!({"tool":"backup_ui","action":"restore"}))?;
    let folder = request.folder.clone();
    tokio::task::spawn_blocking(move || {
        let result = orion_core::onboarding_backup::restore(
            &request.file,
            &request.folder,
            &request.password,
            request.recovery,
            &request.new_password,
        );
        request.password.zeroize();
        request.new_password.zeroize();
        result
    })
    .await
    .map_err(anyhow::Error::from)?
    .map_err(|e| ApiError::new(422, e.to_string()))?;
    Ok(Json(
        json!({"restored":folder,"message":"Backup verificado e restaurado em outra pasta. Abra esse perfil pelo assistente de entrada."}),
    ))
}

pub async fn screen_describe(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    if st.modos.panico() {
        return erro(409, "modo pânico");
    }
    if !st.settings.vision_tools {
        return erro(409, "habilite Descrever imagens e tela nas Configurações");
    }
    let tool = st
        .agent
        .as_ref()
        .and_then(|a| a.tools.read().get("explicar_tela").cloned())
        .ok_or_else(|| ApiError::new(409, "configure um provedor com visão"))?;
    st.ops
        .audit_add(&json!({"tool":"screen_ui","action":"describe"}))?;
    let value = tool.run(Default::default()).await;
    Ok(Json(
        serde_json::from_str(&value).map_err(anyhow::Error::from)?,
    ))
}

pub async fn face_enroll(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    if st.modos.panico() {
        return erro(409, "modo pânico");
    }
    if !(st.settings.camera && st.settings.presence_enabled) {
        return erro(409, "habilite câmera e presença nas Configurações");
    }
    let faces = st
        .vigia
        .as_ref()
        .and_then(|v| v.rostos.clone())
        .ok_or_else(|| ApiError::new(409, "reconhecimento local indisponível"))?;
    if !faces.disponivel() {
        return erro(409, "modelo de rosto indisponível");
    }
    st.ops
        .audit_add(&json!({"tool":"face_ui","action":"enroll"}))?;
    let device = st.settings.camera_device as i32;
    let modes = st.modos.clone();
    let count = tokio::task::spawn_blocking(move || -> anyhow::Result<usize> {
        let mut photos = vec![];
        for _ in 0..orion_core::rosto::MIN_AMOSTRAS {
            anyhow::ensure!(!modes.panico(), "modo pânico");
            photos.push(orion_core::camera::foto_jpeg(device, 8.0)?);
        }
        anyhow::ensure!(!modes.panico(), "modo pânico");
        Ok(faces.cadastrar(&photos, "antonio")?)
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(Json(json!({"enrolled":true,"samples":count})))
}
pub async fn face_delete(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    st.ops
        .audit_add(&json!({"tool":"face_ui","action":"delete"}))?;
    let faces = orion_core::rosto::Rostos::com_lbph(st.memory.clone(), orion_core::rosto::LIMIAR);
    let count = faces.apagar("antonio");
    Ok(Json(json!({"deleted":count})))
}

/// Configuração completa somente para edição na interface administrativa, nunca no status do chat.
pub async fn mcp_configuration(
    _: Admin,
    State(st): State<Estado>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Json<Value>> {
    let connection = st
        .mcp_host
        .as_ref()
        .and_then(|h| h.get(&id))
        .ok_or_else(|| ApiError::new(404, "conexão não encontrada"))?;
    Ok(Json(connection.config.to_json()))
}

/// Arquivo espelho das Configurações (F02.05). Segredos (chaves, tokens) nunca entram nele.
pub const SETTINGS_TOML_FILE: &str = "settings.toml";

fn mirror_text(prefs: &orion_core::runtime_preferences::RuntimePreferences) -> anyhow::Result<String> {
    Ok(format!(
        "# Espelho das Configurações do Orion. Segredos (chaves e tokens) ficam fora deste arquivo.\n{}",
        toml::to_string_pretty(prefs)?
    ))
}

/// Grava o espelho de forma atômica; só troca o arquivo quando o conteúdo está completo.
fn write_mirror(data: &std::path::Path, prefs: &orion_core::runtime_preferences::RuntimePreferences) -> anyhow::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(data)?;
    let mut out = tempfile::NamedTempFile::new_in(data)?;
    out.write_all(mirror_text(prefs)?.as_bytes())?;
    out.as_file().sync_all()?;
    out.persist(data.join(SETTINGS_TOML_FILE)).map_err(|e| e.error)?;
    Ok(())
}

/// `GET /config/toml`: o conteúdo atual do espelho, para editar no app.
pub async fn toml_get(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    let text = mirror_text(&st.runtime_preferences.lock())?;
    Ok(Json(json!({"content": text})))
}

/// `PUT /config/toml`: aplica o text digitado no app; mesmas regras de `PUT /config/runtime`.
pub async fn toml_save(_: Admin, State(st): State<Estado>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    let text = body["content"].as_str().ok_or_else(|| ApiError::new(422, "conteúdo ausente"))?;
    let tabela: toml::Table = toml::from_str(text).map_err(|e| ApiError::new(422, format!("TOML inválido: {e}")))?;
    let patch = serde_json::to_value(tabela).map_err(anyhow::Error::from)?;
    Ok(Json(apply_preferences(&st, patch)?))
}

/// Núcleo atual para o observador: cada reconfiguração troca o ponteiro, sem empilhar observadores.
static CURRENT_CORE: parking_lot::Mutex<Option<Estado>> = parking_lot::Mutex::new(None);
static WATCHER_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Registra o núcleo recém-montado e liga o observador uma única vez.
pub fn register_core(st: &Estado) {
    *CURRENT_CORE.lock() = Some(st.clone());
    if !WATCHER_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        tokio::spawn(watch_settings_toml());
    }
}

/// Observa o `settings.toml`: quando você edita o arquivo à mão, aplica o que der e reescreve o espelho.
pub async fn watch_settings_toml() {
    let mut last_error: Option<String> = None;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let Some(st) = CURRENT_CORE.lock().clone() else { continue };
        watch_once(&st, &mut last_error);
    }
}

fn watch_once(st: &Estado, last_error: &mut Option<String>) {
    let path = st.settings.data_dir.join(SETTINGS_TOML_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        if let Ok(espelho) = mirror_text(&st.runtime_preferences.lock()) {
            let _ = std::fs::write(&path, espelho);
        }
        return;
    };
    let atual = match mirror_text(&st.runtime_preferences.lock()) {
        Ok(t) => t,
        Err(_) => return,
    };
    if text.trim() == atual.trim() {
        return;
    }
    let aplicado = toml::from_str::<toml::Table>(&text)
        .map_err(|e| format!("TOML inválido: {e}"))
        .and_then(|t| serde_json::to_value(t).map_err(|e| e.to_string()))
        .and_then(|patch| apply_preferences(st, patch).map(|_| ()).map_err(|_| "valor recusado".to_string()));
    match aplicado {
        Ok(()) => {
            *last_error = None;
            tracing::info!("settings.toml aplicado");
        }
        Err(e) => {
            if last_error.as_deref() != Some(e.as_str()) {
                tracing::warn!("settings.toml não aplicado (o valor atual continua): {e}");
                *last_error = Some(e);
            }
        }
    }
}

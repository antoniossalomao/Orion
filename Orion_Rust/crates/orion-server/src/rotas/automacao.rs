//! Históricos locais, "posso desligar?", Downloads, rotinas e tarefas em segundo plano.

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, Query, State};
use axum::Json;
use orion_core::{desligar, downloads};
use serde::Deserialize;
use serde_json::{json, Value};

// ── históricos (REGRAS.md: históricos locais: opt-in, só texto, só neste computador) ──

/// 404 quando o histórico está desligado (opt-in); 409 no pânico.
fn historico_ligado(st: &Estado, area: bool) -> ApiResult<()> {
    let ligado = if area { st.settings.clipboard_days > 0 } else { st.settings.command_history };
    if !ligado {
        return erro(404, "histórico desligado (ORION_CLIPBOARD_DAYS / ORION_COMMAND_HISTORY)");
    }
    if st.modos.panico() {
        return erro(409, "modo pânico");
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct AreaTexto {
    texto: String,
}

pub async fn guardar_area(_: Admin, State(st): State<Estado>, Json(c): Json<AreaTexto>) -> ApiResult<Json<Value>> {
    if !(1..=20_000).contains(&c.texto.chars().count()) {
        return erro(422, "texto: entre 1 e 20000 caracteres");
    }
    historico_ligado(&st, true)?;
    Ok(Json(json!({"resultado": st.historicos.guardar_area(&c.texto)?})))
}

#[derive(Deserialize)]
pub struct BuscaQ {
    #[serde(default)]
    q: String,
    limite: Option<i64>,
    dias: Option<i64>,
}

pub async fn ver_area(_: Admin, State(st): State<Estado>, Query(q): Query<BuscaQ>) -> ApiResult<Json<Value>> {
    let limite = q.limite.unwrap_or(30);
    if q.q.chars().count() > 200 || !(1..=100).contains(&limite) {
        return erro(422, "parâmetros inválidos");
    }
    historico_ligado(&st, true)?;
    let itens = if q.q.trim().is_empty() { st.historicos.recentes_area(limite)? } else { st.historicos.buscar_area(&q.q, limite, st.settings.clipboard_days as i64)? };
    Ok(Json(json!({"dias": st.settings.clipboard_days, "itens": itens})))
}

pub async fn limpar_area(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    historico_ligado(&st, true)?;
    Ok(Json(json!({"apagados": st.historicos.limpar_area()?})))
}

pub async fn ver_comandos(_: Admin, State(st): State<Estado>, Query(q): Query<BuscaQ>) -> ApiResult<Json<Value>> {
    let (limite, dias) = (q.limite.unwrap_or(20), q.dias.unwrap_or(90));
    if q.q.chars().count() > 200 || !(1..=50).contains(&limite) || !(1..=365).contains(&dias) {
        return erro(422, "parâmetros inválidos");
    }
    historico_ligado(&st, false)?;
    Ok(Json(json!({"itens": st.historicos.buscar_comandos(&q.q, limite, dias)?})))
}

pub async fn importar_comandos(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    historico_ligado(&st, false)?;
    let h = st.historicos.clone();
    let n = tokio::task::spawn_blocking(move || h.importar()).await.map_err(anyhow::Error::from)??;
    Ok(Json(json!({"novos": n})))
}

pub async fn limpar_comandos(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    historico_ligado(&st, false)?;
    Ok(Json(json!({"apagados": st.historicos.limpar_comandos()?})))
}

// ── posso desligar ───────────────────────────────────────────────────────

pub async fn posso_desligar(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    let tarefas = st.tarefas_bg.as_ref().and_then(|t| t.listar(30).ok()).unwrap_or_default();
    let pasta = std::path::PathBuf::from(st.downloads.estado()["pasta"].as_str().unwrap_or(""));
    let backup = Some(st.settings.effective_backup_dir());
    let repos = st.settings.repos.clone();
    let r = tokio::task::spawn_blocking(move || desligar::posso_desligar(&repos, backup.as_deref(), &pasta, &tarefas)).await.map_err(anyhow::Error::from)?;
    Ok(Json(r))
}

// ── downloads ────────────────────────────────────────────────────────────

pub async fn ver_downloads(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let mut e = st.downloads.estado();
    e["historico"] = json!(st.downloads.historico(20));
    Json(e)
}

pub async fn conceder_downloads(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    let r = st.downloads.conceder().map_err(|e| ApiError::new(422, e))?;
    tracing::info!(target: "orion.audit", "downloads_concedido");
    Ok(Json(r))
}

pub async fn revogar_downloads(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(json!({"ok": st.downloads.revogar()}))
}

#[derive(Deserialize)]
pub struct SimularQ {
    #[serde(default)]
    simular: bool,
}

pub async fn rodar_downloads(_: Admin, State(st): State<Estado>, Query(q): Query<SimularQ>) -> ApiResult<Json<Value>> {
    if st.modos.panico() {
        return erro(409, "modo pânico: o organizador está cortado");
    }
    let d = st.downloads.clone();
    let r = tokio::task::spawn_blocking(move || d.rodar(q.simular)).await.map_err(anyhow::Error::from)?;
    Ok(Json(json!({"status": r.status, "movidos": r.movidos, "ficaram": r.ficaram, "recusados": r.recusados, "simulado": q.simular})))
}

#[derive(Deserialize)]
pub struct DesfazerDownloads {
    #[serde(default = "ultimo")]
    escopo: String,
}

fn ultimo() -> String {
    "ultimo".into()
}

pub async fn desfazer_downloads(_: Admin, State(st): State<Estado>, Json(c): Json<DesfazerDownloads>) -> ApiResult<Json<Value>> {
    conferir(&c.escopo, "^(ultimo|hoje)$", "escopo")?;
    let _ = downloads::NOME;
    Ok(Json(st.downloads.desfazer(&c.escopo).map_err(|e| ApiError::new(422, e))?))
}

// ── rotinas (REGRAS.md: automação sua vale pelo arquivo concedido) ───────

pub fn rotina_json(c: &orion_core::rotinas::Carregada) -> Value {
    let r = c.rotina.as_ref();
    json!({
        "nome": c.nome, "estado": c.estado, "erro": c.erro,
        "descricao": r.map(|r| r.descricao.clone()).unwrap_or_default(),
        "gatilhos": r.map(|r| r.gatilhos.iter().map(|g| match &g.valor { Some(v) => json!({"tipo": g.tipo, "valor": v}), None => json!({"tipo": g.tipo}) }).collect::<Vec<_>>()).unwrap_or_default(),
        "passos": r.map(|r| r.passos.len()).unwrap_or(0),
        "liberar": r.map(|r| r.liberar.clone()).unwrap_or_default(),
    })
}

pub async fn listar_rotinas(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let itens: Vec<Value> = st.rotinas.listar().iter().map(rotina_json).collect();
    Json(json!({"total": itens.len(), "rotinas": itens}))
}

#[derive(Deserialize)]
pub struct FraseRotina {
    texto: String,
}

/// Roda a rotina concedida cuja frase bate EXATAMENTE (sem acento nem pontuação).
pub async fn rotina_por_frase(_: Admin, State(st): State<Estado>, Json(c): Json<FraseRotina>) -> ApiResult<Json<Value>> {
    if !(1..=200).contains(&c.texto.chars().count()) {
        return erro(422, "texto: entre 1 e 200 caracteres");
    }
    let (Some(r), Some(ex)) = (st.rotinas.por_frase(&c.texto), st.executor_rotinas.as_ref()) else { return erro(404, "nenhuma rotina concedida com essa frase") };
    Ok(Json(ex.executar(&r.nome, "frase").await.publico()))
}

pub async fn conceder_rotina(_: Admin, State(st): State<Estado>, Path(nome): Path<String>) -> ApiResult<Json<Value>> {
    match st.rotinas.conceder(&nome) {
        Ok(Some(c)) => {
            tracing::info!(target: "orion.audit", "rotina_concedida rotina={nome}");
            Ok(Json(rotina_json(&c)))
        }
        Ok(None) => erro(404, "rotina não encontrada"),
        Err(e) => erro(422, e),
    }
}

pub async fn revogar_rotina(_: Admin, State(st): State<Estado>, Path(nome): Path<String>) -> Json<Value> {
    Json(json!({"ok": st.rotinas.revogar(&nome)}))
}

#[derive(Deserialize)]
pub struct OrigemQ {
    origem: Option<String>,
}

pub async fn executar_rotina(_: Admin, State(st): State<Estado>, Path(nome): Path<String>, Query(q): Query<OrigemQ>) -> ApiResult<Json<Value>> {
    let origem = q.origem.unwrap_or_else(|| "manual".into());
    conferir(&origem, "^(manual|tecla|frase|telegram|palmas)$", "origem")?;
    let (Some(ex), true) = (st.executor_rotinas.as_ref(), st.rotinas.obter(&nome).is_some()) else { return erro(404, "rotina não encontrada") };
    Ok(Json(ex.executar(&nome, &origem).await.publico()))
}

// ── tarefa em segundo plano (REGRAS.md: tarefa em segundo plano só lê e propõe) ──

fn tarefas(st: &Estado) -> ApiResult<&std::sync::Arc<orion_core::tarefas_bg::TarefasBg>> {
    st.tarefas_bg.as_ref().ok_or_else(|| ApiError::new(503, "tarefa em segundo plano indisponível: falta o gateway"))
}

fn bg<T>(r: Result<T, orion_core::tarefas_bg::TarefaBgError>) -> ApiResult<T> {
    r.map_err(|e| ApiError::new(e.status, e.mensagem))
}

#[derive(Deserialize)]
pub struct TarefaBgPedido {
    objetivo: String,
    max_passos: Option<i64>,
    max_min: Option<i64>,
}

pub async fn criar_tarefa_bg(_: Admin, State(st): State<Estado>, Json(c): Json<TarefaBgPedido>) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    let (passos, min) = (c.max_passos.unwrap_or(10), c.max_min.unwrap_or(10));
    if !(1..=500).contains(&c.objetivo.chars().count()) || !(1..=30).contains(&passos) || !(1..=30).contains(&min) {
        return erro(422, "objetivo (1 a 500), max_passos e max_min (1 a 30)");
    }
    let r = bg(tarefas(&st)?.criar(&c.objetivo, passos, min))?;
    tracing::info!(target: "orion.audit", "tarefa_bg id={} passos={passos}", r["id"]);
    Ok((axum::http::StatusCode::ACCEPTED, Json(r)))
}

pub async fn listar_tarefas_bg(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    let itens = bg(tarefas(&st)?.listar(30))?;
    Ok(Json(json!({"total": itens.len(), "tarefas": itens})))
}

pub async fn ver_tarefa_bg(_: Admin, State(st): State<Estado>, Path(tid): Path<i64>) -> ApiResult<Json<Value>> {
    Ok(Json(bg(tarefas(&st)?.obter(tid))?))
}

pub async fn cancelar_tarefa_bg(_: Admin, State(st): State<Estado>, Path(tid): Path<i64>) -> ApiResult<Json<Value>> {
    Ok(Json(bg(tarefas(&st)?.cancelar(tid))?))
}

#[derive(Deserialize)]
pub struct PropostaDecisao {
    acao: String,
}

pub async fn decidir_proposta_bg(_: Admin, State(st): State<Estado>, Path((tid, indice)): Path<(i64, i64)>, Json(c): Json<PropostaDecisao>) -> ApiResult<Json<Value>> {
    conferir(&c.acao, "^(aceitar|descartar)$", "acao")?;
    Ok(Json(bg(tarefas(&st)?.decidir(tid, indice, &c.acao))?))
}

// ── cliente MCP (E7.3, E7.5) ─────────────────────────────────────────────

/// O que o mcp.json subiu, e o que não.
pub async fn servidores_mcp(_: Admin, State(st): State<Estado>) -> Json<Value> {
    match &st.mcp {
        None => Json(json!({"ativo": false, "servidores": []})),
        Some(m) => Json(json!({"ativo": true, "servidores": m.diagnostico()})),
    }
}

pub async fn testar_servidor_mcp(_: Admin, State(st): State<Estado>, Path(nome): Path<String>) -> ApiResult<Json<Value>> {
    let Some(m) = &st.mcp else { return erro(409, "nenhum servidor MCP configurado em mcp.json") };
    let r = m.testar(&nome).await;
    tracing::info!(target: "orion.audit", "mcp_teste servidor={nome} ok={}", r["ok"] == json!(true));
    if r["erro"] == json!("servidor desconhecido") {
        return erro(404, "servidor desconhecido");
    }
    Ok(Json(r))
}

/// Prompts de servidores MCP (REGRAS.md: recurso e prompt de MCP são dado, não comando): só você
/// invoca, o modelo não os vê.
pub async fn prompts_mcp(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let itens = match &st.mcp {
        Some(m) => m.listar_prompts().await,
        None => vec![],
    };
    Json(json!({"total": itens.len(), "prompts": itens}))
}

#[derive(Deserialize, Default)]
pub struct PromptMcp {
    #[serde(default)]
    argumentos: std::collections::HashMap<String, String>,
}

/// Devolve o texto do prompt para o campo de mensagem; nada é enviado ao modelo aqui.
pub async fn obter_prompt_mcp(_: Admin, State(st): State<Estado>, Path((servidor, nome)): Path<(String, String)>, corpo: Option<Json<PromptMcp>>) -> ApiResult<Json<Value>> {
    let Some(m) = &st.mcp else { return erro(409, "nenhum servidor MCP configurado em mcp.json") };
    let args = corpo.map(|Json(c)| c.argumentos).unwrap_or_default();
    if args.len() > 20 {
        return erro(422, "argumentos: no máximo 20");
    }
    let mut r = m.obter_prompt(&servidor, &nome, &args).await;
    tracing::info!(target: "orion.audit", "mcp_prompt servidor={servidor} prompt={nome}");
    if let Some(e) = r.get("erro").and_then(Value::as_str) {
        return erro(502, e);
    }
    r["origem"] = json!(format!("mcp:{servidor}"));
    Ok(Json(r))
}

// ── pesquisa noturna (E6.5; REGRAS.md: pesquisa noturna só lê, sem aprovação) ──

fn assuntos_da_pesquisa(st: &Estado) -> Value {
    let (assuntos, origem) = orion_core::research::assuntos_salvos(&st.memory, &st.settings.research_topics);
    json!({
        "assuntos": assuntos, "origem": origem, "max": orion_core::research::MAX_ASSUNTOS, "max_tamanho": orion_core::research::MAX_ASSUNTO,
        "hora": if st.settings.research_at.is_empty() { Value::Null } else { json!(st.settings.research_at) },
        "disponivel": st.pesquisa.is_some(), "rodando": st.pesquisa.as_ref().is_some_and(|p| p.esta_rodando()),
    })
}

pub async fn ver_assuntos_da_pesquisa(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(assuntos_da_pesquisa(&st))
}

#[derive(Deserialize)]
pub struct AssuntosPesquisa {
    assuntos: Vec<String>,
}

/// Quem escolhe os assuntos é você (nunca o modelo): até 5, 200 caracteres cada.
pub async fn trocar_assuntos_da_pesquisa(_: Admin, State(st): State<Estado>, Json(c): Json<AssuntosPesquisa>) -> ApiResult<Json<Value>> {
    orion_core::research::salvar_assuntos(&st.memory, &c.assuntos).map_err(|e| ApiError::new(422, e))?;
    tracing::info!(target: "orion.audit", "pesquisa_assuntos total={}", c.assuntos.len());
    Ok(Json(assuntos_da_pesquisa(&st)))
}

pub async fn relatorios_da_pesquisa(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let itens = st.pesquisa.as_ref().map(|p| p.relatorios(30)).unwrap_or_default();
    Json(json!({"total": itens.len(), "relatorios": itens}))
}

/// Dispara a pesquisa em segundo plano. Respeita o pânico e a cota gratuita.
pub async fn rodar_pesquisa_agora(_: Admin, State(st): State<Estado>) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    let Some(p) = st.pesquisa.clone() else { return erro(409, "pesquisa indisponível: falta ORION_WEB_TOOLS, gateway ou vault") };
    if st.modos.panico() {
        return erro(409, "modo pânico: a pesquisa está cortada");
    }
    if p.esta_rodando() {
        return erro(409, "a pesquisa já está rodando");
    }
    if p.assuntos().is_empty() {
        return erro(422, "nenhum assunto definido");
    }
    for prov in ["gateway", "gemini"] {
        if !st.custos.pode_usar(prov, true, "a pesquisa noturna") {
            return erro(429, "cota gratuita do dia quase no fim: tente amanhã");
        }
    }
    tracing::info!(target: "orion.audit", "pesquisa_manual assuntos={}", p.assuntos().len());
    tokio::spawn(async move {
        let _ = p.run(false).await;
    });
    Ok((axum::http::StatusCode::ACCEPTED, Json(json!({"iniciada": true}))))
}

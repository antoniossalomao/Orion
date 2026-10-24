//! Conversas: sessões, histórico, exportação, chat (SSE) e aprovações.

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::Json;
use orion_core::agent::{Agent, TurnRequest};
use orion_core::extensions::context::{ContextReader, ContextRequest};
use orion_core::extensions::skill_runtime::SkillReference;
use orion_store::sessions::{Message, Session};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

const SESSAO_ID: &str = r"^[a-f0-9]{32}$";

fn canal_padrao() -> String {
    "web".into()
}

fn agente(st: &Estado) -> ApiResult<Arc<Agent>> {
    st.agent.clone().filter(|a| a.gateway.configured()).ok_or_else(|| ApiError::new(503, "modelos não configurados (ORION_PROVEDORES ou ORION_GATEWAY_URL/MODEL)"))
}

fn projeto_arquivado(st: &Estado, s: &Session) -> bool {
    s.project_id.as_ref().is_some_and(|p| {
        st.memory.query("SELECT archived FROM projects WHERE id=?", &[p]).ok().and_then(|r| r.first().and_then(|x| x["archived"].as_i64())).unwrap_or(0) != 0
    })
}

pub fn sessao_json(st: &Estado, s: &Session, ativa: Option<&str>) -> Value {
    let project_name = s.project_id.as_deref().and_then(|pid| orion_core::projects::Projects::new(st.memory.clone()).get(pid).ok()).and_then(|p|p["name"].as_str().map(str::to_owned));
    json!({
        "sessao_id": s.id,
        "titulo": s.title.clone().unwrap_or_else(|| "Conversa sem título".into()),
        "canal": s.channel,
        "project_id": s.project_id,
        "project_name": project_name,
        "criada": iso(s.created_at),
        "ultima_atividade": iso(s.last_active_at),
        "ativa": Some(s.id.as_str()) == ativa,
        "favorita": s.favorite,
        "arquivada": s.archived,
        "importada": s.read_only,
        "somente_leitura": s.archived || s.read_only || projeto_arquivado(st, s),
    })
}

fn mensagem_json(st: &Estado, m: &Message) -> Value {
    let mut d = json!({"id": m.id, "role": m.role, "content": m.text, "timestamp": iso(m.created_at), "provenance": m.provenance});
    if m.role == "user" {
        let versoes = st.memory.message_versions(&m.session_id, m.version_of.unwrap_or(m.id)).unwrap_or_default();
        if versoes.len() > 1 {
            // E3.5: o pedido foi editado; as setas ‹ n/m › trocam a exibição
            d["versoes"] = json!(versoes.iter().map(|v| json!({"id": v.id, "texto": v.text, "atual": v.id == m.id})).collect::<Vec<_>>());
        }
    }
    d
}

fn conversa(st: &Estado, canal: &str, sessao: Option<&str>) -> ApiResult<Option<Session>> {
    let s = match sessao {
        Some(id) => st.memory.get_session(id)?,
        None => st.memory.selected_session(canal)?,
    };
    if sessao.is_some() && s.as_ref().is_none_or(|s| s.channel != canal) {
        return erro(404, "sessão inexistente neste canal");
    }
    Ok(s)
}

#[derive(Deserialize)]
pub struct ListarQ {
    #[serde(default = "canal_padrao")]
    canal: String,
    limite: Option<i64>,
    arquivadas: Option<bool>,
    projeto: Option<String>,
}

/// `arquivadas=true`: só as arquivadas. `projeto=<id>` ou `nenhum` (sem projeto).
pub async fn sessoes_listar(_: Admin, State(st): State<Estado>, Query(q): Query<ListarQ>) -> ApiResult<Json<Value>> {
    conferir(&q.canal, CANAL, "canal")?;
    let limite = q.limite.unwrap_or(50);
    if !(1..=100).contains(&limite) {
        return erro(422, "limite fora do intervalo");
    }
    let ativa = st.memory.selected_session(&q.canal)?.map(|s| s.id);
    let arq = q.arquivadas.filter(|a| *a);
    let projeto = q.projeto.filter(|p| !p.is_empty());
    let lista: Vec<Value> = st.memory.list_sessions(Some(&q.canal), limite, arq, projeto.as_deref())?.iter().map(|s| sessao_json(&st, s, ativa.as_deref())).collect();
    Ok(Json(json!({"total": lista.len(), "sessoes": lista, "ativa": ativa})))
}

#[derive(Deserialize)]
pub struct BuscaQ {
    texto: String,
    #[serde(default = "canal_padrao")]
    canal: String,
    limite: Option<i64>,
    offset: Option<i64>,
    arquivadas: Option<bool>,
}

pub async fn buscar_sessoes(_: Admin, State(st): State<Estado>, Query(q): Query<BuscaQ>) -> ApiResult<Json<Value>> {
    conferir(&q.canal, CANAL, "canal")?;
    let (limite, offset) = (q.limite.unwrap_or(25), q.offset.unwrap_or(0));
    if q.texto.is_empty() || q.texto.chars().count() > 200 || !(1..=100).contains(&limite) || !(0..=1_000_000).contains(&offset) {
        return erro(422, "parâmetros inválidos");
    }
    let (achadas, total) = st.memory.search_sessions(&q.canal, &q.texto, limite, offset, q.arquivadas.filter(|a| *a))?;
    let ativa = st.memory.selected_session(&q.canal)?.map(|s| s.id);
    let itens: Vec<Value> = achadas
        .iter()
        .map(|(s, trecho)| {
            let mut v = sessao_json(&st, s, ativa.as_deref());
            v["trecho"] = json!(trecho);
            v
        })
        .collect();
    let proximo = offset + itens.len() as i64;
    Ok(Json(json!({"sessoes": itens, "total": total, "mais": proximo < total, "proximo_offset": if proximo < total { Some(proximo) } else { None }})))
}

#[derive(Deserialize, Default)]
pub struct SessaoNova {
    project_id: Option<String>,
    #[serde(default = "canal_padrao")]
    canal: String,
    titulo: Option<String>,
}

pub async fn sessao_nova(_: Admin, State(st): State<Estado>, corpo: Option<Json<SessaoNova>>) -> ApiResult<Json<Value>> {
    let c = corpo.map(|Json(c)| c).unwrap_or(SessaoNova { canal: "web".into(), ..Default::default() });
    conferir(&c.canal, CANAL, "canal")?;
    if let Some(p) = &c.project_id {
        conferir(p, SESSAO_ID, "project_id")?;
    }
    let titulo = c.titulo.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(|t| t.chars().take(120).collect::<String>());
    let s = st.memory.new_session(&c.canal, titulo.as_deref(), c.project_id.as_deref()).map_err(|_| ApiError::new(422, "project_unavailable"))?;
    let mut v = sessao_json(&st, &s, Some(&s.id));
    v["ok"] = json!(true);
    v["mensagens"] = json!([]);
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct SessaoAtivar {
    sessao_id: String,
    #[serde(default = "canal_padrao")]
    canal: String,
}

pub async fn sessao_ativar(_: Admin, State(st): State<Estado>, Json(c): Json<SessaoAtivar>) -> ApiResult<Json<Value>> {
    conferir(&c.sessao_id, SESSAO_ID, "sessao_id")?;
    conferir(&c.canal, CANAL, "canal")?;
    let s = match st.memory.activate_session(&c.canal, &c.sessao_id) {
        Ok(s) => s,
        // mesmo resultado para inexistente e de outro canal: não revela existência
        Err(orion_store::StoreError::NotFound(_)) => return erro(404, "sessão inexistente neste canal"),
        Err(orion_store::StoreError::Invalid(m)) => return erro(409, m),
        Err(e) => return Err(e.into()),
    };
    let mensagens: Vec<Value> = st
        .memory
        .context_history(&s.id, 50)?
        .iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| json!({"role": m.role, "content": m.text, "timestamp": iso(m.created_at), "provenance": m.provenance}))
        .collect();
    let mut v = sessao_json(&st, &s, Some(&s.id));
    v["ok"] = json!(true);
    v["mensagens"] = json!(mensagens);
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct SessaoEditar {
    #[serde(default = "canal_padrao")]
    canal: String,
    titulo: Option<String>,
    favorita: Option<bool>,
    arquivada: Option<bool>,
}

pub async fn sessao_editar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Json(c): Json<SessaoEditar>) -> ApiResult<Json<Value>> {
    conferir(&id, SESSAO_ID, "session_id")?;
    conferir(&c.canal, CANAL, "canal")?;
    if c.titulo.as_ref().is_some_and(|t| t.is_empty() || t.chars().count() > 120) {
        return erro(422, "titulo: entre 1 e 120 caracteres");
    }
    conversa(&st, &c.canal, Some(&id))?;
    if c.arquivada == Some(true) && (st.policy.approvals.unresolved(&id) || st.agent.as_ref().is_some_and(|a| a.busy(&id))) {
        return erro(409, "termine a resposta e resolva as aprovações antes de arquivar");
    }
    let s = match st.memory.edit_session(&c.canal, &id, c.titulo.as_deref(), c.favorita, c.arquivada) {
        Ok(s) => s,
        Err(orion_store::StoreError::NotFound(_)) => return erro(404, "sessão inexistente neste canal"),
        Err(orion_store::StoreError::Invalid(m)) => return erro(409, m),
        Err(e) => return Err(e.into()),
    };
    let ativa = st.memory.selected_session(&c.canal)?.map(|s| s.id);
    let mut v = sessao_json(&st, &s, ativa.as_deref());
    v["ok"] = json!(true);
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct CanalQ {
    #[serde(default = "canal_padrao")]
    canal: String,
}

/// Só conversa arquivada e sem nada apontando para ela (ramos, artefatos, planos).
pub async fn sessao_apagar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<CanalQ>) -> ApiResult<Json<Value>> {
    conferir(&id, SESSAO_ID, "session_id")?;
    conferir(&q.canal, CANAL, "canal")?;
    if st.policy.approvals.unresolved(&id) || st.agent.as_ref().is_some_and(|a| a.busy(&id)) {
        return erro(409, "termine a resposta e resolva as aprovações antes de apagar");
    }
    match st.memory.delete_session(&q.canal, &id) {
        Ok(()) => Ok(Json(json!({"ok": true}))),
        Err(orion_store::StoreError::NotFound(_)) => erro(404, "sessão inexistente neste canal"),
        Err(orion_store::StoreError::Invalid(m)) => erro(409, m),
        Err(e) => Err(e.into()),
    }
}

/// O resumo gerado do começo da conversa (REGRAS.md: resumo da conversa nunca substitui o
/// registro), se existe.
pub async fn sessao_resumo(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    conferir(&id, SESSAO_ID, "session_id")?;
    if st.memory.get_session(&id)?.is_none() {
        return erro(404, "sessão inexistente");
    }
    Ok(Json(match st.memory.get_summary(&id)? {
        None => json!({"resumo": null}),
        Some(r) => json!({"resumo": {"texto": r["text"], "ate_mensagem": r["upto_message_id"], "modelo": r["model"], "criado": r["created_at"], "gerado": true}}),
    }))
}

pub async fn sessao_resumo_descartar(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    conferir(&id, SESSAO_ID, "session_id")?;
    if st.memory.get_session(&id)?.is_none() {
        return erro(404, "sessão inexistente");
    }
    Ok(Json(json!({"ok": st.memory.drop_summary(&id)?})))
}

#[derive(Deserialize)]
pub struct HistoricoQ {
    #[serde(default = "canal_padrao")]
    canal: String,
    sessao: Option<String>,
    limite: Option<i64>,
    antes: Option<i64>,
    #[serde(default)]
    completo: bool,
}

/// Página por id (do fim para o começo). `completo=true` mostra tudo (limpar nunca apaga).
pub async fn historico(_: Admin, State(st): State<Estado>, Query(q): Query<HistoricoQ>) -> ApiResult<Json<Value>> {
    let limite = q.limite.unwrap_or(50);
    if !(1..=100).contains(&limite) || q.antes.is_some_and(|a| a < 1) || q.sessao.as_ref().is_some_and(|s| s.len() > 64) {
        return erro(422, "parâmetros inválidos");
    }
    let s = conversa(&st, &q.canal, q.sessao.as_deref())?;
    let (msgs, total, cursor) = match &s {
        Some(s) => st.memory.history_page(&s.id, limite, q.antes, q.completo)?,
        None => (vec![], 0, None),
    };
    Ok(Json(json!({
        "sessao": s.as_ref().map(|s| s.id.clone()),
        "total": total,
        "mensagens": msgs.iter().map(|m| mensagem_json(&st, m)).collect::<Vec<_>>(),
        "mais": cursor.is_some(),
        "proximo_antes": cursor,
        "somente_leitura": s.as_ref().is_some_and(|s| s.archived || s.read_only || projeto_arquivado(&st, s)),
    })))
}

#[derive(Deserialize)]
pub struct PedidoEditado {
    texto: String,
    #[serde(default = "canal_padrao")]
    canal: String,
}

/// Reescreve um pedido como nova versão e refaz o turno (E3.5).
pub async fn editar_pedido(_: Admin, State(st): State<Estado>, Path(msg_id): Path<i64>, Json(c): Json<PedidoEditado>) -> ApiResult<Response> {
    if c.texto.is_empty() || c.texto.chars().count() > 8000 {
        return erro(422, "texto: entre 1 e 8000 caracteres");
    }
    conferir(&c.canal, CANAL, "canal")?;
    let a = agente(&st)?;
    let s = st.memory.active_session(&c.canal)?;
    if s.archived || s.read_only || projeto_arquivado(&st, &s) {
        return erro(409, "conversa somente leitura");
    }
    if st.policy.approvals.unresolved(&s.id) || a.busy(&s.id) {
        return erro(409, "termine a resposta e resolva as aprovações antes de editar");
    }
    let achou = st.memory.query("SELECT 1 FROM messages WHERE id=? AND session_id=? AND role='user' AND superseded=0", &[&msg_id, &s.id])?;
    if achou.is_empty() {
        return erro(404, "pedido não encontrado nesta conversa");
    }
    Ok(stream_sse(a.run(TurnRequest { channel: c.canal, text: c.texto, expected_session: Some(s.id), edit_message: Some(msg_id), ..Default::default() })))
}

#[derive(Deserialize)]
pub struct SessaoQ {
    #[serde(default = "canal_padrao")]
    canal: String,
    sessao: Option<String>,
    #[serde(default)]
    completo: bool,
}

pub async fn limpar_historico(_: Admin, State(st): State<Estado>, Query(q): Query<SessaoQ>) -> ApiResult<Json<Value>> {
    let s = conversa(&st, &q.canal, q.sessao.as_deref())?;
    if let Some(s) = &s {
        if s.archived {
            return erro(409, "conversa arquivada: só leitura");
        }
        let r = match &st.agent {
            Some(a) => a.clear_history(&s.id).await,
            None if st.policy.approvals.unresolved(&s.id) => Err(anyhow::anyhow!("resolva as aprovações desta conversa antes de limpar")),
            None => st.memory.clear_context(&s.id).map(|_| ()).map_err(Into::into),
        };
        if let Err(e) = r {
            return erro(409, e.to_string());
        }
    }
    Ok(Json(json!({"ok": true, "sessao": s.map(|s| s.id), "mensagem": "Contexto limpo; registro de mensagens e memória preservados."})))
}

pub async fn exportar(_: Admin, State(st): State<Estado>, Query(q): Query<SessaoQ>) -> ApiResult<Json<Value>> {
    let s = conversa(&st, &q.canal, q.sessao.as_deref())?;
    let mut linhas = vec!["# Conversa com o Orion".to_string(), String::new()];
    let mut total = 0;
    if let Some(s) = &s {
        linhas[0] = format!("# {}",s.title.as_deref().unwrap_or("Conversa com o Orion").replace(['\r','\n']," "));
        // a exportação lê um retrato consistente (mesma transação)
        let paginas = st.memory.tx(|_| {
            let mut antes = None;
            let mut paginas = vec![];
            loop {
                let (msgs, t, prox) = st.memory.history_page(&s.id, 100, antes, q.completo)?;
                total = t;
                paginas.push(msgs);
                antes = prox;
                if antes.is_none() {
                    break;
                }
            }
            Ok(paginas)
        })?;
        for pagina in paginas.iter().rev() {
            for m in pagina {
                let autor = if m.role == "user" { "Antônio" } else { "Orion" };
                linhas.extend([format!("## {autor} — {}", iso(m.created_at)), String::new(), m.text.clone(), String::new()]);
                if let Some(p) = &m.provenance {
                    linhas.push("Proveniência:".into());
                    linhas.push(String::new());
                    for l in serde_json::to_string_pretty(p).unwrap_or_default().lines() {
                        linhas.push(format!("    {l}"));
                    }
                    linhas.push(String::new());
                }
            }
        }
    }
    Ok(Json(json!({"markdown": linhas.join("\n"), "total_msgs": total, "sessao": s.map(|s| s.id)})))
}

#[derive(Deserialize)]
pub struct Mensagem {
    texto: String,
    #[serde(default)]
    anexos: Vec<String>,
    #[serde(default = "canal_padrao")]
    canal: String,
    /// recursos e prompts de MCP escolhidos (dados externos com fonte, nunca privilégios)
    #[serde(default)]
    contexto: Vec<Value>,
    /// skills de extensões escolhidas à mão (até 3)
    #[serde(default)]
    skills: Vec<String>,
    #[serde(default)]
    referencias: Vec<Value>,
    skill: Option<String>,
}

/// Skills de extensões visíveis no escopo da conversa selecionada.
pub async fn listar_skills(_: Admin, State(st): State<Estado>, Query(q): Query<CanalQ>) -> ApiResult<Json<Value>> {
    conferir(&q.canal, CANAL, "canal")?;
    let escopo = match st.memory.selected_session(&q.canal)? {
        Some(s) if s.project_id.is_some() => format!("project:{}", s.project_id.unwrap()),
        _ => "personal".to_string(),
    };
    let itens: Vec<Value> = st.skill_runtime.summaries().into_iter().filter(|s| s["scope"] == json!(escopo)).collect();
    Ok(Json(json!(itens)))
}

#[derive(Deserialize)]
pub struct SkillLigar {
    id: String,
    ativo: bool,
}

/// Liga ou desliga uma skill pela tela. Só mexe no que a fonte já deixou ligado; a escolha fica salva.
pub async fn ligar_skill(_: Admin, State(st): State<Estado>, Json(c): Json<SkillLigar>) -> ApiResult<Json<Value>> {
    if c.id.is_empty() || c.id.chars().count() > 160 {
        return erro(422, "id: entre 1 e 160 caracteres");
    }
    let ok = if c.ativo { st.skill_runtime.retomar(&c.id) } else { st.skill_runtime.pausar(&c.id) };
    if !ok {
        return erro(409, "skill desconhecida, ou de uma fonte desligada, ou já nesse estado");
    }
    let ids = serde_json::to_string(&st.skill_runtime.ids_pausadas()).unwrap_or_else(|_| "[]".into());
    st.memory.meta_set("skills:pausadas", Some(&ids))?;
    tracing::info!(target: "orion.audit", "skill_ligar ativo={}", c.ativo);
    Ok(Json(json!({"ok": true, "id": c.id, "ativa": c.ativo})))
}

/// Primeira linha do pedido, em até 48 caracteres, para nomear a conversa nova.
fn titulo_de(texto: &str) -> String {
    let linha = texto.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Conversa");
    let mut t: String = linha.chars().take(48).collect();
    if linha.chars().count() > 48 {
        t = t.trim_end().to_string();
        t.push('…');
    }
    t
}

pub async fn chat(_: Admin, State(st): State<Estado>, Json(mut c): Json<Mensagem>) -> ApiResult<Response> {
    if c.texto.is_empty() || c.texto.chars().count() > 8000 {
        return erro(422, "texto: entre 1 e 8000 caracteres");
    }
    conferir(&c.canal, CANAL, "canal")?;
    if let Some(sk) = &c.skill {
        conferir(sk, r"^[a-z0-9][a-z0-9-]{0,63}$", "skill")?;
    }
    let a = agente(&st)?;
    if let (Some(r), Some(ex)) = (st.rotinas.por_frase(&c.texto), st.executor_rotinas.clone()) {
        // frase EXATA de uma rotina concedida (REGRAS.md: automação sua vale pelo arquivo concedido)
        return Ok(stream_sse(turno_de_rotina(st.clone(), ex, r.nome)));
    }
    if c.skills.len() > 3 || c.referencias.len() > 8 || c.contexto.len() > 8 || c.anexos.len() > 8 {
        return erro(422, "skills (até 3), referencias e contexto (até 8)");
    }
    let s = st.memory.active_session(&c.canal)?;
    // O prompt só é expandido no escopo selecionado e continua um pedido normal,
    // sujeito à política (REGRAS.md: skill é texto, não código).
    c.texto = st.memory.expand_prompt_command(s.project_id.as_deref(),&c.texto)?;
    if s.title.is_none() {
        // título automático e barato: a primeira linha do primeiro pedido (sem chamar modelo; o usuário pode renomear)
        let _ = st.memory.edit_session(&c.canal, &s.id, Some(&titulo_de(&c.texto)), None, None);
    }
    let escopo = s.project_id.as_ref().map(|p| format!("project:{p}")).unwrap_or_else(|| "personal".into());
    let referencias = c.referencias.iter().map(SkillReference::parse).collect::<Result<Vec<_>, _>>().map_err(|m| ApiError::new(422, m))?;
    let selection = {
        let (rt, texto, skills, escopo) = (st.skill_runtime.clone(), c.texto.clone(), c.skills.clone(), escopo.clone());
        let r = tokio::task::spawn_blocking(move || rt.select(&texto, &skills, &referencias, 12000, &escopo)).await.map_err(|e| anyhow::anyhow!(e))?;
        Some(r?)
    };
    let mut external = vec![];
    if !c.contexto.is_empty() {
        let host = st.mcp_host.clone().ok_or_else(|| ApiError::new(503, "mcp_unavailable"))?;
        let pedidos = c.contexto.iter().map(ContextRequest::parse).collect::<Result<Vec<_>, _>>().map_err(|m| ApiError::new(422, m))?;
        external = ContextReader::new(host).selected(&pedidos, &escopo).await?;
    }
    let mut remaining_attachment_chars = 32000usize;
    for id in &c.anexos {
        conferir(id, SESSAO_ID, "anexo")?;
        let rows = st.memory.query("SELECT name,status FROM uploads WHERE id=? AND project_id IS ?", &[id, &s.project_id])?;
        let row = rows.first().ok_or_else(|| ApiError::new(404, "document_not_found"))?;
        if row["status"] != json!("ready") { return erro(409, "O documento ainda está sendo processado ou falhou. Tente novamente após a extração."); }
        let name = row["name"].as_str().unwrap_or("");
        let source = match &s.project_id { Some(project) => format!("project:{project}/upload:{id}/{name}"), None => format!("upload:{id}/{name}") };
        let chunks = st.memory.query("SELECT c.text FROM chunks c JOIN documents d ON d.id=c.document_id WHERE d.source=? AND d.project_id IS ? ORDER BY c.ord LIMIT 80", &[&source, &s.project_id])?;
        let full = chunks.iter().filter_map(|chunk| chunk["text"].as_str()).collect::<Vec<_>>().join("\n\n");
        let text: String = full.chars().take(remaining_attachment_chars.min(24000)).collect();
        remaining_attachment_chars = remaining_attachment_chars.saturating_sub(text.chars().count());
        use sha2::{Digest, Sha256};
        external.push(orion_core::extensions::context::ExternalData { connection: "user-upload".into(), kind: "document".into(), key: id.clone(), digest: format!("{:x}", Sha256::digest(full.as_bytes())), truncated: text.len() < full.len() || chunks.len() == 80, text });
    }
    Ok(stream_sse(a.run(TurnRequest { channel: c.canal, text: c.texto, external, selection, expected_session: Some(s.id), skill_forcada: c.skill, ..Default::default() })))
}

/// As skills da sua pasta `skills/` (nome e descrição): para `/skill` e a paleta.
pub async fn catalogo_de_skills(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let itens: Vec<Value> = st
        .skills
        .as_ref()
        .map(|c| c.read().skills.values().map(|s| json!({"nome": s.nome, "descricao": s.descricao})).collect())
        .unwrap_or_default();
    Json(json!({"total": itens.len(), "skills": itens}))
}

#[derive(Deserialize, Default)]
pub struct Retomada {
    #[serde(default = "canal_padrao")]
    canal: String,
}

/// Depois de aprovada, executa a ação e deixa o modelo relatar o resultado.
pub async fn retomar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, corpo: Option<Json<Retomada>>) -> ApiResult<Response> {
    let a = agente(&st)?;
    let canal = corpo.map(|Json(c)| c.canal).unwrap_or_else(canal_padrao);
    Ok(stream_sse(a.resume(&canal, &id)))
}

pub async fn pendentes(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(Value::Array(
        st.policy
            .approvals
            .pending(None, false)
            .iter()
            .map(|a| {
                // redigido: quem aprova precisa ver O QUÊ, sem vazar segredo
                json!({
                    "id": a.id, "session_id": a.session_id, "tool": a.tool, "reason": a.reason,
                    "risk": st.policy.spec(&a.tool).and_then(|s| s.risk).map(|r| r.as_str()),
                    "args": orion_policy::redact(&Value::Object(a.args.clone()), 2000),
                    "args_truncated": orion_core::agent::truncado(&a.args),
                    "expires_at": a.expires_at,
                })
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct Decisao {
    approved: bool,
    #[serde(default = "ator")]
    actor: String,
    #[serde(default = "canal_padrao")]
    channel: String,
}

fn ator() -> String {
    "antonio".into()
}

/// Decisão por canal autenticado (REGRAS.md: confirmação fora de banda (canal autenticado)).
pub async fn decidir(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Json(c): Json<Decisao>) -> ApiResult<Json<Value>> {
    if c.approved {
        let approval=st.policy.approvals.get(&id).ok_or_else(||ApiError::new(404,"aprovação inexistente"))?;
        // O servidor não depende de um botão desabilitado na UI
        // (REGRAS.md: só se aprova o que se vê inteiro).
        if orion_core::agent::truncado(&approval.args) {
            return erro(409,"approval_arguments_truncated");
        }
    }
    match st.policy.approvals.decide(&id, c.approved, &c.channel, &c.actor) {
        Ok(a) => Ok(Json(json!({"id": a.id, "status": a.status.as_str()}))),
        Err(orion_policy::approvals::DecideError::NotFound(_)) => erro(404, "aprovação inexistente"),
        Err(e) => erro(409, e.to_string()),
    }
}

fn turno_de_rotina(st: Estado, ex: std::sync::Arc<orion_core::rotinas::Executor>, nome: String) -> impl futures::Stream<Item = orion_core::agent::AgentEvent> {
    use orion_core::agent::AgentEvent;
    async_stream::stream! {
        let r = ex.executar(&nome, "frase").await;
        let icone = match r.status.as_str() { "concluida" => "✅", "aguardando_aprovacao" => "⏸", "recusada" => "⛔", _ => "⚠️" };
        let mut texto = format!("{icone} Rotina {nome}: {}", r.status.replace('_', " "));
        if !r.motivo.is_empty() {
            texto += &format!(" — {}", r.motivo);
        }
        yield AgentEvent::new("text", json!({"text": texto}));
        if let Some(a) = r.aprovacao_id.as_ref().and_then(|id| st.policy.approvals.get(id)) {
            yield AgentEvent::new("approval", json!({
                "id": a.id, "tool": a.tool, "reason": a.reason,
                "risk": st.policy.spec(&a.tool).and_then(|s| s.risk).map(|r| r.as_str()),
                "args": orion_policy::redact(&Value::Object(a.args.clone()), 2000), "args_truncated": orion_core::agent::truncado(&a.args),
            }));
        }
        yield AgentEvent::new("done", json!({"provenance": {"rotina": nome, "passos": r.passos}}));
    }
}

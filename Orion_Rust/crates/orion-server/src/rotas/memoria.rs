//! Fatos auditáveis, projetos, grafo da memória, busca global e preferências de atividade.

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, Query, State};
use axum::Json;
use orion_core::grafo;
use orion_core::projects::{ProjectError, Projects};
use orion_policy::{Action, Context, Status, ToolCall};
use orion_store::{with_scope, DataScope};
use serde::Deserialize;
use serde_json::{json, Map, Value};

const ID32: &str = r"^[a-f0-9]{32}$";

fn escopo<T>(pid: &Option<String>, f: impl FnOnce() -> T) -> T {
    with_scope(DataScope::project(pid.clone(), false), f)
}

fn erro_projeto(e: ProjectError) -> ApiError {
    let status = if e.0 == "project_not_found" || e.0 == "session_not_found" { 404 } else { 409 };
    ApiError::new(status, e.0)
}

// ── fatos ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FatoQ {
    project_id: Option<String>,
    #[serde(default)]
    query: String,
}

fn fato(st: &Estado, id: i64, pid: &Option<String>) -> ApiResult<Map<String, Value>> {
    let mut rows = st.memory.query("SELECT * FROM facts WHERE id=? AND project_id IS ?", &[&id, pid])?;
    if rows.is_empty() {
        return erro(404, "fact_not_found");
    }
    let mut r = rows.remove(0);
    r.remove("embedded");
    Ok(r)
}

pub async fn fatos_listar(_: Admin, State(st): State<Estado>, Query(q): Query<FatoQ>) -> ApiResult<Json<Value>> {
    let busca: String = q.query.chars().take(200).collect::<String>().to_lowercase();
    let v = escopo(&q.project_id, || st.memory.facts())?;
    let v: Vec<_> = v.into_iter().filter(|f| f.text.to_lowercase().contains(&busca)).take(200).collect();
    Ok(Json(json!(v)))
}

#[derive(Deserialize)]
pub struct ProjQ {
    project_id: Option<String>,
}

/// Pares que o ciclo de sono achou contraditórios e esperam a decisão do Antônio.
pub async fn fatos_conflitos(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>) -> ApiResult<Json<Value>> {
    let v = escopo(&q.project_id, || st.memory.pending_conflicts())?;
    Ok(Json(json!(v.iter().map(|(a, b)| json!({"older": a, "newer": b})).collect::<Vec<_>>())))
}

#[derive(Deserialize)]
pub struct Decisao {
    older_id: i64,
    newer_id: i64,
    decision: String,
}

/// "o novo vale" marca o antigo como substituído; "os dois valem" encerra o aviso. Nenhum dos dois
/// apaga nada, e só o Antônio decide (REGRAS.md: o Orion não decide sozinho qual fato vale).
pub async fn fatos_resolver(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>, Json(b): Json<Decisao>) -> ApiResult<Json<Value>> {
    if b.older_id < 1 || b.newer_id < 1 || !["novo", "ambos"].contains(&b.decision.as_str()) {
        return erro(422, "decisão inválida");
    }
    fato(&st, b.older_id, &q.project_id)?;
    fato(&st, b.newer_id, &q.project_id)?;
    match escopo(&q.project_id, || st.memory.resolve_conflict(b.older_id, b.newer_id, &b.decision)) {
        Ok(()) => Ok(Json(json!({"ok": true}))),
        Err(orion_store::StoreError::NotFound(_)) => erro(404, "conflict_not_found"),
        Err(e) => Err(e.into()),
    }
}

const FERRAMENTAS_DE_FATO: [&str; 2] = ["editar_fato", "esquecer_fato"];

pub async fn fatos_pendentes(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>) -> ApiResult<Json<Value>> {
    let mut out = vec![];
    for a in st.policy.approvals.pending(None, true) {
        let Some(sessao) = st.memory.get_session(&a.session_id)? else { continue };
        if sessao.project_id != q.project_id || !FERRAMENTAS_DE_FATO.contains(&a.tool.as_str()) {
            continue;
        }
        let fid = a.args.get("id").cloned().unwrap_or(Value::Null);
        let orig = st.memory.query("SELECT text FROM facts WHERE id=? AND project_id IS ?", &[&fid.as_i64(), &q.project_id])?;
        let Some(orig) = orig.first() else { continue };
        let texto = a.args.get("texto").and_then(Value::as_str).filter(|t| !t.is_empty()).unwrap_or(orig["text"].as_str().unwrap_or(""));
        out.push(json!({
            "id": a.id, "tool": a.tool, "fact_id": fid, "session_id": sessao.id, "reason": a.reason,
            "status": a.status.as_str(), "text": texto, "source": a.args.get("fonte").cloned().unwrap_or(Value::Null),
        }));
    }
    Ok(Json(json!(out)))
}

pub async fn fato_detalhe(_: Admin, State(st): State<Estado>, Path(id): Path<i64>, Query(q): Query<ProjQ>) -> ApiResult<Json<Value>> {
    Ok(Json(Value::Object(fato(&st, id, &q.project_id)?)))
}

/// Apaga o fato por um clique do próprio Antônio. O que o modelo pede passa por `/review` e pela
/// aprovação (REGRAS.md: confirmação fora de banda (canal autenticado)).
pub async fn fato_esquecer(_: Admin, State(st): State<Estado>, Path(id): Path<i64>, Query(q): Query<ProjQ>) -> ApiResult<Json<Value>> {
    fato(&st, id, &q.project_id)?;
    st.policy.approvals.invalidate_fact(id);
    let ok = escopo(&q.project_id, || st.memory.forget_fact(id))?;
    Ok(Json(json!({"ok": ok})))
}

#[derive(Deserialize)]
pub struct Mover {
    project_id: Option<String>,
}

/// Muda onde o fato vale (None = pessoal). Fato de projeto nunca aparece em outro projeto nem sem
/// projeto (REGRAS.md: o que é de um projeto fica no projeto).
pub async fn fato_mover(_: Admin, State(st): State<Estado>, Path(id): Path<i64>, Query(q): Query<ProjQ>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    let Some(alvo) = b.get("project_id") else { return erro(422, "project_id obrigatório") };
    let alvo = alvo.as_str().map(String::from);
    if let Some(p) = &alvo {
        conferir(p, ID32, "project_id")?;
    }
    fato(&st, id, &q.project_id)?;
    if let Some(p) = &alvo {
        let r = st.memory.query("SELECT archived FROM projects WHERE id=?", &[p])?;
        if r.first().is_none_or(|r| r["archived"].as_i64().unwrap_or(1) != 0) {
            return erro(409, "project_missing_or_archived");
        }
    }
    st.policy.approvals.invalidate_fact(id);
    Ok(Json(json!(st.memory.move_fact(id, alvo.as_deref())?)))
}

#[derive(Deserialize)]
pub struct Validade {
    valid_until: Option<f64>,
}

pub async fn fato_validade(_: Admin, State(st): State<Estado>, Path(id): Path<i64>, Query(q): Query<ProjQ>, Json(b): Json<Validade>) -> ApiResult<Json<Value>> {
    if b.valid_until.is_some_and(|v| v < 0.0) {
        return erro(422, "valid_until deve ser >= 0");
    }
    fato(&st, id, &q.project_id)?;
    Ok(Json(json!(escopo(&q.project_id, || st.memory.set_fact_validity(id, b.valid_until))?)))
}

#[derive(Deserialize)]
pub struct Substituir {
    by: i64,
}

pub async fn fato_substituir(_: Admin, State(st): State<Estado>, Path(id): Path<i64>, Query(q): Query<ProjQ>, Json(b): Json<Substituir>) -> ApiResult<Json<Value>> {
    if b.by < 1 {
        return erro(422, "by deve ser >= 1");
    }
    fato(&st, id, &q.project_id)?;
    fato(&st, b.by, &q.project_id)?;
    match escopo(&q.project_id, || st.memory.supersede_fact(id, b.by)) {
        Ok(f) => Ok(Json(json!(f))),
        Err(orion_store::StoreError::Invalid(m)) => erro(422, m),
        Err(e) => Err(e.into()),
    }
}

fn contexto_de_fato(st: &Estado, session_id: Option<&str>, pid: &Option<String>) -> ApiResult<Context> {
    let sessao = match session_id {
        Some(s) => st.memory.get_session(s)?,
        None => Some(st.memory.active_session("web")?),
    };
    let Some(sessao) = sessao.filter(|s| s.channel == "web" && s.project_id == *pid) else { return erro(409, "fact_session_out_of_scope") };
    if sessao.archived || sessao.read_only || st.agent.as_ref().is_some_and(|a| a.busy(&sessao.id)) {
        return erro(409, "session_busy_or_read_only");
    }
    let mut ctx = Context::new(sessao.id.clone());
    ctx.project_id = pid.clone();
    ctx.tainted = st.memory.counter_get(&format!("taint:{}", sessao.id)) > 0;
    if let Some(p) = pid {
        let proj = Projects::new(st.memory.clone()).get(p).map_err(erro_projeto)?;
        if proj["archived"] == Value::Bool(true) {
            return erro(409, "project_archived");
        }
        ctx.project_revision = proj["updated_at"].as_f64();
    }
    Ok(ctx)
}

#[derive(Deserialize)]
pub struct Revisao {
    action: String,
    text: Option<String>,
    source: Option<String>,
    session_id: Option<String>,
}

pub async fn fato_revisar(_: Admin, State(st): State<Estado>, Path(id): Path<i64>, Query(q): Query<ProjQ>, Json(b): Json<Revisao>) -> ApiResult<Json<Value>> {
    if !["edit", "forget"].contains(&b.action.as_str()) {
        return erro(422, "action inválida");
    }
    if b.action == "edit" && b.text.as_ref().is_none_or(|t| t.trim().is_empty()) {
        return erro(422, "fact_text_required");
    }
    if b.text.as_ref().is_some_and(|t| t.chars().count() > 16000) || b.source.as_ref().is_some_and(|t| t.chars().count() > 1000) {
        return erro(422, "texto grande demais");
    }
    if let Some(s) = &b.session_id {
        conferir(s, ID32, "session_id")?;
    }
    fato(&st, id, &q.project_id)?;
    let ctx = contexto_de_fato(&st, b.session_id.as_deref(), &q.project_id)?;
    let (nome, args) = if b.action == "edit" {
        ("editar_fato", json!({"id": id, "texto": b.text, "fonte": b.source}))
    } else {
        ("esquecer_fato", json!({"id": id}))
    };
    let d = st.policy.evaluate_opts(&ToolCall::new(nome, args), &ctx, false);
    match d.action {
        Action::Deny => erro(403, d.reason),
        Action::Confirm => Ok(Json(json!({"approval_id": d.approval_id, "session_id": ctx.session_id, "status": "pending"}))),
        Action::Allow => erro(409, "fact_review_requires_new_approval"),
    }
}

#[derive(Deserialize)]
pub struct Retomar {
    approval_id: String,
}

pub async fn fato_retomar(_: Admin, State(st): State<Estado>, Path(id): Path<i64>, Query(q): Query<ProjQ>, Json(b): Json<Retomar>) -> ApiResult<Json<Value>> {
    if b.approval_id.is_empty() || b.approval_id.chars().count() > 128 {
        return erro(422, "approval_id inválido");
    }
    fato(&st, id, &q.project_id)?;
    let a = st.policy.approvals.get(&b.approval_id);
    let Some(a) = a.filter(|a| a.status == Status::Approved && FERRAMENTAS_DE_FATO.contains(&a.tool.as_str()) && a.args.get("id").and_then(Value::as_i64) == Some(id)) else {
        return erro(409, "fact_approval_unavailable");
    };
    let ctx = contexto_de_fato(&st, Some(&a.session_id), &q.project_id)?;
    let d = st.policy.evaluate(&ToolCall { name: a.tool.clone(), args: a.args.clone() }, &ctx);
    if d.action != Action::Allow {
        return erro(409, "fact_approval_out_of_scope");
    }
    st.policy.approvals.invalidate_fact(id);
    let st2 = st.clone();
    let pid = q.project_id.clone();
    tokio::task::spawn_blocking(move || -> ApiResult<Json<Value>> {
        escopo(&pid, || {
            if a.tool == "esquecer_fato" {
                st2.memory.forget_fact(id)?;
                return Ok(Json(json!({"ok": true, "forgotten": true})));
            }
            let texto = a.args.get("texto").and_then(Value::as_str).unwrap_or("");
            let r = st2.memory.update_fact(id, texto, a.args.get("fonte").and_then(Value::as_str))?;
            Ok(Json(json!({"ok": true, "fact": r})))
        })
    })
    .await
    .map_err(anyhow::Error::from)?
}

// ── projetos ─────────────────────────────────────────────────────────────

fn travar(st: &Estado, session_id: &str) -> ApiResult<()> {
    if st.agent.as_ref().is_some_and(|a| a.busy(session_id)) || st.policy.approvals.unresolved(session_id) {
        return erro(409, "session_busy_or_approval_pending");
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct ArquivadosQ {
    #[serde(default)]
    archived: bool,
}

pub async fn projetos_listar(_: Admin, State(st): State<Estado>, Query(q): Query<ArquivadosQ>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(Projects::new(st.memory.clone()).list(q.archived).map_err(erro_projeto)?)))
}

fn validar_projeto(b: &Value, nome_obrigatorio: bool) -> ApiResult<()> {
    let len = |k: &str| b.get(k).and_then(Value::as_str).map(|s| s.chars().count());
    match b.get("name") {
        None if nome_obrigatorio => return erro(422, "name obrigatório"),
        Some(v) if !v.is_string() => return erro(422, "name inválido"),
        Some(v) if nome_obrigatorio && v.as_str().is_some_and(|s| s.trim().is_empty()) => return erro(422, "project_name_required"),
        _ => {}
    }
    if len("name").is_some_and(|n| n > 120) || len("instructions").is_some_and(|n| n > 16000) || len("root").is_some_and(|n| n > 4096) {
        return erro(422, "campo grande demais");
    }
    Ok(())
}

pub async fn projeto_criar(_: Admin, State(st): State<Estado>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    validar_projeto(&b, true)?;
    let p = Projects::new(st.memory.clone())
        .create(
            b["name"].as_str().unwrap_or("").trim(),
            b["instructions"].as_str().unwrap_or(""),
            b["share_personal"].as_bool().unwrap_or(false),
            b["root"].as_str(),
        )
        .map_err(erro_projeto)?;
    Ok(Json(json!(p)))
}

#[derive(Deserialize)]
pub struct Selecao {
    project_id: Option<String>,
    #[serde(default = "web")]
    channel: String,
}

fn web() -> String {
    "web".into()
}

fn conferir_selecao(b: &Selecao) -> ApiResult<()> {
    if let Some(p) = &b.project_id {
        conferir(p, ID32, "project_id")?;
    }
    conferir(&b.channel, r"^[a-z0-9_-]{1,48}$", "channel")
}

pub async fn projeto_ativar(_: Admin, State(st): State<Estado>, Json(b): Json<Selecao>) -> ApiResult<Json<Value>> {
    conferir_selecao(&b)?;
    if let Some(antiga) = st.memory.selected_session(&b.channel)? {
        travar(&st, &antiga.id)?;
    }
    let s = Projects::new(st.memory.clone()).activate(&b.channel, b.project_id.as_deref()).map_err(erro_projeto)?;
    Ok(Json(json!({"session_id": s.id, "project_id": s.project_id})))
}

pub async fn projeto_detalhe(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let mut row = Projects::new(st.memory.clone()).get(&id).map_err(erro_projeto)?;
    let sessoes = st.memory.query("SELECT id,title,channel,archived FROM sessions WHERE project_id=? ORDER BY last_active_at DESC", &[&id])?;
    let docs = st.memory.query("SELECT id,title,indexed_at FROM documents WHERE project_id=? ORDER BY indexed_at DESC", &[&id])?;
    row.insert("sessions".into(), json!(sessoes));
    row.insert("documents".into(), json!(docs));
    row.insert("extensions".into(), json!([]));
    Ok(Json(Value::Object(row)))
}

pub async fn projeto_editar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    validar_projeto(&b, false)?;
    let sessoes = st.memory.query("SELECT id FROM sessions WHERE project_id=?", &[&id])?;
    if b["archived"].as_bool() == Some(true) {
        for s in &sessoes {
            travar(&st, s["id"].as_str().unwrap_or(""))?;
        }
    }
    for s in &sessoes {
        if st.agent.as_ref().is_some_and(|a| a.busy(s["id"].as_str().unwrap_or(""))) {
            return erro(409, "session_busy");
        }
    }
    st.policy.approvals.invalidate_binding(&id);
    let p = Projects::new(st.memory.clone())
        .update(
            &id,
            b["name"].as_str(),
            b["instructions"].as_str(),
            b["share_personal"].as_bool(),
            b["archived"].as_bool(),
            b["root"].as_str(),
            b.get("root").is_some(),
        )
        .map_err(erro_projeto)?;
    Ok(Json(json!(p)))
}

pub async fn projeto_associar(_: Admin, State(st): State<Estado>, Path(sid): Path<String>, Json(b): Json<Selecao>) -> ApiResult<Json<Value>> {
    conferir_selecao(&b)?;
    travar(&st, &sid)?;
    let s = Projects::new(st.memory.clone()).associate(&b.channel, &sid, b.project_id.as_deref()).map_err(erro_projeto)?;
    Ok(Json(json!({"session_id": s.id, "project_id": s.project_id})))
}

// ── atividade ────────────────────────────────────────────────────────────

fn preferencias(st: &Estado, pid: &Option<String>) -> ApiResult<Value> {
    let chave = pid.clone().unwrap_or_else(|| "personal".into());
    let r = st.memory.query("SELECT completion, question, approval FROM activity_preferences WHERE scope=?", &[&chave])?;
    Ok(match r.first() {
        Some(r) => json!({
            "completion": r["completion"].as_i64() != Some(0), "question": r["question"].as_i64() != Some(0), "approval": r["approval"].as_i64() != Some(0),
        }),
        None => json!({"completion": true, "question": true, "approval": true}),
    })
}

#[derive(Deserialize)]
pub struct AtividadeQ {
    project_id: Option<String>,
    #[serde(default)]
    unread: bool,
}

/// Caixa persistente de avisos e ponte para decisões revisáveis por contexto.
pub async fn atividade_caixa(_: Admin, State(st): State<Estado>, Query(q): Query<AtividadeQ>) -> ApiResult<Json<Value>> {
    let avisos = escopo(&q.project_id, || {
        st.memory.query("SELECT * FROM notifications WHERE (?=0 OR delivered_at IS NULL) ORDER BY id DESC LIMIT 200", &[&(q.unread as i64)])
    })?;
    let mut aprovacoes = vec![];
    for a in st.policy.approvals.pending(None, true) {
        let Some(s) = st.memory.get_session(&a.session_id)? else { continue };
        if s.channel != "web" || s.project_id != q.project_id {
            continue;
        }
        let alvo = match a.tool.as_str() {
            "editar_fato" | "esquecer_fato" => "memoria",
            "aplicar_organizacao" => "fontes",
            "criar_evento_agenda" => "integracoes",
            _ => "chat",
        };
        aprovacoes.push(json!({
            "id": a.id, "tool": a.tool, "reason": a.reason, "session_id": s.id,
            "title": s.title.clone().unwrap_or_else(|| "Conversa".into()), "status": a.status.as_str(), "target": alvo,
        }));
    }
    Ok(Json(json!({"notifications": avisos, "approvals": aprovacoes, "preferences": preferencias(&st, &q.project_id)?})))
}

pub async fn atividade_preferencias(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    let p = |k: &str| b.get(k).map(|v| v.as_bool().unwrap_or(true)).unwrap_or(true);
    let (c, qn, a) = (p("completion"), p("question"), p("approval"));
    let chave = q.project_id.clone().unwrap_or_else(|| "personal".into());
    st.memory.tx(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO activity_preferences(scope,completion,question,approval) VALUES (?,?,?,?)",
            orion_store::rusqlite::params![chave, c as i64, qn as i64, a as i64],
        )?;
        Ok(())
    })?;
    Ok(Json(json!({"completion": c, "question": qn, "approval": a})))
}

// ── grafo e busca ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct GrafoQ {
    limite: Option<usize>,
    #[serde(default)]
    pasta: String,
    projeto: Option<String>,
    project_id: Option<String>,
}

fn esc(projeto: &Option<String>, pid: &Option<String>) -> Option<String> {
    projeto.clone().filter(|s| !s.is_empty()).or_else(|| pid.clone().filter(|s| !s.is_empty()))
}

pub async fn grafo_completo(_: Admin, State(st): State<Estado>, Query(q): Query<GrafoQ>) -> ApiResult<Json<Value>> {
    let limite = q.limite.unwrap_or(grafo::LIMITE_PADRAO);
    if !(10..=3000).contains(&limite) || q.pasta.chars().count() > 200 {
        return erro(422, "parâmetros inválidos");
    }
    let e = esc(&q.projeto, &q.project_id);
    let g = tokio::task::spawn_blocking(move || grafo::montar(&st.memory, e.as_deref(), Some(limite), &q.pasta)).await.map_err(anyhow::Error::from)??;
    Ok(Json(g))
}

#[derive(Deserialize)]
pub struct NoQ {
    q: Option<String>,
    de: Option<String>,
    para: Option<String>,
    projeto: Option<String>,
    project_id: Option<String>,
}

fn texto_ok(v: &Option<String>, min: usize, campo: &str) -> ApiResult<String> {
    match v {
        Some(t) if (min..=200).contains(&t.chars().count()) => Ok(t.clone()),
        _ => erro(422, format!("{campo}: tamanho inválido")),
    }
}

/// "Me leva até X": o nó que melhor casa com o nome dito.
pub async fn grafo_no(_: Admin, State(st): State<Estado>, Query(q): Query<NoQ>) -> ApiResult<Json<Value>> {
    let texto = texto_ok(&q.q, 1, "q")?;
    let e = esc(&q.projeto, &q.project_id);
    let g = tokio::task::spawn_blocking(move || grafo::montar(&st.memory, e.as_deref(), None, "")).await.map_err(anyhow::Error::from)??;
    Ok(Json(json!({"no": grafo::achar_no(g["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]), &texto)})))
}

/// "O que liga X a Y?": o menor caminho entre os dois nós.
pub async fn grafo_caminho(_: Admin, State(st): State<Estado>, Query(q): Query<NoQ>) -> ApiResult<Json<Value>> {
    let (de, para) = (texto_ok(&q.de, 1, "de")?, texto_ok(&q.para, 1, "para")?);
    let e = esc(&q.projeto, &q.project_id);
    let g = tokio::task::spawn_blocking(move || grafo::montar(&st.memory, e.as_deref(), None, "")).await.map_err(anyhow::Error::from)??;
    let nos = g["nodes"].as_array().cloned().unwrap_or_default();
    let (a, b) = (grafo::achar_no(&nos, &de), grafo::achar_no(&nos, &para));
    let (Some(a), Some(b)) = (a.clone(), b.clone()) else {
        return Ok(Json(json!({"de": a, "para": b, "caminho": [], "ligacoes": []})));
    };
    let rota = grafo::caminho(&g, a["id"].as_str().unwrap_or(""), b["id"].as_str().unwrap_or(""));
    let por_id: std::collections::HashMap<&str, &Value> = nos.iter().filter_map(|n| n["id"].as_str().map(|i| (i, n))).collect();
    let pares: std::collections::HashSet<(&str, &str)> = rota.windows(2).map(|w| (w[0].as_str(), w[1].as_str())).collect();
    let ligacoes: Vec<&Value> = g["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|lk| {
            let (s, t) = (lk["source"].as_str().unwrap_or(""), lk["target"].as_str().unwrap_or(""));
            pares.contains(&(s, t)) || pares.contains(&(t, s))
        })
        .collect();
    let caminho: Vec<&Value> = rota.iter().filter_map(|i| por_id.get(i.as_str()).copied()).collect();
    Ok(Json(json!({"de": a, "para": b, "caminho": caminho, "ligacoes": ligacoes})))
}

#[derive(Deserialize)]
pub struct LimQ {
    limite: Option<i64>,
}

/// As relações que o ciclo de sono tirou dos fatos (arestas `kind=sono`).
pub async fn relacoes(_: Admin, State(st): State<Estado>, Query(q): Query<LimQ>) -> ApiResult<Json<Value>> {
    let l = q.limite.unwrap_or(500);
    if !(1..=5000).contains(&l) {
        return erro(422, "limite fora do intervalo");
    }
    Ok(Json(json!(st.ops.list_edges(Some("sono"), l)?)))
}

#[derive(Deserialize)]
pub struct RelQ {
    src: String,
    rel: String,
    dst: String,
}

/// Apaga UMA relação do sono (só `kind=sono`).
pub async fn relacao_apagar(_: Admin, State(st): State<Estado>, Query(q): Query<RelQ>) -> ApiResult<Json<Value>> {
    if !st.ops.remove_edge(&q.src, &q.rel, &q.dst, Some("sono"))? {
        return erro(404, "relation_not_found");
    }
    Ok(Json(json!({"ok": true})))
}

fn trecho(t: &str, n: usize) -> String {
    t.chars().take(n).collect()
}

/// Busca global: conversas, fatos, documentos, memória da tela e resultados gerados, até 5 de
/// cada, no escopo pedido (REGRAS.md: o que é de um projeto fica no projeto).
pub async fn busca(_: Admin, State(st): State<Estado>, Query(q): Query<NoQ>) -> ApiResult<Json<Value>> {
    let texto = texto_ok(&q.q, 2, "q")?;
    let e = esc(&q.projeto, &q.project_id);
    tokio::task::spawn_blocking(move || -> ApiResult<Json<Value>> {
        let mut grupos: Vec<Value> = vec![];
        let (achadas, _) = st.memory.search_sessions("web", &texto, 60, 0, None)?;
        let conversas: Vec<Value> = achadas
            .iter()
            .filter(|(s, _)| s.project_id == e)
            .take(5)
            .map(|(s, t)| json!({"id": s.id, "titulo": s.title.clone().unwrap_or_else(|| "Conversa".into()), "trecho": t}))
            .collect();
        grupos.push(json!({"fonte": "conversas", "titulo": "Conversas", "itens": conversas}));
        let (fatos, docs) = with_scope(DataScope::project(e.clone(), false), || -> ApiResult<_> {
            let fatos: Vec<Value> = st.memory.search_facts(&texto, 5)?.iter().map(|f| json!({"id": f.id, "texto": trecho(&f.text, 200), "antigo": f.old})).collect();
            let docs: Vec<Value> = st
                .memory
                .search(&texto, 5, &[orion_store::search::Kind::Chunk])?
                .iter()
                .map(|h| {
                    let fonte = if h.source.starts_with("project:") { h.source.split_once('/').map(|x| x.1).unwrap_or(&h.source) } else { &h.source };
                    json!({"fonte": fonte, "trecho": trecho(&h.text, 240)})
                })
                .collect();
            Ok((fatos, docs))
        })?;
        grupos.push(json!({"fonte": "fatos", "titulo": "Fatos", "itens": fatos}));
        grupos.push(json!({"fonte": "documentos", "titulo": "Documentos e notas", "itens": docs}));
        if e.is_none() {
            // a memória da tela é pessoal
            let tela: Vec<Value> = st.memory.search_screen(&texto, 5, 90)?.iter().map(|t| json!({"id": t["id"], "ts": t["ts"], "janela": t["titulo"], "trecho": t["trecho"]})).collect();
            grupos.push(json!({"fonte": "tela", "titulo": "Memória da tela", "itens": tela}));
        }
        let like = format!("%{}%", texto.to_lowercase().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        let gerados = st.memory.query(
            "SELECT id, name, kind, version FROM library_results WHERE project_id IS ? AND lower(name) LIKE ? ESCAPE '\\' ORDER BY created_at DESC LIMIT 5",
            &[&e, &like],
        )?;
        let itens: Vec<Value> = gerados.iter().map(|r| json!({"id": r["id"], "nome": r["name"], "tipo": r["kind"], "versao": r["version"]})).collect();
        grupos.push(json!({"fonte": "resultados", "titulo": "Arquivos gerados", "itens": itens}));
        let grupos: Vec<Value> = grupos.into_iter().filter(|g| g["itens"].as_array().is_some_and(|i| !i.is_empty())).collect();
        Ok(Json(json!({"q": texto, "grupos": grupos})))
    })
    .await
    .map_err(anyhow::Error::from)?
}

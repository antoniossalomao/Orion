//! Biblioteca de resultados, imagens, modos, privacidade, atividade, avisos, lembretes e tela.

use crate::estado::Estado;
use crate::http::*;
use axum::body::Body;
use axum::extract::{ConnectInfo, Path, Query, State};
use axum::response::Response;
use axum::Json;
use chrono::{Local, TimeZone};
use orion_core::resultados::{extensao, previa_imagem, PREVIA_PDF, PREVIA_TEXTO};
use orion_store::{with_scope, DataScope};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::LazyLock;

static NOME_IMAGEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^img-\d{9,12}-[0-9a-f]{8}\.(png|jpg|webp)$").unwrap());

fn previa(nome: &str) -> Value {
    let ext = extensao(std::path::Path::new(nome));
    if PREVIA_TEXTO.contains(&ext.as_str()) {
        json!("texto")
    } else if previa_imagem(&ext).is_some() {
        json!("imagem")
    } else if ext == PREVIA_PDF {
        json!("pdf")
    } else {
        Value::Null
    }
}

#[derive(Deserialize)]
pub struct ProjetoQ {
    projeto: Option<String>,
}

pub async fn listar_resultados(_: Admin, State(st): State<Estado>, Query(q): Query<ProjetoQ>) -> ApiResult<Json<Value>> {
    let itens: Vec<Value> = st
        .memory
        .list_artifacts(q.projeto.as_deref(), 100)?
        .iter()
        .map(|a| {
            json!({
                "id": a["id"], "nome": a["name"], "tipo": a["kind"], "versao": a["version"], "anterior": a["parent_id"],
                "bytes": a["bytes"], "ferramenta": a["tool"], "conversa": a["sessao_titulo"], "sessao_id": a["session_id"],
                "projeto_id": a["project_id"], "criado": iso(a["created_at"].as_f64().unwrap_or(0.0)),
                "previa": previa(a["name"].as_str().unwrap_or("")),
            })
        })
        .collect();
    Ok(Json(json!({"total": itens.len(), "resultados": itens})))
}

pub async fn previa_de_texto(_: Admin, State(st): State<Estado>, Path(rid): Path<i64>) -> ApiResult<Json<Value>> {
    let Some(caminho) = st.biblioteca.caminho(rid) else { return erro(404, "resultado não encontrado") };
    if !PREVIA_TEXTO.contains(&extensao(&caminho).as_str()) {
        return erro(415, "sem prévia de texto para este tipo (baixe o arquivo)");
    }
    let texto = String::from_utf8_lossy(&std::fs::read(&caminho).map_err(anyhow::Error::from)?).into_owned();
    let n = texto.chars().count();
    Ok(Json(json!({"texto": texto.chars().take(20_000).collect::<String>(), "truncado": n > 20_000})))
}

fn arquivo(caminho: &std::path::Path, mime: &str, cabecalhos: &[(&str, String)]) -> ApiResult<Response> {
    let dados = std::fs::read(caminho).map_err(anyhow::Error::from)?;
    let mut r = Response::builder().header("content-type", mime);
    for (k, v) in cabecalhos {
        r = r.header(*k, v);
    }
    Ok(r.body(Body::from(dados)).map_err(anyhow::Error::from)?)
}

fn disposicao(tipo: &str, nome: &str) -> String {
    let ascii: String = nome.chars().map(|c| if c.is_ascii() && c != '"' { c } else { '_' }).collect();
    let codificado: String =
        nome.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    format!("{tipo}; filename=\"{ascii}\"; filename*=utf-8''{codificado}")
}

#[derive(Deserialize)]
pub struct PreviaQ {
    #[serde(default)]
    previa: bool,
}

/// Download (sempre anexo); com `previa=true`, só imagem raster e PDF vão inline. HTML e SVG
/// gerados pelo modelo nunca executam no navegador.
pub async fn arquivo_do_resultado(_: Admin, State(st): State<Estado>, Path(rid): Path<i64>, Query(q): Query<PreviaQ>) -> ApiResult<Response> {
    let (Some(caminho), Some(a)) = (st.biblioteca.caminho(rid), st.memory.get_artifact(rid)?) else {
        return erro(404, "resultado não encontrado");
    };
    let nome = a["name"].as_str().unwrap_or("resultado").to_string();
    let sufixo = extensao(std::path::Path::new(&nome));
    let seguros = vec![("X-Content-Type-Options", "nosniff".to_string()), ("Content-Security-Policy", "sandbox".to_string())];
    if q.previa {
        if let Some(mime) = previa_imagem(&sufixo) {
            return arquivo(&caminho, mime, &seguros);
        }
        if sufixo == PREVIA_PDF {
            // o visualizador de PDF do navegador não abre sob `sandbox`: aqui vale a CSP vazia
            return arquivo(
                &caminho,
                "application/pdf",
                &[
                    ("X-Content-Type-Options", "nosniff".into()),
                    ("Content-Security-Policy", "default-src 'none'".into()),
                    ("Content-Disposition", disposicao("inline", &nome)),
                ],
            );
        }
    }
    let mut cab = seguros;
    cab.push(("Content-Disposition", disposicao("attachment", &nome)));
    arquivo(&caminho, "application/octet-stream", &cab)
}

#[derive(Deserialize)]
pub struct DiffQ {
    com: i64,
}

pub async fn diff_do_resultado(_: Admin, State(st): State<Estado>, Path(rid): Path<i64>, Query(q): Query<DiffQ>) -> ApiResult<Json<Value>> {
    match st.biblioteca.diff(rid, q.com) {
        Err(e) => erro(415, e),
        Ok(None) => erro(404, "resultado não encontrado"),
        Ok(Some(v)) => Ok(Json(v)),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimparResultados {
    manter: i64,
    nome: Option<String>,
    #[serde(default)]
    simular: bool,
}

pub async fn limpar_resultados(_: Admin, State(st): State<Estado>, Json(c): Json<LimparResultados>) -> ApiResult<Json<Value>> {
    if !(1..=1000).contains(&c.manter) || c.nome.as_ref().is_some_and(|n| n.is_empty() || n.chars().count() > 120) {
        return erro(422, "parâmetros inválidos");
    }
    let r = st.biblioteca.limpar(c.manter as usize, c.nome.as_deref(), c.simular);
    if !c.simular && r["apagados"].as_u64().unwrap_or(0) > 0 {
        tracing::info!(target: "orion.audit", "resultados_limpos manter={} apagados={}", c.manter, r["apagados"]);
    }
    Ok(Json(r))
}

pub async fn apagar_resultado(_: Admin, State(st): State<Estado>, Path(rid): Path<i64>) -> ApiResult<Json<Value>> {
    if !st.biblioteca.apagar(rid) {
        return erro(404, "resultado não encontrado");
    }
    tracing::info!(target: "orion.audit", "resultado_apagado resultado={rid}");
    Ok(Json(json!({"ok": true})))
}

/// Imagem gerada por `gerar_imagem`: só com login, nome conferido, tipo pela extensão
/// (REGRAS.md: imagem gerada com nome do código, tipo conferido e só com login).
pub async fn imagem(_: Admin, State(st): State<Estado>, Path(nome): Path<String>) -> ApiResult<Response> {
    let Some(c) = NOME_IMAGEM.captures(&nome) else { return erro(404, "imagem inexistente") };
    let caminho = st.settings.data_dir.join("imagens").join(&nome);
    if !caminho.is_file() {
        return erro(404, "imagem inexistente");
    }
    let mime = match &c[1] {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        _ => "image/webp",
    };
    arquivo(
        &caminho,
        mime,
        &[
            ("Cache-Control", "private, max-age=3600".into()),
            ("X-Content-Type-Options", "nosniff".into()),
            ("Content-Security-Policy", "default-src 'none'; sandbox".into()),
        ],
    )
}

// ── modos (REGRAS.md: pânico é corte; não perturbe só segura) ──────────────
pub async fn modo_estado(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(st.modos.estado())
}

#[derive(Deserialize)]
pub struct PanicoControle {
    ativo: bool,
    #[serde(default)]
    senha: String,
}

/// Sair do pânico pela interface pede a senha de novo, com a mesma trava do login.
fn senha_de_novo(st: &Estado, senha: &str, cliente: &str) -> bool {
    if !st.auth.has_password() {
        let esperado = &st.settings.admin_token;
        return !esperado.is_empty() && subtle::ConstantTimeEq::ct_eq(senha.as_bytes(), esperado.as_bytes()).into();
    }
    if st.auth.throttle.espera(cliente) > 0 {
        return false;
    }
    if senha.chars().count() <= 1024 && st.auth.check_password(senha) {
        st.auth.throttle.acertou(cliente);
        return true;
    }
    st.auth.throttle.falhou(cliente);
    false
}

/// Entrar é um clique; sair pede a senha de novo (o pânico nunca volta sozinho).
pub async fn modo_panico(
    _: Admin,
    State(st): State<Estado>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(c): Json<PanicoControle>,
) -> ApiResult<Json<Value>> {
    if c.senha.chars().count() > 1024 {
        return erro(422, "senha longa demais");
    }
    if c.ativo {
        st.modos.entrar_panico("web")?;
        if let Some(e) = &st.escuta {
            e.set_suspended(true);
        }
        return Ok(Json(st.modos.estado()));
    }
    let (st2, cliente) = (st.clone(), addr.ip().to_string());
    let ok = tokio::task::spawn_blocking(move || senha_de_novo(&st2, &c.senha, &cliente)).await.unwrap_or(false);
    if !ok {
        return erro(403, "para sair do modo pânico, confirme a senha");
    }
    st.modos.sair_panico("web").map_err(|e| ApiError::new(503, e.to_string()))?;
    Ok(Json(st.modos.estado()))
}

#[derive(Deserialize)]
pub struct NaoPerturbe {
    ate: Option<String>,
}

pub async fn modo_nao_perturbe(_: Admin, State(st): State<Estado>, Json(c): Json<NaoPerturbe>) -> ApiResult<Json<Value>> {
    match c.ate {
        None => st.modos.desligar_nao_perturbe()?,
        Some(ate) => {
            conferir(&ate, r"^([01]\d|2[0-3]):[0-5]\d$", "ate")?;
            st.modos.ligar_nao_perturbe(&ate).map_err(|e| ApiError::new(422, e.to_string()))?;
        }
    }
    Ok(Json(st.modos.estado()))
}

#[derive(Deserialize)]
pub struct AutonomoControle {
    ativo: bool,
    /// Ligar o modo autônomo pede a senha de novo (PLANO F04.17); desligar não pede.
    senha: Option<String>,
}

pub async fn modo_autonomo(_: Admin, State(st): State<Estado>, Json(c): Json<AutonomoControle>) -> ApiResult<Json<Value>> {
    if c.ativo && !c.senha.as_deref().is_some_and(|s| st.auth.check_password(s)) {
        return Err(ApiError::new(403, "Senha incorreta: o modo autônomo só liga com a sua senha"));
    }
    st.modos.set_autonomo(c.ativo).map_err(|e| ApiError::new(500, e.to_string()))?;
    Ok(Json(st.modos.estado()))
}

#[derive(Deserialize)]
pub struct DiasQ {
    dias: Option<i64>,
}

fn dia(ts: f64) -> String {
    orion_core::util::local(ts).format("%Y-%m-%d").to_string()
}

/// O que saiu do computador (REGRAS.md: toda saída de dado fica registrada, sem conteúdo): por
/// dia e provedor; nunca o conteúdo. O modelo local não conta como saída.
pub fn privacidade_de(st: &Estado, dias: i64, agora: f64) -> ApiResult<Value> {
    let hoje = Local
        .from_local_datetime(&orion_core::util::local(agora).date_naive().and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .map(|d| d.timestamp() as f64)
        .unwrap_or(agora);
    let inicio = hoje - ((dias - 1) * 86400) as f64;
    let linhas: Vec<Map<String, Value>> = st
        .memory
        .external_calls(inicio, None, 50_000)?
        .into_iter()
        .filter(|r| {
            let p = r["provider"].as_str().unwrap_or("");
            !orion_ai::saidas::LOCAIS.contains(&p) && !p.starts_with("local")
        })
        .collect();
    let mut por_dia: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
    for r in &linhas {
        let p = por_dia.entry(dia(r["ts"].as_f64().unwrap_or(0.0))).or_default();
        let prov = r["provider"].as_str().unwrap_or("").to_string();
        let e = p.entry(prov).or_insert_with(|| json!({"envios": 0, "bytes": 0, "tipos": {}}));
        e["envios"] = json!(e["envios"].as_i64().unwrap_or(0) + 1);
        e["bytes"] = json!(e["bytes"].as_i64().unwrap_or(0) + r["bytes_out"].as_i64().unwrap_or(0));
        let tipo = r["content_kind"].as_str().filter(|t| !t.is_empty()).unwrap_or("texto").to_string();
        let n = e["tipos"][&tipo].as_i64().unwrap_or(0) + 1;
        e["tipos"][&tipo] = json!(n);
    }
    let serie: Vec<Value> = (0..dias)
        .rev()
        .map(|i| {
            let d = dia(hoje - (i * 86400) as f64);
            let provs = por_dia.get(&d).cloned().unwrap_or_default();
            json!({
                "dia": d,
                "envios": provs.values().map(|p| p["envios"].as_i64().unwrap_or(0)).sum::<i64>(),
                "bytes": provs.values().map(|p| p["bytes"].as_i64().unwrap_or(0)).sum::<i64>(),
                "provedores": provs,
            })
        })
        .collect();
    let de_hoje: Vec<Value> = linhas
        .iter()
        .filter(|r| r["ts"].as_f64().unwrap_or(0.0) >= hoje)
        .take(200)
        .map(|r| {
            json!({
                "hora": orion_core::util::local(r["ts"].as_f64().unwrap_or(0.0)).format("%H:%M:%S").to_string(),
                "provedor": r["provider"], "tipo": r["kind"],
                "conteudo": r["content_kind"].as_str().filter(|t| !t.is_empty()).unwrap_or("texto"),
                "bytes": r["bytes_out"], "ok": r["ok"].as_i64().unwrap_or(0) != 0,
            })
        })
        .collect();
    let mut provedores: Vec<String> = linhas.iter().filter_map(|r| r["provider"].as_str().map(String::from)).collect();
    provedores.sort();
    provedores.dedup();
    Ok(json!({"dias": dias, "serie": serie, "provedores": provedores, "hoje": de_hoje}))
}

pub async fn privacidade(_: Admin, State(st): State<Estado>, Query(q): Query<DiasQ>) -> ApiResult<Json<Value>> {
    let dias = q.dias.unwrap_or(7);
    if !(1..=90).contains(&dias) {
        return erro(422, "dias fora do intervalo");
    }
    Ok(Json(privacidade_de(&st, dias, orion_core::util::agora())?))
}

#[derive(Deserialize)]
pub struct LimiteQ {
    limite: Option<i64>,
}

/// A caixa de atividade: avisos, aprovações esperando e erros dos jobs. Só leitura.
pub async fn atividade(_: Admin, State(st): State<Estado>, Query(q): Query<LimiteQ>) -> ApiResult<Json<Value>> {
    let limite = q.limite.unwrap_or(30);
    if !(1..=100).contains(&limite) {
        return erro(422, "limite fora do intervalo");
    }
    let avisos: Vec<Value> = st
        .ops
        .recent_notifications(limite)?
        .iter()
        .map(|n| {
            json!({
                "id": n["id"], "tipo": n["kind"],
                "texto": n["text"].as_str().unwrap_or("").chars().take(1500).collect::<String>(),
                "criado": iso(n["created_at"].as_f64().unwrap_or(0.0)),
                "entregue": !n["delivered_at"].is_null(),
            })
        })
        .collect();
    let nao_lidos = avisos.iter().filter(|a| a["entregue"] == json!(false)).count();
    Ok(Json(json!({
        "avisos": avisos, "nao_lidos": nao_lidos,
        "aprovacoes": st.policy.approvals.pending(None, false).len(),
        "erros_dos_jobs": [],
    })))
}

#[derive(Deserialize)]
pub struct ProjetoIdQ {
    project_id: Option<String>,
    minutos: Option<i64>,
}

/// Avisos ainda não entregues; o canal entrega e confirma em `/notifications/{id}/ack`.
pub async fn avisos(_: Admin, State(st): State<Estado>, Query(q): Query<ProjetoIdQ>) -> ApiResult<Json<Value>> {
    let v = with_scope(DataScope::project(q.project_id, false), || st.ops.pending_notifications(50, false))?;
    Ok(Json(json!(v)))
}

pub async fn confirmar_aviso(_: Admin, State(st): State<Estado>, Path(id): Path<i64>, Query(q): Query<ProjetoIdQ>) -> ApiResult<Json<Value>> {
    let ok = with_scope(DataScope::project(q.project_id, false), || st.ops.ack_notification(id))?;
    if !ok {
        return erro(404, "aviso inexistente ou já confirmado");
    }
    Ok(Json(json!({"ok": true})))
}

/// "Feito" conclui (e para de insistir); "adiar" volta a ser um lembrete novo (E9.3).
pub async fn agir_no_lembrete(
    _: Admin,
    State(st): State<Estado>,
    Path((id, acao)): Path<(i64, String)>,
    Query(q): Query<ProjetoIdQ>,
) -> ApiResult<Json<Value>> {
    if acao != "feito" && acao != "adiar" {
        return erro(422, "ação inválida");
    }
    let minutos = q.minutos.unwrap_or(60);
    if !(1..=1440).contains(&minutos) {
        return erro(422, "minutos fora do intervalo");
    }
    let r =
        with_scope(
            DataScope::project(q.project_id, false),
            || {
                if acao == "feito" {
                    st.ops.complete_reminder(id)
                } else {
                    st.ops.snooze_reminder(id, minutos)
                }
            },
        );
    match r {
        Ok(l) => Ok(Json(json!({"ok": true, "lembrete": l}))),
        Err(orion_store::StoreError::NotFound(_)) => erro(404, "lembrete não encontrado"),
        Err(e) => Err(e.into()),
    }
}

#[derive(Deserialize)]
pub struct TelaQ {
    q: String,
    dias: Option<i64>,
    limite: Option<i64>,
}

/// Busca no texto que esteve na tela (E5.1), a pedido do próprio Antônio.
pub async fn tela_busca(_: Admin, State(st): State<Estado>, Query(q): Query<TelaQ>) -> ApiResult<Json<Value>> {
    let (dias, limite) = (q.dias.unwrap_or(7), q.limite.unwrap_or(10));
    let n = q.q.chars().count();
    if !(2..=200).contains(&n) || !(1..=90).contains(&dias) || !(1..=20).contains(&limite) {
        return erro(422, "parâmetros inválidos");
    }
    let achados = st.memory.search_screen(&q.q, limite, dias)?;
    Ok(Json(json!({"resultados": achados.iter().map(|a| json!({
        "id": a["id"], "quando": a["ts"], "janela": a["titulo"], "app": a["app"], "trecho": a["trecho"],
    })).collect::<Vec<_>>()})))
}

/// Apaga TODO o texto de tela guardado.
pub async fn tela_limpar(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    let n = st.memory.clear_screen()?;
    tracing::info!(target: "orion.audit", "tela_limpa registros={n}");
    Ok(Json(json!({"ok": true, "apagados": n})))
}

/// O estado do Orion numa resposta só (modelos, CLIs, aprovações, política, jobs...).
pub async fn painel_unico(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(st.painel.montar())
}

/// Estado da memória da tela: ligada, pausada e contagens; nunca o texto.
pub async fn tela_estado(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(orion_core::screen_memory::estado_da_tela(st.tela.as_deref()))
}

#[derive(Deserialize)]
pub struct EscutaControle {
    ativa: bool,
}

/// `ativa=false` pausa a captura; `ativa=true` retoma. 409 se a memória da tela está desligada.
pub async fn tela_pausar(_: Admin, State(st): State<Estado>, Json(c): Json<EscutaControle>) -> ApiResult<Json<Value>> {
    let Some(t) = &st.tela else { return erro(409, "a memória da tela está desligada") };
    t.pausar(!c.ativa);
    Ok(Json(json!({"pausada": t.pausada()})))
}

#[derive(Deserialize, Default)]
pub struct CameraPedido {
    #[serde(default)]
    pub pergunta: String,
}

/// Uma foto da webcam, descrita (paleta e `/camera`). Só com `ORION_CAMERA=true`
/// (REGRAS.md: a câmera só liga por pedido seu, e cada uso fica registrado).
pub async fn camera_ver(_: Admin, State(st): State<Estado>, Json(c): Json<CameraPedido>) -> ApiResult<Json<Value>> {
    if st.modos.panico() {
        return erro(409, "modo pânico ativo: a câmera está cortada");
    }
    let Some(cd) = st.camera_descrever.clone() else {
        return erro(409, "câmera desligada (ORION_CAMERA)");
    };
    let pergunta: String = c.pergunta.chars().take(1000).collect();
    let r = tokio::task::spawn_blocking(move || cd(&pergunta)).await.map_err(|e| anyhow::anyhow!(e))?;
    tracing::info!(target: "orion.audit", "câmera: foto pedida pela interface");
    let descricao = if r["ok"] == json!(true) {
        r["descricao"].as_str().unwrap_or("").to_string()
    } else {
        format!("Não consegui olhar: {}", r["erro"].as_str().unwrap_or("falha"))
    };
    Ok(Json(json!({
        "ok": r["ok"],
        "descricao": descricao,
        "provedor": r["provedor"],
        "objetos": r["objetos"],
    })))
}

#[derive(Deserialize, Default)]
pub struct LegendaQ {
    #[serde(default)]
    pub limpar: bool,
}

/// Últimas falas da legenda ao vivo (E11.7). Só texto em memória; nada gravado.
pub async fn legenda_estado(_: Admin, State(st): State<Estado>, Query(q): Query<LegendaQ>) -> Json<Value> {
    let Some(leg) = &st.legenda else {
        return Json(json!({"ativa": false, "linhas": []}));
    };
    if q.limpar {
        leg.limpar();
    }
    Json(leg.estado())
}

/// Pausa ou retoma a escuta da palavra de ativação (REGRAS.md: palavra de ativação só abre o
/// microfone; detecção local).
pub async fn escuta_pausar(_: Admin, State(st): State<Estado>, Json(c): Json<EscutaControle>) -> ApiResult<Json<Value>> {
    let Some(e) = &st.escuta else {
        return erro(409, "a escuta da palavra de ativação está desligada");
    };
    if c.ativa && st.modos.panico() {
        return erro(409, "modo pânico ativo: saia dele antes de retomar a escuta");
    }
    e.pausar(!c.ativa);
    Ok(Json(json!({"pausada": e.pausada()})))
}

/// REGRAS.md: painel só lê e nunca mostra segredo.
pub async fn budget_status(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(json!({"quotas":st.budget.summary(),"source":"estimativa_local_em_chamadas"}))
}
pub async fn budget_configure(
    _: Admin,
    State(st): State<Estado>,
    Json(q): Json<std::collections::BTreeMap<String, orion_ai::budget::Quota>>,
) -> ApiResult<Json<Value>> {
    st.budget.configure(q).map_err(|e| ApiError::new(422, e.to_string()))?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
pub struct BudgetCalibration {
    provider: String,
    used: i64,
}
pub async fn budget_calibrate(_: Admin, State(st): State<Estado>, Json(c): Json<BudgetCalibration>) -> ApiResult<Json<Value>> {
    st.budget.calibrate(&c.provider, c.used).map_err(|e| ApiError::new(422, e.to_string()))?;
    Ok(Json(json!({"ok":true})))
}

/// Quais chaves de provedor já estão no cofre do sistema (só sim/não: o valor nunca sai do cofre).
pub async fn chaves_estado(_: Admin, State(_st): State<Estado>) -> ApiResult<Json<Value>> {
    let estado = tokio::task::spawn_blocking(|| {
        let mut m = Map::new();
        for nome in crate::onboarding::SECRET_NAMES {
            m.insert((*nome).into(), json!(orion_ai::secrets::get_secret(nome).is_some_and(|v| !v.is_empty())));
        }
        Value::Object(m)
    })
    .await
    .map_err(anyhow::Error::from)?;
    Ok(Json(estado))
}

#[derive(Deserialize)]
pub struct ChaveEntrada {
    nome: String,
    valor: String,
}

/// Guarda no cofre e atualiza os endpoints entre turnos (REGRAS.md: segredos nunca em log).
pub async fn chave_guardar(_: Admin, State(st): State<Estado>, Json(c): Json<ChaveEntrada>) -> ApiResult<Json<Value>> {
    let valor = c.valor.trim().to_string();
    if !crate::onboarding::SECRET_NAMES.contains(&c.nome.as_str()) {
        return Err(ApiError::new(422, "chave de integração desconhecida"));
    }
    if valor.is_empty() || valor.len() > 16384 || valor.contains(['\r', '\n']) {
        return Err(ApiError::new(422, "a chave está vazia ou tem caracteres inválidos"));
    }
    let nome = c.nome.clone();
    tokio::task::spawn_blocking(move || orion_ai::secrets::set_secret(&nome, &valor))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|_| ApiError::new(500, "não foi possível guardar no cofre do sistema"))?;
    if c.nome.starts_with("ORION_KEY_") {
        recarregar_gateway(&st, st.settings.clone()).await?;
    }
    Ok(Json(json!({"ok": true, "reiniciar": !c.nome.starts_with("ORION_KEY_")})))
}

#[derive(Deserialize)]
pub struct LanguageChoice { language: String }

/// Preferência local autenticada; só altera o idioma (REGRAS.md: toda rota da API exige login).
pub async fn language_save(_: Admin, State(st): State<Estado>, Json(choice): Json<LanguageChoice>) -> ApiResult<Json<Value>> {
    if !["pt-BR", "en"].contains(&choice.language.as_str()) { return Err(ApiError::new(422, "idioma inválido")); }
    let (path, value) = match orion_core::onboarding::Profile::load(&st.settings.data_dir)? {
        Some(mut profile) => {
            profile.language = choice.language;
            (st.settings.data_dir.join(orion_core::onboarding::FILE), serde_json::to_value(profile).map_err(anyhow::Error::from)?)
        }
        // Instalações legadas podem ter conta sem perfil. Não inventa um perfil de primeiro uso.
        None => (st.settings.data_dir.join("interface.json"), json!({"language":choice.language})),
    };
    let mut staged = tempfile::NamedTempFile::new_in(&st.settings.data_dir).map_err(anyhow::Error::from)?;
    use std::io::Write;
    staged.write_all(&serde_json::to_vec_pretty(&value).map_err(anyhow::Error::from)?).map_err(anyhow::Error::from)?;
    staged.as_file().sync_all().map_err(anyhow::Error::from)?;
    staged.persist(path).map_err(|e| anyhow::anyhow!(e.error))?;
    Ok(Json(json!({"ok":true})))
}

/// Refaz os provedores e os endpoints do gateway entre turnos, com as chaves do cofre e o catálogo do banco.
async fn recarregar_gateway(st: &Estado, mut settings: orion_core::config::Settings) -> ApiResult<()> {
    if !settings.providers_explicit { settings.provedores.clear(); }
    crate::estado::load_providers(&mut settings, &st.memory)?;
    if let Some(gateway) = &st.gateway {
        let mut endpoints = crate::estado::endpoints_dos_provedores(&settings, None);
        // Preserva também gateway explícito e reserva local, com seus opt-ins.
        if let Some(configured) = crate::estado::gateway_from_settings(&settings) {
            endpoints = configured.endpoint_snapshot();
        }
        gateway.configure(endpoints).await;
    }
    Ok(())
}

/// Campo a campo do `Debug` das configurações: só nomes e textos para comparar, nunca saem daqui.
fn campos_das_configuracoes(s: &orion_core::config::Settings) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::<String, String>::new();
    let mut atual: Option<String> = None;
    for linha in format!("{s:#?}").lines().skip(1) {
        let nome = linha.strip_prefix("    ").filter(|r| !r.starts_with(' ')).and_then(|r| r.split_once(": ")).filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        match nome {
            Some((n, v)) => {
                atual = Some(n.to_string());
                out.insert(n.to_string(), v.to_string());
            }
            None => {
                if let Some(n) = &atual {
                    out.entry(n.clone()).and_modify(|v| { v.push('\n'); v.push_str(linha); });
                }
            }
        }
    }
    out
}

/// Campos que o servidor define ao iniciar (perfil, instalador, banco): diferem do `.env` sem o usuário ter mudado nada.
const DEFINIDOS_NA_INICIALIZACAO: [&str; 11] = ["data_dir", "vault_dir", "admin_token", "auth_user", "seed_default_password", "host", "port", "allow_public_bind", "cookie_secure", "jobs_enabled", "provedores"];
/// Mudanças que refazem o gateway na hora.
const DO_GATEWAY: [&str; 12] = ["gateway_url", "gateway_model", "gateway_model_fast", "gateway_model_heavy", "gateway_api_key", "vision_model", "local_model", "local_model_path", "local_model_gpu", "local_url", "allow_paid", "providers_explicit"];

/// F07.08: relê o `.env` e o ambiente e aplica o que dá sem reiniciar. O resto vai listado em
/// `exigem_reinicio`. Só nomes de campos saem daqui, nunca valores (REGRAS.md: segredos nunca em log).
pub async fn config_recarregar(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    let novo = tokio::task::spawn_blocking(orion_core::config::Settings::load)
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| ApiError::new(422, format!("configuração inválida: {e}")))?;
    let (antes, depois) = (campos_das_configuracoes(&st.settings), campos_das_configuracoes(&novo));
    let mudados: Vec<String> = depois.iter().filter(|(n, v)| !DEFINIDOS_NA_INICIALIZACAO.contains(&n.as_str()) && antes.get(*n) != Some(*v)).map(|(n, _)| n.clone()).collect();
    let (mut aplicados, mut reinicio) = (vec![], vec![]);
    let mut refazer_gateway = false;
    for n in mudados {
        match n.as_str() {
            "injection_guard" => { st.viva.injecao.store(novo.injection_guard, std::sync::atomic::Ordering::Relaxed); aplicados.push(n); }
            "vault_diario" => { st.viva.diario.store(novo.vault_diario, std::sync::atomic::Ordering::Relaxed); aplicados.push(n); }
            n_ if DO_GATEWAY.contains(&n_) => { refazer_gateway = true; aplicados.push(n); }
            _ => reinicio.push(n),
        }
    }
    if refazer_gateway {
        let mut s = novo.clone();
        s.data_dir = st.settings.data_dir.clone();
        s.vault_dir = st.settings.vault_dir.clone();
        recarregar_gateway(&st, s).await?;
    }
    Ok(Json(json!({"ok": true, "aplicados": aplicados, "exigem_reinicio": reinicio})))
}

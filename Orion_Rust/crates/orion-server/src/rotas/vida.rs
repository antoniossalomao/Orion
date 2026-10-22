//! E10: rotas de finanças, contas, preços e simulação. REGRAS.md: tudo local, nada sai daqui.

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{Datelike, NaiveDate};
use orion_core::vida::{self, lancar_fatura, linha_do_tempo, parse_fatura, parse_gasto, parse_valor, simular};
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct MesQ {
    ano: Option<i32>,
    mes: Option<u32>,
}

pub async fn resumo(_: Admin, State(st): State<Estado>, Query(q): Query<MesQ>) -> ApiResult<Json<Value>> {
    let h = chrono::Local::now();
    st.financas.resumo_mensal(q.ano.unwrap_or(h.year()), q.mes.unwrap_or(h.month())).map(Json).map_err(|_| ApiError::new(422, "mês inválido"))
}

#[derive(Deserialize)]
pub struct PedidoGasto {
    texto: String,
}

pub async fn lancar_gasto(_: Admin, State(st): State<Estado>, Json(c): Json<PedidoGasto>) -> ApiResult<Json<Value>> {
    if !(1..=300).contains(&c.texto.chars().count()) {
        return erro(422, "texto: entre 1 e 300 caracteres");
    }
    let Some(g) = parse_gasto(&c.texto) else { return erro(422, "não achei o valor (ex.: 'gastei 30 no mercado')") };
    Ok(Json(st.financas.registrar_gasto(g["cents"].as_i64().unwrap_or(0), g["category"].as_str().unwrap_or("outros"), g["description"].as_str().unwrap_or(""), "chat").map_err(|e| ApiError::new(422, e.0))?))
}

pub async fn contas(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"contas": st.financas.contas_abertas().map_err(|e| ApiError::new(500, e.0))?})))
}

#[derive(Deserialize)]
pub struct PedidoConta {
    descricao: String,
    reais: f64,
    vencimento: String,
}

pub async fn cadastrar_conta(_: Admin, State(st): State<Estado>, Json(c): Json<PedidoConta>) -> ApiResult<Json<Value>> {
    if !(1..=200).contains(&c.descricao.chars().count()) || !(c.reais > 0.0 && c.reais <= 10_000_000.0) {
        return erro(422, "descrição ou valor fora do limite");
    }
    let cents = parse_valor(&c.reais.to_string()).map_err(|e| ApiError::new(422, e.0))?;
    let Ok(venc) = NaiveDate::parse_from_str(&c.vencimento, "%Y-%m-%d") else { return erro(422, "vencimento: use AAAA-MM-DD") };
    Ok(Json(st.financas.cadastrar_conta(&c.descricao, cents, venc, "", "").map_err(|e| ApiError::new(422, e.0))?))
}

pub async fn pagar(_: Admin, State(st): State<Estado>, Path(rid): Path<i64>) -> ApiResult<Json<Value>> {
    st.financas.pagar_conta(rid).map(Json).map_err(|e| ApiError::new(if e.0.contains("já paga") { 409 } else { 404 }, e.0))
}

pub async fn precos(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({"itens": st.precos.listar().map_err(|e| ApiError::new(500, e.0))?})))
}

#[derive(Deserialize)]
pub struct PedidoPreco {
    item: String,
    consulta: String,
    alvo_reais: Option<f64>,
}

pub async fn acompanhar(_: Admin, State(st): State<Estado>, Json(c): Json<PedidoPreco>) -> ApiResult<Json<Value>> {
    if !(1..=120).contains(&c.item.chars().count()) || !(1..=200).contains(&c.consulta.chars().count()) || c.alvo_reais.is_some_and(|a| !(a > 0.0 && a <= 10_000_000.0)) {
        return erro(422, "campo fora do limite");
    }
    let alvo = match c.alvo_reais {
        Some(a) => Some(parse_valor(&a.to_string()).map_err(|e| ApiError::new(422, e.0))?),
        None => None,
    };
    Ok(Json(st.precos.adicionar(&c.item, &c.consulta, alvo).map_err(|e| ApiError::new(422, e.0))?))
}

#[derive(Deserialize)]
pub struct PedidoSimulacao {
    saldo_reais: f64,
    #[serde(default)]
    extra_mensal_reais: f64,
    meses: u32,
}

pub async fn simulacao(_: Admin, State(st): State<Estado>, Json(c): Json<PedidoSimulacao>) -> ApiResult<Json<Value>> {
    if c.saldo_reais.abs() > 10_000_000.0 || !(0.0..=10_000_000.0).contains(&c.extra_mensal_reais) || !(1..=120).contains(&c.meses) {
        return erro(422, "campo fora do limite");
    }
    let hist = st.financas.historico_mensal(12).map_err(|e| ApiError::new(500, e.0))?;
    let faixas = simular((c.saldo_reais * 100.0).round() as i64, &hist, (c.extra_mensal_reais * 100.0).round() as i64, c.meses, 10_000, None).map_err(|e| ApiError::new(409, e.0))?;
    Ok(Json(json!({"meses": c.meses, "base_meses": hist.len(), "faixas_cents": faixas, "aviso": "faixas de cenário, não previsão"})))
}

#[derive(Deserialize)]
pub struct PedidoFatura {
    texto: String,
}

/// Lê a fatura (texto já extraído do PDF, localmente) e devolve as linhas para revisar.
pub async fn previa_fatura(_: Admin, Json(c): Json<PedidoFatura>) -> ApiResult<Json<Value>> {
    if !(1..=60_000).contains(&c.texto.chars().count()) {
        return erro(422, "texto: entre 1 e 60000 caracteres");
    }
    let linhas = parse_fatura(&c.texto, None);
    let total: i64 = linhas.iter().map(|x| x["cents"].as_i64().unwrap_or(0)).sum();
    Ok(Json(json!({"linhas": linhas, "total_cents": total})))
}

#[derive(Deserialize)]
pub struct LinhaFatura {
    descricao: String,
    cents: i64,
}

#[derive(Deserialize)]
pub struct ConfirmacaoFatura {
    linhas: Vec<LinhaFatura>,
    confirmado: bool,
}

pub async fn confirmar_fatura(_: Admin, State(st): State<Estado>, Json(c): Json<ConfirmacaoFatura>) -> ApiResult<Json<Value>> {
    if c.linhas.is_empty() || c.linhas.len() > 500 || c.linhas.iter().any(|l| !(1..=200).contains(&l.descricao.chars().count()) || !(1..=1_000_000_000).contains(&l.cents)) {
        return erro(422, "linhas fora do limite");
    }
    let linhas: Vec<Value> = c.linhas.iter().map(|l| json!({"cents": l.cents, "descricao": l.descricao})).collect();
    let n = lancar_fatura(&st.financas, &linhas, c.confirmado).map_err(|e| ApiError::new(422, e.0))?;
    Ok(Json(json!({"lancadas": n})))
}

#[derive(Deserialize)]
pub struct IntervaloQ {
    de: Option<f64>,
    ate: Option<f64>,
}

/// E10.8: conversas, gastos, tela, área de transferência, cápsulas, documentos, diário e commits.
pub async fn timeline(_: Admin, State(st): State<Estado>, Query(q): Query<IntervaloQ>) -> ApiResult<Json<Value>> {
    let agora = orion_core::util::agora();
    let (ini, fim) = (q.de.unwrap_or(agora - 7.0 * 86400.0), q.ate.unwrap_or(agora));
    if fim < ini || fim - ini > 62.0 * 86400.0 {
        return erro(422, "intervalo inválido (até 62 dias)");
    }
    let (m, v, r) = (st.memory.clone(), st.settings.vault_dir.clone(), st.settings.repos.clone());
    let itens = tokio::task::spawn_blocking(move || linha_do_tempo(&m, ini, fim, v.as_deref(), &r)).await.map_err(anyhow::Error::from)?.map_err(|e| ApiError::new(500, e.0))?;
    Ok(Json(json!({"itens": itens})))
}

fn fechar_sonho(st: &Estado, rid: i64) -> ApiResult<()> {
    let agora = st.memory.now();
    st.memory.tx(|c| Ok(c.execute("UPDATE notifications SET delivered_at=? WHERE id=? AND kind='sonho'", orion_store::rusqlite::params![agora, rid]).map(|_| ())?))?;
    Ok(())
}

/// "boa" grava a ideia como fato (E10.9). Só então ela entra na memória.
pub async fn sonho_boa(_: Admin, State(st): State<Estado>, Path(rid): Path<i64>) -> ApiResult<Json<Value>> {
    let rows = st.memory.query("SELECT text FROM notifications WHERE id=? AND kind='sonho' AND delivered_at IS NULL", &[&rid])?;
    let Some(r) = rows.first() else { return erro(404, "sonho não encontrado") };
    let t = r["text"].as_str().unwrap_or("");
    let ideia = t.strip_prefix("Sonho do dia: ").unwrap_or(t).to_string();
    st.memory.add_fact(&ideia, "sonho", None)?;
    fechar_sonho(&st, rid)?;
    Ok(Json(json!({"fato": ideia})))
}

pub async fn sonho_descartar(_: Admin, State(st): State<Estado>, Path(rid): Path<i64>) -> ApiResult<Json<Value>> {
    fechar_sonho(&st, rid)?;
    Ok(Json(json!({"ok": true})))
}

pub async fn foco_estado(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(json!({"ativo": st.foco.ativo()}))
}

#[derive(Deserialize)]
pub struct PedidoFoco {
    minutos: i64,
}

/// REGRAS.md: o bloco só habilita a pergunta discreta; não bloqueia nada.
pub async fn foco_iniciar(_: Admin, State(st): State<Estado>, Json(c): Json<PedidoFoco>) -> ApiResult<Json<Value>> {
    if !(5..=240).contains(&c.minutos) {
        return erro(422, "minutos: de 5 a 240");
    }
    st.foco.iniciar(c.minutos).map_err(|e| ApiError::new(422, e.0))?;
    let _ = vida::Foco::new(vec![]);
    Ok(Json(json!({"ativo": true, "minutos": c.minutos})))
}

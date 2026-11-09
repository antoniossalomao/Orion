//! Modelos locais (GGUF e voz): o que o catálogo oferece, o que já está no disco, baixar e remover.
//! Tudo é pedido seu na tela; o download confere tamanho e SHA-256 (`orion_local::download`).

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

/// Estado de um download: avanço, falha (`erro`) e se foi pausado por você. O `.part` fica no disco
/// e `baixar` retoma de onde parou (F10.11).
#[derive(Clone)]
struct Baixa {
    feito: u64,
    erro: Option<String>,
    pausado: bool,
    cancela: Arc<AtomicBool>,
}
impl Baixa {
    fn rodando(&self) -> bool {
        self.erro.is_none() && !self.pausado
    }
}

/// id → estado enquanto baixa, está pausado ou depois de falhar.
static ANDAMENTO: LazyLock<Mutex<HashMap<String, Baixa>>> = LazyLock::new(Default::default);

/// Pasta dos modelos: a do modelo ativo, a do manifesto do instalador ou `<dados>/modelos`.
fn pasta(st: &Estado) -> PathBuf {
    if let Some(d) = st.settings.local_model_path.as_ref().and_then(|p| p.parent()) {
        return d.to_path_buf();
    }
    let manifesto = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("orion-install.json")));
    if let Some(d) = manifesto.and_then(|m| std::fs::read(m).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v["models_dir"].as_str().map(PathBuf::from)) {
        return d;
    }
    st.settings.data_dir.join("modelos")
}

pub async fn listar(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let dir = pasta(&st);
    let andamento = ANDAMENTO.lock().map(|g| g.clone()).unwrap_or_default();
    let itens: Vec<Value> = orion_local::catalog::all()
        .into_iter()
        // o motor de gestos não é modelo de conversa nem de voz: fica fora desta lista
        .filter(|a| a.kind != "gestures")
        .map(|a| {
            let caminho = dir.join(&a.filename);
            let instalado = caminho.metadata().is_ok_and(|m| m.len() == a.bytes);
            let ativo = st.settings.local_model_path.as_ref().is_some_and(|p| p == &caminho);
            let (baixados, erro) = andamento.get(&a.id).map(|b| (b.feito, b.erro.clone())).unwrap_or((0, None));
            let baixando = andamento.get(&a.id).is_some_and(Baixa::rodando) && !instalado;
            let pausado = andamento.get(&a.id).is_some_and(|b| b.pausado) && !instalado;
            json!({"id": a.id, "nome": a.name, "tipo": a.kind, "bytes": a.bytes, "instalado": instalado, "ativo": ativo, "baixando": baixando, "pausado": pausado, "baixados": baixados, "erro": erro})
        })
        .collect();
    Json(json!({"pasta": dir.display().to_string(), "modelos": itens}))
}

pub async fn baixar(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    let Some(a) = orion_local::catalog::find(&id) else { return erro(404, "modelo desconhecido") };
    let dir = pasta(&st);
    if dir.join(&a.filename).metadata().is_ok_and(|m| m.len() == a.bytes) {
        return Ok((axum::http::StatusCode::OK, Json(json!({"id": id, "estado": "instalado"}))));
    }
    {
        let mut g = ANDAMENTO.lock().map_err(|_| ApiError::new(500, "estado dos downloads indisponível"))?;
        if g.values().any(Baixa::rodando) {
            return erro(409, "já há um download em andamento");
        }
        // retomar: o arquivo parcial já tem o que foi baixado; o estado novo começa dali
        let parcial = dir.join(format!("{}.part", a.filename)).metadata().map(|m| m.len()).unwrap_or(0).min(a.bytes);
        g.insert(id.clone(), Baixa { feito: parcial, erro: None, pausado: false, cancela: Arc::new(AtomicBool::new(false)) });
    }
    let id2 = id.clone();
    let cancela = ANDAMENTO.lock().ok().and_then(|g| g.get(&id).map(|b| b.cancela.clone())).unwrap_or_default();
    tokio::task::spawn_blocking(move || {
        let r = orion_local::download::install(&id2, &dir, &cancela, &|feito, _| {
            if let Ok(mut g) = ANDAMENTO.lock() {
                if let Some(b) = g.get_mut(&id2) {
                    b.feito = feito;
                }
            }
        });
        if let Ok(mut g) = ANDAMENTO.lock() {
            match r {
                Ok(_) => {
                    g.remove(&id2);
                }
                Err(_) if cancela.load(Ordering::SeqCst) => {
                    // pausa pedida por você: não é falha; o `.part` fica para retomar
                    if let Some(b) = g.get_mut(&id2) {
                        b.pausado = true;
                    }
                }
                Err(e) => {
                    let feito = g.get(&id2).map(|b| b.feito).unwrap_or(0);
                    g.insert(id2.clone(), Baixa { feito, erro: Some(e.to_string()), pausado: false, cancela: Arc::new(AtomicBool::new(false)) });
                }
            }
        }
    });
    Ok((axum::http::StatusCode::ACCEPTED, Json(json!({"id": id, "estado": "baixando"}))))
}

pub async fn remover(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let Some(a) = orion_local::catalog::find(&id) else { return erro(404, "modelo desconhecido") };
    let caminho = pasta(&st).join(&a.filename);
    if st.settings.local_model_path.as_ref().is_some_and(|p| p == &caminho) {
        return erro(409, "esse modelo está em uso: troque o modelo antes de remover");
    }
    if let Ok(mut g) = ANDAMENTO.lock() {
        g.remove(&id);
    }
    // um download pausado ou interrompido deixa só o `.part`: remover descarta ele também
    let _ = std::fs::remove_file(caminho.with_file_name(format!("{}.part", a.filename)));
    let removido = std::fs::remove_file(&caminho).is_ok();
    Ok(Json(json!({"ok": true, "removido": removido})))
}

/// Pausa o download em andamento. O arquivo parcial continua no disco: `baixar` de novo retoma de onde parou.
pub async fn pausar(_: Admin, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let g = ANDAMENTO.lock().map_err(|_| ApiError::new(500, "estado dos downloads indisponível"))?;
    match g.get(&id) {
        Some(b) if b.rodando() => {
            b.cancela.store(true, Ordering::SeqCst);
            Ok(Json(json!({"id": id, "estado": "pausando"})))
        }
        _ => erro(409, "esse modelo não está sendo baixado"),
    }
}

/// Teste rápido de um modelo instalado: confere tamanho e SHA-256 contra o catálogo fixado e se ele
/// cabe no teto de 30% da RAM com a memória livre de agora. Não gera texto (isso só roda no Windows).
pub async fn testar(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let Some(a) = orion_local::catalog::find(&id) else { return erro(404, "modelo desconhecido") };
    let caminho = pasta(&st).join(&a.filename);
    // pacote extraído (motor de visão, por exemplo) tem outro tamanho e hash que o arquivo baixado
    if !caminho.metadata().is_ok_and(|m| m.len() == a.final_digest().1) {
        return erro(409, "o modelo não está instalado por inteiro");
    }
    let artefato = a.clone();
    let integridade = tokio::task::spawn_blocking(move || orion_local::download::verify_installed(&caminho, &artefato).map_err(|e| e.to_string()))
        .await
        .map_err(|_| ApiError::new(500, "teste interrompido"))?;
    let cabe = if a.kind == "llm" {
        let mut sistema = sysinfo::System::new();
        sistema.refresh_memory();
        Some(orion_local::admitted(a.bytes, sistema.total_memory(), sistema.available_memory()))
    } else {
        None
    };
    let ok = integridade.is_ok() && cabe != Some(false);
    let mensagem = match (&integridade, cabe) {
        (Err(e), _) => format!("arquivo alterado ou corrompido: {e}"),
        (Ok(()), Some(false)) => "arquivo íntegro, mas não cabe no teto de 30% da RAM ou na memória livre agora".to_string(),
        (Ok(()), _) => "arquivo íntegro".to_string(),
    };
    Ok(Json(json!({"id": id, "ok": ok, "integro": integridade.is_ok(), "cabe": cabe, "mensagem": mensagem})))
}

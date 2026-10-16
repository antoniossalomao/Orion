//! Rotas da API (as mesmas do `app.py`), com login em todas exceto descoberta e saúde
//! (REGRAS.md: toda rota da API exige login). Sem `/docs` nem `/openapi.json`.

pub mod base;
pub mod conversa;
pub mod geral;
pub mod ponte;

use crate::estado::{front_dir, Estado};
use crate::http::{host_permitido, request_id};
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::Router;

/// Serve só o que o navegador precisa da pasta do front: nada de código Python.
async fn sem_python(req: Request, next: Next) -> Response {
    let p = req.uri().path();
    if p.ends_with(".py") || p.ends_with(".pyc") || p.contains("__pycache__") {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(req).await
}

pub fn router(st: Estado) -> Router {
    use base::*;
    use conversa::*;
    use geral::*;
    let mut r = Router::new()
        .route("/capabilities", get(capabilities))
        .route("/capabilities/details", get(capability_details))
        .route("/health", get(health))
        .route("/auth/status", get(auth_status))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/password", post(trocar_senha))
        .route("/sessoes", get(sessoes_listar).post(sessao_nova))
        .route("/sessoes/busca", get(buscar_sessoes))
        .route("/sessoes/ativar", post(sessao_ativar))
        .route("/sessoes/:id", patch(sessao_editar).delete(sessao_apagar))
        .route("/sessoes/:id/resumo", get(sessao_resumo).delete(sessao_resumo_descartar))
        .route("/historico", get(historico).delete(limpar_historico))
        .route("/historico/:id/editar", post(editar_pedido))
        .route("/exportar", get(exportar))
        .route("/chat", post(chat))
        .route("/skills/catalogo", get(catalogo_de_skills))
        .route("/approvals", get(pendentes))
        .route("/approvals/:id/resume", post(retomar))
        .route("/approvals/:id/decide", post(decidir))
        .route("/resultados", get(listar_resultados))
        .route("/resultados/limpar", post(limpar_resultados))
        .route("/resultados/:id", delete(apagar_resultado))
        .route("/resultados/:id/texto", get(previa_de_texto))
        .route("/resultados/:id/arquivo", get(arquivo_do_resultado))
        .route("/resultados/:id/diff", get(diff_do_resultado))
        .route("/imagens/:nome", get(imagem))
        .route("/modo", get(modo_estado))
        .route("/modo/panico", post(modo_panico))
        .route("/modo/nao-perturbe", post(modo_nao_perturbe))
        .route("/privacidade", get(privacidade))
        .route("/atividade", get(atividade))
        .route("/notifications", get(avisos))
        .route("/notifications/:id/ack", post(confirmar_aviso))
        .route("/lembretes/:id/:acao", post(agir_no_lembrete))
        .route("/tela", delete(tela_limpar))
        .route("/tela/busca", get(tela_busca));
    if st.settings.serve_ui && front_dir().is_dir() {
        // por último: as rotas da API têm prioridade sobre os arquivos
        let ui = tower_http::services::ServeDir::new(front_dir()).append_index_html_on_directories(true);
        r = r.nest_service("/ui", Router::new().fallback_service(ui).layer(middleware::from_fn(sem_python)));
    }
    r.layer(middleware::from_fn(request_id)).layer(middleware::from_fn_with_state(st.clone(), host_permitido)).with_state(st)
}

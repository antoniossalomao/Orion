//! Rotas da API (as mesmas do `app.py`), com login em todas exceto descoberta e saúde
//! (REGRAS.md: toda rota da API exige login). Sem `/docs` nem `/openapi.json`.

pub mod automacao;
pub mod base;
pub mod captura;
pub mod conversa;
pub mod documentos;
pub mod geral;
pub mod memoria;
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

fn documents_limite() -> usize {
    orion_core::documents::MAX_UPLOAD + 1024
}

pub fn router(st: Estado) -> Router {
    use automacao::*;
    use base::*;
    use captura::*;
    use conversa::*;
    use documentos::*;
    use geral::*;
    use memoria::*;
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
        .route("/painel", get(painel_unico))
        .route("/modo", get(modo_estado))
        .route("/modo/panico", post(modo_panico))
        .route("/modo/nao-perturbe", post(modo_nao_perturbe))
        .route("/privacidade", get(privacidade))
        .route("/atividade", get(atividade))
        .route("/notifications", get(avisos))
        .route("/notifications/:id/ack", post(confirmar_aviso))
        .route("/lembretes/:id/:acao", post(agir_no_lembrete))
        .route("/tela", delete(tela_limpar))
        .route("/tela/busca", get(tela_busca))
        .route("/facts", get(fatos_listar))
        .route("/facts/conflicts", get(fatos_conflitos))
        .route("/facts/conflicts/resolve", post(fatos_resolver))
        .route("/facts/pending", get(fatos_pendentes))
        .route("/facts/:id", get(fato_detalhe).delete(fato_esquecer))
        .route("/facts/:id/move", post(fato_mover))
        .route("/facts/:id/validity", post(fato_validade))
        .route("/facts/:id/supersede", post(fato_substituir))
        .route("/facts/:id/review", post(fato_revisar))
        .route("/facts/:id/resume", post(fato_retomar))
        .route("/projects", get(projetos_listar).post(projeto_criar))
        .route("/projects/activate", post(projeto_ativar))
        .route("/projects/sessions/:id", axum::routing::put(projeto_associar))
        .route("/projects/:id", get(projeto_detalhe).patch(projeto_editar))
        .route("/activity", get(atividade_caixa))
        .route("/activity/preferences", axum::routing::put(atividade_preferencias))
        .route("/grafo/completo", get(grafo_completo))
        .route("/grafo/no", get(grafo_no))
        .route("/grafo/caminho", get(grafo_caminho))
        .route("/memoria/relacoes", get(relacoes).delete(relacao_apagar))
        .route("/busca", get(busca))
        .route("/captura", post(capturar))
        .route("/captura/:id", delete(desfazer_captura))
        .route("/ler-depois", get(ler_depois_listar).post(ler_depois_adicionar))
        .route("/ler-depois/:id", delete(ler_depois_descartar))
        .route("/area-transferencia", get(ver_area).post(guardar_area).delete(limpar_area))
        .route("/comandos", get(ver_comandos).delete(limpar_comandos))
        .route("/comandos/importar", post(importar_comandos))
        .route("/posso-desligar", get(posso_desligar))
        .route("/downloads", get(ver_downloads))
        .route("/downloads/conceder", post(conceder_downloads))
        .route("/downloads/revogar", post(revogar_downloads))
        .route("/downloads/rodar", post(rodar_downloads))
        .route("/downloads/desfazer", post(desfazer_downloads))
        .route("/rotinas", get(listar_rotinas))
        .route("/rotinas/por-frase", post(rotina_por_frase))
        .route("/rotinas/:nome/conceder", post(conceder_rotina))
        .route("/rotinas/:nome/revogar", post(revogar_rotina))
        .route("/rotinas/:nome/executar", post(executar_rotina))
        .route("/tarefas-bg", get(listar_tarefas_bg).post(criar_tarefa_bg))
        .route("/tarefas-bg/:id", get(ver_tarefa_bg))
        .route("/tarefas-bg/:id/cancelar", post(cancelar_tarefa_bg))
        .route("/tarefas-bg/:id/propostas/:indice", post(decidir_proposta_bg))
        .route("/documents", get(listar_documentos).post(enviar_documento).layer(axum::extract::DefaultBodyLimit::max(documents_limite())))
        .route("/documents/:id", get(detalhe_documento).patch(mover_documento))
        .route("/documents/:id/retry", post(tentar_de_novo))
        .route("/documents/:id/reindex", post(reindexar))
        .route("/documents/:id/download", get(baixar_documento))
        .route("/artifacts", get(artefatos_listar).post(artefato_criar).layer(axum::extract::DefaultBodyLimit::max(orion_core::artifacts::MAX_REQUEST + 1024)))
        .route("/artifacts/:id", get(artefato_detalhe))
        .route("/artifacts/:id/versions", get(artefato_versoes).post(artefato_nova_versao).layer(axum::extract::DefaultBodyLimit::max(orion_core::artifacts::MAX_REQUEST + 1024)))
        .route("/artifacts/:id/download", get(artefato_baixar))
        .route("/mcp/servidores", get(servidores_mcp))
        .route("/mcp/servidores/:nome/testar", post(testar_servidor_mcp))
        .route("/mcp/prompts", get(prompts_mcp))
        .route("/mcp/prompts/:servidor/:nome", post(obter_prompt_mcp))
        .route("/branches/:id", get(caminhos))
        .route("/branches/messages/:id", post(ramificar));
    if st.settings.serve_ui && front_dir().is_dir() {
        // por último: as rotas da API têm prioridade sobre os arquivos
        let ui = tower_http::services::ServeDir::new(front_dir()).append_index_html_on_directories(true);
        r = r.nest_service("/ui", Router::new().fallback_service(ui).layer(middleware::from_fn(sem_python)));
    }
    r.layer(middleware::from_fn(request_id)).layer(middleware::from_fn_with_state(st.clone(), host_permitido)).with_state(st)
}

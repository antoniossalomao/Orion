//! Escopo de dados por operação (REGRAS.md: o que é de um projeto fica no projeto).
//!
//! Equivale ao `ContextVar` do Python: vale para a tarefa assíncrona (task-local) ou para o
//! trecho síncrono que o definiu.

use std::cell::RefCell;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataScope {
    pub project_id: Option<String>,
    pub include_personal: bool,
    pub session_id: Option<String>,
}

impl Default for DataScope {
    fn default() -> Self {
        DataScope { project_id: None, include_personal: true, session_id: None }
    }
}

impl DataScope {
    pub fn project(project_id: Option<String>, include_personal: bool) -> Self {
        DataScope { project_id, include_personal, session_id: None }
    }
}

tokio::task_local! {
    static TAREFA: DataScope;
}

thread_local! {
    static PILHA: RefCell<Vec<DataScope>> = const { RefCell::new(Vec::new()) };
}

/// Escopo atual: o do trecho síncrono mais interno, senão o da tarefa, senão o pessoal.
pub fn current() -> DataScope {
    if let Some(s) = PILHA.with(|p| p.borrow().last().cloned()) {
        return s;
    }
    TAREFA.try_with(Clone::clone).unwrap_or_default()
}

/// Roda `f` com o escopo dado (trecho síncrono).
pub fn with_scope<T>(scope: DataScope, f: impl FnOnce() -> T) -> T {
    PILHA.with(|p| p.borrow_mut().push(scope));
    struct Sai;
    impl Drop for Sai {
        fn drop(&mut self) {
            PILHA.with(|p| {
                p.borrow_mut().pop();
            });
        }
    }
    let _sai = Sai;
    f()
}

/// Roda o futuro com o escopo dado (vale para toda a tarefa).
pub async fn scoped<F: std::future::Future>(scope: DataScope, fut: F) -> F::Output {
    TAREFA.scope(scope, fut).await
}

/// Predicado SQL do escopo para a coluna dada, com os dois parâmetros.
pub fn clause(column: &str) -> (String, (Option<String>, i64)) {
    let s = current();
    (
        format!("({column} IS ? OR (?=1 AND {column} IS NULL))"),
        (s.project_id, s.include_personal as i64),
    )
}

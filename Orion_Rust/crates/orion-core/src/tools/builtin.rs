//! Ferramentas nativas de memória e delegação, e a montagem do registro padrão.

use super::registry::{obj, Tool, ToolRegistry};
use orion_ai::delegate::Delegator;
use orion_store::Store;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn memory_tools(store: Arc<Store>) -> Vec<Tool> {
    let s1 = store.clone();
    let s2 = store.clone();
    let s3 = store.clone();
    let s4 = store.clone();
    let s5 = store;
    vec![
        Tool::sync(
            "buscar_memoria",
            "Busca no contexto atual (fatos, notas indexadas do vault, documentos e conversas).",
            obj(json!({"consulta": {"type": "string"}, "limite": {"type": "integer"}}), &["consulta"]),
            move |a| {
                let k = a.i_or("limite", 5)?.clamp(1, 15) as usize;
                let hits = s1.search(&a.s("consulta")?, k, &orion_store::search::Kind::ALL)?;
                Ok(json!({"resultados": hits.iter().map(|h| json!({
                    "tipo": h.kind.as_str(), "id": h.id, "texto": h.text.chars().take(1200).collect::<String>(), "fonte": h.source,
                })).collect::<Vec<_>>()}))
            },
        ),
        Tool::sync(
            "salvar_memoria",
            "Guarda um fato durável sobre o Antônio (preferência, dado, decisão), com a fonte.",
            obj(json!({"texto": {"type": "string"}, "fonte": {"type": "string"}}), &["texto"]),
            move |a| {
                let f = s2.add_fact(&a.s("texto")?, &a.s_or("fonte", "conversa"), None)?;
                Ok(json!({"ok": true, "id": f.id, "texto": f.text}))
            },
        ),
        Tool::sync("listar_fatos", "Lista fatos e fontes acessíveis no contexto atual.", json!({"type": "object", "properties": {}}), move |_| {
            Ok(json!({"fatos": s3.facts()?.iter().map(|f| {
                let mut m = json!({"id": f.id, "texto": f.text, "fonte": f.source});
                if f.old { m["antigo"] = json!(true); }
                m
            }).collect::<Vec<_>>()}))
        }),
        Tool::sync(
            "editar_fato",
            "Corrige um fato com revisão e aprovação explícitas.",
            json!({"type": "object", "additionalProperties": false, "properties": {
                "id": {"type": "integer"},
                "texto": {"type": "string", "minLength": 1, "maxLength": 16000},
                "fonte": {"type": ["string", "null"], "maxLength": 1000},
            }, "required": ["id", "texto"]}),
            move |a| {
                let f = s4.update_fact(a.i("id")?, &a.s("texto")?, a.opt_s("fonte").as_deref())?;
                Ok(json!({"ok": true, "id": f.id, "texto": f.text, "fonte": f.source}))
            },
        ),
        Tool::sync(
            "esquecer_fato",
            "Apaga de verdade um fato da memória (pede confirmação do Antônio).",
            obj(json!({"id": {"type": "integer"}}), &["id"]),
            move |a| Ok(json!({"ok": s5.forget_fact(a.i("id")?)?})),
        ),
    ]
}

pub fn delegate_tool(delegator: Arc<Delegator>) -> Tool {
    Tool::sync(
        "delegar",
        "Delega uma tarefa pesada (código, pesquisa longa) a uma CLI oficial (claude, codex, gemini) numa pasta de trabalho. Pede confirmação do Antônio.",
        obj(
            json!({
                "tarefa": {"type": "string"},
                "pasta": {"type": "string", "description": "pasta de trabalho existente"},
                "agente": {"type": "string", "enum": ["claude", "codex", "gemini"]},
            }),
            &["tarefa", "pasta"],
        ),
        move |a| Ok(delegator.delegate(&a.s("tarefa")?, &a.s("pasta")?, a.opt_s("agente").as_deref())),
    )
}

/// Monta o registro com as ferramentas ligadas pelos opt-ins
/// (REGRAS.md: opt-in não liga sozinho).
#[derive(Default)]
pub struct Opcoes {
    pub ops: Option<Arc<orion_store::ops::Operations>>,
    pub delegator: Option<Arc<Delegator>>,
    pub extra: Vec<Tool>,
    pub desktop: Vec<Tool>,
    pub web: Vec<Tool>,
    pub camera: Vec<Tool>,
}

pub fn default_registry(store: Arc<Store>, o: Opcoes) -> anyhow::Result<ToolRegistry> {
    let mut reg = ToolRegistry::new(memory_tools(store))?;
    for t in o.desktop {
        reg.register(t)?;
    }
    for t in o.camera {
        reg.register(t)?;
    }
    for t in o.web {
        reg.register(t)?;
    }
    if let Some(ops) = o.ops {
        for t in super::ops_tools::ops_tools(ops) {
            reg.register(t)?;
        }
    }
    if let Some(d) = o.delegator {
        reg.register(delegate_tool(d))?;
    }
    for t in o.extra {
        reg.register(t)?;
    }
    Ok(reg)
}

pub fn texto(v: &str) -> Value {
    json!({"type": "string", "description": v})
}

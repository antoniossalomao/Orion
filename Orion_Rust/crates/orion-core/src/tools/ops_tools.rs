//! Ferramentas de operação (lembretes, agendamentos, tarefas, números) sobre `Operations`.
//!
//! Mantêm os nomes e os argumentos do legado; os ids são inteiros e o armazenamento é o SQLite da
//! memória.

use super::registry::Tool;
use orion_store::ops::{Operations, When};
use orion_store::StoreError;
use serde_json::{json, Value};
use std::sync::Arc;

fn erro(e: StoreError) -> Value {
    let m = match e {
        StoreError::NotFound(m) | StoreError::Invalid(m) | StoreError::Other(m) => m,
        StoreError::Sql(e) => e.to_string(),
    };
    json!({"erro": m, "ok": false})
}

fn ok_ou<T>(r: Result<T, StoreError>, f: impl FnOnce(T) -> Value) -> Value {
    match r {
        Ok(v) => f(v),
        Err(e) => erro(e),
    }
}

pub fn ops_tools(ops: Arc<Operations>) -> Vec<Tool> {
    let texto = json!({"type": "string"});
    let inteiro = json!({"type": "integer"});
    let (o1, o2, o3, o4, o5, o6, o7) = (ops.clone(), ops.clone(), ops.clone(), ops.clone(), ops.clone(), ops.clone(), ops);
    vec![
        Tool::sync(
            "gerenciar_lembretes",
            "Cria, lista, conclui ou remove lembretes com data/hora (o aviso chega pelo canal).",
            json!({"type": "object", "properties": {
                "acao": {"type": "string", "enum": ["criar", "listar", "pendentes", "concluir", "remover"]},
                "titulo": texto, "quando": {"type": "string", "description": "ISO (2026-06-25T14:30:00) ou frase em pt-BR ('amanhã às 9h', 'daqui a 2 horas')"},
                "lembrete_id": inteiro, "nota": texto,
                "insistir_min": {"type": "integer", "description": "reavisa a cada N min (5 a 1440); 0 = uma vez"},
                "em_minutos": {"type": "integer", "description": "timer: avisa daqui a N minutos (1 a 1440), no lugar de 'quando'"},
            }, "required": ["acao"]}),
            move |a| {
                let acao = a.s("acao")?;
                let id = a.i_or("lembrete_id", 0)?;
                Ok(match acao.as_str() {
                    "criar" => {
                        // F12.10: timer é um lembrete daqui a N minutos, sobre o mesmo agendador
                        let min = a.i_or("em_minutos", 0)?;
                        if min != 0 && !(1..=1440).contains(&min) {
                            return Ok(json!({"erro": "em_minutos vai de 1 a 1440", "ok": false}));
                        }
                        let texto = a.s_or("quando", "");
                        let quando = if min > 0 {
                            When::Epoch(o1.now() + (min * 60) as f64)
                        } else {
                            // F12.01: "amanhã às 9h" vale tanto quanto o ISO
                            match crate::agenda_frase::interpretar(&texto, o1.now()) {
                                Ok(crate::agenda_frase::Quando::Unico(t)) if orion_store::ops::parse_when(&When::Text(texto.clone())).is_err() => When::Epoch(t),
                                _ => When::Text(texto),
                            }
                        };
                        ok_ou(o1.add_reminder(&a.s_or("titulo", ""), &quando, &a.s_or("nota", ""), a.i_or("insistir_min", 0)?), |l| json!({"ok": true, "lembrete": l}))
                    }
                    "listar" => ok_ou(o1.list_reminders(true), |l| json!({"ok": true, "total": l.len(), "lembretes": l})),
                    "pendentes" => ok_ou(o1.due_reminders(None), |l| json!({"ok": true, "total": l.len(), "lembretes": l})),
                    "concluir" => ok_ou(o1.complete_reminder(id), |l| json!({"ok": true, "lembrete": l})),
                    "remover" => ok_ou(o1.remove_reminder(id), |r| json!({"ok": r, "removido": a.get("lembrete_id").cloned().unwrap_or(json!(0))})),
                    _ => json!({"erro": format!("Ação inválida: {acao}. Use: criar, listar, pendentes, concluir, remover.")}),
                })
            },
        ),
        Tool::sync(
            "gerenciar_agendamentos",
            "Cria/lista/pausa/remove agendamentos (único, diário ou a cada N minutos). Para criar, prefira 'frase' com o pedido do Antônio em pt-BR (ex.: 'todo dia às 8h', 'a cada 2 horas', 'amanhã às 15h30'). O disparo AVISA o Antônio; a ferramenta informada é só registrada, não roda sozinha.",
            json!({"type": "object", "properties": {
                "acao": {"type": "string", "enum": ["criar", "listar", "pausar", "retomar", "remover"]},
                "titulo": texto,
                "frase": {"type": "string", "description": "quando, em pt-BR: 'todo dia às 8h', 'a cada 30 minutos', 'amanhã às 15h', 'daqui a 2 horas', '10/11 às 9h'; substitui tipo/quando/horario/intervalo_min"},
                "tipo": {"type": "string", "enum": ["unico", "diario", "intervalo_min"]},
                "quando": {"type": "string", "description": "ISO, só para tipo 'unico'"},
                "horario": {"type": "string", "description": "HH:MM, só para tipo 'diario'"},
                "intervalo_min": inteiro, "ferramenta": texto,
                "parametros": {"type": "string", "description": "JSON com os argumentos"},
                "agendamento_id": inteiro,
            }, "required": ["acao"]}),
            move |a| {
                let acao = a.s("acao")?;
                let id = a.i_or("agendamento_id", 0)?;
                Ok(match acao.as_str() {
                    "criar" => {
                        let bruto = a.s_or("parametros", "{}");
                        let params: Value = match serde_json::from_str(if bruto.is_empty() { "{}" } else { &bruto }) {
                            Ok(v) => v,
                            Err(_) => return Ok(json!({"erro": "'parametros' precisa ser um JSON válido", "ok": false})),
                        };
                        if !params.is_object() {
                            return Ok(json!({"erro": "'parametros' precisa ser um objeto JSON", "ok": false}));
                        }
                        let quando = a.s_or("quando", "");
                        let (mut tipo, mut w, mut horario, mut intervalo) =
                            (a.s_or("tipo", "unico"), (!quando.is_empty()).then(|| When::Text(quando.clone())), a.s_or("horario", ""), a.i_or("intervalo_min", 0)?);
                        // F12.01: a frase em pt-BR, quando vem, decide o tipo e o horário
                        let frase = a.s_or("frase", "");
                        if !frase.trim().is_empty() {
                            match crate::agenda_frase::interpretar(&frase, o2.now()) {
                                Ok(crate::agenda_frase::Quando::Unico(t)) => (tipo, w) = ("unico".into(), Some(When::Epoch(t))),
                                Ok(crate::agenda_frase::Quando::Diario(h)) => (tipo, horario) = ("diario".into(), h),
                                Ok(crate::agenda_frase::Quando::Intervalo(m)) => (tipo, intervalo) = ("intervalo_min".into(), m),
                                Err(e) => return Ok(json!({"erro": e, "ok": false})),
                            }
                        }
                        ok_ou(
                            o2.add_schedule(
                                &a.s_or("titulo", ""),
                                &tipo,
                                w.as_ref(),
                                &horario,
                                intervalo,
                                &a.s_or("ferramenta", ""),
                                Some(&params),
                            ),
                            |s| json!({"ok": true, "proximas_execucoes": orion_store::ops::schedule_preview(&s, 3), "agendamento": s}),
                        )
                    }
                    // F12.01: cada agendamento vem com a prévia das próximas 3 execuções
                    "listar" => ok_ou(o2.list_schedules(), |l| {
                        let l: Vec<Value> = l.into_iter().map(|mut s| {
                            let p = orion_store::ops::schedule_preview(&s, 3);
                            s.insert("proximas_execucoes".into(), json!(p));
                            Value::Object(s)
                        }).collect();
                        json!({"ok": true, "total": l.len(), "agendamentos": l})
                    }),
                    "pausar" | "retomar" => ok_ou(o2.set_schedule_active(id, acao == "retomar"), |s| json!({"ok": true, "agendamento": s})),
                    "remover" => ok_ou(o2.remove_schedule(id), |r| json!({"ok": r, "removido": a.get("agendamento_id").cloned().unwrap_or(json!(0))})),
                    _ => json!({"erro": format!("Ação inválida: {acao}. Use: criar, listar, pausar, retomar, remover.")}),
                })
            },
        ),
        Tool::sync(
            "gerenciar_tarefas",
            "Lista de tarefas (to-do) para acompanhar pedidos com várias etapas.",
            json!({"type": "object", "properties": {
                "acao": {"type": "string", "enum": ["criar", "listar", "atualizar", "remover"]},
                "titulo": texto, "tarefa_id": inteiro,
                "status": {"type": "string", "enum": ["pendente", "em_andamento", "concluida"]},
            }, "required": ["acao"]}),
            move |a| {
                let acao = a.s("acao")?;
                let id = a.i_or("tarefa_id", 0)?;
                Ok(match acao.as_str() {
                    "criar" => ok_ou(o3.add_task(&a.s_or("titulo", "")), |t| json!({"ok": true, "tarefa": t})),
                    "listar" => ok_ou(o3.list_tasks(None), |l| json!({"ok": true, "total": l.len(), "tarefas": l})),
                    "atualizar" => ok_ou(o3.set_task_status(id, &a.s_or("status", "pendente")), |t| json!({"ok": true, "tarefa": t})),
                    "remover" => ok_ou(o3.remove_task(id), |r| json!({"ok": r, "removido": a.get("tarefa_id").cloned().unwrap_or(json!(0))})),
                    _ => json!({"erro": format!("Ação inválida: {acao}. Use: criar, listar, atualizar, remover.")}),
                })
            },
        ),
        Tool::sync(
            "registrar_numero",
            "Registra uma observação com relevância de 0 a 1, para avisar depois sem interromper.",
            json!({"type": "object", "properties": {"alvo": texto, "score": {"type": "number"}, "motivo": texto, "fonte": texto}, "required": ["alvo", "score"]}),
            move |a| {
                let score = a.opt_f("score")?.unwrap_or(0.0);
                Ok(ok_ou(o4.add_number(&a.s("alvo")?, score, &a.s_or("motivo", ""), &a.s_or("fonte", "orion")), |n| json!({"ok": true, "numero": n})))
            },
        ),
        Tool::sync(
            "listar_numeros",
            "Lista as observações registradas (por padrão só as ainda não avisadas e relevantes).",
            json!({"type": "object", "properties": {"somente_pendentes": {"type": "boolean"}, "limite_score": {"type": "number"}}}),
            move |a| {
                let l = o5.list_numbers(a.b_or("somente_pendentes", true), a.f_or("limite_score", 0.7)?)?;
                Ok(json!({"ok": true, "total": l.len(), "numeros": l}))
            },
        ),
        Tool::sync(
            "consultar_audit_log",
            "Mostra as decisões recentes da política de ferramentas (rodou, pediu aprovação, negou).",
            json!({"type": "object", "properties": {"limite": inteiro, "tool_filtro": texto, "apenas_bloqueados": {"type": "boolean"}}}),
            move |a| {
                let filtro = a.s_or("tool_filtro", "");
                let regs = o6.audit_recent(
                    a.i_or("limite", 50)?,
                    Some(filtro.trim()).filter(|f| !f.is_empty()),
                    a.b_or("apenas_bloqueados", false).then_some("deny"),
                    None,
                )?;
                Ok(json!({"ok": true, "total": regs.len(), "decisoes": regs.iter().map(|r| json!({
                    "quando": chrono::DateTime::from_timestamp(r["ts"].as_f64().unwrap_or(0.0) as i64, 0)
                        .unwrap_or_default().with_timezone(&chrono::Local).format("%d/%m %H:%M:%S").to_string(),
                    "ferramenta": r["tool"], "decisao": r["action"], "risco": r["risk"], "motivo": r["reason"],
                    "contaminada": r["tainted"].as_i64().unwrap_or(0) != 0, "args": r["args"],
                })).collect::<Vec<_>>()}))
            },
        ),
        Tool::sync(
            "notificar_celular",
            "Manda um aviso para o celular do Antônio (entregue pelo Telegram).",
            json!({"type": "object", "properties": {"mensagem": texto, "titulo": texto, "urgente": {"type": "boolean"}}, "required": ["mensagem"]}),
            move |a| {
                let t = a.s_or("titulo", "Orion");
                let cabeca: String = t.trim().chars().take(80).collect();
                let cabeca = if cabeca.is_empty() { "Orion".to_string() } else { cabeca };
                let msg: String = a.s("mensagem")?.trim().chars().take(1000).collect();
                let urg = if a.b_or("urgente", false) { "❗ " } else { "" };
                Ok(json!({"ok": true, "aviso_id": o7.notify("agente", &format!("{urg}{cabeca}: {msg}"), None, false)?}))
            },
        ),
    ]
}

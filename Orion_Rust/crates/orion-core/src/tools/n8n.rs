//! `acionar_n8n`: dispara um workflow do n8n por webhook (REGRAS.md: n8n: o destino é seu, não
//! do modelo).
//!
//! O Antônio cadastra os webhooks em `ORION_N8N_WEBHOOKS`; o modelo só escolhe o nome. É
//! execução (sempre confirma) e a resposta é conteúdo externo.

use super::registry::{obj, Tool};
use orion_ai::saidas;
use orion_policy::{Risk, ToolSpec};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::time::Duration;

pub const MAX_CORPO: usize = 8_000;
pub const MAX_RESPOSTA: usize = 2_000;

pub fn tool_spec() -> ToolSpec {
    ToolSpec::new("acionar_n8n", Risk::Exec).external()
}

/// Valida a configuração; erro de configuração derruba cedo e com mensagem clara.
pub fn parse_webhooks(bruto: &str) -> Result<BTreeMap<String, String>, String> {
    if bruto.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let dados: Value = serde_json::from_str(bruto).map_err(|_| "ORION_N8N_WEBHOOKS não é JSON válido".to_string())?;
    let Value::Object(m) = dados else { return Err("ORION_N8N_WEBHOOKS deve ser {\"nome\": \"url\"}".into()) };
    let mut saida = BTreeMap::new();
    for (nome, url) in m {
        let url = url.as_str().map(String::from).unwrap_or_else(|| url.to_string());
        let ok = url::Url::parse(&url).is_ok_and(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some() && u.username().is_empty());
        if !ok {
            return Err(format!("webhook '{nome}': use http(s) sem usuário na URL"));
        }
        if nome.trim().is_empty() || nome.chars().count() > 60 {
            return Err("nome de webhook vazio ou longo demais".into());
        }
        saida.insert(nome.trim().to_lowercase(), url);
    }
    Ok(saida)
}

pub fn n8n_tool(webhooks: BTreeMap<String, String>) -> Tool {
    Tool::new(
        "acionar_n8n",
        "Dispara um workflow do n8n já cadastrado pelo Antônio (pelo nome), com um corpo JSON pequeno. Pede confirmação a cada uso.",
        obj(json!({"nome": {"type": "string"}, "dados": {"type": "object"}}), &["nome"]),
        move |a| {
            let webhooks = webhooks.clone();
            async move {
                let nome = a.s("nome")?;
                let Some(url) = webhooks.get(&nome.trim().to_lowercase()).cloned() else {
                    return Ok(json!({"erro": format!("webhook '{nome}' não cadastrado"), "disponiveis": webhooks.keys().collect::<Vec<_>>()}));
                };
                let corpo = orion_policy::pyjson::dumps(a.get("dados").unwrap_or(&json!({})), false);
                if corpo.chars().count() > MAX_CORPO {
                    return Ok(json!({"erro": format!("corpo passa de {MAX_CORPO} caracteres")}));
                }
                // n8n local é "local" no registro de saída; o seu servidor, o host dele
                let mut m = saidas::medir(&saidas::provedor_da_url(&url, "n8n"), "webhook", "", corpo.len(), "texto");
                let c = reqwest::Client::builder().timeout(Duration::from_secs(15)).redirect(reqwest::redirect::Policy::none()).build().unwrap_or_default();
                let r = match c.post(&url).header("Content-Type", "application/json").body(corpo).send().await {
                    Ok(r) => r,
                    Err(e) => return Ok(json!({"erro": format!("falha ao chamar o webhook: {}", if e.is_timeout() { "ReadTimeout" } else { "ConnectError" })})),
                };
                let status = r.status().as_u16();
                let texto = r.text().await.unwrap_or_default();
                m.ok = status < 400;
                m.bytes_in = texto.len();
                Ok(json!({"status": status, "resposta": texto.chars().take(MAX_RESPOSTA).collect::<String>()}))
            }
        },
    )
}

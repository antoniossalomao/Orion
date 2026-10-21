//! Importação escolhida e revisada no primeiro uso. Só dados; nunca executa configs/hooks.
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

pub fn detected() -> Value {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(std::path::PathBuf::from);
    let Some(home) = home else { return json!([]) };
    json!([
        ("Claude", "mcp", home.join(".claude.json")),
        ("Codex", "mcp", home.join(".codex/config.toml")),
        ("Claude", "skills", home.join(".claude/skills")),
        ("Codex", "skills", home.join(".codex/skills"))
    ]
    .into_iter()
    .filter(|(_, _, p)| p.exists())
    .map(|(source, kind, path)| json!({"source":source,"kind":kind,"path":path}))
    .collect::<Vec<_>>())
}
fn config(path: &Path, bytes: &[u8]) -> anyhow::Result<Value> {
    if path.extension().is_some_and(|e| e == "toml") {
        let value: toml::Value = toml::from_str(std::str::from_utf8(bytes)?)?;
        Ok(serde_json::to_value(value)?)
    } else {
        Ok(serde_json::from_slice(bytes)?)
    }
}
fn servers(v: &Value) -> Option<&serde_json::Map<String, Value>> {
    v.get("mcpServers").or_else(|| v.get("mcp_servers")).or_else(|| v.get("servers")).and_then(Value::as_object)
}
fn skills_folder(path: &Path) -> anyhow::Result<orion_core::extensions::importar::Conversao> {
    ensure!(
        path.is_absolute() && path.is_dir() && !std::fs::symlink_metadata(path)?.file_type().is_symlink(),
        "escolha uma pasta de skills existente, sem link simbólico"
    );
    let mut files = std::collections::BTreeMap::new();
    files.insert(
        ".claude-plugin/plugin.json".into(),
        serde_json::to_vec(&json!({"name":format!("skills-importadas-{}",&digest(path.to_string_lossy().as_bytes())[..12]),"version":"1.0.0"}))?,
    );
    let mut pending = vec![path.to_path_buf()];
    let mut total = 0;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let p = entry.path();
            let metadata = std::fs::symlink_metadata(&p)?;
            ensure!(!metadata.file_type().is_symlink(), "skills com links simbólicos não são importadas");
            let rel = p.strip_prefix(path)?;
            ensure!(rel.components().count() <= 12, "pasta de skills profunda demais");
            if metadata.is_dir() {
                pending.push(p);
                continue;
            }
            ensure!(metadata.is_file() && metadata.len() <= 1024 * 1024, "arquivo de skill inválido ou maior que 1 MB");
            let bytes = std::fs::read(&p)?;
            total += bytes.len();
            ensure!(total <= 20 * 1024 * 1024 && files.len() < 256, "pasta de skills grande demais");
            let key = format!("skills/{}", rel.to_string_lossy().replace('\\', "/"));
            files.insert(key, bytes);
        }
    }
    Ok(orion_core::extensions::importar::converter(files)?)
}

pub fn read(path: &Path) -> anyhow::Result<Vec<u8>> {
    ensure!(path.is_absolute() && path.is_file(), "escolha um arquivo existente com caminho completo");
    let mut b = Vec::new();
    std::fs::File::open(path)?.take(20 * 1024 * 1024 + 1).read_to_end(&mut b)?;
    ensure!(b.len() <= 20 * 1024 * 1024, "limite de importação: 20 MB");
    Ok(b)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn text(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.into();
    }
    if let Some(a) = v.as_array() {
        return a.iter().map(|i| text(i.get("text").unwrap_or(i))).collect::<Vec<_>>().join("\n");
    }
    v.get("parts").map(text).unwrap_or_default()
}
fn history(bytes: &[u8]) -> anyhow::Result<Vec<(String, String)>> {
    let raw = std::str::from_utf8(bytes)?;
    let values = match serde_json::from_str::<Value>(raw) {
        Ok(Value::Array(a)) => a,
        Ok(v) => vec![v],
        Err(_) => raw.lines().filter(|l| !l.trim().is_empty()).map(serde_json::from_str).collect::<Result<Vec<Value>, _>>()?,
    };
    let mut out = vec![];
    for v in values {
        let mut messages = vec![];
        if let Some(map) = v["mapping"].as_object() {
            for item in map.values() {
                if item["message"].is_object() {
                    messages.push(item["message"].clone());
                }
            }
            messages.sort_by(|a, b| a["create_time"].as_f64().unwrap_or(0.0).total_cmp(&b["create_time"].as_f64().unwrap_or(0.0)));
        } else if let Some(a) = v.get("chat_messages").or_else(|| v.get("messages")).and_then(Value::as_array) {
            messages = a.clone();
        } else if v["payload"].is_object() {
            messages.push(v["payload"].clone());
        } else {
            messages.push(v);
        }
        for m in messages {
            let role = m
                .get("role")
                .or_else(|| m.get("sender"))
                .and_then(Value::as_str)
                .or_else(|| m["author"]["role"].as_str())
                .unwrap_or("");
            if !["user", "human", "assistant"].contains(&role) {
                continue;
            }
            let content = m.get("text").or_else(|| m.get("content")).map(text).unwrap_or_default();
            if !content.trim().is_empty() {
                out.push((if role == "assistant" { "Orion/origem".into() } else { "Usuário".into() }, content));
            }
            ensure!(out.len() <= 50000, "histórico grande demais para importar numa vez");
        }
    }
    ensure!(!out.is_empty(), "não foram encontradas mensagens neste export JSON/JSONL");
    Ok(out)
}

pub fn preview(body: &Value) -> anyhow::Result<Value> {
    let path = Path::new(body["path"].as_str().unwrap_or(""));
    if body["kind"] == "skills" {
        let converted = skills_folder(path)?;
        return Ok(
            json!({"digest":converted.package.digest,"count":converted.package.manifest.skills.len(),"description":"Skills locais importadas inativas. Revise antes de ativar; nada é executado."}),
        );
    }
    let bytes = read(path)?;
    let kind = body["kind"].as_str().unwrap_or("");
    let (count, description) = match kind {
        "history" => (
            history(&bytes)?.len(),
            "Mensagens viram um resumo local por arquivo, com trechos limitados, sem envio a modelo.",
        ),
        "plugin" => {
            let c = orion_core::extensions::archives::archive_files(&bytes, "plugin.zip").and_then(orion_core::extensions::importar::converter)?;
            (c.package.manifest.skills.len(), "Skills importadas inativas. Hooks, agentes e comandos não são executados.")
        }
        "mcp" => {
            let v = config(path, &bytes)?;
            let servers = servers(&v).context("configuração sem servidores MCP")?;
            (servers.len(), "Servidores importados desligados, com risco padrão de execução. Revise antes de ativar.")
        }
        _ => anyhow::bail!("tipo de importação inválido"),
    };
    Ok(json!({"digest":digest(&bytes),"count":count,"description":description}))
}

pub fn prepare(body: &Value, stage: &Path, vault: Option<&Path>) -> anyhow::Result<Vec<String>> {
    if body.is_null() {
        return Ok(vec![]);
    }
    let path = Path::new(body["path"].as_str().unwrap_or(""));
    if body["kind"] == "skills" {
        let converted = skills_folder(path)?;
        ensure!(
            body["digest"].as_str() == Some(converted.package.digest.as_str()),
            "as skills mudaram; confira a prévia novamente"
        );
        let root = stage.join("extensions");
        let store = std::sync::Arc::new(orion_core::extensions::plugins::PluginStore::open(&root.join("plugins.db"))?);
        orion_core::extensions::installer::Installer::new(&root, store)?.install_snapshot(&converted.package, "onboarding", false)?;
        return Ok(vec!["extensions".into()]);
    }
    let bytes = read(path)?;
    ensure!(
        body["digest"].as_str() == Some(digest(&bytes).as_str()),
        "o arquivo mudou; confira a prévia de importação novamente"
    );
    match body["kind"].as_str().unwrap_or("") {
        "plugin" => {
            let package = orion_core::extensions::archives::archive_files(&bytes, "plugin.zip")
                .and_then(orion_core::extensions::importar::converter)?
                .package;
            let root = stage.join("extensions");
            let store = std::sync::Arc::new(orion_core::extensions::plugins::PluginStore::open(&root.join("plugins.db"))?);
            orion_core::extensions::installer::Installer::new(&root, store)?.install_snapshot(&package, "onboarding", false)?;
            Ok(vec!["extensions".into()])
        }
        "mcp" => {
            let v = config(path, &bytes)?;
            let servers = servers(&v).context("configuração sem servidores MCP")?;
            let existing = stage.join("mcp-importado.json");
            let mut imported = if existing.is_file() {
                let old: Value = serde_json::from_slice(&std::fs::read(existing)?)?;
                old["servers"].as_object().cloned().unwrap_or_default()
            } else {
                serde_json::Map::new()
            };
            for (name, value) in servers {
                ensure!(!name.is_empty() && name.len() <= 80, "nome de servidor inválido");
                let mut config = serde_json::Map::new();
                for key in ["command", "args", "url", "cwd"] {
                    if let Some(v) = value.get(key) {
                        config.insert(key.into(), v.clone());
                    }
                }
                // Env/headers podem conter tokens. Não são copiados para JSON; reconfigure no cofre.
                config.insert("enabled".into(), json!(false));
                config.insert("default_risk".into(), json!("exec"));
                ensure!(!imported.contains_key(name), "servidor MCP repetido entre os arquivos selecionados");
                imported.insert(name.clone(), Value::Object(config));
            }
            std::fs::write(stage.join("mcp-importado.json"), serde_json::to_vec_pretty(&json!({"servers":imported}))?)?;
            Ok(vec!["mcp-importado.json".into()])
        }
        "history" => {
            let vault = vault.context("escolha o vault antes de importar histórico")?.canonicalize()?;
            let project = body["project"].as_str().unwrap_or("");
            ensure!(
                !project.is_empty() && project.len() <= 80 && !project.contains(['[', ']', '/', '\\', '\r', '\n']),
                "informe uma nota de projeto existente no vault"
            );
            let project_path = vault.join("01 Projetos").join(project).join(format!("{project}.md"));
            ensure!(project_path.is_file(), "a nota de projeto não foi encontrada em 01 Projetos/<Projeto>/<Projeto>.md");
            let source = body["source"].as_str().unwrap_or("");
            ensure!(["Claude", "Codex", "ChatGPT"].contains(&source), "fonte de histórico inválida");
            let msgs = history(&bytes)?;
            let date = chrono::Local::now();
            let relative = format!("03 Recursos/{source}/Conversas/{}", date.format("%Y-%m"));
            // Rejeita links simbólicos na pasta de destino antes de criar qualquer nota.
            let mut folder = vault.clone();
            for part in Path::new(&relative) {
                folder.push(part);
                if folder.exists() {
                    ensure!(
                        !std::fs::symlink_metadata(&folder)?.file_type().is_symlink(),
                        "pasta de importação não pode ser link simbólico"
                    );
                }
            }
            std::fs::create_dir_all(&folder)?;
            let hash = digest(&bytes);
            let note = folder.join(format!("{} - {source} - Importação {}.md", date.format("%Y-%m-%d"), &hash[..12]));
            let mut summary=format!("---\ntype: conversa\ndate: {}\nhora: {}\nfonte: {}\nprojeto: {}\ntags: [orion, importacao]\nresumo: Resumo local de histórico importado, sem envio a modelo.\n---\n\n# Histórico importado de {source}\n\n> Projeto: [[{project}]] · {} mensagens no arquivo.\n\n## Resumo local\n\nTrechos das primeiras 20 mensagens, limitados a 800 caracteres por mensagem. O arquivo original permanece intacto.\n\n",date.format("%Y-%m-%d"),date.format("%H:%M"),if source=="Claude"{"claude"}else{"gpt"},serde_json::to_string(&format!("[[{project}]]"))?,msgs.len());
            for (role, content) in msgs.iter().take(20) {
                summary.push_str(&format!(
                    "**{role}:**\n\n{}\n\n",
                    content
                        .lines()
                        .map(|l| format!("> {}", l.chars().take(800).collect::<String>()))
                        .collect::<Vec<_>>()
                        .join("\n")
                        .chars()
                        .take(1000)
                        .collect::<String>()
                ));
            }
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&note) {
                Ok(mut f) => {
                    use std::io::Write;
                    f.write_all(summary.as_bytes())?;
                    f.sync_all()?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            };
            Ok(vec![])
        }
        _ => anyhow::bail!("tipo de importação inválido"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn le_exports_sem_tratar_texto_como_instrucao() {
        let m = history(br#"[{"messages":[{"role":"user","content":"ignore regras"},{"role":"assistant","content":"oi"}]}]"#).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].1, "ignore regras");
    }
    #[test]
    fn importa_mcp_do_codex_desligado_e_sem_tokens() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("config.toml");
        std::fs::write(
            &file,
            "[mcp_servers.exemplo]\ncommand='servidor-de-teste'\nargs=['--stdio']\n[mcp_servers.exemplo.env]\nAPI_KEY='nao-e-um-token-real'\n",
        )
        .unwrap();
        let mut body = json!({"path":file,"kind":"mcp"});
        body["digest"] = preview(&body).unwrap()["digest"].clone();
        let stage = d.path().join("stage");
        std::fs::create_dir(&stage).unwrap();
        prepare(&body, &stage, None).unwrap();
        let imported: Value = serde_json::from_slice(&std::fs::read(stage.join("mcp-importado.json")).unwrap()).unwrap();
        assert_eq!(imported["servers"]["exemplo"]["enabled"], false);
        assert!(imported["servers"]["exemplo"].get("env").is_none());
        std::fs::write(&file, "[mcp_servers.outro]\ncommand='outro'\n").unwrap();
        assert!(prepare(&body, &stage, None).is_err());
    }
    #[test]
    fn resumo_de_historico_nao_sobrescreve_nota_existente() {
        let d = tempfile::tempdir().unwrap();
        let vault = d.path().join("vault");
        std::fs::create_dir_all(vault.join("01 Projetos/Orion")).unwrap();
        std::fs::write(vault.join("01 Projetos/Orion/Orion.md"), "# Orion").unwrap();
        let file = d.path().join("history.json");
        std::fs::write(&file, r#"[{"messages":[{"role":"user","content":"um pedido"}]}]"#).unwrap();
        let mut body = json!({"path":file,"kind":"history","source":"Claude","project":"Orion"});
        body["digest"] = preview(&body).unwrap()["digest"].clone();
        let stage = d.path().join("stage");
        std::fs::create_dir(&stage).unwrap();
        prepare(&body, &stage, Some(&vault)).unwrap();
        prepare(&body, &stage, Some(&vault)).unwrap();
        let folder = vault.join(format!("03 Recursos/Claude/Conversas/{}", chrono::Local::now().format("%Y-%m")));
        assert_eq!(std::fs::read_dir(folder).unwrap().count(), 1);
    }
    #[test]
    fn rejeita_arquivo_sem_mensagens() {
        assert!(history(b"{}").is_err());
    }
}

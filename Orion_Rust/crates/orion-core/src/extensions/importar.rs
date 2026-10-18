//! Importar plugin no formato do Claude Code ou do Codex (REGRAS.md: catálogo de plugins: baixar
//! não instala, nada atualiza sozinho).
//!
//! Só viaja o que tem equivalente no Orion: as **skills** (texto). Servidores MCP, hooks, comandos
//! e agentes ficam de fora e entram numa lista "ignorado" que a pessoa lê **antes** de instalar. O
//! resultado é um pacote Orion comum (`manifest.json` + skills) que segue o caminho de sempre:
//! instala inativo, sem concessão, e só vale depois da revisão e da ativação.

use super::installer::{fingerprint, Package, MAX_FILE};
use super::plugins::{parse_manifest, perr, PluginError};
use super::skills::cabecalho;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::LazyLock;

type R<T> = Result<T, PluginError>;

const FORMATOS: [(&str, &str); 2] = [(".claude-plugin/plugin.json", "claude-code"), (".codex-plugin/plugin.json", "codex")];
const PERMITIDOS: [&str; 6] = ["name", "description", "license", "compatibility", "metadata", "allowed-tools"];
const SEM_EQUIVALENTE: [(&str, &str); 4] = [("commands", "comandos de barra"), ("agents", "agentes"), ("hooks", "hooks (rodariam código na sua máquina)"), ("output-styles", "estilos de saída")];
static FRONT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)\A---\s*\n(.*?)\n---\s*(?:\n|\z)").unwrap());
static SEMVER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[a-z0-9.-]+)?$").unwrap());
static NAO_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9_-]+").unwrap());
static ID_OK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,47}$").unwrap());

pub struct Conversao {
    pub package: Package,
    pub formato: String,
    pub ignorado: Vec<String>,
}

impl Conversao {
    pub fn resumo(&self) -> Value {
        let m = &self.package.manifest;
        json!({"formato": self.formato, "id": m.id, "name": m.name, "version": m.version, "description": m.description, "license": m.license,
               "skills": m.skills.iter().map(|s| s.rsplit('/').next().unwrap_or("")).collect::<Vec<_>>(), "ignorado": self.ignorado, "digest": self.package.digest})
    }
}

/// Zip de uma pasta (`meu-plugin/...`): tira o prefixo comum se o marcador só aparece dentro.
fn sem_pasta_envolvente(files: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    if FORMATOS.iter().any(|(p, _)| files.contains_key(*p)) || files.contains_key("manifest.json") {
        return files;
    }
    let topos: std::collections::HashSet<&str> = files.keys().map(|p| p.split('/').next().unwrap_or("")).collect();
    if topos.len() != 1 || files.keys().any(|p| !p.contains('/')) {
        return files;
    }
    let prefixo = format!("{}/", topos.into_iter().next().unwrap());
    files.into_iter().map(|(p, d)| (p[prefixo.len()..].to_string(), d)).collect()
}

fn detectar(files: &BTreeMap<String, Vec<u8>>) -> R<&'static str> {
    if files.contains_key("manifest.json") {
        return Ok("orion");
    }
    FORMATOS.iter().find(|(c, _)| files.contains_key(*c)).map(|(_, f)| *f).ok_or_else(|| perr("import_format_unknown"))
}

fn id_de(nome: &str) -> R<String> {
    let s = NAO_ID.replace_all(&nome.trim().to_lowercase(), "-").trim_matches(|c| c == '-' || c == '_').to_string();
    let id: String = s.chars().take(48).collect();
    if !ID_OK.is_match(&id) {
        return Err(perr("import_name_invalid"));
    }
    Ok(id)
}

fn como_texto(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => if *b { "True".into() } else { "False".into() },
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "None".into(),
        outro => serde_yaml::to_string(outro).unwrap_or_default().trim().to_string(),
    }
}

fn skill(pasta: &str, dado: &[u8], ignorado: &mut Vec<String>) -> Option<Vec<u8>> {
    let Ok(texto) = std::str::from_utf8(dado) else {
        ignorado.push(format!("skill {pasta}: SKILL.md não é UTF-8"));
        return None;
    };
    let Some(achou) = FRONT.captures(texto) else {
        ignorado.push(format!("skill {pasta}: sem frontmatter"));
        return None;
    };
    let Some(cab) = serde_yaml::from_str::<serde_yaml::Value>(&achou[1]).ok().and_then(|v| v.as_mapping().cloned()) else {
        ignorado.push(format!("skill {pasta}: frontmatter ilegível"));
        return None;
    };
    let mut fora: Vec<String> = cab.keys().map(como_texto).filter(|k| !PERMITIDOS.contains(&k.as_str()) || k == "allowed-tools").collect();
    fora.sort();
    let mut novo = serde_yaml::Mapping::new();
    for (k, v) in &cab {
        let ks = como_texto(k);
        if PERMITIDOS.contains(&ks.as_str()) && ks != "allowed-tools" {
            novo.insert(k.clone(), v.clone());
        }
    }
    let nome = match novo.get("name") {
        Some(v) if !matches!(v, serde_yaml::Value::Null) && !como_texto(v).is_empty() => como_texto(v),
        _ => pasta.to_string(),
    };
    novo.insert("name".into(), nome.clone().into());
    match novo.get("metadata").cloned() {
        Some(serde_yaml::Value::Mapping(m)) => {
            let mm: serde_yaml::Mapping = m.iter().map(|(k, v)| (como_texto(k).into(), como_texto(v).into())).collect();
            novo.insert("metadata".into(), mm.into());
        }
        Some(_) => {
            novo.remove("metadata");
        }
        None => {}
    }
    for k in ["description", "license", "compatibility"] {
        if let Some(v) = novo.get(k).cloned() {
            novo.insert(k.into(), como_texto(&v).into());
        }
    }
    let ymal = serde_yaml::to_string(&novo).unwrap_or_default();
    if cabecalho(&ymal).is_err() {
        ignorado.push(format!("skill {pasta}: nome ou descrição fora do formato do Orion"));
        return None;
    }
    if nome != pasta {
        ignorado.push(format!("skill {pasta}: o nome ({nome}) difere da pasta"));
        return None;
    }
    if !fora.is_empty() {
        ignorado.push(format!("skill {pasta}: campos ignorados ({})", fora.join(", ")));
    }
    let corpo = &texto[achou.get(0).unwrap().end()..];
    Some(format!("---\n{}\n---\n{corpo}", ymal.trim()).into_bytes())
}

/// Converte os arquivos de um ZIP no pacote Orion. Erra com um código de `PluginError`.
pub fn converter(files: BTreeMap<String, Vec<u8>>) -> R<Conversao> {
    let files = sem_pasta_envolvente(files);
    let formato = detectar(&files)?;
    if formato == "orion" {
        let manifest = parse_manifest(&files["manifest.json"])?;
        let digest = fingerprint(&files);
        return Ok(Conversao { package: Package { manifest, digest, files }, formato: "orion".into(), ignorado: vec![] });
    }
    let caminho = FORMATOS.iter().find(|(_, f)| *f == formato).map(|(c, _)| *c).unwrap();
    let bruto: Value = serde_json::from_slice(&files[caminho]).map_err(|_| perr("import_manifest_invalid"))?;
    let nome_pacote = bruto.get("name").and_then(Value::as_str).ok_or_else(|| perr("import_manifest_invalid"))?.to_string();
    let mut ignorado = vec![];
    let mut versao = bruto.get("version").and_then(Value::as_str).unwrap_or("").to_string();
    if !SEMVER.is_match(&versao) {
        ignorado.push(format!("versão {}: usei 0.1.0", if versao.is_empty() { "ausente" } else { "inválida" }));
        versao = "0.1.0".into();
    }
    let (mut novos, mut skills) = (BTreeMap::new(), vec![]);
    let mut pastas: Vec<String> = files.keys().filter(|p| p.starts_with("skills/") && p.matches('/').count() >= 2).filter_map(|p| p.split('/').nth(1).map(String::from)).collect();
    pastas.sort();
    pastas.dedup();
    for pasta in pastas {
        let Some(principal) = files.get(&format!("skills/{pasta}/SKILL.md")) else {
            ignorado.push(format!("skill {pasta}: sem SKILL.md"));
            continue;
        };
        let Some(convertida) = skill(&pasta, principal, &mut ignorado) else { continue };
        novos.insert(format!("skills/{pasta}/SKILL.md"), convertida);
        skills.push(format!("skills/{pasta}"));
        // referências e anexos vão como estão (scripts não rodam)
        for (c, d) in &files {
            if c.starts_with(&format!("skills/{pasta}/")) && *c != format!("skills/{pasta}/SKILL.md") {
                novos.insert(c.clone(), d.clone());
            }
        }
    }
    for (pasta, o_que) in SEM_EQUIVALENTE {
        let n = files.keys().filter(|p| p.starts_with(&format!("{pasta}/"))).count();
        if n > 0 {
            ignorado.push(format!("{pasta}/: {n} arquivo(s) de {o_que}, sem equivalente no Orion"));
        }
    }
    if files.contains_key(".mcp.json") || bruto.get("mcpServers").is_some() || bruto.get("mcp").is_some() {
        let mut servidores: Vec<String> = files.get(".mcp.json").and_then(|d| serde_json::from_slice::<Value>(d).ok()).and_then(|v| v.get("mcpServers").and_then(Value::as_object).map(|m| m.keys().cloned().collect())).unwrap_or_default();
        servidores.sort();
        let quais = if servidores.is_empty() { String::new() } else { format!(" ({})", servidores.join(", ")) };
        ignorado.push(format!("servidores MCP{quais}: não viajam em pacote; cadastre-os em Conexões MCP, onde você revisa o comando e a classe de risco"));
    }
    if skills.is_empty() {
        return Err(perr("import_nothing_to_import"));
    }
    let desc = bruto.get("description").and_then(Value::as_str).map(str::trim).filter(|d| !d.is_empty()).map(String::from).unwrap_or_else(|| format!("Importado: {nome_pacote}"));
    let nome = bruto.get("displayName").and_then(Value::as_str).unwrap_or(&nome_pacote);
    let manifesto = json!({"schema_version": 1, "orion_api": 1, "id": id_de(&nome_pacote)?, "version": versao,
        "name": nome.chars().take(80).collect::<String>(), "description": desc.chars().take(1024).collect::<String>(),
        "license": bruto.get("license").and_then(Value::as_str).unwrap_or("LicenseRef-Importado").chars().take(128).collect::<String>(),
        "skills": skills, "capabilities": []});
    let dados = serde_json::to_vec_pretty(&manifesto).map_err(|_| perr("import_manifest_invalid"))?;
    novos.insert("manifest.json".into(), dados.clone());
    if novos.values().any(|d| d.len() > MAX_FILE) {
        return Err(perr("bundle_size_limit"));
    }
    let digest = fingerprint(&novos);
    Ok(Conversao { package: Package { manifest: parse_manifest(&dados)?, digest, files: novos }, formato: formato.into(), ignorado })
}

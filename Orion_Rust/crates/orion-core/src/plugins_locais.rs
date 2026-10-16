//! Plugins locais (REGRAS.md: plugin é pacote sem poder próprio): pacote de skills e servidores MCP,
//! instalado e concedido.
//!
//! Um plugin é uma pasta com `plugin.json` (nome, versão, descrição), uma pasta opcional `skills/`
//! (cada skill é uma pasta com `SKILL.md`) e servidores MCP declarados no próprio manifesto (no
//! formato do `mcp.json`, com a classe de risco de cada ferramenta).
//!
//! - **Instalar não executa nada**: copiar a pasta para `<dados>/plugins/<nome>` só valida e copia
//!   (sem link simbólico, até 200 arquivos e 5 MB).
//! - **Nada vale sem concessão**: `conceder` registra o hash do pacote inteiro (manifesto + todos os
//!   arquivos); `revogar` tira. **Qualquer mudança no pacote invalida a concessão** até conceder
//!   de novo.
//! - A classe de risco vem do manifesto que você leu, nunca do servidor. Mudanças valem depois de
//!   reiniciar o Orion.

use crate::mcp_client::ServerConfig;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static NOME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9]{0,11}$").unwrap());
static NOME_SERVIDOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9]{0,10}$").unwrap());
const MAX_ARQUIVOS: usize = 200;
const MAX_BYTES: u64 = 5 * 1024 * 1024;
const ESTADO: &str = ".estado.json";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PluginError(pub String);

type R<T> = Result<T, PluginError>;

fn erro<T>(m: impl Into<String>) -> R<T> {
    Err(PluginError(m.into()))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifesto {
    pub name: String,
    pub version: String,
    pub description: String,
    /// pasta (relativa ao plugin) com as skills; padrão: `skills/` se existir
    #[serde(default)]
    pub skills: Option<String>,
    #[serde(default)]
    pub mcp: HashMap<String, ServerConfig>,
}

#[derive(Debug, Clone)]
pub struct Plugin {
    pub nome: String,
    pub versao: String,
    pub descricao: String,
    pub pasta: PathBuf,
    pub manifesto: Manifesto,
    pub hash: String,
    /// há concessão e ela ainda vale para ESTE pacote
    pub concedido: bool,
    /// já houve concessão, mas o pacote mudou depois (precisa conceder de novo)
    pub mudou: bool,
    pub skills: usize,
    pub servidores: BTreeMap<String, ServerConfig>,
}

fn arquivos(pasta: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let mut pilha = vec![pasta.to_path_buf()];
    while let Some(d) = pilha.pop() {
        if let Ok(rd) = fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    pilha.push(p);
                } else if p.is_file() && p.file_name().is_some_and(|n| n != ESTADO) {
                    out.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

pub fn hash_do_pacote(pasta: &Path) -> String {
    let mut h = Sha256::new();
    for p in arquivos(pasta) {
        let rel = p.strip_prefix(pasta).unwrap_or(&p).to_string_lossy().replace('\\', "/");
        h.update(rel.as_bytes());
        h.update([0u8]);
        h.update(Sha256::digest(fs::read(&p).unwrap_or_default()));
    }
    hex::encode(h.finalize())
}

fn validar_pasta(pasta: &Path) -> R<()> {
    if !pasta.is_dir() {
        return erro("a pasta do plugin não existe");
    }
    let (mut n, mut total) = (0usize, 0u64);
    let mut pilha = vec![pasta.to_path_buf()];
    while let Some(d) = pilha.pop() {
        for e in fs::read_dir(&d).map_err(|_| PluginError("pasta ilegível".into()))?.flatten() {
            let p = e.path();
            let m = fs::symlink_metadata(&p).map_err(|_| PluginError("pasta ilegível".into()))?;
            if m.file_type().is_symlink() {
                return erro(format!("link simbólico não é aceito: {}", p.strip_prefix(pasta).unwrap_or(&p).to_string_lossy()));
            }
            if m.is_dir() {
                pilha.push(p);
            } else if m.is_file() {
                n += 1;
                total += m.len();
            }
        }
    }
    if n > MAX_ARQUIVOS || total > MAX_BYTES {
        return erro(format!("pacote grande demais (máx. {MAX_ARQUIVOS} arquivos e 5 MB)"));
    }
    Ok(())
}

pub fn ler_manifesto(pasta: &Path) -> R<Manifesto> {
    let arquivo = pasta.join("plugin.json");
    if !arquivo.is_file() {
        return erro("falta o plugin.json");
    }
    let bruto = fs::read_to_string(&arquivo).map_err(|_| PluginError("plugin.json ilegível".into()))?;
    let m: Manifesto = serde_json::from_str(&bruto).map_err(|e| PluginError(format!("plugin.json inválido: {}", e.to_string().chars().take(300).collect::<String>())))?;
    let inv = |c: &str| PluginError(format!("plugin.json inválido: {c}"));
    if !NOME.is_match(&m.name) {
        return Err(inv("name: minúsculas e dígitos, começando por letra, até 12"));
    }
    if m.version.is_empty() || m.version.chars().count() > 32 || m.description.is_empty() || m.description.chars().count() > 300 {
        return Err(inv("version (1–32) ou description (1–300) fora do limite"));
    }
    for (n, cfg) in &m.mcp {
        if !NOME_SERVIDOR.is_match(n) {
            return Err(inv(&format!("mcp.{n}: nome de servidor inválido (minúsculas/dígitos, até 11)")));
        }
        cfg.validar().map_err(|e| inv(&format!("mcp.{n}: {e}")))?;
    }
    Ok(m)
}

pub struct PluginStore {
    pub raiz: PathBuf,
}

impl PluginStore {
    pub fn new(raiz: impl Into<PathBuf>) -> Self {
        PluginStore { raiz: raiz.into() }
    }

    fn estado(&self) -> HashMap<String, String> {
        fs::read_to_string(self.raiz.join(ESTADO))
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v.as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))).collect()))
            .unwrap_or_default()
    }

    fn gravar_estado(&self, e: &HashMap<String, String>) -> R<()> {
        fs::create_dir_all(&self.raiz).map_err(|_| PluginError("não consegui gravar o estado".into()))?;
        let ord: BTreeMap<_, _> = e.iter().collect();
        fs::write(self.raiz.join(ESTADO), serde_json::to_string_pretty(&ord).unwrap()).map_err(|_| PluginError("não consegui gravar o estado".into()))
    }

    pub fn lista(&self) -> Vec<Plugin> {
        if !self.raiz.is_dir() {
            return vec![];
        }
        let estado = self.estado();
        let mut pastas: Vec<PathBuf> = fs::read_dir(&self.raiz).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
        pastas.sort();
        let mut saida = vec![];
        for pasta in pastas {
            // pasta estranha é ignorada: não derruba os outros
            let Ok(()) = validar_pasta(&pasta) else { continue };
            let Ok(m) = ler_manifesto(&pasta) else { continue };
            if pasta.file_name().map(|n| n.to_string_lossy().to_string()).as_deref() != Some(m.name.as_str()) {
                continue;
            }
            let h = hash_do_pacote(&pasta);
            let conc = estado.get(&m.name);
            let skills_dir = pasta.join(m.skills.as_deref().unwrap_or("skills"));
            let n_skills = if skills_dir.is_dir() { fs::read_dir(&skills_dir).map(|rd| rd.flatten().filter(|e| e.path().is_dir() && e.path().join("SKILL.md").is_file()).count()).unwrap_or(0) } else { 0 };
            saida.push(Plugin {
                nome: m.name.clone(),
                versao: m.version.clone(),
                descricao: m.description.clone(),
                pasta,
                hash: h.clone(),
                concedido: conc == Some(&h),
                mudou: conc.is_some_and(|c| *c != h),
                skills: n_skills,
                servidores: m.mcp.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                manifesto: m,
            });
        }
        saida
    }

    pub fn get(&self, nome: &str) -> Option<Plugin> {
        self.lista().into_iter().find(|p| p.nome == nome)
    }

    pub fn ativos(&self) -> Vec<Plugin> {
        self.lista().into_iter().filter(|p| p.concedido).collect()
    }

    pub fn pastas_de_skills(&self) -> Vec<PathBuf> {
        self.ativos().into_iter().map(|p| p.pasta.join(p.manifesto.skills.as_deref().unwrap_or("skills"))).filter(|d| d.is_dir()).collect()
    }

    /// Servidores dos plugins concedidos, com o nome `<plugin><servidor>` (único, sem `_`).
    pub fn servidores_mcp(&self) -> Vec<(String, ServerConfig)> {
        let mut saida: Vec<(String, ServerConfig)> = vec![];
        for p in self.ativos() {
            for (nome, cfg) in &p.servidores {
                let chave = format!("{}{}", p.nome, nome);
                // colisão (ab+cd × abc+d): vale o primeiro
                if !saida.iter().any(|(k, _)| *k == chave) {
                    saida.push((chave, cfg.clone()));
                }
            }
        }
        saida
    }

    /// Valida e copia. NÃO concede nada e NÃO executa nada do pacote.
    pub fn instalar(&self, origem: &Path, atualizar: bool) -> R<Plugin> {
        let origem = origem.canonicalize().map_err(|_| PluginError("a pasta do plugin não existe".into()))?;
        validar_pasta(&origem)?;
        let m = ler_manifesto(&origem)?;
        let destino = self.raiz.join(&m.name);
        if destino.exists() && !atualizar {
            return erro(format!("o plugin '{}' já está instalado (use atualizar)", m.name));
        }
        fs::create_dir_all(&self.raiz).map_err(|_| PluginError("não consegui criar a pasta de plugins".into()))?;
        let tmp = self.raiz.join(format!(".instalando-{}", m.name));
        let _ = fs::remove_dir_all(&tmp);
        copiar(&origem, &tmp).map_err(|_| PluginError("falha ao copiar o plugin".into()))?;
        if destino.exists() {
            fs::remove_dir_all(&destino).map_err(|_| PluginError("falha ao substituir o plugin".into()))?;
        }
        fs::rename(&tmp, &destino).map_err(|_| PluginError("falha ao publicar o plugin".into()))?;
        self.get(&m.name).ok_or_else(|| PluginError("plugin instalado não foi reconhecido".into()))
    }

    pub fn conceder(&self, nome: &str) -> R<Plugin> {
        let p = self.get(nome).ok_or_else(|| PluginError(format!("plugin '{nome}' não encontrado")))?;
        let mut estado = self.estado();
        estado.insert(nome.into(), p.hash);
        self.gravar_estado(&estado)?;
        self.get(nome).ok_or_else(|| PluginError(format!("plugin '{nome}' não encontrado")))
    }

    pub fn revogar(&self, nome: &str) -> R<bool> {
        let mut estado = self.estado();
        if estado.remove(nome).is_none() {
            return Ok(false);
        }
        self.gravar_estado(&estado)?;
        Ok(true)
    }

    pub fn remover(&self, nome: &str) -> R<bool> {
        if !NOME.is_match(nome) || !self.raiz.join(nome).is_dir() {
            return Ok(false);
        }
        self.revogar(nome)?;
        fs::remove_dir_all(self.raiz.join(nome)).map_err(|_| PluginError("não consegui remover".into()))?;
        Ok(true)
    }
}

fn copiar(de: &Path, para: &Path) -> std::io::Result<()> {
    fs::create_dir_all(para)?;
    for e in fs::read_dir(de)? {
        let e = e?;
        let alvo = para.join(e.file_name());
        if e.path().is_dir() {
            copiar(&e.path(), &alvo)?;
        } else {
            fs::copy(e.path(), alvo)?;
        }
    }
    Ok(())
}

/// O que a concessão libera, para a pessoa ler antes de conceder (CLI, API e interface).
pub fn describe(p: &Plugin) -> Value {
    let servidores: Vec<Value> = p
        .servidores
        .iter()
        .map(|(nome, cfg)| {
            let ferramentas: serde_json::Map<String, Value> = cfg.tools.iter().map(|(t, r)| (t.clone(), json!(r.risk.clone().unwrap_or_else(|| cfg.default_risk.clone())))).collect();
            let cmd = std::iter::once(cfg.command.clone().unwrap_or_default()).chain(cfg.args.clone()).collect::<Vec<_>>().join(" ");
            json!({"nome": format!("{}{}", p.nome, nome), "transporte": if cfg.url.is_some() { "remoto (HTTP)" } else { "local (comando)" },
                   "executa": cfg.url.clone().unwrap_or_else(|| cmd.chars().take(200).collect()), "risco_padrao": cfg.default_risk,
                   "ferramentas": ferramentas, "externo": cfg.external})
        })
        .collect();
    json!({"nome": p.nome, "versao": p.versao, "descricao": p.descricao, "skills": p.skills, "servidores": servidores,
           "estado": if p.concedido { "ativo" } else if p.mudou { "mudou" } else { "sem_concessao" }})
}

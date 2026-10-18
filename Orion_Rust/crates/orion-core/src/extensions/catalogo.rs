//! Catálogo de plugins, local e remoto (REGRAS.md: baixar não instala, nada atualiza sozinho).
//!
//! - **Local**: a pasta `<dados>/catalogo/` com `catalogo.json` e os `.zip` que ele lista.
//! - **Remoto**: só as URLs cadastradas em `ORION_PLUGIN_CATALOGS`. Baixa-se o índice (até 256 KB)
//!   e, quando você manda instalar, **um** pacote do mesmo host do índice, sempre pela barreira de
//!   rede (`netguard`). O SHA-256 que o índice fixa é conferido antes de o pacote entrar em qualquer
//!   lugar.

use super::archives::{archive_snapshot, MAX_ARCHIVE};
use super::plugins::{perr, PluginError};
use orion_ai::netguard;
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub const MAX_INDICE: usize = 256 * 1024;
pub const MAX_ENTRADAS: usize = 200;
pub const MAX_FONTES: usize = 5;
pub const LOCAL: &str = "local";
static ARQUIVO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\.zip$").unwrap());
static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,47}$").unwrap());
static VERSAO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[a-z0-9.-]+)?$").unwrap());
static SHA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-f0-9]{64}$").unwrap());

type R<T> = Result<T, PluginError>;

#[derive(Debug, Clone)]
pub struct Entrada {
    /// "local" ou a URL do índice
    pub fonte: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub sha256: String,
    /// arquivo (local) ou URL do pacote (remoto)
    pub destino: String,
}

impl Entrada {
    pub fn publico(&self) -> Value {
        json!({"fonte": self.fonte, "id": self.id, "name": self.name, "version": self.version, "description": self.description, "sha256": self.sha256})
    }
}

fn chave(v: &str) -> (Vec<u64>, u8) {
    let (base, _, pre) = v.partition_point_str();
    (base.split('.').map(|x| x.parse().unwrap_or(0)).collect(), if pre.is_empty() { 1 } else { 0 })
}

trait Particao {
    fn partition_point_str(&self) -> (&str, &str, &str);
}
impl Particao for str {
    fn partition_point_str(&self) -> (&str, &str, &str) {
        match self.split_once('-') {
            Some((a, b)) => (a, "-", b),
            None => (self, "", ""),
        }
    }
}

/// a > b, por número (a parte depois do `-` conta como pré-lançamento: vale menos).
pub fn versao_maior(a: &str, b: &str) -> bool {
    chave(a) > chave(b)
}

/// `ORION_PLUGIN_CATALOGS`: URLs https separadas por `;`, até 5.
pub fn urls_validas(bruto: &str) -> Result<Vec<String>, String> {
    let urls: Vec<String> = bruto.split(';').map(str::trim).filter(|u| !u.is_empty()).map(String::from).collect();
    if urls.len() > MAX_FONTES {
        return Err(format!("no máximo {MAX_FONTES} catálogos"));
    }
    for u in &urls {
        let ok = url::Url::parse(u).ok().is_some_and(|p| p.scheme() == "https" && p.host_str().is_some() && p.username().is_empty() && p.fragment().is_none());
        if !ok {
            return Err(format!("catálogo inválido (use https, sem usuário): {}", u.chars().take(80).collect::<String>()));
        }
    }
    Ok(urls)
}

struct EntradaJson {
    id: String,
    name: String,
    version: String,
    description: String,
    sha256: String,
    file: Option<String>,
    url: Option<String>,
}

fn indice(bruto: &[u8]) -> Result<Vec<EntradaJson>, ()> {
    let v: Value = serde_json::from_slice(bruto).map_err(|_| ())?;
    let o = v.as_object().ok_or(())?;
    if o.keys().any(|k| k != "plugins") {
        return Err(());
    }
    let lista = o.get("plugins").and_then(Value::as_array).filter(|a| a.len() <= MAX_ENTRADAS).ok_or(())?;
    let mut out = vec![];
    for e in lista {
        let m = e.as_object().ok_or(())?;
        if m.keys().any(|k| !["id", "name", "version", "description", "sha256", "file", "url"].contains(&k.as_str())) {
            return Err(());
        }
        let s = |k: &str| e.get(k).and_then(Value::as_str);
        let (id, name, version, sha) = (s("id").ok_or(())?, s("name").ok_or(())?, s("version").ok_or(())?, s("sha256").ok_or(())?);
        let description = s("description").unwrap_or("");
        if !ID.is_match(id) || name.is_empty() || name.chars().count() > 80 || !VERSAO.is_match(version) || version.len() > 80 || description.chars().count() > 1024 || !SHA.is_match(sha) {
            return Err(());
        }
        let file = s("file").map(String::from);
        if file.as_ref().is_some_and(|f| !ARQUIVO.is_match(f)) || s("url").is_some_and(|u| u.len() > 2048) {
            return Err(());
        }
        out.push(EntradaJson { id: id.into(), name: name.into(), version: version.into(), description: description.into(), sha256: sha.into(), file, url: s("url").map(String::from) });
    }
    Ok(out)
}

fn mesma_origem(a: &str, b: &str) -> bool {
    match (url::Url::parse(a), url::Url::parse(b)) {
        (Ok(pa), Ok(pb)) => pa.scheme() == pb.scheme() && pa.host_str() == pb.host_str() && pa.port_or_known_default() == pb.port_or_known_default() && pa.scheme() == "https",
        _ => false,
    }
}

pub struct Catalogos {
    pasta: PathBuf,
    urls: Vec<String>,
}

fn host_de(u: &str) -> String {
    url::Url::parse(u).ok().and_then(|p| p.host_str().map(String::from)).unwrap_or_else(|| "?".into())
}

impl Catalogos {
    pub fn new(pasta: PathBuf, urls: Vec<String>) -> Self {
        Catalogos { pasta, urls }
    }

    fn local(&self) -> R<Vec<Entrada>> {
        let arquivo = self.pasta.join("catalogo.json");
        if !arquivo.is_file() {
            return Ok(vec![]);
        }
        if std::fs::metadata(&arquivo).map_err(|_| perr("catalog_index_invalid"))?.len() as usize > MAX_INDICE {
            return Err(perr("catalog_index_too_large"));
        }
        let dados = std::fs::read(&arquivo).map_err(|_| perr("catalog_index_invalid"))?;
        let lista = indice(&dados).map_err(|_| perr("catalog_index_invalid"))?;
        Ok(lista.into_iter().filter_map(|e| e.file.clone().map(|f| Entrada { fonte: LOCAL.into(), id: e.id, name: e.name, version: e.version, description: e.description, sha256: e.sha256, destino: f })).collect())
    }

    async fn remoto(&self, url: &str) -> R<Vec<Entrada>> {
        let mut m = orion_ai::saidas::medir(&format!("catalogo:{}", host_de(url)), "catalogo", "", url.len(), "");
        let r = netguard::buscar(url, 15.0, MAX_INDICE + 1).await.map_err(|e| PluginError(format!("catalog_unreachable: {}", e.to_string().chars().take(60).collect::<String>())))?;
        m.ok = r.status == 200;
        m.bytes_in = r.corpo.len();
        drop(m);
        if r.status != 200 || r.truncado || r.corpo.len() > MAX_INDICE {
            return Err(perr("catalog_index_invalid"));
        }
        let lista = indice(&r.corpo).map_err(|_| perr("catalog_index_invalid"))?;
        let base = url::Url::parse(&r.url_final).map_err(|_| perr("catalog_index_invalid"))?;
        let mut saida = vec![];
        for e in lista {
            let Some(u) = e.url else { continue };
            let Ok(alvo) = base.join(&u) else { continue };
            // pacote em outro host: recusado
            if !mesma_origem(alvo.as_str(), &r.url_final) {
                continue;
            }
            saida.push(Entrada { fonte: url.into(), id: e.id, name: e.name, version: e.version, description: e.description, sha256: e.sha256, destino: alvo.to_string() });
        }
        Ok(saida)
    }

    /// (entradas, estado de cada fonte). Fonte que falha não derruba as outras.
    pub async fn listar(&self) -> (Vec<Entrada>, Vec<Value>) {
        let (mut entradas, mut fontes) = (vec![], vec![]);
        let todas = std::iter::once(LOCAL.to_string()).chain(self.urls.iter().cloned());
        for fonte in todas {
            let r = if fonte == LOCAL { self.local() } else { self.remoto(&fonte).await };
            match r {
                Ok(a) => {
                    fontes.push(json!({"fonte": fonte, "ok": true, "total": a.len()}));
                    entradas.extend(a);
                }
                Err(e) => fontes.push(json!({"fonte": fonte, "ok": false, "erro": e.0})),
            }
        }
        (entradas, fontes)
    }

    /// O ZIP do pacote, com o SHA-256 do índice já conferido. Não instala nada.
    pub async fn baixar(&self, fonte: &str, id: &str, versao: Option<&str>) -> R<(Entrada, Vec<u8>)> {
        if fonte != LOCAL && !self.urls.iter().any(|u| u == fonte) {
            return Err(perr("catalog_source_unknown"));
        }
        let (entradas, _) = self.listar().await;
        let mut candidatas: Vec<Entrada> = entradas.into_iter().filter(|e| e.fonte == fonte && e.id == id && versao.is_none_or(|v| e.version == v)).collect();
        candidatas.sort_by_key(|e| e.version.split('-').next().unwrap_or("").split('.').map(|x| x.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>());
        let entrada = candidatas.pop().ok_or_else(|| perr("catalog_entry_not_found"))?;
        let dados = if fonte == LOCAL { self.ler_local(&entrada)? } else { self.baixar_remoto(&entrada).await? };
        if hex::encode(Sha256::digest(&dados)) != entrada.sha256 {
            return Err(perr("catalog_hash_mismatch"));
        }
        Ok((entrada, dados))
    }

    fn ler_local(&self, e: &Entrada) -> R<Vec<u8>> {
        let caminho = self.pasta.join(&e.destino);
        let real = caminho.canonicalize().map_err(|_| perr("catalog_package_missing"))?;
        if !ARQUIVO.is_match(&e.destino) || real.parent() != self.pasta.canonicalize().ok().as_deref() {
            return Err(perr("catalog_path_invalid"));
        }
        let m = std::fs::symlink_metadata(&caminho).map_err(|_| perr("catalog_package_missing"))?;
        if m.file_type().is_symlink() || m.len() as usize > MAX_ARCHIVE {
            return Err(perr("catalog_package_invalid"));
        }
        std::fs::read(&caminho).map_err(|_| perr("catalog_package_missing"))
    }

    async fn baixar_remoto(&self, e: &Entrada) -> R<Vec<u8>> {
        if !mesma_origem(&e.destino, &e.fonte) {
            return Err(perr("catalog_package_other_host"));
        }
        let mut m = orion_ai::saidas::medir(&format!("catalogo:{}", host_de(&e.destino)), "catalogo", "", e.destino.len(), "");
        let r = netguard::buscar(&e.destino, 30.0, MAX_ARCHIVE + 1).await.map_err(|err| PluginError(format!("catalog_unreachable: {}", err.to_string().chars().take(60).collect::<String>())))?;
        m.ok = r.status == 200;
        m.bytes_in = r.corpo.len();
        drop(m);
        if r.status != 200 || r.truncado || r.corpo.len() > MAX_ARCHIVE {
            return Err(perr("catalog_package_invalid"));
        }
        Ok(r.corpo)
    }
}

/// Publica ZIPs no catálogo local (usado por `orion plugin publicar`). Cada item é
/// (nome do arquivo .zip, conteúdo); o índice é refeito com os dados do manifesto.
pub fn escrever_indice(pasta: &Path, pacotes: &[(String, Vec<u8>)]) -> R<PathBuf> {
    std::fs::create_dir_all(pasta).map_err(|_| perr("catalog_path_invalid"))?;
    let mut atual: Vec<Value> = std::fs::read(pasta.join("catalogo.json")).ok().and_then(|d| serde_json::from_slice::<Value>(&d).ok()).and_then(|v| v["plugins"].as_array().cloned()).unwrap_or_default();
    for (nome, dados) in pacotes {
        let m = archive_snapshot(dados, nome)?.manifest;
        std::fs::write(pasta.join(nome), dados).map_err(|_| perr("catalog_path_invalid"))?;
        atual.retain(|e| !(e["id"] == json!(m.id) && e["version"] == json!(m.version)));
        atual.push(json!({"id": m.id, "name": m.name, "version": m.version, "description": m.description, "sha256": hex::encode(Sha256::digest(dados)), "file": nome}));
    }
    let alvo = pasta.join("catalogo.json");
    std::fs::write(&alvo, serde_json::to_vec_pretty(&json!({"plugins": atual})).unwrap()).map_err(|_| perr("catalog_path_invalid"))?;
    Ok(alvo)
}

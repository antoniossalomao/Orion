//! Atualizador do Orion (F10.14/F10.15).
//!
//! Regras: fail-closed. Sem chave pública ou sem endereço do feed, não faz nada. O feed
//! (`latest.json`) só vale com assinatura minisign válida; cada arquivo só entra com o SHA-256 que
//! o feed assinado declara; só versão **maior** que a instalada é aceita (sem downgrade); os
//! arquivos antigos ficam como `.bak` e `rollback` os devolve. Nada baixado é executado aqui.

use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "orion-install.json";
pub const MAX_FEED_BYTES: u64 = 256 * 1024;
pub const MAX_FILE_BYTES: u64 = 400 * 1024 * 1024;
/// Únicos arquivos que o feed pode trocar na pasta do app.
pub const ALLOWED_FILES: [&str; 2] = ["orion.exe", "orion-llama.exe"];

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("atualização não configurada: {0}")]
    NotConfigured(&'static str),
    #[error("assinatura do feed inválida: {0}")]
    BadSignature(String),
    #[error("feed inválido: {0}")]
    BadFeed(String),
    #[error("SHA-256 de {0} não confere")]
    BadHash(String),
    #[error("download: {0}")]
    Fetch(String),
    #[error("disco: {0}")]
    Io(String),
}

type Result<T> = std::result::Result<T, UpdateError>;

fn io<T>(r: std::io::Result<T>) -> Result<T> {
    r.map_err(|e| UpdateError::Io(e.to_string()))
}

/// `update.json` na pasta de dados, gravado pelo instalador.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateConfig {
    #[serde(default)]
    pub channel: String,
    /// Falso: só avisa que há versão nova. Verdadeiro: baixa, confere e troca.
    #[serde(default)]
    pub auto_update: bool,
    /// Verdadeiro: baixa e confere agora, mas só troca os arquivos quando o Orion fecha (ou na próxima abertura).
    #[serde(default)]
    pub install_on_close: bool,
    #[serde(default)]
    pub feed_url: Option<String>,
    /// Chave pública minisign (texto do `.pub` ou só a linha base64).
    #[serde(default)]
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedFile {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Feed {
    pub version: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub notes: String,
    pub files: Vec<FeedFile>,
}

pub trait Fetch {
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>>;
}

/// Download real: só HTTPS, com teto de tamanho.
pub struct HttpFetch;

impl Fetch for HttpFetch {
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>> {
        use std::io::Read;
        if !url.starts_with("https://") {
            return Err(UpdateError::Fetch(format!("só HTTPS é aceito: {url}")));
        }
        let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(300)).build().map_err(|e| UpdateError::Fetch(e.to_string()))?;
        let resp = client.get(url).send().and_then(|r| r.error_for_status()).map_err(|e| UpdateError::Fetch(e.to_string()))?;
        let mut out = Vec::new();
        resp.take(max_bytes + 1).read_to_end(&mut out).map_err(|e| UpdateError::Fetch(e.to_string()))?;
        if out.len() as u64 > max_bytes {
            return Err(UpdateError::Fetch(format!("{url} passa de {max_bytes} bytes")));
        }
        Ok(out)
    }
}

/// Versões como números separados por ponto; `None` se não for isso.
pub fn parse_version(v: &str) -> Option<Vec<u64>> {
    let p: Option<Vec<u64>> = v.trim().split('.').map(|x| x.parse().ok()).collect();
    p.filter(|p| !p.is_empty() && p.len() <= 4)
}

pub fn is_newer(candidate: &str, installed: &str) -> bool {
    let (Some(mut c), Some(mut i)) = (parse_version(candidate), parse_version(installed)) else { return false };
    let n = c.len().max(i.len());
    c.resize(n, 0);
    i.resize(n, 0);
    c > i
}

fn public_key(raw: &str) -> Result<PublicKey> {
    // aceita o arquivo `.pub` inteiro ou só a linha da chave
    let line = raw.lines().map(str::trim).rfind(|l| !l.is_empty() && !l.starts_with("untrusted comment")).unwrap_or("");
    PublicKey::from_base64(line).map_err(|e| UpdateError::BadSignature(format!("chave pública: {e}")))
}

/// Confere a assinatura do feed e o lê. Só depois disso qualquer campo dele é confiável.
pub fn verify_feed(feed_bytes: &[u8], signature: &str, public_key_text: &str) -> Result<Feed> {
    let pk = public_key(public_key_text)?;
    let sig = Signature::decode(signature).map_err(|e| UpdateError::BadSignature(e.to_string()))?;
    pk.verify(feed_bytes, &sig, false).map_err(|e| UpdateError::BadSignature(e.to_string()))?;
    let feed: Feed = serde_json::from_slice(feed_bytes).map_err(|e| UpdateError::BadFeed(e.to_string()))?;
    if parse_version(&feed.version).is_none() {
        return Err(UpdateError::BadFeed(format!("versão {:?}", feed.version)));
    }
    for f in &feed.files {
        if !ALLOWED_FILES.contains(&f.name.as_str()) {
            return Err(UpdateError::BadFeed(format!("arquivo não permitido: {}", f.name)));
        }
        if !f.url.starts_with("https://") || f.bytes == 0 || f.bytes > MAX_FILE_BYTES || f.sha256.len() != 64 || !f.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(UpdateError::BadFeed(format!("entrada inválida para {}", f.name)));
        }
    }
    if feed.files.is_empty() {
        return Err(UpdateError::BadFeed("sem arquivos".into()));
    }
    Ok(feed)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    /// Há versão nova, mas `auto_update` está desligado.
    Available(String),
    Updated(String),
    /// Baixada e conferida; a troca acontece ao fechar o Orion (`apply_staged`).
    Staged(String),
}

/// Versão instalada, lida do manifesto do instalador.
fn installed_version(app_dir: &Path) -> Result<String> {
    let m: serde_json::Value = serde_json::from_slice(&io(std::fs::read(app_dir.join(MANIFEST)))?).map_err(|e| UpdateError::Io(format!("{MANIFEST}: {e}")))?;
    Ok(m["version"].as_str().unwrap_or("0").to_string())
}

/// Baixa o feed assinado e confere a assinatura. Só depois disso qualquer campo dele vale.
fn buscar_feed(url: &str, key: &str, fetch: &dyn Fetch) -> Result<Feed> {
    let feed_bytes = fetch.get(url, MAX_FEED_BYTES)?;
    let sig = String::from_utf8(fetch.get(&format!("{url}.minisig"), 8 * 1024)?).map_err(|e| UpdateError::BadSignature(e.to_string()))?;
    verify_feed(&feed_bytes, &sig, key)
}

/// Instalação nova: baixa os arquivos do release (feed assinado, SHA-256 de cada um) para `destino`,
/// sem tocar em mais nada. Quem chama copia de lá. `progresso(feitos, total)` conta arquivos.
pub fn baixar_release(feed_url: &str, public_key: &str, fetch: &dyn Fetch, destino: &Path, progresso: &dyn Fn(usize, usize)) -> Result<Feed> {
    if public_key.trim().is_empty() {
        return Err(UpdateError::NotConfigured("sem chave pública do release (recusa baixar sem verificar)"));
    }
    let feed = buscar_feed(feed_url, public_key, fetch)?;
    io(std::fs::create_dir_all(destino))?;
    progresso(0, feed.files.len());
    for (i, f) in feed.files.iter().enumerate() {
        let bytes = fetch.get(&f.url, f.bytes.min(MAX_FILE_BYTES))?;
        if bytes.len() as u64 != f.bytes || hex::encode(Sha256::digest(&bytes)).to_lowercase() != f.sha256.to_lowercase() {
            return Err(UpdateError::BadHash(f.name.clone()));
        }
        io(std::fs::write(destino.join(&f.name), &bytes))?;
        progresso(i + 1, feed.files.len());
    }
    Ok(feed)
}

pub fn check(config: &UpdateConfig, app_dir: &Path, fetch: &dyn Fetch, apply: bool) -> Result<Outcome> {
    let url = config.feed_url.as_deref().ok_or(UpdateError::NotConfigured("sem feed_url"))?;
    let key = config.public_key.as_deref().ok_or(UpdateError::NotConfigured("sem chave pública (recusa atualizar sem verificar)"))?;
    let feed = buscar_feed(url, key, fetch)?;
    if !is_newer(&feed.version, &installed_version(app_dir)?) {
        return Ok(Outcome::UpToDate);
    }
    if !(apply || config.auto_update) {
        return Ok(Outcome::Available(feed.version));
    }
    if config.install_on_close {
        stage(&feed, app_dir, fetch)?;
        return Ok(Outcome::Staged(feed.version));
    }
    install(&feed, app_dir, fetch)?;
    Ok(Outcome::Updated(feed.version))
}

/// Pasta com a atualização já baixada e conferida, à espera do fechamento do app.
pub const STAGED_DIR: &str = ".staged";
const STAGED_FEED: &str = "staged.json";

fn staged_dir(app_dir: &Path) -> PathBuf {
    app_dir.join(STAGED_DIR)
}

/// Baixa e confere cada arquivo (SHA-256 do feed assinado) para uma pasta temporária e só então a publica como
/// `.staged/` por renomeação: quem lê `.staged/` nunca vê download pela metade, e `apply_staged` não mexe na temporária.
fn stage(feed: &Feed, app_dir: &Path, fetch: &dyn Fetch) -> Result<()> {
    let tmp = app_dir.join(".staged-tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    io(std::fs::create_dir_all(&tmp))?;
    let r = (|| {
        for f in &feed.files {
            let bytes = fetch.get(&f.url, f.bytes.min(MAX_FILE_BYTES))?;
            if bytes.len() as u64 != f.bytes || hex::encode(Sha256::digest(&bytes)).to_lowercase() != f.sha256.to_lowercase() {
                return Err(UpdateError::BadHash(f.name.clone()));
            }
            io(std::fs::write(tmp.join(&f.name), &bytes))?;
        }
        io(std::fs::write(tmp.join(STAGED_FEED), serde_json::to_vec_pretty(feed).map_err(|e| UpdateError::Io(e.to_string()))?))?;
        let dir = staged_dir(app_dir);
        let _ = std::fs::remove_dir_all(&dir);
        io(std::fs::rename(&tmp, &dir))
    })();
    if r.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    r
}

/// Há atualização pronta para trocar? Devolve a versão.
pub fn staged_version(app_dir: &Path) -> Option<String> {
    let feed: Feed = serde_json::from_slice(&std::fs::read(staged_dir(app_dir).join(STAGED_FEED)).ok()?).ok()?;
    Some(feed.version)
}

/// Troca os arquivos pela atualização guardada em `.staged/`. Chamada ao fechar o Orion e na abertura
/// seguinte (se o fechamento não chegou a rodar). Reconfere nome, versão maior e SHA-256 de cada arquivo
/// antes de trocar; o que não passar é descartado. Devolve a versão instalada, ou `None` se não havia nada.
pub fn apply_staged(app_dir: &Path) -> Result<Option<String>> {
    let dir = staged_dir(app_dir);
    // `.staged/` só existe completa (publicada por renomeação); sem ela não há nada a fazer e nada a apagar.
    let Ok(raw) = std::fs::read(dir.join(STAGED_FEED)) else { return Ok(None) };
    let valido = (|| -> Result<Feed> {
        let feed: Feed = serde_json::from_slice(&raw).map_err(|e| UpdateError::BadFeed(e.to_string()))?;
        if !is_newer(&feed.version, &installed_version(app_dir)?) {
            return Err(UpdateError::BadFeed("versão guardada não é maior que a instalada".into()));
        }
        for f in &feed.files {
            if !ALLOWED_FILES.contains(&f.name.as_str()) {
                return Err(UpdateError::BadFeed(format!("arquivo não permitido: {}", f.name)));
            }
            let bytes = io(std::fs::read(dir.join(&f.name)))?;
            if bytes.len() as u64 != f.bytes || hex::encode(Sha256::digest(&bytes)).to_lowercase() != f.sha256.to_lowercase() {
                return Err(UpdateError::BadHash(f.name.clone()));
            }
        }
        Ok(feed)
    })();
    let feed = match valido {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    // Falha na troca (sem permissão na pasta, por exemplo) mantém `.staged/` para a tarefa privilegiada tentar.
    swap(&feed, app_dir, &dir)?;
    let _ = std::fs::remove_dir_all(&dir);
    registrar_versao(app_dir, &feed)?;
    anotar_novidades(app_dir, &feed)?;
    Ok(Some(feed.version))
}

fn bak(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".bak");
    PathBuf::from(s)
}

/// Baixa e confere tudo antes de tocar no app; a troca é por renomeação com volta em caso de erro.
fn install(feed: &Feed, app_dir: &Path, fetch: &dyn Fetch) -> Result<()> {
    let staging = app_dir.join(format!(".update-{}", feed.version));
    let _ = std::fs::remove_dir_all(&staging);
    io(std::fs::create_dir_all(&staging))?;
    let resultado = (|| {
        for f in &feed.files {
            let bytes = fetch.get(&f.url, f.bytes.min(MAX_FILE_BYTES))?;
            if bytes.len() as u64 != f.bytes || hex::encode(Sha256::digest(&bytes)).to_lowercase() != f.sha256.to_lowercase() {
                return Err(UpdateError::BadHash(f.name.clone()));
            }
            io(std::fs::write(staging.join(&f.name), &bytes))?;
        }
        swap(feed, app_dir, &staging)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    resultado?;
    registrar_versao(app_dir, feed)?;
    anotar_novidades(app_dir, feed)
}

/// Troca: antigo -> `.bak`, novo (de `staging`) -> lugar; qualquer falha devolve o que já foi trocado.
fn swap(feed: &Feed, app_dir: &Path, staging: &Path) -> Result<()> {
    let mut trocados: Vec<PathBuf> = Vec::new();
    for f in &feed.files {
        let novo = staging.join(&f.name);
        let alvo = app_dir.join(&f.name);
        let r = (|| -> Result<()> {
            if alvo.exists() {
                let _ = std::fs::remove_file(bak(&alvo));
                io(std::fs::rename(&alvo, bak(&alvo)))?;
            }
            io(std::fs::rename(&novo, &alvo))
        })();
        if let Err(e) = r {
            if bak(&alvo).exists() && !alvo.exists() {
                let _ = std::fs::rename(bak(&alvo), &alvo);
            }
            for t in trocados.iter().rev() {
                // devolve o novo à pasta de origem: uma falha passageira deixa a atualização completa para tentar de novo
                if let Some(nome) = t.file_name() {
                    let _ = std::fs::rename(t, staging.join(nome));
                }
                let _ = std::fs::rename(bak(t), t);
            }
            return Err(e);
        }
        trocados.push(alvo);
    }
    Ok(())
}

/// Arquivo que a mini tela "O que mudou" lê; fica na pasta do app até ser vista.
pub const WHATS_NEW: &str = "whats-new.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WhatsNew {
    pub version: String,
    #[serde(default)]
    pub previous_version: String,
    #[serde(default)]
    pub notes: String,
}

/// Guarda as notas da versão instalada para a próxima abertura visível (a tarefa diária roda sem janela).
fn anotar_novidades(app_dir: &Path, feed: &Feed) -> Result<()> {
    let anterior = std::fs::read(app_dir.join(MANIFEST))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|m| m["previous_version"].as_str().map(str::to_string))
        .unwrap_or_default();
    let novo = WhatsNew { version: feed.version.clone(), previous_version: anterior, notes: feed.notes.clone() };
    io(std::fs::write(app_dir.join(WHATS_NEW), serde_json::to_vec_pretty(&novo).map_err(|e| UpdateError::Io(e.to_string()))?))
}

/// Versão cujas notas o usuário já viu, guardada na pasta do usuário: a pasta do app (Program Files, criada pelo
/// SYSTEM) pode não deixar o usuário apagar `whats-new.json`.
pub fn seen_marker() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("Orion").join("whats-new-seen.txt"))
}

/// Novidade ainda não vista, se houver.
pub fn pending_whats_new(app_dir: &Path) -> Option<WhatsNew> {
    let news: WhatsNew = serde_json::from_slice(&std::fs::read(app_dir.join(WHATS_NEW)).ok()?).ok()?;
    let seen = seen_marker().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    (seen.trim() != news.version).then_some(news)
}

/// Marca como vista: grava a versão na pasta do usuário e, se der, apaga o arquivo da pasta do app.
pub fn dismiss_whats_new(app_dir: &Path) {
    if let Some(news) = std::fs::read(app_dir.join(WHATS_NEW)).ok().and_then(|b| serde_json::from_slice::<WhatsNew>(&b).ok()) {
        if let Some(path) = seen_marker() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, news.version);
        }
    }
    let _ = std::fs::remove_file(app_dir.join(WHATS_NEW));
}

/// Põe a nova versão e os novos hashes no manifesto, para a desinstalação continuar certa.
fn registrar_versao(app_dir: &Path, feed: &Feed) -> Result<()> {
    let path = app_dir.join(MANIFEST);
    let mut m: serde_json::Value = serde_json::from_slice(&io(std::fs::read(&path))?).map_err(|e| UpdateError::Io(e.to_string()))?;
    let anterior = m["version"].clone();
    m["previous_version"] = anterior;
    m["version"] = serde_json::json!(feed.version);
    if let Some(files) = m["files"].as_array_mut() {
        for f in &feed.files {
            match files.iter_mut().find(|x| x["name"] == f.name.as_str()) {
                Some(x) => x["sha256"] = serde_json::json!(f.sha256),
                None => files.push(serde_json::json!({"name": f.name, "sha256": f.sha256})),
            }
        }
    }
    io(std::fs::write(&path, serde_json::to_vec_pretty(&m).map_err(|e| UpdateError::Io(e.to_string()))?))
}

/// Devolve os arquivos `.bak` de uma atualização anterior. Devolve quantos voltaram.
pub fn rollback(app_dir: &Path) -> Result<usize> {
    let mut n = 0;
    for name in ALLOWED_FILES {
        let alvo = app_dir.join(name);
        let antigo = bak(&alvo);
        if antigo.is_file() {
            let _ = std::fs::remove_file(&alvo);
            io(std::fs::rename(&antigo, &alvo))?;
            n += 1;
        }
    }
    if n > 0 {
        // o manifesto deixa de descrever os arquivos de agora: refaz hash e versão pelo disco
        let path = app_dir.join(MANIFEST);
        let mut m: serde_json::Value = serde_json::from_slice(&io(std::fs::read(&path))?).map_err(|e| UpdateError::Io(e.to_string()))?;
        if let Some(files) = m["files"].as_array_mut() {
            for f in files.iter_mut() {
                if let Some(name) = f["name"].as_str() {
                    if let Ok(b) = std::fs::read(app_dir.join(name)) {
                        f["sha256"] = serde_json::json!(hex::encode(Sha256::digest(&b)));
                    }
                }
            }
        }
        if let Some(v) = m.get("previous_version").cloned().filter(|v| v.is_string()) {
            m["version"] = v;
        }
        io(std::fs::write(&path, serde_json::to_vec_pretty(&m).map_err(|e| UpdateError::Io(e.to_string()))?))?;
    }
    Ok(n)
}

/// `orion update --check [--apply]` e `--rollback` (a tarefa diária e a mão). `app_dir` é a pasta do
/// `orion.exe`; lê `update.json` da pasta de dados que o manifesto indica. Devolve o código de saída.
pub fn executar(args: &[String], app_dir: &Path) -> i32 {
    let data_dir = || -> Option<PathBuf> {
        let m: serde_json::Value = serde_json::from_slice(&std::fs::read(app_dir.join(MANIFEST)).ok()?).ok()?;
        Some(PathBuf::from(m["data_dir"].as_str()?))
    };
    if args.iter().any(|a| a == "--rollback") {
        return match rollback(app_dir) {
            Ok(n) => {
                println!("{n} arquivo(s) devolvido(s)");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        };
    }
    if args.iter().any(|a| a == "--apply-staged") {
        let code = match apply_staged(app_dir) {
            Ok(Some(v)) => {
                println!("atualizado para {v}");
                0
            }
            Ok(None) => {
                println!("nada guardado para instalar");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        };
        // a tarefa diária faz as duas coisas: instala o que ficou guardado e depois confere o feed
        if !args.iter().any(|a| a == "--check") {
            return code;
        }
    }
    if args.iter().any(|a| a == "--check") {
        let config: UpdateConfig = data_dir().and_then(|d| std::fs::read(d.join("update.json")).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        return match check(&config, app_dir, &HttpFetch, args.iter().any(|a| a == "--apply")) {
            Ok(Outcome::UpToDate) => {
                println!("em dia");
                0
            }
            Ok(Outcome::Available(v)) => {
                println!("versão {v} disponível (auto_update desligado)");
                0
            }
            Ok(Outcome::Updated(v)) => {
                println!("atualizado para {v}; reinicie o Orion");
                0
            }
            Ok(Outcome::Staged(v)) => {
                println!("versão {v} baixada e conferida; será instalada quando o Orion fechar");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        };
    }
    eprintln!("uso: orion update --check [--apply] | --apply-staged | --rollback");
    2
}

#[cfg(test)]
mod tests;

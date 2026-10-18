//! Resultados versionados em SQLite: IDs opacos, escopo e conteúdo sem caminhos arbitrários.

use base64::Engine as _;
use orion_store::Store;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::sync::{Arc, LazyLock};

pub const MAX_CONTENT: usize = 4 * 1024 * 1024;
pub const MAX_REQUEST: usize = 6 * 1024 * 1024;
pub const MAX_TOTAL: i64 = 256 * 1024 * 1024;
static LINGUAGEM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_+.-]*$").unwrap());
static ID32: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-f0-9]{32}$").unwrap());

/// (tipo MIME, extensão) por tipo de artefato.
pub fn mime_de(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "markdown" => ("text/markdown; charset=utf-8", ".md"),
        "image" => ("image/png", ".png"),
        _ => ("text/plain; charset=utf-8", ".txt"),
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ArtifactError(pub String);

fn err<T>(c: &str) -> Result<T, ArtifactError> {
    Err(ArtifactError(c.into()))
}

impl From<orion_store::StoreError> for ArtifactError {
    fn from(e: orion_store::StoreError) -> Self {
        ArtifactError(e.to_string())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub title: String,
    #[serde(default = "markdown")]
    pub kind: String,
    pub content: String,
    pub session_id: String,
    #[serde(default)]
    pub message_id: Option<i64>,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub expected_version: Option<i64>,
}

fn markdown() -> String {
    "markdown".into()
}

impl Payload {
    /// Validação do formato do pedido; `Err` vira 422 `artifact_request_invalid`.
    pub fn validar(&mut self) -> Result<(), ArtifactError> {
        let t = self.title.trim().to_string();
        let n = self.title.chars().count();
        if t.is_empty() || n > 160 || self.title.chars().any(|c| (c as u32) < 32 || c == '/' || c == '\\') {
            return err("artifact_request_invalid");
        }
        self.title = t;
        if !["text", "markdown", "code", "image"].contains(&self.kind.as_str())
            || self.content.is_empty()
            || self.content.len() > MAX_REQUEST
            || !ID32.is_match(&self.session_id)
            || self.message_id.is_some_and(|m| m < 1)
            || self.expected_version.is_some_and(|v| v < 1)
            || self.language.chars().count() > 48
            || !LINGUAGEM.is_match(&self.language)
        {
            return err("artifact_request_invalid");
        }
        Ok(())
    }
}

/// Conteúdo pronto para gravar: texto como está; imagem decodificada e regravada como PNG.
pub fn encode(p: &Payload) -> Result<Vec<u8>, ArtifactError> {
    let dados = if p.kind != "image" {
        p.content.as_bytes().to_vec()
    } else {
        let invalida = || ArtifactError("artifact_image_invalid".into());
        let raw = base64::engine::general_purpose::STANDARD.decode(p.content.as_bytes()).map_err(|_| invalida())?;
        if raw.len() > MAX_CONTENT {
            return err("artifact_size_limit");
        }
        let leitor = image::ImageReader::new(Cursor::new(&raw)).with_guessed_format().map_err(|_| invalida())?;
        if !matches!(leitor.format(), Some(image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::WebP)) {
            return Err(invalida());
        }
        let mut leitor = leitor;
        let mut limites = image::Limits::default();
        limites.max_alloc = Some(256 * 1024 * 1024);
        limites.max_image_width = Some(8192);
        limites.max_image_height = Some(8192);
        leitor.limits(limites);
        let img = leitor.decode().map_err(|_| invalida())?;
        if (img.width() as u64) * (img.height() as u64) > 16_000_000 {
            return Err(invalida());
        }
        let mut saida = Cursor::new(vec![]);
        image::DynamicImage::ImageRgba8(img.to_rgba8()).write_to(&mut saida, image::ImageFormat::Png).map_err(|_| invalida())?;
        saida.into_inner()
    };
    if dados.is_empty() || dados.len() > MAX_CONTENT {
        return err("artifact_size_limit");
    }
    Ok(dados)
}

pub struct Artifacts {
    pub memory: Arc<Store>,
}

type Linha = Map<String, Value>;

impl Artifacts {
    pub fn new(memory: Arc<Store>) -> Self {
        Artifacts { memory }
    }

    pub fn get(&self, id: &str, project_id: Option<&str>) -> Result<Linha, ArtifactError> {
        let rows = self.memory.query("SELECT * FROM artifacts WHERE id=? AND project_id IS ?", &[&id, &project_id])?;
        let Some(mut row) = rows.into_iter().next() else { return err("artifact_not_found") };
        let titulo = self.memory.get_session(row["session_id"].as_str().unwrap_or(""))?.and_then(|s| s.title);
        row.insert("session_title".into(), json!(titulo));
        Ok(row)
    }

    pub fn list(&self, project_id: Option<&str>, query: &str) -> Result<Vec<Linha>, ArtifactError> {
        Ok(self.memory.query(
            "SELECT a.*, (SELECT max(version) FROM artifact_versions WHERE artifact_id=a.id) AS version FROM artifacts a WHERE project_id IS ? AND instr(lower(title),lower(?))>0 ORDER BY updated_at DESC,id LIMIT 200",
            &[&project_id, &query],
        )?)
    }

    pub fn save(&self, p: &Payload, id: Option<&str>, project_id: Option<&str>) -> Result<Linha, ArtifactError> {
        let dados = encode(p)?;
        let Some(sessao) = self.memory.get_session(&p.session_id)?.filter(|s| s.project_id.as_deref() == project_id) else { return err("artifact_session_out_of_scope") };
        if sessao.read_only || sessao.archived {
            return err("artifact_session_read_only");
        }
        if let Some(pid) = project_id {
            let a = self.memory.query("SELECT archived FROM projects WHERE id=?", &[&pid])?;
            if a.first().is_none_or(|r| r["archived"].as_i64().unwrap_or(1) != 0) {
                return err("project_archived");
            }
        }
        let mut proveniencia: Option<String> = None;
        if let Some(mid) = p.message_id {
            let rows = self.memory.query("SELECT provenance FROM messages WHERE id=? AND session_id=?", &[&mid, &sessao.id])?;
            let Some(r) = rows.first() else { return err("artifact_message_out_of_scope") };
            proveniencia = r["provenance"].as_str().map(String::from);
        }
        let agora = self.memory.now();
        let artefato_id = id.map(String::from).unwrap_or_else(orion_store::uuid_hex);
        let digest = hex::encode(Sha256::digest(&dados));
        let antigo = match id {
            Some(i) => Some(self.get(i, project_id)?),
            None => None,
        };
        let versao: Result<i64, ArtifactError> = self.memory.tx(|db| {
            let versao;
            if let Some(old) = &antigo {
                if old["session_id"] != json!(sessao.id) || old["kind"] != json!(p.kind) {
                    return Ok(err("artifact_version_out_of_scope"));
                }
                let anterior: Option<i64> = db.query_row("SELECT max(version) FROM artifact_versions WHERE artifact_id=?", [&artefato_id], |r| r.get(0))?;
                if p.expected_version != anterior {
                    return Ok(err("artifact_version_conflict"));
                }
                versao = anterior.unwrap_or(0) + 1;
                if versao > 50 {
                    return Ok(err("artifact_version_limit"));
                }
                db.execute("UPDATE artifacts SET title=?,updated_at=? WHERE id=?", orion_store::rusqlite::params![p.title, agora, artefato_id])?;
            } else {
                let n: i64 = db.query_row("SELECT count(*) FROM artifacts", [], |r| r.get(0))?;
                if n >= 1000 {
                    return Ok(err("artifact_count_limit"));
                }
                versao = 1;
                db.execute(
                    "INSERT INTO artifacts VALUES(?,?,?,?,?,?,?,?)",
                    orion_store::rusqlite::params![artefato_id, project_id, sessao.id, p.title, p.kind, p.language, agora, agora],
                )?;
            }
            let total: i64 = db.query_row("SELECT coalesce(sum(length(content)),0) FROM artifact_versions", [], |r| r.get(0))?;
            if total + dados.len() as i64 > MAX_TOTAL {
                return Ok(err("artifact_storage_limit"));
            }
            db.execute(
                "INSERT INTO artifact_versions VALUES(?,?,?,?,?,?,?)",
                orion_store::rusqlite::params![artefato_id, versao, dados, digest, p.message_id, proveniencia, agora],
            )?;
            Ok(Ok(versao))
        })?;
        let versao = versao?;
        let mut r = self.get(&artefato_id, project_id)?;
        r.insert("version".into(), json!(versao));
        r.insert("digest".into(), json!(digest));
        Ok(r)
    }

    pub fn versions(&self, id: &str, project_id: Option<&str>) -> Result<Vec<Linha>, ArtifactError> {
        self.get(id, project_id)?;
        let rows = self.memory.query(
            "SELECT version,digest,message_id,provenance,created_at,length(content) AS bytes FROM artifact_versions WHERE artifact_id=? ORDER BY version DESC",
            &[&id],
        )?;
        Ok(rows.into_iter().map(|mut r| {
            let p = r.get("provenance").and_then(Value::as_str).and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(Value::Null);
            r.insert("provenance".into(), p);
            r
        }).collect())
    }

    /// (linha do artefato, versão com `content` em bytes).
    pub fn read(&self, id: &str, project_id: Option<&str>, version: Option<i64>) -> Result<(Linha, Linha, Vec<u8>), ArtifactError> {
        let row = self.get(id, project_id)?;
        let vs = self.memory.query(
            "SELECT version,digest,message_id,provenance,created_at FROM artifact_versions WHERE artifact_id=? AND (? IS NULL OR version=?) ORDER BY version DESC LIMIT 1",
            &[&id, &version, &version],
        )?;
        let Some(v) = vs.into_iter().next() else { return err("artifact_version_not_found") };
        let ver = v["version"].as_i64().unwrap_or(0);
        let conteudo: Vec<u8> = self.memory.conn().query_row("SELECT content FROM artifact_versions WHERE artifact_id=? AND version=?", orion_store::rusqlite::params![id, ver], |r| r.get(0)).map_err(orion_store::StoreError::from)?;
        if hex::encode(Sha256::digest(&conteudo)) != v["digest"].as_str().unwrap_or("") {
            return err("artifact_integrity_failed");
        }
        Ok((row, v, conteudo))
    }
}

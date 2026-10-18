//! Pacotes Agent Skills: frontmatter limitado, referências confinadas e zero execução.

use orion_policy::PathGuard;
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SkillError(pub String);

pub fn serr(c: &str) -> SkillError {
    SkillError(c.into())
}

type R<T> = Result<T, SkillError>;

static NOME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap());
static NAMESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,47}$").unwrap());
static ANCORA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)(?:^|[\s,\[\{])[&*][A-Za-z0-9_-]").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]\(([^)]+)\)").unwrap());
static REFER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?:references|scripts|assets)/[^\s`"<>]+"#).unwrap());
static ESQUEMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9+.\-]*:").unwrap());

fn guarda() -> PathGuard {
    PathGuard { protected_roots: vec![], safe_roots: vec![], system_roots: vec![] }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillHeader {
    pub name: String,
    pub description: String,
    pub license: Option<String>,
    pub compatibility: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub allowed_tools: Option<String>,
}

fn texto_opcional(m: &serde_yaml::Mapping, k: &str, max: usize) -> R<Option<String>> {
    match m.get(k) {
        None | Some(serde_yaml::Value::Null) => Ok(None),
        Some(serde_yaml::Value::String(s)) if s.chars().count() <= max => Ok(Some(s.clone())),
        _ => Err(serr("frontmatter_invalid")),
    }
}

fn sem_marcas(v: &serde_yaml::Value) -> bool {
    match v {
        serde_yaml::Value::Tagged(_) => false,
        serde_yaml::Value::Sequence(s) => s.iter().all(sem_marcas),
        serde_yaml::Value::Mapping(m) => m.iter().all(|(k, v)| sem_marcas(k) && sem_marcas(v)),
        _ => true,
    }
}

pub fn cabecalho(texto: &str) -> R<SkillHeader> {
    if ANCORA.is_match(texto) {
        return Err(serr("frontmatter_unsupported_yaml"));
    }
    let v: serde_yaml::Value = serde_yaml::from_str(texto).map_err(|_| serr("frontmatter_invalid"))?;
    if !sem_marcas(&v) {
        return Err(serr("frontmatter_unsupported_yaml"));
    }
    let m = v.as_mapping().ok_or_else(|| serr("frontmatter_invalid"))?;
    let permitidos = ["name", "description", "license", "compatibility", "metadata", "allowed-tools"];
    if m.keys().any(|k| k.as_str().map_or(true, |k| !permitidos.contains(&k))) {
        return Err(serr("frontmatter_invalid"));
    }
    let name = texto_opcional(m, "name", 64)?.filter(|n| !n.is_empty() && NOME.is_match(n)).ok_or_else(|| serr("frontmatter_invalid"))?;
    let description = texto_opcional(m, "description", 1024)?.filter(|d| !d.trim().is_empty()).ok_or_else(|| serr("frontmatter_invalid"))?;
    let mut metadata = BTreeMap::new();
    match m.get("metadata") {
        None | Some(serde_yaml::Value::Null) => {}
        Some(serde_yaml::Value::Mapping(mm)) if mm.len() <= 32 => {
            for (k, x) in mm {
                match (k.as_str(), x.as_str()) {
                    (Some(k), Some(x)) => {
                        metadata.insert(k.to_string(), x.to_string());
                    }
                    _ => return Err(serr("frontmatter_invalid")),
                }
            }
        }
        _ => return Err(serr("frontmatter_invalid")),
    }
    Ok(SkillHeader { name, description, license: texto_opcional(m, "license", 1024)?, compatibility: texto_opcional(m, "compatibility", 500)?, metadata, allowed_tools: texto_opcional(m, "allowed-tools", 2048)? })
}

pub fn safe_file(root: &Path, relative: &str, limit: u64) -> R<PathBuf> {
    if relative.contains('\\') || relative.contains(':') || relative.contains('\0') {
        return Err(serr("reference_out_of_scope"));
    }
    let rel = Path::new(relative);
    let partes: Vec<Component> = rel.components().collect();
    if partes.is_empty() || rel.is_absolute() || partes.iter().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(serr("reference_out_of_scope"));
    }
    if fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(serr("symlink_forbidden"));
    }
    let mut atual = root.to_path_buf();
    for p in &partes {
        atual.push(p);
        if fs::symlink_metadata(&atual).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(serr("symlink_forbidden"));
        }
    }
    let real = atual.canonicalize().map_err(|_| serr("reference_out_of_scope"))?;
    let raiz = root.canonicalize().map_err(|_| serr("reference_out_of_scope"))?;
    if !real.starts_with(&raiz) {
        return Err(serr("reference_out_of_scope"));
    }
    if guarda().check_read(&atual.to_string_lossy()).is_some() {
        return Err(serr("secret_reference_forbidden"));
    }
    let m = fs::metadata(&atual).map_err(|_| serr("file_missing_or_too_large"))?;
    if !m.is_file() || m.len() > limit {
        return Err(serr("file_missing_or_too_large"));
    }
    Ok(atual)
}

fn ler_linha(f: &mut fs::File, max: usize) -> std::io::Result<Vec<u8>> {
    let mut linha = vec![];
    let mut b = [0u8; 1];
    while linha.len() < max {
        if f.read(&mut b)? == 0 {
            break;
        }
        linha.push(b[0]);
        if b[0] == b'\n' {
            break;
        }
    }
    Ok(linha)
}

pub fn metadata(root: &Path, namespace: &str, origin: &str) -> R<Skill> {
    if !NAMESPACE.is_match(namespace) {
        return Err(serr("namespace_invalid"));
    }
    let path = safe_file(root, "SKILL.md", 256_000)?;
    // só o frontmatter para descoberta; nenhum corpo/script é carregado
    let mut f = fs::File::open(&path).map_err(|_| serr("frontmatter_invalid"))?;
    let io = |_| serr("frontmatter_invalid");
    if String::from_utf8_lossy(&ler_linha(&mut f, 4096).map_err(io)?).trim() != "---" {
        return Err(serr("frontmatter_missing"));
    }
    let (mut linhas, mut conta) = (vec![], 0usize);
    loop {
        let l = ler_linha(&mut f, 4096).map_err(io)?;
        conta += l.len();
        if l.is_empty() || conta > 16000 {
            return Err(serr("frontmatter_unclosed_or_too_large"));
        }
        if String::from_utf8_lossy(&l).trim() == "---" {
            break;
        }
        linhas.extend(l);
    }
    let offset = f.stream_position().map_err(io)?;
    let texto = String::from_utf8(linhas).map_err(|_| serr("frontmatter_invalid"))?;
    let header = cabecalho(&texto)?;
    if root.file_name().map(|n| n.to_string_lossy().to_string()).as_deref() != Some(header.name.as_str()) {
        return Err(serr("name_directory_mismatch"));
    }
    Ok(Skill { root: root.to_path_buf(), namespace: namespace.into(), origin: origin.into(), header, offset })
}

#[derive(Debug, Clone)]
pub struct SkillBody {
    pub text: String,
    pub references: Vec<String>,
    pub digest: String,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub root: PathBuf,
    pub namespace: String,
    pub origin: String,
    pub header: SkillHeader,
    pub offset: u64,
}

fn desescapar(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if let Some(x) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

impl Skill {
    pub fn id(&self) -> String {
        format!("{}:{}", self.namespace, self.header.name)
    }

    pub fn version(&self) -> String {
        self.header.metadata.get("version").cloned().unwrap_or_else(|| "unversioned".into()).chars().take(80).collect()
    }

    pub fn summary(&self) -> Value {
        json!({"id": self.id(), "name": self.header.name, "description": self.header.description, "origin": self.origin, "version": self.version(), "license": self.header.license})
    }

    pub fn load(&self) -> R<SkillBody> {
        // revalida o cabeçalho se o diretório local mudar desde a descoberta
        let atual = metadata(&self.root, &self.namespace, &self.origin)?;
        if atual.header != self.header {
            return Err(serr("skill_changed"));
        }
        let path = safe_file(&self.root, "SKILL.md", 256_000)?;
        let mut f = fs::File::open(&path).map_err(|_| serr("file_missing_or_too_large"))?;
        f.seek(SeekFrom::Start(atual.offset)).map_err(|_| serr("file_missing_or_too_large"))?;
        let mut raw = vec![];
        (&mut f).take(64001).read_to_end(&mut raw).map_err(|_| serr("file_missing_or_too_large"))?;
        if raw.len() > 64000 {
            return Err(serr("skill_body_too_large"));
        }
        let text = String::from_utf8(raw.clone()).map_err(|_| serr("skill_encoding_invalid"))?;
        let mut links: Vec<String> = LINK.captures_iter(&text).map(|c| c[1].to_string()).collect();
        links.extend(REFER.find_iter(&text).map(|m| m.as_str().to_string()));
        let mut references: Vec<String> = vec![];
        for link in links {
            let alvo = desescapar(link.trim().trim_matches(|c| c == '<' || c == '>').split('#').next().unwrap_or(""));
            if alvo.is_empty() {
                continue;
            }
            if alvo.starts_with("http://") || alvo.starts_with("https://") {
                continue; // informativo; não buscar rede implicitamente
            }
            if ESQUEMA.is_match(&alvo) || alvo.contains('?') {
                return Err(serr("reference_out_of_scope"));
            }
            let alvo = alvo.trim_end_matches(|c| c == ')' || c == '.' || c == ',').to_string();
            safe_file(&self.root, &alvo, 256_000)?;
            if !references.contains(&alvo) {
                references.push(alvo);
            }
        }
        Ok(SkillBody { text, references, digest: hex::encode(Sha256::digest(&raw)) })
    }

    pub fn reference(&self, relative: &str, limit: u64) -> R<String> {
        let path = safe_file(&self.root, relative, limit)?;
        let mut data = vec![];
        fs::File::open(&path).and_then(|f| f.take(limit + 1).read_to_end(&mut data)).map_err(|_| serr("file_missing_or_too_large"))?;
        if data.len() as u64 > limit {
            return Err(serr("reference_too_large"));
        }
        String::from_utf8(data).map_err(|_| serr("reference_encoding_invalid"))
    }
}

#[derive(Default, Clone)]
pub struct SkillIndex {
    pub skills: BTreeMap<String, Skill>,
}

impl SkillIndex {
    pub fn add(&mut self, skill: Skill) -> R<()> {
        let id = skill.id();
        if self.skills.contains_key(&id) {
            return Err(serr("skill_name_collision"));
        }
        self.skills.insert(id, skill);
        Ok(())
    }

    pub fn discover(&mut self, directory: &Path, namespace: &str, origin: &str) -> R<()> {
        let mut roots: Vec<PathBuf> = fs::read_dir(directory).map_err(|_| serr("source_invalid"))?.flatten().map(|e| e.path()).collect();
        roots.sort();
        if roots.len() > 128 {
            return Err(serr("too_many_skills"));
        }
        let mut pendentes = vec![];
        for r in roots.iter().filter(|r| r.join("SKILL.md").exists()) {
            pendentes.push(metadata(r, namespace, origin)?);
        }
        if pendentes.iter().any(|s| self.skills.contains_key(&s.id())) {
            return Err(serr("skill_name_collision"));
        }
        for s in pendentes {
            self.add(s)?;
        }
        Ok(())
    }

    pub fn summaries(&self) -> Vec<Value> {
        self.skills.values().map(Skill::summary).collect()
    }
}

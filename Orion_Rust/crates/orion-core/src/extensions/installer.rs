//! Instalação por snapshot/staging; nenhuma importação, hook ou dependência é executada.

use super::plugins::{parse_manifest, perr, relative_path, PluginError, PluginManifest, PluginStore};
use super::skills::{metadata, safe_file};
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use unicode_normalization::UnicodeNormalization;

pub const MAX_FILES: usize = 512;
pub const MAX_EXPANDED: usize = 16_000_000;
pub const MAX_FILE: usize = 2_000_000;
static RESERVADO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)").unwrap());
const PROIBIDAS: [&str; 4] = [".git", ".venv", "node_modules", "__pycache__"];

type R<T> = Result<T, PluginError>;

pub fn package_path(v: &str) -> R<&str> {
    relative_path(v)?;
    if v.len() > 1024 || v.contains('\0') {
        return Err(perr("bundle_path_invalid"));
    }
    if v.split('/').any(|p| RESERVADO.is_match(p) || p.ends_with('.') || p.ends_with(' ') || PROIBIDAS.contains(&p.to_lowercase().as_str())) {
        return Err(perr("bundle_path_invalid"));
    }
    Ok(v)
}

pub fn fingerprint(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut h = Sha256::new();
    for (rel, content) in files {
        h.update(rel.as_bytes());
        h.update([0u8]);
        h.update(Sha256::digest(content));
    }
    hex::encode(h.finalize())
}

/// `casefold` do Python: minúsculas após NFC (suficiente para detectar colisão de nome).
pub fn canonico(s: &str) -> String {
    s.nfc().collect::<String>().to_lowercase()
}

#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: PluginManifest,
    pub digest: String,
    pub files: BTreeMap<String, Vec<u8>>,
}

pub fn folder_snapshot(source: &Path) -> R<Package> {
    let meta = fs::symlink_metadata(source).map_err(|_| perr("bundle_source_invalid"))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(perr("bundle_source_invalid"));
    }
    let (mut files, mut nomes) = (BTreeMap::new(), HashSet::new());
    let (mut total, mut entradas) = (0usize, 0usize);
    let mut pilha = vec![source.to_path_buf()];
    let mut todos = vec![];
    while let Some(d) = pilha.pop() {
        for e in fs::read_dir(&d).map_err(|_| perr("bundle_source_invalid"))?.flatten() {
            entradas += 1;
            if entradas > MAX_FILES * 2 {
                return Err(perr("bundle_too_many_entries"));
            }
            let p = e.path();
            let m = fs::symlink_metadata(&p).map_err(|_| perr("bundle_source_invalid"))?;
            if m.file_type().is_symlink() {
                return Err(perr("bundle_symlink_forbidden"));
            }
            if m.is_dir() {
                pilha.push(p.clone());
            }
            todos.push((p, m));
        }
    }
    todos.sort_by(|a, b| a.0.cmp(&b.0));
    for (p, m) in todos {
        let rel = p.strip_prefix(source).map_err(|_| perr("bundle_path_invalid"))?.to_string_lossy().replace('\\', "/");
        package_path(&rel)?;
        if !nomes.insert(canonico(&rel)) {
            return Err(perr("bundle_name_collision"));
        }
        if m.is_dir() {
            continue;
        }
        if !m.is_file() || m.nlink() != 1 {
            return Err(perr("bundle_special_file_forbidden"));
        }
        safe_file(source, &rel, MAX_FILE as u64).map_err(|_| perr("bundle_file_forbidden"))?;
        let content = fs::read(&p).map_err(|_| perr("bundle_file_forbidden"))?;
        total += content.len();
        if content.len() > MAX_FILE || total > MAX_EXPANDED || files.len() >= MAX_FILES {
            return Err(perr("bundle_size_limit"));
        }
        files.insert(rel, content);
    }
    let manifest = files.get("manifest.json").ok_or_else(|| perr("manifest_missing"))?;
    let manifest = parse_manifest(manifest)?;
    let digest = fingerprint(&files);
    Ok(Package { manifest, digest, files })
}

pub fn validate_package(root: &Path, manifest: &PluginManifest) -> R<()> {
    let mut ids = HashSet::new();
    for rel in &manifest.skills {
        package_path(rel)?;
        let skill = metadata(&root.join(rel), &manifest.id, "plugin").map_err(|_| perr("bundle_reference_invalid"))?;
        skill.load().map_err(|_| perr("bundle_reference_invalid"))?;
        if !ids.insert(skill.id()) {
            return Err(perr("bundle_skill_collision"));
        }
    }
    for c in &manifest.mcp {
        if let Some(e) = &c.entrypoint {
            safe_file(root, package_path(e)?, MAX_FILE as u64).map_err(|_| perr("bundle_reference_invalid"))?;
        }
    }
    Ok(())
}

/// Só para staging/objects do próprio installer; nunca segue symlink na limpeza.
pub fn remove_tree(root: &Path) {
    let Ok(m) = fs::symlink_metadata(root) else { return };
    if m.file_type().is_symlink() {
        let _ = fs::remove_file(root);
        return;
    }
    let _ = fs::set_permissions(root, fs::Permissions::from_mode(0o700));
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            match fs::symlink_metadata(&p) {
                Ok(m) if m.file_type().is_symlink() => {
                    let _ = fs::remove_file(&p);
                }
                Ok(m) if m.is_dir() => remove_tree(&p),
                Ok(_) => {
                    let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o600));
                    let _ = fs::remove_file(&p);
                }
                Err(_) => {}
            }
        }
    }
    let _ = fs::remove_dir(root);
}

pub struct Installer {
    pub root: PathBuf,
    pub store: Arc<PluginStore>,
    staging: PathBuf,
    bundles: PathBuf,
    lock: Mutex<()>,
}

fn publico(row: Map<String, Value>) -> Map<String, Value> {
    row
}

impl Installer {
    pub fn new(root: &Path, store: Arc<PluginStore>) -> R<Self> {
        let (staging, bundles) = (root.join("staging"), root.join("bundles"));
        fs::create_dir_all(&staging).map_err(|_| perr("bundle_source_invalid"))?;
        fs::create_dir_all(&bundles).map_err(|_| perr("bundle_source_invalid"))?;
        Ok(Installer { root: root.to_path_buf(), store, staging, bundles, lock: Mutex::new(()) })
    }

    pub fn install_folder(&self, source: &Path, origin: &str, update: bool) -> R<Map<String, Value>> {
        let package = folder_snapshot(source)?;
        self.install_snapshot(&package, origin, update)
    }

    pub fn install_snapshot(&self, package: &Package, origin: &str, update: bool) -> R<Map<String, Value>> {
        if fingerprint(&package.files) != package.digest {
            return Err(perr("bundle_hash_invalid"));
        }
        let _g = self.lock.lock();
        // confinamento revalidado mesmo para snapshots vindos de outros transportes
        let mut total = 0usize;
        for (rel, data) in &package.files {
            package_path(rel)?;
            total += data.len();
            if data.len() > MAX_FILE || total > MAX_EXPANDED {
                return Err(perr("bundle_size_limit"));
            }
        }
        if package.files.len() > MAX_FILES {
            return Err(perr("bundle_size_limit"));
        }
        let target = self.bundles.join(&package.manifest.id).join(&package.digest);
        let rel_bundle = target.strip_prefix(&self.root).unwrap_or(&target).to_string_lossy().to_string();
        if target.exists() {
            if folder_snapshot(&target)?.digest != package.digest {
                return Err(perr("bundle_tampered"));
            }
            self.store.record(&package.manifest, &package.digest, origin, Some(&rel_bundle), update)?;
            return self.store.get(&package.manifest.id).map(publico);
        }
        let temporario = tempfile::Builder::new().prefix("install-").tempdir_in(&self.staging).map_err(|_| perr("bundle_source_invalid"))?;
        let staged = temporario.path().to_path_buf();
        let mut movido = false;
        let r: R<()> = (|| {
            for (rel, data) in &package.files {
                let p = staged.join(rel);
                if let Some(pai) = p.parent() {
                    fs::create_dir_all(pai).map_err(|_| perr("bundle_source_invalid"))?;
                }
                fs::write(&p, data).map_err(|_| perr("bundle_source_invalid"))?;
            }
            // valida o snapshot completo, sem executar nada
            let atual = folder_snapshot(&staged)?;
            if atual.digest != package.digest || atual.manifest != package.manifest {
                return Err(perr("bundle_hash_invalid"));
            }
            validate_package(&staged, &atual.manifest)?;
            fs::create_dir_all(target.parent().unwrap()).map_err(|_| perr("bundle_source_invalid"))?;
            proteger(&staged);
            // `rename` não atravessa sistemas de arquivos, mas staging e bundles estão sob a mesma raiz
            let destino_tmp = staged.clone();
            fs::rename(&destino_tmp, &target).map_err(|_| perr("bundle_source_invalid"))?;
            movido = true;
            let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o555));
            self.store.record(&package.manifest, &package.digest, origin, Some(&rel_bundle), update)?;
            Ok(())
        })();
        if let Err(e) = r {
            if movido {
                remove_tree(&target);
            }
            remove_tree(&staged);
            return Err(e);
        }
        std::mem::forget(temporario); // o diretório foi movido; nada a limpar
        self.store.get(&package.manifest.id).map(publico)
    }

    pub fn bundle(&self, id: &str, digest: &str) -> R<PathBuf> {
        let version = self.store.version(id, digest)?;
        let esperado = format!("bundles/{id}/{digest}");
        if version["bundle"].as_str() != Some(&esperado) {
            return Err(perr("bundle_path_invalid"));
        }
        let path = self.root.join(&esperado);
        let real = path.canonicalize().map_err(|_| perr("bundle_path_invalid"))?;
        if !real.starts_with(self.root.canonicalize().map_err(|_| perr("bundle_path_invalid"))?) {
            return Err(perr("bundle_path_invalid"));
        }
        if folder_snapshot(&path)?.digest != digest {
            return Err(perr("bundle_tampered"));
        }
        Ok(path)
    }

    pub fn plan_update(&self, id: &str, digest: &str) -> R<Value> {
        let current = self.store.get(id)?;
        let target = self.store.version(id, digest)?;
        let caps = |v: &Value| -> HashSet<String> { v["capabilities"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() };
        let (old, new) = (caps(&current["manifest"]), caps(&target["manifest"]));
        self.bundle(id, digest)?;
        let ord = |s: HashSet<String>| {
            let mut v: Vec<String> = s.into_iter().collect();
            v.sort();
            v
        };
        Ok(json!({"id": id, "current_digest": current["selected_digest"], "target_digest": digest, "version": target["version"],
                  "added": ord(new.difference(&old).cloned().collect()), "removed": ord(old.difference(&new).cloned().collect()),
                  "review_required": current["selected_digest"].as_str() != Some(digest)}))
    }

    pub fn change_version(&self, id: &str, digest: &str, reviewed_digest: &str, reviewed_capabilities: &HashSet<String>) -> R<Map<String, Value>> {
        let _g = self.lock.lock();
        self.bundle(id, digest)?;
        self.store.change_version(id, digest, reviewed_digest, reviewed_capabilities)
    }

    pub fn uninstall(&self, id: &str) -> R<()> {
        let _g = self.lock.lock();
        let versions = self.store.versions(id)?;
        // valida também os ancestrais antes da operação destrutiva
        for v in &versions {
            let esperado = format!("bundles/{id}/{}", v["digest"].as_str().unwrap_or(""));
            if v["bundle"].as_str() != Some(&esperado) {
                return Err(perr("bundle_path_invalid"));
            }
            let path = self.root.join(&esperado);
            let mut p = Some(path.as_path());
            while let Some(x) = p {
                if x == self.root.parent().unwrap_or(Path::new("/")) {
                    break;
                }
                if fs::symlink_metadata(x).is_ok_and(|m| m.file_type().is_symlink()) {
                    return Err(perr("bundle_path_invalid"));
                }
                p = x.parent();
            }
            if let Ok(real) = path.canonicalize() {
                if !real.starts_with(self.root.canonicalize().map_err(|_| perr("bundle_path_invalid"))?) {
                    return Err(perr("bundle_path_invalid"));
                }
            }
        }
        self.store.remove(id)?;
        for v in &versions {
            let esperado = format!("bundles/{id}/{}", v["digest"].as_str().unwrap_or(""));
            if v["bundle"].as_str() == Some(&esperado) {
                remove_tree(&self.root.join(&esperado));
            }
        }
        let _ = fs::remove_dir(self.bundles.join(id));
        Ok(())
    }
}

fn proteger(dir: &Path) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                proteger(&p);
                let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o555));
            } else {
                let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o444));
            }
        }
    }
}

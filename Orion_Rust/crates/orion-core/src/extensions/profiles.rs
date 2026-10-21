//! Pacotes mantidos no repositório (embutidos no binário), instalados somente por ação administrativa.

use super::installer::{fingerprint, package_path, Package, MAX_FILE};
use super::plugins::{parse_manifest, perr, PluginError};
use include_dir::{include_dir, Dir};
use serde_json::{json, Value};
use std::collections::BTreeMap;

static EMBUTIDOS: Dir = include_dir!("$CARGO_MANIFEST_DIR/bundled");

pub const IDS: [&str; 6] = ["orion-pesquisa", "orion-agenda", "orion-memoria-vault", "orion-arquivos", "orion-desenvolvimento", "orion-auditoria"];

fn coletar(dir: &Dir, base: &str, out: &mut BTreeMap<String, Vec<u8>>) -> Result<(), PluginError> {
    for f in dir.files() {
        let rel = f.path().to_string_lossy().replace('\\', "/");
        let rel = rel.strip_prefix(&format!("{base}/")).unwrap_or(&rel).to_string();
        package_path(&rel)?;
        if f.contents().len() > MAX_FILE {
            return Err(perr("bundle_size_limit"));
        }
        out.insert(rel, f.contents().to_vec());
    }
    for d in dir.dirs() {
        coletar(d, base, out)?;
    }
    Ok(())
}

pub fn profile(id: &str) -> Result<Package, PluginError> {
    let pasta = IDS.contains(&id).then(|| id.strip_prefix("orion-")).flatten().and_then(|n| EMBUTIDOS.get_dir(n)).ok_or_else(|| perr("plugin_profile_not_found"))?;
    let base = pasta.path().to_string_lossy().to_string();
    let mut files = BTreeMap::new();
    coletar(pasta, &base, &mut files)?;
    let manifest = files.get("manifest.json").ok_or_else(|| perr("manifest_missing"))?;
    let manifest = parse_manifest(manifest)?;
    let digest = fingerprint(&files);
    Ok(Package { manifest, digest, files })
}

pub fn profiles() -> Result<Vec<Value>, PluginError> {
    IDS.iter()
        .map(|id| {
            let p = profile(id)?;
            let m = &p.manifest;
            Ok(json!({"id": m.id, "name": m.name, "version": m.version, "description": m.description, "license": m.license, "capabilities": m.capabilities, "digest": p.digest}))
        })
        .collect()
}

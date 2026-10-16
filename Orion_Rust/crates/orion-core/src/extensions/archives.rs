//! ZIP de distribuição limitado; sem extractall, links, ZIP64 ou execução.

use super::installer::{canonico, fingerprint, package_path, Installer, Package, MAX_EXPANDED, MAX_FILE, MAX_FILES};
use super::plugins::{parse_manifest, perr, PluginError};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read};
use std::path::Path;

pub const MAX_ARCHIVE: usize = 16_000_000;

type R<T> = Result<T, PluginError>;

fn u16le(d: &[u8], o: usize) -> usize {
    u16::from_le_bytes([d[o], d[o + 1]]) as usize
}
fn u32le(d: &[u8], o: usize) -> usize {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize
}

/// Limita o diretório central antes de a biblioteca criar milhares de entradas.
fn check_directory(data: &[u8]) -> R<()> {
    let inicio = data.len().saturating_sub(65557);
    let offset = data[inicio..].windows(4).rposition(|w| w == b"PK\x05\x06").map(|p| p + inicio).ok_or_else(|| perr("archive_corrupt"))?;
    if offset + 22 > data.len() {
        return Err(perr("archive_corrupt"));
    }
    let (disk, directory_disk, on_disk, count) = (u16le(data, offset + 4), u16le(data, offset + 6), u16le(data, offset + 8), u16le(data, offset + 10));
    let (size, start, comment) = (u32le(data, offset + 12), u32le(data, offset + 16), u16le(data, offset + 20));
    if disk != 0 || directory_disk != 0 || on_disk != count || count > MAX_FILES * 2 || start + size != offset || offset + 22 + comment != data.len() {
        return Err(perr("archive_format_or_size_unsupported"));
    }
    let (mut cursor, mut entries) = (start, 0usize);
    while cursor < offset {
        if cursor + 46 > offset || &data[cursor..cursor + 4] != b"PK\x01\x02" {
            return Err(perr("archive_corrupt"));
        }
        let (nome, extra, nota) = (u16le(data, cursor + 28), u16le(data, cursor + 30), u16le(data, cursor + 32));
        if nome > 1024 || extra > 4096 || nota > 1024 {
            return Err(perr("archive_metadata_limit"));
        }
        cursor += 46 + nome + extra + nota;
        entries += 1;
        if entries > MAX_FILES * 2 {
            return Err(perr("archive_entry_limit"));
        }
    }
    if cursor != offset || entries != count {
        return Err(perr("archive_corrupt"));
    }
    Ok(())
}

/// Os arquivos do ZIP, já verificados (a importação E8.3 converte a partir daqui).
pub fn archive_files(data: &[u8], filename: &str) -> R<BTreeMap<String, Vec<u8>>> {
    if !filename.to_lowercase().ends_with(".zip") {
        return Err(perr("archive_extension_unsupported"));
    }
    if data.len() > MAX_ARCHIVE {
        return Err(perr("archive_size_limit"));
    }
    check_directory(data)?;
    let corrupto = |_| perr("archive_corrupt");
    let mut arq = zip::ZipArchive::new(Cursor::new(data)).map_err(corrupto)?;
    let (mut files, mut arquivos_canon, mut nomes) = (BTreeMap::new(), HashSet::new(), HashSet::new());
    let mut total = 0usize;
    for i in 0..arq.len() {
        let mut f = arq.by_index(i).map_err(corrupto)?;
        let bruto = f.name_raw().to_vec();
        if bruto.contains(&0) || f.encrypted() {
            return Err(perr("archive_encrypted_or_invalid"));
        }
        let nome_orig = String::from_utf8(bruto).map_err(|_| perr("archive_corrupt"))?;
        let dir = nome_orig.ends_with('/');
        let nome = package_path(if dir { nome_orig.trim_end_matches('/') } else { &nome_orig })?.to_string();
        let canon = canonico(&nome);
        if !nomes.insert(canon.clone()) {
            return Err(perr("archive_name_collision"));
        }
        let modo = f.unix_mode().map(|m| m & 0o170000).unwrap_or(0);
        if ![0, 0o100000, 0o040000].contains(&modo) {
            return Err(perr("archive_special_file_forbidden"));
        }
        if !matches!(f.compression(), zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated) {
            return Err(perr("archive_compression_unsupported"));
        }
        if dir {
            if f.size() != 0 {
                return Err(perr("archive_directory_data"));
            }
            continue;
        }
        if modo == 0o040000 {
            return Err(perr("archive_entry_invalid"));
        }
        arquivos_canon.insert(canon);
        if f.size() as usize > MAX_FILE || f.size() > 200 * f.compressed_size().max(1) || files.len() >= MAX_FILES {
            return Err(perr("archive_expansion_limit"));
        }
        total += f.size() as usize;
        if total > MAX_EXPANDED {
            return Err(perr("archive_expansion_limit"));
        }
        let mut content = vec![];
        (&mut f).take(MAX_FILE as u64 + 1).read_to_end(&mut content).map_err(|_| perr("archive_corrupt"))?;
        if content.len() as u64 != f.size() {
            return Err(perr("archive_size_mismatch"));
        }
        files.insert(nome, content);
    }
    for n in &nomes {
        let partes: Vec<&str> = n.split('/').collect();
        if (1..partes.len()).any(|i| arquivos_canon.contains(&partes[..i].join("/"))) {
            return Err(perr("archive_file_directory_collision"));
        }
    }
    Ok(files)
}

pub fn archive_snapshot(data: &[u8], filename: &str) -> R<Package> {
    let files = archive_files(data, filename)?;
    let manifest = files.get("manifest.json").ok_or_else(|| perr("manifest_missing"))?;
    let manifest = parse_manifest(manifest)?;
    let digest = fingerprint(&files);
    Ok(Package { manifest, digest, files })
}

pub fn install_archive(installer: &Installer, data: &[u8], filename: &str, update: bool) -> R<Map<String, Value>> {
    // exatamente o mesmo validador/staging e publicação da pasta local
    installer.install_snapshot(&archive_snapshot(data, filename)?, "archive", update)
}

pub fn archive_file(installer: &Installer, path: &Path, update: bool) -> R<Map<String, Value>> {
    let m = std::fs::symlink_metadata(path).map_err(|_| perr("archive_source_invalid"))?;
    if m.file_type().is_symlink() || !m.is_file() || m.len() as usize > MAX_ARCHIVE {
        return Err(perr("archive_source_invalid"));
    }
    let data = std::fs::read(path).map_err(|_| perr("archive_source_invalid"))?;
    install_archive(installer, &data, &path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), update)
}

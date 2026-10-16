//! Arquivos do `orion-desktop`: `escrever_arquivo` e `organizar_pasta`.
//!
//! A política cuida do onde (REGRAS.md: código, persona e política imutáveis para as
//! ferramentas); aqui só ficam os limites que a ferramenta impõe a si mesma.

use super::registry::{obj, Tool};
use crate::util::expanduser;
use serde_json::{json, Value};
use std::io::Write;
use std::path::Path;

/// Bytes por chamada.
pub const MAX_ESCRITA: usize = 5 * 1024 * 1024;
pub const CRITERIOS: [&str; 3] = ["tipo", "data", "tamanho"];

fn tipo(ext: &str) -> &'static str {
    match ext {
        ".pdf" | ".doc" | ".docx" | ".odt" => "Documentos",
        ".xls" | ".xlsx" | ".ods" => "Planilhas",
        ".ppt" | ".pptx" => "Apresentacoes",
        ".jpg" | ".jpeg" | ".png" | ".gif" | ".bmp" | ".webp" | ".svg" | ".heic" => "Imagens",
        ".mp4" | ".mkv" | ".avi" | ".mov" => "Videos",
        ".mp3" | ".wav" | ".flac" | ".m4a" => "Audio",
        ".py" | ".js" | ".ts" | ".rs" | ".go" | ".java" | ".cpp" | ".c" | ".cs" => "Codigo",
        ".zip" | ".rar" | ".7z" | ".tar" | ".gz" => "Arquivados",
        ".txt" | ".md" => "Texto",
        ".csv" | ".json" | ".xml" => "Dados",
        _ => "Outros",
    }
}

fn destino(arquivo: &Path, criterio: &str) -> std::io::Result<String> {
    let meta = std::fs::metadata(arquivo)?;
    Ok(match criterio {
        "tipo" => tipo(&crate::resultados::extensao(arquivo)).to_string(),
        "data" => {
            let m = meta.modified()?.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            crate::util::local(m).format("%Y-%m").to_string()
        }
        _ => {
            let t = meta.len();
            if t < 1024 * 1024 {
                "Pequenos_menos1MB".into()
            } else if t < 100 * 1024 * 1024 {
                "Medios_1a100MB".into()
            } else {
                "Grandes_mais100MB".into()
            }
        }
    })
}

pub fn fs_tools() -> Vec<Tool> {
    vec![
        Tool::sync(
            "escrever_arquivo",
            "Grava texto num arquivo ('w' sobrescreve, 'a' acrescenta). Fora das pastas seguras (Documents, Downloads, Desktop) ou em arquivo sensível, pede confirmação do Antônio.",
            obj(json!({"path": {"type": "string"}, "conteudo": {"type": "string"}, "modo": {"type": "string", "enum": ["w", "a"]}}), &["path", "conteudo"]),
            |a| {
                let modo = a.s_or("modo", "w");
                if modo != "w" && modo != "a" {
                    return Ok(json!({"erro": "modo inválido: use 'w' (sobrescrever) ou 'a' (acrescentar)"}));
                }
                let dados = a.s("conteudo")?.into_bytes();
                if dados.len() > MAX_ESCRITA {
                    return Ok(json!({"erro": format!("conteúdo de {} bytes passa do limite de {MAX_ESCRITA}", dados.len())}));
                }
                let alvo = expanduser(&a.s("path")?);
                if alvo.is_dir() {
                    return Ok(json!({"erro": format!("é uma pasta, não um arquivo: {}", alvo.display())}));
                }
                if let Some(p) = alvo.parent() {
                    std::fs::create_dir_all(p)?;
                }
                let existia = alvo.exists();
                let mut f = std::fs::OpenOptions::new().create(true).write(true).append(modo == "a").truncate(modo == "w").open(&alvo)?;
                f.write_all(&dados)?;
                Ok(json!({"ok": true, "path": alvo.display().to_string(), "bytes_escritos": dados.len(), "substituiu": existia && modo == "w"}))
            },
        ),
        Tool::sync(
            "organizar_pasta",
            "Move os arquivos de uma pasta para subpastas por tipo, data ou tamanho. Use simular=true para ver o que seria movido sem mexer em nada.",
            obj(json!({"path": {"type": "string"}, "criterio": {"type": "string", "enum": CRITERIOS}, "simular": {"type": "boolean"}}), &["path"]),
            |a| {
                let criterio = a.s_or("criterio", "tipo");
                if !CRITERIOS.contains(&criterio.as_str()) {
                    return Ok(json!({"erro": format!("critério inválido: {criterio}. Use: {}", CRITERIOS.join(", "))}));
                }
                let simular = a.b_or("simular", false);
                let pasta = expanduser(&a.s("path")?);
                if !pasta.is_dir() {
                    return Ok(json!({"erro": format!("não é uma pasta: {}", pasta.display())}));
                }
                let mut arquivos: Vec<std::path::PathBuf> = std::fs::read_dir(&pasta)?.flatten().map(|e| e.path()).collect();
                arquivos.sort();
                let mut movidos: Vec<Value> = vec![];
                for arq in arquivos {
                    let nome = arq.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let link = std::fs::symlink_metadata(&arq).map(|m| m.file_type().is_symlink()).unwrap_or(true);
                    // só arquivos comuns visíveis; link e oculto ficam onde estão
                    if !arq.is_file() || link || nome.starts_with('.') {
                        continue;
                    }
                    let sub = destino(&arq, &criterio)?;
                    let mut alvo = pasta.join(&sub).join(&nome);
                    if alvo.exists() {
                        let stem = arq.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        let ext = arq.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                        alvo = alvo.with_file_name(format!("{stem}_{}{ext}", crate::util::agora() as i64));
                    }
                    if !simular {
                        std::fs::create_dir_all(pasta.join(&sub))?;
                        if std::fs::rename(&arq, &alvo).is_err() {
                            std::fs::copy(&arq, &alvo)?;
                            std::fs::remove_file(&arq)?;
                        }
                    }
                    let final_ = alvo.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    movidos.push(json!({"de": nome, "para": format!("{sub}/{final_}")}));
                }
                Ok(json!({"ok": true, "criterio": criterio, "simulado": simular, "movidos": movidos.len(), "detalhes": movidos}))
            },
        ),
    ]
}

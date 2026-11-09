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

/// F04.26: se o arquivo gravado é JSON, YAML ou TOML e não é válido, devolve o erro (com linha e coluna
/// quando o formato informa). O arquivo já foi gravado como o modelo pediu; o aviso volta ao modelo para ele
/// corrigir regravando, o que passa pela política como qualquer escrita, e o Diff mostra o que mudou.
pub(crate) fn erro_de_sintaxe(path: &Path, texto: &str) -> Option<String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "json" => serde_json::from_str::<Value>(texto).err().map(|e| format!("JSON inválido: {e}")),
        "yaml" | "yml" => {
            use serde::Deserialize;
            // vários documentos separados por `---` são válidos
            for doc in serde_yaml::Deserializer::from_str(texto) {
                if let Err(e) = serde_yaml::Value::deserialize(doc) {
                    return Some(format!("YAML inválido: {e}"));
                }
            }
            None
        }
        "toml" => toml::from_str::<toml::Table>(texto).err().map(|e| format!("TOML inválido: {e}")),
        _ => None,
    }
}
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
                drop(f);
                // o que vale é o arquivo inteiro: em 'a' (acrescentar) lê de volta
                let texto = if modo == "w" { String::from_utf8(dados.clone()).ok() } else { std::fs::read_to_string(&alvo).ok() };
                let mut resposta = json!({"ok": true, "path": alvo.display().to_string(), "bytes_escritos": dados.len(), "substituiu": existia && modo == "w"});
                if let Some(erro) = texto.as_deref().and_then(|t| erro_de_sintaxe(&alvo, t)) {
                    resposta["aviso_sintaxe"] = json!(erro);
                    resposta["corrija"] = json!("O arquivo foi gravado, mas tem erro de sintaxe. Corrija o conteúdo e grave de novo; o que mudou aparece no Diff.");
                }
                Ok(resposta)
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

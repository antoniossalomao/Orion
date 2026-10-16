//! `orion-desktop` v0: o mínimo para agir no computador, sempre atrás da política.
//!
//! `executar_comando` (execução: só leitura provada roda direto), `ler_arquivo` e
//! `listar_arquivos` (leitura; segredo e chave pedem confirmação). Desligado por padrão
//! (`ORION_DESKTOP_TOOLS`). O shell roda sem herdar `ORION_*` nem variável com nome de segredo.

use super::registry::{obj, Tool};
use crate::util::{cortar, expanduser, home, rodar, which, ErroProcesso};
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

/// Bytes lidos de um arquivo antes de decodificar.
pub const MAX_LEITURA: u64 = 1_000_000;

/// Linha de comando para rodar `cmd` no shell do sistema; `None` se não há shell.
pub fn argv_do_shell(cmd: &str, windows: bool) -> Option<Vec<String>> {
    if windows {
        let exe = which("pwsh").or_else(|| which("powershell"))?;
        return Some(vec![exe.to_string_lossy().into(), "-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), cmd.into()]);
    }
    let exe = which("sh")?;
    Some(vec![exe.to_string_lossy().into(), "-c".into(), cmd.into()])
}

pub fn desktop_tools(timeout_s: f64, max_chars: usize) -> Vec<Tool> {
    vec![
        Tool::sync(
            "executar_comando",
            "Executa um comando no shell do computador (PowerShell no Windows, sh no macOS/Linux). Só leitura provada roda direto; qualquer outro comando pede aprovação do Antônio.",
            obj(json!({"cmd": {"type": "string"}, "pasta": {"type": "string", "description": "pasta de trabalho (padrão: home)"}}), &["cmd"]),
            move |a| {
                let pasta = a.s_or("pasta", "");
                let cwd = if pasta.trim().is_empty() { home() } else { expanduser(&pasta) };
                if !cwd.is_dir() {
                    return Ok(json!({"erro": format!("pasta de trabalho inexistente: {}", cwd.display())}));
                }
                let Some(argv) = argv_do_shell(&a.s("cmd")?, cfg!(windows)) else {
                    return Ok(json!({"erro": "nenhum shell encontrado (PowerShell no Windows, sh no macOS/Linux)"}));
                };
                // argv fixo (shell + cmd); a política já decidiu se `cmd` roda sem confirmação
                // (REGRAS.md: shell só roda sem confirmação o que é leitura provada)
                match rodar(&argv, Some(&cwd), Duration::from_secs_f64(timeout_s), None) {
                    Err(ErroProcesso::Tempo) => Ok(json!({"erro": format!("tempo esgotado ({timeout_s:.0}s); o comando foi encerrado")})),
                    Err(ErroProcesso::Io(e)) => Ok(json!({"erro": format!("não foi possível executar: {:?}", e.kind())})),
                    Ok(r) => Ok(json!({
                        "codigo": r.codigo,
                        "saida": cortar(&String::from_utf8_lossy(&r.stdout), max_chars),
                        "erro": cortar(&String::from_utf8_lossy(&r.stderr), max_chars / 2),
                    })),
                }
            },
        ),
        Tool::sync(
            "ler_arquivo",
            "Lê um arquivo de texto (até alguns milhares de caracteres). Segredos e chaves (.env, .ssh, *.pem...) pedem confirmação do Antônio.",
            obj(json!({"path": {"type": "string"}, "max_chars_lidos": {"type": "integer"}}), &["path"]),
            move |a| {
                let alvo = expanduser(&a.s("path")?);
                if !alvo.is_file() {
                    return Ok(json!({"erro": format!("não é um arquivo: {}", alvo.display())}));
                }
                let pedido = a.i_or("max_chars_lidos", 0)?;
                let limite = (if pedido == 0 { max_chars as i64 } else { pedido }).clamp(1, max_chars as i64) as usize;
                let mut bruto = vec![];
                std::fs::File::open(&alvo)?.take(MAX_LEITURA).read_to_end(&mut bruto)?;
                if bruto[..bruto.len().min(4096)].contains(&0) {
                    return Ok(json!({"erro": "arquivo binário: use uma ferramenta de documento"}));
                }
                let texto = String::from_utf8_lossy(&bruto).into_owned();
                let tamanho = std::fs::metadata(&alvo)?.len();
                Ok(json!({
                    "path": alvo.display().to_string(),
                    "conteudo": cortar(&texto, limite),
                    "truncado": texto.chars().count() > limite || tamanho > MAX_LEITURA,
                }))
            },
        ),
        Tool::sync(
            "listar_arquivos",
            "Lista os arquivos e pastas de uma pasta (nome, tipo, tamanho).",
            obj(json!({"path": {"type": "string"}, "limite": {"type": "integer"}}), &["path"]),
            move |a| {
                let alvo = expanduser(&a.s("path")?);
                if !alvo.is_dir() {
                    return Ok(json!({"erro": format!("não é uma pasta: {}", alvo.display())}));
                }
                let teto = a.i_or("limite", 200)?.clamp(1, 500) as usize;
                let mut entradas: Vec<std::path::PathBuf> = std::fs::read_dir(&alvo)?.flatten().map(|e| e.path()).collect();
                entradas.sort_by_key(|p| (!p.is_dir(), p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()));
                let mut itens: Vec<Value> = vec![];
                for e in entradas {
                    let pasta = e.is_dir();
                    let tamanho = if pasta { None } else { std::fs::metadata(&e).ok().map(|m| m.len()) };
                    itens.push(json!({"nome": e.file_name().map(|n| n.to_string_lossy().into_owned()), "tipo": if pasta { "pasta" } else { "arquivo" }, "bytes": tamanho}));
                    if itens.len() >= teto {
                        break;
                    }
                }
                let truncado = itens.len() >= teto;
                Ok(json!({"path": alvo.display().to_string(), "itens": itens, "truncado": truncado}))
            },
        ),
    ]
}

//! "Posso desligar?": confere o que ficaria pela metade se o computador desligasse agora (E9.5).
//!
//! Só lê: repositórios em `ORION_REPOS` com mudança não commitada ou commit não enviado, tarefa em
//! segundo plano rodando, backup do dia e downloads em andamento. Nada é corrigido aqui.

use crate::downloads::PARCIAIS;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(raiz: &Path, args: &[&str]) -> Option<String> {
    let o = Command::new("git").arg("-C").arg(raiz).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

pub fn conferir_repo(raiz: &Path) -> Value {
    let nome = raiz.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| raiz.display().to_string());
    let id = format!("repo:{nome}");
    if !raiz.join(".git").exists() {
        return json!({"id": id, "ok": true, "texto": format!("{nome}: não é um repositório Git")});
    }
    let Some(sujo) = git(raiz, &["status", "--porcelain"]) else {
        return json!({"id": id, "ok": false, "texto": format!("{nome}: não consegui consultar o Git")});
    };
    // commits que não estão em nenhum remoto (cobre branch sem upstream também)
    let local = git(raiz, &["log", "--branches", "--not", "--remotes", "--oneline"]).unwrap_or_default();
    let conta = |s: &str| s.lines().filter(|l| !l.trim().is_empty()).count();
    let (mudancas, nao_enviados) = (conta(&sujo), conta(&local));
    let mut partes = vec![];
    if mudancas > 0 {
        partes.push(format!("{mudancas} arquivo(s) com mudança não commitada"));
    }
    if nao_enviados > 0 {
        partes.push(format!("{nao_enviados} commit(s) não enviado(s)"));
    }
    if partes.is_empty() {
        json!({"id": id, "ok": true, "texto": format!("{nome}: tudo commitado e enviado")})
    } else {
        json!({"id": id, "ok": false, "texto": format!("{nome}: {}", partes.join(" e "))})
    }
}

pub fn conferir_downloads(pasta: &Path) -> Value {
    let mut vivos: Vec<String> = std::fs::read_dir(pasta)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| PARCIAIS.iter().any(|p| n.to_lowercase().ends_with(p)))
                .collect()
        })
        .unwrap_or_default();
    vivos.sort();
    if vivos.is_empty() {
        json!({"id": "downloads", "ok": true, "texto": "nenhum download em andamento"})
    } else {
        json!({"id": "downloads", "ok": false, "texto": format!("{} download(s) em andamento: {}", vivos.len(), vivos.iter().take(3).cloned().collect::<Vec<_>>().join(", "))})
    }
}

pub fn conferir_backup(pasta: Option<&Path>, agora: f64) -> Value {
    let Some(pasta) = pasta else { return json!({"id": "backup", "ok": true, "texto": "backup não configurado"}) };
    let hoje = crate::util::local(agora).format("%Y%m%d").to_string();
    let prefixo = format!("orion-{hoje}-");
    let feito = std::fs::read_dir(pasta).map(|rd| rd.flatten().any(|e| { let n = e.file_name().to_string_lossy().into_owned(); n.starts_with(&prefixo) && n.ends_with(".db") })).unwrap_or(false);
    if feito {
        json!({"id": "backup", "ok": true, "texto": "backup de hoje feito"})
    } else {
        json!({"id": "backup", "ok": false, "texto": "o backup de hoje ainda não foi feito"})
    }
}

pub fn conferir_tarefas(tarefas: &[Value]) -> Value {
    match tarefas.iter().find(|t| t["status"] == "rodando") {
        Some(t) => json!({"id": "tarefa_bg", "ok": false, "texto": format!("tarefa em segundo plano: {}", t["objective"].as_str().unwrap_or("").chars().take(60).collect::<String>())}),
        None => json!({"id": "tarefa_bg", "ok": true, "texto": "nenhuma tarefa em segundo plano"}),
    }
}

pub fn posso_desligar(repos: &[PathBuf], backup_dir: Option<&Path>, downloads_dir: &Path, tarefas: &[Value]) -> Value {
    let mut itens: Vec<Value> = repos.iter().map(|r| conferir_repo(r)).collect();
    itens.push(conferir_tarefas(tarefas));
    itens.push(conferir_backup(backup_dir, crate::util::agora()));
    itens.push(conferir_downloads(downloads_dir));
    let pendentes = itens.iter().filter(|i| i["ok"] != json!(true)).count();
    json!({"pode": pendentes == 0, "pendentes": pendentes, "itens": itens})
}

pub fn texto(r: &Value) -> String {
    let cabeca = if r["pode"] == json!(true) { "✅ Pode desligar.".to_string() } else { format!("⚠️ Ainda não: {} pendência(s).", r["pendentes"]) };
    let mut linhas = vec![cabeca];
    for i in r["itens"].as_array().into_iter().flatten() {
        linhas.push(format!("{} {}", if i["ok"] == json!(true) { "✅" } else { "⚠️" }, i["texto"].as_str().unwrap_or("")));
    }
    linhas.join("\n")
}

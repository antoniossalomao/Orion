//! `orion-updater --check [--apply]` e `--rollback`. Roda na tarefa agendada diária (SYSTEM) e à
//! mão. Lê `update.json` da pasta de dados e o manifesto da pasta do app (a pasta onde está este .exe).

use orion_updater::{check, rollback, HttpFetch, Outcome, UpdateConfig};
use std::path::PathBuf;

fn data_dir(app_dir: &std::path::Path) -> Option<PathBuf> {
    let m: serde_json::Value = serde_json::from_slice(&std::fs::read(app_dir.join(orion_updater::MANIFEST)).ok()?).ok()?;
    Some(PathBuf::from(m["data_dir"].as_str()?))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let app_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default();
    let code = if args.iter().any(|a| a == "--rollback") {
        match rollback(&app_dir) {
            Ok(n) => {
                println!("{n} arquivo(s) devolvido(s)");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    } else if args.iter().any(|a| a == "--check") {
        let config: UpdateConfig = data_dir(&app_dir)
            .and_then(|d| std::fs::read(d.join("update.json")).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        match check(&config, &app_dir, &HttpFetch, args.iter().any(|a| a == "--apply")) {
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
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    } else {
        eprintln!("uso: orion-updater --check [--apply] | --rollback");
        2
    };
    std::process::exit(code);
}

//! Comandos de automação e MCP: `mcp-servidor`, `posso-desligar`, `downloads` e `rotina`.

use orion_core::config::{project_root, Settings};
use orion_core::{desligar, downloads, mcp_servidor, rotinas};
use orion_policy::{PathGuard, PolicyEngine};
use orion_store::Store;
use std::io::BufRead;
use std::sync::Arc;

fn perguntar(p: &str) -> bool {
    eprint!("{p}");
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    s.trim().eq_ignore_ascii_case("s")
}

/// O Orion como servidor MCP só de leitura (E7.7). Em modo servidor, stdout é do protocolo: tudo o
/// que for mensagem humana sai em stderr.
pub fn mcp_servidor(s: &Settings, token: bool, revogar: bool) -> anyhow::Result<i32> {
    let store = Arc::new(Store::open(s.db_path())?);
    if token {
        let t = mcp_servidor::gerar_token(&store)?;
        let e = mcp_servidor::ENV_TOKEN;
        println!(
            "Token gerado (aparece só agora; o anterior deixou de valer):\n\n  {t}\n\nClaude Code:\n  claude mcp add orion -e {e}={t} -- orion mcp-servidor\nCodex (~/.codex/config.toml):\n  [mcp_servers.orion]\n  command = \"orion\"\n  args = [\"mcp-servidor\"]\n  env = {{ {e} = \"{t}\" }}"
        );
        return Ok(0);
    }
    if revogar {
        println!("{}", if mcp_servidor::revogar(&store)? { "token revogado" } else { "não havia token" });
        return Ok(0);
    }
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    Ok(rt.block_on(mcp_servidor::servir(store, std::env::var(mcp_servidor::ENV_TOKEN).ok(), Some(s.effective_skills_dir()))))
}

/// O que ficaria pela metade se o computador desligasse agora (só lê).
pub fn posso_desligar(s: &Settings) -> anyhow::Result<i32> {
    let store = Store::open(s.db_path())?;
    let rodando: Vec<serde_json::Value> = store
        .query("SELECT objective FROM background_tasks WHERE status='rodando'", &[])?
        .iter()
        .map(|r| serde_json::json!({"status": "rodando", "objective": r["objective"]}))
        .collect();
    let r = desligar::posso_desligar(&s.repos, Some(&s.effective_backup_dir()), &downloads::dirs_home().join("Downloads"), &rodando);
    println!("{}", desligar::texto(&r));
    Ok(if r["pode"] == serde_json::json!(true) { 0 } else { 1 })
}

fn guarda(s: &Settings) -> PathGuard {
    let mut safe = orion_policy::paths::default_safe_roots();
    safe.extend(s.extra_safe_roots.iter().cloned());
    PathGuard { protected_roots: vec![project_root()], safe_roots: safe, system_roots: orion_policy::paths::system_roots() }
}

/// Organizador de Downloads com regras suas (E9.4).
pub fn downloads(s: &Settings, acao: &str, simular: bool, hoje: bool, sim: bool) -> anyhow::Result<i32> {
    let store = Arc::new(Store::open(s.db_path())?);
    let d = downloads::Downloads::new(s.data_dir.join("regras_downloads.yaml"), store, guarda(s));
    match acao {
        "exemplo" => {
            if d.arquivo.exists() {
                println!("{} já existe; nada foi trocado", d.arquivo.display());
                return Ok(1);
            }
            if let Some(p) = d.arquivo.parent() {
                std::fs::create_dir_all(p)?;
            }
            std::fs::write(&d.arquivo, downloads::EXEMPLO)?;
            println!("exemplo gravado em {}\nleia e rode: orion downloads conceder", d.arquivo.display());
        }
        "estado" => {
            let e = d.estado();
            println!("{}: {}, {} regra(s)", e["arquivo"].as_str().unwrap_or(""), e["estado"].as_str().unwrap_or("").replace('_', " "), e["regras"]);
            if let Some(err) = e["erro"].as_str() {
                println!("ERRO: {err}");
            }
            for m in d.historico(10) {
                println!("  {}{} -> {}", if m["desfeito"] == serde_json::json!(true) { "(desfeito) " } else { "" }, m["de"].as_str().unwrap_or(""), m["para"].as_str().unwrap_or(""));
            }
        }
        "conceder" => {
            if !sim && !perguntar("As regras vão mover arquivos SEM perguntar. Conceder? [s/N] ") {
                println!("nada foi concedido");
                return Ok(1);
            }
            if let Err(e) = d.conceder() {
                eprintln!("{e}");
                return Ok(1);
            }
            println!("concedido (vale enquanto o arquivo de regras não mudar)");
        }
        "revogar" => println!("{}", if d.revogar() { "concessão revogada" } else { "não havia concessão" }),
        "desfazer" => match d.desfazer(if hoje { "hoje" } else { "ultimo" }) {
            Ok(r) => {
                println!("{} desfeito(s)", r["desfeitos"].as_array().map(Vec::len).unwrap_or(0));
                for p in r["pulados"].as_array().into_iter().flatten() {
                    println!("  pulado — {}", p.as_str().unwrap_or(""));
                }
            }
            Err(e) => {
                eprintln!("{e}");
                return Ok(1);
            }
        },
        _ => {
            let r = d.rodar(simular);
            println!("{}: {} {}, {} ficaram", r.status, r.movidos.len(), if simular { "a mover" } else { "movido(s)" }, r.ficaram);
            for m in &r.movidos {
                println!("  {} -> {}", m["de"].as_str().unwrap_or(""), m["para"].as_str().unwrap_or(""));
            }
            for x in &r.recusados {
                println!("  recusado — {x}");
            }
            return Ok(if r.status == "ok" { 0 } else { 1 });
        }
    }
    Ok(0)
}

/// Rotinas suas (E9.1): listar, exemplo, conceder, revogar.
pub fn rotina(s: &Settings, acao: &str, nome: Option<&str>, sim: bool) -> anyhow::Result<i32> {
    let store = Arc::new(Store::open(s.db_path())?);
    let pasta = s.data_dir.join("rotinas");
    let r = rotinas::RotinaStore::new(pasta.clone(), store);
    match acao {
        "exemplo" => {
            std::fs::create_dir_all(&pasta)?;
            let alvo = pasta.join("bom-dia.yaml");
            if alvo.exists() {
                println!("{} já existe; nada foi trocado", alvo.display());
                return Ok(1);
            }
            std::fs::write(&alvo, rotinas::EXEMPLO_BOM_DIA)?;
            println!("exemplo gravado em {}\nleia o arquivo e rode: orion rotina conceder bom-dia", alvo.display());
            return Ok(0);
        }
        "listar" => {
            let lista = r.listar();
            for c in &lista {
                let extra = c.erro.as_ref().map(|e| format!(" — ERRO: {e}")).unwrap_or_default();
                println!("{}: {}{extra}", c.nome, c.estado.replace('_', " "));
            }
            if lista.is_empty() {
                println!("nenhuma rotina em {} (orion rotina exemplo cria uma)", pasta.display());
            }
            return Ok(0);
        }
        _ => {}
    }
    let Some(nome) = nome else {
        eprintln!("faltou o nome da rotina");
        return Ok(1);
    };
    if acao == "conceder" {
        let Some((c, rot)) = r.obter(nome).and_then(|c| c.rotina.clone().map(|x| (c, x))) else {
            eprintln!("rotina '{nome}' não existe ou tem erro: {}", r.obter(nome).and_then(|c| c.erro).unwrap_or_default());
            return Ok(1);
        };
        println!("A rotina '{}' vai poder rodar SEM perguntar:", c.nome);
        for (i, p) in rot.passos.iter().enumerate() {
            let t = p.ferramenta.clone().or_else(|| p.falar.as_ref().map(|f| format!("falar: {f}"))).unwrap_or_else(|| "briefing".into());
            println!("  {}. {t}{}", i + 1, if p.argumentos.is_empty() { String::new() } else { format!(" {}", serde_json::Value::Object(p.argumentos.clone())) });
        }
        println!("liberar: {}. O resto pede aprovação.", if rot.liberar.is_empty() { "nada".to_string() } else { rot.liberar.join(", ") });
        if !sim && !perguntar("Conceder? [s/N] ") {
            println!("nada foi concedido");
            return Ok(1);
        }
        if let Err(e) = r.conceder(nome) {
            eprintln!("{e}");
            return Ok(1);
        }
        println!("concedida (vale enquanto o arquivo não mudar)");
        return Ok(0);
    }
    if acao == "revogar" {
        println!("{}", if r.revogar(nome) { "concessão revogada" } else { "não havia concessão" });
        return Ok(0);
    }
    eprintln!("para executar: com o Orion no ar, /rotina no Telegram, a frase no chat ou POST /rotinas/<nome>/executar");
    Ok(1)
}

#[allow(dead_code)]
fn _usa(_: Option<PolicyEngine>) {}

/// Converte um arquivo no computador e guarda a cópia na biblioteca de resultados (E9.9).
pub fn converter(s: &Settings, arquivo: &str, formato: &str, saida: Option<&str>) -> anyhow::Result<i32> {
    use orion_core::conversor::{converter_e_guardar, formatos_para};
    let origem = std::path::PathBuf::from(arquivo);
    if formato == "?" {
        let lista = formatos_para(&origem.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default());
        println!("{}", if lista.is_empty() { "não sei converter esse tipo de arquivo".to_string() } else { lista.join(", ") });
        return Ok(if lista.is_empty() { 1 } else { 0 });
    }
    let store = Arc::new(Store::open(s.db_path())?);
    let destino = saida.map(std::path::PathBuf::from).unwrap_or_else(|| s.data_dir.join("conversoes"));
    let lib = orion_core::resultados::Library::new(store, s.data_dir.join("resultados"));
    match converter_e_guardar(&origem, formato, &destino, Some(&lib)) {
        Ok((arq, item)) => {
            println!("pronto: {}", arq.display());
            if let Some(i) = item {
                println!("cópia na biblioteca de resultados (id {})", i["id"]);
            }
            Ok(0)
        }
        Err(e) => {
            eprintln!("não converteu: {e}");
            Ok(1)
        }
    }
}

/// Skill `estilo-antonio` a partir dos seus repositórios (E7.8). Nasce desativada.
pub fn estilo(s: &Settings, repos: &[String], agente: &str, ativar: bool, sim: bool) -> anyhow::Result<i32> {
    use orion_core::estilo;
    let pasta = s.effective_skills_dir();
    if ativar {
        return Ok(match estilo::ativar(&pasta) {
            Ok(p) => {
                println!("skill ativada: {}", p.display());
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        });
    }
    if repos.is_empty() {
        eprintln!("informe as pastas dos repositórios (ou --ativar)");
        return Ok(1);
    }
    if !sim && !perguntar(&format!("O código de {} repositório(s) será LIDO pela CLI da sua assinatura ({agente}) e o resumo volta para uma skill. Continuar? [s/N] ", repos.len())) {
        println!("nada foi enviado");
        return Ok(1);
    }
    let store = Arc::new(Store::open(s.db_path())?);
    // o código sai do computador (REGRAS.md: toda saída de dado fica registrada, sem conteúdo)
    orion_ai::saidas::definir_destino(Some(orion_ai::saidas::destino_store(store.clone())));
    let d = orion_ai::delegate::Delegator::new(store, orion_ai::delegate::default_agents(), 600.0, 20_000);
    let r = estilo::gerar(&repos.iter().map(std::path::PathBuf::from).collect::<Vec<_>>(), &d, &pasta, Some(agente));
    orion_ai::saidas::definir_destino(None);
    match r {
        Ok(a) => {
            println!("skill pendente gravada em {}\nleia e rode: orion estilo --ativar", a.display());
            Ok(0)
        }
        Err(e) => {
            eprintln!("{e}");
            Ok(1)
        }
    }
}

/// Confere instalação, banco, login, chaves, MCP e backup (sem rede).
pub fn doctor(s: &Settings) -> anyhow::Result<i32> {
    let (txt, cod) = orion_core::doctor::relatorio(&orion_core::doctor::checar(s));
    println!("{txt}");
    Ok(cod)
}

/// Gera o arquivo de início automático do sistema (não ativa sozinho).
pub fn autostart(s: &Settings, plataforma: Option<&str>, install: bool, dir: Option<&str>, force: bool, ponte: bool) -> anyhow::Result<i32> {
    use orion_core::autostart::*;
    let exe = std::env::current_exe()?;
    let home = orion_core::downloads::dirs_home();
    let a = match render(plataforma.unwrap_or_else(|| detectar()), &exe, &project_root(), &s.data_dir.join("logs"), &home, ponte) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return Ok(1);
        }
    };
    if !(install || dir.is_some()) {
        println!("# {}\n{}", a.arquivo.display(), a.conteudo);
    } else {
        match instalar(&a, dir.map(std::path::Path::new), force) {
            Ok(alvo) => {
                if a.plataforma == "macos" {
                    std::fs::create_dir_all(s.data_dir.join("logs"))?;
                }
                println!("gravado: {}", alvo.display());
            }
            Err(e) => {
                eprintln!("{e}");
                return Ok(1);
            }
        }
    }
    println!("para ligar: {}\npara desligar: {}", a.ativar, a.desativar);
    Ok(0)
}

/// Sobe os servidores do mcp.json e mostra o estado de cada um com suas ferramentas.
pub fn mcp_check(s: &Settings, config: Option<&str>) -> anyhow::Result<i32> {
    use orion_core::mcp_client::{describe, manager_from_file};
    let caminho = config.map(std::path::PathBuf::from).unwrap_or_else(|| s.effective_mcp_config());
    let g = match manager_from_file(&caminho, vec![]) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            return Ok(1);
        }
    };
    let Some(g) = g else {
        println!("nenhum servidor habilitado em {}", caminho.display());
        return Ok(0);
    };
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    rt.block_on(async {
        g.start().await;
        println!("{}", describe(&g).join("\n"));
        let ok = g.status().values().all(|v| v.starts_with("ok"));
        g.stop().await;
        Ok(if ok { 0 } else { 1 })
    })
}

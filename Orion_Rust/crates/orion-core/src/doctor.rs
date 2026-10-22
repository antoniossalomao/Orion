//! `orion doctor`: confere a instalação num comando, sem rede e sem imprimir segredo.
//!
//! A única chamada é local: com `ORION_LOCAL_MODEL`, pergunta ao Ollama deste computador
//! (`127.0.0.1`) se o modelo está baixado. Nível `erro` impede o uso; `aviso` é algo a resolver;
//! `ok` está certo. Só os NOMES das chaves aparecem, nunca o valor.

use crate::config::Settings;
use crate::util::which;
use orion_ai::secrets::get_secret;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nivel {
    Ok,
    Aviso,
    Erro,
}

#[derive(Debug, Clone)]
pub struct Checagem {
    pub nome: String,
    pub nivel: Nivel,
    pub detalhe: String,
}

fn c(nome: &str, nivel: Nivel, detalhe: impl Into<String>) -> Checagem {
    Checagem { nome: nome.into(), nivel, detalhe: detalhe.into() }
}

/// Sufixo dos avisos de opt-in: onde o ORION_LEGADO.md (operação) explica como ligar direito.
fn ver(secao: &str) -> String {
    format!(" (ver ORION_LEGADO, operação §{secao})")
}

fn chave(valor: &str, cofre: &str) -> bool {
    !valor.is_empty() || get_secret(cofre).is_some()
}

fn dados(s: &Settings) -> Checagem {
    let r = std::fs::create_dir_all(&s.data_dir).and_then(|_| {
        let t = s.data_dir.join(".doctor");
        std::fs::write(&t, "x")?;
        std::fs::remove_file(&t)
    });
    match r {
        Ok(()) => c("pasta de dados", Nivel::Ok, s.data_dir.display().to_string()),
        Err(e) => c("pasta de dados", Nivel::Erro, format!("{}: {:?}", s.data_dir.display(), e.kind())),
    }
}

fn banco(s: &Settings) -> Checagem {
    let p = s.db_path();
    if !p.exists() {
        return c("banco da memória", Nivel::Aviso, "ainda não existe (nasce no primeiro uso)");
    }
    let r = (|| -> Result<(u32, bool), rusqlite::Error> {
        let con = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let v: String = con.query_row("SELECT value FROM meta WHERE key='schema_version'", [], |r| r.get(0))?;
        let ok: String = con.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        Ok((v.parse().unwrap_or(0), ok == "ok"))
    })();
    match r {
        Err(e) => c("banco da memória", Nivel::Erro, format!("ilegível: {e}")),
        Ok((_, false)) => c("banco da memória", Nivel::Erro, "integrity_check falhou: restaure um backup"),
        Ok((v, _)) if v > orion_store::schema::SCHEMA_VERSION => c("banco da memória", Nivel::Erro, format!("esquema v{v} é mais novo que este Orion")),
        Ok((v, _)) => c("banco da memória", Nivel::Ok, format!("esquema v{v}, íntegro")),
    }
}

fn login(s: &Settings) -> Checagem {
    let ap = s.auth_db_path();
    if !ap.exists() && s.admin_token.is_empty() {
        return c("login", Nivel::Aviso, "sem senha nem token: rode `orion set-password`");
    }
    if !ap.exists() {
        return c("login", Nivel::Ok, "só pelo ORION_ADMIN_TOKEN (sem senha)");
    }
    match crate::auth::AuthService::open(&ap, &s.auth_user, 3600.0) {
        Ok(a) => {
            if !a.has_password() {
                return c("login", Nivel::Aviso, "sem senha definida: `orion set-password`");
            }
            if a.uses_default_password() {
                return c("login", Nivel::Erro, "ainda é a senha de fábrica: troque com set-password");
            }
            c("login", Nivel::Ok, "senha definida")
        }
        Err(e) => c("login", Nivel::Erro, format!("ilegível: {e}")),
    }
}

fn rede(s: &Settings) -> Checagem {
    let fora = !s.hosts_de_fora().is_empty();
    if fora && s.admin_token.is_empty() && !s.auth_db_path().exists() {
        return c("acesso de fora", Nivel::Erro, "host liberado sem login (REGRAS.md: acesso de fora só com login)");
    }
    if !["127.0.0.1", "localhost", "::1"].contains(&s.host.as_str()) && !s.allow_public_bind {
        return c("acesso de fora", Nivel::Erro, format!("host {} sem allow_public_bind", s.host));
    }
    c("acesso de fora", Nivel::Ok, if fora { "com login" } else { "só local" })
}

fn gateway(s: &Settings) -> Checagem {
    let mut ligados: Vec<String> = s.provedores.iter().filter(|p| get_secret(&orion_ai::provedores::chave_env(&p.id)).is_some()).map(|p| p.id.clone()).collect();
    let mut sem_chave: Vec<String> = s.provedores.iter().filter(|p| !ligados.contains(&p.id)).map(|p| orion_ai::provedores::chave_env(&p.id)).collect();
    if !s.gateway_url.is_empty() {
        if chave(&s.gateway_api_key, "ORION_GATEWAY_API_KEY") {
            ligados.push("avulso".into());
        } else {
            sem_chave.push("ORION_GATEWAY_API_KEY".into());
        }
    }
    if ligados.is_empty() {
        let motivo = if sem_chave.is_empty() { "ORION_PROVEDORES vazio".to_string() } else { format!("sem chave: {}", sem_chave.join(", ")) };
        return c("modelos", Nivel::Aviso, format!("{motivo}: /chat desligado"));
    }
    if !sem_chave.is_empty() {
        return c("modelos", Nivel::Aviso, format!("{} · sem chave: {}", ligados.join(", "), sem_chave.join(", ")));
    }
    c("modelos", Nivel::Ok, format!("{} (não testei a rede)", ligados.join(", ")))
}

fn opcionais(s: &Settings) -> Vec<Checagem> {
    let mut out = vec![];
    let precisa = [
        ("embeddings", true, s.embed_api_key.as_str(), "ORION_EMBED_API_KEY", "busca só por texto (FTS)".to_string()),
        ("voz (Groq)", s.voice_enabled, s.transcribe_api_key.as_str(), "ORION_TRANSCRIBE_API_KEY", format!("voz ligada sem chave de transcrição{}", ver("4.3"))),
        ("Telegram", !s.telegram_allowed_users.is_empty(), s.telegram_token.as_str(), "ORION_TELEGRAM_TOKEN", format!("usuários liberados mas sem token{}", ver("6"))),
    ];
    for (nome, ligado, campo, cofre, falta) in precisa {
        if chave(campo, cofre) {
            out.push(c(nome, Nivel::Ok, "chave presente"));
        } else if ligado {
            out.push(c(nome, Nivel::Aviso, falta));
        }
    }
    if s.screen_memory {
        if which("tesseract").is_none() {
            out.push(c("memória da tela", Nivel::Aviso, format!("ligada, mas falta instalar: tesseract{}", ver("8"))));
        } else {
            out.push(c("memória da tela", Nivel::Ok, format!("OCR local, guarda {} dia(s)", s.screen_retention_days)));
        }
    }
    if !s.research_at.is_empty() && !(s.web_tools && s.vault_dir.is_some()) {
        out.push(c("pesquisa noturna", Nivel::Aviso, format!("ORION_RESEARCH_AT sem ORION_WEB_TOOLS e vault{}", ver("10"))));
    }
    if s.wake_enabled && !s.voice_enabled {
        out.push(c("palavra de ativação", Nivel::Aviso, format!("wake ligado sem ORION_VOICE_ENABLED{}", ver("4.4"))));
    }
    if s.weekly_ai && s.briefing_at.is_empty() {
        out.push(c("leitura semanal", Nivel::Aviso, format!("ORION_WEEKLY_AI sem ORION_BRIEFING_AT: sai junto com o briefing{}", ver("11"))));
    }
    out
}

fn mcp(s: &Settings) -> Checagem {
    let p = s.effective_mcp_config();
    if !s.mcp_enabled {
        return c("MCP", Nivel::Ok, "desligado");
    }
    let nome = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if !p.exists() {
        return c("MCP", Nivel::Aviso, format!("sem {nome} (copie mcp.example.json)"));
    }
    let cfg: Result<serde_json::Value, String> = std::fs::read_to_string(&p).map_err(|e| format!("{:?}", e.kind())).and_then(|t| serde_json::from_str(&t).map_err(|_| "JSON".to_string()));
    match cfg {
        Err(e) => c("MCP", Nivel::Erro, format!("{nome} inválido: {e}")),
        Ok(v) => {
            let n = v.get("servers").or_else(|| v.get("mcpServers")).and_then(|x| x.as_object()).map(|o| o.len()).unwrap_or(0);
            c("MCP", Nivel::Ok, format!("{n} servidor(es) em {nome} (rode `mcp-check`)"))
        }
    }
}

fn backup(s: &Settings) -> Checagem {
    let pasta = s.effective_backup_dir();
    let mut dbs: Vec<std::time::SystemTime> = std::fs::read_dir(&pasta).map(|rd| rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "db")).filter_map(|e| e.metadata().ok()?.modified().ok()).collect()).unwrap_or_default();
    if dbs.is_empty() {
        return c("backup", Nivel::Aviso, format!("nenhum backup em {} (`orion backup`)", pasta.display()));
    }
    dbs.sort();
    let dias = dbs.last().and_then(|t| t.elapsed().ok()).map(|d| d.as_secs_f64() / 86400.0).unwrap_or(0.0);
    c("backup", if dias < 3.0 { Nivel::Ok } else { Nivel::Aviso }, format!("último há {dias:.0} dia(s), {} guardado(s)", dbs.len()))
}

fn vault(s: &Settings) -> Checagem {
    match &s.vault_dir {
        None => c("vault", Nivel::Aviso, "ORION_VAULT_DIR vazio: notas não entram na memória"),
        Some(v) if !Path::new(v).is_dir() => c("vault", Nivel::Erro, format!("{} não é uma pasta", v.display())),
        Some(v) => c("vault", Nivel::Ok, v.display().to_string()),
    }
}

/// REGRAS.md (custo zero): endereço fora da lista de gratuitos pede ORION_ALLOW_PAID=true.
fn custo(s: &Settings) -> Vec<Checagem> {
    let mut itens: Vec<(String, String, bool)> = vec![("gateway".into(), s.gateway_url.clone(), !s.gateway_url.is_empty()), ("transcrição".into(), s.transcribe_url.clone(), s.voice_enabled || !s.transcribe_api_key.is_empty())];
    itens.extend(s.provedores.iter().map(|p| (format!("provedor {}", p.id), p.url.clone(), !p.url.is_empty())));
    let mut out = vec![];
    for (nome, url, usado) in itens {
        if !usado || orion_ai::costs::host_gratuito(&url) {
            continue;
        }
        let host = url::Url::parse(&url).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_else(|| "?".into());
        if s.allow_paid {
            out.push(c(&format!("custo ({nome})"), Nivel::Ok, format!("{host} confirmado (ORION_ALLOW_PAID)")));
        } else {
            out.push(c(&format!("custo ({nome})"), Nivel::Aviso, format!("{host} está fora da lista de provedores gratuitos (REGRAS.md: custo zero): se é seu e gratuito, confirme com ORION_ALLOW_PAID=true{}", ver("17"))));
        }
    }
    out
}

/// REGRAS.md (modelo local): o Ollama responde em /api/tags e o modelo está baixado.
fn modelo_local(s: &Settings) -> Vec<Checagem> {
    if s.local_model.is_empty() {
        return vec![];
    }
    let base = s.local_url.trim_end_matches('/').trim_end_matches("/v1").to_string();
    let r = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(2)).build().and_then(|cl| cl.get(format!("{base}/api/tags")).send()).and_then(|r| r.json::<serde_json::Value>());
    let v = match r {
        Ok(v) => v,
        Err(_) => return vec![c("modelo local", Nivel::Aviso, format!("o Ollama não respondeu em {base}: instale e abra o Ollama{}", ver("17")))],
    };
    let nomes: Vec<String> = v["models"].as_array().into_iter().flatten().filter_map(|m| m["name"].as_str().map(String::from)).collect();
    let alvo = &s.local_model;
    if nomes.iter().any(|n| n == alvo || *n == format!("{alvo}:latest")) {
        vec![c("modelo local", Nivel::Ok, format!("{alvo} baixado; reserva sem ferramentas"))]
    } else {
        vec![c("modelo local", Nivel::Aviso, format!("{alvo} não está baixado: `ollama pull {alvo}`{}", ver("17")))]
    }
}

/// E4: o auto-compact precisa do modelo local; o reranker, do modelo ONNX.
fn memoria_nova(s: &Settings) -> Vec<Checagem> {
    let mut out = vec![];
    if ![0, 40].contains(&s.compact_at) && s.local_model.is_empty() {
        out.push(c("auto-compact", Nivel::Aviso, format!("ORION_COMPACT_AT foi ajustado, mas sem ORION_LOCAL_MODEL o resumo da conversa não roda (o texto nunca vai a um provedor de fora para ser resumido; REGRAS.md: o resumo nunca substitui o registro){}", ver("19"))));
    }
    if s.rerank {
        let pasta = if s.rerank_model_dir.is_empty() { s.data_dir.join("modelos").join("rerank") } else { s.rerank_model_dir.clone().into() };
        if !pasta.join("model.onnx").is_file() || !pasta.join("tokenizer.json").is_file() {
            out.push(c("reranker", Nivel::Aviso, format!("ORION_RERANK ligado, mas faltam model.onnx e tokenizer.json em {}{}", pasta.display(), ver("19"))));
        }
    }
    out
}

/// Pânico ligado é coisa que você precisa ver logo (REGRAS.md: pânico é corte, não pausa).
fn modos(s: &Settings) -> Vec<Checagem> {
    let p = s.db_path();
    if !p.exists() {
        return vec![];
    }
    let r = rusqlite::Connection::open_with_flags(&p, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).and_then(|con| con.query_row("SELECT value FROM meta WHERE key='valor:modo:panico'", [], |r| r.get::<_, String>(0)));
    match r {
        Ok(_) => vec![c("modo pânico", Nivel::Aviso, "LIGADO: saia com `orion panico --sair`")],
        Err(_) => vec![],
    }
}

pub fn checar(s: &Settings) -> Vec<Checagem> {
    let mut v = vec![dados(s), banco(s), login(s), rede(s), gateway(s), mcp(s), backup(s), vault(s)];
    v.extend(opcionais(s));
    v.extend(custo(s));
    v.extend(modelo_local(s));
    v.extend(memoria_nova(s));
    v.extend(modos(s));
    v
}

/// (texto, código de saída)
pub fn relatorio(itens: &[Checagem]) -> (String, i32) {
    let marca = |n: Nivel| match n {
        Nivel::Ok => "ok   ",
        Nivel::Aviso => "aviso",
        Nivel::Erro => "ERRO ",
    };
    let mut linhas: Vec<String> = itens.iter().map(|i| format!("[{}] {}: {}", marca(i.nivel), i.nome, i.detalhe)).collect();
    let erros = itens.iter().filter(|i| i.nivel == Nivel::Erro).count();
    let avisos = itens.iter().filter(|i| i.nivel == Nivel::Aviso).count();
    linhas.push(format!("\n{erros} erro(s), {avisos} aviso(s)"));
    (linhas.join("\n"), (erros > 0) as i32)
}

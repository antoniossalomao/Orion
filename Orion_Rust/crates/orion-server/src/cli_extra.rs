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

/// Importa o export do SurrealDB do legado.
pub fn import_surreal(s: &Settings, pasta: &str, assistente: &[String]) -> anyhow::Result<i32> {
    let store = Store::open(s.db_path())?;
    let mut nomes = vec!["Orion".to_string()];
    nomes.extend(assistente.iter().cloned());
    match orion_core::importer::import_surreal_export(&store, std::path::Path::new(pasta), &nomes) {
        Ok(r) => {
            println!("{}", r.resumo());
            Ok(0)
        }
        Err(e) => {
            eprintln!("{e}");
            Ok(1)
        }
    }
}

/// Fase 0: confere o export do legado (contagens, ensaio de importação e restauração).
pub fn verify_export(pasta: &str, assistente: &[String], env: Option<&str>, google_auth: Option<&str>, vault: Option<&str>, sem_ensaio: bool) -> anyhow::Result<i32> {
    let mut nomes = vec!["Orion".to_string()];
    nomes.extend(assistente.iter().cloned());
    let p = |x: Option<&str>| x.map(std::path::PathBuf::from);
    let (env, ga, v) = (p(env), p(google_auth), p(vault));
    let rel = orion_core::migration::verify_export(std::path::Path::new(pasta), &nomes, env.as_deref(), ga.as_deref(), v.as_deref(), !sem_ensaio);
    println!("{}", rel.linhas().join("\n"));
    Ok(if rel.ok() { 0 } else { 1 })
}

/// Transcreve áudio ou vídeo para uma nota no 00 Inbox (o áudio vai ao provedor).
pub fn transcrever(s: &Settings, alvo: &str, titulo: &str, sim: bool, propor: bool) -> anyhow::Result<i32> {
    use orion_core::capture::Capturer;
    use orion_core::media_transcribe::*;
    let Some(tr) = crate::estado::transcriber_from_settings(s) else {
        eprintln!("sem ORION_TRANSCRIBE_API_KEY: não há provedor de transcrição");
        return Ok(1);
    };
    let Some(vault) = &s.vault_dir else {
        eprintln!("defina ORION_VAULT_DIR: a nota vai para o 00 Inbox do vault");
        return Ok(1);
    };
    let e_link = alvo.to_lowercase().starts_with("http://") || alvo.to_lowercase().starts_with("https://");
    let origem = if e_link { "baixado do link".to_string() } else { format!("de '{}'", std::path::Path::new(alvo).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()) };
    if !sim && !perguntar(&format!("O áudio {origem} será ENVIADO ao provedor de transcrição ({}). Continuar? [s/N] ", s.transcribe_url)) {
        println!("nada foi enviado");
        return Ok(1);
    }
    let cap = Capturer::new(vault.clone(), &s.capture_folder);
    let store = Arc::new(Store::open(s.db_path())?);
    // o áudio sai do computador (REGRAS.md: toda saída de dado fica registrada, sem conteúdo)
    orion_ai::saidas::definir_destino(Some(orion_ai::saidas::destino_store(store.clone())));
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let progresso = |i: usize, n: usize| println!("transcrevendo parte {i}/{n}…");
    let r = rt.block_on(async {
        if e_link {
            println!("baixando o áudio…");
            transcrever_link(alvo, &tr, &cap, titulo, &progresso).await
        } else {
            transcrever_arquivo(std::path::Path::new(alvo), &tr, &cap, titulo, &progresso).await
        }
    });
    orion_ai::saidas::definir_destino(None);
    let nota = match r {
        Ok(n) => n,
        Err(e) => {
            eprintln!("{e}");
            return Ok(1);
        }
    };
    println!("nota criada: {}", nota.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    if propor {
        return propor_da_nota(s, &nota, sim, store, &rt);
    }
    Ok(0)
}

/// E9.11: o texto transcrito vai ao modelo, que propõe tarefas, lembretes e fatos. Cada item vira
/// um aviso na caixa de atividade para aprovar ou descartar; nada é criado aqui.
fn propor_da_nota(s: &Settings, nota: &std::path::Path, sim: bool, store: Arc<Store>, rt: &tokio::runtime::Runtime) -> anyhow::Result<i32> {
    let Some(gw) = crate::estado::gateway_from_settings(s) else {
        eprintln!("sem modelo configurado: a nota ficou, mas não há propostas");
        return Ok(0);
    };
    if !sim && !perguntar("O TEXTO transcrito será enviado ao modelo de chat para extrair tarefas. Continuar? [s/N] ") {
        println!("nenhuma proposta foi pedida");
        return Ok(0);
    }
    orion_ai::saidas::definir_destino(Some(orion_ai::saidas::destino_store(store.clone())));
    let texto = String::from_utf8_lossy(&std::fs::read(nota)?).into_owned();
    let itens = rt.block_on(orion_core::propostas::extrair(&gw, &texto));
    orion_ai::saidas::definir_destino(None);
    let itens = match itens {
        Ok(i) => i,
        Err(e) => {
            eprintln!("o modelo não respondeu: {e}");
            return Ok(1);
        }
    };
    let ops = Arc::new(orion_store::ops::Operations::new(store.clone()));
    let ids = orion_core::propostas::Propostas::new(store, ops).guardar(&nota.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), &itens)?;
    println!("{} proposta(s) na caixa de atividade (Atividade → Aprovar ou Descartar)", ids.len());
    Ok(0)
}

/// `orion plugin ...`: plugins locais (a concessão vale pelo hash do pacote inteiro).
pub fn plugin(s: &Settings, acao: &str, alvo: Option<&str>) -> i32 {
    use orion_core::plugins_locais::{describe, PluginStore};
    let loja = PluginStore::new(s.effective_plugins_dir());
    if acao == "listar" {
        let todos = loja.lista();
        for p in &todos {
            let d = describe(p);
            println!("{} {} [{}] — {}", d["nome"].as_str().unwrap_or(""), d["versao"].as_str().unwrap_or(""), d["estado"].as_str().unwrap_or(""), d["descricao"].as_str().unwrap_or(""));
            println!("   skills: {}", d["skills"]);
            for sv in d["servidores"].as_array().into_iter().flatten() {
                println!("   MCP {}: {} → {} (risco padrão {})", sv["nome"].as_str().unwrap_or(""), sv["transporte"].as_str().unwrap_or(""), sv["executa"].as_str().unwrap_or(""), sv["risco_padrao"].as_str().unwrap_or(""));
            }
        }
        if todos.is_empty() {
            println!("nenhum plugin em {}", loja.raiz.display());
        }
        return 0;
    }
    let Some(alvo) = alvo else {
        eprintln!("faltou a pasta ou o nome do plugin");
        return 1;
    };
    let r: Result<(), orion_core::plugins_locais::PluginError> = (|| {
        match acao {
            "instalar" | "atualizar" => {
                let p = loja.instalar(std::path::Path::new(alvo), acao == "atualizar")?;
                let d = describe(&p);
                println!("{} {} instalado, SEM concessão. Isto é o que ele liberaria:", p.nome, p.versao);
                println!("   skills: {}", d["skills"]);
                for sv in d["servidores"].as_array().into_iter().flatten() {
                    println!("   MCP {}: {} → {}", sv["nome"].as_str().unwrap_or(""), sv["transporte"].as_str().unwrap_or(""), sv["executa"].as_str().unwrap_or(""));
                    let livres = sv["ferramentas"].as_object().filter(|o| !o.is_empty()).map(|o| serde_json::Value::Object(o.clone()).to_string()).unwrap_or_else(|| format!("(todas com risco {})", sv["risco_padrao"].as_str().unwrap_or("")));
                    println!("      ferramentas: {livres}");
                }
                println!("Para liberar: orion plugin conceder {} (vale depois de reiniciar o Orion)", p.nome);
            }
            "conceder" => {
                loja.conceder(alvo)?;
                println!("{alvo}: concedido; reinicie o Orion para valer");
            }
            "revogar" => println!("{}", if loja.revogar(alvo)? { "revogado; reinicie o Orion" } else { "esse plugin não tinha concessão" }),
            _ => println!("{}", if loja.remover(alvo)? { "removido" } else { "plugin não encontrado" }),
        }
        Ok(())
    })();
    match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// `orion ponte --parear`: cria o token da ponte (escopo `ponte`, só o hash vai para o `auth.db`) e o
/// guarda no cofre do sistema. Parear de novo desfaz o pareamento anterior. Rodar a ponte em si
/// (teclas globais, bandeja, captura de tela) é o cliente de desktop.
pub fn ponte(s: &Settings, parear: bool) -> i32 {
    const SEGREDO: &str = "ORION_PONTE_TOKEN";
    if !parear {
        eprintln!("a ponte de desktop (teclas globais, bandeja e captura) é o cliente de desktop; aqui só há `orion ponte --parear`");
        return 1;
    }
    let auth = match orion_core::auth::AuthService::open(&s.auth_db_path(), &s.auth_user, s.session_ttl_h as f64 * 3600.0) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("não abri o banco de login: {e}");
            return 1;
        }
    };
    let host = std::fs::read_to_string("/etc/hostname").map(|h| h.trim().to_string()).ok().or_else(|| std::env::var("COMPUTERNAME").ok()).unwrap_or_else(|| "ponte".into());
    let token = match auth.create_device_token("ponte", &host) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("não consegui criar o token: {e}");
            return 1;
        }
    };
    match orion_ai::secrets::set_secret(SEGREDO, &token) {
        Ok(()) => {
            println!("ponte pareada: o token ficou no cofre do sistema como {SEGREDO} (escopo 'ponte').");
            0
        }
        Err(e) => {
            eprintln!("cofre indisponível ({e}): defina {SEGREDO} no ambiente da ponte com este valor (aparece só agora):\n{token}");
            1
        }
    }
}

/// Confirma (ou desfaz) que a tela pode ser bloqueada ao sair
/// (REGRAS.md: presença só pausa e retoma, cumpre e bloqueia só com sua confirmação).
pub fn presenca(s: &Settings, acao: &str) -> anyhow::Result<i32> {
    use orion_core::vigia::CHAVE_BLOQUEIO;
    let store = Store::open(s.db_path())?;
    match acao {
        "confirmar" => {
            store.counter_set(CHAVE_BLOQUEIO, 1)?;
            let aviso = if s.presence_lock { "" } else { " (ligue ORION_PRESENCE_LOCK=true)" };
            println!("bloqueio ao sair confirmado{aviso}");
        }
        "desfazer" => {
            store.counter_set(CHAVE_BLOQUEIO, 0)?;
            println!("bloqueio ao sair desfeito");
        }
        _ => {
            let confirmado = store.counter_get(CHAVE_BLOQUEIO) == 1;
            println!(
                "presença {}: bloqueio ao sair {} e {}",
                if s.presence_enabled { "ligada" } else { "desligada (ORION_PRESENCE_ENABLED)" },
                if s.presence_lock { "pedido" } else { "desligado (ORION_PRESENCE_LOCK)" },
                if confirmado { "confirmado" } else { "não confirmado (`orion presenca confirmar`)" },
            );
        }
    }
    Ok(0)
}

/// Cadastro e apagar do próprio rosto (E11.5; REGRAS.md: rosto é local, cadastrado só pelo dono,
/// e nunca sai do computador). Nenhuma ferramenta do modelo.
pub fn rosto(s: &Settings, acao: &str, nome: &str, fotos: usize) -> anyhow::Result<i32> {
    use orion_core::rosto::{CofreSistema, Rostos, LIMIAR};
    let store = Arc::new(Store::open(s.db_path())?);
    let rostos = Rostos::new(store, s.face_model.clone(), LIMIAR, None).com_cofre(Arc::new(CofreSistema));
    match acao {
        "apagar" => {
            println!("rosto apagado: {} vetor(es)", rostos.apagar(nome));
            Ok(0)
        }
        "estado" => {
            println!(
                "cadastrado({nome}): {}, modelo: {}, câmera: {}",
                rostos.cadastrado(nome),
                if rostos.disponivel() { "ok" } else { "indisponível" },
                if s.camera { "ligada" } else { "desligada (ORION_CAMERA)" },
            );
            Ok(0)
        }
        _ => {
            if !s.camera {
                eprintln!("defina ORION_CAMERA=true para capturar fotos do rosto");
                return Ok(1);
            }
            if s.face_model.is_none() {
                eprintln!("defina ORION_FACE_MODEL com o caminho do modelo de rosto (.onnx)");
                return Ok(1);
            }
            let n = fotos.clamp(orion_core::rosto::MIN_AMOSTRAS, 10);
            println!("olhe para a câmera: {n} fotos, uma por segundo");
            let mut amostras = Vec::with_capacity(n);
            for _ in 0..n {
                match orion_core::camera::foto_jpeg(s.camera_device as i32, 10.0) {
                    Ok(jpeg) => amostras.push(jpeg),
                    Err(e) => {
                        eprintln!("{e}");
                        return Ok(1);
                    }
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            match rostos.cadastrar(&amostras, nome) {
                Ok(total) => {
                    println!("cadastrado: {total} amostra(s)");
                    Ok(0)
                }
                Err(e) => {
                    eprintln!("{e}");
                    Ok(1)
                }
            }
        }
    }
}

/// Legenda ao vivo (E11.7): abre a URL da legenda no navegador ou imprime por N segundos.
pub fn legenda(s: &Settings, segundos: u64) -> anyhow::Result<i32> {
    if !(s.subtitles && s.subtitle_model.is_some()) {
        eprintln!("legenda desligada: defina ORION_SUBTITLES=true e ORION_SUBTITLE_MODEL");
        return Ok(1);
    }
    if segundos == 0 {
        let url = format!("http://127.0.0.1:{}/ui/legenda.html", s.port);
        let _ = orion_core::tools::camera::abrir_navegador(&url);
        println!("janela da legenda aberta: {url}");
        return Ok(0);
    }
    let Some(modelo) = &s.subtitle_model else { return Ok(1) };
    let tr = match orion_core::legenda::criar_transcritor(&modelo.to_string_lossy()) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            return Ok(1);
        }
    };
    let leg = Arc::new(orion_core::legenda::Legenda::new(tr, 200.0));
    let parar = orion_core::legenda::iniciar_thread(leg.clone(), &s.subtitle_device);
    let inicio = std::time::Instant::now();
    let mut vistas = 0usize;
    while inicio.elapsed().as_secs() < segundos {
        let est = leg.estado();
        let linhas = est["linhas"].as_array().cloned().unwrap_or_default();
        if linhas.len() > vistas {
            for l in &linhas[vistas..] {
                println!("[{}] {}", l["hora"].as_str().unwrap_or(""), l["texto"].as_str().unwrap_or(""));
            }
            vistas = linhas.len();
        }
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    parar();
    Ok(0)
}

/// Diagnóstico da escuta: lista dispositivos ou informa estado (sem agente, sem rede, sem gravar).
pub fn wake_test(s: &Settings, listar: bool, segundos: u64) -> anyhow::Result<i32> {
    if listar {
        let devs = orion_core::wake::listar_microfones();
        if devs.is_empty() {
            println!("nenhum microfone listado (instale ffmpeg ou pactl/arecord)");
        } else {
            for (idx, nome) in devs {
                println!("[{idx}] {nome}");
            }
        }
        return Ok(0);
    }
    println!(
        "wake-test ({segundos}s, motor={}): o detector nativo de áudio em tempo real requer pareamento de dispositivo local.",
        s.wake_engine
    );
    Ok(0)
}


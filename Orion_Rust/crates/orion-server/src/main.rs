//! `orion`: sobe o servidor (padrão) e os comandos de manutenção.

mod estado;
mod http;
mod log;
mod rotas;

use clap::{Parser, Subcommand};
use orion_core::config::Settings;
use orion_core::modos::Modos;
use orion_store::ops::Operations;
use orion_store::Store;
use std::io::BufRead;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "orion", about = "Orion — assistente pessoal")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Sobe o servidor (padrão).
    Serve,
    /// Interno: extrai o texto de um documento num processo descartável.
    #[command(hide = true)]
    ExtrairDocumento { arquivo: String, tipo: String },
    /// Backup diário da memória (mantém os últimos N).
    Backup {
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long, default_value_t = 7)]
        keep: usize,
    },
    /// Define ou troca a senha do login (revoga as sessões abertas).
    #[command(name = "set-password")]
    SetPassword {
        #[arg(long)]
        stdin: bool,
    },
    /// Guarda a chave de API de um provedor no cofre do sistema.
    Chave {
        provedor: String,
        #[arg(long)]
        stdin: bool,
    },
    /// Modo pânico: corta rede, execução, tela e escuta (só sai com --sair).
    Panico {
        #[arg(long)]
        sair: bool,
        #[arg(long)]
        estado: bool,
    },
    /// Segura avisos não urgentes até um horário (HH:MM) ou --sair.
    #[command(name = "nao-perturbe")]
    NaoPerturbe {
        ate: Option<String>,
        #[arg(long)]
        sair: bool,
    },
    /// Memória da tela: estado, ou apagar tudo o que foi guardado.
    Tela {
        #[arg(long)]
        limpar: bool,
    },
    /// Lista os fatos da memória (com id, fonte e data).
    Fatos {
        #[arg(long)]
        duplicados: bool,
    },
    /// Lista as skills válidas e as rejeitadas, com o motivo.
    Skills,
    /// Apaga fatos da memória (texto, índice de busca e vetor); pede confirmação.
    Esquecer {
        consulta: String,
        #[arg(long)]
        sim: bool,
    },
    /// Restaura um backup no lugar do banco (confere antes).
    Restore {
        arquivo: PathBuf,
        #[arg(long)]
        force: bool,
    },
}

fn ler_linha() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().lock().read_line(&mut s);
    s.trim_end_matches(['\r', '\n']).to_string()
}

fn guardar_chave(provedor: &str, stdin: bool) -> i32 {
    use orion_ai::provedores::{catalogado, chave_env};
    let pid = provedor.trim().to_lowercase();
    let chave = if stdin {
        ler_linha().trim().to_string()
    } else {
        rpassword::prompt_password(format!("Chave de API de {pid} (não aparece na tela): ")).unwrap_or_default().trim().to_string()
    };
    if chave.is_empty() {
        eprintln!("chave vazia: nada foi gravado");
        return 1;
    }
    if let Err(e) = orion_ai::secrets::set_secret(&chave_env(&pid), &chave) {
        eprintln!("cofre indisponível ({e}): use {} no ambiente", chave_env(&pid));
        return 1;
    }
    let extra = if catalogado(&pid).is_some() { "" } else { " (fora do catálogo: informe 'url' em ORION_PROVEDORES)" };
    println!("chave de {pid} guardada no cofre como {}{extra}", chave_env(&pid));
    0
}

fn set_password(s: &Settings, stdin: bool) -> anyhow::Result<i32> {
    let senha = if stdin {
        ler_linha()
    } else {
        let a = rpassword::prompt_password("Nova senha do Orion: ")?;
        if a != rpassword::prompt_password("Repita a senha: ")? {
            eprintln!("as senhas não conferem");
            return Ok(1);
        }
        a
    };
    let auth = orion_core::auth::AuthService::open(&s.auth_db_path(), &s.auth_user, s.session_ttl_h as f64 * 3600.0)?;
    if let Err(e) = auth.set_password(&senha) {
        eprintln!("{e}");
        return Ok(1);
    }
    println!("senha de '{}' definida; as sessões abertas foram encerradas", s.auth_user);
    Ok(0)
}

fn modos(s: &Settings, cmd: &Cmd) -> anyhow::Result<i32> {
    let store = Arc::new(Store::open(s.db_path())?);
    let ops = Arc::new(Operations::new(store.clone()));
    let m = Modos::new(store, &s.dnd_at, Some(Arc::new(move |e| ops.audit_add(e).map(|_| ()).map_err(Into::into))));
    match cmd {
        Cmd::Panico { estado: true, .. } => println!("{}", if m.panico() { "modo pânico LIGADO" } else { "modo pânico desligado" }),
        Cmd::Panico { sair: true, .. } => match m.sair_panico("cli") {
            Ok(saiu) => println!("{}", if saiu { "modo pânico desligado" } else { "o modo pânico não estava ligado" }),
            Err(e) => {
                eprintln!("{e}");
                return Ok(1);
            }
        },
        Cmd::Panico { .. } => {
            m.entrar_panico("cli")?;
            println!("MODO PÂNICO LIGADO: ferramentas de rede e de execução cortadas, memória da tela, escuta e jobs de rede parados. Nada volta sozinho: `orion panico --sair`.");
        }
        Cmd::NaoPerturbe { sair: true, .. } => {
            m.desligar_nao_perturbe()?;
            println!("não perturbe manual desligado");
        }
        Cmd::NaoPerturbe { ate: Some(ate), .. } => match m.ligar_nao_perturbe(ate) {
            Ok(fim) => println!("não perturbe até {}", orion_core::util::local(fim).format("%d/%m %H:%M")),
            Err(e) => {
                eprintln!("{e}");
                return Ok(1);
            }
        },
        _ => println!("não perturbe {}", if m.nao_perturbe() { "ligado" } else { "desligado" }),
    }
    Ok(0)
}

fn esquecer(s: &Settings, consulta: &str, sim: bool) -> anyhow::Result<i32> {
    let store = Store::open(s.db_path())?;
    let achados = match consulta.trim().parse::<i64>() {
        Ok(id) => store.get_fact(id)?.into_iter().collect(),
        Err(_) => store.search_facts(consulta, 20)?,
    };
    if achados.is_empty() {
        println!("nenhum fato casa com isso");
        return Ok(2);
    }
    for f in &achados {
        println!("[{}] {}  (fonte: {})", f.id, f.text, f.source);
    }
    if !sim {
        print!("apagar {} fato(s)? [s/N] ", achados.len());
        let _ = std::io::Write::flush(&mut std::io::stdout());
        if ler_linha().trim().to_lowercase() != "s" {
            println!("nada foi apagado");
            return Ok(1);
        }
    }
    let mut apagados = 0;
    for f in &achados {
        if store.forget_fact(f.id)? {
            apagados += 1;
        }
    }
    println!("{apagados} fato(s) apagado(s) do banco, do índice de busca e dos vetores.\nAtenção: backups antigos (<dados>/backups), mensagens de conversas e notas do vault que repetem o fato NÃO foram tocados.");
    Ok(0)
}

async fn serve(s: Settings) -> anyhow::Result<i32> {
    let (host, porta) = (s.host.clone(), s.port);
    let st = estado::montar(s).await?;
    if let Some(j) = st.jobs.clone() {
        tokio::spawn(j.run_forever(st.settings.jobs_tick_s));
    }
    let app = rotas::router(st.clone());
    let ouvinte = tokio::net::TcpListener::bind((host.as_str(), porta)).await?;
    tracing::info!("Orion em http://{host}:{porta}");
    axum::serve(ouvinte, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    if let Some(m) = &st.mcp {
        m.stop().await;
    }
    orion_ai::saidas::definir_destino(None);
    Ok(0)
}

fn rodar(cli: Cli) -> anyhow::Result<i32> {
    if let Some(Cmd::ExtrairDocumento { arquivo, tipo }) = &cli.cmd {
        return Ok(orion_core::document_extract::principal(arquivo, tipo));
    }
    let s = Settings::load()?;
    log::setup(&s.log_level, s.log_json);
    let cmd = cli.cmd.unwrap_or(Cmd::Serve);
    match &cmd {
        Cmd::ExtrairDocumento { .. } => Ok(0),
        Cmd::Serve => tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(serve(s)),
        Cmd::Backup { dir, keep } => {
            let store = Store::open(s.db_path())?;
            let feito = store.daily_backup(&dir.clone().unwrap_or_else(|| s.data_dir.join("backups")), *keep)?;
            match feito {
                Some(p) => println!("backup: {}", p.display()),
                None => println!("backup de hoje já existe"),
            }
            Ok(0)
        }
        Cmd::SetPassword { stdin } => set_password(&s, *stdin),
        Cmd::Chave { provedor, stdin } => Ok(guardar_chave(provedor, *stdin)),
        Cmd::Panico { .. } | Cmd::NaoPerturbe { .. } => modos(&s, &cmd),
        Cmd::Tela { limpar } => {
            let store = Store::open(s.db_path())?;
            if *limpar {
                println!("{} registro(s) de tela apagado(s)", store.clear_screen()?);
            } else {
                let estado = if s.screen_memory { "ligada" } else { "desligada (ORION_SCREEN_MEMORY)" };
                let pausa = if store.counter_get("tela:pausa") != 0 { " e pausada" } else { "" };
                println!("memória da tela {estado}{pausa}: {} registro(s), guarda {} dia(s)", store.screen_count(0.0), s.screen_retention_days);
            }
            Ok(0)
        }
        Cmd::Fatos { duplicados } => {
            let store = Store::open(s.db_path())?;
            if *duplicados {
                let pares = store.duplicate_facts(0.75)?;
                for (a, b, j) in &pares {
                    println!("{:.0}%  [{}] {}\n     [{}] {}", j * 100.0, a.id, a.text, b.id, b.text);
                }
                println!("{} par(es); apague um com `orion esquecer <id>`", pares.len());
            } else {
                for f in store.facts()? {
                    println!("[{}] {}  (fonte: {})", f.id, f.text, f.source);
                }
            }
            Ok(0)
        }
        Cmd::Skills => {
            let cat = orion_core::skills::SkillCatalog::new(&s.effective_skills_dir(), vec![]);
            println!("pasta: {}", s.effective_skills_dir().display());
            for sk in cat.skills.values() {
                let avisos: String = sk.avisos.iter().map(|a| format!("  ({a})")).collect();
                println!("[ok]  {}: {}{avisos}", sk.nome, sk.descricao.chars().take(80).collect::<String>());
            }
            for r in &cat.rejeitadas {
                println!("[rejeitada] {}: {}", r.pasta, r.motivo);
            }
            if cat.skills.is_empty() && cat.rejeitadas.is_empty() {
                println!("nenhuma skill (crie <pasta>/<nome>/SKILL.md)");
            }
            Ok(0)
        }
        Cmd::Esquecer { consulta, sim } => esquecer(&s, consulta, *sim),
        Cmd::Restore { arquivo, force } => {
            let contagens = match Store::verify_backup(arquivo) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("backup recusado: {e}");
                    return Ok(1);
                }
            };
            if s.db_path().exists() && !force {
                eprintln!("{} já existe; pare o Orion e use --force para substituir (o banco atual é perdido: faça `orion backup` antes)", s.db_path().display());
                return Ok(1);
            }
            Store::restore(arquivo, &s.db_path())?;
            let resumo: Vec<String> = contagens.iter().filter(|(_, n)| *n > 0).map(|(t, n)| format!("{t}={n}")).collect();
            println!("restaurado em {}: {}", s.db_path().display(), resumo.join(", "));
            Ok(0)
        }
    }
}

fn main() {
    let codigo = match rodar(Cli::parse()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("erro: {e:#}");
            1
        }
    };
    std::process::exit(codigo);
}

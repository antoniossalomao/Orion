//! `orion`: abre o app (padrão) e traz os comandos de manutenção.
//!
//! No Windows o executável é de janela (sem console piscando ao abrir o app); a CLI anexa o console de
//! quem a chamou, a menos que a saída já esteja redirecionada (pipe ou arquivo).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod appmode;
mod onboarding;
mod onboarding_drive;
mod onboarding_hello;
mod onboarding_imports;
mod cli_db;
mod cli_extra;
mod estado;
mod embedded;
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
    /// Abre escondido na bandeja (é o que o início com o Windows usa).
    #[arg(long, global = true)]
    background: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Confere a inferência embarcada com um modelo GGUF já instalado, sem rede.
    #[command(name = "local-test")]
    LocalTest {
        #[arg(long)]
        model: PathBuf,
        #[arg(long)]
        gpu: bool,
    },
    /// Servidor HTTP local (só para desenvolvimento e acesso remoto; o produto não abre porta).
    #[cfg(feature = "http")]
    Serve,
    /// Abre o aplicativo: janela nativa com o núcleo no mesmo processo, sem rede (é o padrão).
    App,
    /// Atualiza o Orion: `--check [--apply]` confere o feed assinado; `--rollback` devolve a versão anterior.
    Update {
        #[arg(long)]
        check: bool,
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        rollback: bool,
    },
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
    /// Cifra o banco (SQLCipher) com uma senha e gera a chave de recuperação.
    #[command(name = "db-cifrar")]
    DbCifrar {
        #[arg(long)]
        stdin: bool,
    },
    /// Troca a senha do banco cifrado (o banco não é recifrado).
    #[command(name = "db-senha")]
    DbSenha {
        #[arg(long)]
        stdin: bool,
    },
    /// Esqueci a senha do banco: redefine com a chave de recuperação.
    #[command(name = "db-recuperar")]
    DbRecuperar {
        #[arg(long)]
        stdin: bool,
    },
    /// Gera outra chave de recuperação; a anterior deixa de valer.
    #[command(name = "db-nova-recuperacao")]
    DbNovaRecuperacao {
        #[arg(long)]
        stdin: bool,
    },
    /// Apaga o backup em claro que sobra depois de cifrar.
    #[command(name = "db-apagar-claro")]
    DbApagarClaro,
    /// Lê o log cifrado de um dia (padrão: o mais recente).
    Logs {
        #[arg(long)]
        dia: Option<String>,
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
    /// Ponte de desktop: `--parear` cria o token (escopo `ponte`) e o guarda no cofre do sistema.
    Ponte {
        #[arg(long)]
        parear: bool,
    },
    /// Plugins locais: listar, instalar, atualizar, conceder, revogar, remover.
    Plugin {
        #[arg(value_parser = ["listar", "instalar", "atualizar", "conceder", "revogar", "remover"])]
        acao: String,
        /// Pasta do plugin (instalar/atualizar) ou o nome.
        alvo: Option<String>,
    },
    /// Apaga fatos da memória (texto, índice de busca e vetor); pede confirmação.
    Esquecer {
        consulta: String,
        #[arg(long)]
        sim: bool,
    },
    /// O Orion como servidor MCP só de leitura (stdio) para o Claude Code ou o Codex.
    #[command(name = "mcp-servidor")]
    McpServidor {
        /// Gera um token novo (revoga o anterior).
        #[arg(long)]
        token: bool,
        /// Revoga o token: nada mais lê.
        #[arg(long)]
        revogar: bool,
    },
    /// Converte um arquivo no computador (use '?' de formato para ver as opções).
    Converter {
        arquivo: String,
        formato: String,
        #[arg(long)]
        saida: Option<String>,
    },
    /// Gera a skill estilo-antonio lendo seus repositórios (nasce desativada).
    Estilo {
        repos: Vec<String>,
        #[arg(long, default_value = "claude", value_parser = ["claude", "codex", "gemini"])]
        agente: String,
        #[arg(long)]
        ativar: bool,
        #[arg(long)]
        sim: bool,
    },
    /// Confere instalação, banco, login, chaves, MCP e backup (sem rede).
    Doctor,
    /// Gera o arquivo de início automático do sistema (não ativa sozinho).
    Autostart {
        #[arg(long, value_parser = ["windows", "macos", "linux"])]
        plataforma: Option<String>,
        #[arg(long)]
        install: bool,
        #[arg(long)]
        dir: Option<String>,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        ponte: bool,
    },
    /// Sobe os servidores do mcp.json e mostra o estado e as ferramentas de cada um.
    #[command(name = "mcp-check")]
    McpCheck {
        #[arg(long)]
        config: Option<String>,
    },
    /// Importa o export do SurrealDB do legado.
    #[command(name = "import-surreal")]
    ImportSurreal {
        /// Pasta com evento.json, sessao.json... (backup_memoria).
        pasta: String,
        #[arg(long)]
        assistente: Vec<String>,
    },
    /// Fase 0: confere o export do legado (contagens, ensaio de importação e restauração).
    #[command(name = "verify-export")]
    VerifyExport {
        pasta: String,
        #[arg(long)]
        assistente: Vec<String>,
        #[arg(long)]
        env: Option<String>,
        #[arg(long = "google-auth")]
        google_auth: Option<String>,
        #[arg(long)]
        vault: Option<String>,
        #[arg(long = "sem-ensaio")]
        sem_ensaio: bool,
    },
    /// Transcreve áudio ou vídeo para uma nota no 00 Inbox (o áudio vai ao provedor).
    Transcrever {
        /// Arquivo local, ou um link http(s) (precisa do yt-dlp).
        arquivo: String,
        #[arg(long, default_value = "")]
        titulo: String,
        #[arg(long)]
        sim: bool,
        #[arg(long)]
        propor: bool,
    },
    /// O que ficaria pela metade se desligasse agora.
    #[command(name = "posso-desligar")]
    PossoDesligar,
    /// Organizador de Downloads com as suas regras.
    Downloads {
        #[arg(value_parser = ["exemplo", "estado", "conceder", "revogar", "rodar", "desfazer"])]
        acao: String,
        #[arg(long)]
        simular: bool,
        #[arg(long)]
        hoje: bool,
        #[arg(long)]
        sim: bool,
    },
    /// Suas rotinas: listar, exemplo, conceder, revogar.
    Rotina {
        #[arg(value_parser = ["listar", "exemplo", "conceder", "revogar"])]
        acao: String,
        nome: Option<String>,
        #[arg(long)]
        sim: bool,
    },
    /// Presença (E11.6): confirmar ou desfazer o bloqueio ao sair.
    Presenca {
        #[arg(value_parser = ["confirmar", "desfazer", "estado"])]
        acao: String,
    },
    /// Reconhecimento facial local (E11.5): cadastrar, apagar ou estado.
    Rosto {
        #[arg(value_parser = ["cadastrar", "apagar", "estado"])]
        acao: String,
        #[arg(default_value = "")]
        nome: String,
        #[arg(long, default_value_t = 5)]
        fotos: usize,
    },
    /// Legenda ao vivo (E11.7): abre a janela flutuante ou imprime por N segundos.
    Legenda {
        #[arg(long, default_value_t = 0)]
        segundos: u64,
    },
    /// Testa o microfone e a palavra de ativação (sem agente, sem rede, sem gravar nada).
    #[command(name = "wake-test")]
    WakeTest {
        #[arg(long)]
        listar: bool,
        #[arg(long, default_value_t = 30)]
        segundos: u64,
    },
    /// Recupera a conta com o código salvo no primeiro uso, sem alterar os dados.
    #[command(name="conta-recuperar")]
    ContaRecuperar,
    /// Exporta banco e perfil para um arquivo autenticado, sem tokens ou auth.db.
    #[command(name="backup-exportar")]
    BackupExportar { destino:PathBuf },
    /// Importa um arquivo autenticado para uma pasta NOVA, com nova senha de login.
    #[command(name="backup-importar")]
    BackupImportar { arquivo:PathBuf, pasta:PathBuf, #[arg(long)] recuperacao:bool },
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
    let store = Arc::new(orion_core::dbopen::open_store(&s.db_path())?);
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
    let store = orion_core::dbopen::open_store(&s.db_path())?;
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

#[cfg(feature = "http")]
async fn serve(s: Settings) -> anyhow::Result<i32> {
    let (host, porta) = (s.host.clone(), s.port);
    #[cfg(windows)]
    let vigiar = s.clone();
    let st = estado::montar(s).await?;
    #[cfg(windows)]
    cli_db::watch_screen_lock(&vigiar, st.auth.clone());
    let ouvinte = tokio::net::TcpListener::bind((host.as_str(), porta)).await?;
    serve_state(st, ouvinte, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await
}

/// Liga os trabalhos de fundo (escuta, rotinas, Telegram). Devolve a tarefa do Telegram.
fn iniciar_fundo(st: &estado::Estado) -> Option<tokio::task::JoinHandle<()>> {
    if let Some(e) = &st.escuta {
        e.start();
    }
    if let Some(j) = st.jobs.clone() {
        tokio::spawn(j.run_forever(st.settings.jobs_tick_s));
    }
    st.telegram.clone().map(|t| tokio::spawn(t.run()))
}

async fn encerrar_fundo(st: &estado::Estado, tarefa_telegram: Option<tokio::task::JoinHandle<()>>) {
    if let Some(e) = &st.escuta {
        e.stop();
    }
    if let Some(l) = &st.legenda {
        l.rodando.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(t) = tarefa_telegram {
        t.abort();
    }
    if let Some(m) = &st.mcp {
        m.stop().await;
    }
    if let Some(e) = &st.extensions {
        e.close().await;
    }
    st.skill_runtime.close();
    if let Some(a) = &st.accounts {
        a.close().await;
    }
    if let Some(h) = &st.mcp_host {
        h.close().await;
    }
    orion_ai::saidas::definir_destino(None);
}

#[cfg(feature = "http")]
async fn serve_state(
    st: estado::Estado,
    ouvinte: tokio::net::TcpListener,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<i32> {
    let (host, porta) = (&st.settings.host, st.settings.port);
    let tarefa_telegram = iniciar_fundo(&st);
    let app = rotas::router(st.clone());
    tracing::info!("Orion em http://{host}:{porta}");
    axum::serve(ouvinte, app.into_make_service_with_connect_info::<std::net::SocketAddr>()).with_graceful_shutdown(shutdown).await?;
    encerrar_fundo(&st, tarefa_telegram).await;
    Ok(0)
}

fn rodar(cli: Cli) -> anyhow::Result<i32> {
    if let Some(Cmd::LocalTest { model, gpu }) = &cli.cmd {
        use futures::StreamExt;
        return tokio::runtime::Runtime::new()?.block_on(async {
            let path = std::fs::canonicalize(model)?;
            let generator = embedded::generator(path, *gpu);
            let mut stream = generator(vec![serde_json::json!({"role": "system", "content": orion_ai::gateway::AVISO_RESERVA}), serde_json::json!({"role": "user", "content": "Responda apenas o resultado: 2 + 2. /no_think"})]);
            let mut text = String::new();
            let mut finished = false;
            while let Some(event) = stream.next().await {
                match event? {
                    orion_ai::gateway::Event::TextDelta(delta) => text.push_str(&delta),
                    orion_ai::gateway::Event::Finish { .. } => finished = true,
                    _ => anyhow::bail!("modelo local tentou usar ferramentas"),
                }
            }
            anyhow::ensure!(finished && !text.trim().is_empty(), "modelo não completou a resposta");
            println!("Teste local concluído; worker encerrado e pesos liberados.");
            Ok(0)
        });
    }
    if let Some(Cmd::ExtrairDocumento { arquivo, tipo }) = &cli.cmd {
        return Ok(orion_core::document_extract::principal(arquivo, tipo));
    }
    if let Some(Cmd::Update { check, apply, rollback }) = &cli.cmd {
        // roda na tarefa diária (SYSTEM), sem banco nem senha: antes de carregar configuração
        let mut args = Vec::new();
        for (on, flag) in [(*check, "--check"), (*apply, "--apply"), (*rollback, "--rollback")] {
            if on {
                args.push(flag.to_string());
            }
        }
        let pasta = std::env::current_exe()?.parent().map(PathBuf::from).unwrap_or_default();
        return Ok(orion_updater::executar(&args, &pasta));
    }
    let s = Settings::load()?;
    let cmd = cli.cmd.unwrap_or(Cmd::App);
    // comandos que cuidam da própria senha não passam pelo prompt automático
    let cuida_da_senha = matches!(cmd, Cmd::App | Cmd::ContaRecuperar | Cmd::BackupImportar { .. } | Cmd::BackupExportar { .. } | Cmd::DbCifrar { .. } | Cmd::DbSenha { .. } | Cmd::DbRecuperar { .. } | Cmd::DbNovaRecuperacao { .. } | Cmd::DbApagarClaro);
    if !cuida_da_senha {
        cli_db::unlock_if_needed(&s)?;
    }
    log::setup(&s.log_level, s.log_json, cli_db::encrypted_log(&s));
    match &cmd {
        Cmd::DbCifrar { stdin } => cli_db::cifrar(&s, *stdin),
        Cmd::DbSenha { stdin } => cli_db::trocar_senha(&s, *stdin),
        Cmd::DbRecuperar { stdin } => cli_db::recuperar(&s, *stdin),
        Cmd::DbNovaRecuperacao { stdin } => cli_db::nova_recuperacao(&s, *stdin),
        Cmd::DbApagarClaro => cli_db::apagar_claro(&s),
        Cmd::Logs { dia } => cli_db::ler_logs(&s, dia.as_deref()),
        Cmd::LocalTest { .. } => Ok(0),
        Cmd::ExtrairDocumento { .. } => Ok(0),
        Cmd::McpServidor { token, revogar } => cli_extra::mcp_servidor(&s, *token, *revogar),
        Cmd::Converter { arquivo, formato, saida } => cli_extra::converter(&s, arquivo, formato, saida.as_deref()),
        Cmd::Estilo { repos, agente, ativar, sim } => cli_extra::estilo(&s, repos, agente, *ativar, *sim),
        Cmd::Doctor => cli_extra::doctor(&s),
        Cmd::Autostart { plataforma, install, dir, force, ponte } => cli_extra::autostart(&s, plataforma.as_deref(), *install, dir.as_deref(), *force, *ponte),
        Cmd::McpCheck { config } => cli_extra::mcp_check(&s, config.as_deref()),
        Cmd::ImportSurreal { pasta, assistente } => cli_extra::import_surreal(&s, pasta, assistente),
        Cmd::VerifyExport { pasta, assistente, env, google_auth, vault, sem_ensaio } => {
            cli_extra::verify_export(pasta, assistente, env.as_deref(), google_auth.as_deref(), vault.as_deref(), *sem_ensaio)
        }
        Cmd::Transcrever { arquivo, titulo, sim, propor } => cli_extra::transcrever(&s, arquivo, titulo, *sim, *propor),
        Cmd::PossoDesligar => cli_extra::posso_desligar(&s),
        Cmd::Downloads { acao, simular, hoje, sim } => cli_extra::downloads(&s, acao, *simular, *hoje, *sim),
        Cmd::Rotina { acao, nome, sim } => cli_extra::rotina(&s, acao, nome.as_deref(), *sim),
        Cmd::Presenca { acao } => cli_extra::presenca(&s, acao),
        Cmd::Rosto { acao, nome, fotos } => cli_extra::rosto(&s, acao, nome, *fotos),
        Cmd::Legenda { segundos } => cli_extra::legenda(&s, *segundos),
        Cmd::WakeTest { listar, segundos } => cli_extra::wake_test(&s, *listar, *segundos),
        Cmd::App => appmode::run(s, cli.background),
        Cmd::Update { .. } => unreachable!("tratado antes da configuração"),
        #[cfg(feature = "http")]
        Cmd::Serve => tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(serve(s)),
        Cmd::ContaRecuperar => {
            let profile=orion_core::onboarding::Profile::load(&s.data_dir)?.ok_or_else(||anyhow::anyhow!("perfil de primeiro uso não encontrado"))?;
            let code=zeroize::Zeroizing::new(rpassword::prompt_password("Código do arquivo de recuperação: ")?);
            let password=zeroize::Zeroizing::new(rpassword::prompt_password("Nova senha do Orion: ")?);
            let confirmation=zeroize::Zeroizing::new(rpassword::prompt_password("Confirme a nova senha: ")?);
            anyhow::ensure!(*password==*confirmation && (12..=256).contains(&password.chars().count()),"use senhas iguais com 12 a 256 caracteres");
            let keypath=s.data_dir.join(if orion_core::dbopen::is_encrypted(&s.db_path()) {orion_core::dbkey::KEY_FILE}else{"orion-recovery.key"});
            let mut file=orion_core::dbkey::KeyFile::load(&keypath)?;let master=file.unlock_recovery(&code)?;file.change_password(&master,&password)?;file.save(&keypath)?;
            orion_core::auth::AuthService::open(&s.auth_db_path(),&profile.user,s.session_ttl_h as f64*3600.0)?.set_password(&password)?;
            let _=orion_ai::secrets::delete_secret("ORION_HELLO_LOGIN");
            println!("Senha recuperada. Entre com a nova senha; configure o Hello novamente se o utilizava.");Ok(0)
        },
        Cmd::BackupExportar { destino } => {
            let senha=zeroize::Zeroizing::new(rpassword::prompt_password("Senha do Orion: ")?);
            onboarding::unlock(&s,&senha)?;
            let key=onboarding::BACKUP_KEY.lock().clone().ok_or_else(||anyhow::anyhow!("este perfil não foi criado no primeiro uso"))?;
            let store=orion_core::dbopen::open_store(&s.db_path())?;
            let file=orion_core::onboarding_backup::export(&store,&s.data_dir,destino,&key)?;
            println!("Backup cifrado exportado: {}",file.display());Ok(0)
        },
        Cmd::BackupImportar { arquivo,pasta,recuperacao } => {
            let credential=zeroize::Zeroizing::new(rpassword::prompt_password(if *recuperacao {"Código de recuperação: "}else{"Senha do backup: "})?);
            let password=zeroize::Zeroizing::new(rpassword::prompt_password("Nova senha do Orion: ")?);
            let confirmation=zeroize::Zeroizing::new(rpassword::prompt_password("Confirme a nova senha: ")?);
            anyhow::ensure!(*password==*confirmation,"as senhas não coincidem");
            orion_core::onboarding_backup::restore(arquivo,pasta,&credential,*recuperacao,&password)?;
            println!("Backup conferido e restaurado em {}. Tokens e integrações precisam ser conectados novamente.",pasta.display());Ok(0)
        },
        Cmd::Backup { dir, keep } => {
            let store = orion_core::dbopen::open_store(&s.db_path())?;
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
            let store = orion_core::dbopen::open_store(&s.db_path())?;
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
            let store = orion_core::dbopen::open_store(&s.db_path())?;
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
        Cmd::Ponte { parear } => Ok(cli_extra::ponte(&s, *parear)),
        Cmd::Plugin { acao, alvo } => Ok(cli_extra::plugin(&s, acao, alvo.as_deref())),
        Cmd::Restore { arquivo, force } => {
            let contagens = match Store::verify_backup(arquivo) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("backup recusado: {e}");
                    return Ok(1);
                }
            };
            if s.db_path().exists() && !force {
                eprintln!(
                    "{} já existe; pare o Orion e use --force para substituir (o banco atual é perdido: faça `orion backup` antes)",
                    s.db_path().display()
                );
                return Ok(1);
            }
            Store::restore(arquivo, &s.db_path())?;
            let resumo: Vec<String> = contagens.iter().filter(|(_, n)| *n > 0).map(|(t, n)| format!("{t}={n}")).collect();
            println!("restaurado em {}: {}", s.db_path().display(), resumo.join(", "));
            Ok(0)
        }
    }
}

/// Executável de janela não tem console: para a CLI, anexa o do terminal que a chamou e liga a ele
/// só os fluxos que estiverem sem destino (saída redirecionada continua redirecionada).
#[cfg(windows)]
fn anexar_console() {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(n: u32) -> *mut c_void;
        fn SetStdHandle(n: u32, h: *mut c_void) -> i32;
        fn AttachConsole(pid: u32) -> i32;
        fn CreateFileW(nome: *const u16, acesso: u32, comp: u32, sa: *mut c_void, disp: u32, flags: u32, modelo: *mut c_void) -> *mut c_void;
    }
    const ENTRADA: u32 = -10i32 as u32;
    const SAIDA: u32 = -11i32 as u32;
    const ERRO: u32 = -12i32 as u32;
    const ANEXAR_PAI: u32 = u32::MAX;
    fn sem_destino(h: *mut c_void) -> bool {
        h.is_null() || h as isize == -1
    }
    unsafe {
        let faltando: Vec<u32> = [ENTRADA, SAIDA, ERRO].into_iter().filter(|n| sem_destino(GetStdHandle(*n))).collect();
        // tudo já tem destino (pipe, arquivo, console próprio), ou não há terminal pai (duplo clique)
        if faltando.is_empty() || AttachConsole(ANEXAR_PAI) == 0 {
            return;
        }
        let abrir = |nome: &str, acesso: u32| -> *mut c_void {
            let w: Vec<u16> = nome.encode_utf16().chain(std::iter::once(0)).collect();
            // GENERIC_READ/WRITE, FILE_SHARE_READ|WRITE, OPEN_EXISTING
            CreateFileW(w.as_ptr(), acesso, 3, std::ptr::null_mut(), 3, 0, std::ptr::null_mut())
        };
        for n in faltando {
            let h = if n == ENTRADA { abrir("CONIN$", 0x8000_0000) } else { abrir("CONOUT$", 0x4000_0000) };
            if !sem_destino(h) {
                SetStdHandle(n, h);
            }
        }
    }
}

#[cfg(windows)]
fn caixa_de_erro(texto: &str) {
    use std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(janela: *mut c_void, texto: *const u16, titulo: *const u16, tipo: u32) -> i32;
    }
    let w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    unsafe {
        MessageBoxW(std::ptr::null_mut(), w(texto).as_ptr(), w("Orion não abriu").as_ptr(), 0x10); // MB_ICONERROR
    }
}

fn main() {
    #[cfg(windows)]
    anexar_console();
    let codigo = match rodar(Cli::parse()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("erro: {e:#}");
            // sem console (duplo clique ou início com o Windows) ninguém leria o erro: mostra numa caixa
            #[cfg(windows)]
            if std::env::args().skip(1).all(|a| a == "--background" || a == "app") {
                caixa_de_erro(&format!("{e:#}"));
            }
            1
        }
    };
    std::process::exit(codigo);
}

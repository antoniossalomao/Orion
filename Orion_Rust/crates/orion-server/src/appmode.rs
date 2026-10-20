//! `orion app`: um só executável, um só processo e **nenhuma rede**. O núcleo roda aqui dentro e a
//! janela chama as rotas direto (sem socket, porta ou HTTP de verdade); o token é aleatório, só vive
//! na memória e serve apenas para a mesma checagem de login das rotas.
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request};
use futures::{future::BoxFuture, StreamExt};
use orion_app::net::{Corpo, Local, Resposta};
use orion_core::config::Settings;
use std::time::Duration;
use tower::ServiceExt;

/// O núcleo montado (estado + rotas). Fica vazio enquanto o banco cifrado está trancado.
struct Nucleo {
    router: axum::Router,
    st: crate::estado::Estado,
    telegram: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

type Slot = std::sync::Arc<parking_lot::Mutex<Option<std::sync::Arc<Nucleo>>>>;

/// Monta o núcleo e liga os trabalhos de fundo. Roda dentro do runtime do núcleo.
async fn montar_nucleo(s: Settings) -> anyhow::Result<std::sync::Arc<Nucleo>> {
    #[cfg(windows)]
    let vigiar = s.clone();
    let st = crate::estado::montar(s).await?;
    #[cfg(windows)]
    crate::cli_db::watch_screen_lock(&vigiar, st.auth.clone());
    let telegram = crate::iniciar_fundo(&st);
    Ok(std::sync::Arc::new(Nucleo { router: crate::rotas::router(st.clone()), st, telegram: parking_lot::Mutex::new(telegram) }))
}

struct Interno {
    nucleo: Slot,
    settings: Settings,
    rt: tokio::runtime::Handle,
    mostrar: std::path::PathBuf,
}

impl Interno {
    fn atual(&self) -> Option<std::sync::Arc<Nucleo>> {
        self.nucleo.lock().clone()
    }
}

impl Local for Interno {
    fn chamar(&self, metodo: &str, caminho: &str, corpo: Option<Vec<u8>>, bearer: Option<String>) -> BoxFuture<'static, Result<Resposta, String>> {
        let n = self.atual();
        let (metodo, caminho) = (metodo.to_string(), caminho.to_string());
        Box::pin(async move {
            let router = n.ok_or("O banco cifrado ainda está trancado.")?.router.clone();
            let mut req = Request::builder().method(Method::from_bytes(metodo.as_bytes()).map_err(|e| e.to_string())?).uri(&caminho).header(header::HOST, "127.0.0.1").header(header::ORIGIN, "http://127.0.0.1");
            if let Some(b) = bearer {
                req = req.header(header::AUTHORIZATION, format!("Bearer {b}"));
            }
            let tem_corpo = corpo.is_some();
            if tem_corpo {
                req = req.header(header::CONTENT_TYPE, "application/json");
            }
            let mut req = req.body(corpo.map(Body::from).unwrap_or_else(Body::empty)).map_err(|e| e.to_string())?;
            req.extensions_mut().insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
            let resp = router.oneshot(req).await.map_err(|e| e.to_string())?;
            let status = resp.status().as_u16();
            let set_cookie = resp.headers().get_all(header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()).find(|c| c.starts_with(orion_app::net::COOKIE)).map(String::from);
            let corpo: Corpo = resp.into_body().into_data_stream().map(|c| c.map(|b| b.to_vec()).map_err(|e| e.to_string())).boxed();
            Ok(Resposta { status, set_cookie, corpo })
        })
    }

    fn acordar(&self) -> Option<orion_app::net::Acordar> {
        use orion_app::net::Acordar;
        // outra instância pediu para mostrar a janela: deixa um arquivo, sem socket nem porta
        if self.mostrar.exists() && std::fs::remove_file(&self.mostrar).is_ok() {
            return Some(Acordar::Janela);
        }
        self.atual().is_some_and(|n| n.st.desktop_wake.swap(false, std::sync::atomic::Ordering::SeqCst)).then_some(Acordar::Voz)
    }

    fn panico(&self) -> Result<String, String> {
        let n = self.atual().ok_or("O banco cifrado ainda está trancado.")?;
        match n.st.modos.entrar_panico("atalho_desktop") {
            Ok(_) => Ok("Pânico ligado. Entre na conta e retome nas configurações.".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn bloqueado(&self) -> bool {
        self.atual().is_none()
    }

    fn desbloquear(&self, senha: String) -> BoxFuture<'static, Result<(), String>> {
        let (cfg, slot, rt) = (self.settings.clone(), self.nucleo.clone(), self.rt.clone());
        Box::pin(async move {
            if slot.lock().is_some() {
                return Ok(());
            }
            rt.spawn(async move {
                let db = cfg.db_path();
                let aberto = tokio::task::spawn_blocking(move || orion_core::dbopen::unlock_with_password(&db, &senha)).await.map_err(|e| e.to_string())?;
                aberto.map_err(|e| match e {
                    orion_core::dbkey::KeyError::Wrong => "Senha do banco incorreta.".to_string(),
                    e => e.to_string(),
                })?;
                let n = montar_nucleo(cfg).await.map_err(|e| e.to_string())?;
                *slot.lock() = Some(n);
                Ok(())
            })
            .await
            .map_err(|e| e.to_string())?
        })
    }
}

fn token_aleatorio() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Só uma cópia do Orion por pasta de dados: a segunda pede (por arquivo) que a primeira mostre a
/// janela e sai. A trava solta sozinha quando o processo termina.
struct Trava(#[allow(dead_code)] std::fs::File);

fn instancia_unica(dados: &std::path::Path) -> anyhow::Result<Option<Trava>> {
    std::fs::create_dir_all(dados)?;
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dados.join("orion.lock"))?;
    match f.try_lock() {
        Ok(()) => Ok(Some(Trava(f))),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
    }
}

/// Modo único do Orion: janela nativa + núcleo no mesmo processo, sem rede. `fundo` abre escondido
/// na bandeja (início com o Windows), salvo se o banco cifrado ainda pede a senha; clicar na bandeja ou abrir de novo mostra a janela.
pub fn run(mut s: Settings, fundo: bool) -> anyhow::Result<i32> {
    let dados = s.data_dir.clone();
    let mostrar = dados.join("orion.mostrar");
    let Some(_trava) = instancia_unica(&dados)? else {
        let _ = std::fs::write(&mostrar, b"1");
        return Ok(0);
    };
    if s.admin_token.is_empty() {
        s.admin_token = token_aleatorio();
    }
    let token = s.admin_token.clone();
    // banco cifrado sem senha em variável nem terminal: a janela pede a senha antes de abrir o núcleo
    let trancado = match crate::cli_db::unlock_if_needed(&s) {
        Ok(()) => false,
        Err(e) if orion_core::dbopen::is_encrypted(&s.db_path()) && !orion_core::dbopen::is_unlocked() => {
            tracing::info!("banco cifrado: {e}");
            true
        }
        Err(e) => return Err(e),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let slot: Slot = Default::default();
    if !trancado {
        *slot.lock() = Some(runtime.block_on(montar_nucleo(s.clone()))?);
    }
    let api = orion_app::net::Api::local(std::sync::Arc::new(Interno { nucleo: slot.clone(), settings: s, rt: runtime.handle().clone(), mostrar: mostrar.clone() }), token);
    let _ = std::fs::remove_file(&mostrar);
    // banco cifrado e trancado: mesmo no início com o Windows a janela abre já na tela de senha
    let fundo = fundo && !trancado;
    let r = orion_app::run::run_ui(Some(api), None, fundo);
    if let Some(n) = slot.lock().take() {
        let tel = n.telegram.lock().take();
        runtime.block_on(async {
            if tokio::time::timeout(Duration::from_secs(10), crate::encerrar_fundo(&n.st, tel)).await.is_err() {
                tracing::warn!("encerramento do núcleo passou de 10 s");
            }
        });
    }
    r?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use orion_app::net::{Api, Chat};
    use serde_json::json;

    fn interno_de_teste(st: crate::estado::Estado, dir: &std::path::Path) -> std::sync::Arc<Interno> {
        let n = std::sync::Arc::new(Nucleo { router: crate::rotas::router(st.clone()), st, telegram: Default::default() });
        std::sync::Arc::new(Interno { nucleo: std::sync::Arc::new(parking_lot::Mutex::new(Some(n))), settings: Settings::load().unwrap(), rt: tokio::runtime::Handle::current(), mostrar: dir.join("mostrar") })
    }

    /// Os testes mexem em variáveis de ambiente do processo: um de cada vez.
    static VEZ: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Chat de ponta a ponta pelo caminho sem rede. Só roda com um LLM de teste em `ORION_TEST_GATEWAY`
    /// (ex.: http://127.0.0.1:9099/v1, modelo `mock`).
    #[tokio::test(flavor = "multi_thread")]
    async fn chat_sem_rede_passa_pelas_rotas() {
        let Ok(gw) = std::env::var("ORION_TEST_GATEWAY") else { return };
        let _vez = VEZ.lock().await;
        let dir = std::env::temp_dir().join(format!("orion-appmode-{}", std::process::id()));
        std::env::set_var("ORION_DATA_DIR", &dir);
        std::env::set_var("ORION_GATEWAY_URL", gw);
        std::env::set_var("ORION_GATEWAY_MODEL", "mock");
        let mut s = Settings::load().expect("config");
        s.admin_token = token_aleatorio();
        let token = s.admin_token.clone();
        let st = crate::estado::montar(s).await.expect("estado");
        let api = Api::local(interno_de_teste(st, &dir), token);
        api.health().await.expect("health");
        assert!(api.auth_ok().await, "o token vale como login");
        let sessao = api.post("/sessoes", json!({})).await.unwrap();
        assert!(sessao["sessao_id"].as_str().is_some());
        let (mut texto, mut fim) = (String::new(), false);
        let mut ev = Box::pin(api.stream("/chat", json!({"texto": "oi, Orion", "canal": "web"})));
        while let Some(e) = ev.next().await {
            match e {
                Chat::Text(t) => texto.push_str(&t),
                Chat::Done { .. } => fim = true,
                Chat::Error(e) => panic!("erro no chat: {e}"),
                _ => {}
            }
        }
        assert!(fim && texto.contains("modelo de teste"), "texto: {texto:?}");
        let sem_token = Api::local(interno_de_teste(crate::estado::montar(Settings::load().unwrap()).await.unwrap(), &dir), "token-errado-0123456789".into());
        assert!(!sem_token.auth_ok().await, "token errado não autentica");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Aprovação ponta a ponta sem rede: o modelo pede `escrever_arquivo`, o app aprova, o turno retoma
    /// e o arquivo existe. Precisa do mock com `MOCK_PATH=/tmp/orion-teste-aprovado.txt`.
    #[tokio::test(flavor = "multi_thread")]
    async fn aprovar_escrita_sem_rede() {
        let Ok(gw) = std::env::var("ORION_TEST_GATEWAY") else { return };
        let _vez = VEZ.lock().await;
        let alvo = std::path::Path::new("/tmp/orion-teste-aprovado.txt");
        let _ = std::fs::remove_file(alvo);
        let dir = std::env::temp_dir().join(format!("orion-appmode-ap-{}", std::process::id()));
        std::env::set_var("ORION_DATA_DIR", &dir);
        // `escrever_arquivo` só existe com as ferramentas de desktop ligadas (opt-in)
        std::env::set_var("ORION_DESKTOP_TOOLS", "1");
        std::env::set_var("ORION_GATEWAY_URL", gw);
        std::env::set_var("ORION_GATEWAY_MODEL", "mock");
        let mut s = Settings::load().expect("config");
        s.admin_token = token_aleatorio();
        let token = s.admin_token.clone();
        let st = crate::estado::montar(s).await.expect("estado");
        let api = Api::local(interno_de_teste(st, &dir), token);
        api.post("/sessoes", json!({})).await.unwrap();
        let mut id = String::new();
        let mut ev = Box::pin(api.stream("/chat", json!({"texto": "ESCREVER", "canal": "web"})));
        while let Some(e) = ev.next().await {
            if let Chat::Approval { id: i, .. } = e {
                id = i;
            }
        }
        assert!(!id.is_empty(), "o modelo pediu aprovação");
        assert!(!alvo.exists(), "nada escrito antes de aprovar");
        let r = api.post(&format!("/approvals/{id}/decide"), json!({"approved": true})).await.unwrap();
        assert_eq!(r["status"], "approved");
        let mut texto = String::new();
        let mut tools = vec![];
        let mut ev = Box::pin(api.stream(&format!("/approvals/{id}/resume"), json!({"canal": "web"})));
        while let Some(e) = ev.next().await {
            match e {
                Chat::Text(t) => texto.push_str(&t),
                Chat::Tool(t) => tools.push(t),
                Chat::Error(e) => panic!("erro: {e}"),
                _ => {}
            }
        }
        assert!(tools.iter().any(|t| t["state"] == "completed"), "ferramenta concluída: {tools:?}");
        assert!(!texto.is_empty(), "o modelo relatou o resultado");
        assert!(alvo.exists(), "o arquivo foi escrito depois de aprovar");
        let _ = std::fs::remove_file(alvo);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Banco cifrado: o núcleo só abre depois da senha digitada na janela (senha errada recusa).
    #[cfg(feature = "sqlcipher")]
    #[tokio::test(flavor = "multi_thread")]
    async fn banco_cifrado_abre_pela_janela() {
        use orion_core::dbkey::Kdf;
        let _vez = VEZ.lock().await;
        let dir = std::env::temp_dir().join(format!("orion-appmode-db-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("ORION_DATA_DIR", &dir);
        let s = Settings::load().expect("config");
        // cria o banco em claro e cifra
        drop(crate::estado::montar(s.clone()).await.expect("estado"));
        let db = s.db_path();
        orion_core::dbopen::encrypt_database(&db, "senha-do-banco-123", Kdf::TESTE).expect("cifra");
        orion_core::dbopen::lock();
        assert!(orion_core::dbopen::is_encrypted(&db) && !orion_core::dbopen::is_unlocked());

        let slot: Slot = Default::default();
        let interno = std::sync::Arc::new(Interno { nucleo: slot, settings: s, rt: tokio::runtime::Handle::current(), mostrar: dir.join("mostrar") });
        let mut cfg_token = Settings::load().unwrap();
        cfg_token.admin_token = token_aleatorio();
        let api = Api::local(interno.clone(), "qualquer-token-0123456789".into());
        let _ = &interno;
        assert!(api.bloqueado());
        assert!(api.health().await.is_err(), "trancado não responde");
        assert_eq!(api.desbloquear("senha-errada".into()).await.unwrap_err(), "Senha do banco incorreta.");
        assert!(api.bloqueado());
        let _ = cfg_token;
        api.desbloquear("senha-do-banco-123".into()).await.expect("senha certa abre");
        assert!(!api.bloqueado());
        assert!(api.health().await.is_ok(), "depois de abrir, o núcleo responde");
        let _ = std::fs::remove_dir_all(dir);
    }
}

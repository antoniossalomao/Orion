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
    backup: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

static BOOT_PROGRESS: parking_lot::Mutex<Option<String>> = parking_lot::Mutex::new(None);
pub(crate) fn progress(message: &str) { *BOOT_PROGRESS.lock() = Some(message.into()); }

static BOOT_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static RECONFIGURING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(windows)]
static SCREEN_WATCH_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static NATIVE_THROTTLE: std::sync::LazyLock<orion_core::auth::LoginThrottle> = std::sync::LazyLock::new(Default::default);
static NATIVE_2FA: std::sync::LazyLock<orion_core::two_factor::TwoFactor> = std::sync::LazyLock::new(Default::default);
static NATIVE_PENDING: std::sync::LazyLock<parking_lot::Mutex<std::collections::HashMap<String, zeroize::Zeroizing<String>>>> = std::sync::LazyLock::new(Default::default);

type Slot = std::sync::Arc<parking_lot::Mutex<Option<std::sync::Arc<Nucleo>>>>;

/// Monta o núcleo e liga os trabalhos de fundo. Roda dentro do runtime do núcleo.
async fn preparar_nucleo(s: Settings,preserve_extensions:bool) -> anyhow::Result<std::sync::Arc<Nucleo>> {
    #[cfg(windows)]
    let vigiar = s.clone();
    let mut s = s;
    if let Some(p) = orion_core::onboarding::Profile::load(&s.data_dir)? {
        p.apply(&mut s);
    }
    let st = if preserve_extensions{crate::estado::montar_with_runtime(s,true).await?}else{crate::estado::montar(s).await?};
    // F02.05: observa o settings.toml e aplica as edições feitas à mão.
    crate::rotas::resources::registrar_nucleo(&st);
    #[cfg(windows)]
    if !SCREEN_WATCH_STARTED.swap(true,std::sync::atomic::Ordering::SeqCst) {
        crate::cli_db::watch_screen_lock(&vigiar, st.auth.clone());
    }
    Ok(std::sync::Arc::new(Nucleo {
        backup: Default::default(),
        router: crate::rotas::router(st.clone()),
        st,
        telegram: Default::default(),
    }))
}

fn iniciar_nucleo(n: &Nucleo) {
    *n.telegram.lock()=crate::iniciar_fundo(&n.st);
    *n.backup.lock()=crate::onboarding::start_backup(&n.st.settings,n.st.memory.clone(),n.st.modos.clone());
}
async fn montar_nucleo(s:Settings) -> anyhow::Result<std::sync::Arc<Nucleo>> {
    let n=preparar_nucleo(s,false).await?;
    iniciar_nucleo(&n);
    Ok(n)
}

async fn abrir_slot(slot: Slot, cfg: Settings) -> Result<(), String> {
    let _boot = BOOT_LOCK.lock().await;
    if slot.lock().is_none() {
        let n = montar_nucleo(cfg).await.map_err(|e| e.to_string())?;
        *slot.lock() = Some(n);
    }
    Ok(())
}

struct Interno {
    nucleo: Slot,
    settings: Settings,
    rt: tokio::runtime::Handle,
    mostrar: std::path::PathBuf,
    requires_login: bool,
}

impl Interno {
    fn atual(&self) -> Option<std::sync::Arc<Nucleo>> {
        self.nucleo.lock().clone()
    }
}

impl Local for Interno {
    fn reconfigure(&self,bearer:Option<String>) -> BoxFuture<'static,Result<serde_json::Value,String>> {
        let (slot,cfg,rt)=(self.nucleo.clone(),self.settings.clone(),self.rt.clone());
        Box::pin(async move {
            rt.spawn(async move {
                let _boot=BOOT_LOCK.lock().await;
                let old=slot.lock().clone().ok_or("Entre no Orion antes de configurar os recursos.")?;
                use subtle::ConstantTimeEq;
                if bearer.as_deref().is_none_or(|token| !bool::from(token.as_bytes().ct_eq(old.st.settings.admin_token.as_bytes()))) {
                    return Err("Entre no Orion antes de configurar os recursos.".into());
                }
                struct Reset;
                impl Drop for Reset { fn drop(&mut self) { RECONFIGURING.store(false,std::sync::atomic::Ordering::SeqCst); } }
                RECONFIGURING.store(true,std::sync::atomic::Ordering::SeqCst);
                let _reset=Reset;
                if old.st.audio.active() || old.st.agent.as_ref().is_some_and(|a|a.any_busy()) || !old.st.policy.approvals.pending(None,false).is_empty() {
                    return Err("Aguarde a resposta e resolva as aprovações antes de aplicar os recursos.".into());
                }
                let connections=old.st.mcp_host.as_ref().map(|host|host.list().into_iter().filter(|c|!old.st.extensions.as_ref().is_some_and(|manager|manager.dona_da_conexao(c.config.id()))).map(|c|c.config.clone()).collect::<Vec<_>>()).unwrap_or_default();
                let reviews=old.st.extensions.as_ref().map(|manager|manager.runtime_reviews()).unwrap_or_default();
                // Preserva o que a recarga da main já aplicou em memória antes de refazer o núcleo.
                let mut cfg=old.st.live.current.lock().clone().unwrap_or(cfg);
                cfg.injection_guard=old.st.live.injection.load(std::sync::atomic::Ordering::Relaxed);
                cfg.vault_diario=old.st.live.diary.load(std::sync::atomic::Ordering::Relaxed);
                if let Some(compactor)=old.st.agent.as_ref().and_then(|a|a.compactor.as_ref()){cfg.compact_at=compactor.at();}
                // Prepara o novo estado antes de desligar o antigo; falha não apaga o estado em uso.
                let next=match preparar_nucleo(cfg,true).await {
                    Ok(n)=>n,
                    Err(_)=>{
                        orion_ai::saidas::definir_destino(Some(orion_ai::saidas::destino_store(old.st.memory.clone())));
                        return Err("Configuração salva, mas não foi possível aplicá-la. Confira os recursos e tente novamente.".into());
                    }
                };
                if let Some(task)=old.backup.lock().take() { task.abort(); }
                let telegram=old.telegram.lock().take();
                crate::encerrar_fundo(&old.st,telegram).await;
                let mut warnings=Vec::new();
                if let Some(host)=&next.st.mcp_host{
                    for config in connections{
                        let id=config.id().to_string();
                        if let Some(connection)=host.remove(&id){connection.close().await;}
                        let connection=host.connection(config);host.insert(connection.clone());connection.start().await;
                        if next.st.settings.mcp_enabled && connection.config.enabled() && connection.state()!="connected"{warnings.push(format!("MCP {id}: use Testar para reconectar."));}
                    }
                }
                if let Some(manager)=&next.st.extensions{
                    // Só repete a revisão já concedida, para o mesmo hash e escopo; ativar valida o bundle novamente.
                    let mut pending=reviews;
                    loop{
                        let count=pending.len();if count==0{break;}
                        let mut deferred=Vec::new();
                        for review in pending{
                            let active=next.st.settings.plugins_enabled && (next.st.settings.mcp_enabled || !review.capabilities.iter().any(|c|c.starts_with("mcp:")||c.starts_with("resource:")||c.starts_with("prompt:")));
                            if !active{manager.retain_runtime_review(review);continue;}
                            match manager.activate(&review.id,&review.digest,&review.capabilities,review.trusted_local,review.authorized_remote,&review.classifications,&review.scope).await{
                                Ok(value) if value["state"].as_str()==Some("active")=>{},
                                Err(e) if e.0=="plugin_dependency_unavailable"=>deferred.push(review),
                                _=>{warnings.push(format!("Plugin {}: revise a conexão nas Configurações.",review.id));manager.retain_runtime_review(review);},
                            }
                        }
                        if deferred.len()==count{for review in deferred{warnings.push(format!("Plugin {}: revise as dependências nas Configurações.",review.id));manager.retain_runtime_review(review);}break;}
                        pending=deferred;
                    }
                }
                if let Some(catalog)=&next.st.catalog{let _=catalog.refresh().await;}
                *slot.lock()=Some(next.clone());
                orion_ai::saidas::definir_destino(Some(orion_ai::saidas::destino_store(next.st.memory.clone())));
                iniciar_nucleo(&next);
                Ok(serde_json::json!({"applied":true,"warnings":warnings}))
            }).await.map_err(|_|"Não foi possível aplicar os recursos.".to_string())?
        })
    }
    fn chamar(&self, metodo: &str, caminho: &str, corpo: Option<Vec<u8>>, bearer: Option<String>) -> BoxFuture<'static, Result<Resposta, String>> {
        let n = self.atual();
        let (metodo, caminho) = (metodo.to_string(), caminho.to_string());
        Box::pin(async move {
            if RECONFIGURING.load(std::sync::atomic::Ordering::SeqCst) { return Err("Aplicando recursos. Aguarde um instante.".into()); }
            let router = n.ok_or("O banco cifrado ainda está trancado.")?.router.clone();
            let mut req = Request::builder()
                .method(Method::from_bytes(metodo.as_bytes()).map_err(|e| e.to_string())?)
                .uri(&caminho)
                .header(header::HOST, "127.0.0.1")
                .header(header::ORIGIN, "http://127.0.0.1");
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
            let set_cookie = resp
                .headers()
                .get_all(header::SET_COOKIE)
                .iter()
                .filter_map(|v| v.to_str().ok())
                .find(|c| c.starts_with(orion_app::net::COOKIE))
                .map(String::from);
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
        self.atual()
            .is_some_and(|n| n.st.desktop_wake.swap(false, std::sync::atomic::Ordering::SeqCst))
            .then_some(Acordar::Voz)
    }

    fn panico(&self) -> Result<String, String> {
        let n = self.atual().ok_or("O banco cifrado ainda está trancado.")?;
        match n.st.modos.entrar_panico("atalho_desktop") {
            Ok(_) => Ok("Pânico ligado. Entre na conta e retome nas configurações.".into()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn entrar(&self, senha: String) -> BoxFuture<'static, Result<orion_app::net::LoginResult, String>> {
        let (cfg, slot, rt) = (self.settings.clone(), self.nucleo.clone(), self.rt.clone());
        Box::pin(async move {
            rt.spawn(async move {
                let profile = orion_core::onboarding::Profile::load(&cfg.data_dir).map_err(|e| e.to_string())?;
                let needs_2fa = profile.as_ref().is_some_and(|p| p.telegram_2fa);
                let senha = zeroize::Zeroizing::new(senha);
                let mut verify = cfg.clone();
                if let Some(p) = &profile {
                    verify.auth_user = p.user.clone();
                }
                let pw = senha.clone();
                tokio::task::spawn_blocking(move || {
                    if NATIVE_THROTTLE.espera("native") > 0 {
                        return Err("Muitas tentativas. Espere um pouco antes de tentar novamente.".into());
                    }
                    let auth = orion_core::auth::AuthService::open(&verify.auth_db_path(), &verify.auth_user, 3600.0).map_err(|_| "não foi possível abrir o login")?;
                    if auth.verify_credentials(&pw, "native", Some(&verify.auth_user)).is_err() {
                        NATIVE_THROTTLE.falhou("native");
                        return Err("usuário ou senha incorretos".into());
                    }
                    NATIVE_THROTTLE.acertou("native");
                    Ok::<(), String>(())
                })
                .await
                .map_err(|_| "não foi possível verificar a senha".to_string())??;
                if needs_2fa {
                    let (id, code) = NATIVE_2FA.issue("native").map_err(String::from)?;
                    let token = zeroize::Zeroizing::new(orion_ai::secrets::get_secret("ORION_TELEGRAM_TOKEN").ok_or("token do bot não configurado")?);
                    if !token.chars().all(|c| c.is_ascii_alphanumeric() || ":_-".contains(c)) {
                        return Err("token do bot inválido".into());
                    }
                    let user = profile.as_ref().and_then(|p| p.telegram_users.first()).ok_or("ID do Telegram não configurado")?;
                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(15))
                        .redirect(reqwest::redirect::Policy::none())
                        .build()
                        .map_err(|_| "falha ao iniciar Telegram")?;
                    let sent = client
                        .post(format!("https://api.telegram.org/bot{}/sendMessage", &*token))
                        .json(&serde_json::json!({"chat_id":user,"text":format!("Seu código para entrar no Orion: {code}. Válido por 5 minutos.")}))
                        .send()
                        .await;
                    let delivered = match sent {
                        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.is_ok_and(|v| v["ok"] == true),
                        _ => false,
                    };
                    if !delivered {
                        NATIVE_2FA.cancel(&id);
                        return Err("não foi possível entregar o código no Telegram; confira o bot ou use Hello".into());
                    }
                    NATIVE_PENDING.lock().clear();
                    NATIVE_PENDING.lock().insert(id.clone(), senha);
                    let expires = id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(300)).await;
                        NATIVE_PENDING.lock().remove(&expires);
                    });
                    return Ok(orion_app::net::LoginResult::TwoFactor(id));
                }
                crate::onboarding::unlock(&cfg, &senha).map_err(|_| "senha do banco incorreta".to_string())?;
                abrir_slot(slot, cfg).await?;
                Ok(orion_app::net::LoginResult::Ok)
            })
            .await
            .map_err(|_| "falha ao abrir o Orion".to_string())?
        })
    }
    fn confirmar_entrada(&self, challenge: String, code: String) -> BoxFuture<'static, Result<(), String>> {
        let (cfg, slot, rt) = (self.settings.clone(), self.nucleo.clone(), self.rt.clone());
        Box::pin(async move {
            if !NATIVE_2FA.verify(&challenge, &code, "native") {
                return Err("código incorreto ou expirado; tente novamente ou use Windows Hello".into());
            }
            let senha = NATIVE_PENDING.lock().remove(&challenge).ok_or("entrada expirada; digite sua senha de novo")?;
            rt.spawn(async move {
                crate::onboarding::unlock(&cfg, &senha).map_err(|_| "não foi possível abrir o banco")?;
                abrir_slot(slot, cfg).await?;
                Ok(())
            })
            .await
            .map_err(|_| "falha ao abrir o Orion".to_string())?
        })
    }
    fn initialize(&self) -> BoxFuture<'static, Result<(), String>> {
        let (slot, cfg, rt) = (self.nucleo.clone(), self.settings.clone(), self.rt.clone());
        let open = !self.requires_login && self.primeiro_uso().is_none();
        Box::pin(async move {
            if open { rt.spawn(abrir_slot(slot, cfg)).await.map_err(|e| e.to_string())??; }
            Ok(())
        })
    }
    fn progress(&self) -> Option<String> { BOOT_PROGRESS.lock().clone() }
    fn entrar_usuario(&self, usuario: String, senha: String) -> BoxFuture<'static, Result<orion_app::net::LoginResult, String>> {
        let expected = orion_core::onboarding::Profile::load(&self.settings.data_dir).ok().flatten().map(|p| p.user).unwrap_or_else(|| self.settings.auth_user.clone());
        if usuario != expected { return Box::pin(async { Err("Usuário ou senha incorretos.".into()) }); }
        self.entrar(senha)
    }
    fn logs_dir(&self) -> Option<std::path::PathBuf> { Some(self.settings.data_dir.join("logs")) }
    fn perfil(&self)->Option<serde_json::Value>{
        let p = match orion_core::onboarding::Profile::load(&self.settings.data_dir).ok().flatten() { Some(profile) => profile, None => {
            let language = std::fs::read(self.settings.data_dir.join("interface.json")).ok().filter(|bytes| bytes.len() <= 1024).and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok()).and_then(|value| value["language"].as_str().filter(|language| ["pt-BR", "en"].contains(language)).map(str::to_owned)).unwrap_or_else(|| "pt-BR".into());
            return Some(serde_json::json!({"user": self.settings.auth_user,"language":language}));
        } };
        Some(serde_json::json!({"user":p.user,"name":p.name,"language":p.language}))
    }
    fn cancelar_entrada(&self, challenge: &str) {
        NATIVE_2FA.cancel(challenge);
        NATIVE_PENDING.lock().remove(challenge);
    }
    fn hello_ativo(&self) -> bool {
        orion_core::onboarding::Profile::load(&self.settings.data_dir).ok().flatten().is_some_and(|p| p.hello)
    }
    fn hello(&self) -> BoxFuture<'static, Result<(), String>> {
        let (cfg, slot, rt) = (self.settings.clone(), self.nucleo.clone(), self.rt.clone());
        Box::pin(async move {
            rt.spawn(async move {
                let p = orion_core::onboarding::Profile::load(&cfg.data_dir)
                    .map_err(|e| e.to_string())?
                    .ok_or("perfil não configurado")?;
                if !p.hello {
                    return Err("Windows Hello não foi ativado".into());
                }
                let unlocked = cfg.clone();
                tokio::task::spawn_blocking(move || {
                    crate::onboarding_hello::verify()?;
                    let raw = zeroize::Zeroizing::new(orion_ai::secrets::get_secret("ORION_HELLO_LOGIN").ok_or("credencial Hello não encontrada; use sua senha")?);
                    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|_| "credencial Hello inválida")?;
                    if value["user"] != p.user {
                        return Err("credencial Hello pertence a outro perfil".into());
                    }
                    crate::onboarding::unlock(&unlocked, value["password"].as_str().ok_or("credencial Hello inválida")?)
                        .map_err(|_| "senha alterada; use sua senha e configure Hello novamente".to_string())
                })
                .await
                .map_err(|_| "falha ao verificar Windows Hello".to_string())??;
                abrir_slot(slot, cfg).await?;
                Ok(())
            })
            .await
            .map_err(|_| "falha ao abrir o Orion".to_string())?
        })
    }
    fn primeiro_uso(&self) -> Option<serde_json::Value> {
        crate::onboarding::needed(&self.settings)
            .ok()
            .filter(|v| *v)
            .map(|_| crate::onboarding::defaults(&self.settings))
    }

    fn testar_config(&self, form: serde_json::Value) -> BoxFuture<'static, Result<serde_json::Value, String>> {
        let allowed = self.primeiro_uso().is_some();
        Box::pin(async move {
            if !allowed {
                return Err("primeiro uso indisponível".into());
            }
            crate::onboarding::test_connection(form).await
        })
    }

    fn configurar(&self, form: serde_json::Value) -> BoxFuture<'static, Result<serde_json::Value, String>> {
        let (mut cfg, slot, rt) = (self.settings.clone(), self.nucleo.clone(), self.rt.clone());
        Box::pin(async move {
            rt.spawn(async move {
                let _boot = BOOT_LOCK.lock().await;
                if slot.lock().is_some() {
                    return Err("o núcleo já está aberto".into());
                }
                let senha = zeroize::Zeroizing::new(form["password"].as_str().unwrap_or("").to_string());
                let saved_cfg = cfg.clone();
                let result = tokio::task::spawn_blocking(move || crate::onboarding::finish(&saved_cfg, form))
                    .await
                    .map_err(|_| "falha ao preparar primeiro uso".to_string())?
                    .map_err(|e| e.to_string())?;
                if let Some(p) = orion_core::onboarding::Profile::load(&cfg.data_dir).map_err(|e| e.to_string())? {
                    p.apply(&mut cfg);
                }
                crate::onboarding::unlock(&cfg, &senha).map_err(|e| e.to_string())?;
                let n = montar_nucleo(cfg).await.map_err(|_| "configuração salva; reabra o Orion para entrar".to_string())?;
                *slot.lock() = Some(n);
                Ok(result)
            })
            .await
            .map_err(|_| "falha ao iniciar o Orion".to_string())?
        })
    }

    fn bloqueado(&self) -> bool {
        self.requires_login && self.primeiro_uso().is_none() && self.atual().is_none()
    }

    fn desbloquear(&self, senha: String) -> BoxFuture<'static, Result<(), String>> {
        let (cfg, slot, rt) = (self.settings.clone(), self.nucleo.clone(), self.rt.clone());
        Box::pin(async move {
            if slot.lock().is_some() {
                return Ok(());
            }
            rt.spawn(async move {
                let unlock_cfg = cfg.clone();
                let aberto = tokio::task::spawn_blocking(move || crate::onboarding::unlock(&unlock_cfg, &senha))
                    .await
                    .map_err(|e| e.to_string())?;
                aberto.map_err(|_| "Senha do banco incorreta.".to_string())?;
                abrir_slot(slot, cfg).await?;
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
        // Esta cópia é a que você acabou de abrir: deixa a primeira trazer a janela para a frente.
        #[cfg(windows)]
        orion_platform::allow_foreground();
        let _ = std::fs::write(&mostrar, b"1");
        return Ok(0);
    };
    if s.admin_token.is_empty() {
        s.admin_token = token_aleatorio();
    }
    let primeiro_uso = crate::onboarding::needed(&s)?;
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
    let trancado = trancado || (!primeiro_uso && (orion_core::onboarding::Profile::load(&s.data_dir)?.is_some() || crate::onboarding::login_protegido(&s)));
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let slot: Slot = Default::default();
    let api = orion_app::net::Api::local(
        std::sync::Arc::new(Interno {
            nucleo: slot.clone(),
            settings: s,
            rt: runtime.handle().clone(),
            mostrar: mostrar.clone(),
            requires_login: trancado,
        }),
        token,
    );
    let _ = std::fs::remove_file(&mostrar);
    // banco cifrado e trancado: mesmo no início com o Windows a janela abre já na tela de senha
    let fundo = fundo && !trancado && !primeiro_uso;
    // Atualização que ficou guardada (o fechamento anterior não chegou a trocar): troca já; vale na próxima abertura.
    instalar_atualizacao_guardada("na abertura");
    let r = orion_app::run::run_ui(Some(api), None, fundo);
    if let Some(n) = slot.lock().take() {
        if let Some(backup) = n.backup.lock().take() {
            backup.abort();
        }
        let tel = n.telegram.lock().take();
        runtime.block_on(async {
            if tokio::time::timeout(Duration::from_secs(10), crate::encerrar_fundo(&n.st, tel)).await.is_err() {
                tracing::warn!("encerramento do núcleo passou de 10 s");
            }
        });
    }
    *crate::onboarding::BACKUP_KEY.lock() = None;
    // "Instala ao fechar": o núcleo já encerrou, então é seguro trocar os arquivos.
    instalar_atualizacao_guardada("ao fechar");
    r?;
    Ok(0)
}

/// Troca os arquivos pela atualização baixada e conferida pelo atualizador, se houver. Falha só é registrada:
/// o app antigo continua valendo (os `.bak` e o rollback cobrem o resto).
fn instalar_atualizacao_guardada(quando: &str) {
    let Some(pasta) = std::env::current_exe().ok().and_then(|e| e.parent().map(std::path::PathBuf::from)) else { return };
    match orion_updater::apply_staged(&pasta) {
        Ok(Some(v)) => tracing::info!("atualização para {v} instalada {quando}"),
        Ok(None) => {}
        Err(e) => tracing::warn!("atualização guardada não instalada {quando} (a tarefa do Windows tenta de novo): {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use orion_app::net::{Api, Chat};
    use serde_json::json;

    fn interno_de_teste(st: crate::estado::Estado, dir: &std::path::Path) -> std::sync::Arc<Interno> {
        let n = std::sync::Arc::new(Nucleo {
            backup: Default::default(),
            router: crate::rotas::router(st.clone()),
            st,
            telegram: Default::default(),
        });
        std::sync::Arc::new(Interno {
            nucleo: std::sync::Arc::new(parking_lot::Mutex::new(Some(n))),
            settings: Settings::load().unwrap(),
            rt: tokio::runtime::Handle::current(),
            mostrar: dir.join("mostrar"),
            requires_login: true,
        })
    }

    /// Os testes mexem em variáveis de ambiente do processo: um de cada vez.
    static VEZ: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// O caminho nativo usa o vault configurado, exige login e informa ambas as versões em conflito.
    #[tokio::test(flavor = "multi_thread")]
    async fn vault_nativo_preserva_edicao_e_bloqueia_segredo_sem_login() {
        let _vez = VEZ.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault-escolhido");
        std::fs::create_dir_all(vault.join("01 Projetos/Orion")).unwrap();
        std::fs::write(vault.join("01 Projetos/Orion/Orion.md"), "---\ntype: projeto\n---\n# Orion\n").unwrap();
        let mut s = Settings::load().unwrap();
        s.data_dir = dir.path().join("dados");
        s.vault_dir = Some(vault.clone());
        s.admin_token = token_aleatorio();
        s.jobs_enabled = false;
        s.gateway_url.clear();
        s.gateway_model.clear();
        s.provedores.clear();
        s.telegram_token.clear();
        let token = s.admin_token.clone();
        let st = crate::estado::montar(s).await.unwrap();
        let interno = interno_de_teste(st, dir.path());
        let api = Api::local(interno.clone(), token);
        let invalid = Api::local(interno, "token-invalido-0123456789".into());
        let form = json!({"date":"2026-10-20","hora":"22:00","project_name":"Orion","title":"Teste nativo","summary":"Preserve a nota","tags":["orion"],"dialogue":"Conteúdo local"});
        assert!(invalid.post("/vault/save", form.clone()).await.is_err());
        assert!(!vault.join("03 Recursos").exists());
        let saved = api.post("/vault/save", form.clone()).await.unwrap();
        let original = vault.join(saved["file_path"].as_str().unwrap());
        assert!(original.is_file());
        std::fs::write(&original, "Alteração feita no Obsidian").unwrap();
        let conflict = api.post("/vault/save", form.clone()).await.unwrap();
        assert!(conflict["conflict"]["proposed_path"].as_str().is_some());
        assert_eq!(std::fs::read_to_string(original).unwrap(), "Alteração feita no Obsidian");
        let mut secret = form;
        secret["dialogue"] = json!(format!("github_pat_{}", "x".repeat(40)));
        assert!(api.post("/vault/save", secret).await.is_err());
    }

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
        let sem_token = Api::local(
            interno_de_teste(crate::estado::montar(Settings::load().unwrap()).await.unwrap(), &dir),
            "token-errado-0123456789".into(),
        );
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

    #[tokio::test(flavor = "multi_thread")]
    async fn primeiro_uso_nativo_cria_conta_antes_de_abrir_nucleo() {
        let _vez = VEZ.lock().await;
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::load().unwrap();
        s.data_dir = dir.path().join("dados");
        std::fs::create_dir(&s.data_dir).unwrap();
        s.admin_token = token_aleatorio();
        s.jobs_enabled = false;
        s.camera = false;
        s.wake_enabled = false;
        s.gateway_url.clear();
        s.gateway_model.clear();
        s.provedores.clear();
        s.telegram_token.clear();
        s.telegram_allowed_users.clear();
        let interno = std::sync::Arc::new(Interno {
            nucleo: Default::default(),
            settings: s.clone(),
            rt: tokio::runtime::Handle::current(),
            mostrar: dir.path().join("mostrar"),
            requires_login: true,
        });
        let api = Api::local(interno.clone(), s.admin_token.clone());
        assert!(api.primeiro_uso().is_some());
        assert!(api.health().await.is_err());
        let form = json!({"user":"pessoa","name":"Antônio","language":"pt-BR","password":"senha-de-teste-123","confirmation":"senha-de-teste-123","encrypted":cfg!(feature="sqlcipher"),"recovery":dir.path().join("recuperacao.txt"),"vault":"","projects":"","backup":"","telegram_users":"","telegram_2fa":false,"secrets":{},"imports":[]});
        api.configurar(form.clone()).await.unwrap();
        assert!(api.primeiro_uso().is_none());
        assert!(api.auth_ok().await);
        assert!(api.configurar(form).await.is_err());
        assert_eq!(interno.atual().unwrap().st.settings.auth_user, "pessoa");
        let next = std::sync::Arc::new(Interno {
            nucleo: Default::default(),
            settings: s.clone(),
            rt: tokio::runtime::Handle::current(),
            mostrar: dir.path().join("mostrar"),
            requires_login: true,
        });
        let next = Api::local(next, s.admin_token.clone());
        assert!(next.bloqueado());
        assert!(next.entrar("errada".into()).await.is_err());
        assert!(matches!(next.entrar("senha-de-teste-123".into()).await.unwrap(), orion_app::net::LoginResult::Ok));
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
        let interno = std::sync::Arc::new(Interno {
            nucleo: slot,
            settings: s,
            rt: tokio::runtime::Handle::current(),
            mostrar: dir.join("mostrar"),
            requires_login: true,
        });
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

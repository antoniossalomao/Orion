//! Áudio nativo sob pedido autenticado; conversa ao vivo não recebe memória nem ferramentas.
use crate::{estado::Estado, http::*};
use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[derive(Default)]
pub struct Audio {
    pub session: parking_lot::Mutex<Option<Arc<Session>>>,
}
pub struct Session {
    id: String,
    stop: Arc<AtomicBool>,
    finish: Arc<AtomicBool>,
    done: AtomicBool,
    state: parking_lot::Mutex<Value>,
    task: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}
impl Audio {
    pub fn cancel(&self) {
        if let Some(s) = self.session.lock().as_ref() {
            s.stop.store(true, Ordering::SeqCst);
            s.finish.store(true, Ordering::SeqCst);
        }
    }
    pub fn active(&self) -> bool {
        self.session
            .lock()
            .as_ref()
            .is_some_and(|s| !s.done.load(Ordering::SeqCst))
    }
}
impl Drop for Audio {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Start {
    #[serde(default)]
    live: bool,
}
pub async fn start(
    _: Admin,
    State(st): State<Estado>,
    Json(request): Json<Start>,
) -> ApiResult<Json<Value>> {
    if !cfg!(windows) {
        return erro(409, "captura nativa disponível no Windows");
    }
    if st.modos.panico() {
        return erro(409, "modo pânico");
    }
    if !(st.settings.voice_enabled && (!request.live || st.settings.voice_live_enabled)) {
        return erro(409, "habilite a voz nas Configurações");
    }
    if st.transcriber.is_none() {
        return erro(
            409,
            "configure o modelo Whisper local ou a transcrição gratuita",
        );
    }
    let session = {
        let mut current = st.audio.session.lock();
        if current
            .as_ref()
            .is_some_and(|s| !s.done.load(Ordering::SeqCst))
        {
            return erro(409, "microfone em uso");
        }
        let session = Arc::new(Session {
            id: uuid::Uuid::new_v4().simple().to_string(),
            stop: Arc::new(AtomicBool::new(false)),
            finish: Arc::new(AtomicBool::new(false)),
            done: AtomicBool::new(false),
            state: parking_lot::Mutex::new(json!({"status":"starting","live":request.live})),
            task: Default::default(),
        });
        st.ops
            .audit_add(&json!({"tool":"native_audio","action":"start","live":request.live}))?;
        *current = Some(session.clone());
        session
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let task_session = session.clone();
    let handle = tokio::spawn(async move {
        let audit = st.ops.clone();
        let runtime = st.clone();
        let watch = task_session.clone();
        let timeout = if request.live {
            st.settings.voice_live_max_min as u64 * 60
        } else {
            st.settings.wake_max_s as u64 + 120
        };
        let cancelled = async {
            let start = std::time::Instant::now();
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if watch.stop.load(Ordering::SeqCst)
                    || runtime.modos.panico()
                    || locked()
                    || start.elapsed().as_secs() >= timeout
                {
                    break;
                }
            }
        };
        let result = tokio::select! {result=run(st,task_session.clone(),request.live,tx)=>result,_=cancelled=>{task_session.stop.store(true,Ordering::SeqCst);*task_session.state.lock()=json!({"status":"stopped"});Ok(())}};
        let _ = audit.audit_add(&json!({"tool":"native_audio","action":"stop"}));
        if let Err(e) = result {
            *task_session.state.lock() = json!({"status":"error","error":e.to_string()});
        }
        task_session.done.store(true, Ordering::SeqCst);
    });
    *session.task.lock() = Some(handle);
    match tokio::time::timeout(std::time::Duration::from_secs(8), rx).await {
        Ok(Ok(Ok(()))) => Ok(Json(
            json!({"status":"recording","live":request.live,"capture_id":session.id}),
        )),
        Ok(Ok(Err(e))) => erro(409, e),
        _ => {
            session.stop.store(true, Ordering::SeqCst);
            erro(504, "o microfone não respondeu")
        }
    }
}
pub async fn status(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let value = st
        .audio
        .session
        .lock()
        .as_ref()
        .map(|session| {
            let mut value = session.state.lock().clone();
            value["capture_id"] = json!(session.id);
            value
        })
        .unwrap_or(json!({"status":"idle"}));
    Json(value)
}
pub async fn cancel(_: Admin, State(st): State<Estado>) -> Json<Value> {
    st.audio.cancel();
    Json(json!({"status":"stopped"}))
}
pub async fn finish(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    let session = st
        .audio
        .session
        .lock()
        .clone()
        .ok_or_else(|| ApiError::new(409, "nenhuma gravação"))?;
    session.finish.store(true, Ordering::SeqCst);
    let task = session.task.lock().take();
    if let Some(task) = task {
        tokio::time::timeout(std::time::Duration::from_secs(120), task)
            .await
            .map_err(|_| ApiError::new(504, "transcrição demorou demais"))?
            .map_err(anyhow::Error::from)?;
    }
    let mut value = session.state.lock().clone();
    value["capture_id"] = json!(session.id);
    Ok(Json(value))
}
async fn run(
    st: Estado,
    session: Arc<Session>,
    live: bool,
    ready: tokio::sync::oneshot::Sender<Result<(), String>>,
) -> anyhow::Result<()> {
    struct WakeGuard(Option<Arc<orion_core::wake::WakeListener>>);
    impl Drop for WakeGuard {
        fn drop(&mut self) {
            if let Some(wake) = &self.0 {
                wake.set_capture_active(false);
            }
        }
    }
    let _wake = WakeGuard(st.escuta.clone());
    if let Some(wake) = &_wake.0 {
        wake.set_capture_active(true);
    }
    let start = std::time::Instant::now();
    let mut pending = None;
    let mut ready = Some(ready);
    let mut history = vec![
        json!({"role":"system","content":"Você é Orion. Converse por voz em português. Esta sessão é temporária e não tem memória, ferramentas nem capacidade de aprovar ações. Não alegue executar ações."}),
    ];
    loop {
        if session.stop.load(Ordering::SeqCst) || st.modos.panico() || locked() {
            *session.state.lock() = json!({"status":"stopped"});
            break;
        }
        if live && start.elapsed().as_secs() >= st.settings.voice_live_max_min as u64 * 60 {
            *session.state.lock() = json!({"status":"stopped"});
            break;
        }
        let (capture_st, capture_session, device, ack) = (
            st.clone(),
            session.clone(),
            st.settings.wake_device.clone(),
            ready.take(),
        );
        let pcm = if let Some(pcm) = pending.take() {
            pcm
        } else {
            tokio::task::spawn_blocking(move || {
                capture(&device, &capture_st, &capture_session, live, ack)
            })
            .await??
        };
        if session.stop.load(Ordering::SeqCst) || st.modos.panico() || locked() {
            *session.state.lock() = json!({"status":"stopped"});
            break;
        }
        if pcm.is_empty() {
            if live {
                continue;
            }
            anyhow::bail!("gravação vazia");
        }
        *session.state.lock() = json!({"status":"transcribing","live":live});
        let audio = orion_core::wake::pcm_para_wav(&pcm, 16000);
        let text = st
            .transcriber
            .as_ref()
            .unwrap()
            .transcribe(audio, "voz.wav", "audio/wav")
            .await?;
        if session.stop.load(Ordering::SeqCst) || st.modos.panico() || locked() {
            break;
        }
        if !live {
            *session.state.lock() = json!({"status":"done","transcript":text});
            break;
        }
        history.push(json!({"role":"user","content":text}));
        if history.len() > 13 {
            history.drain(1..3);
        }
        *session.state.lock() = json!({"status":"thinking","live":true,"transcript":text});
        let gateway = st
            .gateway
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("configure um provedor para conversar"))?;
        let reply = gateway.complete(history.clone()).await?;
        if session.stop.load(Ordering::SeqCst) || st.modos.panico() || locked() {
            break;
        }
        history.push(json!({"role":"assistant","content":reply}));
        *session.state.lock() =
            json!({"status":"speaking","live":true,"transcript":text,"reply":reply});
        if let Some(speaker) = &st.speaker {
            let audio = speaker.synthesize(&reply).await?;
            let (play_st, play_session) = (st.clone(), session.clone());
            pending = tokio::task::spawn_blocking(move || play(&audio, &play_st, &play_session))
                .await??;
        }
        *session.state.lock() =
            json!({"status":"recording","live":true,"transcript":text,"reply":reply});
    }
    Ok(())
}
#[cfg(windows)]
fn capture(
    device: &str,
    st: &Estado,
    session: &Session,
    live: bool,
    ready: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
) -> anyhow::Result<Vec<u8>> {
    let mic = match orion_voice::Microphone::open(device) {
        Ok(m) => {
            if let Some(tx) = ready {
                let _ = tx.send(Ok(()));
            }
            m
        }
        Err(e) => {
            if let Some(tx) = ready {
                let _ = tx.send(Err(e.to_string()));
            }
            return Err(e);
        }
    };
    *session.state.lock() = json!({"status":"recording","live":live});
    let mut pcm = Vec::new();
    let mut speech = false;
    let mut silence = std::time::Instant::now();
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < st.settings.wake_max_s as u64
        && !session.stop.load(Ordering::SeqCst)
        && !session.finish.load(Ordering::SeqCst)
        && !st.modos.panico()
        && !locked()
    {
        if let Some(frame) = mic.read_frame() {
            let loud = orion_core::wake::rms(&frame) > 900.0;
            if loud {
                speech = true;
                silence = std::time::Instant::now();
            }
            pcm.extend(frame);
            if live && speech && silence.elapsed().as_secs_f64() >= st.settings.wake_silence_s {
                break;
            }
        }
    }
    if !speech {
        pcm.clear();
    }
    Ok(pcm)
}
#[cfg(not(windows))]
fn capture(
    _: &str,
    _: &Estado,
    _: &Session,
    _: bool,
    ready: Option<tokio::sync::oneshot::Sender<Result<(), String>>>,
) -> anyhow::Result<Vec<u8>> {
    if let Some(tx) = ready {
        let _ = tx.send(Err("captura nativa disponível no Windows".into()));
    }
    anyhow::bail!("captura nativa disponível no Windows")
}
#[cfg(windows)]
fn play(audio: &[u8], st: &Estado, session: &Session) -> anyhow::Result<Option<Vec<u8>>> {
    let abort = || session.stop.load(Ordering::SeqCst) || st.modos.panico() || locked();
    if st.settings.voice_barge_in {
        orion_voice::play_interruptible(audio, &st.settings.wake_device, 0, 900.0, &abort)
    } else {
        orion_voice::play_audio(audio, &abort)?;
        Ok(None)
    }
}
#[cfg(not(windows))]
fn play(_: &[u8], _: &Estado, _: &Session) -> anyhow::Result<Option<Vec<u8>>> {
    Ok(None)
}

#[cfg(windows)]
pub(crate) fn locked() -> bool {
    orion_platform::session_locked()
}
#[cfg(not(windows))]
pub(crate) fn locked() -> bool {
    false
}

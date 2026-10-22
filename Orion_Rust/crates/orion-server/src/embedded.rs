//! REGRAS.md: modelo local é reserva sem ferramentas; segredos nunca em processo filho.
use orion_ai::gateway::{EventStream, GatewayError, LocalGenerator};
#[cfg(windows)]
use orion_ai::gateway::Event;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(windows)]
use std::sync::LazyLock;

#[cfg(windows)]
static ONE_MODEL: LazyLock<Arc<tokio::sync::Semaphore>> = LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(1)));

fn failure(message: &str) -> GatewayError {
    GatewayError {
        mensagem: message.into(),
        tentativas: vec![],
    }
}

pub fn generator(model: PathBuf, gpu: bool) -> LocalGenerator {
    Arc::new(move |messages| {
        let (model, messages) = (model.clone(), messages);
        Box::pin(async_stream::stream! {
            #[cfg(windows)]
            {
                use orion_local::protocol::{Message, Request, Response, MAX_REQUEST, MAX_RESPONSE};
                use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
                use std::process::Stdio;
                let Ok(_permit) = ONE_MODEL.clone().acquire_owned().await else {
                    yield Err(failure("modelo local indisponível")); return;
                };
                let mut system = sysinfo::System::new();
                system.refresh_memory();
                let bytes = model.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
                if !orion_local::admitted(bytes, system.total_memory(), system.available_memory()) {
                    yield Err(failure("modelo não cabe no teto de 30% da RAM ou na memória disponível")); return;
                }
                let mut chat = Vec::new();
                for message in messages {
                    let role = message["role"].as_str().unwrap_or("");
                    if !["system", "assistant", "user"].contains(&role) { continue; }
                    let Some(content) = message["content"].as_str() else { continue; };
                    chat.push(Message { role: role.into(), content: content.into() });
                }
                let request = Request { model: model.clone(), messages: chat, gpu, context: 2048, max_tokens: 512 };
                if request.validate().is_err() {
                    yield Err(failure("pedido local inválido")); return;
                }
                let input = serde_json::to_vec(&request).unwrap_or_default();
                if input.len() as u64 > MAX_REQUEST {
                    yield Err(failure("conversa grande demais para o modelo local")); return;
                }
                let Ok(executable) = std::env::current_exe().map(|p| p.with_file_name("orion-llama.exe")) else { yield Err(failure("executável do Orion não encontrado")); return; };
                let mut command = tokio::process::Command::new(executable);
                command.arg("--worker").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).env_clear().kill_on_drop(true);
                for key in ["SystemRoot", "WINDIR", "PATH", "TEMP", "TMP"] {
                    if let Some(value) = std::env::var_os(key) { command.env(key, value); }
                }
                command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
                let Ok(mut child) = command.spawn() else { yield Err(failure("não foi possível iniciar o modelo local")); return; };
                let _job = match child.raw_handle().ok_or(()).and_then(|h| orion_local::job::MemoryJob::attach(h, orion_local::ram_limit(system.total_memory())).map_err(|_| ())) {
                    Ok(job) => job,
                    Err(_) => { let _ = child.kill().await; yield Err(failure("Windows recusou o teto de memória; modelo não carregado")); return; }
                };
                let mut stdin = child.stdin.take().unwrap();
                if stdin.write_all(&input).await.is_err() { yield Err(failure("worker local indisponível")); return; }
                drop(stdin); // EOF autoriza carregar, só depois de aplicar o teto.
                let mut lines = BufReader::new(child.stdout.take().unwrap().take((MAX_RESPONSE * 2) as u64)).lines();
                let deadline = tokio::time::sleep(std::time::Duration::from_secs(180));
                tokio::pin!(deadline);
                let mut done = false;
                loop {
                    let line = tokio::select! {
                        _ = &mut deadline => { yield Err(failure("modelo local excedeu o tempo limite")); return; },
                        line = lines.next_line() => match line { Ok(Some(line)) => line, _ => break }
                    };
                    match serde_json::from_str::<Response>(&line) {
                        Ok(Response::Text { text }) => yield Ok(Event::TextDelta(text)),
                        Ok(Response::Done) => { done = true; break; },
                        _ => { yield Err(failure("inferência local falhou; confira modelo, RAM e Vulkan")); return; }
                    }
                }
                let status = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
                if done && matches!(status, Ok(Ok(status)) if status.success()) {
                    yield Ok(Event::Finish { reason: "stop".into(), endpoint: "local".into(), model: model.file_name().unwrap_or_default().to_string_lossy().into_owned() });
                } else { yield Err(failure("worker local foi encerrado ou atingiu o teto de memória")); }
            }
            #[cfg(not(windows))]
            {
                let _ = (model, messages, gpu);
                yield Err(failure("inferência embarcada disponível no Windows"));
            }
        }) as EventStream<'static>
    })
}

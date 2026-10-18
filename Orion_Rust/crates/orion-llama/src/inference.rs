//! llama.cpp estático no worker; pesos/contexto são liberados quando ele termina.
use anyhow::ensure;
use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, LlamaChatMessage, LlamaModel},
    sampling::LlamaSampler,
};
use orion_local::protocol::{Request, Response, MAX_REQUEST};
use std::io::{Read, Write};
use std::num::NonZeroU32;

fn emit(event: Response) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &event)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

pub fn worker() -> anyhow::Result<()> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let mut input = Vec::new();
    std::io::stdin().take(MAX_REQUEST + 1).read_to_end(&mut input)?;
    ensure!(input.len() as u64 <= MAX_REQUEST, "pedido local grande demais");
    let request: Request = serde_json::from_slice(&input)?;
    request.validate()?;
    orion_local::job::require_current_limit(orion_local::ram_limit(system.total_memory()))?;
    let result = generate(&request);
    if result.is_err() {
        // Nunca imprimir prompt, caminho, pesos ou erro nativo no protocolo/log.
        emit(Response::Error {
            message: "inferência local falhou; confira modelo, memória e suporte Vulkan".into(),
        })?;
    }
    result
}

fn generate(r: &Request) -> anyhow::Result<()> {
    let mut backend = LlamaBackend::init()?;
    backend.void_logs();
    let gpu = r.gpu && cfg!(feature = "vulkan") && backend.supports_gpu_offload();
    ensure!(!r.gpu || gpu, "Vulkan indisponível; selecione CPU explicitamente");
    // Sem mmap: a memória comprometida dos pesos entra no teto imposto pelo Windows.
    let params = LlamaModelParams::default()
        .with_n_gpu_layers(if gpu { 999 } else { 0 })
        .with_use_mmap(false);
    let model = LlamaModel::load_from_file(&backend, &r.model, &params)?;
    let messages = r
        .messages
        .iter()
        .map(|m| LlamaChatMessage::new(m.role.clone(), m.content.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let template = model.chat_template(None)?;
    let prompt = model.apply_chat_template(&template, &messages, true)?;
    let vocab = model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), true, true);
    ensure!(
        !tokens.is_empty() && tokens.len() + r.max_tokens as usize <= r.context as usize,
        "conversa excede o contexto local; reduza-a antes de reenviar"
    );
    let threads = std::thread::available_parallelism().map(|v| v.get().min(8) as i32).unwrap_or(2);
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(r.context))
        .with_n_batch(256)
        .with_n_threads(threads)
        .with_n_threads_batch(threads);
    let mut context = model.new_context(&backend, params)?;
    let mut batch = LlamaBatch::new(256, 1);
    for (chunk_index, chunk) in tokens.chunks(256).enumerate() {
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let position = chunk_index * 256 + i;
            batch.add(*token, position as i32, &[0], position + 1 == tokens.len())?;
        }
        context.decode(&mut batch)?;
    }
    let mut sampler = LlamaSampler::greedy();
    let mut pending = Vec::new();
    for i in 0..r.max_tokens {
        let token = sampler.sample(&context, -1);
        if vocab.is_eog(token) {
            break;
        }
        pending.extend(vocab.token_to_piece(token, false, None));
        match std::str::from_utf8(&pending) {
            Ok(text) => {
                if !text.is_empty() {
                    emit(Response::Text { text: text.into() })?;
                }
                pending.clear();
            }
            Err(e) if e.error_len().is_some() => {
                emit(Response::Text {
                    text: String::from_utf8_lossy(&pending).into_owned(),
                })?;
                pending.clear();
            }
            Err(_) => {}
        }
        batch.clear();
        batch.add(token, (tokens.len() + i as usize) as i32, &[0], true)?;
        context.decode(&mut batch)?;
    }
    if !pending.is_empty() {
        emit(Response::Text {
            text: String::from_utf8_lossy(&pending).into_owned(),
        })?;
    }
    emit(Response::Done)
}

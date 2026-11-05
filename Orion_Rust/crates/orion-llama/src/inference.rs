//! llama.cpp estático no worker; pesos/contexto são liberados quando ele termina.
use anyhow::ensure;
use llama_cpp_2::{
    context::params::{KvCacheType, LlamaContextParams},
    context::LlamaContext,
    token::LlamaToken,
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
    let base = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(r.context))
        .with_n_batch(256)
        .with_n_threads(threads)
        .with_n_threads_batch(threads);
    // L7: cache de atenção em 8 bits exige atenção "flash"; se o backend recusar, volta ao f16
    // em vez de falhar (a resposta é a mesma, só gasta mais memória).
    let mut context = match r.kv_q8 {
        true => model
            .new_context(
                &backend,
                base.clone().with_type_k(KvCacheType::Q8_0).with_type_v(KvCacheType::Q8_0).with_flash_attention_policy(FLASH_ATTN_ENABLED),
            )
            .or_else(|_| model.new_context(&backend, base))?,
        false => model.new_context(&backend, base)?,
    };
    let mut batch = LlamaBatch::new(256, 1);
    let mut historico: Vec<i32> = tokens.iter().map(|t| t.0).collect();
    prefill(&mut context, &mut batch, &tokens)?;
    let mut sampler = LlamaSampler::greedy();
    let mut pending = Vec::new();
    let mut rascunho_max = r.rascunho as usize;
    let mut produzidos = 0u32;
    let mut proximo = sampler.sample(&context, -1);
    while produzidos < r.max_tokens {
        if vocab.is_eog(proximo) {
            break;
        }
        emitir(&vocab.token_to_piece(proximo, false, None), &mut pending)?;
        historico.push(proximo.0);
        produzidos += 1;
        if produzidos >= r.max_tokens {
            break;
        }
        // L1: propõe o que veio depois da última vez que estes tokens apareceram e confere tudo num
        // só passo. Só vale o que a amostragem gulosa escolheria; o resto é descartado do cache.
        let pos = historico.len() as i32 - 1;
        let faltam = (r.max_tokens - produzidos) as usize;
        let rascunho = orion_local::lookup::rascunho(&historico, rascunho_max.min(faltam).min(200));
        batch.clear();
        batch.add(proximo, pos, &[0], true)?;
        for (i, d) in rascunho.iter().enumerate() {
            batch.add(LlamaToken(*d), pos + 1 + i as i32, &[0], true)?;
        }
        context.decode(&mut batch)?;
        let mut aceitos = 0usize;
        let mut seguinte = sampler.sample(&context, 0);
        while aceitos < rascunho.len() && seguinte.0 == rascunho[aceitos] && !vocab.is_eog(seguinte) {
            emitir(&vocab.token_to_piece(seguinte, false, None), &mut pending)?;
            historico.push(seguinte.0);
            produzidos += 1;
            aceitos += 1;
            if produzidos >= r.max_tokens {
                break;
            }
            seguinte = sampler.sample(&context, aceitos as i32);
        }
        if aceitos < rascunho.len() {
            let primeiro_descartado = (pos + 1 + aceitos as i32) as u32;
            if context.kv_cache_seq_rm(0, Some(primeiro_descartado), None).is_err() {
                // Este modelo não aceita cortar o cache pela metade: refaz o cache só com o texto
                // aceito e deixa de propor rascunhos nesta resposta.
                context.clear_kv_cache();
                let aceitos_ate_aqui: Vec<LlamaToken> = historico.iter().map(|t| LlamaToken(*t)).collect();
                prefill(&mut context, &mut batch, &aceitos_ate_aqui)?;
                rascunho_max = 0;
            }
        }
        proximo = seguinte;
    }
    if !pending.is_empty() {
        emit(Response::Text {
            text: String::from_utf8_lossy(&pending).into_owned(),
        })?;
    }
    emit(Response::Done)
}

/// Valor de `LLAMA_FLASH_ATTN_TYPE_ENABLED` (llama.h).
const FLASH_ATTN_ENABLED: i32 = 1;

/// Manda os tokens ao modelo em blocos de 256; só o último pede os números da próxima palavra.
fn prefill(context: &mut LlamaContext<'_>, batch: &mut LlamaBatch, tokens: &[LlamaToken]) -> anyhow::Result<()> {
    for (chunk_index, chunk) in tokens.chunks(256).enumerate() {
        batch.clear();
        for (i, token) in chunk.iter().enumerate() {
            let position = chunk_index * 256 + i;
            batch.add(*token, position as i32, &[0], position + 1 == tokens.len())?;
        }
        context.decode(batch)?;
    }
    Ok(())
}

/// Envia o texto de um token, segurando bytes de um caractere UTF-8 ainda incompleto.
fn emitir(piece: &[u8], pending: &mut Vec<u8>) -> anyhow::Result<()> {
    pending.extend_from_slice(piece);
    match std::str::from_utf8(pending) {
        Ok(text) => {
            if !text.is_empty() {
                emit(Response::Text { text: text.into() })?;
            }
            pending.clear();
        }
        Err(e) if e.error_len().is_some() => {
            emit(Response::Text {
                text: String::from_utf8_lossy(pending).into_owned(),
            })?;
            pending.clear();
        }
        Err(_) => {}
    }
    Ok(())
}

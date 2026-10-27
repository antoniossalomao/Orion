//! Áudio anexado ao chat (F05.12): vira texto em segundo plano e entra como documento anexado.
//!
//! O envio devolve na hora (`processing`); a transcrição roda em tarefa própria e o app acompanha por
//! `GET /documents/<id>`, como em qualquer documento. A transcrição é dado externo (nunca instrução).
//! Whisper local primeiro; só com a chave de transcrição configurada o áudio pode sair do computador,
//! e então fica no registro de saídas (REGRAS.md: toda saída de dado fica registrada, sem conteúdo).

use super::documentos::{linha_de, validar};
use crate::estado::Estado;
use crate::http::*;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::Json;
use orion_ai::transcribe::MAX_AUDIO;
use orion_core::documents::MAX_TOTAL;
use orion_store::{with_scope, DataScope};
use serde::Deserialize;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

/// Teto do texto de uma transcrição indexada.
const MAX_TEXTO: usize = 400_000;

/// Formato pela assinatura dos primeiros bytes (extensão não vale).
pub fn tipo(b: &[u8]) -> Option<(&'static str, &'static str)> {
    let m4a = b.len() >= 12 && &b[4..8] == b"ftyp";
    if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WAVE" {
        Some((".wav", "audio/wav"))
    } else if b.starts_with(b"ID3") || (b.len() >= 2 && b[0] == 0xFF && b[1] & 0xE0 == 0xE0) {
        Some((".mp3", "audio/mpeg"))
    } else if b.starts_with(b"OggS") {
        Some((".ogg", "audio/ogg"))
    } else if b.starts_with(b"fLaC") {
        Some((".flac", "audio/flac"))
    } else if m4a {
        Some((".m4a", "audio/mp4"))
    } else if b.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        Some((".webm", "audio/webm"))
    } else {
        None
    }
}

fn wav_16k_mono(b: &[u8]) -> bool {
    // Cabeçalho canônico: PCM (1), mono, 16 kHz, 16 bits. Qualquer outra forma passa pelo ffmpeg.
    let u16_at = |i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    b.len() > 44 && &b[12..16] == b"fmt " && u16_at(20) == 1 && u16_at(22) == 1 && u32::from_le_bytes([b[24], b[25], b[26], b[27]]) == 16000 && u16_at(34) == 16
}

/// Converte para o WAV que o Whisper local lê (mono, 16 kHz). Sem `ffmpeg` ou se falhar, devolve `None`.
fn para_wav(origem: &[u8]) -> Option<Vec<u8>> {
    let exe = orion_core::util::which("ffmpeg")?;
    let dir = tempfile::tempdir().ok()?;
    let (entrada, saida) = (dir.path().join("entrada"), dir.path().join("saida.wav"));
    std::fs::File::create(&entrada).ok()?.write_all(origem).ok()?;
    let ok = Command::new(exe)
        .args(["-nostdin", "-loglevel", "error", "-y", "-i"])
        .arg(&entrada)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "-f", "wav"])
        .arg(&saida)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?
        .success();
    ok.then(|| std::fs::read(&saida).ok()).flatten().filter(|w| !w.is_empty() && w.len() <= MAX_AUDIO)
}

#[derive(Deserialize)]
pub struct AudioQ {
    name: String,
    project_id: Option<String>,
}

pub async fn enviar_audio(_: Admin, State(st): State<Estado>, Query(q): Query<AudioQ>, corpo: Bytes) -> ApiResult<Json<Value>> {
    validar(&st, &q.project_id)?;
    let nome = q.name.trim().to_string();
    if nome.is_empty() || nome.chars().count() > 160 || nome.chars().any(|c| (c as u32) < 32 || c == '/' || c == '\\') {
        return erro(422, "audio_name_invalid");
    }
    if corpo.is_empty() {
        return erro(422, "audio_empty");
    }
    if corpo.len() > MAX_AUDIO {
        return erro(413, "audio_size_limit");
    }
    let Some((kind, mime)) = tipo(&corpo) else { return erro(422, "audio_type_invalid") };
    if st.transcriber.is_none() {
        return erro(503, "transcricao_nao_configurada");
    }
    if st.modos.panico() {
        return erro(409, "modo pânico: transcrição bloqueada");
    }
    let id = orion_store::uuid_hex();
    let agora = st.memory.now();
    let r: ApiResult<()> = st.memory.tx(|c| {
        let total: i64 = c.query_row("SELECT COALESCE(sum(length(raw)),0) FROM uploads", [], |r| r.get(0))?;
        if total + corpo.len() as i64 > MAX_TOTAL {
            return Ok(Err(ApiError::new(413, "document_storage_limit")));
        }
        c.execute(
            "INSERT INTO uploads(id,project_id,name,kind,raw,status,created_at,progress) VALUES (?,?,?,?,?,?,?,0)",
            orion_store::rusqlite::params![id, q.project_id, nome, kind, corpo.to_vec(), "processing", agora],
        )?;
        Ok(Ok(()))
    })?;
    r?;

    transcrever_em_segundo_plano(&st, id.clone(), q.project_id.clone(), nome, corpo.to_vec(), mime)?;
    linha_de(&st, &q.project_id, &id)
}

/// Formatos de áudio que o upload aceita (o `kind` guardado em `uploads`).
pub fn e_audio(kind: &str) -> bool {
    [".wav", ".mp3", ".ogg", ".flac", ".m4a", ".webm"].contains(&kind)
}

/// Transcreve em tarefa própria e indexa o texto como documento. Serve ao envio e às operações de
/// manutenção (tentar de novo, reindexar, mover): áudio nunca passa pelo extrator de texto.
pub(super) fn transcrever_em_segundo_plano(st: &Estado, id: String, pid: Option<String>, nome: String, bytes: Vec<u8>, mime: &'static str) -> ApiResult<()> {
    let Some(transcriber) = st.transcriber.clone() else { return erro(503, "transcricao_nao_configurada") };
    let memory = st.memory.clone();
    tokio::spawn(async move {
        let (id2, nome2) = (id, nome);
        // O Whisper local só lê WAV mono 16 kHz: converte antes, quando houver ffmpeg.
        let entrada = tokio::task::spawn_blocking(move || if wav_16k_mono(&bytes) { bytes } else { para_wav(&bytes).unwrap_or(bytes) }).await.unwrap_or_default();
        let resultado = transcriber.transcribe(entrada, &nome2, mime).await;
        let fim = tokio::task::spawn_blocking(move || {
            let (status, erro) = match resultado {
                Ok(texto) => {
                    let texto: String = texto.chars().take(MAX_TEXTO).collect();
                    let r = with_scope(DataScope::project(pid.clone(), false), || memory.index_document(&format!("upload:{id2}/{nome2}"), &nome2, &texto));
                    match r {
                        Ok(_) => ("ready", None),
                        Err(e) => {
                            tracing::error!("indexação da transcrição falhou: {e}");
                            ("error", Some("transcription_index_failed".to_string()))
                        }
                    }
                }
                // A mensagem da transcrição nunca leva chave nem URL.
                Err(e) => ("error", Some(e.0)),
            };
            let _ = memory.tx(|c| {
                c.execute("UPDATE uploads SET status=?,error=?,progress=100 WHERE id=?", orion_store::rusqlite::params![status, erro, id2])?;
                Ok(())
            });
        })
        .await;
        if fim.is_err() {
            tracing::error!("tarefa de transcrição interrompida");
        }
    });
    Ok(())
}

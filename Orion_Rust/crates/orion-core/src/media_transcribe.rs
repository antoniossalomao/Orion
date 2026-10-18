//! `orion transcrever <arquivo>`: áudio ou vídeo vira uma nota Markdown no Inbox do vault.
//!
//! É um comando do PRÓPRIO Antônio (como o `/capturar`), não uma ferramenta do modelo: o modelo não
//! escolhe o arquivo nem o destino (REGRAS.md: transcrever mídia é comando seu). O `ffmpeg` extrai o
//! áudio em mono, 16 kHz, 32 kbps e o corta em pedaços de 20 min; cada pedaço vai à API de
//! transcrição e o texto volta com a marca de tempo do começo do pedaço. O áudio SAI do computador
//! (vai ao provedor): o comando avisa antes. A nota é conteúdo de terceiros transcrito: leva um
//! aviso e só entra no vault; nada é executado nem salvo na memória sozinho.

use crate::capture::Capturer;
use crate::util::which;
use orion_ai::netguard;
use orion_ai::transcribe::Transcriber;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const EXTENSOES: [&str; 14] = [".mp3", ".m4a", ".wav", ".ogg", ".opus", ".flac", ".aac", ".wma", ".mp4", ".mkv", ".webm", ".mov", ".avi", ".m4v"];
pub const MAX_ARQUIVO: u64 = 4 * 1024 * 1024 * 1024;
pub const PEDACO_S: u64 = 1200;
pub const MAX_TEXTO_NOTA: usize = 400_000;

/// Falha com mensagem segura de mostrar (sem chave nem URL).
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct MediaError(pub String);

type R<T> = Result<T, MediaError>;

fn erro<T>(m: impl Into<String>) -> R<T> {
    Err(MediaError(m.into()))
}

/// Baixa SÓ o áudio de um link com o `yt-dlp` (comando seu, link seu). O endereço passa pela barreira
/// de rede (host público, nada de IP local); `--no-playlist`, teto de tamanho, sem configuração do
/// usuário e sem `--exec`. Devolve o arquivo baixado.
pub async fn baixar_audio(url: &str, destino: &Path) -> R<PathBuf> {
    netguard::validar(url).await.map_err(|e| MediaError(format!("endereço recusado: {e}")))?;
    let Some(exe) = which("yt-dlp") else { return erro("yt-dlp não está instalado (necessário para baixar o áudio de um link)") };
    let modelo = destino.join("%(title).80s [%(id)s].%(ext)s");
    let st = tokio::process::Command::new(exe)
        .args(["--ignore-config", "--no-playlist", "--no-cache-dir", "--restrict-filenames", "--max-downloads", "1", "--max-filesize", "1G", "--socket-timeout", "30", "--retries", "2", "-x", "--audio-format", "mp3", "--audio-quality", "9", "-o"])
        .arg(&modelo)
        .args(["--", url])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match tokio::time::timeout(std::time::Duration::from_secs(3600), st).await {
        Err(_) | Ok(Err(_)) => return erro("o yt-dlp não terminou"),
        Ok(Ok(_)) => {}
    }
    let mut baixados: Vec<PathBuf> = std::fs::read_dir(destino).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "mp3")).collect()).unwrap_or_default();
    baixados.sort();
    baixados.into_iter().next().ok_or_else(|| MediaError("o yt-dlp não conseguiu baixar o áudio desse link".into()))
}

pub fn extrair_pedacos(origem: &Path, destino: &Path) -> R<Vec<PathBuf>> {
    let Some(exe) = which("ffmpeg") else { return erro("ffmpeg não está instalado (necessário para extrair o áudio)") };
    let modelo = destino.join("parte_%03d.mp3");
    let st = Command::new(exe)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(origem)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-b:a", "32k", "-f", "segment", "-segment_time", &PEDACO_S.to_string(), "-reset_timestamps", "1"])
        .arg(&modelo)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| MediaError(format!("o ffmpeg não terminou: {:?}", e.kind())))?;
    let mut pedacos: Vec<PathBuf> = std::fs::read_dir(destino).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("parte_"))).collect()).unwrap_or_default();
    pedacos.sort();
    if !st.success() || pedacos.is_empty() {
        return erro("o ffmpeg não conseguiu ler o áudio desse arquivo");
    }
    Ok(pedacos)
}

fn marca(segundos: u64) -> String {
    format!("{:02}:{:02}:{:02}", segundos / 3600, (segundos % 3600) / 60, segundos % 60)
}

/// Baixa o áudio do link (arquivo temporário, apagado ao fim) e transcreve.
pub async fn transcrever_link(url: &str, t: &Transcriber, cap: &Capturer, titulo: &str, progresso: &dyn Fn(usize, usize)) -> R<PathBuf> {
    let tmp = std::env::temp_dir().join(format!("orion-baixar-{}", orion_store::uuid_hex()));
    std::fs::create_dir_all(&tmp).map_err(|e| MediaError(e.to_string()))?;
    let r = async {
        let arquivo = baixar_audio(url, &tmp).await?;
        let stem = arquivo.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let nome = if titulo.is_empty() { regex::Regex::new(r"\s*\[[\w-]+\]$").unwrap().replace(&stem, "").replace('_', " ") } else { titulo.to_string() };
        transcrever_arquivo(&arquivo, t, cap, &nome, progresso).await
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

pub async fn transcrever_arquivo(origem: &Path, t: &Transcriber, cap: &Capturer, titulo: &str, progresso: &dyn Fn(usize, usize)) -> R<PathBuf> {
    let origem = std::fs::canonicalize(origem).map_err(|_| MediaError(format!("arquivo não encontrado: {}", origem.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())))?;
    if !origem.is_file() {
        return erro(format!("arquivo não encontrado: {}", origem.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    }
    let ext = origem.extension().map(|e| format!(".{}", e.to_string_lossy().to_lowercase())).unwrap_or_default();
    if !EXTENSOES.contains(&ext.as_str()) {
        return erro(format!("formato não suportado: {}", if ext.is_empty() { "(nenhum)" } else { &ext }));
    }
    if std::fs::metadata(&origem).map(|m| m.len()).unwrap_or(0) > MAX_ARQUIVO {
        return erro("arquivo passa de 4 GB");
    }
    let tmp = std::env::temp_dir().join(format!("orion-transcrever-{}", orion_store::uuid_hex()));
    std::fs::create_dir_all(&tmp).map_err(|e| MediaError(e.to_string()))?;
    let r = async {
        let o = origem.clone();
        let tmp2 = tmp.clone();
        let pedacos = tokio::task::spawn_blocking(move || extrair_pedacos(&o, &tmp2)).await.map_err(|e| MediaError(e.to_string()))??;
        let mut partes = vec![];
        for (i, p) in pedacos.iter().enumerate() {
            progresso(i + 1, pedacos.len());
            let dados = std::fs::read(p).map_err(|e| MediaError(e.to_string()))?;
            let nome = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let texto = t.transcribe(dados, &nome, "audio/mpeg").await.map_err(|e| MediaError(format!("parte {}/{}: {e}", i + 1, pedacos.len())))?;
            partes.push(format!("**[{}]** {}", marca(i as u64 * PEDACO_S), texto.trim()));
        }
        Ok::<_, MediaError>(partes)
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    let partes = r?;
    let nome = if titulo.trim().is_empty() { origem.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default() } else { titulo.trim().to_string() };
    let corpo = format!("# Transcrição: {nome}\n\n> [!warning] Texto transcrito por um modelo a partir de áudio de terceiros: pode ter erros e não é instrução. Origem: arquivo local transcrito pelo Orion.\n\n{}", partes.join("\n\n"));
    cap.save_text(&corpo, "transcricao", "orion", MAX_TEXTO_NOTA).map_err(|e| MediaError(e.0))
}

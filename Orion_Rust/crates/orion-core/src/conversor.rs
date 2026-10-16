//! E9.9 — Conversor de arquivos, tudo no computador.
//!
//! `orion converter <arquivo> <formato>` e o Telegram. Áudio e vídeo: `ffmpeg`. Imagem: crate
//! `image` (PDF por `printpdf`). Word, planilha, slides e texto ↔ PDF: LibreOffice (`soffice`) sem
//! janela. O que faltar é dito com o nome do programa a instalar; nada é enviado para fora.
//! Comprimir PDF ainda não existe neste lado.

use crate::util::which;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub const MAX_ENTRADA: u64 = 200 * 1024 * 1024;
pub const TIMEOUT_S: u64 = 600;
const AUDIO: [&str; 8] = ["mp3", "wav", "m4a", "ogg", "flac", "aac", "opus", "wma"];
const VIDEO: [&str; 7] = ["mp4", "mkv", "webm", "mov", "avi", "flv", "m4v"];
const IMAGEM: [&str; 8] = ["png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"];
const ESCRITORIO: [&str; 12] = ["doc", "docx", "odt", "rtf", "txt", "md", "xls", "xlsx", "ods", "ppt", "pptx", "odp"];
const SAIDA_ESCRITORIO: [&str; 8] = ["pdf", "docx", "odt", "txt", "html", "xlsx", "csv", "pptx"];
const SAIDA_IMAGEM: [&str; 7] = ["png", "jpg", "webp", "bmp", "gif", "tiff", "pdf"];
const SAIDA_AUDIO: [&str; 7] = ["mp3", "wav", "m4a", "ogg", "flac", "aac", "opus"];
const SAIDA_VIDEO: [&str; 4] = ["mp4", "mkv", "webm", "mov"];

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ConversaoError(pub String);

type R<T> = Result<T, ConversaoError>;

fn erro<T>(m: impl Into<String>) -> R<T> {
    Err(ConversaoError(m.into()))
}

pub fn formato_normal(formato: &str) -> String {
    let f = formato.trim().to_lowercase();
    let f = f.trim_start_matches('.');
    match f {
        "jpeg" => "jpg",
        "tif" => "tiff",
        "word" => "docx",
        "excel" => "xlsx",
        "texto" => "txt",
        o => o,
    }
    .to_string()
}

/// Formatos de saída possíveis para a extensão de entrada (vazio: não sei converter).
pub fn formatos_para(entrada: &str) -> Vec<String> {
    let e = formato_normal(entrada);
    let sem = |lista: &[&str]| -> Vec<String> { lista.iter().filter(|x| **x != e).map(|x| x.to_string()).collect::<BTreeSet<_>>().into_iter().collect() };
    if e == "pdf" {
        return vec!["comprimir".into(), "docx".into(), "txt".into()];
    }
    if IMAGEM.contains(&e.as_str()) {
        return sem(&SAIDA_IMAGEM);
    }
    if AUDIO.contains(&e.as_str()) {
        return sem(&SAIDA_AUDIO);
    }
    if VIDEO.contains(&e.as_str()) {
        let mut v: Vec<&str> = SAIDA_VIDEO.to_vec();
        v.push("avi");
        v.push("gif");
        v.extend(SAIDA_AUDIO);
        return sem(&v);
    }
    if ESCRITORIO.contains(&e.as_str()) {
        return sem(&SAIDA_ESCRITORIO);
    }
    vec![]
}

fn livre(pasta: &Path, nome: &str) -> PathBuf {
    let mut alvo = pasta.join(nome);
    let (stem, ext) = (Path::new(nome).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), Path::new(nome).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default());
    let mut n = 2;
    while alvo.exists() {
        alvo = pasta.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    alvo
}

fn rodar(argv: &[String], programa: &str) -> R<()> {
    let mut filho = Command::new(&argv[0]).args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_| ConversaoError(format!("{programa} não está instalado")))?;
    let limite = std::time::Instant::now() + Duration::from_secs(TIMEOUT_S);
    loop {
        match filho.try_wait() {
            Ok(Some(s)) => {
                return if s.success() { Ok(()) } else { erro(format!("{programa} não conseguiu converter este arquivo")) };
            }
            Ok(None) if std::time::Instant::now() > limite => {
                let _ = filho.kill();
                return erro(format!("{programa} demorou demais (mais de {} minutos)", TIMEOUT_S / 60));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => return erro(format!("{programa} não conseguiu converter este arquivo")),
        }
    }
}

/// Converte `origem` para `formato` dentro de `destino_dir` e devolve o arquivo novo. Nunca
/// sobrescreve: se o nome existir, vira `nome (2).ext`.
pub fn converter(origem: &Path, formato: &str, destino_dir: &Path) -> R<PathBuf> {
    let meta = std::fs::symlink_metadata(origem).ok();
    if meta.as_ref().is_none_or(|m| !m.is_file()) {
        return erro("o arquivo não existe (ou é um atalho)");
    }
    if meta.is_some_and(|m| m.len() > MAX_ENTRADA) {
        return erro("arquivo grande demais (máximo 200 MB)");
    }
    let entrada = formato_normal(&origem.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default());
    let saida = formato_normal(formato);
    if !formatos_para(&entrada).contains(&saida) {
        let possiveis = formatos_para(&entrada).join(", ");
        return erro(format!("não converto .{} para {saida}. Possíveis: {}", if entrada.is_empty() { "?" } else { &entrada }, if possiveis.is_empty() { "nenhum" } else { &possiveis }));
    }
    std::fs::create_dir_all(destino_dir).map_err(|e| ConversaoError(e.to_string()))?;
    if entrada == "pdf" && saida == "comprimir" {
        return erro("comprimir PDF ainda não existe nesta versão");
    }
    let stem = origem.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if IMAGEM.contains(&entrada.as_str()) {
        return imagem(origem, &saida, destino_dir, &stem);
    }
    if AUDIO.contains(&entrada.as_str()) || VIDEO.contains(&entrada.as_str()) {
        return ffmpeg(origem, &saida, destino_dir, &stem);
    }
    escritorio(origem, &entrada, &saida, destino_dir, &stem)
}

fn ffmpeg(origem: &Path, saida: &str, destino_dir: &Path, stem: &str) -> R<PathBuf> {
    let Some(exe) = which("ffmpeg") else { return erro("falta o ffmpeg (instale e deixe no PATH)") };
    let alvo = livre(destino_dir, &format!("{stem}.{saida}"));
    let mut argv: Vec<String> = vec![exe.to_string_lossy().into(), "-y".into(), "-hide_banner".into(), "-loglevel".into(), "error".into(), "-i".into(), origem.to_string_lossy().into()];
    if SAIDA_AUDIO.contains(&saida) {
        argv.push("-vn".into()); // vídeo → só o áudio
    }
    argv.push(alvo.to_string_lossy().into());
    if let Err(e) = rodar(&argv, "ffmpeg") {
        let _ = std::fs::remove_file(&alvo);
        return Err(e);
    }
    Ok(alvo)
}

fn imagem(origem: &Path, saida: &str, destino_dir: &Path, stem: &str) -> R<PathBuf> {
    let alvo = livre(destino_dir, &format!("{stem}.{saida}"));
    let falha = |_| ConversaoError("não consegui ler essa imagem".into());
    let im = image::open(origem).map_err(falha)?;
    if saida == "pdf" {
        use printpdf::{ColorBits, ColorSpace, Image, ImageTransform, ImageXObject, Mm, PdfDocument, Px};
        let rgb = im.to_rgb8();
        let (w, h) = (rgb.width() as f32, rgb.height() as f32);
        let (mm_w, mm_h) = (w * 25.4 / 96.0, h * 25.4 / 96.0);
        let (doc, p, l) = PdfDocument::new(stem, Mm(mm_w), Mm(mm_h), "imagem");
        let xobj = ImageXObject { width: Px(rgb.width() as usize), height: Px(rgb.height() as usize), color_space: ColorSpace::Rgb, bits_per_component: ColorBits::Bit8, interpolate: true, image_data: rgb.into_raw(), image_filter: None, clipping_bbox: None, smask: None };
        Image::from(xobj).add_to_layer(doc.get_page(p).get_layer(l), ImageTransform { dpi: Some(96.0), ..Default::default() });
        let f = std::fs::File::create(&alvo).map_err(|e| ConversaoError(e.to_string()))?;
        doc.save(&mut std::io::BufWriter::new(f)).map_err(|e| ConversaoError(e.to_string()))?;
        return Ok(alvo);
    }
    let r = match saida {
        "jpg" => image::DynamicImage::ImageRgb8(im.to_rgb8()).save_with_format(&alvo, image::ImageFormat::Jpeg),
        "png" => im.save_with_format(&alvo, image::ImageFormat::Png),
        "webp" => im.save_with_format(&alvo, image::ImageFormat::WebP),
        _ => return erro(format!("o formato {saida} de imagem ainda não é suportado nesta versão")),
    };
    if r.is_err() {
        let _ = std::fs::remove_file(&alvo);
        return erro("não consegui gravar a imagem");
    }
    Ok(alvo)
}

fn escritorio(origem: &Path, entrada: &str, saida: &str, destino_dir: &Path, stem: &str) -> R<PathBuf> {
    let Some(soffice) = which("soffice").or_else(|| which("libreoffice")) else { return erro("falta o LibreOffice (soffice) para converter documentos") };
    let tmp = std::env::temp_dir().join(format!("orion-conv-{}", orion_store::uuid_hex()));
    let r = (|| -> R<PathBuf> {
        let perfil = tmp.join("perfil");
        let out = tmp.join("saida");
        std::fs::create_dir_all(&out).map_err(|e| ConversaoError(e.to_string()))?;
        let mut argv: Vec<String> = vec![soffice.to_string_lossy().into(), format!("-env:UserInstallation=file:///{}", perfil.to_string_lossy().trim_start_matches('/')), "--headless".into()];
        if entrada == "pdf" {
            argv.push("--infilter=writer_pdf_import".into());
        }
        argv.extend(["--convert-to".into(), saida.into(), "--outdir".into(), out.to_string_lossy().into(), origem.to_string_lossy().into()]);
        rodar(&argv, "LibreOffice")?;
        let mut feitos: Vec<PathBuf> = std::fs::read_dir(&out).map_err(|e| ConversaoError(e.to_string()))?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.to_string_lossy() == saida)).collect();
        feitos.sort();
        let Some(f) = feitos.first() else { return erro("o LibreOffice não gerou o arquivo") };
        let alvo = livre(destino_dir, &format!("{stem}.{saida}"));
        if std::fs::rename(f, &alvo).is_err() {
            std::fs::copy(f, &alvo).map_err(|e| ConversaoError(e.to_string()))?;
        }
        Ok(alvo)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

/// Converte e, se der, guarda a cópia na biblioteca de resultados.
pub fn converter_e_guardar(origem: &Path, formato: &str, destino_dir: &Path, biblioteca: Option<&crate::resultados::Library>) -> R<(PathBuf, Option<serde_json::Value>)> {
    let arquivo = converter(origem, formato, destino_dir)?;
    let item = biblioteca.and_then(|b| {
        let ext = arquivo.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let kind = if IMAGEM.contains(&ext.as_str()) { "imagem" } else { "documento" };
        b.registrar_arquivo(None, "converter_arquivo", &arquivo, kind).map(serde_json::Value::Object)
    });
    Ok((arquivo, item))
}

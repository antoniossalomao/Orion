//! Captura rápida: o que o Antônio manda vira nota na caixa de entrada do vault.
//!
//! Não é ferramenta do modelo: é um comando do próprio Antônio, então o destino é fixo e estreito
//! (REGRAS.md: captura rápida e briefing não passam pelo modelo): só dentro de
//! `<vault>/<pasta de captura>`, nome de arquivo gerado aqui, um arquivo novo por captura (nunca
//! sobrescreve), texto até `MAX_TEXTO` e foto até `MAX_FOTO`.

use crate::util::{agora, local};
use std::io::Write;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub const MAX_TEXTO: usize = 20_000;
pub const MAX_FOTO: usize = 4 * 1024 * 1024;
pub const EXTENSOES_FOTO: [&str; 3] = [".jpg", ".png", ".webp"];

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct CaptureError(pub String);

type R<T> = Result<T, CaptureError>;

fn err<T>(m: impl Into<String>) -> R<T> {
    Err(CaptureError(m.into()))
}

/// Até 6 palavras em ASCII minúsculo, para o nome do arquivo.
pub fn resumo(texto: &str, padrao: &str) -> String {
    let ascii: String = texto.nfkd().filter(|c| c.is_ascii()).collect::<String>().to_lowercase();
    let palavras: Vec<&str> = ascii.split(|c: char| !c.is_ascii_alphanumeric()).filter(|p| !p.is_empty()).take(6).collect();
    let r: String = palavras.join("-").chars().take(50).collect();
    let r = r.trim_matches('-').to_string();
    if r.is_empty() { padrao.into() } else { r }
}

pub struct Capturer {
    vault: PathBuf,
    pasta: String,
    clock: Box<dyn Fn() -> f64 + Send + Sync>,
}

impl Capturer {
    pub fn new(vault: impl Into<PathBuf>, pasta: &str) -> Self {
        Capturer { vault: vault.into(), pasta: pasta.into(), clock: Box::new(agora) }
    }

    pub fn com_relogio(mut self, c: impl Fn() -> f64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(c);
        self
    }

    fn vault_real(&self) -> R<PathBuf> {
        let v = std::fs::canonicalize(&self.vault).map_err(|_| CaptureError(format!("o vault não existe: {}", self.vault.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())))?;
        if !v.is_dir() {
            return err("o vault não existe");
        }
        Ok(v)
    }

    /// A pasta de captura (criada se faltar), sempre dentro do vault.
    pub fn pasta(&self) -> R<PathBuf> {
        let vault = self.vault_real()?;
        let mut fundo = 0i32;
        for c in Path::new(&self.pasta).components() {
            match c {
                std::path::Component::Normal(_) => fundo += 1,
                std::path::Component::ParentDir => fundo -= 1,
                std::path::Component::CurDir => {}
                _ => fundo = -1,
            }
            if fundo < 0 {
                return err("a pasta de captura precisa ficar dentro do vault");
            }
        }
        let alvo = vault.join(&self.pasta);
        std::fs::create_dir_all(&alvo).map_err(|e| CaptureError(e.to_string()))?;
        let real = std::fs::canonicalize(&alvo).map_err(|e| CaptureError(e.to_string()))?;
        if real == vault || !real.starts_with(&vault) {
            return err("a pasta de captura precisa ficar dentro do vault");
        }
        Ok(real)
    }

    /// Cria `<base><ext>` (ou `<base> (2)<ext>`...) sem sobrescrever nada.
    fn gravar(pasta: &Path, base: &str, ext: &str, dado: &[u8]) -> R<PathBuf> {
        for n in 1..100 {
            let nome = if n == 1 { format!("{base}{ext}") } else { format!("{base} ({n}){ext}") };
            let p = pasta.join(nome);
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                Ok(mut f) => {
                    f.write_all(dado).map_err(|e| CaptureError(e.to_string()))?;
                    return Ok(p);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return err(e.to_string()),
            }
        }
        err("não consegui um nome livre para a nota")
    }

    fn carimbo(&self) -> (String, String, String) {
        let dt = local((self.clock)());
        (dt.format("%Y-%m-%d").to_string(), dt.format("%H:%M").to_string(), dt.format("%Y-%m-%d %H%M").to_string())
    }

    /// Grava uma nota pronta (com o próprio frontmatter); o título vira o nome do arquivo.
    pub fn save_markdown(&self, titulo: &str, conteudo: &str) -> R<PathBuf> {
        let (_, _, c) = self.carimbo();
        Self::gravar(&self.pasta()?, &format!("{c} - {}", resumo(titulo, "leitura")), ".md", conteudo.as_bytes())
    }

    /// `origem`: texto, link, voz, pesquisa ou transcricao (vai nas tags); `fonte`: telegram ou orion.
    pub fn save_text(&self, texto: &str, origem: &str, fonte: &str, limite: usize) -> R<PathBuf> {
        let texto = texto.trim();
        if texto.is_empty() {
            return err("nada para guardar");
        }
        if texto.chars().count() > limite {
            return err(format!("texto passa de {limite} caracteres"));
        }
        let (data, hora, c) = self.carimbo();
        let tag = if ["texto", "link", "voz", "pesquisa", "transcricao"].contains(&origem) { origem } else { "texto" };
        let fonte = if ["telegram", "orion"].contains(&fonte) { fonte } else { "telegram" };
        let nota = format!("---\ndate: {data}\nhora: {hora}\nfonte: {fonte}\ntags: [captura, {tag}]\n---\n\n{texto}\n");
        Self::gravar(&self.pasta()?, &format!("{c} - {}", resumo(texto, "captura")), ".md", nota.as_bytes())
    }

    /// Grava a foto em `anexos/` e uma nota que a incorpora (com a legenda, se houver).
    pub fn save_photo(&self, imagem: &[u8], ext: &str, legenda: &str) -> R<PathBuf> {
        let ext = ext.to_lowercase();
        if !EXTENSOES_FOTO.contains(&ext.as_str()) {
            return err(format!("formato de imagem não suportado: {}", if ext.is_empty() { "(nenhum)" } else { &ext }));
        }
        if imagem.is_empty() {
            return err("imagem vazia");
        }
        if imagem.len() > MAX_FOTO {
            return err(format!("imagem passa de {} MB", MAX_FOTO / 1024 / 1024));
        }
        let legenda: String = legenda.trim().chars().take(MAX_TEXTO).collect();
        let (data, hora, c) = self.carimbo();
        let pasta = self.pasta()?;
        let anexos = pasta.join("anexos");
        std::fs::create_dir_all(&anexos).map_err(|e| CaptureError(e.to_string()))?;
        let base = format!("{c} - {}", resumo(&legenda, "foto"));
        let foto = Self::gravar(&anexos, &base, &ext, imagem)?;
        let nome = foto.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let corpo = if legenda.is_empty() { String::new() } else { format!("\n{legenda}\n") };
        let nota = format!("---\ndate: {data}\nhora: {hora}\nfonte: telegram\ntags: [captura, foto]\n---\n\n![[{nome}]]\n{corpo}");
        Self::gravar(&pasta, &base, ".md", nota.as_bytes())
    }

    /// Desfaz uma captura: apaga a nota `.md` que ESTA pasta de captura criou. Recusa o que sai
    /// da pasta. `false` se já não existe.
    pub fn apagar_nota(&self, relativo: &str) -> R<bool> {
        let pasta = self.pasta()?;
        let alvo = self.vault_real()?.join(relativo);
        let alvo = match std::fs::canonicalize(&alvo) {
            Ok(a) => a,
            Err(_) => {
                // inexistente: ainda recusa o que não seria da pasta
                if alvo.extension().is_none_or(|e| e != "md") || alvo.parent().map(|p| p.components().collect::<PathBuf>()) != Some(pasta.clone()) {
                    return err("só desfaz notas da pasta de captura");
                }
                return Ok(false);
            }
        };
        if alvo.extension().is_none_or(|e| e != "md") || alvo.parent() != Some(pasta.as_path()) {
            return err("só desfaz notas da pasta de captura");
        }
        match std::fs::remove_file(&alvo) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => err(e.to_string()),
        }
    }

    /// Caminho da nota relativo ao vault (é o que se mostra no chat, nunca o absoluto).
    pub fn relativo(&self, caminho: &Path) -> String {
        let v = self.vault_real().unwrap_or_else(|_| self.vault.clone());
        caminho.strip_prefix(&v).unwrap_or(caminho).to_string_lossy().replace('\\', "/")
    }
}

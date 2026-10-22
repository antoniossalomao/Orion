//! Extrator em processo descartável, sem interpretar código/links dos documentos
//! (`orion extrair-documento`, chamado por `documents`).
//!
//! O andamento sai em linhas JSON `{"progress": N}` no stdout (N de 0 a 100) e a última linha é o
//! resultado: `{"text": ...}` ou `{"error": codigo}`.
//! PDF escaneado: esta versão não lê imagem de página (sem OCR); devolve `pdf_requires_ocr`.

use serde_json::json;
use std::io::Write;

pub const MAX_TEXT: usize = 2 * 1024 * 1024;
pub const MIN_TEXTO_PAGINA: usize = 20;

/// Texto do documento; o `Err` é um dos códigos públicos.
pub fn extract(raw: &[u8], kind: &str, progress: &mut dyn FnMut(u32)) -> Result<String, &'static str> {
    let texto = if kind == ".pdf" {
        let paginas = match pdf_extract::extract_text_from_mem_by_pages(raw) {
            Ok(p) => p,
            Err(e) => {
                let m = e.to_string().to_lowercase();
                return Err(if m.contains("encrypt") || m.contains("password") { "pdf_encrypted" } else { "document_invalid" });
            }
        };
        if paginas.len() > 200 {
            return Err("pdf_page_limit");
        }
        let total = paginas.len().max(1);
        let (mut partes, mut tamanho) = (vec![], 0usize);
        for (i, p) in paginas.iter().enumerate() {
            tamanho += p.len();
            if tamanho > MAX_TEXT {
                return Err("document_text_limit");
            }
            partes.push(p.clone());
            progress(((100 * (i + 1)) as f64 / total as f64).round() as u32);
        }
        let t = partes.join("\n\n");
        if t.trim().is_empty() || partes.iter().all(|p| p.trim().chars().count() < MIN_TEXTO_PAGINA) && t.trim().chars().count() < MIN_TEXTO_PAGINA {
            return Err("pdf_requires_ocr");
        }
        t
    } else {
        let bruto = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
        let t = std::str::from_utf8(bruto).map_err(|_| "document_invalid")?.to_string();
        if t.contains('\0') {
            return Err("document_invalid_text");
        }
        t
    };
    if texto.trim().is_empty() || texto.len() > MAX_TEXT {
        return Err("document_text_limit");
    }
    Ok(texto)
}

/// Ponto de entrada do subcomando: `arquivo tipo`. Devolve o código de saída.
pub fn principal(arquivo: &str, kind: &str) -> i32 {
    let out = std::io::stdout();
    let mut andamento = |pct: u32| {
        let mut o = out.lock();
        let _ = writeln!(o, "{}", json!({"progress": pct}));
        let _ = o.flush();
    };
    let resultado = match std::fs::read(arquivo) {
        Ok(raw) => match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| extract(&raw, kind, &mut andamento))) {
            Ok(Ok(t)) => json!({"text": t}),
            Ok(Err(c)) => json!({"error": c}),
            Err(_) => json!({"error": "document_invalid"}),
        },
        Err(_) => json!({"error": "document_invalid"}),
    };
    println!("{resultado}");
    0
}

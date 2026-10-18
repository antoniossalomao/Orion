//! O lado do servidor da ponte: a ponte conectada (no máximo uma), os comandos que o servidor lhe
//! manda e a captura do "O que é isso?".
//!
//! REGRAS.md: o servidor só consegue pedir `abrir` (uma tela do Orion) e `colar` (texto que o
//! Antônio já viu e aprovou na tela). Nada que leia a tela ou a área de transferência parte daqui.

use parking_lot::Mutex;
use rand::RngCore;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use tokio::sync::mpsc;

pub const MAX_COLAR: usize = 8000;
static ROTA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#/[a-z0-9/_-]{0,40}$").unwrap());

/// Comando fora da lista, ou com argumento que não passa na validação.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ComandoInvalido(pub String);

/// O comando normalizado (só as chaves conhecidas), ou o motivo da recusa.
pub fn validar(comando: &Value) -> Result<Value, ComandoInvalido> {
    let erro = |m: &str| ComandoInvalido(m.into());
    match comando.get("cmd").and_then(Value::as_str) {
        Some("abrir") => {
            let rota = comando.get("rota").and_then(Value::as_str).unwrap_or("#/");
            if !ROTA.is_match(rota) {
                return Err(erro("rota inválida"));
            }
            Ok(json!({"cmd": "abrir", "rota": rota}))
        }
        Some("colar") => match comando.get("texto").and_then(Value::as_str) {
            Some(t) if !t.trim().is_empty() && t.chars().count() <= MAX_COLAR => Ok(json!({"cmd": "colar", "texto": t})),
            _ => Err(ComandoInvalido(format!("texto vazio ou maior que {MAX_COLAR} caracteres"))),
        },
        _ => Err(erro("comando desconhecido")),
    }
}

pub type Fila = mpsc::Receiver<Option<Value>>;

/// Uma conexão por vez: parear de novo (ou abrir a ponte duas vezes) derruba a antiga.
pub struct PonteHub {
    atual: Mutex<Option<(u64, mpsc::Sender<Option<Value>>)>>,
    desde: Mutex<Option<f64>>,
    enviados: AtomicU64,
    geracao: AtomicU64,
}

impl Default for PonteHub {
    fn default() -> Self {
        PonteHub { atual: Mutex::new(None), desde: Mutex::new(None), enviados: AtomicU64::new(0), geracao: AtomicU64::new(0) }
    }
}

impl PonteHub {
    pub fn conectada(&self) -> bool {
        self.atual.lock().is_some()
    }

    /// Chamada pelo handler do WebSocket. Devolve o identificador da conexão e a fila.
    pub fn anexar(&self) -> (u64, Fila) {
        let (tx, rx) = mpsc::channel(50);
        let id = self.geracao.fetch_add(1, Ordering::SeqCst) + 1;
        let antiga = self.atual.lock().replace((id, tx));
        *self.desde.lock() = Some(crate::util::agora());
        if let Some((_, a)) = antiga {
            let _ = a.try_send(None); // o handler antigo fecha o socket dele
        }
        (id, rx)
    }

    pub fn soltar(&self, id: u64) {
        let mut g = self.atual.lock();
        if g.as_ref().is_some_and(|(i, _)| *i == id) {
            *g = None;
            *self.desde.lock() = None;
        }
    }

    /// Valida e enfileira. `Ok(false)` se não há ponte conectada. Seguro de qualquer thread (a
    /// escuta de palmas chama daqui).
    pub fn enviar(&self, comando: &Value) -> Result<bool, ComandoInvalido> {
        let pronto = validar(comando)?;
        let Some(tx) = self.atual.lock().as_ref().map(|(_, t)| t.clone()) else { return Ok(false) };
        // ponte travada: melhor perder o comando que acumular
        if tx.try_send(Some(pronto)).is_ok() {
            self.enviados.fetch_add(1, Ordering::SeqCst);
        }
        Ok(true)
    }

    pub fn estado(&self) -> Value {
        json!({"conectada": self.conectada(), "desde": *self.desde.lock(), "enviados": self.enviados.load(Ordering::SeqCst)})
    }
}

// ── captura do "O que é isso?" ─────────────────────────────────────────────

pub const TTL_S: f64 = 300.0;
pub const MAX_GUARDADAS: usize = 3;

pub fn tipo_da_imagem(d: &[u8]) -> Option<&'static str> {
    if d.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if d.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if d.len() >= 12 && &d[..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// A captura fica **só na memória** do servidor local, por 5 minutos; nada vai a disco.
#[derive(Default)]
pub struct Imagens {
    itens: Mutex<HashMap<String, (f64, Vec<u8>, &'static str)>>,
}

impl Imagens {
    fn podar(m: &mut HashMap<String, (f64, Vec<u8>, &'static str)>) {
        let agora = crate::util::agora();
        m.retain(|_, v| agora - v.0 < TTL_S);
        while m.len() > MAX_GUARDADAS {
            if let Some(k) = m.iter().min_by(|a, b| a.1 .0.total_cmp(&b.1 .0)).map(|(k, _)| k.clone()) {
                m.remove(&k);
            }
        }
    }

    pub fn guardar(&self, dados: &[u8], max: usize) -> Result<String, String> {
        if dados.is_empty() {
            return Err("imagem vazia".into());
        }
        if dados.len() > max {
            return Err(format!("imagem passa de {} MB", max / 1024 / 1024));
        }
        let mime = tipo_da_imagem(dados).ok_or("só JPEG, PNG ou WebP")?;
        let mut b = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut b);
        let id = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, b);
        let mut g = self.itens.lock();
        g.insert(id.clone(), (crate::util::agora(), dados.to_vec(), mime));
        Self::podar(&mut g);
        Ok(id)
    }

    pub fn pegar(&self, id: &str) -> Option<(Vec<u8>, &'static str)> {
        let mut g = self.itens.lock();
        Self::podar(&mut g);
        g.get(id).map(|v| (v.1.clone(), v.2))
    }
}

// ── cliente da ponte: teclas, OCR e área (E2.1, E2.4, E2.5) ───────────────

pub const MODIFICADORES: [&str; 4] = ["ctrl", "alt", "shift", "cmd"];
static TECLA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([a-z0-9]|f([1-9]|1[0-2])|space|tab|enter|esc)$").unwrap());
static LINHAS_VAZIAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());
pub const MIN_LADO_PX: i32 = 8;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct TeclasInvalidas(pub String);

pub fn padrao_teclas() -> HashMap<String, String> {
    HashMap::from([
        ("captura".into(), "ctrl+alt+space".into()),
        ("ocr".into(), "ctrl+alt+t".into()),
        ("isso".into(), "ctrl+alt+e".into()),
        ("panico".into(), "ctrl+alt+shift+p".into()),
    ])
}

pub fn normalizar_tecla(comb: &str) -> Result<String, TeclasInvalidas> {
    let mut partes: Vec<String> = vec![];
    for p in comb.to_lowercase().split('+') {
        let s = p.trim().to_string();
        if !s.is_empty() && !partes.contains(&s) {
            partes.push(s);
        }
    }
    let mut ord: Vec<String> = MODIFICADORES.iter().filter(|&&m| partes.iter().any(|p| p == m)).map(|&m| m.to_string()).collect();
    let resto: Vec<String> = partes.iter().filter(|p| !MODIFICADORES.contains(&p.as_str())).cloned().collect();
    if ord.is_empty() {
        return Err(TeclasInvalidas(format!("'{comb}' precisa de pelo menos um modificador (ctrl, alt, shift ou cmd)")));
    }
    if resto.len() != 1 || !TECLA.is_match(&resto[0]) {
        return Err(TeclasInvalidas(format!("'{comb}' precisa terminar numa tecla (a–z, 0–9, f1–f12, space, tab, enter, esc)")));
    }
    ord.push(resto[0].clone());
    Ok(ord.join("+"))
}

pub fn mapa_de_teclas(bruto: &str) -> Result<HashMap<String, String>, TeclasInvalidas> {
    let mut saida = padrao_teclas();
    if bruto.trim().is_empty() {
        return Ok(saida);
    }
    let dado: Value = serde_json::from_str(bruto).map_err(|_| TeclasInvalidas("ORION_HOTKEYS precisa ser um JSON válido".into()))?;
    let Some(obj) = dado.as_object() else {
        return Err(TeclasInvalidas("ORION_HOTKEYS precisa ser um objeto JSON".into()));
    };
    for (acao, comb) in obj {
        if !saida.contains_key(acao) {
            let mut validas: Vec<_> = saida.keys().cloned().collect();
            validas.sort();
            return Err(TeclasInvalidas(format!("ação desconhecida em ORION_HOTKEYS: {acao:?} (válidas: {})", validas.join(", "))));
        }
        let Some(s) = comb.as_str() else {
            return Err(TeclasInvalidas(format!("combinação de {acao:?} precisa ser texto")));
        };
        saida.insert(acao.clone(), normalizar_tecla(s)?);
    }
    let mut unicos = std::collections::HashSet::new();
    for v in saida.values() {
        if !unicos.insert(v) {
            return Err(TeclasInvalidas("duas ações não podem ter a mesma combinação de teclas".into()));
        }
    }
    Ok(saida)
}

pub fn para_pynput(comb: &str) -> String {
    comb.split('+')
        .map(|p| if p.len() == 1 { p.to_string() } else { format!("<{p}>") })
        .collect::<Vec<_>>()
        .join("+")
}

pub fn limpar_texto_ocr(bruto: &str) -> String {
    let norm = bruto.replace("\r\n", "\n").replace('\r', "\n");
    let linhas: Vec<&str> = norm.split('\n').map(str::trim_end).collect();
    LINHAS_VAZIAS.replace_all(linhas.join("\n").trim(), "\n\n").into_owned()
}

pub fn normalizar_area(x1: i32, y1: i32, x2: i32, y2: i32) -> Option<(i32, i32, i32, i32)> {
    let (x, y) = (x1.min(x2), y1.min(y2));
    let (w, h) = ((x2 - x1).abs(), (y2 - y1).abs());
    (w >= MIN_LADO_PX && h >= MIN_LADO_PX).then_some((x, y, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapa_padrao_e_troca_por_json() {
        let p = padrao_teclas();
        assert_eq!(mapa_de_teclas("").unwrap(), p);
        let troca = mapa_de_teclas(r#"{"captura": "Ctrl+Shift+K"}"#).unwrap();
        assert_eq!(troca["captura"], "ctrl+shift+k");
        assert_eq!(troca["ocr"], p["ocr"]);
        assert_eq!(para_pynput("ctrl+alt+space"), "<ctrl>+<alt>+<space>");
        assert_eq!(para_pynput("ctrl+shift+k"), "<ctrl>+<shift>+k");
        assert_eq!(normalizar_tecla("ALT + ctrl + alt + F5").unwrap(), "ctrl+alt+f5");
    }

    #[test]
    fn teclas_invalidas_sao_recusadas() {
        for bruto in [
            "não é json",
            "[1]",
            r#"{"voar": "ctrl+alt+v"}"#,
            r#"{"captura": "k"}"#,
            r#"{"captura": "ctrl+alt"}"#,
            r#"{"captura": "ctrl+a+b"}"#,
            r#"{"captura": 3}"#,
            r#"{"captura": "ctrl+alt+t"}"#,
        ] {
            assert!(mapa_de_teclas(bruto).is_err(), "{bruto}");
        }
    }

    #[test]
    fn validar_aceita_so_abrir_e_colar() {
        assert_eq!(validar(&json!({"cmd": "abrir", "rota": "#/chat"})).unwrap(), json!({"cmd": "abrir", "rota": "#/chat"}));
        assert_eq!(validar(&json!({"cmd": "abrir"})).unwrap(), json!({"cmd": "abrir", "rota": "#/"}));
        assert_eq!(validar(&json!({"cmd": "colar", "texto": "oi", "extra": 1})).unwrap(), json!({"cmd": "colar", "texto": "oi"}));
        for ruim in [
            json!({"cmd": "executar", "texto": "rm -rf /"}),
            json!({"cmd": "abrir", "rota": "javascript:alert(1)"}),
            json!({"cmd": "abrir", "rota": "#/../x y"}),
            json!({"cmd": "colar", "texto": "   "}),
            json!({"cmd": "colar", "texto": "x".repeat(8001)}),
            json!({"cmd": "colar", "texto": 5}),
            json!({}),
        ] {
            assert!(validar(&ruim).is_err());
        }
    }

    #[test]
    fn limpar_ocr_e_normalizar_area() {
        assert_eq!(limpar_texto_ocr("  Olá  \r\n\r\n\r\n\r\nmundo \r\n"), "Olá\n\nmundo");
        assert_eq!(limpar_texto_ocr("a-\nb"), "a-\nb");
        assert_eq!(limpar_texto_ocr("   \n \n"), "");
        assert_eq!(normalizar_area(10, 20, 110, 70), Some((10, 20, 100, 50)));
        assert_eq!(normalizar_area(110, 70, 10, 20), Some((10, 20, 100, 50)));
        assert_eq!(normalizar_area(5, 5, 5, 5), None);
        assert_eq!(normalizar_area(0, 0, 300, 4), None);
    }
}

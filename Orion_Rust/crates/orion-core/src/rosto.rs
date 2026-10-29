//! Reconhecimento do seu rosto no computador (E11.2; REGRAS.md: rosto é local, cadastrado só pelo
//! dono, e nunca sai do computador).
//!
//! Só o vetor médio vai para a tabela `faces`, cifrado com chave guardada no cofre do sistema
//! (`ORION_FACE_KEY`); a foto nunca é salva. Aqui o rosto é sinal de presença (pausar/retomar tela
//! e avisos) e nunca aprova ferramenta do agente. Entrar e confirmar pedidos de senha pelo rosto é
//! o Face ID (`faceid.rs`), opt-in, só na janela nativa e sempre com a senha como alternativa.

use base64::Engine as _;
use hmac::{Hmac, Mac};
use orion_store::Store;
use parking_lot::Mutex;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use subtle::ConstantTimeEq;

pub const CHAVE: &str = "ORION_FACE_KEY";
pub const LIMIAR: f64 = 0.45;
pub const MIN_AMOSTRAS: usize = 5;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct RostoError(pub String);

pub trait CofreRosto: Send + Sync {
    fn get(&self, nome: &str) -> Option<String>;
    fn set(&self, nome: &str, valor: &str) -> Result<(), String>;
    fn del(&self, nome: &str);
}

pub struct CofreSistema;

impl CofreRosto for CofreSistema {
    fn get(&self, nome: &str) -> Option<String> {
        orion_ai::secrets::get_secret(nome)
    }
    fn set(&self, nome: &str, valor: &str) -> Result<(), String> {
        orion_ai::secrets::set_secret(nome, valor).map_err(|e| e.to_string())
    }
    fn del(&self, nome: &str) {
        let _ = orion_ai::secrets::delete_secret(nome);
    }
}

#[derive(Default)]
pub struct CofreMemoria(pub Mutex<HashMap<String, String>>);

impl CofreRosto for CofreMemoria {
    fn get(&self, nome: &str) -> Option<String> {
        self.0.lock().get(nome).cloned()
    }
    fn set(&self, nome: &str, valor: &str) -> Result<(), String> {
        self.0.lock().insert(nome.to_string(), valor.to_string());
        Ok(())
    }
    fn del(&self, nome: &str) {
        self.0.lock().remove(nome);
    }
}

fn normalizar(v: &[f64]) -> Result<Vec<f64>, RostoError> {
    let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if n <= 1e-9 {
        return Err(RostoError("vetor de rosto nulo".into()));
    }
    Ok(v.iter().map(|x| x / n).collect())
}

pub fn media(vetores: &[Vec<f64>]) -> Result<Vec<f64>, RostoError> {
    if vetores.is_empty() || vetores[0].is_empty() {
        return Err(RostoError("vetor de rosto nulo".into()));
    }
    let dim = vetores[0].len();
    let mut acc = vec![0.0; dim];
    for v in vetores {
        let u = normalizar(v)?;
        for (i, x) in u.into_iter().enumerate() {
            acc[i] += x;
        }
    }
    normalizar(&acc)
}

pub fn cosseno(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (Ok(ua), Ok(ub)) = (normalizar(a), normalizar(b)) else { return 0.0 };
    ua.iter().zip(ub.iter()).map(|(x, y)| x * y).sum()
}

fn fluxo_xor(chave: &[u8; 32], nonce: &[u8; 16], dados: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(dados.len());
    for (bloco, fatia) in dados.chunks(32).enumerate() {
        let mut h = Sha256::new();
        h.update(chave);
        h.update(nonce);
        h.update((bloco as u64).to_be_bytes());
        let ks = h.finalize();
        for (b, k) in fatia.iter().zip(ks.iter()) {
            out.push(b ^ k);
        }
    }
    out
}

fn decodificar_chave(texto: &str) -> Option<[u8; 32]> {
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(texto.trim())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(texto.trim()))
        .ok()?;
    let mut k = [0u8; 32];
    if raw.len() == 32 {
        k.copy_from_slice(&raw);
        Some(k)
    } else {
        let d = Sha256::digest(&raw);
        k.copy_from_slice(&d);
        Some(k)
    }
}

fn obter_chave(cofre: &dyn CofreRosto, criar: bool) -> Result<[u8; 32], RostoError> {
    if let Some(existente) = cofre.get(CHAVE).and_then(|s| decodificar_chave(&s)) {
        return Ok(existente);
    }
    if !criar {
        return Err(RostoError("chave do rosto ausente no cofre do sistema".into()));
    }
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(k);
    cofre.set(CHAVE, &b64).map_err(|e| RostoError(format!("cofre do sistema indisponível para guardar a chave do rosto: {e}")))?;
    Ok(k)
}

/// Cifra bytes com a chave do rosto (fluxo SHA-256 + HMAC-SHA-256); cria a chave se faltar.
pub fn cifrar_bytes(cofre: &dyn CofreRosto, payload: &[u8]) -> Result<Vec<u8>, RostoError> {
    let chave = obter_chave(cofre, true)?;
    let mut nonce = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = fluxo_xor(&chave, &nonce, payload);
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&chave).map_err(|e| RostoError(e.to_string()))?;
    mac.update(&nonce);
    mac.update(&ct);
    let tag = mac.finalize().into_bytes();
    let mut token = Vec::with_capacity(16 + 32 + ct.len());
    token.extend_from_slice(&nonce);
    token.extend_from_slice(&tag);
    token.extend_from_slice(&ct);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token).into_bytes())
}

/// Inverso de `cifrar_bytes`; falha (sem revelar o motivo exato) se a chave mudou ou o blob foi mexido.
pub fn decifrar_bytes(cofre: &dyn CofreRosto, blob: &[u8]) -> Result<Vec<u8>, RostoError> {
    let invalido = || RostoError("vetor de rosto inválido ou chave trocada".into());
    let chave = obter_chave(cofre, false)?;
    let s = std::str::from_utf8(blob).map_err(|_| invalido())?;
    let raw = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim()).map_err(|_| invalido())?;
    if raw.len() <= 48 {
        return Err(invalido());
    }
    let mut nonce = [0u8; 16];
    nonce.copy_from_slice(&raw[..16]);
    let tag = &raw[16..48];
    let ct = &raw[48..];
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&chave).map_err(|_| invalido())?;
    mac.update(&nonce);
    mac.update(ct);
    let esperado = mac.finalize().into_bytes();
    if !bool::from(esperado.as_slice().ct_eq(tag)) {
        return Err(invalido());
    }
    Ok(fluxo_xor(&chave, &nonce, ct))
}

pub fn cifrar_com(cofre: &dyn CofreRosto, vetor: &[f64]) -> Result<Vec<u8>, RostoError> {
    let limpo: Vec<f64> = vetor.iter().map(|x| (x * 1e6).round() / 1e6).collect();
    let payload = serde_json::to_vec(&limpo).map_err(|e| RostoError(e.to_string()))?;
    cifrar_bytes(cofre, &payload)
}

pub fn decifrar_com(cofre: &dyn CofreRosto, blob: &[u8]) -> Result<Vec<f64>, RostoError> {
    let pt = decifrar_bytes(cofre, blob)?;
    let dados: Vec<f64> = serde_json::from_slice(&pt).map_err(|_| RostoError("vetor de rosto corrompido".into()))?;
    if dados.is_empty() {
        return Err(RostoError("vetor de rosto corrompido".into()));
    }
    Ok(dados)
}

pub use crate::rosto_lbph::{
    detectar_face_haar, equalizar_histograma, extrair_lbph_grid, extrator_haar_lbph, ImagemIntegral,
    RetanguloFace,
};

pub type ExtratorRosto = Arc<dyn Fn(&[u8]) -> Option<Vec<f64>> + Send + Sync>;

pub struct Rostos {
    pub memory: Arc<Store>,
    pub modelo_path: Option<PathBuf>,
    pub limiar: f64,
    pub extrator: Option<ExtratorRosto>,
    pub cofre: Arc<dyn CofreRosto>,
}

impl Rostos {
    pub fn new(memory: Arc<Store>, modelo_path: Option<PathBuf>, limiar: f64, extrator: Option<ExtratorRosto>) -> Self {
        let extrator = extrator.or_else(|| {
            if modelo_path.is_none() {
                Some(Arc::new(extrator_haar_lbph) as ExtratorRosto)
            } else {
                None
            }
        });
        Self {
            memory,
            modelo_path,
            limiar,
            extrator,
            cofre: Arc::new(CofreSistema),
        }
    }

    /// Cria uma instância usando o extrator nativo clássico Haar + LBPH (F18.05; C21; sem IA e sem nuvem).
    pub fn com_lbph(memory: Arc<Store>, limiar: f64) -> Self {
        Self::new(memory, None, limiar, Some(Arc::new(extrator_haar_lbph)))
    }

    pub fn com_cofre(mut self, cofre: Arc<dyn CofreRosto>) -> Self {
        self.cofre = cofre;
        self
    }

    pub fn disponivel(&self) -> bool {
        self.extrator.is_some() || self.modelo_path.as_ref().is_some_and(|p| p.is_file())
    }

    pub fn cadastrado(&self, nome: &str) -> bool {
        self.memory.query("SELECT 1 FROM faces WHERE name=?", &[&nome]).map(|r| !r.is_empty()).unwrap_or(false)
    }

    pub fn apagar(&self, nome: &str) -> usize {
        let n = self.memory.tx(|c| Ok(c.execute("DELETE FROM faces WHERE name=?", [nome])?)).unwrap_or(0);
        self.cofre.del(CHAVE);
        n
    }

    pub fn guardar_vetor(&self, nome: &str, vetor: &[f64]) -> Result<(), RostoError> {
        let unit = normalizar(vetor)?;
        let blob = cifrar_com(&*self.cofre, &unit)?;
        let agora = (self.memory.clock())();
        self.memory
            .tx(|c| {
                c.execute("DELETE FROM faces WHERE name=?", [nome])?;
                c.execute(
                    "INSERT INTO faces(name, embedding, created_at) VALUES (?, ?, ?)",
                    rusqlite::params![nome, blob, agora],
                )?;
                Ok(())
            })
            .map_err(|e| RostoError(e.to_string()))
    }

    pub fn carregar_vetor(&self, nome: &str) -> Option<Vec<f64>> {
        self.memory
            .tx(|c| {
                let mut st = c.prepare("SELECT embedding FROM faces WHERE name=?")?;
                let mut rows = st.query([nome])?;
                let Some(r) = rows.next()? else { return Ok(None) };
                let blob: Vec<u8> = r.get(0)?;
                Ok(decifrar_com(&*self.cofre, &blob).ok())
            })
            .ok()
            .flatten()
    }

    pub fn extrair(&self, jpeg: &[u8]) -> Option<Vec<f64>> {
        let f = self.extrator.as_ref()?;
        f(jpeg)
    }

    /// Extrai o vetor de cada foto (em memória), tira a média e grava cifrado; as fotos não ficam.
    pub fn cadastrar(&self, fotos_jpeg: &[Vec<u8>], nome: &str) -> Result<usize, RostoError> {
        if !self.disponivel() {
            return Err(RostoError("modelo de rosto indisponível (defina ORION_FACE_MODEL)".into()));
        }
        let vetores: Vec<Vec<f64>> = fotos_jpeg.iter().filter_map(|f| self.extrair(f)).collect();
        if vetores.len() < MIN_AMOSTRAS {
            return Err(RostoError(format!("só consegui ler {} foto(s); preciso de pelo menos {MIN_AMOSTRAS}", vetores.len())));
        }
        self.guardar_vetor(nome, &media(&vetores)?)?;
        tracing::info!(target: "orion.audit", "rosto_cadastrado nome={nome} amostras={}", vetores.len());
        Ok(vetores.len())
    }

    /// `true` se o rosto na foto bate com o cadastrado; `false` se há outro ou nenhum rosto;
    /// `None` se não há cadastro ou modelo.
    pub fn e_voce(&self, jpeg: &[u8], nome: &str) -> Option<bool> {
        let ( Some(alvo), true ) = (self.carregar_vetor(nome), self.disponivel()) else {
            return None;
        };
        let Some(atual) = self.extrair(jpeg) else {
            return Some(false);
        };
        Some(cosseno(&alvo, &atual) >= self.limiar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rosto_cifra_reconhece_e_apaga() {
        let store = Arc::new(Store::open(":memory:").unwrap());
        let cofre = Arc::new(CofreMemoria::default());
        let base = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];
        let b2 = base.clone();
        let extrator: ExtratorRosto = Arc::new(move |jpeg: &[u8]| {
            if jpeg == b"outro" {
                Some(vec![-0.8, 0.1, -0.5, 0.2, -0.3, 0.4, -0.1, 0.2])
            } else if jpeg == b"vazio" {
                None
            } else {
                Some(b2.clone())
            }
        });
        let r = Rostos::new(store.clone(), None, LIMIAR, Some(extrator)).com_cofre(cofre.clone());
        let amostras = vec![b"1".to_vec(), b"2".to_vec(), b"3".to_vec(), b"4".to_vec(), b"5".to_vec()];
        assert_eq!(r.cadastrar(&amostras, "antonio").unwrap(), 5);
        assert!(r.cadastrado("antonio"));
        let bruto: Vec<u8> = store.tx(|c| Ok(c.query_row("SELECT embedding FROM faces WHERE name='antonio'", [], |row| row.get(0))?)).unwrap();
        assert!(!String::from_utf8_lossy(&bruto).contains("0.1"));
        assert_eq!(r.e_voce(b"eu", "antonio"), Some(true));
        assert_eq!(r.e_voce(b"outro", "antonio"), Some(false));
        assert_eq!(r.e_voce(b"vazio", "antonio"), Some(false));
        cofre.set(CHAVE, &base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([9u8; 32])).unwrap();
        assert_eq!(r.carregar_vetor("antonio"), None);
        assert_eq!(r.apagar("antonio"), 1);
        assert!(!r.cadastrado("antonio"));
    }

    #[test]
    fn rosto_recusa_menos_de_cinco_amostras() {
        let store = Arc::new(Store::open(":memory:").unwrap());
        let cofre = Arc::new(CofreMemoria::default());
        let r = Rostos::new(store, None, LIMIAR, Some(Arc::new(|_| Some(vec![1.0, 0.0, 0.0])))).com_cofre(cofre);
        assert!(r.cadastrar(&[b"1".to_vec(), b"2".to_vec()], "antonio").is_err());
    }

    #[test]
    fn haar_lbph_reconhecimento_end_to_end() {
        let store = Arc::new(Store::open(":memory:").unwrap());
        let cofre = Arc::new(CofreMemoria::default());

        fn gerar_jpeg_face(variacao: u8) -> Vec<u8> {
            let mut img = image::GrayImage::new(64, 64);
            for y in 0..64 {
                for x in 0..64 {
                    let base = if y < 16 {
                        180 // testa clara
                    } else if y < 32 {
                        if (x >= 12 && x <= 24) || (x >= 40 && x <= 52) {
                            50 + variacao // órbitas/olhos escuros
                        } else {
                            160 // centro/nariz alto
                        }
                    } else if y < 48 {
                        if x >= 26 && x <= 38 {
                            200 // ponte nasal clara
                        } else {
                            140 // bochechas
                        }
                    } else {
                        120 // boca/queixo
                    };
                    img.put_pixel(x, y, image::Luma([base]));
                }
            }
            let mut buf = Vec::new();
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 90);
            enc.encode(img.as_raw(), 64, 64, image::ExtendedColorType::L8).unwrap();
            buf
        }

        let f1 = gerar_jpeg_face(0);
        let f2 = gerar_jpeg_face(5);
        let f3 = gerar_jpeg_face(10);
        let f4 = gerar_jpeg_face(2);
        let f5 = gerar_jpeg_face(7);

        let r = Rostos::com_lbph(store, LIMIAR).com_cofre(cofre);
        assert!(r.disponivel());

        let cadastradas = r.cadastrar(&[f1.clone(), f2.clone(), f3.clone(), f4.clone(), f5.clone()], "antonio").unwrap();
        assert_eq!(cadastradas, 5);
        assert!(r.cadastrado("antonio"));

        // Foto da mesma pessoa deve ser reconhecida (e_voce = Some(true))
        let teste_mesma_pessoa = gerar_jpeg_face(3);
        assert_eq!(r.e_voce(&teste_mesma_pessoa, "antonio"), Some(true));

        // Bytes vazios ou sem rosto não são reconhecidos
        assert_eq!(r.e_voce(b"invalido", "antonio"), Some(false));
    }
}

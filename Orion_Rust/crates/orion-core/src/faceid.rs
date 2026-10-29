//! Face ID local: entrar e confirmar pedidos que exigem senha olhando para a câmera (REGRAS.md: o
//! Face ID é opt-in, só vale na janela nativa e a senha continua sempre como alternativa).
//!
//! Tudo roda em CPU com o pipeline clássico de `rosto_lbph` (luz → Haar → LBPH uniforme), sem rede
//! e sem modelo neural. O modelo do rosto fica em `faceid.bin` na pasta de dados, cifrado com a
//! chave do rosto no cofre do sistema: precisa existir antes do banco cifrado abrir, então não pode
//! morar dentro dele. A senha que o Face ID libera fica no cofre do sistema (`ORION_FACEID_LOGIN`),
//! como no Windows Hello.
//!
//! O que isto **não** é: prova de vida. LBPH em câmera comum não distingue o rosto de uma boa foto
//! dele. Por isso: três quadros seguidos precisam bater, a imagem precisa variar entre eles (quadro
//! congelado de câmera virtual não passa), cinco sessões falhas travam o Face ID até a próxima
//! entrada com senha, e aprovar ferramenta do agente continua sendo só por botão.

use crate::camera::FonteQuadros;
use crate::rosto::{cifrar_bytes, cosseno, decifrar_bytes, CofreRosto, RostoError};
use crate::rosto_lbph::{analisar_quadro, Analise, RetanguloFace, DIM_UNIFORME};
use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub const ARQUIVO: &str = "faceid.bin";
/// Credencial `{user, password}` liberada depois de o rosto bater (mesmo formato do Hello).
pub const CREDENCIAL: &str = "ORION_FACEID_LOGIN";
/// Quadros aceitos no cadastro.
pub const AMOSTRAS: usize = 8;
/// Intervalo entre verificações: o suficiente para parecer instantâneo sem ocupar a CPU.
pub const INTERVALO: Duration = Duration::from_millis(250);
/// Tempo máximo de uma leitura antes de cair para a senha.
pub const PRAZO: Duration = Duration::from_secs(7);
/// Quadros seguidos que precisam bater.
pub const ACERTOS: u32 = 3;
/// Quadros com rosto que não bate até desistir (~2 s).
pub const REJEICOES: u32 = 8;
/// Quadros escuros seguidos até desistir por falta de luz (~2 s).
pub const ESCUROS: u32 = 8;
/// Sessões falhas seguidas que travam o Face ID até entrar com a senha.
pub const MAX_FALHAS: u32 = 5;
/// Limites do limiar adaptativo (cosseno sobre LBPH uniforme com raiz de Hellinger).
pub const LIMIAR_MIN: f64 = 0.86;
pub const LIMIAR_MAX: f64 = 0.985;
/// Validade de um tíquete de aprovação.
pub const TTL_TIQUETE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FaceIdErro {
    #[error("Face ID não está ativado")]
    Inativo,
    #[error("Face ID travado depois de várias tentativas; entre com a senha")]
    Travado,
    #[error("câmera indisponível: {0}")]
    SemCamera(String),
    #[error("pouca luz para ler o rosto")]
    PoucaLuz,
    #[error("nenhum rosto encontrado")]
    SemRosto,
    #[error("{0}")]
    Cofre(String),
}

impl From<RostoError> for FaceIdErro {
    fn from(e: RostoError) -> Self {
        FaceIdErro::Cofre(e.0)
    }
}

/// O modelo do rosto: vetor médio das amostras e o limiar calibrado no próprio cadastro.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Modelo {
    pub versao: u32,
    pub usuario: String,
    pub media: Vec<f32>,
    pub limiar: f64,
    pub amostras: usize,
}

impl Modelo {
    pub fn pontuar(&self, vetor: &[f64]) -> f64 {
        let media: Vec<f64> = self.media.iter().map(|&x| x as f64).collect();
        cosseno(&media, vetor)
    }
}

/// Monta o modelo: média das amostras e limiar = semelhança típica entre cada amostra e a média,
/// com folga, dentro de `LIMIAR_MIN..=LIMIAR_MAX`. Quem cadastra com amostras parecidas entre si
/// ganha um limiar mais exigente.
pub fn modelo_de_amostras(usuario: &str, amostras: &[Vec<f64>]) -> Result<Modelo, FaceIdErro> {
    if amostras.len() < AMOSTRAS.min(5) || amostras.iter().any(|a| a.len() != DIM_UNIFORME) {
        return Err(FaceIdErro::SemRosto);
    }
    let media = crate::rosto::media(amostras)?;
    let mut semelhancas: Vec<f64> = amostras.iter().map(|a| cosseno(&media, a)).collect();
    semelhancas.sort_by(|a, b| a.total_cmp(b));
    // percentil 25: uma amostra ruim não derruba o limiar de todo mundo
    let p25 = semelhancas[semelhancas.len() / 4];
    let limiar = (p25 - (1.0 - p25) * 0.6).clamp(LIMIAR_MIN, LIMIAR_MAX);
    Ok(Modelo { versao: 1, usuario: usuario.to_string(), media: media.iter().map(|&x| x as f32).collect(), limiar, amostras: amostras.len() })
}

pub fn caminho(data_dir: &Path) -> PathBuf {
    data_dir.join(ARQUIVO)
}

/// Barato o bastante para a tela de entrada consultar a cada desenho.
pub fn ativo(data_dir: &Path) -> bool {
    caminho(data_dir).is_file()
}

pub fn salvar(data_dir: &Path, cofre: &dyn CofreRosto, modelo: &Modelo) -> Result<(), FaceIdErro> {
    let json = zeroize::Zeroizing::new(serde_json::to_vec(modelo).map_err(|e| FaceIdErro::Cofre(e.to_string()))?);
    let blob = cifrar_bytes(cofre, &json)?;
    let tmp = data_dir.join(format!("{ARQUIVO}.tmp"));
    std::fs::write(&tmp, blob).and_then(|_| std::fs::rename(&tmp, caminho(data_dir))).map_err(|e| FaceIdErro::Cofre(format!("não foi possível gravar o Face ID: {e}")))
}

pub fn carregar(data_dir: &Path, cofre: &dyn CofreRosto) -> Result<Modelo, FaceIdErro> {
    let blob = std::fs::read(caminho(data_dir)).map_err(|_| FaceIdErro::Inativo)?;
    let json = zeroize::Zeroizing::new(decifrar_bytes(cofre, &blob)?);
    let m: Modelo = serde_json::from_slice(&json).map_err(|_| FaceIdErro::Cofre("modelo do Face ID corrompido; cadastre de novo".into()))?;
    if m.media.len() != DIM_UNIFORME {
        return Err(FaceIdErro::Cofre("modelo do Face ID de outra versão; cadastre de novo".into()));
    }
    Ok(m)
}

/// Desliga o Face ID: apaga o modelo e a credencial. A chave do rosto fica (a presença usa).
pub fn apagar(data_dir: &Path, cofre: &dyn CofreRosto) {
    let _ = std::fs::remove_file(caminho(data_dir));
    cofre.del(CREDENCIAL);
    zerar_falhas();
}

// ── trava por tentativas ────────────────────────────────────────────────────

static FALHAS: AtomicU32 = AtomicU32::new(0);

pub fn travado() -> bool {
    FALHAS.load(Ordering::SeqCst) >= MAX_FALHAS
}
pub fn registrar_falha() {
    FALHAS.fetch_add(1, Ordering::SeqCst);
}
/// Entrar com a senha (ou o rosto bater) destrava.
pub fn zerar_falhas() {
    FALHAS.store(0, Ordering::SeqCst);
}

// ── verificação ─────────────────────────────────────────────────────────────

/// Dica para a interface enquanto procura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dica {
    Iniciando,
    SemRosto,
    PoucaLuz,
    Lendo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Falha {
    SemCamera,
    PoucaLuz,
    NaoReconhecido,
    SemRosto,
    Congelado,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Passo {
    Procurando(Dica),
    Sucesso { pontuacao: f64 },
    Falha(Falha),
}

/// Máquina de estados de uma leitura. Pura (o tempo entra por parâmetro) para ser testável.
pub struct Verificador {
    modelo: Modelo,
    acertos: u32,
    rejeicoes: u32,
    escuros: u32,
    viu_rosto: bool,
    viu_luz: bool,
    variou: bool,
    anterior: Option<(RetanguloFace, Vec<f64>)>,
    dica: Dica,
}

impl Verificador {
    pub fn new(modelo: Modelo) -> Self {
        Self { modelo, acertos: 0, rejeicoes: 0, escuros: 0, viu_rosto: false, viu_luz: false, variou: false, anterior: None, dica: Dica::Iniciando }
    }

    pub fn dica(&self) -> Dica {
        self.dica
    }

    /// Prazo esgotado sem decisão.
    pub fn esgotar(&self) -> Passo {
        Passo::Falha(if !self.viu_luz && self.escuros > 0 {
            Falha::PoucaLuz
        } else if self.acertos >= ACERTOS && !self.variou {
            Falha::Congelado
        } else if self.viu_rosto {
            Falha::NaoReconhecido
        } else {
            Falha::SemRosto
        })
    }

    pub fn avaliar(&mut self, analise: &Analise, decorrido: Duration) -> Passo {
        if decorrido >= PRAZO {
            return self.esgotar();
        }
        if !analise.luz.suficiente() {
            self.escuros += 1;
            self.acertos = 0;
            self.dica = Dica::PoucaLuz;
            return if self.escuros >= ESCUROS { Passo::Falha(Falha::PoucaLuz) } else { Passo::Procurando(Dica::PoucaLuz) };
        }
        self.escuros = 0;
        self.viu_luz = true;
        let Some((rect, vetor)) = &analise.rosto else {
            self.acertos = 0;
            self.dica = Dica::SemRosto;
            return Passo::Procurando(Dica::SemRosto);
        };
        self.viu_rosto = true;
        self.dica = Dica::Lendo;
        let pontuacao = self.modelo.pontuar(vetor);
        if let Some((r0, v0)) = &self.anterior {
            // micro-movimento natural: o recorte ou a textura mudam um pouco entre quadros
            if r0 != rect || cosseno(v0, vetor) < 0.99995 {
                self.variou = true;
            }
        }
        self.anterior = Some((*rect, vetor.clone()));
        if pontuacao >= self.modelo.limiar {
            self.acertos += 1;
            self.rejeicoes = 0;
            if self.acertos >= ACERTOS && self.variou {
                return Passo::Sucesso { pontuacao };
            }
        } else {
            self.acertos = 0;
            self.rejeicoes += 1;
            if self.rejeicoes >= REJEICOES {
                return Passo::Falha(Falha::NaoReconhecido);
            }
        }
        Passo::Procurando(Dica::Lendo)
    }
}

/// Uma leitura em andamento: a fonte de quadros (webcam) e o verificador. Cada `passo` analisa só
/// o quadro mais novo; quem chama decide o ritmo (a tela de entrada chama a cada `INTERVALO`).
pub struct Sessao {
    fonte: Box<dyn FonteQuadros>,
    verificador: Verificador,
    inicio: Instant,
    visto: u64,
    pub proposito: String,
    pub usuario: String,
}

impl Sessao {
    pub fn new(fonte: Box<dyn FonteQuadros>, modelo: Modelo, proposito: &str) -> Self {
        let usuario = modelo.usuario.clone();
        Self { fonte, verificador: Verificador::new(modelo), inicio: Instant::now(), visto: 0, proposito: proposito.into(), usuario }
    }

    /// Chamada bloqueante e curta (alguns ms): rode fora da thread da interface.
    pub fn passo(&mut self) -> Passo {
        if self.fonte.erro().is_some() {
            return Passo::Falha(Falha::SemCamera);
        }
        let decorrido = self.inicio.elapsed();
        let Some(q) = self.fonte.ultimo(self.visto) else {
            return if decorrido >= PRAZO { self.verificador.esgotar() } else { Passo::Procurando(self.verificador.dica()) };
        };
        self.visto = q.seq;
        let analise = analisar_quadro(&q.luma, q.w as usize, q.h as usize);
        self.verificador.avaliar(&analise, decorrido)
    }
}

/// Cadastro: junta `AMOSTRAS` quadros com luz e rosto, um a cada `INTERVALO`, até `prazo`.
/// Bloqueante; rode em `spawn_blocking`.
pub fn cadastrar(fonte: &dyn FonteQuadros, usuario: &str, prazo: Duration, cancelar: &AtomicBool) -> Result<Modelo, FaceIdErro> {
    let inicio = Instant::now();
    let (mut amostras, mut visto, mut escuros, mut sem_rosto) = (Vec::with_capacity(AMOSTRAS), 0u64, 0u32, 0u32);
    while amostras.len() < AMOSTRAS {
        if cancelar.load(Ordering::Relaxed) {
            return Err(FaceIdErro::Cofre("cadastro cancelado".into()));
        }
        if let Some(e) = fonte.erro() {
            return Err(FaceIdErro::SemCamera(e));
        }
        if inicio.elapsed() >= prazo {
            return Err(if escuros > sem_rosto { FaceIdErro::PoucaLuz } else { FaceIdErro::SemRosto });
        }
        if let Some(q) = fonte.ultimo(visto) {
            visto = q.seq;
            let a = analisar_quadro(&q.luma, q.w as usize, q.h as usize);
            match (a.luz.suficiente(), a.rosto) {
                (false, _) => escuros += 1,
                (true, None) => sem_rosto += 1,
                (true, Some((_, v))) => amostras.push(v),
            }
        }
        std::thread::sleep(INTERVALO);
    }
    modelo_de_amostras(usuario, &amostras)
}

// ── tíquetes de aprovação ───────────────────────────────────────────────────

/// Prova de curta duração de que o rosto bateu, para um único pedido (por exemplo `autonomo`,
/// `panico`). Só a ligação nativa emite; as rotas consomem uma vez. 256 bits aleatórios.
#[derive(Default)]
pub struct Tiquetes(Mutex<HashMap<String, (String, Instant)>>);

impl Tiquetes {
    pub fn emitir(&self, proposito: &str) -> String {
        let mut b = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut b);
        let t = hex::encode(b);
        let mut m = self.0.lock();
        m.retain(|_, (_, at)| at.elapsed() < TTL_TIQUETE);
        m.insert(t.clone(), (proposito.to_string(), Instant::now()));
        t
    }

    /// Consome o tíquete: vale uma vez, só para o propósito emitido e dentro do prazo.
    pub fn consumir(&self, tiquete: &str, proposito: &str) -> bool {
        if tiquete.len() != 64 {
            return false;
        }
        match self.0.lock().remove(tiquete) {
            Some((p, at)) => p == proposito && at.elapsed() < TTL_TIQUETE,
            None => false,
        }
    }
}

pub static TIQUETES: LazyLock<Tiquetes> = LazyLock::new(Default::default);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Quadro;
    use crate::rosto::CofreMemoria;
    use crate::rosto_lbph::Luz;
    use std::sync::Arc;

    fn rosto_sintetico(variacao: u8, deslocamento: usize) -> Vec<u8> {
        let (w, h) = (160usize, 120usize);
        let mut px = vec![90u8; w * h];
        let (ox, oy) = (48 + deslocamento, 24);
        for y in 0..64 {
            for x in 0..64 {
                let v = if y < 16 {
                    180
                } else if y < 32 {
                    if (12..=24).contains(&x) || (40..=52).contains(&x) { 50 + variacao } else { 160 }
                } else if y < 48 {
                    if (26..=38).contains(&x) { 200 } else { 140 }
                } else {
                    120
                };
                // textura fina para o LBPH ter o que ler
                px[(oy + y) * w + ox + x] = v.saturating_add(((x * 7 + y * 3) % 11) as u8);
            }
        }
        px
    }

    fn vetor(px: &[u8]) -> Vec<f64> {
        analisar_quadro(px, 160, 120).rosto.expect("rosto sintético detectado").1
    }

    fn modelo() -> Modelo {
        let amostras: Vec<Vec<f64>> = (0..AMOSTRAS).map(|i| vetor(&rosto_sintetico(i as u8, i % 3))).collect();
        modelo_de_amostras("antonio", &amostras).unwrap()
    }

    fn analise(px: &[u8]) -> Analise {
        analisar_quadro(px, 160, 120)
    }

    #[test]
    fn limiar_fica_dentro_dos_limites() {
        let m = modelo();
        assert!((LIMIAR_MIN..=LIMIAR_MAX).contains(&m.limiar));
        assert_eq!(m.media.len(), DIM_UNIFORME);
    }

    #[test]
    fn mesmo_rosto_com_movimento_passa_em_tres_quadros() {
        let mut v = Verificador::new(modelo());
        let mut passos = vec![];
        for i in 0..3 {
            passos.push(v.avaliar(&analise(&rosto_sintetico(3 + i, i as usize)), Duration::from_millis(250 * i as u64)));
        }
        assert!(matches!(passos[2], Passo::Sucesso { .. }), "{passos:?}");
        assert!(matches!(passos[0], Passo::Procurando(Dica::Lendo)));
    }

    #[test]
    fn quadro_congelado_nao_passa() {
        let mut v = Verificador::new(modelo());
        let fixo = analise(&rosto_sintetico(3, 0));
        for i in 0..6 {
            assert!(!matches!(v.avaliar(&fixo, Duration::from_millis(250 * i)), Passo::Sucesso { .. }));
        }
        assert_eq!(v.esgotar(), Passo::Falha(Falha::Congelado));
    }

    #[test]
    fn outra_textura_e_recusada() {
        let mut v = Verificador::new(modelo());
        let outro = Analise {
            luz: Luz { media: 120.0, desvio: 40.0 },
            rosto: Some((RetanguloFace { x: 0, y: 0, w: 64, h: 64 }, crate::rosto_lbph::extrair_lbph_uniforme(&(0..64 * 64).map(|i| ((i * 37) % 251) as u8).collect::<Vec<_>>(), 64, 64))),
        };
        let mut ultimo = Passo::Procurando(Dica::Iniciando);
        for i in 0..REJEICOES {
            ultimo = v.avaliar(&outro, Duration::from_millis(250 * i as u64));
        }
        assert_eq!(ultimo, Passo::Falha(Falha::NaoReconhecido));
    }

    #[test]
    fn escuro_cai_para_senha() {
        let mut v = Verificador::new(modelo());
        let escuro = analise(&vec![8u8; 160 * 120]);
        let mut ultimo = Passo::Procurando(Dica::Iniciando);
        for i in 0..ESCUROS {
            ultimo = v.avaliar(&escuro, Duration::from_millis(250 * i as u64));
        }
        assert_eq!(ultimo, Passo::Falha(Falha::PoucaLuz));
    }

    #[test]
    fn prazo_sem_rosto_falha() {
        let mut v = Verificador::new(modelo());
        let vazio = analise(&vec![120u8; 160 * 120].iter().enumerate().map(|(i, _)| (i % 200) as u8).collect::<Vec<_>>());
        assert_eq!(v.avaliar(&vazio, PRAZO), Passo::Falha(Falha::SemRosto));
    }

    #[test]
    fn modelo_cifrado_volta_igual_e_apaga() {
        let dir = tempfile::tempdir().unwrap();
        let cofre = CofreMemoria::default();
        let m = modelo();
        salvar(dir.path(), &cofre, &m).unwrap();
        assert!(ativo(dir.path()));
        let bruto = std::fs::read(caminho(dir.path())).unwrap();
        assert!(!String::from_utf8_lossy(&bruto).contains("antonio"));
        assert_eq!(carregar(dir.path(), &cofre).unwrap(), m);
        cofre.set(CREDENCIAL, "x").unwrap();
        apagar(dir.path(), &cofre);
        assert!(!ativo(dir.path()));
        assert!(cofre.get(CREDENCIAL).is_none());
    }

    #[test]
    fn tiquete_vale_uma_vez_e_so_para_o_proposito() {
        let t = Tiquetes::default();
        let a = t.emitir("autonomo");
        assert!(!t.consumir(&a, "panico"));
        let b = t.emitir("autonomo");
        assert!(t.consumir(&b, "autonomo"));
        assert!(!t.consumir(&b, "autonomo"));
        assert!(!t.consumir("curto", "autonomo"));
    }

    struct FonteFixa(Vec<Arc<Quadro>>, std::sync::atomic::AtomicUsize);
    impl FonteQuadros for FonteFixa {
        fn ultimo(&self, depois_de: u64) -> Option<Arc<Quadro>> {
            let i = self.1.fetch_add(1, Ordering::SeqCst);
            self.0.get(i % self.0.len()).filter(|q| q.seq > depois_de).cloned()
        }
        fn erro(&self) -> Option<String> {
            None
        }
    }

    #[test]
    fn sessao_le_quadros_da_fonte_ate_bater() {
        let quadros = (0..4).map(|i| Arc::new(Quadro { w: 160, h: 120, luma: rosto_sintetico(2 + i as u8, i), seq: i as u64 + 1 })).collect();
        let mut s = Sessao::new(Box::new(FonteFixa(quadros, Default::default())), modelo(), "login");
        let mut ultimo = Passo::Procurando(Dica::Iniciando);
        for _ in 0..4 {
            ultimo = s.passo();
            if matches!(ultimo, Passo::Sucesso { .. }) {
                break;
            }
        }
        assert!(matches!(ultimo, Passo::Sucesso { .. }), "{ultimo:?}");
    }

    #[test]
    fn trava_depois_de_falhas() {
        zerar_falhas();
        for _ in 0..MAX_FALHAS {
            registrar_falha();
        }
        assert!(travado());
        zerar_falhas();
        assert!(!travado());
    }
}

//! Detecção de objetos no computador e avistamentos só de rótulo (E11.3; REGRAS.md: a câmera só
//! liga por pedido seu, e cada uso fica registrado).
//!
//! Só o par `(objeto, hora)` entra em `object_sightings` — nunca a foto, o recorte nem onde na sala.

use crate::camera::foto_jpeg;
use orion_store::Store;
use parking_lot::Mutex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

pub const LIMIAR: f64 = 0.45;
pub const IOU_MAX: f64 = 0.5;
pub const ENTRADA: u32 = 640;
pub const MIN_INTERVALO_S: f64 = 180.0;
pub const RETENCAO_DIAS: i64 = 30;

pub const COCO_NOMES: [&str; 80] = [
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat", "traffic light",
    "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat", "dog", "horse", "sheep", "cow",
    "elephant", "bear", "zebra", "giraffe", "backpack", "umbrella", "handbag", "tie", "suitcase", "frisbee",
    "skis", "snowboard", "sports ball", "kite", "baseball bat", "baseball glove", "skateboard", "surfboard",
    "tennis racket", "bottle", "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple",
    "sandwich", "orange", "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch",
    "potted plant", "bed", "dining table", "toilet", "tv", "laptop", "mouse", "remote", "keyboard",
    "cell phone", "microwave", "oven", "toaster", "sink", "refrigerator", "book", "clock", "vase",
    "scissors", "teddy bear", "hair drier", "toothbrush",
];

pub static ALIAS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        ("chaves", "key"),
        ("chave", "key"),
        ("carteira", "handbag"),
        ("bolsa", "handbag"),
        ("mochila", "backpack"),
        ("mala", "suitcase"),
        ("celular", "cell phone"),
        ("telefone", "cell phone"),
        ("garrafa", "bottle"),
        ("copo", "cup"),
        ("caneca", "cup"),
        ("livro", "book"),
        ("caderno", "book"),
        ("notebook", "laptop"),
        ("computador", "laptop"),
        ("mouse", "mouse"),
        ("teclado", "keyboard"),
        ("controle", "remote"),
        ("oculos", "glasses"),
        ("óculos", "glasses"),
        ("relogio", "clock"),
        ("relógio", "clock"),
        ("guarda-chuva", "umbrella"),
        ("tesoura", "scissors"),
        ("faca", "knife"),
        ("colher", "spoon"),
        ("garfo", "fork"),
        ("gato", "cat"),
        ("cachorro", "dog"),
        ("pessoa", "person"),
    ])
});

static PAR_META: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(\d+)\s*:\s*['"]([^'"]+)['"]"#).unwrap());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deteccao {
    pub label: String,
    pub confidence: f64,
    pub bbox: (f64, f64, f64, f64),
}

pub fn nome_em_ingles(consulta: &str) -> String {
    let limpo = consulta.trim().to_lowercase();
    ALIAS.get(limpo.as_str()).copied().unwrap_or(&limpo).to_string()
}

pub fn iou(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let (x1, y1) = (a.0.max(b.0), a.1.max(b.1));
    let (x2, y2) = (a.2.min(b.2), a.3.min(b.3));
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    if inter <= 0.0 {
        return 0.0;
    }
    let area_a = (a.2 - a.0).max(0.0) * (a.3 - a.1).max(0.0);
    let area_b = (b.2 - b.0).max(0.0) * (b.3 - b.1).max(0.0);
    let uniao = area_a + area_b - inter;
    if uniao > 0.0 { inter / uniao } else { 0.0 }
}

pub fn nms(mut itens: Vec<Deteccao>, iou_max: f64) -> Vec<Deteccao> {
    itens.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
    let mut escolhidas: Vec<Deteccao> = vec![];
    for d in itens {
        if escolhidas.iter().all(|e| e.label != d.label || iou(e.bbox, d.bbox) < iou_max) {
            escolhidas.push(d);
        }
    }
    escolhidas
}

/// Decodifica a saída de um modelo YOLO ONNX: `linhas` é `N x (4 + C)` (`cx, cy, w, h, score_0..score_{C-1}`).
pub fn decodificar_saida(linhas: &[Vec<f64>], nomes: &HashMap<usize, String>, limiar: f64) -> Vec<Deteccao> {
    if linhas.is_empty() || linhas[0].len() < 5 {
        return vec![];
    }
    let mut achados = vec![];
    for l in linhas {
        let (cx, cy, w, h) = (l[0], l[1], l[2], l[3]);
        let Some((classe, &conf)) = l[4..].iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal)) else {
            continue;
        };
        if conf < limiar {
            continue;
        }
        let padrao = COCO_NOMES.get(classe).copied().map(String::from).unwrap_or_else(|| format!("cls_{classe}"));
        let nome = nomes.get(&classe).cloned().unwrap_or(padrao).to_lowercase();
        achados.push(Deteccao {
            label: nome,
            confidence: conf,
            bbox: (cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0),
        });
    }
    nms(achados, IOU_MAX)
}

/// Transpõe `(4 + C) x N` (formato padrão do YOLOv8 `(1, 84, N)`) para `N x (4 + C)` quando `linhas < colunas`.
pub fn decodificar_matriz(matriz: &[Vec<f64>], nomes: &HashMap<usize, String>, limiar: f64) -> Vec<Deteccao> {
    if matriz.is_empty() || matriz[0].is_empty() {
        return vec![];
    }
    if matriz.len() < matriz[0].len() {
        let cols = matriz[0].len();
        let transposta: Vec<Vec<f64>> = (0..cols).map(|c| matriz.iter().map(|r| r[c]).collect()).collect();
        decodificar_saida(&transposta, nomes, limiar)
    } else {
        decodificar_saida(matriz, nomes, limiar)
    }
}

pub fn nomes_do_modelo(meta: &HashMap<String, String>) -> HashMap<usize, String> {
    if let Some(bruto) = meta.get("names") {
        let pares: HashMap<usize, String> = PAR_META
            .captures_iter(bruto)
            .filter_map(|c| Some((c[1].parse().ok()?, c[2].to_string())))
            .collect();
        if !pares.is_empty() {
            return pares;
        }
    }
    COCO_NOMES.iter().enumerate().map(|(i, n)| (i, (*n).to_string())).collect()
}

pub struct Detector {
    pub modelo_path: Option<PathBuf>,
    pub limiar: f64,
    model: Mutex<Option<Arc<tract_onnx::prelude::TypedRunnableModel>>>,
}

impl Detector {
    pub fn new(modelo_path: Option<PathBuf>, limiar: f64) -> Self {
        Self { modelo_path, limiar, model:Mutex::new(None) }
    }

    pub fn disponivel(&self) -> bool {
        self.modelo_path.as_ref().is_some_and(|p| p.is_file())
    }

    pub fn detectar_jpeg(&self,jpeg:&[u8])->Vec<Deteccao>{
        match self.detect(jpeg){Ok(v)=>v,Err(e)=>{tracing::warn!("detecção de objetos indisponível: {e}");vec![]}}
    }
    pub fn detect(&self,jpeg:&[u8])->anyhow::Result<Vec<Deteccao>>{
        use tract_onnx::prelude::*;
        let image=image::load_from_memory(jpeg)?.resize_exact(ENTRADA,ENTRADA,image::imageops::FilterType::Triangle).to_rgb8();
        let mut guard=self.model.lock();
        if guard.is_none(){let path=self.modelo_path.as_ref().ok_or_else(||anyhow::anyhow!("configure o modelo YOLO ONNX"))?;*guard=Some(tract_onnx::onnx().model_for_path(path)?.with_input_fact(0,f32::fact([1,3,ENTRADA as usize,ENTRADA as usize]).into())?.into_optimized()?.into_runnable()?);}
        let input=tract_ndarray::Array4::from_shape_fn((1,3,ENTRADA as usize,ENTRADA as usize),|(_,c,y,x)|image.get_pixel(x as u32,y as u32)[c] as f32/255.0);
        let output=guard.as_ref().unwrap().run(tvec!(Tensor::from(input).into()))?;
        let out=output[0].to_plain_array_view::<f32>()?;
        anyhow::ensure!(out.ndim()==3 && out.shape()[0]==1,"saída YOLOv8 inválida");
        let out=out.index_axis(tract_ndarray::Axis(0),0);
        let matrix=out.outer_iter().map(|row|row.iter().map(|v|*v as f64).collect()).collect::<Vec<Vec<f64>>>();
        Ok(decodificar_matriz(&matrix,&nomes_do_modelo(&HashMap::new()),self.limiar))
    }
}

pub struct Avistamentos {
    pub memory: Arc<Store>,
    pub retencao_dias: i64,
    ultimo_por_label: Mutex<HashMap<String, f64>>,
}

impl Avistamentos {
    pub fn new(memory: Arc<Store>, retencao_dias: i64) -> Self {
        Self { memory, retencao_dias, ultimo_por_label: Mutex::new(HashMap::new()) }
    }

    /// Grava cada rótulo no máximo uma vez a cada `MIN_INTERVALO_S`.
    pub fn registrar(&self, deteccoes: &[Deteccao], agora: Option<f64>) -> usize {
        let ts = agora.unwrap_or_else(|| (self.memory.clock())());
        let mut gravados = 0;
        let mut vistos = HashSet::new();
        for d in deteccoes {
            let nome = d.label.trim().to_lowercase();
            if nome.is_empty() || !vistos.insert(nome.clone()) {
                continue;
            }
            {
                let u = self.ultimo_por_label.lock();
                if u.get(&nome).is_some_and(|ant| ts - ant < MIN_INTERVALO_S) {
                    continue;
                }
            }
            let ok = self
                .memory
                .tx(|c| {
                    c.execute(
                        "INSERT INTO object_sightings(ts, label, confidence, x, y) VALUES (?, ?, ?, ?, ?)",
                        rusqlite::params![ts, nome, d.confidence, d.bbox.0 as i64, d.bbox.1 as i64],
                    )?;
                    Ok(())
                })
                .is_ok();
            if ok {
                self.ultimo_por_label.lock().insert(nome, ts);
                gravados += 1;
            }
        }
        gravados
    }

    pub fn ultimo(&self, objeto: &str) -> Option<Map<String, Value>> {
        let bruta = objeto.trim().to_lowercase();
        let mut alvos = vec![nome_em_ingles(&bruta)];
        if !alvos.contains(&bruta) {
            alvos.push(bruta);
        }
        for a in alvos {
            if let Ok(linhas) = self.memory.query(
                "SELECT id, label, confidence, ts AS seen_at, ts, x, y FROM object_sightings WHERE label=? ORDER BY ts DESC LIMIT 1",
                &[&a],
            ) {
                if let Some(primeira) = linhas.into_iter().next() {
                    return Some(primeira);
                }
            }
        }
        None
    }

    pub fn recentes(&self, limite: i64) -> Vec<Map<String, Value>> {
        let lim = limite.clamp(1, 50);
        self.memory
            .query(
                "SELECT label, MAX(ts) AS seen_at, MAX(ts) AS ts, COUNT(*) AS vezes FROM object_sightings GROUP BY label ORDER BY ts DESC LIMIT ?",
                &[&lim],
            )
            .unwrap_or_default()
    }

    pub fn podar(&self) -> usize {
        let corte = (self.memory.clock())() - (self.retencao_dias.max(1) as f64) * 86400.0;
        self.memory
            .tx(|c| Ok(c.execute("DELETE FROM object_sightings WHERE ts < ?", [corte])?))
            .unwrap_or(0)
    }
}

/// Tira um quadro, roda o detector local e grava só os rótulos; a foto morre aqui.
pub fn olhar_uma_vez(dispositivo: i32, detector: &Detector, avistamentos: &Avistamentos) -> Vec<String> {
    if !detector.disponivel() {
        return vec![];
    }
    let Ok(jpeg) = foto_jpeg(dispositivo, 8.0) else { return vec![] };
    let achados = detector.detectar_jpeg(&jpeg);
    avistamentos.registrar(&achados, None);
    tracing::info!(target: "orion.audit", "camera_vigia_local dispositivo={dispositivo} objetos={}", achados.len());
    let mut nomes: Vec<String> = achados.into_iter().map(|d| d.label).collect();
    nomes.sort();
    nomes.dedup();
    nomes
}

pub fn caminho_modelo(p: &str) -> Option<PathBuf> {
    let t = p.trim();
    (!t.is_empty()).then(|| Path::new(t).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodificar_yolo_filtra_por_limiar_e_aplica_nms() {
        // 3 caixas e 84 valores cada (4 + 80 classes)
        let mut c0 = vec![0.0; 84];
        c0[0..4].copy_from_slice(&[100.0, 100.0, 40.0, 40.0]);
        c0[4 + 39] = 0.9; // bottle
        let mut c1 = vec![0.0; 84];
        c1[0..4].copy_from_slice(&[102.0, 101.0, 40.0, 40.0]);
        c1[4 + 39] = 0.7; // bottle sobreposta -> sai no NMS
        let mut c2 = vec![0.0; 84];
        c2[0..4].copy_from_slice(&[300.0, 300.0, 50.0, 50.0]);
        c2[4 + 67] = 0.3; // cell phone abaixo do limiar
        let nomes = nomes_do_modelo(&HashMap::new());
        let achados = decodificar_saida(&[c0, c1, c2], &nomes, 0.45);
        assert_eq!(achados.len(), 1);
        assert_eq!(achados[0].label, "bottle");
        assert!((achados[0].confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn avistamentos_guarda_so_rotulo_e_respeita_intervalo() {
        let store = Arc::new(Store::open(":memory:").unwrap());
        let av = Avistamentos::new(store.clone(), 30);
        let det = vec![Deteccao { label: "cell phone".into(), confidence: 0.88, bbox: (0.0, 0.0, 10.0, 10.0) }];
        assert_eq!(av.registrar(&det, Some(1000.0)), 1);
        assert_eq!(av.registrar(&det, Some(1050.0)), 0);
        assert_eq!(av.registrar(&det, Some(1200.0)), 1);
        let u = av.ultimo("celular").unwrap();
        assert_eq!(u["label"].as_str(), Some("cell phone"));
        assert_eq!(u["seen_at"].as_f64(), Some(1200.0));
        let cols: Vec<String> = store.query("PRAGMA table_info(object_sightings)", &[]).unwrap().into_iter().filter_map(|r| r["name"].as_str().map(String::from)).collect();
        assert_eq!(cols, vec!["id", "ts", "label", "confidence", "x", "y"]);
    }
}

//! Índice vetorial em memória sobre vetores guardados em tabela comum.
//!
//! Os vetores moram na tabela `vectors` (a fonte da verdade, vai no backup); a matriz só é
//! carregada na primeira busca e invalidada a cada escrita. Força bruta: para uma memória
//! pessoal responde em milissegundos.

use std::collections::HashMap;

/// float32 de norma 1: o produto interno passa a ser o cosseno.
pub fn normalizar(v: &[f32]) -> Vec<f32> {
    let norma = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norma > 0.0 { v.iter().map(|x| x / norma).collect() } else { v.to_vec() }
}

/// Bytes little-endian iguais aos do `numpy.tobytes()` do Python.
pub fn para_blob(v: &[f32]) -> Vec<u8> {
    normalizar(v).iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn de_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

#[derive(Default)]
pub struct VectorIndex {
    dados: HashMap<String, (Vec<i64>, Vec<Vec<f32>>)>,
}

impl VectorIndex {
    pub fn invalidar(&mut self, kind: Option<&str>) {
        match kind {
            None => self.dados.clear(),
            Some(k) => self.dados.retain(|key, _| key != k && !key.starts_with(&format!("{k}@"))),
        }
    }

    /// Ids dos `k` vetores mais parecidos com `consulta` (do mais para o menos).
    pub fn topk(
        &mut self,
        kind: &str,
        consulta: &[f32],
        k: usize,
        carregar: impl FnOnce() -> Vec<(i64, Vec<u8>)>,
    ) -> Vec<i64> {
        let (ids, matriz) = self.dados.entry(kind.to_string()).or_insert_with(|| {
            let linhas = carregar();
            (linhas.iter().map(|(i, _)| *i).collect(), linhas.iter().map(|(_, b)| de_blob(b)).collect())
        });
        let q = normalizar(consulta);
        if ids.is_empty() || matriz[0].len() != q.len() {
            return vec![];
        }
        let mut notas: Vec<(usize, f32)> = matriz
            .iter()
            .enumerate()
            .map(|(i, v)| (i, v.iter().zip(&q).map(|(a, b)| a * b).sum()))
            .collect();
        notas.sort_by(|a, b| b.1.total_cmp(&a.1));
        notas.into_iter().take(k).map(|(i, _)| ids[i]).collect()
    }
}

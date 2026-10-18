//! Vigília local da câmera e presença (E11.2, E11.3, E11.9; REGRAS.md: a câmera só liga por pedido
//! seu, e cada uso fica registrado).
//!
//! Roda nos jobs quando `ORION_CAMERA_WATCH` ou `ORION_PRESENCE_ENABLED` estão ligados. Nunca chama
//! rede, nunca grava foto, nunca aprova ferramenta.

use crate::camera::foto_jpeg;
use crate::presenca::{bloquear_tela, ocioso_segundos, sinais_do_sistema, Presenca, Sinais};
use crate::rosto::Rostos;
use crate::screen_memory::{tela_bloqueada, ScreenMemory};
use crate::visao_local::{olhar_uma_vez, Avistamentos, Detector};
use orion_store::ops::Operations;
use orion_store::Store;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::sync::Arc;

pub const CHAVE_BLOQUEIO: &str = "presenca:bloqueio";

pub fn acoes_de_presenca(
    memory: &Store,
    ops: Option<&Operations>,
    screen: Option<&ScreenMemory>,
    estado: &str,
    lock_enabled: bool,
    bloquear: &dyn Fn() -> bool,
) -> Vec<String> {
    let mut feitas = vec![];
    if estado == "ausente" {
        if let Some(s) = screen.filter(|s| !s.pausada()) {
            s.pausar(true);
            let _ = memory.meta_set("presenca:pausou_tela", Some("1"));
            feitas.push("tela_pausada".into());
        }
        if lock_enabled {
            let modo = memory.meta_get(CHAVE_BLOQUEIO).unwrap_or_default();
            if modo == "sempre" {
                if bloquear() {
                    feitas.push("tela_bloqueada".into());
                }
            } else if modo != "desativado" {
                if let Some(o) = ops {
                    let _ = o.notify(
                        "presenca",
                        "Você levantou — quer que eu bloqueie a tela da próxima vez? (`orion presenca confirmar` ativa; `orion presenca desfazer` desliga)",
                        Some("presenca:primeira"),
                        false,
                    );
                }
                feitas.push("perguntou_bloqueio".into());
            }
        }
    } else if estado == "presente" && memory.meta_get("presenca:pausou_tela").as_deref() == Some("1") {
        if let Some(s) = screen {
            s.pausar(false);
        }
        let _ = memory.meta_set("presenca:pausou_tela", None);
        feitas.push("tela_retomada".into());
    }
    feitas
}

pub type LerSinais = Arc<dyn Fn() -> Sinais + Send + Sync>;
pub type BloquearFn = Arc<dyn Fn() -> bool + Send + Sync>;

pub struct Vigia {
    pub memory: Arc<Store>,
    pub ops: Option<Arc<Operations>>,
    pub screen: Option<Arc<ScreenMemory>>,
    pub dispositivo: i32,
    pub watch_s: f64,
    pub detector: Option<Arc<Detector>>,
    pub avistamentos: Option<Arc<Avistamentos>>,
    pub presenca: Option<Arc<Presenca>>,
    pub rostos: Option<Arc<Rostos>>,
    pub bt_alvo: String,
    pub lock_enabled: bool,
    pub ler_sinais: Option<LerSinais>,
    pub bloquear: BloquearFn,
    ultima_camera: Mutex<f64>,
    ultima_poda: Mutex<String>,
}

impl Vigia {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        memory: Arc<Store>,
        ops: Option<Arc<Operations>>,
        screen: Option<Arc<ScreenMemory>>,
        dispositivo: i32,
        watch_s: u64,
        detector: Option<Arc<Detector>>,
        avistamentos: Option<Arc<Avistamentos>>,
        presenca: Option<Arc<Presenca>>,
        rostos: Option<Arc<Rostos>>,
        bt_alvo: &str,
        lock_enabled: bool,
    ) -> Self {
        Self {
            memory,
            ops,
            screen,
            dispositivo,
            watch_s: if watch_s > 0 { (watch_s.max(30)) as f64 } else { 0.0 },
            detector,
            avistamentos,
            presenca,
            rostos,
            bt_alvo: bt_alvo.into(),
            lock_enabled,
            ler_sinais: None,
            bloquear: Arc::new(bloquear_tela),
            ultima_camera: Mutex::new(f64::NEG_INFINITY),
            ultima_poda: Mutex::new(String::new()),
        }
    }

    pub fn tick(&self, suspensa: bool) -> Value {
        let mut rel = json!({"objetos": [], "presenca": Value::Null, "acoes": []});
        if suspensa {
            return rel;
        }
        let agora = (self.memory.clock())();
        if let Some(av) = &self.avistamentos {
            let hoje = crate::util::local(agora).format("%Y%m%d").to_string();
            let mut p = self.ultima_poda.lock();
            if *p != hoje {
                *p = hoje;
                av.podar();
            }
        }
        let pode_camera = {
            let u = *self.ultima_camera.lock();
            self.watch_s > 0.0 && (agora - u) >= self.watch_s && tela_bloqueada() != Some(true)
        };
        if pode_camera {
            if let (Some(det), Some(av)) = (&self.detector, &self.avistamentos) {
                *self.ultima_camera.lock() = agora;
                rel["objetos"] = json!(olhar_uma_vez(self.dispositivo, det, av));
            }
        }
        if let Some(pres) = &self.presenca {
            let sinais = match &self.ler_sinais {
                Some(f) => f(),
                None => sinais_padrao(&self.bt_alvo, self.rostos.as_deref(), self.dispositivo, pres.away_s),
            };
            let (estado, mudou) = pres.avaliar(sinais);
            rel["presenca"] = json!(estado);
            if mudou {
                let acoes = acoes_de_presenca(
                    &self.memory,
                    self.ops.as_deref(),
                    self.screen.as_deref(),
                    estado,
                    self.lock_enabled,
                    &*self.bloquear,
                );
                tracing::info!(target: "orion.audit", "presenca_mudou estado={estado} acoes={acoes:?}");
                rel["acoes"] = json!(acoes);
            }
        }
        rel
    }
}

pub fn sinais_padrao(bt_alvo: &str, rostos: Option<&Rostos>, dispositivo: i32, away_s: f64) -> Sinais {
    let ocioso = ocioso_segundos();
    let mut rosto_presente = None;
    if let (Some(r), Some(oc)) = (rostos, ocioso) {
        if oc >= away_s && r.disponivel() && r.cadastrado("antonio") {
            if let Ok(jpeg) = foto_jpeg(dispositivo, 6.0) {
                rosto_presente = r.e_voce(&jpeg, "antonio");
                tracing::info!(target: "orion.audit", "camera_presenca dispositivo={dispositivo} reconhecido={rosto_presente:?}");
            }
        }
    }
    let mut s = sinais_do_sistema(bt_alvo, rosto_presente);
    s.ocioso_s = ocioso;
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vigia_pausa_e_retoma_tela_e_pede_confirmacao_no_primeiro_bloqueio() {
        let store = Arc::new(Store::open(":memory:").unwrap());
        let ops = Arc::new(Operations::new(store.clone()));
        let tela = Arc::new(ScreenMemory::new(store.clone(), 60, 7, vec![], vec![], true, "por+eng"));
        let bloqueios = Arc::new(Mutex::new(0usize));
        let SinaisRef(sinais) = SinaisRef(Arc::new(Mutex::new(Sinais { ocioso_s: Some(200.0), bloqueada: Some(false), bt_perto: None, rosto_presente: None })));
        struct SinaisRef(Arc<Mutex<Sinais>>);
        let mut v = Vigia::new(
            store.clone(),
            Some(ops.clone()),
            Some(tela.clone()),
            0,
            0,
            None,
            None,
            Some(Arc::new(Presenca::new(2))),
            None,
            "",
            true,
        );
        let s_clone = sinais.clone();
        v.ler_sinais = Some(Arc::new(move || *s_clone.lock()));
        let b_clone = bloqueios.clone();
        v.bloquear = Arc::new(move || {
            *b_clone.lock() += 1;
            true
        });
        let r1 = v.tick(false);
        assert_eq!(r1["presenca"], json!("ausente"));
        assert!(tela.pausada());
        assert_eq!(*bloqueios.lock(), 0);
        assert_eq!(r1["acoes"], json!(["tela_pausada", "perguntou_bloqueio"]));

        *sinais.lock() = Sinais { ocioso_s: Some(1.0), bloqueada: Some(false), bt_perto: None, rosto_presente: None };
        let r2 = v.tick(false);
        assert_eq!(r2["presenca"], json!("presente"));
        assert!(!tela.pausada());

        store.meta_set(CHAVE_BLOQUEIO, Some("sempre")).unwrap();
        *sinais.lock() = Sinais { ocioso_s: Some(200.0), bloqueada: Some(false), bt_perto: None, rosto_presente: None };
        let r3 = v.tick(false);
        assert_eq!(*bloqueios.lock(), 1);
        assert!(r3["acoes"].as_array().unwrap().contains(&json!("tela_bloqueada")));
    }
}

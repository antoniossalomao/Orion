//! Organizador de Downloads com regras suas (E9.4, REGRAS.md: automação sua vale pelo arquivo
//! concedido).
//!
//! `<dados>/regras_downloads.yaml`: pasta opcional e `regras` (extensões, padrão de nome, destino).
//! Só roda depois de `orion downloads conceder` (SHA-256 do arquivo). Move só arquivos soltos da
//! pasta (nada de subpasta, link, arquivo em download ou parado há menos de 30 s), nunca
//! sobrescreve (o repetido ganha ` (2)`), só para destinos que a política de escrita aceitaria sem
//! perguntar e registra cada movimento para desfazer. O que não casa fica onde está.

use crate::automacao::{hash_do_arquivo, yaml_para_json, Concessoes};
use orion_policy::PathGuard;
use orion_store::Store;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const TIPO: &str = "downloads";
pub const NOME: &str = "regras";
pub const MAX_ARQUIVO: u64 = 16 * 1024;
pub const PARCIAIS: [&str; 6] = [".crdownload", ".part", ".partial", ".tmp", ".download", ".opdownload"];
pub const PARADO_S: f64 = 30.0;
pub const LOG_KEY: &str = "downloads:log";
pub const MAX_LOG: usize = 500;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regra {
    #[serde(default)]
    pub extensoes: Vec<String>,
    #[serde(default)]
    pub padrao: Option<String>,
    pub destino: String,
}

impl Regra {
    fn validar(&mut self) -> Result<(), String> {
        if self.extensoes.len() > 20 {
            return Err("extensoes: no máximo 20".into());
        }
        self.extensoes = self.extensoes.iter().filter(|e| !e.trim().is_empty()).map(|e| e.to_lowercase().trim_start_matches('.').to_string()).collect();
        if self.padrao.as_ref().is_some_and(|p| p.contains('/') || p.contains('\\')) {
            return Err("o padrão é só de nome de arquivo, sem caminho".into());
        }
        if self.padrao.as_ref().is_some_and(|p| p.chars().count() > 100) || !(1..=400).contains(&self.destino.chars().count()) {
            return Err("campo fora do limite".into());
        }
        Ok(())
    }

    pub fn casa(&self, nome: &str) -> bool {
        if self.extensoes.is_empty() && self.padrao.is_none() {
            return false; // regra sem condição não casa com tudo por engano
        }
        let ext = nome.rsplit_once('.').map(|x| x.1.to_lowercase()).unwrap_or_default();
        if !self.extensoes.is_empty() && !self.extensoes.contains(&ext) {
            return false;
        }
        self.padrao.as_ref().is_none_or(|p| fnmatch(&p.to_lowercase(), &nome.to_lowercase()))
    }
}

/// `*` e `?` como no `fnmatch`.
pub fn fnmatch(padrao: &str, texto: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (padrao.chars().collect(), texto.chars().collect());
    let (mut i, mut j, mut estrela, mut marca) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) && p[i] != '*' {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            estrela = Some(i);
            marca = j;
            i += 1;
        } else if let Some(e) = estrela {
            i = e + 1;
            marca += 1;
            j = marca;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regras {
    #[serde(default)]
    pub pasta: Option<String>,
    pub regras: Vec<Regra>,
}

#[derive(Debug, Default, Clone)]
pub struct Relatorio {
    pub movidos: Vec<Value>,
    pub ficaram: usize,
    pub recusados: Vec<String>,
    /// ok | nao_concedida | sem_regras | erro
    pub status: String,
}

pub struct Downloads {
    pub arquivo: PathBuf,
    memory: Arc<Store>,
    guard: PathGuard,
    pub concessoes: Concessoes,
    padrao: PathBuf,
}

fn expandir(p: &str) -> PathBuf {
    match p.strip_prefix("~/").or(if p == "~" { Some("") } else { None }) {
        Some(resto) => dirs_home().join(resto),
        None => PathBuf::from(p),
    }
}

pub fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

fn livre(alvo: &Path) -> std::io::Result<PathBuf> {
    if !alvo.exists() {
        return Ok(alvo.into());
    }
    let (stem, ext) = (alvo.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(), alvo.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default());
    for n in 2..1000 {
        let c = alvo.with_file_name(format!("{stem} ({n}){ext}"));
        if !c.exists() {
            return Ok(c);
        }
    }
    Err(std::io::Error::other("sem nome livre no destino"))
}

fn mover(de: &Path, para: &Path) -> std::io::Result<()> {
    if let Some(d) = para.parent() {
        std::fs::create_dir_all(d)?;
    }
    if std::fs::rename(de, para).is_err() {
        std::fs::copy(de, para)?;
        std::fs::remove_file(de)?;
    }
    Ok(())
}

impl Downloads {
    pub fn new(arquivo: PathBuf, memory: Arc<Store>, guard: PathGuard) -> Self {
        Downloads { arquivo, concessoes: Concessoes::new(memory.clone()), memory, guard, padrao: dirs_home().join("Downloads") }
    }

    /// (regras, hash, erro). Sem arquivo: (None, "", "").
    pub fn carregar(&self) -> (Option<Regras>, String, String) {
        if !self.arquivo.is_file() {
            return (None, String::new(), String::new());
        }
        let r = (|| -> Result<(Regras, String), String> {
            let meta = std::fs::symlink_metadata(&self.arquivo).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() || meta.len() > MAX_ARQUIVO {
                return Err("arquivo grande demais ou link".into());
            }
            let h = hash_do_arquivo(&self.arquivo).map_err(|e| e.to_string())?;
            let texto = std::fs::read_to_string(&self.arquivo).map_err(|e| e.to_string())?;
            let y = yaml_para_json(&texto)?;
            if !y.is_object() {
                return Err("o arquivo precisa ser um mapa YAML".into());
            }
            let mut r: Regras = serde_json::from_value(y).map_err(|e| e.to_string())?;
            if r.regras.is_empty() || r.regras.len() > 50 || r.pasta.as_ref().is_some_and(|p| p.chars().count() > 400) {
                return Err("regras: entre 1 e 50".into());
            }
            for x in &mut r.regras {
                x.validar()?;
            }
            Ok((r, h))
        })();
        match r {
            Ok((r, h)) => (Some(r), h, String::new()),
            Err(e) => (None, String::new(), e.chars().take(300).collect()),
        }
    }

    fn pasta(&self, r: Option<&Regras>) -> PathBuf {
        r.and_then(|r| r.pasta.as_deref()).map(expandir).unwrap_or_else(|| self.padrao.clone())
    }

    pub fn estado(&self) -> Value {
        let (r, h, erro) = self.carregar();
        json!({
            "arquivo": self.arquivo.display().to_string(), "existe": self.arquivo.is_file(),
            "erro": if erro.is_empty() { Value::Null } else { json!(erro) },
            "estado": if r.is_some() { self.concessoes.estado(TIPO, NOME, &h) } else { "sem_concessao" },
            "regras": r.as_ref().map(|r| r.regras.len()).unwrap_or(0),
            "pasta": self.pasta(r.as_ref()).display().to_string(),
        })
    }

    pub fn conceder(&self) -> Result<Value, String> {
        let (r, h, erro) = self.carregar();
        if r.is_none() {
            return Err(if erro.is_empty() { "não há regras_downloads.yaml".into() } else { erro });
        }
        self.concessoes.conceder(TIPO, NOME, &h).map_err(|e| e.to_string())?;
        Ok(self.estado())
    }

    pub fn revogar(&self) -> bool {
        self.concessoes.revogar(TIPO, NOME).unwrap_or(false)
    }

    pub fn rodar(&self, simular: bool) -> Relatorio {
        let (r, h, _) = self.carregar();
        let mut rel = Relatorio { status: "ok".into(), ..Default::default() };
        let Some(regras) = r else {
            rel.status = "sem_regras".into();
            return rel;
        };
        if self.concessoes.estado(TIPO, NOME, &h) != "concedida" {
            rel.status = "nao_concedida".into();
            return rel;
        }
        let pasta = self.pasta(Some(&regras));
        let Ok(rd) = std::fs::read_dir(&pasta) else {
            rel.status = "erro".into();
            return rel;
        };
        let mut arquivos: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        arquivos.sort();
        let agora = crate::util::agora();
        for f in arquivos {
            let Ok(meta) = std::fs::symlink_metadata(&f) else { continue };
            let nome = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if meta.file_type().is_symlink() || !meta.is_file() || nome.starts_with('.') {
                continue;
            }
            let mtime = meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs_f64()).unwrap_or(0.0);
            if PARCIAIS.iter().any(|p| nome.to_lowercase().ends_with(p)) || agora - mtime < PARADO_S {
                rel.ficaram += 1;
                continue;
            }
            let Some(regra) = regras.regras.iter().find(|r| r.casa(&nome)) else {
                rel.ficaram += 1;
                continue;
            };
            let destino_dir = expandir(&regra.destino);
            if let Some(motivo) = self.guard.check_write(&destino_dir.join(&nome).to_string_lossy()) {
                rel.recusados.push(format!("{nome}: {motivo}"));
                continue;
            }
            let Ok(alvo) = livre(&destino_dir.join(&nome)) else { continue };
            if !simular {
                if mover(&f, &alvo).is_err() {
                    rel.recusados.push(format!("{nome}: não consegui mover"));
                    continue;
                }
                self.registrar(&f, &alvo);
            }
            rel.movidos.push(json!({"de": f.display().to_string(), "para": alvo.display().to_string()}));
        }
        rel
    }

    fn log(&self) -> Vec<Value> {
        self.memory.meta_get(LOG_KEY).and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v.as_array().cloned()).unwrap_or_default()
    }

    fn salvar_log(&self, l: &[Value]) {
        let _ = self.memory.meta_set(LOG_KEY, Some(&serde_json::to_string(l).unwrap_or_else(|_| "[]".into())));
    }

    fn registrar(&self, de: &Path, para: &Path) {
        let mut l = self.log();
        l.push(json!({"ts": crate::util::agora(), "de": de.display().to_string(), "para": para.display().to_string(), "desfeito": false}));
        let n = l.len();
        self.salvar_log(&l[n.saturating_sub(MAX_LOG)..]);
    }

    pub fn historico(&self, limite: usize) -> Vec<Value> {
        self.log().into_iter().rev().take(limite.clamp(1, MAX_LOG)).collect()
    }

    /// `ultimo` desfaz o último movimento; `hoje` desfaz todos de hoje (do mais novo ao mais velho).
    /// Só volta o que ainda está onde foi posto e cujo lugar de origem está livre.
    pub fn desfazer(&self, escopo: &str) -> Result<Value, String> {
        if !["ultimo", "hoje"].contains(&escopo) {
            return Err("escopo: ultimo ou hoje".into());
        }
        let mut lista = self.log();
        let inicio = {
            use chrono::TimeZone;
            let d = crate::util::local(crate::util::agora()).date_naive().and_hms_opt(0, 0, 0).unwrap();
            chrono::Local.from_local_datetime(&d).earliest().map(|x| x.timestamp() as f64).unwrap_or(0.0)
        };
        let mut candidatos: Vec<usize> = (0..lista.len()).rev().filter(|&i| lista[i]["desfeito"] != json!(true) && (escopo == "ultimo" || lista[i]["ts"].as_f64().unwrap_or(0.0) >= inicio)).collect();
        if escopo == "ultimo" {
            candidatos.truncate(1);
        }
        let (mut desfeitos, mut pulados) = (vec![], vec![]);
        for i in candidatos {
            let (origem, atual) = (PathBuf::from(lista[i]["de"].as_str().unwrap_or("")), PathBuf::from(lista[i]["para"].as_str().unwrap_or("")));
            let nome = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if !atual.is_file() {
                pulados.push(format!("{}: não está mais onde foi posto", nome(&atual)));
            } else if origem.exists() {
                pulados.push(format!("{}: o lugar de origem já está ocupado", nome(&origem)));
            } else if mover(&atual, &origem).is_ok() {
                lista[i]["desfeito"] = json!(true);
                desfeitos.push(lista[i]["de"].clone());
            }
        }
        self.salvar_log(&lista);
        Ok(json!({"desfeitos": desfeitos, "pulados": pulados}))
    }
}

pub const EXEMPLO: &str = "\
# Regras de Downloads. Para valer: orion downloads conceder
pasta: ~/Downloads
regras:
  - {extensoes: [pdf], padrao: \"fatura*\", destino: ~/Documents/Faturas}
  - {extensoes: [jpg, jpeg, png, webp], destino: ~/Documents/Imagens baixadas}
";

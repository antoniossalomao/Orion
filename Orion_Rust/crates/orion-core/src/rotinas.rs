//! Rotinas (E9.1; REGRAS.md: automação sua vale pelo arquivo concedido): uma sequência de passos
//! que você escreveu e concedeu.
//!
//! `<dados>/rotinas/<nome>.yaml` com `descricao`, `gatilhos`, `liberar` e `passos`. Quem decide o
//! que roda é o arquivo, não o modelo. Passo de leitura roda direto; `controlar_midia` e, se a
//! rotina listar em `liberar`, `abrir_app` rodam direto com os argumentos fixos do arquivo; tudo o
//! mais vira o pedido de aprovação de sempre (botão) e a rotina para ali. Cada passo vai ao audit.

use crate::automacao::{hash_do_arquivo, yaml_para_json, Concessoes};
use crate::modos::Modos;
use crate::tools::registry::ToolRegistry;
use orion_policy::{PolicyEngine, Risk};
use orion_store::ops::Operations;
use orion_store::Store;
use parking_lot::{Mutex, RwLock};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use unicode_normalization::UnicodeNormalization;

static NOME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,31}$").unwrap());
static FERRAMENTA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_]{0,63}$").unwrap());
pub const MAX_ARQUIVO: u64 = 16 * 1024;
pub const MAX_PASSOS: usize = 20;
/// Rodam direto em qualquer rotina concedida.
pub const DIRETAS: [&str; 1] = ["controlar_midia"];
/// Só se a rotina listar em `liberar`.
pub const LIBERAVEIS: [&str; 1] = ["abrir_app"];
pub const TIPO: &str = "rotina";
/// O não perturbe segura estes.
pub const GATILHOS_AUTOMATICOS: [&str; 2] = ["palmas", "tecla"];

pub fn normalizar_frase(texto: &str) -> String {
    let sem: String = texto.to_lowercase().nfkd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect();
    let limpo: String = sem.chars().map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() || c == ' ' { c } else { ' ' }).collect();
    limpo.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gatilho {
    pub tipo: String,
    #[serde(default)]
    pub valor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Passo {
    #[serde(default)]
    pub ferramenta: Option<String>,
    #[serde(default)]
    pub argumentos: Map<String, Value>,
    #[serde(default)]
    pub falar: Option<String>,
    #[serde(default)]
    pub briefing: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rotina {
    #[serde(default)]
    pub descricao: String,
    #[serde(default)]
    pub gatilhos: Vec<Gatilho>,
    #[serde(default)]
    pub liberar: Vec<String>,
    pub passos: Vec<Passo>,
}

impl Rotina {
    pub fn validar(&self) -> Result<(), String> {
        if self.descricao.chars().count() > 200 || self.gatilhos.len() > 10 || self.liberar.len() > 5 {
            return Err("campo fora do limite".into());
        }
        if self.passos.is_empty() || self.passos.len() > MAX_PASSOS {
            return Err(format!("passos: entre 1 e {MAX_PASSOS}"));
        }
        for g in &self.gatilhos {
            if !["frase", "palmas", "tecla", "telegram"].contains(&g.tipo.as_str()) {
                return Err(format!("gatilhos: tipo '{}' inválido", g.tipo));
            }
            if g.valor.as_ref().is_some_and(|v| v.chars().count() > 80) {
                return Err("gatilhos: valor grande demais".into());
            }
            if g.tipo == "frase" && g.valor.as_deref().is_none_or(|v| normalizar_frase(v).is_empty()) {
                return Err("gatilho 'frase' precisa de um valor".into());
            }
        }
        if let Some(ruim) = self.liberar.iter().find(|x| !LIBERAVEIS.contains(&x.as_str())) {
            return Err(format!("'liberar' só aceita: {} (não {ruim})", LIBERAVEIS.join(", ")));
        }
        for p in &self.passos {
            if p.ferramenta.as_ref().is_some_and(|f| !FERRAMENTA.is_match(f)) || p.falar.as_ref().is_some_and(|f| f.chars().count() > 500) {
                return Err("passo inválido".into());
            }
            let tipos = [p.ferramenta.is_some(), p.falar.is_some(), p.briefing == Some(true)];
            if tipos.iter().filter(|t| **t).count() != 1 {
                return Err("cada passo tem exatamente um de: ferramenta, falar, briefing".into());
            }
            if !p.argumentos.is_empty() && p.ferramenta.is_none() {
                return Err("'argumentos' só vale com 'ferramenta'".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Carregada {
    pub nome: String,
    pub arquivo: PathBuf,
    pub hash: String,
    pub rotina: Option<Rotina>,
    pub erro: Option<String>,
    /// concedida | mudou | sem_concessao
    pub estado: &'static str,
}

#[derive(Debug, Clone)]
pub struct Resultado {
    pub nome: String,
    /// concluida | aguardando_aprovacao | recusada | falhou | nao_concedida
    pub status: String,
    pub passos: Vec<Value>,
    pub aprovacao_id: Option<String>,
    pub motivo: String,
}

impl Resultado {
    fn novo(nome: &str, status: &str, motivo: &str) -> Self {
        Resultado { nome: nome.into(), status: status.into(), passos: vec![], aprovacao_id: None, motivo: motivo.into() }
    }

    pub fn publico(&self) -> Value {
        json!({"nome": self.nome, "status": self.status, "passos": self.passos, "aprovacao_id": self.aprovacao_id, "motivo": self.motivo})
    }
}

pub struct RotinaStore {
    pub pasta: PathBuf,
    pub concessoes: Concessoes,
}

impl RotinaStore {
    pub fn new(pasta: PathBuf, memory: Arc<Store>) -> Self {
        RotinaStore { pasta, concessoes: Concessoes::new(memory) }
    }

    fn ler(&self, arquivo: &std::path::Path) -> Carregada {
        let nome = arquivo.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let r = (|| -> Result<(Rotina, String), String> {
            let meta = std::fs::symlink_metadata(arquivo).map_err(|e| e.to_string())?;
            if meta.file_type().is_symlink() || meta.len() > MAX_ARQUIVO {
                return Err("arquivo grande demais ou link".into());
            }
            let h = hash_do_arquivo(arquivo).map_err(|e| e.to_string())?;
            let y = yaml_para_json(&std::fs::read_to_string(arquivo).map_err(|e| e.to_string())?)?;
            if !y.is_object() {
                return Err("o arquivo precisa ser um mapa YAML".into());
            }
            let r: Rotina = serde_json::from_value(y).map_err(|e| e.to_string())?;
            r.validar()?;
            Ok((r, h))
        })();
        match r {
            Ok((rotina, h)) => {
                let estado = self.concessoes.estado(TIPO, &nome, &h);
                Carregada { nome, arquivo: arquivo.into(), hash: h, rotina: Some(rotina), erro: None, estado }
            }
            Err(e) => Carregada { nome, arquivo: arquivo.into(), hash: String::new(), rotina: None, erro: Some(e.chars().take(300).collect()), estado: "sem_concessao" },
        }
    }

    pub fn listar(&self) -> Vec<Carregada> {
        let Ok(rd) = std::fs::read_dir(&self.pasta) else { return vec![] };
        let mut arqs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "yaml") && p.is_file()).collect();
        arqs.sort();
        arqs.iter().filter(|a| a.file_stem().is_some_and(|s| NOME.is_match(&s.to_string_lossy()))).map(|a| self.ler(a)).collect()
    }

    pub fn obter(&self, nome: &str) -> Option<Carregada> {
        if !NOME.is_match(nome) {
            return None;
        }
        let a = self.pasta.join(format!("{nome}.yaml"));
        a.is_file().then(|| self.ler(&a))
    }

    /// `Ok(None)`: não existe.
    pub fn conceder(&self, nome: &str) -> Result<Option<Carregada>, String> {
        let Some(c) = self.obter(nome) else { return Ok(None) };
        if c.rotina.is_none() {
            return Err(format!("a rotina '{nome}' tem erro: {}", c.erro.unwrap_or_default()));
        }
        self.concessoes.conceder(TIPO, nome, &c.hash).map_err(|e| e.to_string())?;
        Ok(Some(self.ler(&c.arquivo)))
    }

    pub fn revogar(&self, nome: &str) -> bool {
        self.concessoes.revogar(TIPO, nome).unwrap_or(false)
    }

    /// A rotina concedida cuja frase bate exatamente (sem acento, caixa nem pontuação).
    pub fn por_frase(&self, texto: &str) -> Option<Carregada> {
        let alvo = normalizar_frase(texto);
        if alvo.is_empty() {
            return None;
        }
        self.listar().into_iter().find(|c| c.estado == "concedida" && c.rotina.as_ref().is_some_and(|r| r.gatilhos.iter().any(|g| g.tipo == "frase" && normalizar_frase(g.valor.as_deref().unwrap_or("")) == alvo)))
    }
}

pub type Briefing = Arc<dyn Fn() -> String + Send + Sync>;

/// Roda uma rotina concedida.
pub struct Executor {
    pub store: Arc<RotinaStore>,
    pub tools: Arc<RwLock<ToolRegistry>>,
    pub policy: Arc<PolicyEngine>,
    pub memory: Arc<Store>,
    pub ops: Arc<Operations>,
    pub briefing: Option<Briefing>,
    pub modos: Option<Arc<Modos>>,
    pub passo_timeout_s: f64,
    rodando: Mutex<HashSet<String>>,
}

impl Executor {
    pub fn new(store: Arc<RotinaStore>, tools: Arc<RwLock<ToolRegistry>>, policy: Arc<PolicyEngine>, memory: Arc<Store>, ops: Arc<Operations>) -> Self {
        Executor { store, tools, policy, memory, ops, briefing: None, modos: None, passo_timeout_s: 60.0, rodando: Mutex::new(HashSet::new()) }
    }

    fn bloqueio(&self, origem: &str) -> &'static str {
        let Some(m) = &self.modos else { return "" };
        if m.panico() {
            "modo pânico"
        } else if GATILHOS_AUTOMATICOS.contains(&origem) && m.nao_perturbe() {
            "não perturbe"
        } else {
            ""
        }
    }

    fn audit(&self, nome: &str, passo: &Passo, acao: &str, risco: Option<&str>, motivo: &str) {
        let args: Map<String, Value> = passo.argumentos.iter().map(|(k, v)| (k.clone(), json!(tipo_py(v)))).collect();
        let _ = self.ops.audit_add(&json!({
            "tool": format!("rotina_{}", passo.ferramenta.as_deref().unwrap_or("texto")),
            "action": acao, "risk": risco,
            "reason": format!("rotina={nome}; {motivo}").chars().take(200).collect::<String>(),
            "args": args,
        }));
    }

    pub async fn executar(&self, nome: &str, origem: &str) -> Resultado {
        let Some(c) = self.store.obter(nome) else { return Resultado::novo(nome, "falhou", "rotina não existe") };
        let Some(rotina) = c.rotina.clone() else { return Resultado::novo(nome, "falhou", c.erro.as_deref().unwrap_or("rotina não existe")) };
        if c.estado != "concedida" {
            let motivo = if c.estado == "mudou" { "o arquivo mudou depois da concessão" } else { "sem concessão" };
            return Resultado::novo(nome, "nao_concedida", &format!("{motivo}: rode `orion rotina conceder {nome}`"));
        }
        let exige: Option<&[&str]> = match origem {
            "frase" => Some(&["frase"]),
            "telegram" => Some(&["telegram", "frase"]),
            _ => None,
        };
        if exige.is_some_and(|e| !rotina.gatilhos.iter().any(|g| e.contains(&g.tipo.as_str()))) {
            return Resultado::novo(nome, "recusada", &format!("a rotina não tem gatilho '{origem}'"));
        }
        let b = self.bloqueio(origem);
        if !b.is_empty() {
            return Resultado::novo(nome, "recusada", b);
        }
        if !self.rodando.lock().insert(nome.to_string()) {
            return Resultado::novo(nome, "recusada", "essa rotina já está rodando");
        }
        let r = self.rodar(&c, &rotina, origem).await;
        self.rodando.lock().remove(nome);
        r
    }

    async fn rodar(&self, c: &Carregada, rotina: &Rotina, origem: &str) -> Resultado {
        let nome = c.nome.as_str();
        let mut res = Resultado::novo(nome, "concluida", "");
        let mut sessao: Option<String> = None;
        for (i, passo) in rotina.passos.iter().enumerate() {
            let i = i + 1;
            if self.store.concessoes.estado(TIPO, nome, &c.hash) != "concedida" {
                res.status = "recusada".into();
                res.motivo = "a concessão caiu durante a rotina".into();
                break;
            }
            let tipo = if passo.falar.is_some() { "falar".to_string() } else if passo.briefing == Some(true) { "briefing".to_string() } else { passo.ferramenta.clone().unwrap_or_default() };
            let mut info = json!({"n": i, "tipo": tipo});
            if let Some(f) = &passo.falar {
                self.audit(nome, passo, "allow", Some("read"), &format!("origem={origem}; falar"));
                let _ = self.ops.notify("rotina", f, None, true);
                info["estado"] = json!("ok");
            } else if passo.briefing == Some(true) {
                self.audit(nome, passo, "allow", Some("read"), &format!("origem={origem}; briefing"));
                let texto = self.briefing.as_ref().map(|b| b()).unwrap_or_else(|| "(briefing indisponível)".into());
                let _ = self.ops.notify("rotina", &texto, None, true);
                info["estado"] = json!("ok");
            } else {
                let fer = passo.ferramenta.clone().unwrap_or_default();
                let spec = self.policy.spec(&fer);
                let existe = self.tools.read().get(&fer).is_some();
                let Some(spec) = spec.filter(|_| existe) else {
                    res.status = "falhou".into();
                    res.motivo = format!("ferramenta '{fer}' não existe aqui");
                    info["estado"] = json!("falhou");
                    res.passos.push(info);
                    break;
                };
                let direto = (spec.risk == Some(Risk::Read) && !spec.egress && !spec.external) || DIRETAS.contains(&fer.as_str()) || (LIBERAVEIS.contains(&fer.as_str()) && rotina.liberar.contains(&fer));
                let risco = spec.risk.map(|r| r.as_str());
                if !direto {
                    if sessao.is_none() {
                        let canal = if origem == "telegram" { "telegram" } else { "web" };
                        match self.memory.new_session(canal, Some(&format!("Rotina: {nome}")), None) {
                            Ok(s) => sessao = Some(s.id),
                            Err(e) => {
                                res.status = "falhou".into();
                                res.motivo = e.to_string();
                                break;
                            }
                        }
                    }
                    let a = self.policy.approvals.request(sessao.as_deref().unwrap_or(""), &fer, &passo.argumentos, &format!("passo {i} da rotina {nome}"), "");
                    self.audit(nome, passo, "confirm", risco, &format!("origem={origem}; passo {i}"));
                    res.status = "aguardando_aprovacao".into();
                    res.aprovacao_id = Some(a.id);
                    res.motivo = format!("o passo {i} ({fer}) espera a sua aprovação");
                    info["estado"] = json!("aguardando_aprovacao");
                    res.passos.push(info);
                    break;
                }
                self.audit(nome, passo, "allow", risco, &format!("origem={origem}; direto"));
                let tool = self.tools.read().get(&fer).cloned();
                let Some(tool) = tool else { continue };
                let bruto = tokio::time::timeout(std::time::Duration::from_secs_f64(self.passo_timeout_s), tool.run(passo.argumentos.clone())).await;
                match bruto {
                    Err(_) => {
                        info["estado"] = json!("falhou");
                        res.status = "falhou".into();
                        res.motivo = format!("passo {i}: tempo esgotado");
                        res.passos.push(info);
                        break;
                    }
                    Ok(texto) => {
                        let dados: Option<Value> = serde_json::from_str(&texto).ok();
                        if let Some(d) = dados.as_ref().filter(|d| d.is_object() && (d["ok"] == json!(false) || d.get("erro").is_some_and(|e| !e.is_null() && e != ""))) {
                            info["estado"] = json!("falhou");
                            let detalhe: String = d["erro"].as_str().filter(|e| !e.is_empty()).unwrap_or("falhou").chars().take(160).collect();
                            res.status = "falhou".into();
                            res.motivo = format!("passo {i}: {detalhe}");
                            res.passos.push(info);
                            break;
                        }
                        info["estado"] = json!("ok");
                    }
                }
            }
            res.passos.push(info);
        }
        let _ = self.ops.audit_add(&json!({
            "tool": "rotina", "action": "allow", "risk": "read",
            "reason": format!("{nome}: {}; origem={origem}", res.status).chars().take(200).collect::<String>(),
        }));
        res
    }
}

fn tipo_py(v: &Value) -> &'static str {
    match v {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
    }
}

pub const EXEMPLO_BOM_DIA: &str = "\
# Rotina de exemplo. Para valer: orion rotina conceder bom-dia
descricao: Bom dia — o briefing do dia, música e abrir o Orion
gatilhos:
  - {tipo: frase, valor: bom dia orion}
  - {tipo: telegram}
liberar: [abrir_app]   # sem isto o passo abrir_app pediria a sua aprovação
passos:
  - briefing: true
  - ferramenta: controlar_midia
    argumentos: {acao: tocar_pausar}
  - ferramenta: abrir_app
    argumentos: {nome: \"https://calendar.google.com\"}   # Windows/macOS abrem a página; troque pelo que você abre de manhã
  - falar: \"Bom dia! O briefing está nos avisos.\"
";

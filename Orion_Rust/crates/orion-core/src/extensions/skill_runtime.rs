//! Seleção progressiva de skills: origem por turno e escopo somente restritivo.

use super::context::{ExternalData, Selection};
use super::scripts::{snapshot, ScriptRunner};
use super::skills::{serr, SkillError, SkillIndex};
use crate::tools::registry::{words, Args, Tool, ToolRegistry};
use orion_policy::{PolicyEngine, Risk, ToolSpec};
use parking_lot::RwLock;
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

type R<T> = Result<T, SkillError>;

static NS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,47}$").unwrap());
static ESCOPO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(personal|project:[a-f0-9]{32})$").unwrap());

#[derive(Debug, Clone)]
pub struct SkillSource {
    pub root: PathBuf,
    pub namespace: String,
    pub origin: String,
    pub scope: String,
    pub enabled: bool,
    pub trusted_scripts: bool,
    pub script_timeout_s: f64,
}

impl SkillSource {
    pub fn parse(v: &Value) -> Result<Self, String> {
        let o = v.as_object().ok_or("fonte de skills precisa ser um objeto")?;
        let ok = ["root", "namespace", "origin", "scope", "enabled", "trusted_scripts", "script_timeout_s"];
        if let Some(k) = o.keys().find(|k| !ok.contains(&k.as_str())) {
            return Err(format!("campo desconhecido: {k}"));
        }
        let root = PathBuf::from(v["root"].as_str().ok_or("root obrigatório")?);
        if !root.is_absolute() {
            return Err("raiz de skills precisa ser absoluta".into());
        }
        let namespace = v["namespace"].as_str().filter(|n| NS.is_match(n)).ok_or("namespace inválido")?.to_string();
        let origin = v.get("origin").and_then(Value::as_str).unwrap_or("local").to_string();
        let scope = v.get("scope").and_then(Value::as_str).unwrap_or("personal").to_string();
        if origin.len() > 128 || !ESCOPO.is_match(&scope) {
            return Err("origin ou scope inválido".into());
        }
        let t = v.get("script_timeout_s").and_then(Value::as_f64).unwrap_or(5.0);
        if !(0.1..=30.0).contains(&t) {
            return Err("script_timeout_s fora de 0.1–30".into());
        }
        Ok(SkillSource { root, namespace, origin, scope, enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(false), trusted_scripts: v.get("trusted_scripts").and_then(Value::as_bool).unwrap_or(false), script_timeout_s: t })
    }
}

#[derive(Debug, Clone)]
pub struct SkillReference {
    pub skill: String,
    pub path: String,
}

impl SkillReference {
    pub fn parse(v: &Value) -> Result<Self, String> {
        let o = v.as_object().ok_or("referência precisa ser um objeto")?;
        if o.keys().any(|k| k != "skill" && k != "path") {
            return Err("campo desconhecido".into());
        }
        let skill = v["skill"].as_str().filter(|s| s.len() <= 120).ok_or("skill inválida")?.to_string();
        let path = v["path"].as_str().filter(|s| !s.is_empty() && s.len() <= 1024).ok_or("path inválido")?.to_string();
        Ok(SkillReference { skill, path })
    }
}

#[derive(Default)]
pub struct Inner {
    pub index: SkillIndex,
    pub enabled: HashSet<String>,
    pub scopes: HashMap<String, HashSet<String>>,
    pub authorities: HashMap<String, String>,
    pub contexts: HashMap<String, String>,
    pub script_reviews: BTreeMap<String, (String, f64)>,
    pub diagnostics: Vec<Value>,
    /// Skills que você desligou na tela: id → (contexto, revisão de scripts). Voltam sem nova revisão.
    pub pausadas: HashMap<String, (String, Option<(String, f64)>)>,
}

pub struct SkillRuntime {
    pub runner: ScriptRunner,
    pub inner: RwLock<Inner>,
}

fn cortar(texto: &str, max: usize) -> &str {
    let mut fim = texto.len().min(max);
    while fim > 0 && !texto.is_char_boundary(fim) {
        fim -= 1;
    }
    &texto[..fim]
}

impl SkillRuntime {
    pub fn new(sources: &[SkillSource]) -> Arc<Self> {
        let mut inner = Inner::default();
        for s in sources {
            let antes: HashSet<String> = inner.index.skills.keys().cloned().collect();
            let r: R<()> = (|| {
                inner.index.discover(&s.root, &s.namespace, &s.origin)?;
                if s.enabled {
                    let adicionadas: Vec<String> = inner.index.skills.keys().filter(|k| !antes.contains(*k)).cloned().collect();
                    let mut revisoes = vec![];
                    if s.trusted_scripts && cfg!(unix) {
                        for id in &adicionadas {
                            let sk = &inner.index.skills[id];
                            sk.load()?;
                            let (digest, _) = snapshot(sk)?;
                            revisoes.push((id.clone(), (digest, s.script_timeout_s)));
                        }
                    }
                    for id in adicionadas {
                        inner.enabled.insert(id.clone());
                        inner.contexts.insert(id, s.scope.clone());
                    }
                    inner.script_reviews.extend(revisoes);
                }
                Ok(())
            })();
            if r.is_err() {
                inner.diagnostics.push(json!({"namespace": s.namespace, "code": "source_invalid"}));
            }
        }
        Arc::new(SkillRuntime { runner: ScriptRunner::default(), inner: RwLock::new(inner) })
    }

    /// Desliga a skill sem mexer na fonte (some do chat e dos pedidos). `false` se não está ligada.
    pub fn pausar(&self, id: &str) -> bool {
        let mut g = self.inner.write();
        if !g.enabled.remove(id) {
            return false;
        }
        let ctx = g.contexts.get(id).cloned().unwrap_or_else(|| "personal".into());
        let rev = g.script_reviews.remove(id);
        g.pausadas.insert(id.to_string(), (ctx, rev));
        true
    }

    /// Liga de volta uma skill que você pausou. Skill de fonte desligada nunca entra aqui.
    pub fn retomar(&self, id: &str) -> bool {
        let mut g = self.inner.write();
        let Some((ctx, rev)) = g.pausadas.remove(id) else { return false };
        g.enabled.insert(id.to_string());
        g.contexts.insert(id.to_string(), ctx);
        if let Some(r) = rev {
            g.script_reviews.insert(id.to_string(), r);
        }
        true
    }

    pub fn ids_pausadas(&self) -> Vec<String> {
        let mut v: Vec<String> = self.inner.read().pausadas.keys().cloned().collect();
        v.sort();
        v
    }

    pub fn close(&self) {
        self.inner.write().script_reviews.clear();
        self.runner.close();
    }

    pub fn attach_tools(self: &Arc<Self>, registry: &Arc<RwLock<ToolRegistry>>, policy: &PolicyEngine) {
        let nomes: Vec<String> = self.inner.read().script_reviews.keys().cloned().collect();
        if nomes.is_empty() {
            return;
        }
        let me = self.clone();
        let name = "executar_skill_script";
        let params = json!({"type": "object", "additionalProperties": false, "properties": {
            "skill": {"type": "string", "enum": nomes},
            "script": {"type": "string", "maxLength": 1024},
            "argv": {"type": "array", "maxItems": 32, "items": {"type": "string", "maxLength": 4096}}},
            "required": ["skill", "script"]});
        let t = Tool::new(name, "Executa script Python declarado de skill local revisada. Sempre exige aprovação, sem shell e sem herdar credenciais.", params, move |a: Args| {
            let me = me.clone();
            async move {
                let (skill, script) = (a.s("skill")?, a.s("script")?);
                let argv = a.list_s("argv");
                let (sk, rev) = {
                    let g = me.inner.read();
                    match (g.enabled.contains(&skill), g.script_reviews.get(&skill), g.index.skills.get(&skill)) {
                        (true, Some(r), Some(s)) => (s.clone(), r.clone()),
                        _ => return Ok(json!({"ok": false, "codigo": "scripts_untrusted_or_disabled"})),
                    }
                };
                Ok(match me.runner.run(&sk, &script, &argv, &rev.0, rev.1).await {
                    Ok(v) => v,
                    Err(e) => json!({"ok": false, "codigo": e.0}),
                })
            }
        })
        .validando();
        let _ = registry.write().register(t);
        let _ = policy.register_tool(ToolSpec { external: true, origin: Some("orion:skill-script".into()), masked_args: vec!["argv".into()], ..ToolSpec::new(name, Risk::Exec) }, Some((5, 60)));
    }

    pub fn summaries(&self) -> Vec<Value> {
        let g = self.inner.read();
        g.index
            .summaries()
            .into_iter()
            .map(|mut s| {
                let id = s["id"].as_str().unwrap_or("").to_string();
                let pausada = g.pausadas.contains_key(&id);
                s["enabled"] = json!(g.enabled.contains(&id));
                // só dá para ligar e desligar o que a fonte (mcp/skills) já deixou ligado
                s["ligavel"] = json!(pausada || g.enabled.contains(&id));
                s["scope"] = json!(g.contexts.get(&id).cloned().unwrap_or_else(|| "personal".into()));
                s
            })
            .collect()
    }

    pub fn select(self: &Arc<Self>, query: &str, explicit: &[String], references: &[SkillReference], budget: usize, context: &str) -> R<Selection> {
        let mut ids: Vec<String> = vec![];
        for e in explicit {
            if !ids.contains(e) {
                ids.push(e.clone());
            }
        }
        if ids.len() > 3 || references.len() > 8 {
            return Err(serr("skill_selection_too_many"));
        }
        let g = self.inner.read();
        if ids.is_empty() {
            let palavras = words(query);
            let mut ranked: Vec<(usize, String)> = vec![];
            for id in &g.enabled {
                if g.contexts.get(id).map(String::as_str).unwrap_or("personal") != context {
                    continue;
                }
                let Some(sk) = g.index.skills.get(id) else { continue };
                let score = palavras.intersection(&words(&sk.header.description)).count();
                if score >= 2 {
                    ranked.push((score, id.clone()));
                }
            }
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            ids = ranked.into_iter().take(3).map(|r| r.1).collect();
        }
        let (mut data, mut provenance) = (vec![], vec![]);
        let mut scope: Option<HashSet<String>> = None;
        let mut loaded = HashMap::new();
        let mut restante = budget.min(24000);
        let restringe = |scope: &mut Option<HashSet<String>>, r: HashSet<String>| {
            *scope = Some(match scope.take() {
                None => r,
                Some(s) => s.intersection(&r).cloned().collect(),
            });
        };
        for id in &ids {
            let sk = g.index.skills.get(id).ok_or_else(|| serr("skill_not_found"))?;
            if !g.enabled.contains(id) {
                return Err(serr("skill_disabled"));
            }
            if g.contexts.get(id).map(String::as_str).unwrap_or("personal") != context {
                return Err(serr("skill_out_of_scope"));
            }
            let body = sk.load()?;
            let texto = cortar(&body.text, restante).to_string();
            restante -= texto.len();
            data.push(ExternalData { connection: sk.namespace.clone(), kind: "skill".into(), key: id.clone(), truncated: body.text.len() > texto.len(), text: texto, digest: body.digest.clone() });
            let mut prov = sk.summary();
            prov["digest"] = json!(body.digest);
            provenance.push(prov);
            loaded.insert(id.clone(), body);
            if let Some(r) = g.scopes.get(id) {
                restringe(&mut scope, r.clone());
            }
            if let Some(t) = &sk.header.allowed_tools {
                restringe(&mut scope, t.split_whitespace().map(String::from).collect());
            }
        }
        for r in references {
            let body = loaded.get(&r.skill).ok_or_else(|| serr("skill_reference_not_selected"))?;
            if !body.references.contains(&r.path) {
                return Err(serr("skill_reference_not_declared"));
            }
            let sk = &g.index.skills[&r.skill];
            let bruto = sk.reference(&r.path, 16000)?;
            let texto = cortar(&bruto, restante).to_string();
            restante -= texto.len();
            data.push(ExternalData { connection: sk.namespace.clone(), kind: "skill_reference".into(), key: r.path.clone(), truncated: bruto.len() > texto.len(), text: texto, digest: hex::encode(Sha256::digest(bruto.as_bytes())) });
        }
        let esperado: BTreeMap<String, String> = ids.iter().filter_map(|id| g.authorities.get(id).map(|a| (id.clone(), a.clone()))).collect();
        let mut autoridades: Vec<String> = esperado.values().cloned().collect::<HashSet<_>>().into_iter().collect();
        autoridades.sort();
        let authorized: Option<Arc<dyn Fn() -> bool + Send + Sync>> = if esperado.is_empty() {
            None
        } else {
            let me = self.clone();
            Some(Arc::new(move || {
                let g = me.inner.read();
                esperado.iter().all(|(id, rev)| g.enabled.contains(id) && g.authorities.get(id) == Some(rev))
            }))
        };
        Ok(Selection { data, allowed_tools: scope, skills: provenance, authorities: autoridades, authorized })
    }
}

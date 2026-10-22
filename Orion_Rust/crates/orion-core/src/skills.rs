//! Skills no formato aberto Agent Skills (`<pasta>/<nome>/SKILL.md`), com carregamento gradual.
//!
//! Só o nome e a descrição entram no prompt de todo turno; o corpo vem sob demanda pela
//! ferramenta `carregar_skill`. Uma skill é TEXTO que orienta o modelo: nada aqui executa script,
//! e a pasta `scripts/` é ignorada (REGRAS.md: skill é texto, não código).

use crate::tools::registry::{obj, Tool};
use orion_policy::{Risk, ToolSpec};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

static NOME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]{0,63}$").unwrap());
static FRONT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)\A---[ \t]*\n(.*?)\n---[ \t]*(?:\n|\z)").unwrap());
pub const MAX_DESCRICAO: usize = 1024;
pub const MAX_CORPO: usize = 20_000;
pub const MAX_NO_PROMPT: usize = 30;

pub fn tool_spec() -> ToolSpec {
    ToolSpec::new("carregar_skill", Risk::Read)
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub nome: String,
    pub descricao: String,
    pub caminho: PathBuf,
    pub avisos: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Rejeitada {
    pub pasta: String,
    pub motivo: String,
}

fn valor(bruto: &str) -> String {
    let v = bruto.trim();
    let b = v.as_bytes();
    if v.len() >= 2 && b[0] == b[v.len() - 1] && (b[0] == b'"' || b[0] == b'\'') {
        return v[1..v.len() - 1].to_string();
    }
    v.to_string()
}

/// YAML mínimo (`chave: valor`, uma linha): basta para nome e descrição.
pub fn frontmatter(texto: &str) -> (BTreeMap<String, String>, String) {
    let Some(m) = FRONT.captures(texto) else { return (BTreeMap::new(), texto.to_string()) };
    let mut campos = BTreeMap::new();
    for linha in m[1].lines() {
        if linha.contains(':') && !linha.starts_with([' ', '\t', '#']) {
            let (k, v) = linha.split_once(':').unwrap();
            campos.insert(k.trim().to_lowercase(), valor(v));
        }
    }
    (campos, texto[m.get(0).unwrap().end()..].to_string())
}

pub struct SkillCatalog {
    pub raiz: PathBuf,
    pub extras: Vec<PathBuf>,
    pub skills: indexmap_lite::IndexMap<Skill>,
    pub rejeitadas: Vec<Rejeitada>,
}

/// Mapa que preserva a ordem de inserção (como o `dict` do Python).
pub mod indexmap_lite {
    #[derive(Debug, Clone)]
    pub struct IndexMap<V> {
        pub itens: Vec<(String, V)>,
    }
    impl<V> Default for IndexMap<V> {
        fn default() -> Self {
            IndexMap { itens: vec![] }
        }
    }
    impl<V> IndexMap<V> {
        pub fn get(&self, k: &str) -> Option<&V> {
            self.itens.iter().find(|(c, _)| c == k).map(|(_, v)| v)
        }
        pub fn contains_key(&self, k: &str) -> bool {
            self.get(k).is_some()
        }
        pub fn insert(&mut self, k: String, v: V) {
            match self.itens.iter_mut().find(|(c, _)| *c == k) {
                Some(e) => e.1 = v,
                None => self.itens.push((k, v)),
            }
        }
        pub fn values(&self) -> impl Iterator<Item = &V> {
            self.itens.iter().map(|(_, v)| v)
        }
        pub fn keys(&self) -> impl Iterator<Item = &String> {
            self.itens.iter().map(|(k, _)| k)
        }
        pub fn len(&self) -> usize {
            self.itens.len()
        }
        pub fn is_empty(&self) -> bool {
            self.itens.is_empty()
        }
    }
}

impl SkillCatalog {
    pub fn new(raiz: &Path, extras: Vec<PathBuf>) -> Self {
        let mut c = SkillCatalog { raiz: raiz.into(), extras, skills: Default::default(), rejeitadas: vec![] };
        c.scan();
        c
    }

    pub fn scan(&mut self) {
        self.skills = Default::default();
        self.rejeitadas = vec![];
        let raizes: Vec<PathBuf> = std::iter::once(self.raiz.clone()).chain(self.extras.clone()).collect();
        for raiz in raizes {
            if !raiz.is_dir() {
                continue;
            }
            let base = orion_policy::paths::realpath(&raiz);
            let mut pastas: Vec<PathBuf> = std::fs::read_dir(&raiz).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
            pastas.sort();
            for pasta in pastas {
                let nome = pasta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if !pasta.is_dir() || nome.starts_with('.') {
                    continue;
                }
                if let Err(e) = self.ler(&pasta, &nome, &base) {
                    self.rejeitadas.push(Rejeitada { pasta: nome, motivo: format!("ilegível: {}", e.kind()) });
                }
            }
        }
    }

    fn rejeitar(&mut self, pasta: &str, motivo: impl Into<String>) {
        self.rejeitadas.push(Rejeitada { pasta: pasta.into(), motivo: motivo.into() });
    }

    fn ler(&mut self, pasta: &Path, nome_pasta: &str, base: &Path) -> std::io::Result<()> {
        let arquivo = pasta.join("SKILL.md");
        let link = |p: &Path| std::fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(false);
        if link(pasta) || link(&arquivo) || !arquivo.is_file() {
            self.rejeitar(nome_pasta, "sem SKILL.md regular (link não vale)");
            return Ok(());
        }
        if !orion_policy::paths::realpath(&arquivo).starts_with(base) {
            self.rejeitar(nome_pasta, "SKILL.md fora da pasta de skills");
            return Ok(());
        }
        if std::fs::metadata(&arquivo)?.len() > (MAX_CORPO * 4) as u64 {
            self.rejeitar(nome_pasta, "SKILL.md grande demais");
            return Ok(());
        }
        let texto = String::from_utf8_lossy(&std::fs::read(&arquivo)?).into_owned();
        let (campos, corpo) = frontmatter(&texto);
        let nome = campos.get("name").cloned().unwrap_or_default();
        let desc = campos.get("description").cloned().unwrap_or_default();
        if !NOME.is_match(&nome) {
            self.rejeitar(nome_pasta, "name inválido (a-z, 0-9 e hífen)");
        } else if nome != nome_pasta {
            self.rejeitar(nome_pasta, format!("name '{nome}' difere da pasta"));
        } else if desc.is_empty() || desc.chars().count() > MAX_DESCRICAO {
            self.rejeitar(nome_pasta, "description vazia ou longa demais");
        } else if corpo.chars().count() > MAX_CORPO {
            self.rejeitar(nome_pasta, format!("corpo acima de {MAX_CORPO} caracteres"));
        } else if self.skills.contains_key(&nome) {
            self.rejeitar(nome_pasta, "nome já usado por outra skill");
        } else {
            let avisos = if pasta.join("scripts").exists() { vec!["scripts/ ignorada: skill não executa código".to_string()] } else { vec![] };
            self.skills.insert(nome.clone(), Skill { nome, descricao: desc, caminho: arquivo, avisos });
        }
        Ok(())
    }

    /// Só nome e descrição, para todo turno. Vazio se não há skill válida.
    pub fn prompt_block(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }
        let linhas: Vec<String> = self.skills.values().take(MAX_NO_PROMPT).map(|s| format!("- {}: {}", s.nome, s.descricao)).collect();
        format!(
            "[SKILLS: instruções do Antônio, uma por assunto. Quando o pedido combinar com a descrição, chame `carregar_skill` com o nome antes de agir. Uma skill não dá permissão extra: as aprovações valem do mesmo jeito.]\n{}",
            linhas.join("\n")
        )
    }

    pub fn load(&self, nome: &str) -> Value {
        let Some(s) = self.skills.get(&nome.trim().to_lowercase()) else {
            let mut disp: Vec<&String> = self.skills.keys().collect();
            disp.sort();
            return json!({"erro": format!("skill '{nome}' não existe"), "disponiveis": disp});
        };
        let texto = std::fs::read(&s.caminho).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let (_, corpo) = frontmatter(&texto);
        let refs = s.caminho.parent().map(|p| p.join("references")).unwrap_or_default();
        let mut nomes: Vec<String> = if refs.is_dir() {
            std::fs::read_dir(&refs)
                .map(|d| d.flatten().filter(|e| e.path().is_file()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default()
        } else {
            vec![]
        };
        nomes.sort();
        let mut out = Map::new();
        out.insert("nome".into(), json!(s.nome));
        out.insert("instrucoes".into(), json!(corpo.trim().chars().take(MAX_CORPO).collect::<String>()));
        if !nomes.is_empty() {
            nomes.truncate(50);
            out.insert("referencias_na_pasta".into(), json!(nomes));
        }
        if !s.avisos.is_empty() {
            out.insert("avisos".into(), json!(s.avisos));
        }
        Value::Object(out)
    }
}

pub fn skill_tool(catalogo: Arc<parking_lot::RwLock<SkillCatalog>>) -> Tool {
    Tool::sync(
        "carregar_skill",
        "Carrega as instruções completas de uma skill do Antônio pelo nome.",
        obj(json!({"nome": {"type": "string"}}), &["nome"]),
        move |a| Ok(catalogo.read().load(&a.s("nome")?)),
    )
}

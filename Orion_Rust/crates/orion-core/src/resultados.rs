//! Biblioteca de resultados (C34): o que o Orion gerou fica numa pasta própria, com versões.
//!
//! O agente chama `Library::registrar` depois de `gerar_documento` ou `gerar_imagem`; a cópia vai
//! para `<dados>/resultados/` com nome gerado aqui. A API só serve arquivos que moram nessa
//! pasta: nenhum caminho vindo do modelo é lido de volta.

use orion_store::Store;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

pub const MAX_COPIA: u64 = 25 * 1024 * 1024;
pub const PREVIA_TEXTO: [&str; 4] = [".txt", ".md", ".csv", ".json"];
pub const PREVIA_PDF: &str = ".pdf";
pub const MAX_DIFF_LINHAS: usize = 2000;
static NOME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^A-Za-z0-9._ -]+").unwrap());

pub fn previa_imagem(ext: &str) -> Option<&'static str> {
    match ext {
        ".png" => Some("image/png"),
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".webp" => Some("image/webp"),
        _ => None,
    }
}

pub fn extensao(p: &Path) -> String {
    p.extension().map(|e| format!(".{}", e.to_string_lossy().to_lowercase())).unwrap_or_default()
}

pub fn nome_seguro(nome: &str) -> String {
    let base = Path::new(nome).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let limpo = NOME.replace_all(&base, "_").trim_matches([' ', '.']).to_string();
    let limpo = if limpo.is_empty() { "resultado".to_string() } else { limpo };
    limpo.chars().take(80).collect()
}

pub struct Library {
    pub memory: Arc<Store>,
    pub pasta: PathBuf,
}

impl Library {
    pub fn new(memory: Arc<Store>, pasta: PathBuf) -> Self {
        Library { memory, pasta }
    }

    fn imagens(&self) -> PathBuf {
        self.pasta.parent().map(|p| p.join("imagens")).unwrap_or_default()
    }

    /// Lê o JSON devolvido pela ferramenta e, se ela gerou um arquivo, guarda a cópia.
    pub fn registrar(&self, session_id: Option<&str>, tool: &str, resultado: &str) -> Option<Map<String, Value>> {
        let dados: Value = serde_json::from_str(resultado).ok()?;
        if !dados.get("ok").is_some_and(|v| v.as_bool() == Some(true)) {
            return None;
        }
        let (origem, kind) = match tool {
            "gerar_documento" => (PathBuf::from(dados.get("path")?.as_str()?), "documento"),
            "gerar_imagem" => {
                let n = Path::new(dados.get("arquivo")?.as_str()?).file_name()?.to_owned();
                (self.imagens().join(n), "imagem")
            }
            _ => return None,
        };
        self.registrar_arquivo(session_id, tool, &origem, kind)
    }

    /// Guarda a cópia de um arquivo já pronto na biblioteca.
    pub fn registrar_arquivo(&self, session_id: Option<&str>, tool: &str, origem: &Path, kind: &str) -> Option<Map<String, Value>> {
        let meta = std::fs::metadata(origem).ok()?;
        if !meta.is_file() || meta.len() > MAX_COPIA {
            return None;
        }
        let nome = nome_seguro(&origem.to_string_lossy());
        let r = (|| -> anyhow::Result<Map<String, Value>> {
            std::fs::create_dir_all(&self.pasta)?;
            let prov = self.memory.add_artifact(session_id, kind, &nome, "", meta.len() as i64, tool)?;
            let id = prov["id"].as_i64().unwrap_or(0);
            let guardado = format!("{id}-{nome}");
            std::fs::copy(origem, self.pasta.join(&guardado))?;
            self.memory.tx(|c| {
                c.execute("UPDATE library_results SET stored=? WHERE id=?", orion_store::rusqlite::params![guardado, id])?;
                Ok(())
            })?;
            Ok(self.memory.get_artifact(id)?.unwrap_or_default())
        })();
        match r {
            Ok(m) => Some(m),
            Err(e) => {
                tracing::warn!("não consegui guardar o resultado na biblioteca: {e}");
                None
            }
        }
    }

    /// Arquivo da biblioteca, só se ainda existir e estiver dentro da pasta dela.
    pub fn caminho(&self, aid: i64) -> Option<PathBuf> {
        let a = self.memory.get_artifact(aid).ok()??;
        let guardado = a.get("stored")?.as_str()?.to_string();
        if guardado.is_empty() {
            return None;
        }
        let alvo = orion_policy::paths::realpath(&self.pasta.join(guardado));
        (alvo.starts_with(orion_policy::paths::realpath(&self.pasta)) && alvo.is_file()).then_some(alvo)
    }

    pub fn apagar(&self, aid: i64) -> bool {
        let Ok(Some(guardado)) = self.memory.delete_artifact(aid) else { return false };
        let alvo = orion_policy::paths::realpath(&self.pasta.join(&guardado));
        if !guardado.is_empty() && alvo.starts_with(orion_policy::paths::realpath(&self.pasta)) {
            let _ = std::fs::remove_file(alvo);
        }
        true
    }

    /// Diff de linhas entre duas versões de texto: `com` é a base, `aid` o novo.
    pub fn diff(&self, aid: i64, com: i64) -> Result<Option<Value>, String> {
        let (Some(novo), Some(base)) = (self.caminho(aid), self.caminho(com)) else { return Ok(None) };
        if !PREVIA_TEXTO.contains(&extensao(&novo).as_str()) || !PREVIA_TEXTO.contains(&extensao(&base).as_str()) {
            return Err("só texto, Markdown e CSV têm diff (imagem: compare lado a lado)".into());
        }
        let ler = |p: &Path| String::from_utf8_lossy(&std::fs::read(p).unwrap_or_default()).into_owned();
        let (a, b) = (ler(&base), ler(&novo));
        let d = similar::TextDiff::from_lines(&a, &b);
        let texto = d.unified_diff().context_radius(3).header("base", "novo").to_string();
        let linhas: Vec<String> = texto.lines().map(String::from).collect();
        Ok(Some(json!({
            "de": com,
            "para": aid,
            "iguais": linhas.is_empty(),
            "adicionadas": linhas.iter().filter(|x| x.starts_with('+') && !x.starts_with("+++")).count(),
            "removidas": linhas.iter().filter(|x| x.starts_with('-') && !x.starts_with("---")).count(),
            "linhas": linhas.iter().take(MAX_DIFF_LINHAS).collect::<Vec<_>>(),
            "truncado": linhas.len() > MAX_DIFF_LINHAS,
        })))
    }

    /// Apaga as versões além das `manter` mais novas de cada resultado (E6.3).
    pub fn limpar(&self, manter: usize, nome: Option<&str>, simular: bool) -> Value {
        let sobra = self.memory.surplus_artifacts(manter, nome).unwrap_or_default();
        let bytes: i64 = sobra.iter().map(|x| x["bytes"].as_i64().unwrap_or(0)).sum();
        if simular {
            return json!({"total": sobra.len(), "bytes": bytes, "apagados": 0});
        }
        let apagados = sobra.iter().filter(|x| self.apagar(x["id"].as_i64().unwrap_or(0))).count();
        json!({"total": sobra.len(), "bytes": bytes, "apagados": apagados})
    }
}

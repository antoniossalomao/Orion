//! Verificação da fase 0 (antes de vender o PC): o export abre, as contagens batem e a memória nova
//! consegue guardá-lo, fazer backup, restaurar e responder.
//!
//! `verify_export` nunca toca no banco real: importa para um arquivo temporário. Também confere,
//! só por existência e tamanho, o que precisa ser copiado do PC (`.env`, `google_auth/`, vault) —
//! nunca lê nem imprime valores de segredo (só os NOMES das chaves).

use crate::importer::*;
use orion_store::search::Kind;
use orion_store::Store;
use regex::Regex;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;

pub const OBRIGATORIAS: [&str; 2] = ["evento", "sessao"];
static CHAVE_ENV: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*\S").unwrap());

#[derive(Debug, Default)]
pub struct MigrationReport {
    /// Arquivo do export → registros.
    pub tabelas: BTreeMap<String, usize>,
    pub ausentes: Vec<String>,
    pub erros: Vec<String>,
    pub avisos: Vec<String>,
    pub importacao: Option<ImportReport>,
    pub ensaio: Vec<(String, i64)>,
    pub arquivos: Vec<(String, String)>,
}

impl MigrationReport {
    pub fn ok(&self) -> bool {
        self.erros.is_empty()
    }

    pub fn linhas(&self) -> Vec<String> {
        let mut out = vec!["EXPORT".to_string()];
        if self.tabelas.is_empty() {
            out.push("  (vazio)".into());
        }
        out.extend(self.tabelas.iter().map(|(t, n)| format!("  {t}: {n} registros")));
        out.extend(self.ausentes.iter().map(|t| format!("  ausente (ok se o legado nunca usou): {t}")));
        if let Some(i) = &self.importacao {
            out.push("IMPORTAÇÃO DE ENSAIO (banco temporário)".into());
            out.push(format!("  {}", i.resumo()));
        }
        if !self.ensaio.is_empty() {
            out.push("BACKUP E RESTAURAÇÃO".into());
            out.extend(self.ensaio.iter().filter(|(_, n)| *n > 0).map(|(t, n)| format!("  {t}: {n}")));
        }
        if !self.arquivos.is_empty() {
            out.push("ARQUIVOS A COPIAR DO PC".into());
            out.extend(self.arquivos.iter().map(|(a, s)| format!("  {a}: {s}")));
        }
        out.extend(self.avisos.iter().map(|a| format!("AVISO: {a}")));
        out.extend(self.erros.iter().map(|e| format!("ERRO: {e}")));
        out.push(format!("RESULTADO: {}", if self.ok() { "PRONTO" } else { "NÃO está pronto" }));
        out
    }
}

type Dados = BTreeMap<String, Vec<serde_json::Map<String, Value>>>;

fn contar(pasta: &Path, rel: &mut MigrationReport) -> Dados {
    let mut dados = Dados::new();
    let mut com_problema: Vec<String> = vec![];
    let mut arqs: Vec<_> = std::fs::read_dir(pasta).map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect()).unwrap_or_default();
    arqs.sort();
    for arq in arqs {
        let stem = arq.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let nome = arq.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let conteudo: Result<Value, String> = std::fs::read_to_string(&arq).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string()));
        let conteudo = match conteudo {
            Ok(c) => c,
            Err(e) => {
                rel.erros.push(format!("{nome} ilegível: {e}"));
                com_problema.push(stem);
                continue;
            }
        };
        let Some(lista) = conteudo.as_array() else {
            rel.erros.push(format!("{nome}: esperava uma lista de registros"));
            com_problema.push(stem);
            continue;
        };
        let regs: Vec<_> = lista.iter().filter_map(|d| d.as_object().cloned()).collect();
        rel.tabelas.insert(stem.clone(), lista.len());
        if regs.len() != lista.len() {
            rel.erros.push(format!("{nome}: há itens que não são registros"));
        }
        dados.insert(stem, regs);
    }
    for t in OBRIGATORIAS.iter().chain(OPERACIONAIS.iter()).chain(RELACOES.iter()) {
        if rel.tabelas.contains_key(*t) || com_problema.iter().any(|c| c == t) {
            continue;
        }
        if OBRIGATORIAS.contains(t) {
            rel.erros.push(format!("{t}.json não existe (rode backup_memoria no legado)"));
        } else {
            rel.ausentes.push((*t).into());
        }
    }
    if rel.tabelas.get("evento") == Some(&0) {
        rel.erros.push("evento.json está vazio: o export não tem conversas".into());
    }
    dados
}

/// Todo registro do export tem de estar contado: novo, repetido ou inválido (e dito qual).
fn conferir_contas(imp: &ImportReport, dados: &Dados, rel: &mut MigrationReport) {
    let inv = |t: &str| imp.invalidas_por_tabela.get(t).copied().unwrap_or(0);
    let mut erros: Vec<String> = vec![];
    let mut conta = |tabela: &str, novos: usize, repetidos: usize| {
        let total = dados.get(tabela).map(Vec::len).unwrap_or(0);
        let contados = novos + repetidos + inv(tabela);
        if total != contados {
            erros.push(format!("{tabela}: {total} no export, {contados} contados na importação (novos {novos}, repetidos {repetidos}, inválidos {})", inv(tabela)));
        }
    };
    // sessões criadas no ensaio incluem as "órfãs" que os eventos pedem: só o piso é conferível
    let validas = dados.get("sessao").map(Vec::len).unwrap_or(0).saturating_sub(inv("sessao"));
    conta("evento", imp.mensagens, imp.ja_importadas);
    for t in OPERACIONAIS {
        conta(t, imp.operacionais.get(t).copied().unwrap_or(0), imp.operacionais_repetidos.get(t).copied().unwrap_or(0));
    }
    if imp.sessoes < validas {
        rel.erros.push(format!("sessao: {validas} válidas no export, só {} importadas", imp.sessoes));
    }
    rel.erros.extend(erros);
    for (t, n) in &imp.invalidas_por_tabela {
        rel.avisos.push(format!("{n} registro(s) inválido(s) em {t}.json (ver o log; não são importados)"));
    }
    if !imp.atores_como_system.is_empty() {
        let quem = imp.atores_como_system.iter().map(|(a, n)| format!("{a} ({n})")).collect::<Vec<_>>().join(", ");
        rel.avisos.push(format!("atores que viram 'system': {quem}. Se algum é o nome antigo do assistente, passe --assistente <nome> (senão essas falas não aparecem como respostas na busca)"));
    }
}

fn ensaio(pasta: &Path, assistentes: &[String], rel: &mut MigrationReport, dados: &Dados) {
    let tmp = std::env::temp_dir().join(format!("orion-verify-{}", orion_store::uuid_hex()));
    let r = (|| -> Result<(), String> {
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let store = Store::open(tmp.join("ensaio.db")).map_err(|e| e.to_string())?;
        let imp = import_surreal_export(&store, pasta, assistentes)?;
        conferir_contas(&imp, dados, rel);
        rel.importacao = Some(imp);
        let esperadas = store.query("SELECT COUNT(*) AS n FROM messages", &[]).map_err(|e| e.to_string())?[0]["n"].as_i64().unwrap_or(0);
        let amostra = store.query("SELECT text FROM messages WHERE role IN ('user','assistant') ORDER BY length(text) DESC LIMIT 1", &[]).map_err(|e| e.to_string())?;
        let backup = store.backup_to(&tmp.join("bk").join("orion.db")).map_err(|e| e.to_string())?;
        drop(store);
        rel.ensaio = Store::verify_backup(&backup).map_err(|e| e.to_string())?;
        let restaurado = Store::restore(&backup, &tmp.join("novo").join("orion.db")).map_err(|e| e.to_string())?;
        if restaurado.query("SELECT COUNT(*) AS n FROM messages", &[]).map_err(|e| e.to_string())?[0]["n"].as_i64().unwrap_or(0) != esperadas {
            rel.erros.push("a restauração do backup perdeu mensagens".into());
        }
        if let Some(a) = amostra.first() {
            let trecho = a["text"].as_str().unwrap_or("").split_whitespace().take(12).collect::<Vec<_>>().join(" ");
            if restaurado.search(&trecho, 3, &[Kind::Message]).map_err(|e| e.to_string())?.is_empty() {
                rel.erros.push("a busca não achou, no banco restaurado, uma fala importada".into());
            }
        }
        Ok(())
    })();
    if let Err(e) = r {
        rel.erros.push(format!("ensaio falhou: {e}"));
    }
    let _ = std::fs::remove_dir_all(&tmp);
}

fn conferir_arquivos(rel: &mut MigrationReport, env_file: Option<&Path>, google_auth: Option<&Path>, vault: Option<&Path>) {
    if let Some(env) = env_file {
        if std::fs::metadata(env).map(|m| !m.is_file() || m.len() == 0).unwrap_or(true) {
            rel.erros.push(format!(".env não encontrado ou vazio: {}", env.display()));
        } else {
            let mut chaves: Vec<String> = std::fs::read_to_string(env).unwrap_or_default().lines().filter(|l| !l.trim_start().starts_with('#')).filter_map(|l| CHAVE_ENV.captures(l).map(|c| c[1].to_string())).collect();
            chaves.sort();
            chaves.dedup();
            rel.arquivos.push((".env".into(), format!("{} variáveis preenchidas ({})", chaves.len(), chaves.join(", "))));
        }
    }
    if let Some(g) = google_auth {
        let mut arqs: Vec<String> = std::fs::read_dir(g).map(|rd| rd.flatten().filter(|e| e.metadata().is_ok_and(|m| m.is_file() && m.len() > 0)).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
        arqs.sort();
        if arqs.is_empty() {
            rel.erros.push(format!("google_auth sem arquivos (ou vazios): {}", g.display()));
        } else {
            rel.arquivos.push(("google_auth/".into(), format!("{} arquivo(s): {}", arqs.len(), arqs.join(", "))));
        }
    }
    if let Some(v) = vault {
        fn contar_md(d: &Path, n: &mut usize) {
            if let Ok(rd) = std::fs::read_dir(d) {
                for e in rd.flatten() {
                    let nome = e.file_name().to_string_lossy().into_owned();
                    if nome.starts_with('.') {
                        continue;
                    }
                    let p = e.path();
                    if p.is_dir() {
                        contar_md(&p, n);
                    } else if p.extension().is_some_and(|x| x == "md") {
                        *n += 1;
                    }
                }
            }
        }
        let mut n = 0;
        if v.is_dir() {
            contar_md(v, &mut n);
        }
        if n == 0 {
            rel.erros.push(format!("vault sem notas .md: {}", v.display()));
        } else {
            rel.arquivos.push(("vault".into(), format!("{n} notas .md")));
        }
    }
}

pub fn verify_export(pasta: &Path, assistentes: &[String], env_file: Option<&Path>, google_auth: Option<&Path>, vault: Option<&Path>, com_ensaio: bool) -> MigrationReport {
    let mut rel = MigrationReport::default();
    if !pasta.is_dir() {
        rel.erros.push(format!("pasta do export não existe: {}", pasta.display()));
        return rel;
    }
    let dados = contar(pasta, &mut rel);
    if com_ensaio && rel.ok() {
        ensaio(pasta, assistentes, &mut rel, &dados);
    }
    conferir_arquivos(&mut rel, env_file, google_auth, vault);
    rel
}

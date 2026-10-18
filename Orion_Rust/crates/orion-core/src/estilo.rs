//! Seu estilo de código como skill (E7.8).
//!
//! `orion estilo <pasta-repo> [...]` pede à CLI da sua assinatura (Claude Code, pelo `Delegator`)
//! que leia cada repositório e descreva o jeito de o Antônio escrever. O resultado vira a skill
//! `estilo-antonio`, mas NASCE DESATIVADA: fica em `<skills>/.pendentes/estilo-antonio/SKILL.md` (o
//! catálogo ignora pastas com ponto) até você ler e rodar `orion estilo --ativar`. O conteúdo é texto
//! escrito por um modelo a partir de código: pode errar, por isso a revisão é sua e a skill continua
//! sendo só texto (REGRAS.md: skill é texto, não código).

use crate::skills::MAX_CORPO;
use orion_ai::delegate::Delegator;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub const SKILL: &str = "estilo-antonio";
const DESCRICAO: &str = "O estilo de código do Antônio (nomes, comentários, testes, commits). Use ao escrever ou revisar código para ele.";
const PROMPT: &str = "Leia este repositório (código, testes e `git log`) e descreva, em português, o ESTILO de quem o escreve, para que outra pessoa o imite. Use exatamente estas seções em Markdown: ## Nomes, ## Comentários, ## Testes, ## Commits, ## Estrutura. Seja concreto: padrões que se repetem, com 1 ou 2 exemplos curtos cada. Até 3000 caracteres. Não copie trechos longos de código, não inclua segredos, chaves, e-mails nem caminhos pessoais. Responda só com o Markdown, sem introdução.";
static SECRETO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(api[_-]?key|secret|token|password|senha)\s*[:=]\s*\S{8,}|-----BEGIN [A-Z ]*PRIVATE KEY").unwrap());

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct EstiloError(pub String);

type R<T> = Result<T, EstiloError>;

pub fn pendente(skills_dir: &Path) -> PathBuf {
    skills_dir.join(".pendentes").join(SKILL).join("SKILL.md")
}

/// O que a CLI devolveu, sem cercas de código externas e sem linha com cara de segredo.
fn limpar(texto: &str) -> String {
    let cerca = Regex::new(r"\A```(?:markdown|md)?\s*\n|\n```\s*\z").unwrap();
    let t = cerca.replace_all(texto.trim(), "");
    t.lines().filter(|l| !SECRETO.is_match(l)).collect::<Vec<_>>().join("\n").trim().to_string()
}

pub fn montar(secoes: &[(String, String)]) -> String {
    let teto = 500.max((MAX_CORPO - 800) / secoes.len().max(1));
    let corpo = secoes.iter().map(|(n, t)| format!("# Estilo em `{n}`\n\n{}", t.chars().take(teto).collect::<String>())).collect::<Vec<_>>().join("\n\n");
    format!("---\nname: {SKILL}\ndescription: {DESCRICAO}\n---\n\n> Gerada por `orion estilo` (um modelo leu o código); revisada por você antes de valer.  Não muda permissões: a política vale como sempre.\n\n{corpo}\n")
}

/// Delega a leitura de cada repositório e grava a skill PENDENTE. Devolve o arquivo.
pub fn gerar(repos: &[PathBuf], delegador: &Delegator, skills_dir: &Path, agente: Option<&str>) -> R<PathBuf> {
    if repos.is_empty() {
        return Err(EstiloError("informe ao menos uma pasta de repositório".into()));
    }
    let mut secoes = vec![];
    for repo in repos {
        let raiz = repo.clone();
        if !raiz.is_dir() {
            return Err(EstiloError(format!("pasta inexistente: {}", repo.display())));
        }
        let nome = raiz.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let r = delegador.delegate(PROMPT, &raiz.to_string_lossy(), agente);
        if r["ok"] != serde_json::json!(true) {
            return Err(EstiloError(format!("{nome}: {}", r["erro"].as_str().unwrap_or("a CLI não respondeu"))));
        }
        let texto = limpar(r["saida"].as_str().unwrap_or(""));
        if texto.chars().count() < 80 {
            return Err(EstiloError(format!("{nome}: resposta curta demais para virar skill")));
        }
        secoes.push((nome, texto));
    }
    let alvo = pendente(skills_dir);
    if let Some(p) = alvo.parent() {
        std::fs::create_dir_all(p).map_err(|e| EstiloError(e.to_string()))?;
    }
    std::fs::write(&alvo, montar(&secoes)).map_err(|e| EstiloError(e.to_string()))?;
    Ok(alvo)
}

/// Move a skill pendente para o catálogo. Recusa trocar uma skill que já está valendo.
pub fn ativar(skills_dir: &Path) -> R<PathBuf> {
    let origem = pendente(skills_dir).parent().map(Path::to_path_buf).unwrap_or_default();
    if !origem.join("SKILL.md").is_file() {
        return Err(EstiloError("não há skill pendente: rode `orion estilo <pasta-repo>` antes".into()));
    }
    let destino = skills_dir.join(SKILL);
    if destino.exists() {
        return Err(EstiloError(format!("{} já existe: apague-a (ou renomeie) para ativar a nova", destino.display())));
    }
    std::fs::rename(&origem, &destino).map_err(|e| EstiloError(e.to_string()))?;
    Ok(destino.join("SKILL.md"))
}

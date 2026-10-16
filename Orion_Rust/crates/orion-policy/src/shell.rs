//! Classificador de comandos de shell: leitura segura ou precisa de confirmação.
//!
//! Inverte a lógica da antiga blocklist: só roda sem confirmação o que for provadamente
//! leitura; todo o resto pede confirmação (REGRAS.md: shell só roda sem confirmação o que é
//! leitura provada). Erros caem sempre para o lado seguro. Cobre PowerShell e shell POSIX.

use regex::Regex;
use std::sync::LazyLock;

// Qualquer um destes impede a prova de "só leitura": redirecionamento, substituição de
// comando, subexpressão, bloco de script, chamada de método, acesso estático (::), escape e
// variáveis.
static PROIBIDOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[<>`$(){}\[\]]|::").unwrap());
static SEPARADORES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|\||&&|[;|&\n\r]").unwrap());
// Leitura que vaza segredo para o modelo.
static SEGREDOS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\.env\b|credentials|token\.json|id_rsa|id_ed25519|\.ssh|\.aws|\.gnupg|\bsecrets?\b|keychain|\benv:",
    )
    .unwrap()
});
static GIT_CURTA_OK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-[A-Za-z0-9]+$").unwrap());

const POSIX_LEITURA: &[&str] = &[
    "ls", "pwd", "cat", "head", "tail", "wc", "whoami", "hostname", "uname", "uptime", "df", "du",
    "free", "ps", "which", "echo", "id", "stat", "file", "tree", "grep", "ver", "cal",
];
const PS_ALIASES_LEITURA: &[&str] = &[
    "gci", "gc", "gl", "gps", "gsv", "gm", "gcm", "gi", "gp", "gu", "dir", "type", "ls", "pwd",
    "cat", "echo", "where", "select", "measure", "ft", "fl", "fw", "write-output",
    "select-object", "where-object", "sort-object", "measure-object", "format-table",
    "format-list", "format-wide", "out-string", "convertto-json", "convertto-csv",
    "select-string", "test-path", "resolve-path", "split-path", "join-path",
];
const GET_NEGADOS: &[&str] = &["get-credential", "get-secret", "get-secretinfo"];
const GIT_LEITURA: &[&str] = &[
    "status", "log", "diff", "show", "rev-parse", "ls-files", "blame", "describe", "shortlog",
    "remote", "tag", "branch",
];
// git aceita abreviação única de opção longa (`--out=x` vale por `--output=x`), então não dá
// para listar as proibidas: só passam as opções longas conhecidas e inofensivas.
const GIT_LONG_OK: &[&str] = &[
    "--oneline", "--stat", "--shortstat", "--numstat", "--short", "--branch", "--name-only",
    "--name-status", "--graph", "--decorate", "--all", "--cached", "--staged", "--porcelain",
    "--abbrev-commit", "--show-current", "--list", "--verbose", "--merged", "--no-merged",
    "--summary", "--patch", "--no-color", "--color", "--first-parent", "--reverse", "--follow",
    "--no-merges", "--merges", "--tags", "--remotes", "--topo-order", "--date-order",
    "--untracked-files", "--ignored",
];
const GIT_LONG_EQ: &[&str] = &[
    "--pretty=", "--format=", "--since=", "--until=", "--after=", "--before=", "--author=",
    "--grep=", "--max-count=", "--date=", "--abbrev=", "--diff-filter=",
];
/// config, mudança de dir, apagar, renomear, arquivo de saída
const GIT_CURTA_NEGADA: &[char] = &['c', 'C', 'd', 'D', 'm', 'M', 'o'];
const VERSAO: &[&str] = &["--version", "-v", "-version", "-V"];
const RUNTIMES: &[&str] = &[
    "python", "python3", "pip", "pip3", "node", "npm", "uv", "git", "cargo", "rustc", "java",
    "javac", "dotnet", "go",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellVerdict {
    pub read_only: bool,
    pub reason: String,
}

impl ShellVerdict {
    fn nao(motivo: impl Into<String>) -> Self {
        ShellVerdict { read_only: false, reason: motivo.into() }
    }
}

fn git_flag_ok(a: &str) -> bool {
    if a == "--" {
        return true;
    }
    if a.starts_with("--") {
        return GIT_LONG_OK.contains(&a) || GIT_LONG_EQ.iter().any(|p| a.starts_with(p));
    }
    if a.starts_with('-') {
        return GIT_CURTA_OK.is_match(a) && !a[1..].chars().any(|c| GIT_CURTA_NEGADA.contains(&c));
    }
    true // revisão ou caminho
}

fn strip_aspas(s: &str) -> &str {
    s.trim_matches(['"', '\''])
}

fn first_token(segmento: &str) -> (String, Vec<String>) {
    let mut partes = segmento.split_whitespace();
    let Some(primeiro) = partes.next() else { return (String::new(), vec![]) };
    let mut cmd = strip_aspas(primeiro).to_lowercase();
    if let Some(sem) = cmd.strip_suffix(".exe") {
        cmd = sem.to_string();
    }
    (cmd, partes.map(|p| strip_aspas(p).to_string()).collect())
}

/// `None` se o segmento é leitura; senão o motivo.
fn segmento_leitura(cmd: &str, args: &[String]) -> Option<String> {
    if cmd.is_empty() {
        return Some("comando vazio".into());
    }
    if GET_NEGADOS.contains(&cmd) {
        return Some(format!("'{cmd}' expõe segredos"));
    }
    if cmd.starts_with("get-") || PS_ALIASES_LEITURA.contains(&cmd) || POSIX_LEITURA.contains(&cmd) {
        return None;
    }
    if cmd == "git" {
        let Some(sub) = args.first().filter(|a| !a.starts_with('-')) else {
            return Some("git sem subcomando de leitura explícito".into());
        };
        let sub = sub.to_lowercase();
        if !GIT_LEITURA.contains(&sub.as_str()) {
            return Some(format!("git {sub} pode alterar o repositório"));
        }
        let resto = &args[1..];
        if matches!(sub.as_str(), "branch" | "tag" | "remote") && resto.iter().any(|a| !a.starts_with('-')) {
            return Some(format!("git {sub} com argumento pode criar/alterar"));
        }
        for a in resto {
            if !git_flag_ok(a) {
                return Some(format!("git {sub} {a}: opção fora da lista de leitura segura"));
            }
        }
        return None;
    }
    if RUNTIMES.contains(&cmd) && args.len() == 1 && VERSAO.contains(&args[0].as_str()) {
        return None;
    }
    Some(format!("'{cmd}' fora da lista de leitura segura"))
}

/// Decide se `cmd` roda sem confirmação (leitura provada) ou não.
pub fn classify_command(cmd: &str) -> ShellVerdict {
    let texto = cmd.trim();
    if texto.is_empty() {
        return ShellVerdict::nao("comando vazio");
    }
    if SEGREDOS.is_match(texto) {
        return ShellVerdict::nao("toca em arquivo/variável de segredo (.env, credenciais, chaves)");
    }
    if PROIBIDOS.is_match(texto) {
        return ShellVerdict::nao(
            "contém redirecionamento, substituição, bloco de script, chamada de método \
             ou escape: não dá para provar que é só leitura",
        );
    }
    for segmento in SEPARADORES.split(texto) {
        if segmento.trim().is_empty() {
            continue;
        }
        let (nome, args) = first_token(segmento);
        if let Some(motivo) = segmento_leitura(&nome, &args) {
            return ShellVerdict::nao(motivo);
        }
    }
    ShellVerdict { read_only: true, reason: String::new() }
}

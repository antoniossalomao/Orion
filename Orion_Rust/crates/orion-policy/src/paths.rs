//! PathGuard: onde as ferramentas podem escrever sem confirmação.
//!
//! REGRAS.md: código, persona e política imutáveis para as ferramentas. Escrever dentro de
//! `protected_roots` (a raiz do projeto) sempre pede confirmação, o que também cobre a
//! auto-modificação. Caminhos são resolvidos (symlinks e `..`) antes de comparar.

use std::path::{Component, Path, PathBuf};

const EXT_SENSIVEIS: &[&str] = &[
    ".exe", ".dll", ".ps1", ".psm1", ".bat", ".cmd", ".msi", ".sys", ".reg", ".vbs", ".lnk",
    ".scr", ".app", ".command", ".plist", ".desktop",
];
const NOMES_SENSIVEIS: &[&str] = &[
    ".env", ".bashrc", ".zshrc", ".profile", ".bash_profile", "profile.ps1", "authorized_keys",
    "known_hosts", "credentials.json", "token.json",
    // do próprio Orion: o hash da senha e a lista de programas que o Orion sobe sozinho
    "auth.db", "auth.db-wal", "auth.db-shm", "mcp.json", ".mcp.json",
];
const COMPONENTES_SENSIVEIS: &[&str] =
    &[".ssh", ".aws", ".gnupg", "startup", "launchagents", "launchdaemons"];
// Leitura que vaza segredo para o modelo (REGRAS.md: ler segredo ou chave pede confirmação).
const NOMES_LEITURA_EXTRA: &[&str] = &[
    "id_rsa", "id_ed25519", "id_ecdsa", "id_dsa", ".netrc", ".npmrc", ".pgpass",
    ".git-credentials", ".pypirc", "secrets.json", "secret.json",
];
const EXT_LEITURA_SENSIVEL: &[&str] =
    &[".pem", ".key", ".pfx", ".p12", ".kdbx", ".keystore", ".jks", ".ppk"];
const COMPONENTES_LEITURA_SENSIVEL: &[&str] =
    &[".ssh", ".aws", ".gnupg", ".kube", ".docker", "google_auth"];
const MODELOS_DE_ENV: &[&str] = &[".example", ".sample", ".template"];

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn expanduser(p: &str) -> PathBuf {
    if p == "~" {
        return home();
    }
    if let Some(resto) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        return home().join(resto);
    }
    PathBuf::from(p)
}

/// `os.path.realpath`: absoluto, segue symlinks do prefixo que existe e resolve `..`,
/// mesmo quando o final do caminho ainda não existe.
pub fn realpath(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    realpath_saltos(&abs, 0)
}

fn realpath_saltos(p: &Path, saltos_iniciais: usize) -> PathBuf {
    let mut out = PathBuf::new();
    let comps: Vec<Component> = p.components().collect();
    for (i, c) in comps.iter().enumerate() {
        match c {
            Component::Prefix(_) | Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(n) => {
                out.push(n);
                if saltos_iniciais < 40 {
                    if let Ok(alvo) = std::fs::read_link(&out) {
                        out.pop();
                        let mut novo = if alvo.is_absolute() { alvo } else { out.join(alvo) };
                        for r in &comps[i + 1..] {
                            novo.push(r.as_os_str());
                        }
                        return realpath_saltos(&novo, saltos_iniciais + 1);
                    }
                }
            }
        }
    }
    out
}

/// `os.path.normcase(os.path.realpath(os.path.expanduser(p)))`.
pub fn norm(p: &str) -> String {
    let r = realpath(&expanduser(p));
    let s = r.to_string_lossy().into_owned();
    if cfg!(windows) {
        s.replace('/', "\\").to_lowercase()
    } else {
        s
    }
}

fn sep() -> char {
    std::path::MAIN_SEPARATOR
}

fn dentro(caminho: &str, raiz: &str) -> bool {
    if raiz.is_empty() {
        return false;
    }
    let raiz = format!("{}{}", raiz.trim_end_matches(['\\', '/']), sep());
    format!("{caminho}{}", sep()).starts_with(&raiz)
}

fn basename(p: &str) -> String {
    p.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase()
}

fn ext(nome: &str) -> String {
    // os.path.splitext: ponto inicial não conta como extensão (".env" não tem extensão)
    let nome = nome.trim_start_matches('.');
    match nome.rfind('.') {
        Some(i) => nome[i..].to_lowercase(),
        None => String::new(),
    }
}

fn partes(p: &str) -> Vec<String> {
    p.replace('\\', "/").split('/').filter(|x| !x.is_empty()).map(str::to_lowercase).collect()
}

pub fn system_roots() -> Vec<String> {
    if cfg!(windows) {
        let v = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_string());
        vec![
            norm(&v("SystemRoot", r"C:\Windows")),
            norm(&v("ProgramFiles", r"C:\Program Files")),
            norm(&v("ProgramFiles(x86)", r"C:\Program Files (x86)")),
            norm(&v("ProgramData", r"C:\ProgramData")),
        ]
    } else {
        ["/bin", "/sbin", "/usr", "/etc", "/System", "/Library", "/private/etc", "/boot", "/lib"]
            .iter()
            .map(|p| norm(p))
            .collect()
    }
}

pub fn default_safe_roots() -> Vec<PathBuf> {
    let h = home();
    ["Documents", "Downloads", "Desktop"].iter().map(|d| h.join(d)).collect()
}

#[derive(Debug, Clone)]
pub struct PathGuard {
    pub protected_roots: Vec<PathBuf>,
    pub safe_roots: Vec<PathBuf>,
    pub system_roots: Vec<String>,
}

impl Default for PathGuard {
    fn default() -> Self {
        PathGuard { protected_roots: vec![], safe_roots: default_safe_roots(), system_roots: system_roots() }
    }
}

impl PathGuard {
    pub fn new(protected_roots: Vec<PathBuf>) -> Self {
        PathGuard { protected_roots, ..Default::default() }
    }

    /// `None` se pode escrever sem confirmação; senão o motivo da confirmação.
    pub fn check_write(&self, path: &str) -> Option<String> {
        if path.trim().is_empty() {
            return Some("caminho vazio".into());
        }
        let alvo = norm(path);
        for raiz in &self.protected_roots {
            if dentro(&alvo, &norm(&raiz.to_string_lossy())) {
                return Some(
                    "escrita dentro do código/persona/política do Orion \
                     (núcleo imutável; auto-modificação exige aprovação)"
                        .into(),
                );
            }
        }
        if self.system_roots.iter().any(|r| dentro(&alvo, r)) {
            return Some("escrita em diretório de sistema".into());
        }
        let nome = basename(&alvo);
        let e = ext(&nome);
        if EXT_SENSIVEIS.contains(&e.as_str()) {
            return Some(format!("extensão sensível ({e})"));
        }
        let ps = partes(&alvo);
        if NOMES_SENSIVEIS.contains(&nome.as_str())
            || ps.iter().any(|p| COMPONENTES_SENSIVEIS.contains(&p.as_str()))
        {
            return Some("arquivo/pasta de segredo, configuração de shell ou autostart".into());
        }
        if !self.safe_roots.iter().any(|r| dentro(&alvo, &norm(&r.to_string_lossy()))) {
            return Some("fora das pastas de trabalho (Documents/Downloads/Desktop)".into());
        }
        None
    }

    /// `None` se pode ler sem confirmação; senão o motivo (arquivo ou pasta de segredo).
    ///
    /// O caminho é resolvido antes (symlink para `.env` continua sendo `.env`).
    pub fn check_read(&self, path: &str) -> Option<String> {
        if path.trim().is_empty() {
            return Some("caminho vazio".into());
        }
        let alvo = norm(path);
        let nome = basename(&alvo);
        let ps = partes(&alvo);
        let env = nome == ".env"
            || (nome.starts_with(".env.") && !MODELOS_DE_ENV.iter().any(|m| nome.ends_with(m)));
        let e = ext(&nome);
        if env
            || NOMES_SENSIVEIS.contains(&nome.as_str())
            || NOMES_LEITURA_EXTRA.contains(&nome.as_str())
            || EXT_LEITURA_SENSIVEL.contains(&e.as_str())
            || ps.iter().any(|p| COMPONENTES_LEITURA_SENSIVEL.contains(&p.as_str()))
        {
            return Some("arquivo/pasta de segredo ou chave: ler expõe o conteúdo ao modelo".into());
        }
        None
    }

    /// Reorganizar mexe em tudo dentro da pasta: raízes e sistema pedem confirmação.
    pub fn check_organize(&self, path: &str) -> Option<String> {
        if path.trim().is_empty() {
            return Some("caminho vazio".into());
        }
        let alvo = norm(path);
        if Path::new(&alvo).parent().is_none() {
            return Some("alvo é a raiz de um drive/sistema de arquivos".into());
        }
        let h = home();
        if alvo == norm(&h.to_string_lossy())
            || h.parent().is_some_and(|p| alvo == norm(&p.to_string_lossy()))
        {
            return Some("alvo é a pasta pessoal inteira".into());
        }
        if self.system_roots.iter().any(|r| dentro(&alvo, r)) {
            return Some("alvo é um diretório crítico do sistema".into());
        }
        if self.protected_roots.iter().any(|r| dentro(&alvo, &norm(&r.to_string_lossy()))) {
            return Some("alvo é o código do Orion".into());
        }
        None
    }
}

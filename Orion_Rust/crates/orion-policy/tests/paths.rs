//! Casos de `tests/policy/test_paths.py` (Python) portados.

use orion_policy::PathGuard;
use std::path::{Path, PathBuf};

struct Cenario {
    tmp: tempfile::TempDir,
    guard: PathGuard,
}

fn cenario() -> Cenario {
    let tmp = tempfile::tempdir().unwrap();
    let projeto = tmp.path().join("Orion");
    let docs = tmp.path().join("Documents");
    std::fs::create_dir_all(&projeto).unwrap();
    std::fs::create_dir_all(&docs).unwrap();
    let guard = PathGuard { protected_roots: vec![projeto], safe_roots: vec![docs], system_roots: vec![] };
    Cenario { tmp, guard }
}

fn p(c: &Cenario, rel: &str) -> String {
    c.tmp.path().join(rel).to_string_lossy().into_owned()
}

#[test]
fn pasta_de_trabalho_livre() {
    let c = cenario();
    assert_eq!(c.guard.check_write(&p(&c, "Documents/nota.md")), None);
    assert_eq!(c.guard.check_write(&p(&c, "Documents/sub/script.py")), None);
}

#[test]
fn codigo_do_orion_e_imutavel() {
    let c = cenario();
    for rel in ["Orion/orion/policy/engine.py", "Orion/Orion_Ollama/orion_seguranca.py", "Orion/Memorias Do Projeto/ORION_REGRAS.md", "Orion/novo.txt"] {
        assert!(c.guard.check_write(&p(&c, rel)).unwrap_or_default().contains("núcleo imutável"), "{rel}");
    }
}

#[test]
fn dotdot_e_symlink_nao_escapam() {
    let c = cenario();
    assert!(c.guard.check_write(&p(&c, "Documents/../Orion/x.py")).is_some());
    #[cfg(unix)]
    {
        let link = c.tmp.path().join("Documents/atalho");
        std::os::unix::fs::symlink(c.tmp.path().join("Orion"), &link).unwrap();
        assert!(c.guard.check_write(&link.join("x.py").to_string_lossy()).is_some());
    }
}

#[test]
fn extensoes_e_nomes_sensiveis() {
    let c = cenario();
    for nome in ["a.exe", "b.ps1", "c.bat", "d.lnk", ".env", "authorized_keys"] {
        assert!(c.guard.check_write(&p(&c, &format!("Documents/{nome}"))).is_some(), "{nome}");
    }
}

#[test]
fn componentes_sensiveis() {
    let c = cenario();
    for sub in [".ssh/config", "Startup/a.txt", ".aws/credentials"] {
        assert!(c.guard.check_write(&p(&c, &format!("Documents/{sub}"))).is_some(), "{sub}");
    }
}

#[test]
fn fora_das_pastas_de_trabalho() {
    let c = cenario();
    assert!(c.guard.check_write(&p(&c, "outro/a.txt")).unwrap_or_default().contains("fora das pastas"));
}

#[test]
fn caminho_vazio() {
    let c = cenario();
    assert!(c.guard.check_write("").is_some());
    assert!(c.guard.check_organize("  ").is_some());
}

#[test]
fn organizar_raizes_e_projeto() {
    let c = cenario();
    assert!(c.guard.check_organize("/").is_some());
    assert!(c.guard.check_organize(&p(&c, "Orion")).is_some());
    assert_eq!(c.guard.check_organize(&p(&c, "Documents/bagunca")), None);
}

#[test]
fn ler_segredo_ou_chave_pede_confirmacao() {
    let c = cenario();
    for rel in [
        ".env", "Orion/.env", "Orion/.env.local", ".env.production", ".ssh/id_rsa", "casa/.ssh/qualquer.txt", ".aws/credentials",
        ".gnupg/pubring.kbx", ".kube/config", "Orion/Orion_Core/google_auth/token.json", "Orion/Orion_Core/google_auth/qualquer.json",
        "chaves/servidor.pem", "chaves/SERVIDOR.KEY", "cofre.kdbx", "projeto/credentials.json", "id_ed25519", ".netrc", ".git-credentials",
        "secrets.json", "dados/auth.db", "dados/auth.db-wal", "dados/mcp.json",
    ] {
        assert!(c.guard.check_read(&p(&c, rel)).is_some(), "{rel}");
    }
}

#[test]
fn ler_arquivo_comum_nao_pede_confirmacao() {
    let c = cenario();
    for rel in [
        "Documents/nota.md", "Orion/README.md", "Orion/orion/policy/engine.py", ".env.example", "Orion/.env.sample", "Orion/.env.template",
        "pasta/environment.md", "projeto/chave-de-acesso.txt", "Orion/mcp.example.json",
    ] {
        assert_eq!(c.guard.check_read(&p(&c, rel)), None, "{rel}");
    }
}

#[test]
fn leitura_vazia_pede_confirmacao() {
    let c = cenario();
    assert!(c.guard.check_read("").is_some() && c.guard.check_read("   ").is_some());
}

#[cfg(unix)]
#[test]
fn symlink_para_segredo_nao_escapa() {
    let c = cenario();
    let env = c.tmp.path().join("Documents/.env");
    std::fs::write(&env, "X=1").unwrap();
    let link: PathBuf = c.tmp.path().join("Documents/inocente.txt");
    std::os::unix::fs::symlink(&env, &link).unwrap();
    assert!(c.guard.check_read(&link.to_string_lossy()).is_some(), "o destino é .env: resolve antes de comparar");
}

#[test]
fn escrever_mcp_json_ou_auth_db_pede_confirmacao_ate_nas_pastas_seguras() {
    let c = cenario();
    assert_eq!(c.guard.check_write(&p(&c, "Documents/nota.txt")), None);
    for nome in ["mcp.json", "auth.db", ".mcp.json"] {
        assert!(c.guard.check_write(&p(&c, &format!("Documents/{nome}"))).is_some(), "{nome}");
    }
    let _ = Path::new("");
}

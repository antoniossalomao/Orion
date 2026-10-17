//! Paridade com `tests/test_desligar.py` ("Posso desligar?") e `tests/test_autostart.py` (início
//! automático por sistema).
//!
//! Diferenças deliberadas do Rust: o serviço sobe o próprio executável (`orion serve`), sem
//! `python -m orion`; o Windows usa `"<exe>" desktop --background`. Fora daqui: a rota `/posso-desligar` e a CLI
//! `orion autostart` (`orion-server`).

use orion_core::autostart::{detectar, instalar, render};
use orion_core::desligar::{conferir_downloads, conferir_repo, posso_desligar, texto};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn tmp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(st.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&st.stderr));
}

fn tem_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn texto_do(v: &Value) -> &str {
    v["texto"].as_str().unwrap()
}

// ── desligar ────────────────────────────────────────────────────────────────
#[test]
fn repo_limpo_sujo_e_com_commit_nao_enviado() {
    if !tem_git() {
        return;
    }
    let d = tmp();
    // repositório "remoto" para o limpo ficar em dia com ele
    let remoto = d.path().join("remoto.git");
    std::fs::create_dir(&remoto).unwrap();
    git(&remoto, &["init", "-q", "--bare"]);
    let limpo = d.path().join("limpo");
    std::fs::create_dir(&limpo).unwrap();
    git(&limpo, &["init", "-q", "-b", "main"]);
    git(&limpo, &["commit", "-q", "--allow-empty", "-m", "x"]);
    git(&limpo, &["remote", "add", "origin", remoto.to_str().unwrap()]);
    git(&limpo, &["push", "-q", "origin", "main"]);
    let r = conferir_repo(&limpo);
    assert!(r["ok"] == true && texto_do(&r).contains("tudo commitado e enviado"), "{r}");

    // sujo: 2 arquivos novos/alterados
    std::fs::write(limpo.join("x.py"), "1").unwrap();
    std::fs::write(limpo.join("y"), "2").unwrap();
    let r = conferir_repo(&limpo);
    assert!(r["ok"] == false && texto_do(&r).contains("2 arquivo(s)"), "{r}");

    // commit local sem remoto: nada foi enviado
    let ahead = d.path().join("ahead");
    std::fs::create_dir(&ahead).unwrap();
    git(&ahead, &["init", "-q"]);
    git(&ahead, &["commit", "-q", "--allow-empty", "-m", "x"]);
    let r = conferir_repo(&ahead);
    assert!(texto_do(&r).contains("1 commit(s) não enviado"), "{r}");
    std::fs::write(ahead.join("novo.txt"), "x").unwrap();
    assert!(texto_do(&conferir_repo(&ahead)).contains("1 arquivo(s)"));
}

#[test]
fn git_que_falha_nao_finge_que_esta_ok_e_pasta_solta_nao_conta() {
    let d = tmp();
    let quebrado = d.path().join("quebrado");
    std::fs::create_dir_all(quebrado.join(".git")).unwrap(); // .git vazio: o git recusa
    let r = conferir_repo(&quebrado);
    assert!(r["ok"] == false && texto_do(&r).contains("não consegui"), "{r}");
    let solta = d.path().join("solta");
    std::fs::create_dir(&solta).unwrap();
    let r = conferir_repo(&solta);
    assert!(r["ok"] == true && texto_do(&r).contains("não é um repositório"), "{r}");
}

#[test]
fn backup_downloads_e_tarefa() {
    let d = tmp();
    let (bk, dl) = (d.path().join("bk"), d.path().join("dl"));
    std::fs::create_dir(&bk).unwrap();
    std::fs::create_dir(&dl).unwrap();
    let hoje = chrono::Local::now().format("%Y%m%d").to_string();
    let pendentes = |r: &Value| -> Vec<String> { r["itens"].as_array().unwrap().iter().filter(|i| i["ok"] != true).map(|i| i["id"].as_str().unwrap().to_string()).collect() };
    let r = posso_desligar(&[], Some(&bk), &dl, &[]);
    assert_eq!(pendentes(&r), ["backup"]);
    std::fs::write(bk.join(format!("orion-{hoje}-010101.db")), "x").unwrap();
    std::fs::write(dl.join("filme.mkv.crdownload"), "x").unwrap();
    std::fs::write(dl.join("ok.pdf"), "x").unwrap();
    let r = posso_desligar(&[], Some(&bk), &dl, &[json!({"status": "rodando", "objective": "pesquisar"})]);
    let mut p = pendentes(&r);
    p.sort();
    assert_eq!(p, ["downloads", "tarefa_bg"]);
    assert!(r["pendentes"] == 2 && texto(&r).starts_with("⚠️ Ainda não: 2"), "{}", texto(&r));
    assert_eq!(conferir_downloads(&d.path().join("inexistente"))["ok"], true);
    std::fs::remove_file(dl.join("filme.mkv.crdownload")).unwrap();
    let limpo = posso_desligar(&[], None, &dl, &[]);
    assert!(limpo["pode"] == true && texto(&limpo).starts_with("✅ Pode desligar"));
}

// ── autostart ───────────────────────────────────────────────────────────────
fn exe() -> PathBuf {
    PathBuf::from("/opt/orion/orion")
}

fn gerar(plataforma: &str, ponte: bool) -> orion_core::autostart::Autostart {
    render(plataforma, &exe(), Path::new("/home/ana & co/Orion"), Path::new("/dados/logs"), Path::new("/home/ana"), ponte).unwrap()
}

#[test]
fn macos_gera_plist_valido_com_escape_e_a_pasta_do_projeto() {
    let a = gerar("macos", false);
    // XML bem formado, mesmo com "&" no caminho (o `&amp;` evita quebrar o plist)
    let mut leitor = quick_xml::Reader::from_str(&a.conteudo);
    loop {
        match leitor.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(_) => {}
            Err(e) => panic!("plist inválido: {e}\n{}", a.conteudo),
        }
    }
    assert!(a.conteudo.contains("<string>com.orion.assistente</string>"));
    assert!(a.conteudo.contains("<string>/opt/orion/orion</string>") && a.conteudo.contains("<string>serve</string>"));
    assert!(a.conteudo.contains("<key>WorkingDirectory</key><string>/home/ana &amp; co/Orion</string>")); // é de lá que o .env é lido
    assert!(a.conteudo.contains("<key>RunAtLoad</key><true/>") && a.conteudo.contains("<key>KeepAlive</key><true/>"));
    assert!(a.conteudo.contains(&Path::new("/dados/logs").join("orion.out.log").to_string_lossy().to_string()));
    assert_eq!(a.arquivo, PathBuf::from("/home/ana/Library/LaunchAgents/com.orion.assistente.plist"));
    assert!(a.ativar.contains("launchctl bootstrap") && a.desativar.contains("launchctl bootout"));
}

#[test]
fn linux_gera_unit_do_systemd_com_caminho_com_espaco_e_percent_escapado() {
    let a = render("linux", Path::new("/opt/meu py/orion"), Path::new("/srv/100%/Orion"), Path::new("/dados/logs"), Path::new("/home/ana"), false).unwrap();
    assert!(a.conteudo.contains("ExecStart=\"/opt/meu py/orion\" serve"), "{}", a.conteudo);
    // % é especificador no systemd
    assert!(a.conteudo.contains("WorkingDirectory=/srv/100%%/Orion"));
    assert!(a.conteudo.contains("Restart=on-failure") && a.conteudo.contains("WantedBy=default.target"));
    assert_eq!(a.arquivo, PathBuf::from("/home/ana/.config/systemd/user/orion.service"));
    assert!(a.ativar.contains("enable --now orion.service"));
}

#[test]
fn windows_gera_cmd_na_pasta_inicializar_sem_expandir_variaveis() {
    let a = render("windows", Path::new("C:/py thon/orion.exe"), Path::new("C:/Orion %PATH%"), Path::new("/dados/logs"), Path::new("C:/Users/ana"), false).unwrap();
    assert!(a.arquivo.file_name().unwrap() == "orion.cmd" && a.arquivo.components().any(|c| c.as_os_str() == "Startup"));
    assert!(a.conteudo.contains("cd /d \"C:/Orion %%PATH%%\""), "{}", a.conteudo); // %PATH% literal, não expandido
    assert!(a.conteudo.contains("start \"Orion\" /min \"C:/py thon/orion.exe\" desktop --background"));
    assert_eq!(a.conteudo.matches("\r\n").count(), a.conteudo.matches('\n').count()); // fim de linha do Windows
}

#[test]
fn plataforma_desconhecida_e_recusada() {
    let e = render("beos", &exe(), Path::new("/p"), Path::new("/l"), Path::new("/h"), false).unwrap_err();
    assert!(e.contains("desconhecida"), "{e}");
    assert!(["windows", "macos", "linux"].contains(&detectar()));
}

#[test]
fn instalar_nao_sobrescreve_sem_force_e_cria_a_pasta() {
    let d = tmp();
    let a = gerar("linux", false);
    let destino = d.path().join("novo").join("pasta");
    let alvo = instalar(&a, Some(&destino), false).unwrap();
    assert_eq!(std::fs::read_to_string(&alvo).unwrap(), a.conteudo);
    assert!(instalar(&a, Some(&destino), false).unwrap_err().contains("--force"));
    std::fs::write(&alvo, "editado").unwrap();
    instalar(&a, Some(&destino), true).unwrap();
    assert_eq!(std::fs::read_to_string(&alvo).unwrap(), a.conteudo);
}

#[test]
fn ponte_gera_arquivo_a_parte_e_nao_toca_no_do_servidor() {
    let mac = gerar("macos", true);
    assert!(mac.conteudo.contains("<string>com.orion.ponte</string>") && mac.conteudo.contains("<string>ponte</string>"));
    assert_eq!(mac.arquivo.file_name().unwrap(), "com.orion.ponte.plist");
    assert!(mac.conteudo.contains(&Path::new("/dados/logs").join("orion-ponte.out.log").to_string_lossy().to_string()));
    let linux = gerar("linux", true);
    assert_eq!(linux.arquivo.file_name().unwrap(), "orion-ponte.service");
    assert!(linux.conteudo.contains("\" ponte") && linux.conteudo.contains("graphical-session.target"));
    assert!(linux.ativar.contains("enable --now orion-ponte.service"));
    let win = gerar("windows", true);
    assert!(win.arquivo.file_name().unwrap() == "orion-ponte.cmd" && win.conteudo.contains("\" ponte"));
    // o do servidor continua igual
    assert_eq!(gerar("linux", false).arquivo.file_name().unwrap(), "orion.service");
    assert!(gerar("windows", false).conteudo.contains("\" desktop --background"));
}

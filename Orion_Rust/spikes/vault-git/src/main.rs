//! Spike F00.26: gravação no vault com git2 (commit, push, pull por fast-forward) e conflito com edição no Obsidian.
use anyhow::{ensure, Result};
use git2::{build::CheckoutBuilder, Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository, Signature};
use std::path::Path;
use std::time::Instant;

fn sig() -> Signature<'static> { Signature::now("Orion", "orion@localhost").unwrap() }

/// Commita só os caminhos pedidos (nunca `git add -A`: não pega o que o Obsidian está editando).
fn commit_paths(repo: &Repository, paths: &[&str], msg: &str) -> Result<git2::Oid> {
    let mut idx = repo.index()?;
    for p in paths { idx.add_path(Path::new(p))?; }
    idx.write()?;
    let tree = repo.find_tree(idx.write_tree()?)?;
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    Ok(repo.commit(Some("HEAD"), &sig(), &sig(), msg, &tree, &parents)?)
}

fn callbacks<'a>(repo_cfg: git2::Config, use_helper: bool) -> RemoteCallbacks<'a> {
    let mut cb = RemoteCallbacks::new();
    let mut tries = 0;
    cb.credentials(move |url, user, _allowed| {
        tries += 1;
        if tries > 2 { return Err(git2::Error::from_str("credencial recusada")); }
        if use_helper { Cred::credential_helper(&repo_cfg, url, user) } else { Cred::userpass_plaintext("errado", "errado") }
    });
    cb
}

fn push(repo: &Repository, url: &str, cfg: git2::Config, helper: bool) -> Result<()> {
    let mut remote = repo.remote_anonymous(url)?;
    let mut rejected = None;
    {
        let mut cb = callbacks(cfg, helper);
        cb.push_update_reference(|r, status| { if let Some(s) = status { rejected = Some(format!("{r}: {s}")); } Ok(()) });
        let mut o = PushOptions::new();
        o.remote_callbacks(cb);
        remote.push(&["refs/heads/main:refs/heads/main"], Some(&mut o))?;
    }
    if let Some(r) = rejected { anyhow::bail!("push rejeitado: {r}"); }
    Ok(())
}

enum Pull { UpToDate, FastForward, Diverged, Blocked(String) }

/// Pull só por fast-forward; recusa se o checkout sobrescreveria edição não commitada.
fn pull_ff(repo: &Repository, url: &str, cfg: git2::Config, helper: bool) -> Result<Pull> {
    let mut remote = repo.remote_anonymous(url)?;
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(callbacks(cfg, helper));
    remote.fetch(&["refs/heads/main:refs/orion/remote-main"], Some(&mut fo), None)?;
    let theirs = repo.find_reference("refs/orion/remote-main")?;
    let ac = repo.reference_to_annotated_commit(&theirs)?;
    let (an, _) = repo.merge_analysis(&[&ac])?;
    if an.is_up_to_date() { return Ok(Pull::UpToDate); }
    if !an.is_fast_forward() { return Ok(Pull::Diverged); }
    let target = repo.find_object(ac.id(), None)?;
    let mut co = CheckoutBuilder::new();
    co.safe(); // não sobrescreve arquivo modificado no working tree
    match repo.checkout_tree(&target, Some(&mut co)) {
        Ok(()) => { repo.head()?.set_target(ac.id(), "pull ff")?; Ok(Pull::FastForward) }
        Err(e) => Ok(Pull::Blocked(e.message().to_string())),
    }
}

fn main() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let bare_dir = tmp.path().join("remote.git");
    Repository::init_bare(&bare_dir)?.set_head("refs/heads/main")?;
    let url = format!("file://{}", bare_dir.display());
    let (a_dir, b_dir) = (tmp.path().join("pc"), tmp.path().join("celular"));
    let a = Repository::init(&a_dir)?;
    a.set_head("refs/heads/main")?;
    let cfg = || a.config().unwrap();

    std::fs::write(a_dir.join("nota1.md"), "# Nota 1\nconteúdo\n")?;
    std::fs::write(a_dir.join("nota2.md"), "# Nota 2\nconteúdo\n")?;
    let t = Instant::now();
    commit_paths(&a, &["nota1.md", "nota2.md"], "Orion: cria notas")?;
    push(&a, &url, cfg(), true)?;
    println!("1. commit + push (file://): ok em {:?}", t.elapsed());

    let b = Repository::clone(&url, &b_dir)?;
    // Outro dispositivo muda nota1 e empurra.
    std::fs::write(b_dir.join("nota1.md"), "# Nota 1\neditada no celular\n")?;
    commit_paths(&b, &["nota1.md"], "celular: edita nota1")?;
    push(&b, &url, b.config()?, true)?;

    // Obsidian no PC está com nota1 aberta e editada (não salva no git).
    std::fs::write(a_dir.join("nota1.md"), "# Nota 1\neditada no Obsidian\n")?;
    match pull_ff(&a, &url, cfg(), true)? {
        Pull::Blocked(m) => println!("2. pull com nota1 suja no Obsidian: BLOQUEADO (conflito detectado, nada sobrescrito): {m}"),
        _ => anyhow::bail!("deveria bloquear"),
    }
    ensure!(std::fs::read_to_string(a_dir.join("nota1.md"))?.contains("Obsidian"));

    // Edição em outro arquivo não impede o pull.
    std::fs::write(a_dir.join("nota1.md"), "# Nota 1\nconteúdo\n")?;
    std::fs::write(a_dir.join("nota2.md"), "# Nota 2\neditada no Obsidian\n")?;
    match pull_ff(&a, &url, cfg(), true)? {
        Pull::FastForward => println!("3. pull com outra nota suja: fast-forward ok, edição do Obsidian preservada"),
        _ => anyhow::bail!("deveria avançar"),
    }
    ensure!(std::fs::read_to_string(a_dir.join("nota1.md"))?.contains("celular"));
    ensure!(std::fs::read_to_string(a_dir.join("nota2.md"))?.contains("Obsidian"));

    // Divergência: ambos commitam em cima do mesmo pai.
    std::fs::write(b_dir.join("nota3.md"), "# 3\nb\n")?;
    commit_paths(&b, &["nota3.md"], "celular: nota3")?;
    push(&b, &url, b.config()?, true)?;
    std::fs::write(a_dir.join("nota4.md"), "# 4\na\n")?;
    commit_paths(&a, &["nota4.md"], "pc: nota4")?;
    match push(&a, &url, cfg(), true) { Err(e) => println!("4. push divergente rejeitado: {e}"), Ok(_) => anyhow::bail!("deveria rejeitar") }
    match pull_ff(&a, &url, cfg(), true)? { Pull::Diverged => println!("   pull detecta divergência (sem merge automático)"), _ => anyhow::bail!("deveria divergir") }

    // HTTP com Basic Auth + credential helper (no Windows seria o Git Credential Manager).
    if let (Some(root), Some(port)) = (std::env::args().nth(1), std::env::args().nth(2)) {
        let srv = Path::new(&root).join("remote.git");
        if !srv.exists() { Repository::init_bare(&srv)?.set_head("refs/heads/main")?; }
        let url = format!("http://127.0.0.1:{port}/remote.git");
        let helper = tmp.path().join("helper.sh");
        std::fs::write(&helper, "#!/bin/sh\nif [ \"$1\" = get ]; then echo username=orion; echo password=segredo; fi\n")?;
        std::process::Command::new("chmod").arg("+x").arg(&helper).status()?;
        b.config()?.set_str("credential.helper", &helper.display().to_string())?;
        let r_bad = push(&b, &url, b.config()?, false);
        ensure!(r_bad.is_err());
        println!("5. push HTTP com credencial errada: recusado ({})", r_bad.unwrap_err().to_string().lines().next().unwrap_or(""));
        let t = Instant::now();
        push(&b, &url, b.config()?, true)?;
        println!("   push HTTP com credential helper: ok em {:?} (helper devolveu usuário/senha; nada em arquivo do Orion)", t.elapsed());
    }

    // Escala: vault real.
    if let Ok(v) = std::env::var("VAULT") {
        let r = Repository::open(&v)?;
        let t = Instant::now();
        let st = r.statuses(None)?;
        println!("6. status do vault real ({} entradas sujas) em {:?}", st.len(), t.elapsed());
    }
    Ok(())
}

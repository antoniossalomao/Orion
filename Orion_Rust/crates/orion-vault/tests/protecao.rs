//! Repositórios descartáveis provam preservação de dados e isolamento dos commits.
use orion_vault::{ConversationNote, VaultManager};
use std::{path::Path, process::Command};
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.quotePath=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "Git de teste falhou: {:?}", args);
    String::from_utf8(out.stdout)
        .unwrap()
        .trim_end()
        .to_string()
}
fn fixture(repository: bool) -> TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(d.path().join("01 Projetos/Orion")).unwrap();
    std::fs::write(
        d.path().join("01 Projetos/Orion/Orion.md"),
        "---\ntype: projeto\n---\n# Orion\n",
    )
    .unwrap();
    if repository {
        git(d.path(), &["init", "-b", "main"]);
        git(d.path(), &["config", "user.name", "Teste Orion"]);
        git(d.path(), &["config", "user.email", "teste@example.invalid"]);
        git(d.path(), &["add", "."]);
        git(d.path(), &["commit", "-m", "Base local"]);
    }
    d
}
fn note() -> ConversationNote {
    ConversationNote {
        date_str: "2026-10-20".into(),
        time_str: "22:00".into(),
        project_name: "Orion".into(),
        title: "Conversa protegida".into(),
        summary: "Preserve as edições locais".into(),
        tags: vec!["orion".into()],
        dialogue_markdown: "Texto de teste sem credenciais.".into(),
        decisions: vec![],
    }
}
fn manager(d: &TempDir) -> VaultManager {
    VaultManager::new(Some(d.path().into())).unwrap()
}

#[test]
fn preserva_obsidian_e_proposta_sem_commit_e_sem_duplicar_em_retry() {
    let d = fixture(true);
    let mgr = manager(&d);
    let n = note();
    let first = mgr.save_conversation(&n).unwrap();
    assert!(first.commit_hash.is_some());
    let original = d.path().join(&first.file_path);
    std::fs::write(&original, "Edição feita no Obsidian.\n").unwrap();
    let head = git(d.path(), &["rev-parse", "HEAD"]);
    let second = mgr.save_conversation(&n).unwrap();
    let c = second.conflict.unwrap();
    assert_eq!(
        std::fs::read_to_string(&original).unwrap(),
        "Edição feita no Obsidian.\n"
    );
    assert!(std::fs::read_to_string(d.path().join(&c.proposed_path))
        .unwrap()
        .contains(&n.dialogue_markdown));
    assert!(second.commit_hash.is_none() && !second.pushed && second.warning.is_some());
    assert_eq!(git(d.path(), &["rev-parse", "HEAD"]), head);
    let retry = mgr.save_conversation(&n).unwrap().conflict.unwrap();
    assert_eq!(c.proposed_path, retry.proposed_path);
}

#[test]
fn repetir_conteudo_identico_nao_cria_commit_ou_conflito() {
    let d = fixture(true);
    let mgr = manager(&d);
    let n = note();
    let first = mgr.save_conversation(&n).unwrap();
    let second = mgr.save_conversation(&n).unwrap();
    assert!(second.unchanged && second.conflict.is_none() && !second.pushed);
    assert_eq!(first.commit_hash, second.commit_hash);
    assert!(git(d.path(), &["status", "--porcelain"]).is_empty());
}

#[test]
fn commit_so_da_nota_preserva_staged_e_unstaged_do_usuario() {
    let d = fixture(true);
    std::fs::write(
        d.path().join("segredo-local.txt"),
        "conteúdo não autorizado para o commit",
    )
    .unwrap();
    git(d.path(), &["add", "segredo-local.txt"]);
    std::fs::write(
        d.path().join("segredo-local.txt"),
        "outra edição ainda não staged",
    )
    .unwrap();
    let staged = git(d.path(), &["show", ":segredo-local.txt"]);
    let result = manager(&d).save_conversation(&note()).unwrap();
    assert!(result.commit_hash.is_some(), "{:?}", result.warning);
    assert_eq!(
        git(
            d.path(),
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        ),
        result.file_path
    );
    assert_eq!(git(d.path(), &["show", ":segredo-local.txt"]), staged);
    assert_eq!(
        std::fs::read_to_string(d.path().join("segredo-local.txt")).unwrap(),
        "outra edição ainda não staged"
    );
    assert_eq!(
        git(d.path(), &["diff", "--cached", "--name-only"]),
        "segredo-local.txt"
    );
    assert!(!git(d.path(), &["ls-tree", "-r", "--name-only", "HEAD"]).contains("segredo-local.txt"));
}

#[test]
fn nao_muda_branch_ou_indice_quando_vault_esta_em_outra_branch() {
    let d = fixture(true);
    git(d.path(), &["checkout", "-b", "minha-edicao"]);
    let head = git(d.path(), &["rev-parse", "HEAD"]);
    let result = manager(&d).save_conversation(&note()).unwrap();
    assert!(result.commit_hash.is_none() && result.warning.is_some());
    assert_eq!(git(d.path(), &["branch", "--show-current"]), "minha-edicao");
    assert_eq!(git(d.path(), &["rev-parse", "HEAD"]), head);
    assert!(d.path().join(result.file_path).is_file());
}

#[test]
fn nao_substitui_versao_staged_da_mesma_nota() {
    let d = fixture(true);
    let n = note();
    let rel = format!("03 Recursos/Orion/Memória/{}", n.file_name());
    std::fs::create_dir_all(d.path().join("03 Recursos/Orion/Memória")).unwrap();
    std::fs::write(d.path().join(&rel), "versão staged do usuário").unwrap();
    git(d.path(), &["add", "--", &rel]);
    std::fs::write(d.path().join(&rel), n.to_markdown()).unwrap();
    let head = git(d.path(), &["rev-parse", "HEAD"]);
    let result = manager(&d).save_conversation(&n).unwrap();
    assert!(result.commit_hash.is_none() && result.warning.is_some());
    assert_eq!(
        git(d.path(), &["show", &format!(":{rel}")]),
        "versão staged do usuário"
    );
    assert_eq!(git(d.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn bloqueia_segredos_em_qualquer_campo_antes_de_criar_arquivos() {
    let d = fixture(true);
    let head = git(d.path(), &["rev-parse", "HEAD"]);
    let fake = format!("github_pat_{}", "x".repeat(40));
    for field in 0..5 {
        let mut n = note();
        match field {
            0 => n.summary = fake.clone(),
            1 => n.title = fake.clone(),
            2 => n.dialogue_markdown = fake.clone(),
            3 => n.decisions = vec![fake.clone()],
            _ => n.tags = vec![fake.clone()],
        }
        let error = manager(&d).save_conversation(&n).unwrap_err().to_string();
        assert!(!error.contains(&fake));
    }
    assert!(!d.path().join("03 Recursos").exists());
    assert_eq!(git(d.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn bloqueia_dados_malformados_links_ausentes_e_projeto_inexistente() {
    let d = fixture(false);
    for field in 0..6 {
        let mut n = note();
        match field {
            0 => n.date_str = "../../fora".into(),
            1 => n.time_str = "30:99".into(),
            2 => n.project_name = "Ausente".into(),
            3 => n.summary = "linha\ninjetada".into(),
            4 => n.dialogue_markdown = "[[Nota que não existe]]".into(),
            _ => n.project_name = "../fora".into(),
        }
        assert!(manager(&d).save_conversation(&n).is_err());
    }
    assert!(!d.path().join("03 Recursos").exists());
}

#[test]
fn pull_nao_faz_stash_rebase_ou_descarta_edicoes() {
    let d = fixture(true);
    std::fs::write(d.path().join("edicao.md"), "texto local").unwrap();
    assert!(manager(&d).pull().is_err());
    assert_eq!(
        std::fs::read_to_string(d.path().join("edicao.md")).unwrap(),
        "texto local"
    );
    assert!(git(d.path(), &["stash", "list"]).is_empty());
}

#[test]
fn respeita_lock_existente_do_git() {
    let d = fixture(true);
    let lock = d.path().join(".git/index.lock");
    std::fs::write(&lock, "outro processo").unwrap();
    let result = manager(&d).save_conversation(&note()).unwrap();
    assert!(result.commit_hash.is_none() && result.warning.is_some());
    assert_eq!(std::fs::read_to_string(lock).unwrap(), "outro processo");
}

#[test]
fn push_publica_so_um_commit_e_nao_envia_historico_local_pendente() {
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--bare", "-b", "main"]);
    let d = fixture(true);
    git(
        d.path(),
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    git(d.path(), &["push", "origin", "main"]);
    let first = manager(&d).save_conversation(&note()).unwrap();
    assert!(first.pushed, "{:?}", first.warning);
    assert_eq!(
        git(remote.path(), &["rev-parse", "main"]),
        first.commit_hash.unwrap()
    );
    std::fs::write(d.path().join("outro.md"), "não publicar automaticamente").unwrap();
    git(d.path(), &["add", "outro.md"]);
    git(d.path(), &["commit", "-m", "Trabalho ainda não publicado"]);
    let before = git(remote.path(), &["rev-parse", "main"]);
    let mut n = note();
    n.title = "Outra conversa".into();
    let second = manager(&d).save_conversation(&n).unwrap();
    assert!(second.commit_hash.is_some() && !second.pushed && second.warning.is_some());
    assert_eq!(git(remote.path(), &["rev-parse", "main"]), before);
}

#[cfg(unix)]
#[test]
fn nao_segue_symlink_na_gravacao_ou_indexacao() {
    use std::os::unix::fs::symlink;
    let d = fixture(false);
    let external = tempfile::tempdir().unwrap();
    symlink(external.path(), d.path().join("03 Recursos")).unwrap();
    assert!(manager(&d).save_conversation(&note()).is_err());
    assert_eq!(std::fs::read_dir(external.path()).unwrap().count(), 0);
    std::fs::write(external.path().join("privada.md"), "não indexar").unwrap();
    assert_eq!(manager(&d).index_all().unwrap().total_notes, 1);
}

#[test]
fn duas_gravacoes_concorrentes_preservam_todas_as_versoes() {
    let d = fixture(false);
    let mut jobs = vec![];
    for i in 0..6 {
        let root = d.path().to_path_buf();
        jobs.push(std::thread::spawn(move || {
            let mut n = note();
            n.dialogue_markdown = format!("Versão concorrente {i}");
            VaultManager::new(Some(root))
                .unwrap()
                .save_conversation(&n)
                .unwrap()
        }));
    }
    let results: Vec<_> = jobs.into_iter().map(|j| j.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.conflict.is_none()).count(), 1);
    let folder = d.path().join("03 Recursos/Orion/Memória");
    let contents: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    assert_eq!(contents.len(), 6);
    for i in 0..6 {
        assert!(contents
            .iter()
            .any(|s| s.contains(&format!("Versão concorrente {i}"))));
    }
}

#[test]
fn nao_commita_durante_merge_e_preserva_o_indice() {
    let d = fixture(true);
    let head = git(d.path(), &["rev-parse", "HEAD"]);
    std::fs::write(d.path().join(".git/MERGE_HEAD"), &head).unwrap();
    let result = manager(&d).save_conversation(&note()).unwrap();
    assert!(result.commit_hash.is_none() && result.warning.is_some());
    assert_eq!(git(d.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read_to_string(d.path().join(".git/MERGE_HEAD")).unwrap(),
        head
    );
}

#[cfg(unix)]
#[test]
fn git_nao_executa_hooks_filtros_ou_fsmonitor_do_vault() {
    use std::os::unix::fs::PermissionsExt;
    let d = fixture(true);
    let script = d.path().join("perigoso.sh");
    std::fs::write(&script, "#!/bin/sh\ntouch indevido\ncat\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let hooks = d.path().join("hooks");
    std::fs::create_dir(&hooks).unwrap();
    std::fs::copy(&script, hooks.join("pre-commit")).unwrap();
    git(
        d.path(),
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    git(
        d.path(),
        &["config", "core.fsmonitor", script.to_str().unwrap()],
    );
    git(
        d.path(),
        &["config", "filter.perigoso.clean", script.to_str().unwrap()],
    );
    std::fs::write(d.path().join(".gitattributes"), "*.md filter=perigoso\n").unwrap();
    std::fs::write(
        d.path().join("01 Projetos/Orion/Orion.md"),
        "Edição local do projeto",
    )
    .unwrap();
    let result = manager(&d).save_conversation(&note()).unwrap();
    assert!(result.commit_hash.is_some(), "{:?}", result.warning);
    assert!(!d.path().join("indevido").exists());
    assert!(manager(&d).pull().is_err());
    assert!(!d.path().join("indevido").exists());
    assert_eq!(
        git(d.path(), &["show", &format!("HEAD:{}", result.file_path)]),
        note().to_markdown().trim_end()
    );
}

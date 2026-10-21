use crate::scanner::SecretScanner;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("repositório Git não encontrado")]
    NotAGitRepo,
    #[error("Git recusou a operação {0}; a nota local foi preservada")]
    CommandFailed(&'static str),
    #[error("{0}")]
    Unsafe(&'static str),
    #[error("erro de E/S ao executar Git")]
    Io(#[from] std::io::Error),
}

/// Commit exato preparado pelo Orion; o push nunca usa a ponta mutável da branch.
pub(crate) struct NoteCommit {
    pub hash: String,
    pub parent: Option<String>,
    pub created: bool,
}

/// Git do vault: main, sem hooks/fsmonitor, sem conteúdo ou segredos nos argumentos/logs.
/// REGRAS.md: o que o modelo escreve nunca vira comando; segredos nunca em processo filho.
pub struct GitSync {
    vault_path: PathBuf,
}

// Protege o índice também contra outros processos Git. Nunca apaga lock de outro processo.
struct IndexLock {
    path: PathBuf,
    file: Option<std::fs::File>,
    published: bool,
}
impl IndexLock {
    fn acquire(index: &Path) -> Result<Self, GitError> {
        let mut name = index.as_os_str().to_os_string();
        name.push(".lock");
        let path = PathBuf::from(name);
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| {
                GitError::Unsafe("outro processo está usando o índice Git; tente novamente")
            })?;
        Ok(Self {
            path,
            file: Some(file),
            published: false,
        })
    }
    fn prepare(&mut self, bytes: &[u8]) -> Result<(), GitError> {
        let file = self.file.as_mut().expect("lock aberto");
        file.write_all(bytes)?;
        file.sync_all()?;
        self.file.take();
        Ok(())
    }
    fn publish(mut self, index: &Path) -> Result<(), GitError> {
        std::fs::rename(&self.path, index)?;
        self.published = true;
        Ok(())
    }
}
impl Drop for IndexLock {
    fn drop(&mut self) {
        self.file.take();
        if !self.published {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl GitSync {
    pub fn new(vault_path: impl Into<PathBuf>) -> Result<Self, GitError> {
        let vault_path = vault_path.into().canonicalize()?;
        if !vault_path.join(".git").exists() {
            return Err(GitError::NotAGitRepo);
        }
        let sync = Self { vault_path };
        let root = sync.text(&["rev-parse", "--show-toplevel"], None, None)?;
        if Path::new(&root).canonicalize()? != sync.vault_path {
            return Err(GitError::Unsafe(
                "o vault precisa ser a raiz do seu repositório Git",
            ));
        }
        Ok(sync)
    }

    fn command(&self, index: Option<&Path>) -> Command {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.vault_path).env_clear();
        // Ambiente mínimo para Git, cofre do SO, certificados e proxy configurado.
        for key in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "SystemRoot",
            "WINDIR",
            "APPDATA",
            "LOCALAPPDATA",
            "TEMP",
            "TMP",
            "TMPDIR",
            "SSH_AUTH_SOCK",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "GIT_SSL_CAINFO",
        ] {
            if let Some(value) = std::env::var_os(key) {
                cmd.env(key, value);
            }
        }
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_LITERAL_PATHSPECS", "1");
        if let Some(index) = index {
            cmd.env("GIT_INDEX_FILE", index);
            // A revalidação de entradas do índice não pode ler a árvore de trabalho real
            // nem disparar filtros configurados nos arquivos do usuário.
            cmd.env(
                "GIT_WORK_TREE",
                index.parent().expect("índice temporário com pasta"),
            );
        }
        cmd.args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.quotePath=false",
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        cmd
    }

    fn output(
        &self,
        args: &[&str],
        index: Option<&Path>,
        input: Option<&[u8]>,
    ) -> Result<std::process::Output, GitError> {
        let mut cmd = self.command(index);
        cmd.args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn()?;
        if let Some(input) = input {
            let result = child
                .stdin
                .take()
                .expect("stdin configurado")
                .write_all(input);
            if result.is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitError::CommandFailed("stdin"));
            }
        }
        Ok(child.wait_with_output()?)
    }

    fn text(
        &self,
        args: &[&str],
        index: Option<&Path>,
        input: Option<&[u8]>,
    ) -> Result<String, GitError> {
        let out = self.output(args, index, input)?;
        if !out.status.success() {
            // stderr pode conter URLs com credenciais: nunca retorná-lo nem registrá-lo.
            return Err(GitError::CommandFailed("solicitada"));
        }
        String::from_utf8(out.stdout)
            .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
            .map_err(|_| GitError::CommandFailed("decodificação"))
    }

    fn git_path(&self, name: &str) -> Result<PathBuf, GitError> {
        let p = PathBuf::from(self.text(&["rev-parse", "--git-path", name], None, None)?);
        Ok(if p.is_absolute() {
            p
        } else {
            self.vault_path.join(p)
        })
    }

    fn main_ready(&self) -> Result<(), GitError> {
        if self.text(&["symbolic-ref", "-q", "HEAD"], None, None)? != "refs/heads/main" {
            return Err(GitError::Unsafe(
                "o vault não está na main; nenhuma branch será trocada automaticamente",
            ));
        }
        for name in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
            "sequencer",
            "BISECT_LOG",
        ] {
            if self.git_path(name)?.exists() {
                return Err(GitError::Unsafe(
                    "há uma operação Git em andamento; conclua-a antes de sincronizar",
                ));
            }
        }
        if !self
            .text(&["ls-files", "--unmerged"], None, None)?
            .is_empty()
        {
            return Err(GitError::Unsafe(
                "há conflitos no índice Git; nenhuma nota será commitada",
            ));
        }
        Ok(())
    }

    pub fn pull(&self, branch: &str) -> Result<String, GitError> {
        if branch != "main" {
            return Err(GitError::Unsafe("somente a main pode ser sincronizada"));
        }
        self.main_ready()?;
        // Até o status pode executar um clean filter ao revalidar arquivos modificados.
        // O pull também faria checkout/smudge: exige revisão manual nesses repositórios.
        if self
            .output(&["config", "--get-regexp", "^filter\\."], None, None)?
            .status
            .success()
        {
            return Err(GitError::Unsafe(
                "o vault configura filtros Git; faça a sincronização manualmente",
            ));
        }
        if !self.status_short()?.is_empty() {
            return Err(GitError::Unsafe(
                "há edições locais ou staged no vault; o pull foi bloqueado para preservá-las",
            ));
        }
        self.text(&["fetch", "--no-tags", "origin", "main"], None, None)?;
        self.main_ready()?;
        self.text(
            &["merge", "--ff-only", "--no-edit", "FETCH_HEAD"],
            None,
            None,
        )
    }

    /// Cria uma árvore a partir de HEAD + um blob validado, sem filtros ou hooks.
    /// O índice real só recebe a entrada dessa nota; os demais blobs staged ficam preservados.
    pub(crate) fn commit_note(
        &self,
        rel: &Path,
        content: &str,
        message: &str,
    ) -> Result<NoteCommit, GitError> {
        crate::safe_path::relative(rel)
            .map_err(|_| GitError::Unsafe("caminho de nota inválido"))?;
        if !rel.starts_with(Path::new("03 Recursos").join("Orion").join("Memória"))
            || rel.extension().and_then(|s| s.to_str()) != Some("md")
        {
            return Err(GitError::Unsafe(
                "o commit só aceita notas da memória do Orion",
            ));
        }
        SecretScanner::scan(content)
            .and_then(|_| SecretScanner::scan(message))
            .map_err(|_| GitError::Unsafe("segredo detectado; commit bloqueado"))?;
        self.main_ready()?;
        let index = self.git_path("index")?;
        if std::fs::symlink_metadata(&index).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(GitError::Unsafe(
                "índice Git com link simbólico não é permitido",
            ));
        }
        let mut lock = IndexLock::acquire(&index)?;
        self.main_ready()?;
        let parent_out = self.output(&["rev-parse", "--verify", "--quiet", "HEAD"], None, None)?;
        let parent = parent_out.status.success().then(|| {
            String::from_utf8_lossy(&parent_out.stdout)
                .trim()
                .to_string()
        });
        if parent.is_some() && !index.is_file() {
            return Err(GitError::Unsafe(
                "índice Git ausente; restaure-o antes de sincronizar",
            ));
        }
        let git_path = rel
            .to_str()
            .ok_or(GitError::Unsafe("nome da nota não é UTF-8"))?
            .replace('\\', "/");
        let path = git_path.as_str();
        let staged = self.text(&["ls-files", "--stage", "--", path], None, None)?;
        let head_entry = if let Some(head) = &parent {
            self.text(&["ls-tree", head, "--", path], None, None)?
        } else {
            String::new()
        };
        // Compara modo e blob. Uma versão preparada pelo usuário nunca é substituída.
        let staged_parts = staged.split_whitespace().take(3).collect::<Vec<_>>();
        let head_parts = head_entry.split_whitespace().take(3).collect::<Vec<_>>();
        if staged_parts.first() != head_parts.first() || staged_parts.get(1) != head_parts.get(2) {
            return Err(GitError::Unsafe(
                "esta nota já tem uma versão staged; o índice do usuário foi preservado",
            ));
        }
        let work = tempfile::tempdir_in(
            index
                .parent()
                .ok_or(GitError::Unsafe("pasta Git inválida"))?,
        )?;
        let tree_index = work.path().join("tree");
        let user_index = work.path().join("user");
        if index.exists() {
            std::fs::copy(&index, &user_index)?;
        }
        if let Some(head) = &parent {
            self.text(&["read-tree", head], Some(&tree_index), None)?;
        } else {
            self.text(&["read-tree", "--empty"], Some(&tree_index), None)?;
        }
        let blob = self.text(
            &["hash-object", "-w", "--stdin", "--no-filters"],
            None,
            Some(content.as_bytes()),
        )?;
        for temp in [&tree_index, &user_index] {
            let record = format!("100644 {blob}\t{path}\0");
            self.text(
                &["update-index", "-z", "--index-info"],
                Some(temp),
                Some(record.as_bytes()),
            )?;
        }
        let tree = self.text(&["write-tree"], Some(&tree_index), None)?;
        if let Some(head) = &parent {
            if tree == self.text(&["rev-parse", &format!("{head}^{{tree}}")], None, None)? {
                return Ok(NoteCommit {
                    hash: head.clone(),
                    parent,
                    created: false,
                });
            }
        }
        let mut args = vec!["commit-tree", tree.as_str()];
        if let Some(head) = &parent {
            args.extend(["-p", head.as_str()]);
        }
        let hash = self.text(&args, None, Some(message.as_bytes()))?;
        lock.prepare(&std::fs::read(&user_index)?)?;
        self.main_ready()?;
        let empty = "0".repeat(hash.len());
        self.text(
            &[
                "update-ref",
                "refs/heads/main",
                &hash,
                parent.as_deref().unwrap_or(&empty),
            ],
            None,
            None,
        )?;
        lock.publish(&index)?;
        Ok(NoteCommit {
            hash,
            parent,
            created: true,
        })
    }

    /// Só envia o commit exato criado aqui quando seu pai já está no remoto.
    /// Commits locais anteriores ou posteriores do usuário nunca entram neste push.
    pub(crate) fn push_note(&self, commit: &NoteCommit) -> Result<String, GitError> {
        if !commit.created {
            return Ok(String::new());
        }
        let parent = commit.parent.as_deref().ok_or(GitError::Unsafe(
            "o primeiro commit exige configurar e sincronizar o remoto manualmente",
        ))?;
        let remote = self.text(
            &["ls-remote", "--refs", "origin", "refs/heads/main"],
            None,
            None,
        )?;
        if remote.split_whitespace().next() != Some(parent) {
            return Err(GitError::Unsafe("a main remota difere da base da nota; sincronize os commits pendentes antes do push"));
        }
        self.text(
            &[
                "push",
                "--no-verify",
                "origin",
                &format!("{}:refs/heads/main", commit.hash),
            ],
            None,
            None,
        )
    }

    pub fn head_commit_hash(&self) -> Result<String, GitError> {
        self.text(&["rev-parse", "HEAD"], None, None)
    }
    fn status_short(&self) -> Result<String, GitError> {
        self.text(
            &["status", "--porcelain", "--untracked-files=normal"],
            None,
            None,
        )
    }
}

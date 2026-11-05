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
    /// Mensagem montada na hora (só nomes de arquivo, nunca conteúdo nem stderr do Git).
    #[error("{0}")]
    Sync(String),
    #[error("erro de E/S ao executar Git")]
    Io(#[from] std::io::Error),
}

/// Cada comando Git morre depois disto (fetch e push incluídos).
const COMMAND_DEADLINE: std::time::Duration = std::time::Duration::from_secs(120);
static HARD_STOP: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Limite geral para todos os comandos Git a partir de agora (usado ao fechar o app); `None` tira o limite.
pub fn set_hard_stop(after: Option<std::time::Duration>) {
    *HARD_STOP.lock().unwrap_or_else(|e| e.into_inner()) = after.map(|d| std::time::Instant::now() + d);
}

/// Um escritor Git por vez dentro do processo (nota, sincronização completa e push não se misturam).
static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn writer() -> std::sync::MutexGuard<'static, ()> {
    WRITER.lock().unwrap_or_else(|e| e.into_inner())
}

/// Resultado de uma sincronização completa do vault (só contagens e nomes de arquivo).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FullSyncReport {
    /// Arquivos que entraram no commit desta rodada.
    pub committed: usize,
    /// Arquivos que ficaram só locais por conter segredo ou passar do limite de tamanho.
    pub held_back: Vec<String>,
    /// Veio coisa nova do GitHub.
    pub pulled: bool,
    /// Algo foi enviado ao GitHub.
    pub pushed: bool,
}

/// Maior arquivo que o Orion envia (o GitHub recusa mais de 100 MB).
const MAX_SYNC_FILE: u64 = 25 * 1024 * 1024;

/// Imagem, áudio e vídeo não têm texto para o detector de segredos; o resto é sempre examinado.
fn media(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "mp3", "wav", "ogg", "m4a", "flac", "mp4", "mov", "mkv", "webm"].contains(&ext.as_str())
}

/// Estado volátil do Obsidian e lixeira: nunca entram no commit.
fn volatile(rel: &str) -> bool {
    let rel = rel.trim_start_matches("./");
    rel == ".trash"
        || rel.starts_with(".trash/")
        || rel.starts_with(".git/")
        || (rel.starts_with(".obsidian/") && rel.rsplit('/').next().is_some_and(|n| n.starts_with("workspace")))
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
        let mut cmd = hardened_command();
        cmd.current_dir(&self.vault_path);
        if let Some(index) = index {
            cmd.env("GIT_INDEX_FILE", index);
            // A revalidação de entradas do índice não pode ler a árvore de trabalho real
            // nem disparar filtros configurados nos arquivos do usuário.
            cmd.env(
                "GIT_WORK_TREE",
                index.parent().expect("índice temporário com pasta"),
            );
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
        // A entrada é escrita em outra thread enquanto a saída é lida: com muita entrada e muita
        // saída (ex.: `check-attr --stdin`), escrever tudo antes de ler travaria os dois lados.
        let writer = input.map(|input| {
            let mut stdin = child.stdin.take().expect("stdin configurado");
            let data = input.to_vec();
            std::thread::spawn(move || stdin.write_all(&data).is_ok())
        });
        let mut stdout = child.stdout.take().expect("stdout configurado");
        let mut stderr = child.stderr.take().expect("stderr configurado");
        let read_out = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stdout, &mut b);
            b
        });
        let read_err = std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stderr, &mut b);
            b
        });
        // Prazo por comando e, se houver, o limite geral de encerramento: o processo que passar
        // dele (rede ou gerenciador de credenciais presos) é morto e recolhido.
        let mut limit = std::time::Instant::now() + COMMAND_DEADLINE;
        if let Some(stop) = *HARD_STOP.lock().unwrap_or_else(|e| e.into_inner()) {
            limit = limit.min(stop);
        }
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if std::time::Instant::now() >= limit {
                let _ = child.kill();
                let _ = child.wait();
                // Um subprocesso do Git pode manter os pipes abertos: as threads de leitura e
                // escrita são soltas, não esperadas (terminam sozinhas quando os pipes fecharem).
                drop((read_out, read_err, writer));
                return Err(GitError::Unsafe("o Git demorou demais e foi interrompido (rede ou login do GitHub presos?)"));
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        if let Some(w) = writer {
            // se o Git terminou antes de ler tudo, o erro de escrita é esperado e o status decide
            let _ = w.join();
        }
        Ok(std::process::Output { status, stdout: read_out.join().unwrap_or_default(), stderr: read_err.join().unwrap_or_default() })
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
        if self.uses_filters()? {
            return Err(GitError::Unsafe(
                "o vault usa filtros Git (.gitattributes); faça a sincronização manualmente",
            ));
        }
        if !self.status_short()?.is_empty() {
            return Err(GitError::Unsafe(
                "há edições locais ou staged no vault; o pull foi bloqueado para preservá-las",
            ));
        }
        self.run_explained(&["fetch", "--no-tags", "origin", "main"])?;
        self.main_ready()?;
        if self.filters_selected(Some("FETCH_HEAD"))? {
            return Err(GitError::Unsafe("o GitHub trouxe atributos de filtro Git (.gitattributes); faça a sincronização manualmente"));
        }
        self.run_explained(&["merge", "--ff-only", "--no-edit", "FETCH_HEAD"])
    }

    /// Sincronização completa: commita o que mudou no vault (menos estado volátil e arquivos com
    /// segredo), traz a `main` do GitHub e envia. Nunca usa `--force`, nunca descarta edição sua e,
    /// se os dois lados divergirem em conflito, desfaz a tentativa e avisa quais arquivos.
    /// Usa o Git do seu computador, então vale o login que você já tem (Git Credential Manager).
    /// (REGRAS.md: o vault é sincronizado pelo Orion, sem sobrescrever nada.)
    pub fn sync_all(&self) -> Result<FullSyncReport, GitError> {
        let _guard = writer();
        self.main_ready()?;
        if self.uses_filters()? {
            return Err(GitError::Unsafe("o vault usa filtros Git (.gitattributes); faça a sincronização manualmente"));
        }
        let mut report = FullSyncReport::default();

        // 1. O que mudou: status em formato estável, separado por NUL.
        let raw = self.output(&["status", "--porcelain=v1", "-z", "--untracked-files=all"], None, None)?;
        if !raw.status.success() {
            return Err(GitError::CommandFailed("status"));
        }
        let mut entries = String::from_utf8_lossy(&raw.stdout).split('\0').map(str::to_string).collect::<Vec<_>>().into_iter();
        let (mut allow, mut hold, mut noise) = (vec![], vec![], vec![]);
        while let Some(e) = entries.next() {
            if e.len() < 4 {
                continue;
            }
            let (xy, path) = (&e[..2], e[3..].to_string());
            if xy.starts_with('R') || xy.starts_with('C') {
                entries.next(); // caminho de origem da renomeação
            }
            if volatile(&path) {
                noise.push(path);
                continue;
            }
            let full = self.vault_path.join(&path);
            match std::fs::symlink_metadata(&full) {
                Ok(m) if m.file_type().is_symlink() => hold.push(path),
                Ok(m) if m.is_file() => {
                    if m.len() > MAX_SYNC_FILE {
                        hold.push(path);
                    } else if media(&path) {
                        allow.push(path);
                    } else {
                        // o que não puder ser lido e verificado não sobe (segredos são ASCII: a
                        // conversão com perda serve também para binários)
                        match std::fs::read(&full) {
                            Ok(b) if SecretScanner::scan(&String::from_utf8_lossy(&b)).is_ok() => allow.push(path),
                            _ => hold.push(path),
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => allow.push(path), // arquivo apagado: a exclusão também é sincronizada
            }
        }

        // Estado volátil do Obsidian que já está no repositório volta ao que o Git tem: o Obsidian
        // saiu do projeto e esse arquivo mudava a cada clique, bloqueando a atualização.
        let tracked_noise: Vec<&String> = noise.iter().filter(|p| p.starts_with(".obsidian/")).collect();
        if !tracked_noise.is_empty() {
            let mut input = Vec::new();
            for p in tracked_noise {
                input.extend_from_slice(p.as_bytes());
                input.push(0);
            }
            let _ = self.output(&["restore", "--source=HEAD", "--worktree", "--staged", "--pathspec-from-file=-", "--pathspec-file-nul"], None, Some(&input));
        }

        // 2. Commit.
        // Nada que ficou de fora pode seguir no índice (o commit leva o índice inteiro).
        let excluded: Vec<&String> = hold.iter().chain(noise.iter()).collect();
        if !excluded.is_empty() {
            let mut input = Vec::new();
            for p in excluded {
                input.extend_from_slice(p.as_bytes());
                input.push(0);
            }
            let _ = self.output(&["reset", "-q", "--pathspec-from-file=-", "--pathspec-file-nul"], None, Some(&input));
        }
        if !allow.is_empty() {
            let mut input = Vec::new();
            for p in &allow {
                input.extend_from_slice(p.as_bytes());
                input.push(0);
            }
            self.run_input(&["add", "-A", "--pathspec-from-file=-", "--pathspec-file-nul"], &input)?;
            if !self.output(&["diff", "--cached", "--quiet"], None, None)?.status.success() {
                let msg = format!("Orion: sincronização do vault ({} arquivos)", allow.len());
                self.run_explained(&["commit", "-q", "--no-verify", "-m", &msg])?;
                report.committed = allow.len();
            }
        }
        report.held_back = hold;

        // 3. Traz a main do GitHub e junta.
        self.run_explained(&["fetch", "--no-tags", "origin", "main"])?;
        // Os atributos que chegam do GitHub valem no checkout da junção: se pedirem um filtro, para antes.
        if self.filters_selected(Some("FETCH_HEAD"))? {
            return Err(GitError::Unsafe("o GitHub trouxe atributos de filtro Git (.gitattributes); faça a sincronização manualmente"));
        }
        let head = self.text(&["rev-parse", "HEAD"], None, None)?;
        let remote = self.text(&["rev-parse", "FETCH_HEAD"], None, None)?;
        let is_ancestor = |a: &str, b: &str| -> Result<bool, GitError> {
            Ok(self.output(&["merge-base", "--is-ancestor", a, b], None, None)?.status.success())
        };
        if head != remote {
            if is_ancestor(&remote, &head)? {
                // o GitHub está atrás: só falta enviar
            } else if is_ancestor(&head, &remote)? {
                self.run_explained(&["merge", "--ff-only", "--no-edit", "FETCH_HEAD"])?;
                report.pulled = true;
            } else {
                if self.merge_drivers_selected()? {
                    return Err(GitError::Unsafe("o vault e o GitHub divergiram e há driver de merge externo configurado; resolva no Git"));
                }
                let out = self.output(&["merge", "--no-edit", "-m", "Orion: junta o vault com o GitHub", "FETCH_HEAD"], None, None)?;
                if !out.status.success() {
                    let conflicted = self.text(&["diff", "--name-only", "--diff-filter=U"], None, None).unwrap_or_default();
                    let _ = self.output(&["merge", "--abort"], None, None);
                    let files: Vec<&str> = conflicted.lines().take(8).collect();
                    return Err(GitError::Sync(if files.is_empty() {
                        explain_failure(&String::from_utf8_lossy(&out.stderr)).to_string()
                    } else {
                        format!("o vault e o GitHub mudaram os mesmos arquivos; nada foi sobrescrito. Resolva no Git: {}", files.join(", "))
                    }));
                }
                report.pulled = true;
            }
        }

        // 4. Envia (sem force).
        let head = self.text(&["rev-parse", "HEAD"], None, None)?;
        if head != remote {
            self.run_explained(&["push", "--no-verify", "origin", "main:refs/heads/main"])?;
            report.pushed = true;
        }
        Ok(report)
    }

    fn run_input(&self, args: &[&str], input: &[u8]) -> Result<(), GitError> {
        let out = self.output(args, None, Some(input))?;
        if out.status.success() { Ok(()) } else { Err(GitError::Sync(explain_failure(&String::from_utf8_lossy(&out.stderr)).to_string())) }
    }

    /// O vault só é recusado se os `.gitattributes` dele pedem um filtro (`filter=`). Filtro apenas
    /// configurado no Git do computador (o Git for Windows traz `filter.lfs`) não é usado por ele.
    fn uses_filters(&self) -> Result<bool, GitError> {
        self.filters_selected(None)
    }

    /// Algum arquivo escolhe um filtro? `tree = None` olha a árvore de trabalho e o índice; com
    /// `Some(commit)`, olha os atributos daquele commit (o que viria no checkout de um merge).
    fn filters_selected(&self, tree: Option<&str>) -> Result<bool, GitError> {
        // Sem nenhum filtro configurado (nem no sistema), não há o que executar.
        if !self.output(&["config", "--get-regexp", "^filter\\."], None, None)?.status.success() {
            return Ok(false);
        }
        self.attribute_values(tree, "filter", None)
    }

    /// A junção com divergência executaria driver de merge externo? Vale `merge.<nome>.driver`
    /// escolhido por algum arquivo (no vault ou no commit recebido) e também `merge.default`.
    fn merge_drivers_selected(&self) -> Result<bool, GitError> {
        let cfg = self.output(&["config", "--get-regexp", "^merge\\..*\\.driver$"], None, None)?;
        let text = String::from_utf8_lossy(&cfg.stdout);
        let names: Vec<String> = text
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .filter_map(|k| k.strip_prefix("merge.").and_then(|k| k.strip_suffix(".driver")))
            .map(str::to_string)
            .collect();
        let default = self.output(&["config", "--get", "merge.default"], None, None)?;
        if names.is_empty() && !default.status.success() {
            return Ok(false);
        }
        if default.status.success() {
            return Ok(true); // o padrão mudou: qualquer arquivo sem atributo usaria outro merge
        }
        Ok(self.attribute_values(None, "merge", Some(&names))? || self.attribute_values(Some("FETCH_HEAD"), "merge", Some(&names))?)
    }

    /// Algum arquivo tem o atributo `attr` com valor definido? Com `names`, só conta valor igual a
    /// um deles. `tree = None` olha a árvore de trabalho e o índice; `Some(commit)`, aquele commit.
    fn attribute_values(&self, tree: Option<&str>, attr: &str, names: Option<&[String]>) -> Result<bool, GitError> {
        let listed = match tree {
            None => self.output(&["ls-files", "-z", "--cached", "--others", "--exclude-standard"], None, None)?,
            Some(t) => self.output(&["ls-tree", "-r", "-z", "--name-only", t], None, None)?,
        };
        if !listed.status.success() {
            return Err(GitError::CommandFailed("listagem de arquivos"));
        }
        if listed.stdout.is_empty() {
            return Ok(false);
        }
        let mut args = vec!["check-attr", "-z", "--stdin"];
        let source;
        if let Some(t) = tree {
            source = format!("--source={t}");
            args.push(&source);
        }
        args.push(attr);
        let attrs = self.output(&args, None, Some(&listed.stdout))?;
        if !attrs.status.success() {
            return Err(GitError::CommandFailed("check-attr"));
        }
        // saída: caminho\0atributo\0valor\0 ...
        let text = String::from_utf8_lossy(&attrs.stdout);
        let parts: Vec<&str> = text.split('\0').collect();
        Ok(parts.chunks(3).any(|c| c.len() == 3 && match names {
            None => c[2] != "unspecified",
            Some(n) => n.iter().any(|x| x == c[2]),
        }))
    }

    /// Como `text`, mas devolve um motivo fixo e legível quando o Git recusa. O stderr nunca sai daqui
    /// (pode conter URL com credencial): só é lido para escolher entre mensagens prontas.
    fn run_explained(&self, args: &[&str]) -> Result<String, GitError> {
        let out = self.output(args, None, None)?;
        if !out.status.success() {
            return Err(GitError::Unsafe(explain_failure(&String::from_utf8_lossy(&out.stderr))));
        }
        String::from_utf8(out.stdout)
            .map(|s| s.trim_end_matches(['\r', '\n']).to_string())
            .map_err(|_| GitError::CommandFailed("decodificação"))
    }

    /// Cria uma árvore a partir de HEAD + um blob validado, sem filtros ou hooks.
    /// O índice real só recebe a entrada dessa nota; os demais blobs staged ficam preservados.
    pub(crate) fn commit_note(
        &self,
        rel: &Path,
        content: &str,
        message: &str,
    ) -> Result<NoteCommit, GitError> {
        let _guard = writer();
        crate::safe_path::relative(rel)
            .map_err(|_| GitError::Unsafe("caminho de nota inválido"))?;
        if !(rel.starts_with(Path::new("03 Recursos").join("Orion").join("Memória")) || is_project_note(rel))
            || rel.extension().and_then(|s| s.to_str()) != Some("md")
        {
            return Err(GitError::Unsafe(
                "o commit só aceita notas da memória e notas raiz de projeto do Orion",
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
        let _guard = writer();
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

/// `01 Projetos/<Nome>/<Nome>.md`: a única nota fora da memória que o Orion commita.
fn is_project_note(rel: &Path) -> bool {
    let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    matches!(parts.as_slice(), [root, dir, file] if root == "01 Projetos" && *file == format!("{dir}.md"))
}

/// Git sem shell, sem hooks, sem fsmonitor e só com o ambiente mínimo (cofre do SO, certificados, proxy).
/// Motivo fixo para uma falha de fetch/merge. Só reconhece padrões conhecidos do Git.
fn explain_failure(stderr: &str) -> &'static str {
    let e = stderr.to_lowercase();
    if e.contains("not possible to fast-forward") || e.contains("diverging branches") || e.contains("refusing to merge") {
        "o vault local e o GitHub divergiram (há commits dos dois lados); o pull só avança em linha reta. Resolva no Git (merge ou rebase) e tente de novo"
    } else if e.contains("authentication failed") || e.contains("could not read username") || e.contains("could not read password")
        || e.contains("invalid credentials") || e.contains("permission denied") || e.contains("403") || e.contains("terminal prompts disabled")
    {
        "o GitHub recusou o acesso ao vault: faça login no Git deste computador (Git Credential Manager ou token) e tente de novo"
    } else if e.contains("repository not found") || e.contains("404") || e.contains("does not appear to be a git repository") {
        "o repositório do vault não foi encontrado no GitHub; confira o remoto `origin`"
    } else if e.contains("could not resolve host") || e.contains("unable to access") || e.contains("timed out") || e.contains("connection") {
        "sem conexão com o GitHub; confira a internet, o proxy e o certificado e tente de novo"
    } else if e.contains("couldn't find remote ref") || e.contains("no such remote") || e.contains("unknown revision") {
        "o remoto `origin` não tem a branch main"
    } else if e.contains("non-fast-forward") || e.contains("fetch first") || e.contains("[rejected]") || e.contains("failed to push") && e.contains("updates were rejected") {
        "o GitHub recebeu commits novos enquanto o Orion enviava; a próxima sincronização tenta de novo"
    } else if e.contains("tell me who you are") || e.contains("empty ident") || e.contains("unable to auto-detect email") {
        "o Git deste computador não sabe quem você é; rode `git config --global user.name` e `user.email` e tente de novo"
    } else if e.contains("would be overwritten") || e.contains("local changes") {
        "há arquivos locais no caminho do pull; salve ou mova as edições do vault e tente de novo"
    } else if e.contains("index.lock") || e.contains("unable to create") {
        "outro processo está usando o repositório do vault (índice Git bloqueado); feche o Git ou outro programa que use o vault e tente de novo"
    } else {
        "o Git recusou o pull do vault; rode `git pull --ff-only` na pasta do vault para ver o motivo exato"
    }
}

fn hardened_command() -> Command {
    let mut cmd = Command::new("git");
    cmd.env_clear();
    for key in [
        "PATH", "HOME", "USERPROFILE", "SystemRoot", "WINDIR", "APPDATA", "LOCALAPPDATA", "TEMP", "TMP", "TMPDIR",
        "SSH_AUTH_SOCK", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY", "http_proxy", "https_proxy", "all_proxy",
        "no_proxy", "SSL_CERT_FILE", "SSL_CERT_DIR", "GIT_SSL_CAINFO",
        // Windows: o Git Credential Manager e o gitconfig do sistema precisam destas para achar o login salvo.
        "ProgramData", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "SystemDrive", "USERNAME", "USERDOMAIN", "COMPUTERNAME",
        "HOMEDRIVE", "HOMEPATH", "XDG_CONFIG_HOME",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0").env("GIT_LITERAL_PATHSPECS", "1");
    cmd.args(["-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false", "-c", "core.quotePath=false"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd
}

/// Repositório público do GitHub aceito para clonar: `https://github.com/<dono>/<repo>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRepo {
    pub owner: String,
    pub name: String,
}

impl GithubRepo {
    /// Só https, só github.com, sem usuário, porta, consulta ou fragmento: a URL vira dado validado,
    /// nunca argumento livre de comando (REGRAS.md: o que o modelo escreve nunca vira comando).
    pub fn parse(url: &str) -> Result<Self, GitError> {
        let bad = GitError::Unsafe("URL do repositório inválida; use https://github.com/<dono>/<repo>");
        let rest = url.trim().strip_prefix("https://github.com/").ok_or(bad)?;
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let rest = rest.strip_suffix(".git").unwrap_or(rest);
        let (owner, name) = rest
            .split_once('/')
            .ok_or(GitError::Unsafe("URL do repositório inválida; use https://github.com/<dono>/<repo>"))?;
        let owner_ok = (1..=39).contains(&owner.len())
            && !owner.starts_with('-')
            && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        let name_ok = (1..=100).contains(&name.len())
            && !name.starts_with(['-', '.'])
            && name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
        if !owner_ok || !name_ok {
            return Err(GitError::Unsafe("URL do repositório inválida; use https://github.com/<dono>/<repo>"));
        }
        Ok(GithubRepo { owner: owner.into(), name: name.into() })
    }

    pub fn https_url(&self) -> String {
        format!("https://github.com/{}/{}.git", self.owner, self.name)
    }
}

/// Clona um repositório público do GitHub em `dest` (que não pode existir): raso, uma branch, sem
/// submódulos, sem hooks, sem credenciais e sem LFS. Tempo limite de 5 minutos.
/// Repositório privado falha, de propósito: o Orion não entrega credencial ao Git.
pub fn clone_public_repo(repo: &GithubRepo, dest: &Path) -> Result<(), GitError> {
    if dest.exists() {
        return Err(GitError::Unsafe("a pasta de destino do clone já existe"));
    }
    let mut cmd = hardened_command();
    cmd.args([
        "-c", "credential.helper=", "-c", "protocol.allow=never", "-c", "protocol.https.allow=always",
        "-c", "core.symlinks=false", "-c", "filter.lfs.smudge=", "-c", "filter.lfs.process=", "-c", "filter.lfs.required=false",
    ])
    .args(["clone", "--depth", "1", "--single-branch", "--no-tags", "--no-recurse-submodules", "--"])
    .arg(repo.https_url())
    .arg(dest)
    .env("GIT_LFS_SKIP_SMUDGE", "1")
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    let mut child = cmd.spawn()?;
    let limit = std::time::Instant::now() + std::time::Duration::from_secs(300);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if std::time::Instant::now() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_dir_all(dest);
            return Err(GitError::CommandFailed("clone (tempo esgotado)"));
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    };
    if !status.success() {
        let _ = std::fs::remove_dir_all(dest);
        // stderr pode conter URLs com credenciais: nunca retorná-lo nem registrá-lo.
        return Err(GitError::CommandFailed("clone"));
    }
    Ok(())
}

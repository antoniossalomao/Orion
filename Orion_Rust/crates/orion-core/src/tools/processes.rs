//! Processos em segundo plano do `orion-desktop`: `iniciar_processo_bg`, `status_processo_bg`,
//! `listar_processos_bg` e a vigilância de pastas.
//!
//! `iniciar_processo_bg` é execução: a política classifica o comando como `executar_comando`. O
//! processo roda com o ambiente limpo, sem entrada padrão, com a saída num log só do dono, e
//! sobrevive ao Orion. A lista vive na memória: reiniciar esquece a lista, não o processo.

use super::desktop::argv_do_shell;
use super::registry::{obj, Tool};
use crate::util::{agora, env_limpo, home};
use orion_store::ops::Operations;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, LazyLock};

pub const MAX_ATIVOS: usize = 5;
/// Bytes do fim do log que `status` lê.
pub const MAX_LOG_LIDO: u64 = 16_000;
static NAO_SEGURO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^A-Za-z0-9_-]").unwrap());

pub struct Proc {
    pub id: i64,
    pub nome: String,
    pub comando: String,
    pub pid: u32,
    pub log: PathBuf,
    pub iniciado: f64,
    filho: Child,
    pub notificar: bool,
    pub notificado: bool,
    pub codigo: Option<i32>,
    terminou: bool,
}

impl Proc {
    fn atualizar(&mut self) -> Option<Option<i32>> {
        if !self.terminou {
            if let Ok(Some(st)) = self.filho.try_wait() {
                self.terminou = true;
                self.codigo = st.code();
            }
        }
        self.terminou.then_some(self.codigo)
    }
}

fn cauda(caminho: &Path, linhas: usize) -> String {
    let Ok(mut f) = std::fs::File::open(caminho) else { return String::new() };
    let tamanho = f.seek(SeekFrom::End(0)).unwrap_or(0);
    let _ = f.seek(SeekFrom::Start(tamanho.saturating_sub(MAX_LOG_LIDO)));
    let mut b = vec![];
    let _ = f.read_to_end(&mut b);
    let texto = String::from_utf8_lossy(&b).into_owned();
    let v: Vec<&str> = texto.lines().collect();
    v[v.len().saturating_sub(linhas.max(1))..].join("\n")
}

pub struct ProcessManager {
    dir: PathBuf,
    max: usize,
    keep_s: f64,
    procs: Mutex<(i64, Vec<Proc>)>,
}

impl ProcessManager {
    pub fn new(log_dir: PathBuf) -> Self {
        ProcessManager { dir: log_dir, max: MAX_ATIVOS, keep_s: 7.0 * 86400.0, procs: Mutex::new((0, vec![])) }
    }

    pub fn start(&self, nome: &str, comando: &str, notificar: bool) -> Value {
        let nome: String = nome.trim().chars().take(60).collect();
        let nome = if nome.is_empty() { "processo".to_string() } else { nome };
        let Some(argv) = argv_do_shell(comando, cfg!(windows)) else {
            return json!({"erro": "nenhum shell encontrado (PowerShell no Windows, sh no macOS/Linux)"});
        };
        let id = {
            let mut g = self.procs.lock();
            let ativos = g.1.iter_mut().map(|p| p.atualizar().is_none()).filter(|x| *x).count();
            if ativos >= self.max {
                return json!({"erro": format!("já há {} processos em segundo plano rodando", self.max)});
            }
            g.0 += 1;
            g.0
        };
        let _ = std::fs::create_dir_all(&self.dir);
        self.podar_logs();
        let seguro: String = NAO_SEGURO.replace_all(&nome, "_").chars().take(40).collect();
        let log = self.dir.join(format!("{}_{id}_{seguro}.log", agora() as i64));
        let saida = match std::fs::File::create(&log) {
            Ok(f) => f,
            Err(e) => return json!({"erro": format!("não foi possível iniciar: {:?}", e.kind())}),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o600));
        }
        let erro = match saida.try_clone() {
            Ok(f) => f,
            Err(e) => return json!({"erro": format!("não foi possível iniciar: {:?}", e.kind())}),
        };
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]).current_dir(home()).env_clear().envs(env_limpo()).stdin(Stdio::null()).stdout(saida).stderr(erro);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let filho = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => return json!({"erro": format!("não foi possível iniciar: {:?}", e.kind())}),
        };
        let mut p = Proc {
            id,
            nome,
            comando: comando.into(),
            pid: filho.id(),
            log,
            iniciado: agora(),
            filho,
            notificar,
            notificado: false,
            codigo: None,
            terminou: false,
        };
        let resumo = Self::resumo(&mut p);
        self.procs.lock().1.push(p);
        json!({"ok": true, "processo": resumo})
    }

    fn resumo(p: &mut Proc) -> Value {
        let codigo = p.atualizar();
        let status = match codigo {
            None => "rodando",
            Some(Some(0)) => "concluido",
            Some(_) => "erro",
        };
        json!({"id": p.id, "nome": p.nome, "pid": p.pid, "status": status, "codigo": codigo.flatten(),
               "iniciado": p.iniciado, "comando": orion_policy::redact(&Value::String(p.comando.clone()), 300)})
    }

    pub fn status(&self, id: i64, linhas_log: i64) -> Value {
        let mut g = self.procs.lock();
        let Some(p) = g.1.iter_mut().find(|p| p.id == id) else {
            return json!({"erro": format!("processo {id} desconhecido (a lista zera ao reiniciar o Orion)")});
        };
        let mut r = Self::resumo(p);
        r["ok"] = json!(true);
        r["log"] = orion_policy::redact(&Value::String(cauda(&p.log, linhas_log.min(200) as usize)), MAX_LOG_LIDO as usize);
        r["arquivo_log"] = json!(p.log.display().to_string());
        r
    }

    pub fn listar(&self, somente_ativos: bool) -> Value {
        let mut g = self.procs.lock();
        let lista: Vec<Value> = g
            .1
            .iter_mut()
            .map(Self::resumo)
            .filter(|p| !somente_ativos || p["status"] == "rodando")
            .collect();
        json!({"ok": true, "total": lista.len(), "processos": lista})
    }

    /// Processos que terminaram desde a última chamada e pediram aviso (uma vez só):
    /// (nome, código).
    pub fn finished(&self) -> Vec<(String, Option<i32>)> {
        let mut g = self.procs.lock();
        let mut out = vec![];
        for p in g.1.iter_mut() {
            if p.atualizar().is_some() && !p.notificado {
                p.notificado = true;
                if p.notificar {
                    out.push((p.nome.clone(), p.codigo));
                }
            }
        }
        out
    }

    fn podar_logs(&self) {
        let corte = agora() - self.keep_s;
        if let Ok(d) = std::fs::read_dir(&self.dir) {
            for e in d.flatten() {
                let p = e.path();
                let velho = std::fs::metadata(&p)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .is_some_and(|d| d.as_secs_f64() < corte);
                if p.extension().is_some_and(|x| x == "log") && velho {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
    }
}

pub fn process_tools(manager: Arc<ProcessManager>, ops: Arc<Operations>) -> Vec<Tool> {
    let (m1, m2, m3) = (manager.clone(), manager.clone(), manager);
    let (o1, o2, o3) = (ops.clone(), ops.clone(), ops);
    vec![
        Tool::sync(
            "iniciar_processo_bg",
            "Roda um comando demorado em segundo plano (a saída vai para um log). Pede confirmação como executar_comando; avisa quando terminar.",
            obj(json!({"nome": {"type": "string"}, "comando": {"type": "string"}, "notificar_ao_concluir": {"type": "boolean"}}), &["nome", "comando"]),
            move |a| Ok(m1.start(&a.s("nome")?, &a.s("comando")?, a.b_or("notificar_ao_concluir", true))),
        ),
        Tool::sync(
            "status_processo_bg",
            "Estado de um processo em segundo plano e o fim do log.",
            obj(json!({"processo_id": {"type": "integer"}, "linhas_log": {"type": "integer"}}), &["processo_id"]),
            move |a| Ok(m2.status(a.i("processo_id")?, a.i_or("linhas_log", 30)?)),
        ),
        Tool::sync(
            "listar_processos_bg",
            "Lista os processos em segundo plano (por padrão só os que ainda rodam).",
            json!({"type": "object", "properties": {"somente_ativos": {"type": "boolean"}}}),
            move |a| Ok(m3.listar(a.b_or("somente_ativos", true))),
        ),
        Tool::sync(
            "iniciar_vigilancia_pasta",
            "Passa a avisar quando aparecerem arquivos novos numa pasta (checagem a cada poucos segundos; só os nomes).",
            obj(json!({"pasta": {"type": "string"}}), &["pasta"]),
            move |a| {
                Ok(match o1.watch_add(&a.s("pasta")?) {
                    Ok(v) => json!({"ok": true, "vigilancia": v}),
                    Err(e) => json!({"erro": e.to_string(), "ok": false}),
                })
            },
        ),
        Tool::sync(
            "parar_vigilancia_pasta",
            "Para de vigiar uma pasta.",
            obj(json!({"pasta": {"type": "string"}}), &["pasta"]),
            move |a| {
                let pasta = a.s("pasta")?;
                Ok(json!({"ok": o2.watch_remove(&pasta)?, "pasta": pasta}))
            },
        ),
        Tool::sync("listar_vigilancias", "Lista as pastas vigiadas.", json!({"type": "object", "properties": {}}), move |_| {
            let l = o3.watch_list()?;
            Ok(json!({"ok": true, "total": l.len(), "vigilancias": l}))
        }),
    ]
}

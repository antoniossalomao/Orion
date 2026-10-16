//! Mídia e janelas do `orion-desktop`: `controlar_midia` e `controlar_janela`.
//!
//! `controlar_janela` é execução e confirma sempre (REGRAS.md: título de janela nunca vira
//! comando). O título vindo do modelo nunca entra numa linha de comando: no Windows vai por
//! variável de ambiente; no Linux a janela é achada aqui e o `wmctrl` recebe só o id. Uma busca
//! que acha mais de uma janela não age.

use super::registry::{obj, Tool};
use crate::util::{env_limpo, rodar, which, ErroProcesso};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::Duration;

pub const ACOES_MIDIA: [&str; 6] = ["tocar_pausar", "proxima", "anterior", "volume_mais", "volume_menos", "mudo"];
pub const ACOES_JANELA: [&str; 6] = ["listar", "focar", "minimizar", "maximizar", "restaurar", "fechar"];
static ID_JANELA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^0x[0-9a-fA-F]+$").unwrap());

const PS_MIDIA: &str = "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class OrionTecla { [DllImport(\"user32.dll\")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra); }';$vk = [byte][int]$env:MIDIA_VK; $n = [int]$env:MIDIA_VEZES;1..$n | ForEach-Object {[OrionTecla]::keybd_event($vk, 0, 0, [UIntPtr]::Zero);[OrionTecla]::keybd_event($vk, 0, 2, [UIntPtr]::Zero)}";
const PS_JANELA: &str = "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class OrionJanela { [DllImport(\"user32.dll\")] public static extern bool ShowWindow(IntPtr h, int c); [DllImport(\"user32.dll\")] public static extern bool SetForegroundWindow(IntPtr h); }';$acao = $env:JANELA_ACAO; $alvo = $env:JANELA_TITULO;$todas = @(Get-Process | Where-Object { $_.MainWindowTitle });if ($acao -eq 'listar') { $todas | ForEach-Object { '{0}|{1}' -f $_.ProcessName, $_.MainWindowTitle }; exit 0 };$achadas = @($todas | Where-Object { $_.MainWindowTitle.IndexOf($alvo, [StringComparison]::OrdinalIgnoreCase) -ge 0 });if ($achadas.Count -eq 0) { [Console]::Error.WriteLine('janela não encontrada'); exit 2 };if ($achadas.Count -gt 1) { [Console]::Error.WriteLine('mais de uma janela: seja específico');$achadas | ForEach-Object { '{0}|{1}' -f $_.ProcessName, $_.MainWindowTitle }; exit 3 };$p = $achadas[0]; $h = $p.MainWindowHandle;if ($acao -eq 'focar') { [OrionJanela]::ShowWindow($h, 9) | Out-Null; [OrionJanela]::SetForegroundWindow($h) | Out-Null }elseif ($acao -eq 'minimizar') { [OrionJanela]::ShowWindow($h, 6) | Out-Null }elseif ($acao -eq 'maximizar') { [OrionJanela]::ShowWindow($h, 3) | Out-Null }elseif ($acao -eq 'restaurar') { [OrionJanela]::ShowWindow($h, 9) | Out-Null }elseif ($acao -eq 'fechar') { $p.CloseMainWindow() | Out-Null };'{0}|{1}' -f $p.ProcessName, $p.MainWindowTitle";

fn vk(acao: &str) -> u8 {
    match acao {
        "tocar_pausar" => 179,
        "proxima" => 176,
        "anterior" => 177,
        "volume_mais" => 175,
        "volume_menos" => 174,
        _ => 173,
    }
}

fn mac_apps(verbo: &str) -> String {
    format!("tell application \"System Events\" to set apps to name of processes\nif apps contains \"Spotify\" then\n tell application \"Spotify\" to {verbo}\nelse if apps contains \"Music\" then\n tell application \"Music\" to {verbo}\nend if")
}

fn ps(script: &str, extra: &[(&str, String)]) -> Option<(Vec<String>, HashMap<String, String>)> {
    let exe = which("pwsh").or_else(|| which("powershell"))?.to_string_lossy().into_owned();
    let mut env = env_limpo();
    for (k, v) in extra {
        env.insert(k.to_string(), v.clone());
    }
    Some((vec![exe, "-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), script.into()], env))
}

fn v(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|s| s.to_string()).collect()
}

/// Comando que executa `acao` de mídia; `None` se o sistema não tem como.
pub fn argv_midia(acao: &str) -> Option<(Vec<String>, HashMap<String, String>)> {
    if cfg!(windows) {
        let vezes = if acao.starts_with("volume") { 5 } else { 1 }; // cada tecla vale 2%
        return ps(PS_MIDIA, &[("MIDIA_VK", vk(acao).to_string()), ("MIDIA_VEZES", vezes.to_string())]);
    }
    if cfg!(target_os = "macos") {
        let script = match acao {
            "tocar_pausar" => mac_apps("playpause"),
            "proxima" => mac_apps("next track"),
            "anterior" => mac_apps("previous track"),
            "volume_mais" => "set volume output volume ((output volume of (get volume settings)) + 10)".into(),
            "volume_menos" => "set volume output volume ((output volume of (get volume settings)) - 10)".into(),
            _ => "set volume output muted (not (output muted of (get volume settings)))".into(),
        };
        return Some((vec!["osascript".into(), "-e".into(), script], env_limpo()));
    }
    if matches!(acao, "tocar_pausar" | "proxima" | "anterior") {
        which("playerctl")?;
        let verbo = match acao {
            "tocar_pausar" => "play-pause",
            "proxima" => "next",
            _ => "previous",
        };
        return Some((v(&["playerctl", verbo]), env_limpo()));
    }
    if which("wpctl").is_some() {
        let s = "@DEFAULT_AUDIO_SINK@";
        let por = match acao {
            "volume_mais" => v(&["wpctl", "set-volume", s, "5%+"]),
            "volume_menos" => v(&["wpctl", "set-volume", s, "5%-"]),
            _ => v(&["wpctl", "set-mute", s, "toggle"]),
        };
        return Some((por, env_limpo()));
    }
    if which("pactl").is_some() {
        let por = match acao {
            "volume_mais" => v(&["pactl", "set-sink-volume", "@DEFAULT_SINK@", "+5%"]),
            "volume_menos" => v(&["pactl", "set-sink-volume", "@DEFAULT_SINK@", "-5%"]),
            _ => v(&["pactl", "set-sink-mute", "@DEFAULT_SINK@", "toggle"]),
        };
        return Some((por, env_limpo()));
    }
    if which("amixer").is_some() {
        let por = match acao {
            "volume_mais" => "5%+",
            "volume_menos" => "5%-",
            _ => "toggle",
        };
        return Some((v(&["amixer", "-q", "sset", "Master", por]), env_limpo()));
    }
    None
}

fn texto(b: &[u8]) -> String {
    String::from_utf8_lossy(b).trim().to_string()
}

fn exec(argv: &[String], env: HashMap<String, String>, t: Duration) -> Result<crate::util::Saida, String> {
    rodar(argv, None, t, Some(env)).map_err(|e| match e {
        ErroProcesso::Tempo => "falhou: TimeoutExpired".into(),
        ErroProcesso::Io(e) => format!("falhou: {:?}", e.kind()),
    })
}

fn janela_linux(acao: &str, titulo: &str, t: Duration) -> Value {
    if which("wmctrl").is_none() {
        return json!({"ok": false, "erro": "wmctrl não encontrado (só funciona em sessão X11)"});
    }
    let r = match exec(&v(&["wmctrl", "-l"]), env_limpo(), t) {
        Ok(r) => r,
        Err(e) => return json!({"ok": false, "erro": e}),
    };
    if r.codigo != Some(0) {
        return json!({"ok": false, "erro": format!("wmctrl falhou (precisa de sessão X11): {}", texto(&r.stderr).chars().take(150).collect::<String>())});
    }
    let janelas: Vec<(String, String)> = texto(&r.stdout)
        .lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.splitn(4, char::is_whitespace).filter(|x| !x.is_empty()).collect();
            let partes: Vec<&str> = l.split_whitespace().collect();
            if partes.len() >= 4 && ID_JANELA.is_match(partes[0]) {
                let resto = l.splitn(4, ' ').filter(|x| !x.is_empty()).collect::<Vec<_>>();
                let titulo = resto.get(3).map(|s| s.to_string()).unwrap_or_else(|| p.get(3).map(|s| s.to_string()).unwrap_or_default());
                Some((partes[0].to_string(), titulo))
            } else {
                None
            }
        })
        .collect();
    if acao == "listar" {
        return json!({"ok": true, "janelas": janelas.iter().take(100).map(|j| &j.1).collect::<Vec<_>>()});
    }
    let alvo = titulo.to_lowercase();
    let achadas: Vec<&(String, String)> = janelas.iter().filter(|(_, t)| t.to_lowercase().contains(&alvo)).collect();
    if achadas.is_empty() {
        return json!({"ok": false, "erro": "janela não encontrada"});
    }
    if achadas.len() > 1 {
        return json!({"ok": false, "erro": "mais de uma janela combina: seja mais específico", "janelas": achadas.iter().take(20).map(|j| &j.1).collect::<Vec<_>>()});
    }
    let (id, nome) = achadas[0];
    let argv = match acao {
        "focar" => v(&["wmctrl", "-i", "-a", id]),
        "fechar" => v(&["wmctrl", "-i", "-c", id]),
        "minimizar" => v(&["wmctrl", "-i", "-r", id, "-b", "add,hidden"]),
        "maximizar" => v(&["wmctrl", "-i", "-r", id, "-b", "add,maximized_vert,maximized_horz"]),
        _ => v(&["wmctrl", "-i", "-r", id, "-b", "remove,hidden,maximized_vert,maximized_horz"]),
    };
    match exec(&argv, env_limpo(), t) {
        Ok(r) => json!({"ok": r.codigo == Some(0), "acao": acao, "janela": nome}),
        Err(e) => json!({"ok": false, "erro": e}),
    }
}

fn janela_windows(acao: &str, titulo: &str, t: Duration) -> Value {
    let Some((argv, env)) = ps(PS_JANELA, &[("JANELA_ACAO", acao.into()), ("JANELA_TITULO", titulo.into())]) else {
        return json!({"ok": false, "erro": "PowerShell não encontrado"});
    };
    let r = match exec(&argv, env, t) {
        Ok(r) => r,
        Err(e) => return json!({"ok": false, "erro": e}),
    };
    let linhas: Vec<String> = texto(&r.stdout).lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
    if acao == "listar" {
        return json!({"ok": r.codigo == Some(0), "janelas": linhas.iter().take(100).collect::<Vec<_>>()});
    }
    if r.codigo != Some(0) {
        let e: String = texto(&r.stderr).chars().take(200).collect();
        let mut out = json!({"ok": false, "erro": if e.is_empty() { "falhou".to_string() } else { e }});
        if !linhas.is_empty() {
            out["janelas"] = json!(linhas.iter().take(20).collect::<Vec<_>>());
        }
        return out;
    }
    json!({"ok": true, "acao": acao, "janela": linhas.first().cloned().unwrap_or_default()})
}

pub fn media_tools(timeout_s: f64) -> Vec<Tool> {
    let t = Duration::from_secs_f64(timeout_s);
    vec![
        Tool::sync(
            "controlar_midia",
            "Controla o som do computador: tocar_pausar, proxima, anterior, volume_mais, volume_menos ou mudo.",
            obj(json!({"acao": {"type": "string", "enum": ACOES_MIDIA}}), &["acao"]),
            move |a| {
                let acao = a.s("acao")?;
                if !ACOES_MIDIA.contains(&acao.as_str()) {
                    return Ok(json!({"ok": false, "erro": format!("ação inválida: {acao}. Use: {}", ACOES_MIDIA.join(", "))}));
                }
                let Some((argv, env)) = argv_midia(&acao) else {
                    return Ok(json!({"ok": false, "erro": "nenhum comando de mídia (no Linux instale playerctl e wpctl/pactl)"}));
                };
                Ok(match exec(&argv, env, t) {
                    Err(e) => json!({"ok": false, "erro": e}),
                    Ok(r) => {
                        let e: String = texto(&r.stderr).chars().take(200).collect();
                        json!({"ok": r.codigo == Some(0), "acao": acao, "erro": if e.is_empty() { Value::Null } else { json!(e) }})
                    }
                })
            },
        ),
        Tool::sync(
            "controlar_janela",
            "Lista, foca, minimiza, maximiza, restaura ou fecha uma janela pelo título. Fechar pode perder trabalho não salvo: pede confirmação. Os títulos são externos.",
            obj(json!({"acao": {"type": "string", "enum": ACOES_JANELA}, "titulo": {"type": "string", "description": "parte do título da janela"}}), &["acao"]),
            move |a| {
                let acao = a.s("acao")?;
                if !ACOES_JANELA.contains(&acao.as_str()) {
                    return Ok(json!({"ok": false, "erro": format!("ação inválida: {acao}. Use: {}", ACOES_JANELA.join(", "))}));
                }
                let titulo = a.s_or("titulo", "").trim().to_string();
                if acao != "listar" && titulo.is_empty() {
                    return Ok(json!({"ok": false, "erro": "informe parte do título da janela"}));
                }
                if titulo.chars().count() > 200 || titulo.contains('\0') {
                    return Ok(json!({"ok": false, "erro": "título inválido"}));
                }
                if cfg!(windows) {
                    return Ok(janela_windows(&acao, &titulo, t));
                }
                if cfg!(target_os = "macos") {
                    return Ok(json!({"ok": false, "erro": "controlar_janela não existe no macOS (exigiria Acessibilidade); para trazer um programa à frente use abrir_app"}));
                }
                Ok(janela_linux(&acao, &titulo, t))
            },
        ),
    ]
}

//! Presença e bloqueio ao levantar (E11.9; REGRAS.md: a presença nunca dispara ação destrutiva).
//!
//! Combina ociosidade de teclado/mouse, Bluetooth do celular e (quando ativo) o rosto na webcam.
//! Só pausa/retoma captura de tela e avisos; bloquear a sessão do SO exige `ORION_PRESENCE_LOCK=true`
//! e nunca destrava sozinho.

use crate::screen_memory::tela_bloqueada;
use crate::util::{env_limpo, rodar, which};
use parking_lot::Mutex;
use regex::Regex;
use std::sync::LazyLock;
use std::time::Duration;

const PS_OCIOSO: &str = "Add-Type @\"\nusing System; using System.Runtime.InteropServices;\npublic class I {\n  [StructLayout(LayoutKind.Sequential)] public struct L { public uint cb; public uint dw; }\n  [DllImport(\"user32.dll\")] public static extern bool GetLastInputInfo(ref L l); }\n\"@;\n$l = New-Object I+L; $l.cb = [Runtime.InteropServices.Marshal]::SizeOf($l);[void][I]::GetLastInputInfo([ref]$l);[int](([Environment]::TickCount - $l.dw) / 1000)";

static IDLE_MAC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""HIDIdleTime"\s*=\s*(\d+)"#).unwrap());

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sinais {
    pub ocioso_s: Option<f64>,
    pub bloqueada: Option<bool>,
    pub bt_perto: Option<bool>,
    pub rosto_presente: Option<bool>,
}

fn ps(script: &str) -> Option<Vec<String>> {
    let exe = which("pwsh").or_else(|| which("powershell"))?.to_string_lossy().into_owned();
    Some(vec![exe, "-NoProfile".into(), "-NonInteractive".into(), "-Command".into(), script.into()])
}

fn rodar_texto(argv: &[String], timeout_s: f64) -> Option<String> {
    let r = rodar(argv, None, Duration::from_secs_f64(timeout_s), Some(env_limpo())).ok()?;
    (r.codigo == Some(0)).then(|| String::from_utf8_lossy(&r.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// Segundos sem teclado/mouse, ou `None` se o sistema não informa.
pub fn ocioso_segundos() -> Option<f64> {
    if cfg!(windows) {
        let s = rodar_texto(&ps(PS_OCIOSO)?, 8.0)?;
        return s.parse::<f64>().ok().map(|v| v.max(0.0));
    }
    if cfg!(target_os = "macos") {
        let s = rodar_texto(&["ioreg".into(), "-c".into(), "IOHIDSystem".into()], 8.0)?;
        let ns: u64 = IDLE_MAC.captures(&s)?.get(1)?.as_str().parse().ok()?;
        return Some(ns as f64 / 1e9);
    }
    if which("xprintidle").is_some() {
        let s = rodar_texto(&["xprintidle".into()], 8.0)?;
        return s.parse::<f64>().ok().map(|ms| (ms / 1000.0).max(0.0));
    }
    None
}

/// Bloqueia a estação (só bloqueia; nunca desbloqueia).
pub fn bloquear_tela() -> bool {
    let argv: Vec<String> = if cfg!(windows) {
        vec!["rundll32.exe".into(), "user32.dll,LockWorkStation".into()]
    } else if cfg!(target_os = "macos") {
        vec!["pmset".into(), "displaysleepnow".into()]
    } else if which("loginctl").is_some() {
        vec!["loginctl".into(), "lock-session".into()]
    } else if which("xdg-screensaver").is_some() {
        vec!["xdg-screensaver".into(), "lock".into()]
    } else {
        return false;
    };
    rodar(&argv, None, Duration::from_secs(8), Some(env_limpo())).is_ok_and(|r| r.codigo == Some(0))
}

/// `true`/`false` se conseguir checar o dispositivo Bluetooth; `None` se não há ferramenta ou endereço.
pub fn bt_perto(alvo: &str) -> Option<bool> {
    let limpo = alvo.trim();
    if limpo.is_empty() {
        return None;
    }
    if cfg!(windows) {
        let seguro: String = limpo.chars().filter(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '_' | ' ')).take(60).collect();
        if seguro.is_empty() {
            return None;
        }
        let cmd = format!(
            "$d = Get-PnpDevice -Class Bluetooth -ErrorAction SilentlyContinue | Where-Object {{ $_.FriendlyName -like '*{seguro}*' -or $_.InstanceId -like '*{seguro}*' }}; if (-not $d) {{ 'unknown' }} elseif ($d | Where-Object {{ $_.Status -eq 'OK' }}) {{ 'yes' }} else {{ 'no' }}"
        );
        let s = rodar_texto(&ps(&cmd)?, 8.0)?;
        return match s.as_str() {
            "yes" => Some(true),
            "no" => Some(false),
            _ => None,
        };
    }
    if which("bluetoothctl").is_some() {
        let s = rodar_texto(&["bluetoothctl".into(), "info".into(), limpo.into()], 8.0)?;
        if s.contains("Connected: yes") {
            return Some(true);
        }
        if s.contains("Connected: no") {
            return Some(false);
        }
    }
    None
}

pub struct Presenca {
    pub away_s: f64,
    estado: Mutex<&'static str>,
}

impl Presenca {
    pub fn new(away_min: u32) -> Self {
        Self {
            away_s: (away_min.max(1) as f64) * 60.0,
            estado: Mutex::new("presente"),
        }
    }

    pub fn estado(&self) -> &'static str {
        *self.estado.lock()
    }

    /// Devolve `(estado_atual, mudou)`: `'presente'` ou `'ausente'`.
    pub fn avaliar(&self, sinais: Sinais) -> (&'static str, bool) {
        let mut g = self.estado.lock();
        let anterior = *g;
        if sinais.bloqueada == Some(true) {
            *g = "ausente";
            return (*g, *g != anterior);
        }
        if sinais.rosto_presente == Some(true) {
            *g = "presente";
            return (*g, *g != anterior);
        }
        if let Some(oc) = sinais.ocioso_s {
            if oc < 30.0 {
                *g = "presente";
                return (*g, *g != anterior);
            }
            if oc >= self.away_s {
                if sinais.bt_perto == Some(false) || sinais.rosto_presente == Some(false) || (sinais.bt_perto.is_none() && sinais.rosto_presente.is_none()) {
                    *g = "ausente";
                    return (*g, *g != anterior);
                }
            }
        }
        if sinais.rosto_presente == Some(false) && sinais.bt_perto == Some(false) {
            *g = "ausente";
        }
        (*g, *g != anterior)
    }
}

pub fn sinais_do_sistema(bt_alvo: &str, rosto_presente: Option<bool>) -> Sinais {
    Sinais {
        ocioso_s: ocioso_segundos(),
        bloqueada: tela_bloqueada(),
        bt_perto: bt_perto(bt_alvo),
        rosto_presente,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presenca_transita_entre_presente_e_ausente() {
        let p = Presenca::new(3);
        assert_eq!(p.avaliar(Sinais { ocioso_s: Some(5.0), bloqueada: Some(false), bt_perto: None, rosto_presente: None }), ("presente", false));
        assert_eq!(p.avaliar(Sinais { ocioso_s: Some(200.0), bloqueada: Some(false), bt_perto: None, rosto_presente: None }), ("ausente", true));
        assert_eq!(p.avaliar(Sinais { ocioso_s: Some(250.0), bloqueada: Some(false), bt_perto: None, rosto_presente: None }), ("ausente", false));
        assert_eq!(p.avaliar(Sinais { ocioso_s: Some(250.0), bloqueada: Some(false), bt_perto: None, rosto_presente: Some(true) }), ("presente", true));
        assert_eq!(p.avaliar(Sinais { ocioso_s: Some(2.0), bloqueada: Some(true), bt_perto: None, rosto_presente: Some(true) }), ("ausente", true));
    }
}

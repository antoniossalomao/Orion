//! F12.01: agendamento em linguagem natural (pt-BR), sem modelo. Entende o básico do dia a dia:
//! "todo dia às 8h", "a cada 30 minutos", "de hora em hora", "amanhã às 15h30", "hoje às 9 da noite",
//! "daqui a 2 horas", "em 10/11 às 9h". O que não entender vira erro claro, nunca um palpite.

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
pub enum Quando {
    /// Uma vez, nesse instante (segundos UTC).
    Unico(f64),
    /// Todo dia, em `HH:MM`.
    Diario(String),
    /// A cada N minutos.
    Intervalo(i64),
}

static INTERVALO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bcada\s+(\d{1,4})\s*(minutos?|min|m|horas?|h)\b").unwrap());
static DAQUI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(?:daqui a|daqui|em|dentro de)\s+(\d{1,4})\s*(minutos?|min|horas?|h|dias?)\b").unwrap());
static HORA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(?:às|as|ás|a partir das|pelas)\s+(\d{1,2})(?:\s*(?::|h)\s*(\d{2})?)?(?:\s*(?:hrs?|horas?))?(?:\s+da\s+(manhã|manha|tarde|noite|madrugada))?").unwrap());
static HORA_SOLTA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})(?::(\d{2})|h(\d{2})?)\b").unwrap());
static DATA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{1,2})/(\d{1,2})(?:/(\d{4}))?\b").unwrap());

/// Hora do dia citada na frase (`às 9`, `9h30`, `21:00`, `meio-dia`, `meia-noite`, `9 da noite`).
/// `Ok(None)` sem hora na frase; hora citada mas inválida (`25h`, `15h99`) é erro, nunca 09:00.
fn hora(f: &str) -> Result<Option<NaiveTime>, String> {
    if f.contains("meio-dia") || f.contains("meio dia") {
        return Ok(NaiveTime::from_hms_opt(12, 0, 0));
    }
    if f.contains("meia-noite") || f.contains("meia noite") {
        return Ok(NaiveTime::from_hms_opt(0, 0, 0));
    }
    let num = |s: &str| s.parse::<u32>().map_err(|_| "hora inválida".to_string());
    let (h, m, periodo) = if let Some(c) = HORA.captures(f) {
        (num(&c[1])?, c.get(2).map_or(Ok(0), |m| num(m.as_str()))?, c.get(3).map(|p| p.as_str().to_string()))
    } else if let Some(c) = HORA_SOLTA.captures(f) {
        (num(&c[1])?, c.get(2).or(c.get(3)).map_or(Ok(0), |m| num(m.as_str()))?, None)
    } else {
        return Ok(None);
    };
    let h = match periodo.as_deref() {
        Some("tarde" | "noite") if h < 12 => h + 12,
        Some("manhã" | "manha" | "madrugada") if h == 12 => 0,
        _ => h,
    };
    NaiveTime::from_hms_opt(h, m, 0).map(Some).ok_or_else(|| format!("hora inválida: {h:02}:{m:02}"))
}

fn instante(dia: NaiveDate, h: NaiveTime) -> Option<f64> {
    Local.from_local_datetime(&dia.and_time(h)).earliest().map(|d| d.timestamp() as f64)
}

/// Interpreta a frase a partir de `agora` (segundos UTC).
pub fn interpretar(frase: &str, agora: f64) -> Result<Quando, String> {
    let f = frase.trim().to_lowercase();
    let f = f.as_str();
    if f.is_empty() {
        return Err("diga quando: ex. 'todo dia às 8h', 'a cada 30 minutos', 'amanhã às 15h'".into());
    }
    if f.contains("de hora em hora") {
        return Ok(Quando::Intervalo(60));
    }
    if f.contains("cada meia hora") || f.contains("cada meia-hora") {
        return Ok(Quando::Intervalo(30));
    }
    if let Some(c) = INTERVALO.captures(f) {
        let n: i64 = c[1].parse().map_err(|_| "intervalo inválido")?;
        let min = if c[2].starts_with('h') { n * 60 } else { n };
        if !(1..=10_080).contains(&min) {
            return Err("intervalo de 1 minuto a 7 dias".into());
        }
        return Ok(Quando::Intervalo(min));
    }
    let diario = ["todo dia", "todos os dias", "diariamente", "toda manhã", "toda noite", "toda tarde"].iter().any(|d| f.contains(d));
    if diario {
        let h = hora(f)?.ok_or("diga o horário: ex. 'todo dia às 8h'")?;
        return Ok(Quando::Diario(h.format("%H:%M").to_string()));
    }
    if ["toda segunda", "toda terça", "toda quarta", "toda quinta", "toda sexta", "todo sábado", "todo domingo", "toda semana", "todo mês"].iter().any(|d| f.contains(d)) {
        return Err("repetição por semana ou mês ainda não existe: use 'todo dia' ou 'a cada N horas'".into());
    }
    if let Some(c) = DAQUI.captures(f) {
        let n: i64 = c[1].parse().map_err(|_| "tempo inválido")?;
        let seg = match &c[2] {
            u if u.starts_with('h') => n * 3600,
            u if u.starts_with('d') => n * 86_400,
            _ => n * 60,
        };
        if seg <= 0 || seg > 366 * 86_400 {
            return Err("tempo de 1 minuto a 1 ano".into());
        }
        return Ok(Quando::Unico(agora + seg as f64));
    }
    let hoje = chrono::DateTime::from_timestamp(agora as i64, 0).ok_or("relógio inválido")?.with_timezone(&Local).date_naive();
    let dia = if f.contains("depois de amanhã") {
        Some(hoje + Duration::days(2))
    } else if f.contains("amanhã") || f.contains("amanha") {
        Some(hoje + Duration::days(1))
    } else if f.contains("hoje") {
        Some(hoje)
    } else if let Some(c) = DATA.captures(f) {
        let (d, m): (u32, u32) = (c[1].parse().map_err(|_| "data inválida")?, c[2].parse().map_err(|_| "data inválida")?);
        let ano = c.get(3).and_then(|a| a.as_str().parse().ok()).unwrap_or(hoje.year());
        let data = NaiveDate::from_ymd_opt(ano, m, d).ok_or("data inválida")?;
        // sem ano e já passou: é o do ano que vem
        Some(if c.get(3).is_none() && data < hoje { NaiveDate::from_ymd_opt(ano + 1, m, d).ok_or("data inválida")? } else { data })
    } else {
        None
    };
    let h = hora(f)?;
    let (dia, h) = match (dia, h) {
        (Some(d), Some(h)) => (d, h),
        (Some(d), None) => (d, NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
        // só a hora: hoje, ou amanhã se já passou
        (None, Some(h)) => (hoje, h),
        (None, None) => return Err("não entendi quando; ex.: 'todo dia às 8h', 'a cada 2 horas', 'amanhã às 15h', 'daqui a 20 minutos'".into()),
    };
    let mut t = instante(dia, h).ok_or("hora local inexistente")?;
    if t <= agora {
        if f.contains("hoje") || DATA.is_match(f) {
            return Err("esse horário já passou".into());
        }
        t = instante(dia + Duration::days(1), h).ok_or("hora local inexistente")?;
    }
    Ok(Quando::Unico(t))
}

//! Parses the device "Estado" screens (information pages cal:9, cal:9.1, ...) into metrics.
//!
//! The screens are plain text lines such as "Ida nominal: 30.0°C", "116.1: 25.3°C" or
//! "116.B Zona B: Off", grouped under headings like "Entradas:" or "Bomba circuito 116.1".

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
pub struct Metric {
    /// Stable key used for the history, e.g. "Entradas · 116.1 [°C]".
    pub key: String,
    pub group: String,
    pub label: String,
    pub text: String,
    pub value: Option<f64>,
    pub unit: String,
}

/// Splits "25.3°C" into (25.3, "°C"), "Off"/"On" into 0/1.
fn parse_value(text: &str) -> (Option<f64>, String) {
    let t = text.trim();
    match t.to_ascii_lowercase().as_str() {
        "off" => return (Some(0.0), "on/off".into()),
        "on" => return (Some(1.0), "on/off".into()),
        _ => {}
    }
    let bytes = t.as_bytes();
    let starts_numeric = bytes.first().is_some_and(|b| {
        b.is_ascii_digit() || (*b == b'-' && bytes.get(1).is_some_and(u8::is_ascii_digit))
    });
    if !starts_numeric {
        return (None, String::new());
    }
    let end = t
        .char_indices()
        .skip(1)
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.' || *c == ','))
        .map(|(i, _)| i)
        .unwrap_or(t.len());
    let num = t[..end].replace(',', ".").parse::<f64>().ok();
    (num, t[end..].trim().to_string())
}

pub fn parse(lines: &[String]) -> Vec<Metric> {
    let mut out = Vec::new();
    let mut group = String::new();
    for raw in lines {
        let line = raw.trim();
        // blank lines separate blocks on the device screens
        if line.is_empty() {
            group.clear();
            continue;
        }
        match line
            .rsplit_once(": ")
            .or_else(|| line.strip_suffix(':').map(|l| (l, "")))
        {
            Some((label, "")) => group = label.trim().to_string(),
            Some((label, text)) => {
                if group.starts_with("Fecha") {
                    continue;
                }
                let (value, unit) = parse_value(text);
                let label = label.trim().to_string();
                let unit_key = if unit.is_empty() || unit == "on/off" {
                    String::new()
                } else {
                    format!(" [{unit}]")
                };
                let key = if group.is_empty() {
                    format!("{label}{unit_key}")
                } else {
                    format!("{group} · {label}{unit_key}")
                };
                out.push(Metric {
                    key,
                    group: group.clone(),
                    label,
                    text: text.to_string(),
                    value,
                    unit,
                });
            }
            // a line without a colon is a heading ("Bomba circuito 116.1") unless it is a bare value
            None => {
                if line.chars().any(char::is_alphabetic) {
                    group = line.to_string();
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_screen() {
        let lines: Vec<String> = [
            "Fecha y hora:",
            "2026-10-15",
            "22:21:05",
            "",
            "Exterior: --",
            "",
            "Bomba circuito 116.1",
            "Modo: off",
            "Ida nominal: 30.0°C",
            "Entradas:",
            "1: 25.0°C",
            "1: 72.9% RH",
            "116.N2: _/",
            "Salidas:",
            "116.B Zona B: Off",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let m = parse(&lines);
        let get = |k: &str| {
            m.iter()
                .find(|x| x.key == k)
                .unwrap_or_else(|| panic!("{k} in {m:#?}"))
        };
        assert_eq!(
            get("Bomba circuito 116.1 · Ida nominal [°C]").value,
            Some(30.0)
        );
        assert_eq!(get("Entradas · 1 [°C]").value, Some(25.0));
        assert_eq!(get("Entradas · 1 [% RH]").value, Some(72.9));
        assert_eq!(get("Salidas · 116.B Zona B").value, Some(0.0));
        assert_eq!(get("Exterior").value, None);
        assert!(!m.iter().any(|x| x.key.contains("2026")));
    }
}

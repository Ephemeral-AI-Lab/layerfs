//! What a call prints (S §2.6): the result as one line of JSON on stdout,
//! or with `--text` a readable form; a refusal as JSON on stderr, exit 125.
use crate::run::REFUSED;
use layerfs_api::json;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::{IsTerminal, Write};
use std::process::ExitCode;

/// Prints `refusal` as `{"code","message","detail"}` on stderr.
pub fn refusal(refusal: &SdkRefusal) -> ExitCode {
    let value = SdkValue::map([
        ("code", SdkValue::text(&refusal.code)),
        ("message", SdkValue::text(&refusal.message)),
        ("detail", refusal.detail.clone()),
    ]);
    diagnostic(&json::print(&value));
    ExitCode::from(REFUSED)
}

/// Prints a call's result.
pub fn result(value: &SdkValue, text: bool) -> ExitCode {
    let printed = if text {
        readable(value)
    } else {
        format!("{}\n", json::print(value))
    };
    print(&printed)
}

/// `layerfs doctor --text`: a count line, then each finding as
/// `<severity> <kind> <name>: <message>` and its fix on the next line.
pub fn findings(report: &SdkValue) -> ExitCode {
    let empty = Vec::new();
    let found = match report.get("findings") {
        Some(SdkValue::List(found)) => found,
        _ => &empty,
    };
    let text = |value: &SdkValue, key: &str| value.get_text(key).unwrap_or("").to_owned();
    let errors = found
        .iter()
        .filter(|f| text(f, "severity") == "error")
        .count();
    let mut printed = match found.len() {
        0 => "ok: nothing to report\n".to_owned(),
        n => format!("{errors} error(s), {} warning(s)\n", n - errors),
    };
    for finding in found {
        let entity = finding.get("entity").cloned().unwrap_or_default();
        printed.push_str(&format!(
            "{:<8} {} {}: {}\n         fix: {}\n",
            text(finding, "severity"),
            text(&entity, "kind"),
            text(&entity, "name"),
            text(finding, "message"),
            text(finding, "fix"),
        ));
    }
    print(&printed)
}

/// Writes one line to stderr. A stderr that is gone loses it and the exit
/// code stays the outcome's own (`eprintln!` would panic).
pub fn diagnostic(line: &str) {
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

/// Writes `printed` to stdout.
pub fn print(printed: &str) -> ExitCode {
    let mut out = std::io::stdout().lock();
    if out
        .write_all(printed.as_bytes())
        .and_then(|_| out.flush())
        .is_err()
    {
        // stdout went away; the call itself succeeded
        return ExitCode::from(REFUSED);
    }
    ExitCode::SUCCESS
}

/// Fields formatted as dates in UTC (YYYY-MM-DD HH:MM:SS) in `--text`.
const TIME_FIELDS: &[&str] = &["created", "modified", "at"];

/// Fields formatted as human sizes in `--text`.
const SIZE_FIELDS: &[&str] = &[
    "size",
    "bytes",
    "part_size",
    "memory",
    "disk",
    "free",
    "free_disk",
    "wal_bytes",
    "incoming_bytes",
    "stdout_bytes",
    "stderr_bytes",
    "logical",
    "new_canonical",
    "new_stored",
    "new_metadata",
];

/// Formats epoch milliseconds as `YYYY-MM-DD HH:MM:SS` UTC.
pub fn format_time(ms: i64) -> String {
    if ms < 0 {
        return ms.to_string();
    }
    let s = ms / 1000;
    let rem_s = s % 86400;
    let sec = rem_s % 60;
    let rem_m = rem_s / 60;
    let min = rem_m % 60;
    let hour = rem_m / 60;
    let days = s / 86400;

    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{min:02}:{sec:02}")
}

/// Formats byte counts as human units (B, KiB, MiB, GiB, TiB, PiB).
pub fn format_size(bytes: i64) -> String {
    if bytes < 0 {
        return bytes.to_string();
    }
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let units = ["KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut val = bytes as f64;
    for (i, unit) in units.iter().enumerate() {
        val /= 1024.0;
        if val < 1024.0 || i == units.len() - 1 {
            let mut s = format!("{val:.1}");
            if s.ends_with(".0") {
                s.truncate(s.len() - 2);
            }
            return format!("{s} {unit}");
        }
    }
    format!("{bytes} B")
}

/// One scalar as a table cell, formatted for human reading if field name matches.
pub fn format_field(key: &str, value: &SdkValue) -> String {
    match value {
        SdkValue::Int(n) if TIME_FIELDS.contains(&key) => format_time(*n),
        SdkValue::Int(n) if SIZE_FIELDS.contains(&key) => format_size(*n),
        _ => cell(value),
    }
}

/// One scalar as a table cell.
fn cell(value: &SdkValue) -> String {
    match value {
        SdkValue::Null => "-".to_owned(),
        SdkValue::Text(text) => text.clone(),
        SdkValue::Int(n) => n.to_string(),
        SdkValue::Bool(b) => b.to_string(),
        other => json::print(other),
    }
}

/// `--text`: a file's text as it is; a result whose one list holds records
/// as a table of their scalar fields; anything else as `key  value` lines.
fn readable(value: &SdkValue) -> String {
    if let Some(SdkValue::Text(text)) = value.get("text") {
        return text.clone();
    }
    let SdkValue::Map(fields) = value else {
        return format!("{}\n", cell(value));
    };
    let mut out = String::new();
    for (name, field) in fields {
        match field {
            SdkValue::List(rows) if rows.iter().all(|r| matches!(r, SdkValue::Map(_))) => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&table(name, rows));
            }
            _ => out.push_str(&format!("{name}  {}\n", format_field(name, field))),
        }
    }
    out
}

/// Rows as aligned columns, upper-case headers; nested values are left out.
fn table(name: &str, rows: &[SdkValue]) -> String {
    let mut columns: Vec<&str> = Vec::new();
    for row in rows {
        if let SdkValue::Map(fields) = row {
            for (key, value) in fields {
                if !matches!(value, SdkValue::Map(_) | SdkValue::List(_))
                    && !columns.contains(&key.as_str())
                {
                    columns.push(key);
                }
            }
        }
    }
    if rows.is_empty() || columns.is_empty() {
        return format!("{}  (none)\n", name.to_uppercase());
    }
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|c| {
                    row.get(c)
                        .map(|v| format_field(c, v))
                        .unwrap_or_else(|| "-".to_owned())
                })
                .collect()
        })
        .collect();
    let widths: Vec<usize> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| {
            cells
                .iter()
                .map(|r| r[i].chars().count())
                .max()
                .unwrap_or(0)
                .max(c.len())
        })
        .collect();
    let line = |values: Vec<String>| {
        let mut text = values
            .iter()
            .zip(&widths)
            .map(|(v, w)| format!("{v:<w$}"))
            .collect::<Vec<_>>()
            .join("  ");
        text.truncate(text.trim_end().len());
        text.push('\n');
        text
    };
    let raw_header = line(columns.iter().map(|c| c.to_uppercase()).collect());
    let mut out = if std::io::stdout().is_terminal() {
        // Paint header in bold (1)
        let trimmed = raw_header.trim_end_matches('\n');
        format!("{}\n", crate::banner::paint(trimmed, "1"))
    } else {
        raw_header
    };
    for row in cells {
        out.push_str(&line(row));
    }
    out
}

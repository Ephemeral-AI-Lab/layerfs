//! What a call prints (S §2.6): the result as one line of JSON on stdout,
//! or with `--text` a readable form; a refusal as JSON on stderr, exit 125.
use crate::run::REFUSED;
use layerfs_api::{json, Form, Part};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::Write;
use std::process::ExitCode;

/// Prints `refusal` as one line of JSON on stderr: `{"error","message"}`,
/// `fix` when there is one and the fields the next call needs; with
/// `detail` (`--detail`) its long form, which adds `detail`.
pub fn refusal(refusal: &SdkRefusal, detail: bool) -> ExitCode {
    diagnostic(&json::print(&layerfs_api::refusal::value(refusal, detail)));
    ExitCode::from(REFUSED)
}

/// Prints a call's result; `form` is the call's declared answer, which
/// `--text` reads for the columns its rows may leave out.
pub fn result(value: &SdkValue, text: bool, form: Option<&Form>) -> ExitCode {
    let printed = if text {
        readable(value, form)
    } else {
        format!("{}\n", json::print(value))
    };
    print(&printed)
}

/// A doctor's report with `--text`: a count line, then each finding as
/// `<severity> <thing> <name> <check>: <message>` with its fix on the next
/// line, then each check that could not be made and why.
pub fn doctor(report: &SdkValue) -> ExitCode {
    let empty = Vec::new();
    let rows = |key: &str| match report.get(key) {
        Some(SdkValue::List(rows)) => rows,
        _ => &empty,
    };
    let (found, open) = (rows("findings"), rows("not_checked"));
    let text = |value: &SdkValue, key: &str| value.get_text(key).unwrap_or("").to_owned();
    // a finding or entry names its thing by its own word, its first key
    let named = |value: &SdkValue| match value {
        SdkValue::Map(fields) => fields.first().map_or(String::new(), |(word, name)| {
            format!("{word} {}", name_of(name))
        }),
        _ => String::new(),
    };
    let count = |severity: &str| {
        (found.iter())
            .filter(|f| text(f, "severity") == severity)
            .count()
    };
    let mut printed = match (found.len(), open.len()) {
        (0, 0) => "ok: every check was made and nothing was found\n".to_owned(),
        _ => format!(
            "{} error(s), {} warning(s), {} notice(s), {} not checked\n",
            count("error"),
            count("warning"),
            count("notice"),
            open.len()
        ),
    };
    for finding in found {
        printed.push_str(&format!(
            "{:<8} {} {}: {}\n",
            text(finding, "severity"),
            named(finding),
            text(finding, "check"),
            text(finding, "message"),
        ));
        if let Some(from) = finding.get_text("from") {
            printed.push_str(&format!("         from: {from}\n"));
        }
        if let Some(fix) = finding.get_text("fix") {
            printed.push_str(&format!("         fix: {fix}\n"));
        }
    }
    for entry in open {
        let why = match (entry.get("blocked_by"), entry.get_text("reason")) {
            (Some(by @ SdkValue::Map(_)), _) => {
                format!("blocked by {} ({})", named(by), text(by, "reason"))
            }
            (_, Some(reason)) => reason.to_owned(),
            _ => String::new(),
        };
        let check = entry
            .get_text("check")
            .map_or(String::new(), |check| format!(" {check}"));
        printed.push_str(&format!(
            "{:<8} {}{check}: {why}\n",
            "not checked",
            named(entry)
        ));
    }
    print(&printed)
}

fn name_of(value: &SdkValue) -> String {
    match value {
        SdkValue::Text(text) => text.clone(),
        other => json::print(other),
    }
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

/// Fields formatted as human sizes in `--text`.
const SIZE_FIELDS: &[&str] = &["size", "bytes", "stdout_bytes", "stderr_bytes"];

/// The progress phases whose `done` and `total` are bytes: a Store, a
/// project's parts or an image sent to a machine (`send`), a Store move's
/// `snapshot` and `verify` (each hashes the file), a backup's `hash`,
/// `upload` and `download`, and the bytes `project.export` wrote
/// (`export` and its last event `finalize`; an image copy's `export`
/// carries no counts). Every other
/// phase with counts counts steps or things: a doctor's, a create's,
/// a start's, a remove's.
const BYTE_PHASES: [&str; 8] = [
    "send", "snapshot", "verify", "hash", "upload", "download", "export", "finalize",
];

/// One progress event as the terminal line shows it, `elapsed` seconds
/// after the first: `done`/`total` in bytes (with a rate) only in a phase
/// that moves bytes, else as plain counts (a doctor's or a create's steps;
/// conformance R4 F8).
pub fn progress_line(phase: &str, done: Option<i64>, total: Option<i64>, elapsed: f64) -> String {
    let bytes = BYTE_PHASES.contains(&phase);
    let shown = |n: i64| match bytes {
        true => format_size(n),
        false => n.to_string(),
    };
    let mut status = format!("phase: {phase}");
    if let Some(done) = done {
        status.push_str(&format!("  {}", shown(done)));
        if let Some(total) = total {
            status.push_str(&format!("/{}", shown(total)));
        }
        if bytes && elapsed > 0.5 && done > 0 {
            let rate = (done as f64) / elapsed;
            status.push_str(&format!("  ({}/s)", format_size(rate as i64)));
        }
    }
    status
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
/// `form`, the call's declared answer, gives each list's row form.
pub fn readable(value: &SdkValue, form: Option<&Form>) -> String {
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
                let declared = match form.and_then(|form| form.part(name)) {
                    Some(Part::Rows(rows)) => Some(*rows),
                    _ => None,
                };
                out.push_str(&table(name, rows, declared));
            }
            _ => out.push_str(&format!("{name}  {}\n", format_field(name, field))),
        }
    }
    out
}

/// The cell of column `column` in a row that left it out. The short form
/// leaves out `state: ready` (and null, false and empty values), so a row
/// with no `state` is `ready`; any other absent value is `-`.
fn absent(column: &str) -> &'static str {
    match column {
        "state" => "ready",
        _ => "-",
    }
}

/// Rows as aligned columns, upper-case headers; nested values are left out.
/// A row form (`declared`) whose rows carry a state keeps its STATE column
/// when every row left `state: ready` out, in the place its long form
/// gives it.
fn table(name: &str, rows: &[SdkValue], declared: Option<&Form>) -> String {
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
    let long = declared.map_or(&[][..], |form| form.long);
    if let Some(at) = long.iter().position(|key| *key == "state") {
        if !columns.is_empty() && !columns.contains(&"state") {
            let before = |key: &&str| long.iter().position(|k| k == key).is_some_and(|i| i < at);
            let place = columns.iter().take_while(|key| before(key)).count();
            columns.insert(place, "state");
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
                        .unwrap_or_else(|| absent(c).to_owned())
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
    let mut out = format!(
        "{}\n",
        crate::banner::bold(raw_header.trim_end_matches('\n'))
    );
    for row in cells {
        out.push_str(&line(row));
    }
    out
}

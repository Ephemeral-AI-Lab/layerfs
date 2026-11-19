//! What a call prints (S §2.6): the result as one line of JSON on stdout,
//! or a readable form (`--text`, the default on a terminal); a refusal on
//! stderr in the same form, exit 125.
use crate::run::REFUSED;
use layerfs_api::{json, Form, Part};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::Write;
use std::process::ExitCode;

/// Prints `refusal` on stderr: one line of JSON, `{"error","message"}`,
/// `fix` when there is one and the fields the next call needs; with
/// `detail` (`--detail`) its long form, which adds `detail`. With `text`
/// it reads as `error: <message>  (<code>)`, then the fix and the fields.
pub fn refusal(refusal: &SdkRefusal, detail: bool, text: bool) -> ExitCode {
    let value = layerfs_api::refusal::value(refusal, detail);
    match text {
        true => diagnostic(refusal_text(&value).trim_end()),
        false => diagnostic(&json::print(&value)),
    }
    ExitCode::from(REFUSED)
}

/// A refusal's readable form: its message and code, then `fix:` and each
/// other field it carries, indented.
pub fn refusal_text(value: &SdkValue) -> String {
    let code = value.get_text("error").unwrap_or("Refused");
    let message = value.get_text("message").unwrap_or("");
    let mut text = format!("error: {message}  ({code})\n");
    if let Some(fix) = value.get_text("fix") {
        text.push_str(&format!("  fix: {fix}\n"));
    }
    if let SdkValue::Map(fields) = value {
        let shown = ["error", "message", "fix"];
        for (key, field) in fields.iter().filter(|(k, _)| !shown.contains(&k.as_str())) {
            let news = !matches!(field, SdkValue::Null | SdkValue::Bool(false));
            if news {
                text.push_str(&format!("  {}: {}\n", label(key), inline(key, field)));
            }
        }
    }
    text
}

/// Prints a call's result; `form` is the call's declared answer, which
/// the readable form reads for the columns its rows may leave out.
pub fn result(value: &SdkValue, text: bool, form: Option<&Form>) -> ExitCode {
    let printed = if text {
        readable(value, form)
    } else {
        format!("{}\n", json::print(value))
    };
    print(&printed)
}

/// A doctor's report in the readable form: a count line, then each finding as
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

/// Fields shown as human sizes, beside every `*_bytes`.
const SIZE_FIELDS: &[&str] = &["size", "bytes"];

/// Whether field `key` holds a byte count.
fn is_size(key: &str) -> bool {
    SIZE_FIELDS.contains(&key) || key.ends_with("_bytes")
}

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

/// One scalar as the readable form shows it: a byte count as a size, a
/// Commit id by its first 14 characters (a prefix every lookup takes).
pub fn format_field(key: &str, value: &SdkValue) -> String {
    match value {
        SdkValue::Int(n) if is_size(key) => format_size(*n),
        SdkValue::Text(text) if is_node(text) => text[..SHORT_ID].to_owned(),
        _ => cell(value),
    }
}

/// The characters a Commit id shows: its tag and six bytes.
const SHORT_ID: usize = 14;

/// Whether `text` is a Commit or publication id: 66 hex characters.
fn is_node(text: &str) -> bool {
    text.len() == 66 && text.bytes().all(|b| b.is_ascii_hexdigit())
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

/// A value on one line: a scalar as [`format_field`], a list joined by
/// commas (a record by its first value: a change by its path), anything
/// else as JSON.
fn inline(key: &str, value: &SdkValue) -> String {
    let first = |item: &SdkValue| match item {
        SdkValue::Map(fields) => fields.first().map(|(k, v)| format_field(k, v)),
        other => Some(format_field(key, other)),
    };
    match value {
        SdkValue::List(items) => {
            let items: Vec<String> = items.iter().filter_map(first).collect();
            items.join(", ")
        }
        other => format_field(key, other),
    }
}

fn scalar(value: &SdkValue) -> bool {
    !matches!(value, SdkValue::Map(_) | SdkValue::List(_))
}

/// A key as a label: `memory_used_bytes` reads `memory used` (its value
/// shows as a size), `file_count` reads `file count`.
fn label(key: &str) -> String {
    let key = match key.strip_suffix("_bytes") {
        Some(base) if !base.is_empty() => base,
        _ => key,
    };
    key.replace('_', " ")
}

/// The readable form: a file's text as it is; `done` for a result that
/// says only that it worked; otherwise each scalar as an aligned `label
/// value` line, each map as an indented block under its label, each list
/// of records as a table (a list other than `items` under its label) and
/// each list of plain values one per line. `form`, the call's declared
/// answer, gives each list's row form.
pub fn readable(value: &SdkValue, form: Option<&Form>) -> String {
    if let Some(SdkValue::Text(text)) = value.get("text") {
        return text.clone();
    }
    let SdkValue::Map(fields) = value else {
        return format!("{}\n", cell(value));
    };
    let only_ok = matches!(&fields[..], [(key, SdkValue::Bool(true))] if key == "ok");
    if fields.is_empty() || only_ok {
        return "done\n".to_owned();
    }
    let mut out = String::new();
    block(fields, form, 0, &mut out);
    out
}

/// The fields of one map at `indent`, as [`readable`] lays them out.
fn block(fields: &[(String, SdkValue)], form: Option<&Form>, indent: usize, out: &mut String) {
    let pad = " ".repeat(indent);
    let width = (fields.iter())
        .filter(|(_, v)| scalar(v) || matches!(v, SdkValue::List(l) if l.iter().all(scalar)))
        .map(|(k, _)| label(k).chars().count())
        .max()
        .unwrap_or(0);
    for (name, field) in fields {
        match field {
            SdkValue::List(rows) if !rows.is_empty() && rows.iter().all(|r| !scalar(r)) => {
                let declared = match form.and_then(|form| form.part(name)) {
                    Some(Part::Rows(rows)) => Some(*rows),
                    _ => None,
                };
                if !out.is_empty() {
                    out.push('\n');
                }
                if name != "items" {
                    out.push_str(&format!("{pad}{}\n", crate::banner::bold(&label(name))));
                }
                out.push_str(&table(rows, declared, &pad));
            }
            SdkValue::List(items) if name == "items" && indent == 0 => match items.is_empty() {
                true => out.push_str("(none)\n"),
                false => {
                    for item in items {
                        out.push_str(&format!("{}\n", format_field(name, item)));
                    }
                }
            },
            SdkValue::Map(inner) => {
                out.push_str(&format!("{pad}{}\n", label(name)));
                let declared = match form.and_then(|form| form.part(name)) {
                    Some(Part::Rows(rows)) => Some(*rows),
                    _ => None,
                };
                block(inner, declared, indent + 2, out);
            }
            other => {
                let shown = match other {
                    SdkValue::List(items) if items.is_empty() => "(none)".to_owned(),
                    other => inline(name, other),
                };
                out.push_str(&format!("{pad}{:<width$}  {shown}\n", label(name)));
            }
        }
    }
}

/// What a `*_not_listed` flag says under a table.
fn note(flag: &str) -> String {
    match flag {
        "workspaces_not_listed" => {
            "Workspaces kept on the machine (`layerfs workspace list` reads them)".to_owned()
        }
        other => format!("{} here", label(other)),
    }
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

/// Rows as aligned columns under bold headers, each line after `pad`;
/// nested values are left out. A row form (`declared`) whose rows carry a
/// state keeps its STATE column when every row left `state: ready` out, in
/// the place its long form gives it. A `*_not_listed` flag is no column:
/// a note under the table names the rows it is set on.
fn table(rows: &[SdkValue], declared: Option<&Form>, pad: &str) -> String {
    let mut columns: Vec<&str> = Vec::new();
    let mut flags: Vec<&str> = Vec::new();
    for row in rows {
        if let SdkValue::Map(fields) = row {
            for (key, value) in fields {
                if key.ends_with("_not_listed") {
                    if !flags.contains(&key.as_str()) {
                        flags.push(key);
                    }
                } else if scalar(value) && !columns.contains(&key.as_str()) {
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
    if columns.is_empty() {
        return format!("{pad}(none)\n");
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
    let headers: Vec<String> = columns.iter().map(|c| label(c).to_uppercase()).collect();
    let widths: Vec<usize> = (headers.iter().enumerate())
        .map(|(i, header)| {
            (cells.iter().map(|r| r[i].chars().count()))
                .max()
                .unwrap_or(0)
                .max(header.chars().count())
        })
        .collect();
    let line = |values: &[String]| {
        let text = (values.iter().zip(&widths))
            .map(|(v, w)| format!("{v:<w$}"))
            .collect::<Vec<_>>()
            .join("  ");
        format!("{pad}{}", text.trim_end())
    };
    let mut out = format!("{}\n", crate::banner::bold(&line(&headers)));
    for row in &cells {
        out.push_str(&format!("{}\n", line(row)));
    }
    for flag in flags {
        let named: Vec<String> = (rows.iter().zip(&cells))
            .filter(|(row, _)| row.get(flag) == Some(&SdkValue::Bool(true)))
            .map(|(_, cells)| cells[0].clone())
            .collect();
        if !named.is_empty() {
            out.push_str(&format!("{pad}{}: {}\n", note(flag), named.join(", ")));
        }
    }
    out
}

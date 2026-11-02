//! What a call prints (S §2.6): the result as one line of JSON on stdout,
//! or with `--text` a readable form; a refusal as JSON on stderr, exit 125.
use crate::run::REFUSED;
use layerfs_api::calls::json;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::io::Write;
use std::process::ExitCode;

/// Prints `refusal` as `{"code","message","detail"}` on stderr.
pub fn refusal(refusal: &SdkRefusal) -> ExitCode {
    let value = SdkValue::map([
        ("code", SdkValue::text(&refusal.code)),
        ("message", SdkValue::text(&refusal.message)),
        ("detail", refusal.detail.clone()),
    ]);
    eprintln!("{}", json::print(&value));
    ExitCode::from(REFUSED)
}

/// Prints a call's result.
pub fn result(value: &SdkValue, text: bool) -> ExitCode {
    let printed = if text {
        readable(value)
    } else {
        format!("{}\n", json::print(value))
    };
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
            _ => out.push_str(&format!("{name}  {}\n", cell(field))),
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
                .map(|c| row.get(c).map(cell).unwrap_or_else(|| "-".to_owned()))
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
    let mut out = line(columns.iter().map(|c| c.to_uppercase()).collect());
    for row in cells {
        out.push_str(&line(row));
    }
    out
}

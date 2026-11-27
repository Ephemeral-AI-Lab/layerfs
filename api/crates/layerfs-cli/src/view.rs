//! The readable form of the calls whose answer reads better its own way:
//! `workspace exec` as the command's own output, `file blame` as one row
//! per range, `commit diff --with patch` as changed lines, and the next
//! page or window as the command line that reads it. Every other answer
//! takes [`crate::output::readable`].
use crate::output;
use crate::style::paint;
use layerfs_api::Call;
use layerfs_protocol::sdk::SdkValue;
use std::io::{self, Write};

/// Prints `value`, the answer of `call` given as `words`, readably: the
/// call's own view, else the generic one; then, on stderr, the command
/// line of the next page (`next_cursor`) or line window (`next_line`).
pub fn show(call: &Call, words: &[String], value: &SdkValue) -> io::Result<()> {
    let hunks = |item: &SdkValue| item.get("hunks").is_some();
    let printed = match call.name().as_str() {
        "file.blame" => blame(value),
        "commit.diff" if items(value).iter().any(hunks) => diff(value),
        _ => output::readable(&without(value, "next_cursor"), call.form),
    };
    output::write_stdout(&printed)?;
    let next = |key: &str, flag: &str| {
        let at = match value.get(key)? {
            SdkValue::Text(text) => text.clone(),
            SdkValue::Int(n) => n.to_string(),
            _ => return None,
        };
        Some(format!(
            "more: {} {flag} {}",
            again(call, words, flag),
            quote(&at)
        ))
    };
    if let Some(line) = next("next_cursor", "--cursor").or_else(|| next("next_line", "--line")) {
        output::diagnostic(&line);
    }
    Ok(())
}

/// `value` without its field `key`.
fn without(value: &SdkValue, key: &str) -> SdkValue {
    match value {
        SdkValue::Map(fields) => {
            SdkValue::Map((fields.iter()).filter(|(k, _)| k != key).cloned().collect())
        }
        other => other.clone(),
    }
}

fn items(value: &SdkValue) -> &[SdkValue] {
    match value.get("items") {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

/// The command line given, without `flag` and its value, quoted for a
/// shell: what reads the next page with `flag` added.
fn again(call: &Call, words: &[String], flag: &str) -> String {
    let mut line = vec![
        "layerfs".to_owned(),
        call.family.to_owned(),
        call.call.replace('_', "-"),
    ];
    let mut skip = false;
    for word in words {
        if std::mem::take(&mut skip) {
            continue;
        }
        if word == flag {
            skip = true;
        } else if !word.starts_with(&format!("{flag}=")) {
            line.push(quote(word));
        }
    }
    line.join(" ")
}

/// `word` as one shell word: as it is when it is plain, else in single
/// quotes.
fn quote(word: &str) -> String {
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_./:@%+=,".contains(c);
    match !word.is_empty() && word.chars().all(plain) {
        true => word.to_owned(),
        false => format!("'{}'", word.replace('\'', r"'\''")),
    }
}

/// `file blame`: one row per range of lines, with its Commit and message.
fn blame(value: &SdkValue) -> String {
    let empty = Vec::new();
    let list = |key: &str| match value.get(key) {
        Some(SdkValue::List(rows)) => rows,
        _ => &empty,
    };
    let message = |commit: &str| {
        (list("commits").iter())
            .find(|c| c.get_text("commit") == Some(commit))
            .and_then(|c| c.get_text("message"))
            .unwrap_or("")
            .to_owned()
    };
    let rows: Vec<SdkValue> = (list("ranges").iter())
        .map(|range| {
            let (start, end) = (range.get_int("start"), range.get_int("end"));
            let lines = match (start, end) {
                (Some(start), Some(end)) if start != end => format!("{start}-{end}"),
                (Some(start), _) => start.to_string(),
                _ => "-".to_owned(),
            };
            let commit = range.get_text("commit").unwrap_or("");
            SdkValue::map([
                ("lines", SdkValue::text(lines)),
                ("commit", SdkValue::text(commit)),
                ("message", SdkValue::text(message(commit))),
            ])
        })
        .collect();
    output::readable(&SdkValue::map([("items", SdkValue::List(rows))]), None)
}

/// `commit diff --with patch`: each path with its change as a letter, then
/// its changed lines, `-` red and `+` green on a terminal. A file's last
/// line ends in a newline, so the empty line after it is left out.
fn diff(value: &SdkValue) -> String {
    let mut out = String::new();
    for item in items(value) {
        let letter = match item.get_text("change") {
            Some("added") => "A",
            Some("removed") => "D",
            Some("modified") => "M",
            Some(other) => other,
            None => "?",
        };
        let path = item.get_text("path").unwrap_or("");
        out.push_str(&format!("{} {path}\n", paint(letter, "1")));
        let empty = Vec::new();
        let hunks = match item.get("hunks") {
            Some(SdkValue::List(hunks)) => hunks,
            _ => &empty,
        };
        let shown = match hunks.last() {
            Some(last) if last.get_text("text") == Some("") => &hunks[..hunks.len() - 1],
            _ => &hunks[..],
        };
        for hunk in shown {
            let op = hunk.get_text("op").unwrap_or(" ");
            let line = format!("  {op} {}", hunk.get_text("text").unwrap_or(""));
            out.push_str(&match op {
                "-" => paint(&line, "31"),
                "+" => paint(&line, "32"),
                _ => line,
            });
            out.push('\n');
        }
        if item
            .get("hunks_omitted")
            .is_some_and(|v| v != &SdkValue::Bool(false))
        {
            out.push_str("  (more lines than fit this page)\n");
        }
    }
    out
}

/// `workspace exec` without `--stream`: the command's stdout and stderr
/// written as they are to this process's own, then on stderr what else
/// the answer says (a cancel, a spilled output file).
pub fn exec(value: &SdkValue) -> io::Result<()> {
    let write = |key: &str, out: &mut dyn Write| {
        match value.get(key) {
            Some(SdkValue::Text(text)) => out.write_all(text.as_bytes()),
            Some(SdkValue::Bytes(bytes)) => out.write_all(bytes),
            _ => Ok(()),
        }?;
        out.flush()
    };
    let stdout = write("stdout", &mut io::stdout().lock());
    let _ = write("stderr", &mut io::stderr().lock());
    if value.get("cancelled") == Some(&SdkValue::Bool(true)) {
        output::diagnostic("layerfs: the command was cancelled");
    }
    for (key, which) in [("stdout_file", "stdout"), ("stderr_file", "stderr")] {
        if let Some(file) = value.get_text(key) {
            output::diagnostic(&format!("layerfs: the rest of its {which} is in {file}"));
        }
    }
    stdout
}

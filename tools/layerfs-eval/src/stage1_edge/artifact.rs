use super::row_parse::parse_digits;
use crate::stage1_fixture::{self, EvalResult};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
pub(crate) fn durable_write(path: &Path, contents: &str) -> EvalResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    if let Some(parent) = path.parent() {
        stage1_fixture::sync_directory(parent)?;
    }
    Ok(())
}
pub(crate) fn durable_replace(path: &Path, contents: &str) -> EvalResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "durable replacement has no parent".to_owned())?;
    let temporary = parent.join(format!(
        ".stage1.1-rewrite-{}-{}",
        std::process::id(),
        unix_ns()?
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)?;
    stage1_fixture::sync_directory(parent)
}
pub(crate) fn sha256_file(path: &Path) -> EvalResult<String> {
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!("shasum failed for {}", path.display()));
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .filter(|value| value.len() == 64)
        .ok_or_else(|| "shasum returned no SHA-256".to_owned())
}
pub(crate) fn sha256_bytes(bytes: &[u8]) -> EvalResult<String> {
    let mut child = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    child
        .stdin
        .take()
        .ok_or_else(|| "shasum stdin unavailable".to_owned())?
        .write_all(bytes)
        .map_err(io_error)?;
    let output = child.wait_with_output().map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed for bytes".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .filter(|value| value.len() == 64)
        .ok_or_else(|| "shasum returned no SHA-256".to_owned())
}
pub(crate) fn command_output(program: &str, arguments: &[&str]) -> EvalResult<String> {
    String::from_utf8(command_bytes(program, arguments)?).map_err(display_error)
}
pub(crate) fn command_bytes(program: &str, arguments: &[&str]) -> EvalResult<Vec<u8>> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(stage1_fixture::workspace_root())
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} exited {}",
            arguments.join(" "),
            output.status
        ));
    }
    Ok(output.stdout)
}
pub(crate) fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            value if value.is_control() => format!("\\u{:04x}", value as u32).chars().collect(),
            value => vec![value],
        })
        .collect()
}
pub(crate) fn json_string(json: &str, key: &str) -> EvalResult<String> {
    let needle = format!("\"{key}\":\"");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON string {key}"))?;
    let mut output = String::new();
    let mut escaped = false;
    for character in json[start..].chars() {
        if escaped {
            output.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '"' => '"',
                '\\' => '\\',
                _ => return Err(format!("unsupported JSON escape in {key}")),
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Ok(output);
        } else {
            output.push(character);
        }
    }
    Err(format!("unterminated JSON string {key}"))
}
pub(crate) fn json_u128(json: &str, key: &str) -> EvalResult<u128> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON integer {key}"))?;
    let digits = json[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    if digits.is_empty() {
        return Err(format!("invalid JSON integer {key}"));
    }
    digits.parse().map_err(display_error)
}
pub(crate) fn json_i64(json: &str, key: &str) -> EvalResult<i64> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON signed integer {key}"))?;
    let digits = json[start..]
        .chars()
        .enumerate()
        .take_while(|(index, character)| {
            character.is_ascii_digit() || (*index == 0 && *character == '-')
        })
        .map(|(_, character)| character)
        .collect::<String>();
    if digits.is_empty() || digits == "-" {
        return Err(format!("invalid JSON signed integer {key}"));
    }
    digits.parse().map_err(display_error)
}
pub(crate) fn json_optional_u128(json: &str, key: &str) -> EvalResult<Option<u128>> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON optional integer {key}"))?;
    if json[start..].starts_with("null") {
        Ok(None)
    } else {
        parse_digits(&json[start..], key).map(Some)
    }
}
pub(crate) fn json_bool(json: &str, key: &str) -> EvalResult<bool> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .map(|offset| offset + needle.len())
        .ok_or_else(|| format!("missing JSON boolean {key}"))?;
    if json[start..].starts_with("true") {
        Ok(true)
    } else if json[start..].starts_with("false") {
        Ok(false)
    } else {
        Err(format!("invalid JSON boolean {key}"))
    }
}
pub(crate) fn option_usize_json(value: Option<usize>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}
pub(crate) fn option_u8_json(value: Option<u8>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}
pub(crate) fn unix_ns() -> EvalResult<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .map_err(display_error)
}
pub(crate) fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}
pub(crate) fn io_error(error: std::io::Error) -> String {
    error.to_string()
}
pub(crate) fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

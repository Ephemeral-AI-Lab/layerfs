//! Explicit doctor commands outside ordinary call dispatch.
use crate::{output, run};
use layerfs_bridge::control::SdkValue;
use std::process::ExitCode;

/// Parses and executes one diagnostic without initializing the home.
pub fn main(words: &[String]) -> ExitCode {
    if words.iter().any(|w| w == "-h" || w == "--help") {
        let topic: Vec<&str> = words
            .iter()
            .take_while(|w| !w.starts_with('-'))
            .map(String::as_str)
            .collect();
        return match topic.as_slice() {
            ["host", ..] => help(&["host"]),
            ["machine", ..] => help(&["machine"]),
            ["sandbox", ..] => help(&["sandbox"]),
            ["store", "integrity", ..] => help(&["store", "integrity"]),
            ["store", "connection", ..] => help(&["store", "connection"]),
            ["backup", "connection", ..] => help(&["backup", "connection"]),
            _ => help(&topic),
        };
    }
    let (options, text) = match options(words) {
        Ok(v) => v,
        Err(message) => return run::wrong(message),
    };
    if let Err(r) = layerfs_api::doctor_request(&options) {
        return run::wrong(r.message);
    }
    match layerfs_api::home(None).and_then(|home| layerfs_api::doctor_with_options(&home, &options))
    {
        Ok(report) if text => output::findings(&report),
        Ok(report) => output::result(&report, false),
        Err(r) => output::refusal(&r),
    }
}

fn options(words: &[String]) -> Result<(SdkValue, bool), String> {
    let mut positional = Vec::new();
    let mut fields = Vec::new();
    let mut text = false;
    let mut at = 0;
    while at < words.len() {
        let word = words[at].as_str();
        match word {
            "--text" if !text => text = true,
            "--quick" | "--write-test" => {
                let name = if word == "--quick" {
                    "quick"
                } else {
                    "write_test"
                };
                if fields.iter().any(|(k, _)| k == name) {
                    return Err("duplicate doctor option".to_owned());
                }
                fields.push((name.to_owned(), SdkValue::Bool(true)));
            }
            "--timeout" => {
                at += 1;
                let value = words.get(at).ok_or("--timeout needs a duration")?;
                timeout(&mut fields, value)?;
            }
            w if w.starts_with("--timeout=") => timeout(&mut fields, &w[10..])?,
            w if w.starts_with('-') => return Err(format!("unknown doctor option `{w}`")),
            w => positional.push(w),
        }
        at += 1;
    }
    let (target, name) = match positional.as_slice() {
        [] if fields.is_empty() => return Ok((SdkValue::Null, text)),
        ["host"] => ("host", None),
        ["machine", name] => ("machine", Some(*name)),
        ["sandbox", name] => ("sandbox", Some(*name)),
        ["store", "integrity", name] => ("store.integrity", Some(*name)),
        ["store", "connection", name] => ("store.connection", Some(*name)),
        ["backup", "connection", name] => ("backup.connection", Some(*name)),
        _ => {
            return Err(
                "use `layerfs doctor --help` for diagnostic targets and arguments".to_owned(),
            )
        }
    };
    fields.push(("target".to_owned(), SdkValue::text(target)));
    if let Some(name) = name {
        fields.push(("name".to_owned(), SdkValue::text(name)));
    }
    Ok((SdkValue::Map(fields), text))
}
fn timeout(fields: &mut Vec<(String, SdkValue)>, value: &str) -> Result<(), String> {
    if fields.iter().any(|(k, _)| k == "timeout") {
        return Err("duplicate --timeout".to_owned());
    }
    fields.push(("timeout".to_owned(), SdkValue::text(value)));
    Ok(())
}

/// Help at each explicit diagnostic nesting level.
pub fn help(topic: &[&str]) -> ExitCode {
    let common = " [--timeout <duration>] [--text]\n";
    let lines: &[&str] = match topic {
        [] => &[
            "layerfs doctor [--text]\n",
            "layerfs doctor host",
            "layerfs doctor machine <machine>",
            "layerfs doctor sandbox <sandbox>",
            "layerfs doctor store integrity <store> [--quick]",
            "layerfs doctor store connection <store>",
            "layerfs doctor backup connection <bucket> [--write-test]",
        ],
        ["host"] => &["layerfs doctor host"],
        ["machine"] => &["layerfs doctor machine <machine>"],
        ["sandbox"] => &["layerfs doctor sandbox <sandbox>"],
        ["store"] => &[
            "layerfs doctor store integrity <store> [--quick]",
            "layerfs doctor store connection <store>",
        ],
        ["store", "integrity"] => &["layerfs doctor store integrity <store> [--quick]"],
        ["store", "connection"] => &["layerfs doctor store connection <store>"],
        ["backup"] | ["backup", "connection"] => {
            &["layerfs doctor backup connection <bucket> [--write-test]"]
        }
        _ => return run::wrong("unknown doctor help topic".to_owned()),
    };
    let mut text = String::new();
    for line in lines {
        text.push_str(line);
        if !line.ends_with('\n') {
            text.push_str(common);
        }
    }
    text.push_str("\nObservations only; Store integrity is full by default. Connection checks do not prove integrity.\n");
    text.push_str("Timeouts use ms/s/m/h (e.g. 500ms, 10s, 2m). Inspect complete and check coverage; a report exits 0 even with findings.\n");
    output::print(&text)
}

//! One command line, start to exit code.
use crate::parse::{parse, Usage};
use crate::{help, output, worker};
use layerfs_api::Scope;
use layerfs_bridge::control::SdkValue;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;

/// The command line is wrong; nothing was called.
pub const USAGE: u8 = 2;
/// LayerFS refused or failed the call.
pub const REFUSED: u8 = 125;

/// Runs `words` (the arguments after `layerfs`).
pub fn main(words: Vec<String>) -> ExitCode {
    match words.first().map(String::as_str) {
        None | Some("help" | "-h" | "--help") => {
            let topic: Vec<&str> = words.iter().skip(1).map(String::as_str).collect();
            help::print(&topic)
        }
        Some("--version") => match &words[1..] {
            [] => output::print(&format!("layerfs {}\n", env!("CARGO_PKG_VERSION"))),
            [flag] if flag == "--json" => crate::capabilities::version(),
            _ => wrong("use layerfs --version [--json]".to_owned()),
        },
        Some("worker") if words.len() == 1 => worker::main(),
        Some(family) => {
            let Some(name) = words.get(1) else {
                return help::print(&[family]);
            };
            if words[2..].iter().any(|w| w == "-h" || w == "--help") {
                return help::print(&[family, name]);
            }
            // a call's shell name has `-` for its `_` (`workspace exec-cancel`)
            match layerfs_api::find(&format!("{family}.{}", name.replace('-', "_"))) {
                Some(call) => one(call, &words[2..]),
                None => match layerfs_api::families().iter().find(|f| f.name == family) {
                    Some(fam) => {
                        let target = name.replace('-', "_");
                        let calls = fam.calls().iter().map(|c| c.call);
                        let suggestion = match closest(&target, calls, 3) {
                            Some(call) => format!(
                                "; did you mean `layerfs {family} {}`?",
                                call.replace('_', "-")
                            ),
                            None => String::new(),
                        };
                        wrong(format!(
                            "no call `{family} {name}`; `layerfs help {family}` lists them{suggestion}"
                        ))
                    }
                    None => {
                        let names = layerfs_api::families().iter().map(|f| f.name);
                        let suggestion = match closest(family, names, 2) {
                            Some(best) => format!("; did you mean `layerfs {best}`?"),
                            None => String::new(),
                        };
                        wrong(format!(
                            "no command `{family}`; `layerfs help` lists them{suggestion}"
                        ))
                    }
                },
            }
        }
    }
}

/// The name in `names` nearest `target` (the first of the nearest), when
/// it is at most `max` edits away.
fn closest<'a>(target: &str, names: impl Iterator<Item = &'a str>, max: usize) -> Option<&'a str> {
    let mut best: Option<(usize, &str)> = None;
    for name in names {
        let distance = lev(target, name);
        if best.is_none_or(|(d, _)| distance < d) {
            best = Some((distance, name));
        }
    }
    best.filter(|(d, _)| *d <= max).map(|(_, name)| name)
}

fn lev(a: &str, b: &str) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        prev.copy_from_slice(&curr);
    }
    prev[b.len()]
}

pub fn wrong(message: String) -> ExitCode {
    output::diagnostic(&format!("layerfs: {message}"));
    ExitCode::from(USAGE)
}

/// One call of the table, made once.
fn one(call: &layerfs_api::Call, words: &[String]) -> ExitCode {
    let parsed = match parse(call, words, &mut std::io::stdin()) {
        Ok(parsed) => parsed,
        Err(Usage(message)) => return wrong(message),
    };
    let switches = parsed.switches;
    let scope = match switches.scope.as_ref().map(Scope::parse).transpose() {
        Ok(scope) => scope,
        Err(refusal) => return output::refusal(&refusal, switches.detail),
    };
    let (progress, stream) = (switches.progress, switches.stream);
    let is_term =
        std::io::stderr().is_terminal() && std::env::var_os("LAYERFS_PROGRESS_JSON").is_none();
    let mut last_progress_len = 0;
    let mut progress_start: Option<std::time::Instant> = None;
    let mut send = |event: SdkValue| {
        if progress && event.get("progress").is_some() {
            let mut err = std::io::stderr().lock();
            if is_term {
                let p = event.get("progress").unwrap();
                let phase = p.get_text("phase").unwrap_or("");
                let done = p.get_int("done");
                let total = p.get_int("total");
                let now = std::time::Instant::now();
                let start = *progress_start.get_or_insert(now);
                let elapsed = (now - start).as_secs_f64();
                let status = output::progress_line(phase, done, total, elapsed);
                let len = status.chars().count();
                let pad = if last_progress_len > len {
                    " ".repeat(last_progress_len - len)
                } else {
                    String::new()
                };
                last_progress_len = len;
                let _ = write!(err, "\r{status}{pad}");
                let _ = err.flush();
            } else if let Some(progress) = event.get("progress") {
                // one event, one line: `{"phase","done","total"}` and what
                // else the call sends beside them
                let _ = writeln!(err, "{}", layerfs_api::json::print(progress));
            }
        } else if stream {
            let bytes = match event.get("bytes") {
                Some(SdkValue::Bytes(bytes)) => bytes.as_slice(),
                Some(SdkValue::Text(text)) => text.as_bytes(),
                _ => return,
            };
            let _ = match event.get_text("stream") {
                Some("stdout") => std::io::stdout().lock().write_all(bytes),
                Some("stderr") => std::io::stderr().lock().write_all(bytes),
                _ => Ok(()),
            };
        }
    };
    let outcome = layerfs_api::call_scoped(
        None,
        scope.as_ref(),
        &call.name(),
        parsed.args,
        &mut send,
        layerfs_api::Client::Cli,
    );
    if is_term && last_progress_len > 0 {
        let mut err = std::io::stderr().lock();
        let _ = write!(err, "\r{}\r", " ".repeat(last_progress_len));
        let _ = err.flush();
    }
    // the Stores this one call opened close and its idle Sandbox daemon
    // sessions end before the process does; bounded, the result is ready
    layerfs_api::close();
    // `workspace exec` exits with the command's own code in every form
    // (call surface 2.0); its result JSON is still printed, except with
    // `--stream`, which wrote the output itself
    let exits = call.arg("stream").is_some();
    match outcome {
        Err(refusal) => output::refusal(&refusal, switches.detail),
        Ok(value) if exits => {
            let printed = match stream {
                true => ExitCode::SUCCESS,
                false => output::result(&value, switches.text),
            };
            let _ = std::io::stdout().flush();
            // a result with none is a plain success (125 is for refusals)
            match value.get_int("exit_code") {
                None => printed,
                Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
            }
        }
        // a doctor's report reads as its findings and what it could not check
        Ok(value) if switches.text && call.call == "doctor" => output::doctor(&value),
        Ok(value) => output::result(&value, switches.text),
    }
}

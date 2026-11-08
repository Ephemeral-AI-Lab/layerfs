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
        Some("import-env") if words.len() == 1 => import_env(),
        Some("doctor") => crate::doctor::main(&words[1..]),
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
                None if layerfs_api::families().iter().any(|f| f.name == family) => {
                    let fam = layerfs_api::families()
                        .iter()
                        .find(|f| f.name == family)
                        .unwrap();
                    let target = name.replace('-', "_");
                    let mut best_dist = usize::MAX;
                    let mut best_call = "";
                    for c in fam.calls() {
                        let d = lev(&target, c.call);
                        if d < best_dist {
                            best_dist = d;
                            best_call = c.call;
                        }
                    }
                    let suggestion = if best_dist <= 3 {
                        format!(
                            "; did you mean `layerfs {family} {}`?",
                            best_call.replace('_', "-")
                        )
                    } else {
                        String::new()
                    };
                    wrong(format!(
                        "no call `{family} {name}`; `layerfs help {family}` lists them{suggestion}"
                    ))
                }
                None => {
                    let mut best_dist = usize::MAX;
                    let mut best_fam = "";
                    for f in layerfs_api::families() {
                        let d = lev(family, f.name);
                        if d < best_dist {
                            best_dist = d;
                            best_fam = f.name;
                        }
                    }
                    let suggestion = if best_dist <= 2 {
                        format!("; did you mean `layerfs {best_fam}`?")
                    } else {
                        String::new()
                    };
                    wrong(format!(
                        "no command `{family}`; `layerfs help` lists them{suggestion}"
                    ))
                }
            }
        }
    }
}

/// `layerfs import-env`: this environment's 1.1.0 settings as records, once.
fn import_env() -> ExitCode {
    let env: Vec<(String, String)> = std::env::vars().collect();
    let outcome =
        layerfs_api::home(None).and_then(|home| layerfs_api::home::import_env(&home, &env));
    match outcome {
        Ok(value) => output::result(&value, false),
        Err(refusal) => output::refusal(&refusal),
    }
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
        Err(refusal) => return output::refusal(&refusal),
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
                let mut status = format!("phase: {phase}");
                if let Some(d) = done {
                    let d_str = output::format_size(d);
                    if let Some(t) = total {
                        let t_str = output::format_size(t);
                        status.push_str(&format!("  {d_str}/{t_str}"));
                    } else {
                        status.push_str(&format!("  {d_str}"));
                    }
                    if elapsed > 0.5 && d > 0 {
                        let rate = (d as f64) / elapsed;
                        status.push_str(&format!("  ({}/s)", output::format_size(rate as i64)));
                    }
                }
                let len = status.chars().count();
                let pad = if last_progress_len > len {
                    " ".repeat(last_progress_len - len)
                } else {
                    String::new()
                };
                last_progress_len = len;
                let _ = write!(err, "\r{status}{pad}");
                let _ = err.flush();
            } else {
                let _ = writeln!(err, "{}", layerfs_api::json::print(&event));
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
    let outcome =
        layerfs_api::call_scoped(None, scope.as_ref(), &call.name(), parsed.args, &mut send);
    if is_term && last_progress_len > 0 {
        let mut err = std::io::stderr().lock();
        let _ = write!(err, "\r{}\r", " ".repeat(last_progress_len));
        let _ = err.flush();
    }
    // the Stores this one call opened close before the process ends, so a
    // sqlite Store leaves no write-ahead log behind (lane K2: a later
    // `store push` refused it)
    layerfs_api::close_idle(std::time::Duration::ZERO);
    match outcome {
        Err(refusal) => output::refusal(&refusal),
        Ok(value) if stream => {
            let _ = std::io::stdout().flush();
            // the command's own exit code, as 1.1.0 passed it through; a
            // result with none is a plain success (125 is for refusals)
            match value.get_int("exit_code") {
                None => ExitCode::SUCCESS,
                Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
            }
        }
        Ok(value) => output::result(&value, switches.text),
    }
}

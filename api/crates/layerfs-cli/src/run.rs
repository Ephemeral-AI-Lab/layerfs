//! One command line, start to exit code.
use crate::parse::{parse, Usage};
use crate::{help, output, worker};
use layerfs_api::calls::{self, Scope};
use layerfs_bridge::control::SdkValue;
use std::io::Write;
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
        Some("--version") => {
            println!("layerfs {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("worker") if words.len() == 1 => worker::main(),
        Some("import-env") if words.len() == 1 => import_env(),
        Some(family) => {
            let Some(name) = words.get(1) else {
                return help::print(&[family]);
            };
            if words[2..].iter().any(|w| w == "-h" || w == "--help") {
                return help::print(&[family, name]);
            }
            match calls::find(&format!("{family}.{name}")) {
                Some(call) => one(call, &words[2..]),
                None if calls::FAMILIES.contains(&family) => wrong(format!(
                    "no call `{family} {name}`; `layerfs help {family}` lists them"
                )),
                None => wrong(format!("no command `{family}`; `layerfs help` lists them")),
            }
        }
    }
}

/// `layerfs import-env`: this environment's 1.1.0 settings as records, once.
fn import_env() -> ExitCode {
    let env: Vec<(String, String)> = std::env::vars().collect();
    let outcome = calls::home(None).and_then(|home| layerfs_api::home::import_env(&home, &env));
    match outcome {
        Ok(value) => output::result(&value, false),
        Err(refusal) => output::refusal(&refusal),
    }
}

fn wrong(message: String) -> ExitCode {
    eprintln!("layerfs: {message}");
    ExitCode::from(USAGE)
}

/// One call of the table, made once.
fn one(call: &calls::Call, words: &[String]) -> ExitCode {
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
    let mut send = |event: SdkValue| {
        if progress && event.get("progress").is_some() {
            let mut err = std::io::stderr().lock();
            let _ = writeln!(err, "{}", calls::json::print(&event));
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
    let outcome = calls::call_scoped(None, scope.as_ref(), &call.name(), parsed.args, &mut send);
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

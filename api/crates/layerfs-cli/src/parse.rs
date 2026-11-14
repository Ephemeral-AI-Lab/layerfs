//! The command line of one call, read against its table entry (S §2.1,
//! §2.2): positionals in the table's order, then `--options`. A content or
//! JSON positional (`file write`'s content, `file apply`'s changes) is read
//! from stdin, or from `--from <file>`; a text option `--x` also takes
//! `--x-from <file>`. Anything wrong here is exit 2, before any call.
use layerfs_api::{json, Arg, Call, Kind};
use layerfs_bridge::control::SdkValue;
use std::io::Read;

/// A command line that is wrong; nothing is called.
pub struct Usage(pub String);

/// The front-end switches that are not call arguments.
#[derive(Default)]
pub struct Switches {
    /// `--text`: a readable result instead of JSON.
    pub text: bool,
    /// `--progress`: progress as JSON lines on stderr.
    pub progress: bool,
    /// `--stream` (`workspace exec`): the command's output and exit code
    /// passed through.
    pub stream: bool,
    /// Framed output events followed by one result or refusal on stdout.
    pub events: bool,
    /// `--scope <folder>` with `--scope-project <store>/<project>` and
    /// `--scope-branch <name>` (O32).
    pub scope: Option<SdkValue>,
}

/// One parsed call: its named arguments and the switches.
pub struct Parsed {
    pub args: SdkValue,
    pub switches: Switches,
}

fn usage(message: impl Into<String>) -> Usage {
    Usage(message.into())
}

/// Whether the call reads this positional from stdin or `--from`.
fn piped(arg: &Arg) -> bool {
    matches!(arg.kind, Kind::Content | Kind::Json)
}

/// A shell word as the value an argument of `kind` takes.
fn value(call: &Call, arg: &Arg, word: &str) -> Result<SdkValue, Usage> {
    let what = || format!("{}: `{}`", call.name(), arg.name.replace('_', "-"));
    match arg.kind {
        Kind::Int => word
            .parse::<i64>()
            .map(SdkValue::Int)
            .map_err(|_| usage(format!("{} must be a whole number", what()))),
        Kind::Json => {
            json::parse(word).map_err(|why| usage(format!("{} is not JSON: {}", what(), why.0)))
        }
        Kind::Size => Ok(match word.parse::<i64>() {
            Ok(n) => SdkValue::Int(n),
            Err(_) => SdkValue::text(word),
        }),
        Kind::Bool => match word {
            "true" => Ok(SdkValue::Bool(true)),
            "false" => Ok(SdkValue::Bool(false)),
            _ => Err(usage(format!("{} is true or false", what()))),
        },
        _ => Ok(SdkValue::text(word)),
    }
}

/// Bytes as text when they are UTF-8, else as bytes.
fn content(bytes: Vec<u8>) -> SdkValue {
    match String::from_utf8(bytes) {
        Ok(text) => SdkValue::Text(text),
        Err(e) => SdkValue::Bytes(e.into_bytes()),
    }
}

fn read_file(path: &str) -> Result<Vec<u8>, Usage> {
    std::fs::read(path).map_err(|why| usage(format!("cannot read {path}: {why}")))
}

/// Reads `call`'s command line `words`; `stdin` is read only when a piped
/// positional needs it and no `--from` was given.
pub fn parse(call: &Call, words: &[String], stdin: &mut dyn Read) -> Result<Parsed, Usage> {
    let mut switches = Switches::default();
    let mut positionals: Vec<&str> = Vec::new();
    let mut fields: Vec<(String, SdkValue)> = Vec::new();
    let mut from: Option<String> = None;
    let mut scope_folder: Option<String> = None;
    let mut scope_branch: Option<String> = None;
    let mut scope_project: Option<String> = None;
    let mut i = 0;
    let mut only_positionals = false;
    while i < words.len() {
        let word = &words[i];
        i += 1;
        if only_positionals || !word.starts_with('-') || word == "-" {
            positionals.push(word);
            continue;
        }
        if word == "--" {
            only_positionals = true;
            continue;
        }
        let (flag, inline) = match word.split_once('=') {
            Some((flag, inline)) if flag.starts_with("--") => (flag, Some(inline.to_owned())),
            _ => (word.as_str(), None),
        };
        let take = |i: &mut usize| -> Result<String, Usage> {
            if let Some(inline) = inline.clone() {
                return Ok(inline);
            }
            let next = words
                .get(*i)
                .ok_or_else(|| usage(format!("{flag} needs a value")))?;
            *i += 1;
            Ok(next.clone())
        };
        match flag {
            "--text" => switches.text = true,
            "--events" if call.arg("stdout").is_some() => switches.events = true,
            "--progress" if call.arg("on_progress").is_some() => switches.progress = true,
            "--stream" if call.arg("stream").is_some() => {
                switches.stream = true;
                fields.push(("stream".to_owned(), SdkValue::Bool(true)));
            }
            "--from" if call.args.iter().any(piped) => from = Some(take(&mut i)?),
            "--scope" => scope_folder = Some(take(&mut i)?),
            "--scope-branch" => scope_branch = Some(take(&mut i)?),
            "--scope-project" => scope_project = Some(take(&mut i)?),
            "-m" => {
                let text = take(&mut i)?;
                option(call, "message", &text, &mut fields)?;
            }
            _ => {
                let fail = || {
                    let mut accepted = Vec::new();
                    if call.options.iter().any(|a| a.name == "message") {
                        accepted.push("-m".to_owned());
                    }
                    if call.args.iter().any(piped) {
                        accepted.push("--from".to_owned());
                    }
                    if call.arg("on_progress").is_some() {
                        accepted.push("--progress".to_owned());
                    }
                    if call.arg("stream").is_some() {
                        accepted.push("--stream".to_owned());
                    }
                    accepted.push("--text".to_owned());
                    for opt in call.options {
                        if opt.kind != Kind::Function && opt.name != "message" {
                            accepted.push(format!("--{}", opt.name.replace('_', "-")));
                        }
                    }
                    if accepted.is_empty() {
                        usage(format!(
                            "{}: unknown option {flag}; this call takes no options",
                            call.name()
                        ))
                    } else {
                        usage(format!(
                            "{}: unknown option {flag}\naccepted options: {}",
                            call.name(),
                            accepted.join(", ")
                        ))
                    }
                };
                let Some(name) = flag.strip_prefix("--") else {
                    return Err(fail());
                };
                let name = name.replace('-', "_");
                if let Some(base) = name.strip_suffix("_from") {
                    if matches!(call.arg(base), Some(a) if a.optional && a.kind == Kind::Text) {
                        let bytes = read_file(&take(&mut i)?)?;
                        fields.push((base.to_owned(), content(bytes)));
                        continue;
                    }
                }
                match call.options.iter().find(|a| a.name == name) {
                    Some(arg) if arg.kind == Kind::Bool && inline.is_none() => {
                        fields.push((name, SdkValue::Bool(true)))
                    }
                    Some(arg) if arg.kind != Kind::Function => {
                        let text = take(&mut i)?;
                        fields.push((name, value(call, arg, &text)?));
                    }
                    _ => return Err(fail()),
                }
            }
        }
    }
    if switches.events && (switches.stream || switches.text) {
        return Err(usage("--events is exclusive with --stream and --text"));
    }
    if (scope_branch.is_some() || scope_project.is_some()) && scope_folder.is_none() {
        return Err(usage("--scope-branch and --scope-project need --scope"));
    }
    if let Some(folder) = scope_folder {
        let Some(project) = scope_project else {
            return Err(usage("--scope needs --scope-project <store>/<project>"));
        };
        let mut scope = vec![
            ("project".to_owned(), SdkValue::Text(project)),
            ("folder".to_owned(), SdkValue::Text(folder)),
        ];
        if let Some(branch) = scope_branch {
            scope.push(("branch".to_owned(), SdkValue::Text(branch)));
        }
        switches.scope = Some(SdkValue::Map(scope));
    }
    let on_line: Vec<&Arg> = call.args.iter().filter(|a| !piped(a)).collect();
    let required = on_line.iter().filter(|a| !a.optional).count();
    if positionals.len() < required || positionals.len() > on_line.len() {
        return Err(usage(format!(
            "{} takes {}{} positional arguments, not {}",
            call.name(),
            required,
            if on_line.len() > required {
                format!(" to {}", on_line.len())
            } else {
                String::new()
            },
            positionals.len()
        )));
    }
    for (arg, word) in on_line.iter().zip(&positionals) {
        fields.push((arg.name.to_owned(), value(call, arg, word)?));
    }
    for arg in call.args.iter().filter(|a| piped(a)) {
        let bytes = match &from {
            Some(path) => read_file(path)?,
            None => {
                let mut bytes = Vec::new();
                stdin
                    .read_to_end(&mut bytes)
                    .map_err(|why| usage(format!("cannot read stdin: {why}")))?;
                bytes
            }
        };
        let piped_value = if arg.kind == Kind::Json {
            let text = String::from_utf8(bytes).map_err(|_| usage("stdin is not UTF-8 JSON"))?;
            json::parse(&text).map_err(|why| usage(format!("stdin is not JSON: {}", why.0)))?
        } else {
            content(bytes)
        };
        fields.push((arg.name.to_owned(), piped_value));
    }
    Ok(Parsed {
        args: SdkValue::Map(fields),
        switches,
    })
}

fn option(
    call: &Call,
    name: &str,
    text: &str,
    fields: &mut Vec<(String, SdkValue)>,
) -> Result<(), Usage> {
    let arg = call
        .options
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| usage(format!("{} takes no {name}", call.name())))?;
    fields.push((name.to_owned(), value(call, arg, text)?));
    Ok(())
}

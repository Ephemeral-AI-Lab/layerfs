//! The command line of one call, read against its table entry (S §2.1,
//! §2.2): positionals in the table's order, then `--options`. A content or
//! JSON positional (`file write`'s content, `file apply`'s changes) is read
//! from stdin, or from `--from <file>`; a text option `--x` also takes
//! `--x-from <file>`. A secret option (`password`) is read from stdin when
//! its own flag is given (`--password-stdin`) and taken no other way.
//! Anything wrong here is exit 2, before any call.
use layerfs_api::{json, Arg, Call, Kind};
use layerfs_bridge::control::SdkValue;
use std::io::Read;

/// A command line that is wrong; nothing is called.
pub struct Usage(pub String);

/// The front-end switches that are not call arguments.
#[derive(Default)]
pub struct Switches {
    /// `--text`: the readable form, also when stdout is no terminal.
    pub text: bool,
    /// `--json`: one line of JSON, also when stdout is a terminal.
    pub json: bool,
    /// `--progress`: progress as JSON lines on stderr.
    pub progress: bool,
    /// `--stream` (`workspace exec`): the command's output passed through.
    pub stream: bool,
    /// `--scope <folder>` with `--scope-project <store>/<project>` and
    /// `--scope-branch <name>` (O32).
    pub scope: Option<SdkValue>,
    /// `--detail`: the long form, of the result and of a refusal.
    pub detail: bool,
}

/// One parsed call: its named arguments and the switches.
pub struct Parsed {
    pub args: SdkValue,
    pub switches: Switches,
    /// A write's input left unread for the call's route to read
    /// (`file write`'s content, an unscoped `file apply`'s changes).
    pub input: Option<layerfs_api::CliInput>,
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
    parse_with(call, words, stdin, false)
}

/// [`parse`] of this process's command line: a write input whose Store may
/// take the host worker route is opened but left unread (`Parsed::input`).
pub fn parse_cli(call: &Call, words: &[String]) -> Result<Parsed, Usage> {
    parse_with(call, words, &mut std::io::stdin(), true)
}

fn parse_with(
    call: &Call,
    words: &[String],
    stdin: &mut dyn Read,
    may_defer: bool,
) -> Result<Parsed, Usage> {
    let mut switches = Switches::default();
    let mut positionals: Vec<&str> = Vec::new();
    let mut fields: Vec<(String, SdkValue)> = Vec::new();
    let mut from: Option<String> = None;
    let mut scope_folder: Option<String> = None;
    let mut scope_branch: Option<String> = None;
    let mut scope_project: Option<String> = None;
    let mut secret: Option<&Arg> = None;
    let mut i = 0;
    let mut only_positionals = false;
    // the options given so far, to refuse one given twice
    let mut given: Vec<String> = Vec::new();
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
        once(call, flag, &mut given)?;
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
            "--json" => switches.json = true,
            // the global options of every call (call surface 2.0): the
            // library takes them out of the arguments and checks them
            "--detail" if inline.is_none() => {
                switches.detail = true;
                fields.push((layerfs_api::DETAIL.to_owned(), SdkValue::Bool(true)));
            }
            // `--with` may be given more than once: its sections add up
            "--with" => {
                let sections = take(&mut i)?;
                sections_known(call, &sections)?;
                match fields
                    .iter_mut()
                    .find(|(name, _)| name == layerfs_api::WITH)
                {
                    Some((_, SdkValue::Text(held))) => {
                        held.push(',');
                        held.push_str(&sections);
                    }
                    _ => fields.push((layerfs_api::WITH.to_owned(), SdkValue::Text(sections))),
                }
            }
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
                    accepted.push("--text".to_owned());
                    accepted.push("--json".to_owned());
                    accepted.push("--detail".to_owned());
                    if !call.with.is_empty() {
                        accepted.push("--with".to_owned());
                    }
                    for opt in call.options {
                        if let Some(flag) = opt.shell {
                            accepted.push(flag.to_owned());
                        } else if opt.kind != Kind::Function && opt.name != "message" {
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
                // a secret's own flag: its value is all of stdin
                if let Some(arg) = call.options.iter().find(|a| a.shell == Some(flag)) {
                    if inline.is_some() {
                        return Err(usage(format!(
                            "{}: {flag} takes no value; the value is read from stdin",
                            call.name()
                        )));
                    }
                    secret = Some(arg);
                    continue;
                }
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
                    Some(arg) if arg.shell.is_some() => {
                        return Err(usage(format!(
                            "{}: `{}` is read from stdin only; pass {}",
                            call.name(),
                            arg.name.replace('_', "-"),
                            arg.shell.unwrap_or_default()
                        )));
                    }
                    Some(arg) if arg.kind == Kind::Bool && inline.is_none() => {
                        fields.push((name, SdkValue::Bool(true)))
                    }
                    // a content option (`workspace exec --stdin`): this
                    // process's stdin, read whole
                    Some(arg) if arg.kind == Kind::Content && inline.is_none() => {
                        let mut bytes = Vec::new();
                        stdin
                            .read_to_end(&mut bytes)
                            .map_err(|why| usage(format!("cannot read stdin: {why}")))?;
                        fields.push((name, content(bytes)));
                    }
                    // a repeated option (`--env K=V`): one object of names
                    // to text
                    Some(arg) if arg.repeat => {
                        let text = take(&mut i)?;
                        let Some((key, value)) = text.split_once('=') else {
                            return Err(usage(format!(
                                "{}: `--{}` is K=V, not {text:?}",
                                call.name(),
                                arg.name.replace('_', "-")
                            )));
                        };
                        let entry = (key.to_owned(), SdkValue::text(value));
                        match fields.iter_mut().find(|(held, _)| *held == name) {
                            Some((_, SdkValue::Map(held))) => {
                                held.retain(|(k, _)| k != key);
                                held.push(entry);
                            }
                            _ => fields.push((name, SdkValue::Map(vec![entry]))),
                        }
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
    if switches.text && switches.json {
        return Err(usage("--text and --json ask for two forms; give one"));
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
    let named = format!("{} {}", call.family, call.call.replace('_', "-"));
    if positionals.len() < required {
        let missing: Vec<String> = (on_line.iter().skip(positionals.len()))
            .filter(|arg| !arg.optional)
            .map(|arg| format!("<{}>", arg.name.replace('_', "-")))
            .collect();
        return Err(usage(format!("{named} is missing {}", missing.join(" "))));
    }
    if positionals.len() > on_line.len() {
        let extra: Vec<&str> = positionals[on_line.len()..].to_vec();
        let (many, verb) = match extra.len() {
            1 => ("one too many", "is"),
            _ => ("too many", "are"),
        };
        return Err(usage(format!(
            "{named} takes {} argument{}; `{}` {verb} {many}",
            on_line.len(),
            if on_line.len() == 1 { "" } else { "s" },
            extra.join(" ")
        )));
    }
    for (arg, word) in on_line.iter().zip(&positionals) {
        fields.push((arg.name.to_owned(), value(call, arg, word)?));
    }
    if let Some(arg) = secret {
        if from.is_none() && call.args.iter().any(piped) {
            return Err(usage(format!(
                "{}: stdin holds `{}`; give the content with --from <file>",
                call.name(),
                arg.name.replace('_', "-")
            )));
        }
        let mut bytes = Vec::new();
        stdin
            .read_to_end(&mut bytes)
            .map_err(|why| usage(format!("cannot read stdin: {why}")))?;
        // one line ending is the terminal's or `echo`'s, not the secret's
        if bytes.ends_with(b"\n") {
            bytes.pop();
            if bytes.ends_with(b"\r") {
                bytes.pop();
            }
        }
        fields.push((arg.name.to_owned(), content(bytes)));
    }
    for arg in call.options.iter().filter(|a| a.required) {
        if !fields.iter().any(|(name, _)| name == arg.name) {
            return Err(usage(format!(
                "{}: {} is required",
                call.name(),
                flag_of(arg)
            )));
        }
    }
    let mut input = None;
    for arg in call.args.iter().filter(|a| piped(a)) {
        // a scoped apply checks every change's path before the call
        let scoped = switches.scope.is_some() && arg.kind == Kind::Json;
        let project = fields.iter().find(|(name, _)| name == "project");
        let defer = may_defer
            && secret.is_none()
            && !scoped
            && matches!(project, Some((_, SdkValue::Text(project)))
                if layerfs_api::cli_defers(&call.name(), project));
        if defer {
            let source: Box<dyn Read + Send> = match &from {
                Some(path) => Box::new(
                    std::fs::File::open(path)
                        .map_err(|why| usage(format!("cannot read {path}: {why}")))?,
                ),
                None => Box::new(std::io::stdin()),
            };
            let (kind, placeholder) = match arg.kind {
                Kind::Json => (
                    layerfs_api::CliInputKind::Changes,
                    SdkValue::List(Vec::new()),
                ),
                _ => (
                    layerfs_api::CliInputKind::Content,
                    SdkValue::Bytes(Vec::new()),
                ),
            };
            input = Some(layerfs_api::CliInput { kind, source });
            fields.push((arg.name.to_owned(), placeholder));
            continue;
        }
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
        input,
    })
}

/// The shell flag that gives option `arg`: its stdin flag for a secret.
fn flag_of(arg: &Arg) -> String {
    match arg.shell {
        Some(flag) => flag.to_owned(),
        None => format!("--{}", arg.name.replace('_', "-")),
    }
}

/// Refuses a `--with` that names a section the call does not take: the
/// table says which it takes, so this is a wrong command line.
fn sections_known(call: &Call, sections: &str) -> Result<(), Usage> {
    let names = sections.split(',').map(str::trim).filter(|n| !n.is_empty());
    for name in names {
        if call.section(name).is_some() {
            continue;
        }
        let takes: Vec<&str> = call.with.iter().map(|section| section.name).collect();
        return Err(usage(match takes.is_empty() {
            true => format!(
                "{}: --with names `{name}`, and this call takes no section",
                call.name()
            ),
            false => format!(
                "{}: --with names `{name}`; this call takes {}",
                call.name(),
                takes.join(", ")
            ),
        }));
    }
    Ok(())
}

/// Refuses option `flag` given a second time, naming it: only `--with`
/// (its sections add up) and an option the row marks `repeat` (`--env`)
/// may be given more than once (RF re-check N-F). `-m` is `--message`.
fn once(call: &Call, flag: &str, given: &mut Vec<String>) -> Result<(), Usage> {
    let flag = match flag {
        "-m" => "--message",
        flag => flag,
    };
    let repeat = flag.strip_prefix("--").is_some_and(|name| {
        let name = name.replace('-', "_");
        (call.options.iter()).any(|arg| arg.name == name && arg.repeat)
    });
    if flag == "--with" || repeat {
        return Ok(());
    }
    if given.iter().any(|seen| seen == flag) {
        return Err(usage(format!(
            "{}: {flag} is given more than once; give it once",
            call.name()
        )));
    }
    given.push(flag.to_owned());
    Ok(())
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

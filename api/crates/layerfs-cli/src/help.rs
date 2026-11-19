//! `layerfs help [family [call]]`, from the call table and the words beside
//! it ([`layerfs_api::guide`]): the concepts, a first session and the
//! families; one family's calls; one call's arguments, options, answer and
//! example.
use crate::banner::bold;
use crate::output;
use layerfs_api::{guide, Arg, Call, Kind};
use std::process::ExitCode;

fn shell(name: &str) -> String {
    name.replace('_', "-")
}

/// The families under the headings the top help groups them in.
const GROUPS: [(&str, &[&str]); 3] = [
    (
        "Files and history",
        &["project", "branch", "commit", "file"],
    ),
    ("Run commands", &["sandbox", "workspace"]),
    (
        "Set up and look after",
        &["home", "machine", "store", "backup"],
    ),
];

const CONCEPTS: &[&str] = &[
    "Project     files with history, kept in a Store; written <store>/<project>",
    "Branch      a line of Commits; main moves only by `branch publish`",
    "Commit      a saved tree; its id, or a prefix of 8 or more characters",
    "Sandbox     an isolated place to run commands for one project",
    "Workspace   a Branch mounted in a Sandbox; written <sandbox>:<branch>",
    "Machine     where Stores and Sandboxes live: native, docker, lima, apple-container",
    "Store       the database that holds projects: sqlite or postgresql",
];

const START: &[&str] = &[
    "layerfs home init",
    "layerfs store create shop",
    "layerfs project init shop/web ~/code/web",
    "layerfs branch fork shop/web work",
    "echo hi | layerfs file write shop/web work /hello.txt -m \"say hi\"",
    "layerfs branch publish shop/web work",
    "layerfs sandbox create shop/web box --mount work",
    "layerfs workspace exec box:work \"npm test\"",
    "layerfs workspace commit box:work -m \"tests pass\"",
];

const OPTIONS: &[&str] = &[
    "--text, --json   readable text (the default in a terminal) or one line of JSON (the default when piped)",
    "--detail         every key of the result",
    "--with a,b       add the sections a call lists",
    "-h, --help       help for a family or a call",
];

/// One call's usage line.
pub fn usage(call: &Call) -> String {
    let mut line = format!("layerfs {} {}", call.family, shell(call.call));
    for arg in call.args {
        if matches!(arg.kind, Kind::Content | Kind::Json) {
            continue;
        }
        // as the rows write them: `<name>`, and `[name]` when optional
        let word = match arg.optional {
            true => format!("[{}]", shell(arg.name)),
            false => format!("<{}>", shell(arg.name)),
        };
        line.push(' ');
        line.push_str(&word);
    }
    for arg in call.options {
        if let Some(word) = option_word(call, arg) {
            match (arg.required, arg.repeat) {
                (true, _) => line.push_str(&format!(" {word}")),
                (false, true) => line.push_str(&format!(" [{word}]...")),
                (false, false) => line.push_str(&format!(" [{word}]")),
            }
        }
    }
    if piped(call) {
        line.push_str(" [--from <file>] (content from stdin)");
    }
    if !call.with.is_empty() {
        let sections: Vec<&str> = call.with.iter().map(|section| section.name).collect();
        line.push_str(&format!(" [--with {}]", sections.join(",")));
    }
    line
}

/// Whether the call reads a positional from stdin or `--from`.
fn piped(call: &Call) -> bool {
    (call.args.iter()).any(|a| matches!(a.kind, Kind::Content | Kind::Json))
}

/// One option as the shell writes it: a value word as the call's row shows
/// it ([`crate::words`]); a Commit message by its short flag, `-m msg`;
/// none for an exec sink, which the shell does not take.
fn option_word(call: &Call, arg: &Arg) -> Option<String> {
    Some(match (arg.shell, arg.name, arg.kind) {
        (Some(flag), _, _) => flag.to_owned(),
        (_, "on_progress", _) => "--progress".to_owned(),
        (_, "stdout" | "stderr", _) => return None,
        (_, "message", Kind::Text) => "-m msg".to_owned(),
        (_, name, Kind::Bool | Kind::Content) => format!("--{}", shell(name)),
        (_, name, Kind::Json) if arg.repeat => format!("--{} K=V", shell(name)),
        (_, name, _) => format!(
            "--{} {}",
            shell(name),
            crate::words::word(&call.name(), name)
        ),
    })
}

/// Prints help for `topic`: nothing, a family, or a family and a call.
pub fn print(topic: &[&str]) -> ExitCode {
    match topic {
        ["--json"] => crate::capabilities::help(),
        [] => output::print(&top()),
        [family] => match family_help(family) {
            Some(text) => output::print(&text),
            None => unknown(topic),
        },
        [family, name] => {
            match layerfs_api::calls()
                .into_iter()
                .find(|c| c.family == *family && shell(c.call) == shell(name))
            {
                Some(call) => output::print(&call_help(call)),
                None => unknown(topic),
            }
        }
        _ => unknown(topic),
    }
}

/// The top help: what LayerFS is, its concepts, a first session, the
/// families by heading, the options of every call and the exit codes.
fn top() -> String {
    let mut text = format!(
        "{}\n\nUsage: layerfs <family> <call> <arguments> [options]\n\n",
        bold(&format!(
            "layerfs {}: a versioned filesystem for AI agents",
            env!("CARGO_PKG_VERSION")
        ))
    );
    text.push_str(&format!("{}\n{}\n", bold("Concepts"), indented(CONCEPTS)));
    text.push_str(&format!("{}\n{}\n", bold("Get started"), indented(START)));
    let families = layerfs_api::families();
    let listed = |name: &str| GROUPS.iter().any(|(_, names)| names.contains(&name));
    let others: Vec<&str> = (families.iter().map(|f| f.name))
        .filter(|name| !listed(name))
        .collect();
    let groups = GROUPS.iter().copied().chain([("Other", &others[..])]);
    let groups = groups.filter(|(_, names)| !names.is_empty());
    for (at, (heading, names)) in groups.enumerate() {
        // a blank line between groups
        if at > 0 {
            text.push('\n');
        }
        text.push_str(&format!("{}\n", bold(heading)));
        for family in families.iter().filter(|f| names.contains(&f.name)) {
            let calls: Vec<String> = family.calls().iter().map(|c| shell(c.call)).collect();
            text.push_str(&format!("  {:<10} {}\n", family.name, calls.join(", ")));
        }
    }
    text.push_str(&format!(
        "\n{}\n{}\n",
        bold("Options on every call"),
        indented(OPTIONS)
    ));
    text.push_str("Exit 0 done · 2 wrong command line · 125 refused, with the reason on stderr\n");
    text.push_str("Home: LAYERFS_HOME, else ~/.layerfs · More: layerfs help <family> [<call>]\n");
    text
}

/// Lines, each indented by two spaces.
fn indented(lines: &[&str]) -> String {
    let mut text = String::new();
    for line in lines {
        text.push_str("  ");
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// One family's help: its line, then each call with its one-line help.
fn family_help(family: &str) -> Option<String> {
    let family = (layerfs_api::families().iter()).find(|f| f.name == family)?;
    let calls = family.calls();
    let mut text = format!(
        "{}: {}\n\n",
        bold(family.name),
        guide::family(family.name).unwrap_or("")
    );
    let width = calls.iter().map(|c| c.call.len()).max().unwrap_or(0);
    for call in calls.iter() {
        text.push_str(&format!("  {:<width$}  {}\n", shell(call.call), call.help));
    }
    text.push_str(&format!(
        "\n`layerfs help {} <call>` shows its arguments, options and an example.\n",
        family.name
    ));
    Some(text)
}

/// Rows of a word and its meaning, the words in one column.
fn rows(rows: &[(String, String)]) -> String {
    let width = rows
        .iter()
        .map(|(w, _)| w.chars().count())
        .max()
        .unwrap_or(0);
    let mut text = String::new();
    for (word, means) in rows {
        text.push_str(&format!("  {word:<width$}  {means}\n"));
    }
    text
}

/// One call's help: its usage line, what it does, its arguments and
/// options with their meanings, what it answers, and an example.
pub fn call_help(call: &Call) -> String {
    let name = call.name();
    let means = |arg: &str| guide::arg(&name, arg).unwrap_or("").to_owned();
    let mut text = format!("{}\n\n{}.\n", bold(&usage(call)), sentence(call.help));
    let args: Vec<(String, String)> = (call.args.iter())
        .filter(|a| a.kind != Kind::Function)
        .map(|a| (format!("<{}>", shell(a.name)), means(a.name)))
        .collect();
    if !args.is_empty() {
        text.push_str(&format!("\n{}\n{}", bold("Arguments"), rows(&args)));
    }
    let mut options: Vec<(String, String)> = (call.options.iter())
        .filter_map(|a| Some((option_word(call, a)?, means(a.name))))
        .collect();
    if piped(call) {
        let from = "read the content from this file instead of stdin".to_owned();
        options.push(("--from <file>".to_owned(), from));
    }
    if !options.is_empty() {
        text.push_str(&format!("\n{}\n{}", bold("Options"), rows(&options)));
    }
    if let Some(form) = call.form {
        let mut answer = form.always.join(", ");
        if !form.sometimes.is_empty() {
            let more = form.sometimes.join(", ");
            answer = match answer.is_empty() {
                true => format!("{more}, when they apply"),
                false => format!("{answer}; also {more} when they apply"),
            };
        }
        let short: Vec<&str> = form.always.iter().chain(form.sometimes).copied().collect();
        let extra: Vec<&str> = (form.long.iter().copied())
            .filter(|key| !short.contains(key))
            .collect();
        text.push_str(&format!("\n{}\n  {answer}\n", bold("Answers")));
        if !extra.is_empty() {
            text.push_str(&format!("  --detail adds: {}\n", extra.join(", ")));
        }
    }
    if let Some(example) = guide::example(&name) {
        text.push_str(&format!("\n{}\n  {example}\n", bold("Example")));
    }
    text
}

/// `text` with its first letter upper-case.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn unknown(topic: &[&str]) -> ExitCode {
    output::diagnostic(&format!(
        "layerfs: no help for `{}`; `layerfs help` lists the families",
        topic.join(" ")
    ));
    ExitCode::from(crate::run::USAGE)
}

//! `layerfs help [family [call]]`, from the call table.
use crate::output;
use layerfs_api::{Arg, Call, Kind};
use std::process::ExitCode;

fn shell(name: &str) -> String {
    name.replace('_', "-")
}

/// One call's usage line.
pub fn usage(call: &Call) -> String {
    let mut line = format!("layerfs {} {}", call.family, shell(call.call));
    for arg in call.args {
        let word = match arg.kind {
            Kind::Content | Kind::Json => continue,
            _ => format!("<{}>", shell(arg.name)),
        };
        line.push(' ');
        line.push_str(&if arg.optional {
            format!("[{word}]")
        } else {
            word
        });
    }
    for arg in call.options {
        line.push_str(&option(arg));
    }
    if call
        .args
        .iter()
        .any(|a| matches!(a.kind, Kind::Content | Kind::Json))
    {
        line.push_str(" [--from <file>] (content from stdin)");
    }
    line
}

fn option(arg: &Arg) -> String {
    match (arg.name, arg.kind) {
        ("on_progress", _) => " [--progress]".to_owned(),
        ("stdout" | "stderr", _) => String::new(),
        (name, Kind::Bool) => format!(" [--{}]", shell(name)),
        (name, _) => format!(" [--{} <{}>]", shell(name), shell(name)),
    }
}

/// Prints help for `topic`: nothing, a family, or a family and a call.
pub fn print(topic: &[&str]) -> ExitCode {
    match topic {
        [] => {
            let mut text = format!(
                "{}layerfs <family> <call> <arguments> [--options] [--text]\n\n",
                crate::banner::banner()
            );
            for family in layerfs_api::families() {
                let calls: Vec<String> = family.calls().iter().map(|c| shell(c.call)).collect();
                text.push_str(&format!("  {:<10} {}\n", family.name, calls.join(", ")));
            }
            text.push_str(concat!(
                "\n  doctor     report what is wrong in the home, changing nothing [--text]\n",
                "  import-env write this environment's 1.1.0 settings as records, once\n",
                "  worker     serve this machine's worker (inside a machine)\n",
                "  help       this help; `layerfs help <family> [<call>]` for more\n\n",
                "The home is LAYERFS_HOME, else ~/.layerfs. Exit 0: done; 2: wrong command line; 125: refused.\n",
            ));
            output::print(&text)
        }
        ["doctor", rest @ ..] => crate::doctor::help(rest),
        [family] if layerfs_api::families().iter().any(|f| f.name == *family) => {
            let calls = layerfs_api::calls();
            let shown = calls.into_iter().filter(|c| c.family == *family);
            output::print(&shown.map(described).collect::<String>())
        }
        [family, name] => {
            match layerfs_api::calls()
                .into_iter()
                .find(|c| c.family == *family && shell(c.call) == shell(name))
            {
                Some(call) => output::print(&described(call)),
                None => unknown(topic),
            }
        }
        _ => unknown(topic),
    }
}

/// A call's usage line and its one-line help.
fn described(call: &Call) -> String {
    format!("{}\n    {}\n", usage(call), call.help)
}

fn unknown(topic: &[&str]) -> ExitCode {
    output::diagnostic(&format!(
        "layerfs: no help for `{}`; `layerfs help` lists the families",
        topic.join(" ")
    ));
    ExitCode::from(crate::run::USAGE)
}

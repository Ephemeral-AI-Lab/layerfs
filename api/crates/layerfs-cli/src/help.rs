//! `layerfs help [family [call]]`, from the call table.
use layerfs_api::calls::{Arg, Call, Kind, CALLS, FAMILIES};
use std::process::ExitCode;

fn shell(name: &str) -> String {
    name.replace('_', "-")
}

/// One call's usage line.
pub fn usage(call: &Call) -> String {
    let mut line = format!("layerfs {} {}", call.family, call.call);
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
            println!("layerfs <family> <call> <arguments> [--options] [--text]");
            println!();
            for family in FAMILIES {
                let calls: Vec<&str> = CALLS
                    .iter()
                    .filter(|c| c.family == family)
                    .map(|c| c.call)
                    .collect();
                println!("  {family:<10} {}", calls.join(", "));
            }
            println!();
            println!("  doctor     report what is wrong in the home, changing nothing [--text]");
            println!("  import-env write this environment's 1.1.0 settings as records, once");
            println!("  worker     serve this machine's worker (inside a machine)");
            println!("  help       this help; `layerfs help <family> [<call>]` for more");
            println!();
            println!("The home is LAYERFS_HOME, else ~/.layerfs. Exit 0: done; 2: wrong command line; 125: refused.");
            ExitCode::SUCCESS
        }
        [family] if FAMILIES.contains(family) => {
            for call in CALLS.iter().filter(|c| c.family == *family) {
                println!("{}\n    {}", usage(call), call.help);
            }
            ExitCode::SUCCESS
        }
        [family, name] => match CALLS
            .iter()
            .find(|c| c.family == *family && c.call == *name)
        {
            Some(call) => {
                println!("{}\n    {}", usage(call), call.help);
                ExitCode::SUCCESS
            }
            None => unknown(topic),
        },
        _ => unknown(topic),
    }
}

fn unknown(topic: &[&str]) -> ExitCode {
    eprintln!(
        "layerfs: no help for `{}`; `layerfs help` lists the families",
        topic.join(" ")
    );
    ExitCode::from(crate::run::USAGE)
}

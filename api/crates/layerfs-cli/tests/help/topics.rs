//! Help and the wrong command line: every family, argument and option of
//! the table has its words and every call an example (`layerfs_api::guide`);
//! `layerfs <family> --help` lists the family's calls; a call's help shows
//! its arguments, options, answer and example; a wrong command line names
//! what is wrong and shows the call's usage line.
use crate::common;
use common::{layerfs, run, tempdir, text};
use layerfs_api::{guide, Kind};

#[test]
fn every_family_argument_and_option_has_its_words_and_every_call_an_example() {
    let mut missing = Vec::new();
    for family in layerfs_api::families() {
        if guide::family(family.name).is_none() {
            missing.push(format!("family {}", family.name));
        }
    }
    for call in layerfs_api::calls() {
        let name = call.name();
        let shown = (call.args.iter().chain(call.options))
            .filter(|arg| arg.kind != Kind::Function || arg.name == "on_progress");
        for arg in shown {
            if guide::arg(&name, arg.name).is_none() {
                missing.push(format!("{name} {}", arg.name));
            }
        }
        let shell = format!("layerfs {} {}", call.family, call.call.replace('_', "-"));
        match guide::example(&name) {
            Some(example) if example.contains(&shell) => {}
            other => missing.push(format!("{name} example {other:?}")),
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn help_reads_at_every_level() {
    let home = tempdir("help-levels");
    // the top: concepts, a first session, the families, the options
    let out = run(&mut layerfs(&home, &["help"]));
    assert_eq!(out.status.code(), Some(0));
    let top = text(&out.stdout);
    for part in ["Concepts", "Get started", "Run commands", "--text, --json"] {
        assert!(top.contains(part), "{part}: {top}");
    }
    for family in layerfs_api::families() {
        assert!(
            top.contains(&format!("  {} ", family.name)),
            "{}",
            family.name
        );
    }
    // a family, by `help` and by `--help`/`-h` after it
    for words in [&["help", "file"][..], &["file", "--help"], &["file", "-h"]] {
        let out = run(&mut layerfs(&home, words));
        assert_eq!(out.status.code(), Some(0), "{words:?}");
        let listed = text(&out.stdout);
        assert!(listed.starts_with("file: "), "{listed}");
        assert!(listed.contains("  blame  "), "{listed}");
    }
    // a call: usage, arguments, options, answer, example
    let out = run(&mut layerfs(&home, &["file", "read", "--help"]));
    assert_eq!(out.status.code(), Some(0));
    let call = text(&out.stdout);
    assert!(
        call.starts_with("layerfs file read <project> <branch> <path>"),
        "{call}"
    );
    for part in [
        "Arguments",
        "  <path>  ",
        "Options",
        "--at C",
        "Answers",
        "Example",
    ] {
        assert!(call.contains(part), "{part}: {call}");
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_wrong_command_line_names_what_is_wrong_and_shows_the_usage() {
    let home = tempdir("help-wrong");
    let wrong = |words: &[&str]| {
        let out = run(&mut layerfs(&home, words));
        assert_eq!(out.status.code(), Some(2), "{words:?}");
        assert!(out.stdout.is_empty(), "{words:?}");
        text(&out.stderr)
    };
    let err = wrong(&["file", "list", "shop/web", "main"]);
    assert!(
        err.starts_with("layerfs: file list is missing <path>\n"),
        "{err}"
    );
    assert!(
        err.contains("\nusage: layerfs file list <project> <branch> <path> "),
        "{err}"
    );
    assert!(err.contains("`layerfs help file list` for more"), "{err}");
    let err = wrong(&["file", "stat", "shop/web", "main", "/a", "/b"]);
    assert!(
        err.contains("takes 3 arguments; `/b` is one too many"),
        "{err}"
    );
    let err = wrong(&["home", "get", "--json", "--text"]);
    assert!(err.contains("--text and --json ask for two forms"), "{err}");
    let _ = std::fs::remove_dir_all(home);
}

/// A pipe gets JSON unless `--text` asks for the readable form; a refusal
/// comes in the form the result would.
#[test]
fn a_pipe_gets_json_and_text_when_asked() {
    let home = tempdir("help-forms");
    let out = run(&mut layerfs(&home, &["home", "get"]));
    assert!(
        text(&out.stdout).starts_with("{\"home\":"),
        "{}",
        text(&out.stdout)
    );
    let out = run(&mut layerfs(&home, &["home", "get", "--text"]));
    assert!(
        text(&out.stdout).starts_with("home "),
        "{}",
        text(&out.stdout)
    );
    let refused = |extra: &[&str]| {
        let mut words = vec!["home", "log", "--limit", "0"];
        words.extend(extra);
        let out = run(&mut layerfs(&home, &words));
        assert_eq!(out.status.code(), Some(125));
        text(&out.stderr)
    };
    assert!(refused(&[]).starts_with("{\"error\":\"InvalidInput\""));
    let err = refused(&["--text"]);
    assert!(
        err.starts_with("error: home.log: `limit` is 1 to "),
        "{err}"
    );
    assert!(err.trim_end().ends_with("(InvalidInput)"), "{err}");
    let _ = std::fs::remove_dir_all(home);
}

//! T11.2: the four GC calls on the shell. Each has one command, the call
//! that it names and the arguments the table gives it: a wrong arity or an
//! option the call does not take is exit 2 before any call, a well-formed
//! command line reaches the library, and the help is the table's.
#[path = "../support/mod.rs"]
mod common;
use common::{layerfs, run, tempdir, text};
use std::path::Path;

fn run_code(home: &Path, words: &[&str]) -> (Option<i32>, String, String) {
    let out = run(&mut layerfs(home, words));
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

/// The help line of each call, as the table renders it: hide-path is the
/// shell spelling of hide_path.
const CALLS: &[(&[&str], &str, &str)] = &[
    (
        &["branch", "remove"],
        "layerfs branch remove <project> <branch>",
        "remove one Branch; its name stays taken until compaction",
    ),
    (
        &["project", "delete"],
        "layerfs project delete <project>",
        "retire one project",
    ),
    (
        &["project", "hide-path"],
        "layerfs project hide-path <project> <path>",
        "hide one absolute path",
    ),
    (
        &["store", "compact"],
        "layerfs store compact <store>",
        "reclaim dead bytes",
    ),
    (
        &["store", "repair-mount-holders"],
        "layerfs store repair-mount-holders <project> <branch> <action>",
        "list, preview, settle, release, settle_legacy or audit a Branch's holders (operator)",
    ),
];

#[test]
fn project_remove_is_a_usage_error() {
    let home = tempdir("gc-remove");
    let (code, out, err) = run_code(&home, &["project", "remove", "shop/api"]);
    assert_eq!(code, Some(2), "{out} {err}");
    assert!(err.contains("no call `project remove`"), "{err}");
    assert!(out.is_empty(), "{out}");
    assert!(!home.join("store").exists(), "no call reached a home");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn help_lists_each_gc_call_with_its_usage() {
    let home = tempdir("gc-help");
    let (code, out, err) = run_code(&home, &["help"]);
    assert_eq!(code, Some(0), "{err}");
    assert!(err.is_empty(), "{err}");
    let line = |family: &str| {
        out.lines()
            .find(|line| line.split_whitespace().next() == Some(family))
            .unwrap_or_else(|| panic!("no {family} line in {out}"))
    };
    assert!(line("branch").contains("remove"), "{}", line("branch"));
    assert!(
        line("project").contains("delete") && line("project").contains("hide-path"),
        "{}",
        line("project")
    );
    assert!(line("store").contains("compact"), "{}", line("store"));

    for (words, usage, help) in CALLS {
        let topic: Vec<&str> = std::iter::once("help")
            .chain(words.iter().copied())
            .collect();
        let (code, out, err) = run_code(&home, &topic);
        assert_eq!(code, Some(0), "{words:?}: {err}");
        assert!(err.is_empty(), "{words:?}: {err}");
        assert!(out.starts_with(usage), "{words:?}: {out}");
        // the one-line help, as a sentence
        let sentence = format!("{}{}", help[..1].to_uppercase(), &help[1..]);
        assert!(out.contains(&sentence), "{words:?}: {out}");
        // the same help after the call, through -h and --help
        for flag in ["-h", "--help"] {
            let with_flag: Vec<&str> = words.iter().copied().chain([flag]).collect();
            let (code, more, err) = run_code(&home, &with_flag);
            assert_eq!(code, Some(0), "{with_flag:?}: {err}");
            assert_eq!(more, out, "{with_flag:?}");
        }
    }
    assert!(!home.join("store").exists(), "help made no home");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn wrong_arity_and_options_are_usage_errors() {
    let home = tempdir("gc-usage");
    let wrong: &[&[&str]] = &[
        &["branch", "remove"],
        &["branch", "remove", "shop/api"],
        &["branch", "remove", "shop/api", "work", "extra"],
        &["branch", "remove", "shop/api", "work", "--force"],
        &["project", "delete"],
        &["project", "delete", "shop/api", "extra"],
        &["project", "delete", "shop/api", "--force"],
        &["project", "hide-path", "shop/api"],
        &["project", "hide-path", "shop/api", "/a", "extra"],
        &["project", "hide-path", "shop/api", "/a", "--force"],
        &["store", "compact"],
        &["store", "compact", "shop", "extra"],
        &["store", "compact", "shop", "--force"],
        &["store", "repair-mount-holders"],
        &["store", "repair-mount-holders", "shop/app", "main"],
        &[
            "store",
            "repair-mount-holders",
            "shop/app",
            "main",
            "preview",
            "extra",
        ],
        &[
            "store",
            "repair-mount-holders",
            "shop/app",
            "main",
            "preview",
            "--prune",
        ],
    ];
    for words in wrong {
        let (code, out, err) = run_code(&home, words);
        assert_eq!(code, Some(2), "{words:?}: {err}");
        assert!(out.is_empty(), "{words:?}: {out}");
        assert!(err.starts_with("layerfs: "), "{words:?}: {err}");
    }
    // no positional was called: a usage error makes no home
    assert!(!home.join("store").exists(), "a usage error made a home");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn well_formed_lines_reach_the_library() {
    let home = tempdir("gc-reach");
    let well_formed: &[&[&str]] = &[
        &["branch", "remove", "shop/api", "work"],
        &["project", "delete", "shop/api"],
        &["project", "hide-path", "shop/api", "/a"],
        &["store", "compact", "shop"],
        &[
            "store",
            "repair-mount-holders",
            "shop/app",
            "main",
            "preview",
        ],
    ];
    for words in well_formed {
        let (code, out, err) = run_code(&home, words);
        // no Store in the home: the library refused after reading the line
        assert_eq!(code, Some(125), "{words:?}: {err}");
        assert!(out.is_empty(), "{words:?}: {out}");
        assert!(err.starts_with("{"), "{words:?}: {err}");
        assert!(
            err.contains("\"error\":\"NotFound\"") && err.contains("no Store named shop"),
            "{words:?}: {err}"
        );
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_bare_project_is_refused_and_the_four_are_out_of_scope() {
    let home = tempdir("gc-names");
    // the library, not the parser, refuses a project without its Store
    for words in [
        &["branch", "remove", "api", "main"][..],
        &["project", "delete", "api"],
        &["project", "hide-path", "api", "/a"],
    ] {
        let (code, out, err) = run_code(&home, words);
        assert_eq!(code, Some(125), "{words:?}: {err}");
        assert!(out.is_empty(), "{words:?}: {out}");
        assert!(err.starts_with("{"), "{words:?}: {err}");
        assert!(
            err.contains("\"error\":\"InvalidInput\""),
            "{words:?}: {err}"
        );
        assert!(err.contains("with its Store"), "{words:?}: {err}");
    }

    // a folder scope guards one folder and one Branch: none of the four is a
    // folder operation, so every one is refused, never widened
    let scoped: &[(&[&str], &str)] = &[
        (&["branch", "remove", "shop/api", "main"], "branch.remove"),
        (&["project", "delete", "shop/api"], "project.delete"),
        (
            &["project", "hide-path", "shop/api", "/web/a"],
            "project.hide_path",
        ),
        (&["store", "compact", "shop"], "store.compact"),
    ];
    for (words, call) in scoped {
        let mut command: Vec<&str> = words.to_vec();
        command.extend(["--scope", "/web", "--scope-project", "shop/api"]);
        let (code, out, err) = run_code(&home, &command);
        assert_eq!(code, Some(125), "{call}: {err}");
        assert!(out.is_empty(), "{call}: {out}");
        assert!(err.contains("out of scope: "), "{call}: {err}");
        assert!(err.contains(call), "{call}: {err}");
    }
    std::fs::remove_dir_all(home).unwrap();
}

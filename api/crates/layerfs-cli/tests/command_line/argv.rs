//! A command-line word that is not UTF-8 is a wrong command line (exit 2,
//! nothing called), not a panic: Linux accepts any bytes in `argv`.
#[path = "../support/mod.rs"]
mod common;
use common::{layerfs, run, tempdir, text};
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

fn bad() -> OsString {
    OsString::from_vec(vec![b'a', 0xff, 0xfe])
}

#[test]
fn a_word_that_is_not_utf8_is_exit_2_wherever_it_stands() {
    let home = tempdir("argv");
    let positions: [Vec<OsString>; 5] = [
        vec![bad()],
        vec!["store".into(), "get".into(), bad()],
        vec!["store".into(), "get".into(), "--".into(), bad()],
        vec!["file".into(), "write".into(), "--message".into(), bad()],
        vec!["help".into(), bad()],
    ];
    for words in positions {
        let out = run(&mut layerfs(&home, &words));
        let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
        assert_eq!(out.status.code(), Some(2), "{words:?}: {stderr}");
        assert!(stdout.is_empty(), "{words:?}: {stdout}");
        assert!(!stderr.contains("panicked"), "{words:?}: {stderr}");
        assert!(stderr.contains("not valid UTF-8"), "{words:?}: {stderr}");
        assert!(stderr.contains(r"a\xFF\xFE"), "names the word: {stderr}");
    }
    assert!(!home.join("store").exists(), "nothing was called");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_word_in_another_script_is_still_passed_on_whole() {
    let home = tempdir("argv-utf8");
    let out = run(&mut layerfs(&home, &["store", "get", "商店"]));
    assert_eq!(out.status.code(), Some(125), "a refusal, not a usage error");
    assert!(text(&out.stderr).contains("商店"), "{:?}", out.stderr);
    std::fs::remove_dir_all(home).unwrap();
}

/// A command line the table can judge is judged here, before any call:
/// a `--with` section the call does not take is exit 2; help shows a
/// Commit message as `-m`; each accepted option is listed once.
#[test]
fn the_parser_and_help_follow_the_table() {
    let parse = |name: &str, line: &[&str]| {
        let call = layerfs_api::find(name).expect(name);
        let words: Vec<String> = line.iter().map(|w| (*w).to_owned()).collect();
        layerfs_cli::parse::parse(call, &words, &mut &b""[..])
    };
    let refused = parse("commit.diff", &["shop/api", "agent", "--with", "live"])
        .err()
        .unwrap()
        .0;
    assert_eq!(
        refused,
        "commit.diff: --with names `live`; this call takes patch"
    );
    let refused = parse("branch.list", &["shop/api", "--with", "patch"])
        .err()
        .unwrap()
        .0;
    assert!(refused.ends_with("this call takes no section"), "{refused}");
    assert!(parse("commit.diff", &["shop/api", "agent", "--with", "patch"]).is_ok());
    let refused = parse("workspace.exec", &["sb:w", "true", "--events"])
        .err()
        .unwrap()
        .0;
    assert_eq!(refused.matches("--stream").count(), 1, "{refused}");
    let commit = layerfs_api::find("workspace.commit").unwrap();
    let usage = layerfs_cli::help::usage(commit);
    assert!(usage.contains(" [-m msg] "), "{usage}");
    // the whole run: exit 2 and nothing called
    let home = tempdir("argv-with");
    let out = run(&mut layerfs(
        &home,
        &["commit", "diff", "shop/api", "agent", "--with", "live"],
    ));
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(!home.join("store").exists(), "nothing was called");
    let _ = std::fs::remove_dir_all(home);
}

//! What each kind of outcome exits with, and how stdin, `--from` and `--`
//! are read: exit 2 is a wrong command line (nothing called), 125 is a
//! call LayerFS refused (the library answered), 0 is a call that was made.
//! None of these needs a Store: a call on a Store that does not exist is
//! refused `NoSuchStore` only after its arguments were read, which tells
//! it from a usage error.
mod common;
use common::{layerfs, run, run_with_stdin, tempdir, text};
use std::path::Path;

fn code(home: &Path, words: &[&str]) -> (Option<i32>, String, String) {
    let out = run(&mut layerfs(home, words));
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

#[test]
fn usage_error_refusal_and_success_have_their_own_codes() {
    let home = tempdir("codes");
    let (c, out, err) = code(&home, &["store", "list"]);
    assert_eq!((c, err.as_str()), (Some(0), ""), "{out}");
    assert!(out.starts_with('{') && out.ends_with("}\n"), "{out}");
    // the command line is wrong: exit 2, a line on stderr, nothing on stdout
    for words in [
        &["store", "get"][..],
        &["store", "list", "--bogus"],
        &["nope", "x"],
        &["store", "nope"],
    ] {
        let (c, out, err) = code(&home, words);
        assert_eq!(c, Some(2), "{words:?}: {err}");
        assert!(out.is_empty() && err.starts_with("layerfs: "), "{words:?}");
    }
    // the library refused: exit 125, a JSON refusal on stderr
    let (c, out, err) = code(&home, &["store", "get", "nope"]);
    assert_eq!(c, Some(125));
    assert!(out.is_empty(), "{out}");
    assert!(err.starts_with(r#"{"code":"NotFound""#), "{err}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn doctor_help_is_help_and_help_exits_0() {
    let home = tempdir("doctor-help");
    for words in [
        &["doctor", "--help"][..],
        &["doctor", "-h"],
        &["help", "doctor"],
    ] {
        let (c, out, err) = code(&home, words);
        assert_eq!(c, Some(0), "{words:?}: {err}");
        assert!(out.starts_with("layerfs doctor [--text]\n"), "{out}");
        assert!(err.is_empty(), "{err}");
    }
    // a flag it does not take is still a wrong command line
    assert_eq!(code(&home, &["doctor", "--all"]).0, Some(2));
    assert!(!home.join("store").exists(), "help made no home");
    let (c, out, _) = code(&home, &["--version"]);
    assert_eq!(c, Some(0));
    assert!(out.starts_with("layerfs "), "{out}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn double_dash_makes_every_later_word_a_positional() {
    let home = tempdir("dashdash");
    let (c, _, err) = code(&home, &["store", "get", "--weird"]);
    assert_eq!(c, Some(2), "an option the call does not have: {err}");
    let (c, _, err) = code(&home, &["store", "get", "--", "--weird"]);
    assert_eq!(c, Some(125), "a Store name that is not found: {err}");
    assert!(err.contains("--weird"), "{err}");
    // a lone `-` is a positional too, with or without `--`
    assert_eq!(code(&home, &["store", "get", "-"]).0, Some(125));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn stdin_and_from_feed_the_content_and_json_positionals() {
    let home = tempdir("stdin");
    let call = ["file", "apply", "s/p", "dev"];
    let json = br#"[{"op":"write","path":"/a","content":"x"}]"#;
    // valid JSON on stdin is read and the call made: refused, no such Store
    let out = run_with_stdin(&mut layerfs(&home, &call), json);
    assert_eq!(out.status.code(), Some(125), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("NoSuchStore"));
    // JSON that is not JSON, or not UTF-8, is a wrong command line
    let out = run_with_stdin(&mut layerfs(&home, &call), b"{x");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    let out = run_with_stdin(&mut layerfs(&home, &call), b"\xff");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("not UTF-8"), "{:?}", out.stderr);
    // empty or closed stdin is empty input, not a failure to read
    let (c, _, err) = code(&home, &call);
    assert_eq!(c, Some(2), "empty is not JSON: {err}");
    // --from reads a file, and stdin is then not read
    let file = home.join("changes.json");
    std::fs::write(&file, json).unwrap();
    let from = file.to_str().unwrap();
    let out = run_with_stdin(
        &mut layerfs(&home, &[&call[..], &["--from", from]].concat()),
        b"{x",
    );
    assert_eq!(out.status.code(), Some(125), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("NoSuchStore"));
    // a file that cannot be read is a wrong command line
    let missing = home.join("missing.json");
    let words = [&call[..], &["--from", missing.to_str().unwrap()]].concat();
    let (c, _, err) = code(&home, &words);
    assert_eq!(c, Some(2), "{err}");
    assert!(err.contains("cannot read"), "{err}");
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn binary_stdin_is_content_and_never_a_usage_error() {
    let home = tempdir("binary");
    let call = ["file", "write", "s/p", "dev", "/b.bin"];
    for bytes in [&b"\xff\xfe\x00z"[..], b"", b"plain\n"] {
        let out = run_with_stdin(&mut layerfs(&home, &call), bytes);
        assert_eq!(out.status.code(), Some(125), "{bytes:?}: {:?}", out.stderr);
        assert!(text(&out.stderr).contains("NoSuchStore"), "{bytes:?}");
    }
    std::fs::remove_dir_all(home).unwrap();
}

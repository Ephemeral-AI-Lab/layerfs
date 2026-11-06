//! A command-line word that is not UTF-8 is a wrong command line (exit 2,
//! nothing called), not a panic: Linux accepts any bytes in `argv`.
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

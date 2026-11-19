//! `home.get`'s build comparison: a Linux bundle and a daemon match the
//! calling build only when their commits are equal and both trees were
//! clean; a different commit is a mismatch (the home doctor reports it; the
//! answer carries no warning list since the call surface 2.0), and anything
//! it cannot tell is null. One test, since it sets `LAYERFS_BUNDLE` for the
//! whole process.
use layerfs_bridge::control::SdkValue;
use layerfs_control::home::{get, stamp, Build, With, BUILD_MARKER};
use std::path::PathBuf;

const OURS: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const THEIRS: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-home-get-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn bundle_commits_match_mismatch_or_cannot_tell() {
    let home = folder("home");
    let bundle = folder("bundle");
    std::env::set_var("LAYERFS_BUNDLE", &bundle);
    let clean = Build {
        commit: Some(OURS),
        dirty: false,
    };

    let got = get(&home, clean, With::default()).unwrap();
    let b = got.get("bundle").unwrap();
    assert_eq!(b.get_text("source"), Some("env:LAYERFS_BUNDLE"));
    assert_eq!(b.get("present"), Some(&SdkValue::Bool(false)));
    assert_eq!(b.get("matches"), Some(&SdkValue::Null));
    assert!(b.get_text("path").is_some() && b.get("dir").is_none());
    assert!(got.get("warnings").is_none());

    let manifest = |head: &str, dirty: bool| {
        std::fs::write(
            bundle.join("bundle.json"),
            format!(r#"{{"source":{{"head":"{head}","dirty":{dirty},"sha256":"00"}}}}"#),
        )
        .unwrap()
    };
    manifest(THEIRS, false);
    let got = get(&home, clean, With::default()).unwrap();
    let b = got.get("bundle").unwrap();
    assert_eq!(b.get("present"), Some(&SdkValue::Bool(true)));
    assert_eq!(b.get_text("commit"), Some(THEIRS));
    assert_eq!(b.get("matches"), Some(&SdkValue::Bool(false)));

    manifest(OURS, false);
    let got = get(&home, clean, With::default()).unwrap();
    assert_eq!(
        got.get("bundle").unwrap().get("matches"),
        Some(&SdkValue::Bool(true))
    );

    // a dirty tree on either side cannot be told apart
    manifest(OURS, true);
    let got = get(&home, clean, With::default()).unwrap();
    assert_eq!(
        got.get("bundle").unwrap().get("matches"),
        Some(&SdkValue::Null)
    );
    manifest(OURS, false);
    let got = get(
        &home,
        Build {
            commit: Some(OURS),
            dirty: true,
        },
        With::default(),
    )
    .unwrap();
    assert_eq!(
        got.get("bundle").unwrap().get("matches"),
        Some(&SdkValue::Null)
    );
    // a build without a commit cannot tell either
    let got = get(
        &home,
        Build {
            commit: None,
            dirty: false,
        },
        With::default(),
    )
    .unwrap();
    assert_eq!(
        got.get("bundle").unwrap().get("matches"),
        Some(&SdkValue::Null)
    );

    std::env::remove_var("LAYERFS_BUNDLE");
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&bundle);
}

#[test]
fn a_binary_names_its_build_after_the_marker() {
    let with = |rest: &str| {
        let mut bytes = b"\x7fELF padding ".to_vec();
        bytes.extend_from_slice(BUILD_MARKER);
        bytes.extend_from_slice(rest.as_bytes());
        bytes.extend_from_slice(b" more");
        bytes
    };
    assert_eq!(
        stamp(&with(&format!("{OURS}:clean"))),
        Some((OURS.to_owned(), false))
    );
    assert_eq!(
        stamp(&with(&format!("{OURS}:dirty"))),
        Some((OURS.to_owned(), true))
    );
    assert_eq!(stamp(&with("unknown")), None);
    assert_eq!(stamp(b"no marker here"), None);
}

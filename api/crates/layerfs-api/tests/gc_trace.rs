//! T-G2: the call log redacts every path argument (#41 §4.3). A
//! `project.remove_file` or a hidden `file.read`/`stat`/`list`/`glob` path is
//! logged as `<N bytes>` of its normalized form (one leading `/` stripped),
//! the `Removed` refusal still echoes the request path, the salted trace key
//! changes with the salt, and a `store.compact` report's artifact names never
//! reach the log.
use layerfs_api::Redact;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::path::{path_key, trace_key};
use layerfs_history::LayerStackId;
use layerfs_home::home::paths;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

static ENV: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);

/// The tests share the process environment, so each one holds this lock.
fn serial() -> MutexGuard<'static, ()> {
    let guard = ENV.lock().unwrap_or_else(|poison| poison.into_inner());
    std::env::remove_var("LAYERFS_LOGS");
    std::env::remove_var("LAYERFS_LOGS_TAG");
    guard
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-gc-trace-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn map(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    )
}

fn call(home: &Path, name: &str, fields: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    layerfs_api::call(Some(home), name, map(fields), &mut |_| {})
}

fn ok(home: &Path, name: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, fields).unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"))
}

fn refused(home: &Path, name: &str, fields: &[(&str, SdkValue)]) -> SdkRefusal {
    call(home, name, fields).expect_err("the call is refused")
}

fn p() -> (&'static str, SdkValue) {
    ("project", t("shop/api"))
}

/// A home with Store `shop` and project `shop/api` imported from a folder
/// holding `base.txt`.
fn project() -> (PathBuf, PathBuf) {
    let home = tempdir("home");
    layerfs_api::home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("base.txt"), "hello\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    (home, folder)
}

/// Every line of the call log, oldest first.
fn lines(home: &Path) -> Vec<SdkValue> {
    match std::fs::read_to_string(paths::calls_file(home)) {
        Ok(text) => text
            .lines()
            .map(|line| layerfs_api::json::parse(line).unwrap())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn last(home: &Path) -> SdkValue {
    lines(home).pop().expect("a line")
}

fn log_text(home: &Path) -> String {
    std::fs::read_to_string(paths::calls_file(home)).unwrap_or_default()
}

fn argv(line: &SdkValue) -> Vec<String> {
    match line.get("argv") {
        Some(SdkValue::List(words)) => words
            .iter()
            .map(|word| match word {
                SdkValue::Text(text) => text.clone(),
                other => panic!("argv word {other:?}"),
            })
            .collect(),
        other => panic!("no argv: {other:?}"),
    }
}

fn bytes(n: usize) -> String {
    format!("<{n} bytes>")
}

/// The one path a `Removed` refusal details.
fn removed_path(refusal: &SdkRefusal) -> &str {
    let Some(SdkValue::List(paths)) = refusal.detail.get("paths") else {
        panic!("no paths detail: {:?}", refusal.detail);
    };
    paths
        .first()
        .and_then(|path| path.get_text("path"))
        .expect("one path")
}

#[test]
fn path_arguments_are_byte_counts_in_the_call_log() {
    let _env = serial();
    let (home, folder) = project();
    let removed = ok(
        &home,
        "project.remove_file",
        &[p(), ("path", t("/base.txt"))],
    );
    assert_eq!(removed.get_text("kind"), Some("Removed"));
    assert_eq!(
        argv(&last(&home)),
        [
            "project".to_owned(),
            "remove-file".into(),
            "shop/api".into(),
            bytes("base.txt".len()),
        ]
    );

    // Hidden reads, stats, listings and globs: a path is a byte count.
    let read = refused(
        &home,
        "file.read",
        &[p(), ("branch", t("main")), ("path", t("/base.txt"))],
    );
    assert_eq!(read.code, "NotFound");
    assert_eq!(
        argv(&last(&home)),
        [
            "file".to_owned(),
            "read".into(),
            "shop/api".into(),
            "main".into(),
            bytes("base.txt".len()),
        ]
    );
    let stat = refused(
        &home,
        "file.stat",
        &[p(), ("branch", t("main")), ("path", t("/base.txt"))],
    );
    assert_eq!(stat.code, "NotFound");
    assert_eq!(argv(&last(&home))[4], bytes("base.txt".len()));

    let listed = ok(
        &home,
        "file.list",
        &[
            p(),
            ("branch", t("main")),
            ("path", t("/")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(listed.get_int("skipped_hidden"), Some(1));
    assert_eq!(argv(&last(&home))[4], bytes(0));

    let globbed = ok(
        &home,
        "file.glob",
        &[
            p(),
            ("branch", t("main")),
            ("pattern", t("*.txt")),
            ("path", t("/")),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(globbed.get_int("skipped_hidden"), Some(1));
    let words = argv(&last(&home));
    assert_eq!(words[4], bytes("*.txt".len()));
    assert_eq!(words[5], bytes(0));

    let text = log_text(&home);
    assert!(!text.contains("/base.txt"), "raw path in the log:\n{text}");
    assert!(!text.contains("base.txt"), "raw path in the log:\n{text}");
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn a_removed_refusal_echoes_the_path_the_log_does_not() {
    let _env = serial();
    let (home, folder) = project();
    ok(
        &home,
        "project.remove_file",
        &[p(), ("path", t("/base.txt"))],
    );
    let refusal = refused(
        &home,
        "file.read",
        &[p(), ("branch", t("main")), ("path", t("/base.txt"))],
    );
    assert_eq!(refusal.code, "NotFound");
    assert_eq!(removed_path(&refusal), "/base.txt");
    let line = last(&home);
    assert_eq!(line.get_text("outcome"), Some("Removed"));
    assert!(line.get("detail").is_none(), "a detail reached the log");
    let text = log_text(&home);
    assert!(!text.contains("/base.txt"));
    assert!(!text.contains("base.txt"));
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn the_redacted_set_covers_every_path_bearing_call() {
    // every argument of these names is logged only as a byte count: its
    // table entry says so (the flag replaced the dispatcher's name list)
    for name in ["path", "pattern"] {
        let named = (layerfs_api::calls().into_iter())
            .flat_map(|call| call.args.iter().chain(call.options))
            .filter(|arg| arg.name == name);
        let mut seen = 0;
        for arg in named {
            assert!(arg.redacted(), "{name} is not redacted");
            seen += 1;
        }
        assert!(seen > 0, "no call takes {name}");
    }
    for (name, args) in [
        ("project.remove_file", &["path"][..]),
        ("file.read", &["path"][..]),
        ("file.stat", &["path"][..]),
        ("file.list", &["path"][..]),
        ("file.glob", &["path", "pattern"][..]),
        ("file.grep", &["path", "pattern"][..]),
        ("file.cp", &["from", "to"][..]),
        ("file.mv", &["from", "to"][..]),
        ("commit.diff", &["path"][..]),
    ] {
        let spec = layerfs_api::find(name).unwrap_or_else(|| panic!("{name} is missing"));
        for arg in args {
            assert!(spec.arg(arg).is_some(), "{name} has no {arg}");
            let declared = spec.arg(arg).unwrap();
            assert!(declared.redacted(), "{name}'s {arg} is not redacted");
            // a path is counted in its normalized form (#41 §4.3)
            if *arg != "pattern" {
                assert_eq!(declared.redact, Redact::Path, "{name}'s {arg}");
            }
        }
    }
    // text that is not a path stays visible
    for name in ["project", "branch", "name", "commit"] {
        let named = (layerfs_api::calls().into_iter())
            .flat_map(|call| call.args.iter().chain(call.options))
            .filter(|arg| arg.name == name);
        for arg in named {
            assert_eq!(arg.redact, Redact::No, "{name} is redacted");
        }
    }
}

#[test]
fn the_trace_key_is_salted_by_the_store_policy_salt() {
    let _env = serial();
    let stack = LayerStackId::from_authority([7; 16]);
    let path = b"a/b";
    let first = trace_key(&[1; 32], stack, path);
    assert_eq!(first, trace_key(&[1; 32], stack, path), "stable salt");
    let second = trace_key(&[2; 32], stack, path);
    assert_ne!(first, second, "a different salt changes the keyed hash");
    assert_eq!(first.len(), 32);
    // the keyed hash is not the marker's plain BLAKE3(normalized path)
    assert_ne!(first, path_key(path));
    // the stack is part of the key
    let other = LayerStackId::from_authority([9; 16]);
    assert_ne!(first, trace_key(&[1; 32], other, path));
}

#[test]
fn store_compact_artifact_names_never_reach_the_log() {
    let _env = serial();
    let (home, folder) = project();
    let report = ok(&home, "store.compact", &[("store", t("shop"))]);
    assert!(report.get_int("freed_bytes").is_some(), "{report:?}");
    assert_eq!(
        argv(&last(&home)),
        ["store".to_owned(), "compact".into(), "shop".into()]
    );
    let text = log_text(&home);
    assert!(
        !text.contains(".compact"),
        "an artifact name reached the log:\n{text}"
    );
    let store = paths::store_file(&home, "shop");
    let name = store.file_name().unwrap().to_string_lossy().into_owned();
    assert!(!text.contains(&name), "the Store file name reached the log");
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(folder).unwrap();
}

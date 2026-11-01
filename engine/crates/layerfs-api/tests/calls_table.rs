//! The call table and its dispatcher (#533 step 2): 57 calls in nine
//! families, arguments checked before any effect, one test per family that
//! a valid call reaches its family function and a wrong one is refused.
use layerfs_api::calls::{self, json, keys, size, Kind, CALLS, FAMILIES};
use layerfs_bridge::control::SdkValue;
use std::path::Path;

fn map(fields: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        fields
            .iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    )
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// One call in a private temporary home, its events kept.
fn run(name: &str, args: SdkValue) -> (Result<SdkValue, (String, String)>, Vec<SdkValue>) {
    let home = tempdir();
    let mut events = Vec::new();
    let outcome = calls::call(Some(&home), name, args, &mut |event| events.push(event));
    let _ = std::fs::remove_dir_all(&home);
    (outcome.map_err(|r| (r.code, r.message)), events)
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-calls-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn code(name: &str, args: SdkValue) -> String {
    match run(name, args).0 {
        Ok(value) => format!("ok {}", json::print(&value)),
        Err((code, message)) => format!("{code}: {message}"),
    }
}

#[test]
fn the_table_has_nine_families_and_57_calls() {
    assert_eq!(CALLS.len(), 57);
    for family in FAMILIES {
        assert!(CALLS.iter().any(|c| c.family == family), "{family}");
    }
    assert!(CALLS.iter().all(|c| FAMILIES.contains(&c.family)));
    let mut names: Vec<_> = CALLS.iter().map(|c| c.name()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 57, "call names are unique");
    // an optional positional is only ever followed by optional ones (D7)
    for c in CALLS {
        let first_optional = c.args.iter().position(|a| a.optional);
        if let Some(i) = first_optional {
            assert!(c.args[i..].iter().all(|a| a.optional), "{}", c.name());
        }
    }
    let counts: Vec<usize> = FAMILIES
        .iter()
        .map(|f| CALLS.iter().filter(|c| c.family == *f).count())
        .collect();
    assert_eq!(counts, [4, 6, 3, 14, 4, 5, 8, 8, 5]);
}

#[test]
fn unknown_calls_and_arguments_are_refused_before_any_effect() {
    assert!(code("fs.read", SdkValue::Null).starts_with("InvalidInput: no call named `fs.read`"));
    assert!(code(
        "project.get",
        map(&[("project", t("shop/api")), ("bogus", t("x"))])
    )
    .starts_with("InvalidInput: project.get: unknown argument `bogus`"));
    // a function option never crosses into the library
    assert!(code(
        "store.move",
        map(&[
            ("name", t("s")),
            ("machine", t("m")),
            ("on_progress", t("f"))
        ])
    )
    .starts_with("InvalidInput"));
}

#[test]
fn project_family() {
    assert!(code("project.get", map(&[("project", t("api"))]))
        .starts_with("InvalidInput: project `api` must be written with its Store"));
    assert!(
        code("project.init", map(&[("project", t("shop/api"))])).contains("`folder` is required")
    );
    assert!(code("project.get", map(&[("project", t("shop/API"))])).starts_with("InvalidInput"));
    let reached = code("project.get", map(&[("project", t("shop/api"))]));
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn branch_family() {
    assert!(code(
        "branch.fork",
        map(&[("project", t("shop/api")), ("name", t("fix"))])
    )
    .contains("`from_branch` is required"));
    let reached = code(
        "branch.fork",
        map(&[
            ("project", t("shop/api")),
            ("name", t("fix")),
            ("from_branch", t("main")),
        ]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn commit_family() {
    assert!(code(
        "commit.list",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("page_size", t("9"))
        ])
    )
    .contains("`page_size` must be a whole number"));
    let reached = code(
        "commit.list",
        map(&[("project", t("shop/api")), ("branch", t("main"))]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn file_family() {
    assert!(code(
        "file.write",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a"))
        ])
    )
    .contains("`content` is required"));
    assert!(code(
        "file.write",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a")),
            ("content", SdkValue::Int(1))
        ])
    )
    .contains("`content` must be text or bytes"));
    let reached = code(
        "file.read",
        map(&[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a")),
        ]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn sandbox_family() {
    assert!(code(
        "sandbox.create",
        map(&[("project", t("shop/api")), ("name", t("tests"))])
    )
    .contains("`machine` is required"));
    let reached = code(
        "sandbox.close",
        map(&[("name", t("tests")), ("force", SdkValue::Bool(true))]),
    );
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn workspace_family() {
    assert!(
        code("workspace.exec", map(&[("ws", t("tests:/workspaces/1"))]))
            .contains("`command` is required")
    );
    let reached = code("workspace.status", map(&[("ws", t("tests:/workspaces/1"))]));
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn machine_family() {
    assert!(code(
        "machine.create",
        map(&[
            ("name", t("dev")),
            ("platform", t("lima")),
            ("memory", t("8Q"))
        ])
    )
    .contains("`memory` is a size"));
    let reached = code("machine.list", SdkValue::Null);
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn store_family() {
    assert!(code("store.move", map(&[("name", t("shop"))])).contains("`machine` is required"));
    let reached = code("store.list", SdkValue::Null);
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn backup_family() {
    assert!(code("backup.create", map(&[("store", t("shop"))])).contains("`bucket` is required"));
    let reached = code("backup.list", SdkValue::Null);
    assert!(!reached.starts_with("InvalidInput"), "{reached}");
}

#[test]
fn json_round_trips_every_value() {
    let value = map(&[
        ("a", SdkValue::Int(-12)),
        ("b", t("x\"y\\z\n\u{1}é😀")),
        (
            "c",
            SdkValue::List(vec![
                SdkValue::Null,
                SdkValue::Bool(true),
                SdkValue::Bool(false),
            ]),
        ),
        ("d", SdkValue::Bytes(vec![0xff, 0x00, 0x10])),
        ("e", SdkValue::Bytes(b"plain".to_vec())),
    ]);
    let text = json::print(&value);
    let back = json::parse(&text).unwrap();
    let expect = map(&[
        ("a", SdkValue::Int(-12)),
        ("b", t("x\"y\\z\n\u{1}é😀")),
        (
            "c",
            SdkValue::List(vec![
                SdkValue::Null,
                SdkValue::Bool(true),
                SdkValue::Bool(false),
            ]),
        ),
        ("d", SdkValue::Bytes(vec![0xff, 0x00, 0x10])),
        ("e", t("plain")),
    ]);
    assert_eq!(back, expect);
    assert_eq!(json::parse("\"\\ud83d\\ude00\"").unwrap(), t("😀"));
    for bad in [
        "1.5",
        "{\"a\":1,\"a\":2}",
        "[1,",
        "nul",
        "\"\\ud83d\"",
        "1 2",
    ] {
        assert!(json::parse(bad).is_err(), "{bad}");
    }
    assert_eq!(
        json::unbase64(&json::base64(b"any carnal pleas")).unwrap(),
        b"any carnal pleas"
    );
}

#[test]
fn sizes_and_keys() {
    assert_eq!(size("512"), Some(512));
    assert_eq!(size("64K"), Some(64 << 10));
    assert_eq!(size("8G"), Some(8 << 30));
    assert_eq!(size("8T"), None);
    assert_eq!(keys::snake_name("exitCode"), "exit_code");
    assert_eq!(keys::snake_name("nextPagePos"), "next_page_pos");
    assert_eq!(keys::snake_name("/src/README.md"), "/src/README.md");
    assert_eq!(keys::snake_name("Makefile"), "Makefile");
    assert_eq!(keys::camel_name("start_line"), "startLine");
    assert!(CALLS
        .iter()
        .flat_map(|c| c.args.iter().chain(c.options))
        .all(|a| a.name == a.name.to_lowercase()));
    assert!(CALLS
        .iter()
        .flat_map(|c| c.options)
        .any(|a| a.kind == Kind::Function));
}

#[test]
fn an_explicit_home_must_be_absolute() {
    let mut events = Vec::new();
    let refused = calls::call(
        Some(Path::new("relative")),
        "machine.list",
        SdkValue::Null,
        &mut |e| events.push(e),
    )
    .unwrap_err();
    assert_eq!(refused.code, "InvalidInput");
}

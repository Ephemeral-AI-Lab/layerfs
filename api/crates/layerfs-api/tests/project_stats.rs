//! Namespace counts are path based, pinned and explicit about work
//! exhaustion; `--at` takes a full id or a prefix. Each `project.stats`
//! here asks for the long form (`detail`), which names the counted Commit.
use layerfs_protocol::provision::install::SdkValue;
use std::{
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};
fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phase137-tree-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn args(method: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    let mut fields: Vec<(String, SdkValue)> = fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    if method == "project.stats" {
        fields.push(("detail".to_owned(), SdkValue::Bool(true)));
    }
    SdkValue::Map(fields)
}
fn call(home: &Path, method: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    layerfs_api::call(Some(home), method, args(method, fields), &mut |_| {})
        .unwrap_or_else(|r| panic!("{method}: {r:?}"))
}
fn check_counts(value: &SdkValue, files: i64, dirs: i64, links: i64, size: i64) {
    for (key, n) in [
        ("file_count", files),
        ("dir_count", dirs),
        ("symlink_count", links),
        ("size", size),
    ] {
        assert_eq!(value.get_int(key), Some(n), "{key}: {value:?}");
    }
    assert_eq!(value.get("partial"), Some(&SdkValue::Bool(false)));
    assert_eq!(value.get_text("root").map(str::len), Some(64), "{value:?}");
}
fn check(home: &Path, tag: &str) {
    let source = folder(&format!("source-{tag}"));
    std::fs::write(source.join("file"), "hello").unwrap();
    std::fs::hard_link(source.join("file"), source.join("alias")).unwrap();
    std::fs::create_dir(source.join("empty")).unwrap();
    symlink(".", source.join("loop")).unwrap();
    call(
        home,
        "project.init",
        &[
            ("project", t("trees/p")),
            ("folder", t(source.to_str().unwrap())),
        ],
    );
    let stats = call(home, "project.stats", &[("project", t("trees/p"))]);
    check_counts(&stats, 2, 1, 1, 10);
    let layer = stats.get_text("commit").unwrap().to_owned();
    assert_eq!(layer.len(), 66, "the full id");
    call(
        home,
        "branch.fork",
        &[
            ("project", t("trees/p")),
            ("name", t("dev")),
            ("from", t("main")),
        ],
    );
    call(
        home,
        "file.write",
        &[
            ("project", t("trees/p")),
            ("branch", t("dev")),
            ("path", t("/new")),
            ("content", t("abc")),
        ],
    );
    let saved = call(
        home,
        "project.stats",
        &[("project", t("trees/p")), ("branch", t("dev"))],
    );
    check_counts(&saved, 3, 1, 1, 13);
    let commit = saved.get_text("commit").unwrap().to_owned();
    assert_ne!(commit, layer);
    call(
        home,
        "file.write",
        &[
            ("project", t("trees/p")),
            ("branch", t("dev")),
            ("path", t("/later")),
            ("content", t("late")),
        ],
    );
    let pinned = call(
        home,
        "project.stats",
        &[
            ("project", t("trees/p")),
            ("branch", t("dev")),
            ("at", t(&commit)),
        ],
    );
    assert_eq!(pinned, saved);
    // a prefix of 8 or more characters names the same Commit
    let prefixed = call(
        home,
        "project.stats",
        &[
            ("project", t("trees/p")),
            ("branch", t("dev")),
            ("at", t(&commit[..10])),
        ],
    );
    assert_eq!(prefixed, saved);
    let short = layerfs_api::call(
        Some(home),
        "project.stats",
        args(
            "project.stats",
            &[("project", t("trees/p")), ("at", t(&commit[..6]))],
        ),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(short.code, "InvalidInput", "{short:?}");
    let base = call(
        home,
        "project.stats",
        &[
            ("project", t("trees/p")),
            ("branch", t("main")),
            ("at", t(&layer)),
        ],
    );
    assert_eq!(base, stats);
    let bad = layerfs_api::call(
        Some(home),
        "project.stats",
        args(
            "project.stats",
            &[
                ("project", t("trees/p")),
                ("branch", t("main")),
                ("at", t(&commit)),
            ],
        ),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(bad.code, "NotFound", "{bad:?}");
    assert!(
        matches!(
            bad.detail.get_text("code"),
            Some("NoSuchCommit" | "NotInHistory")
        ),
        "{bad:?}"
    );
    let empty = folder(&format!("empty-{tag}"));
    call(
        home,
        "project.init",
        &[
            ("project", t("trees/empty")),
            ("folder", t(empty.to_str().unwrap())),
        ],
    );
    check_counts(
        &call(home, "project.stats", &[("project", t("trees/empty"))]),
        0,
        0,
        0,
        0,
    );
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(source).unwrap();
    std::fs::remove_dir_all(empty).unwrap();
}
#[test]
fn sqlite_counts_links_empty_trees_and_immutable_commit_and_layer_pins() {
    let home = folder("sqlite-home");
    call(&home, "store.create", &[("name", t("trees"))]);
    check(&home, "sqlite");
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn postgresql_counts_the_same_saved_tree_definition() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL unset");
        return;
    };
    let home = folder("pg-home");
    // the URL's user becomes a saved login (a trust-auth server takes any
    // password); the address is connected without it
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    call(
        &home,
        "store.connect",
        &[
            ("store", t("trees")),
            ("url", t(&format!("{scheme}://{address}"))),
            ("user", t(user)),
            ("password", t("unused")),
            ("synchronous_commit", t("off")),
        ],
    );
    check(&home, "pg");
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn budget_exhaustion_returns_partial_counts() {
    let home = folder("budget-home");
    let source = folder("budget-source");
    for n in 0..10_001 {
        std::fs::write(source.join(format!("f{n:05}")), []).unwrap();
    }
    call(&home, "store.create", &[("name", t("trees"))]);
    call(
        &home,
        "project.init",
        &[
            ("project", t("trees/large")),
            ("folder", t(source.to_str().unwrap())),
        ],
    );
    let value = call(&home, "project.stats", &[("project", t("trees/large"))]);
    assert_eq!(value.get_int("file_count"), Some(10_000));
    assert_eq!(value.get("partial"), Some(&SdkValue::Bool(true)));
    assert_eq!(value.get_int("size"), Some(0));
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}

#[test]
fn hidden_paths_are_not_counted() {
    let home = folder("hidden-home");
    call(&home, "store.create", &[("name", t("trees"))]);
    let source = folder("hidden-source");
    std::fs::write(source.join("keep"), "aa").unwrap();
    std::fs::write(source.join("hide"), "bbb").unwrap();
    std::fs::create_dir(source.join("sub")).unwrap();
    std::fs::write(source.join("sub/inside"), "cccc").unwrap();
    call(
        &home,
        "project.init",
        &[
            ("project", t("trees/p")),
            ("folder", t(source.to_str().unwrap())),
        ],
    );
    let before = call(&home, "project.stats", &[("project", t("trees/p"))]);
    assert_eq!(before.get_int("file_count"), Some(3), "{before:?}");
    assert_eq!(before.get_int("dir_count"), Some(1), "{before:?}");
    let logical = before.get_int("size").unwrap();
    call(
        &home,
        "project.hide_path",
        &[("project", t("trees/p")), ("path", t("/hide"))],
    );
    let after = call(&home, "project.stats", &[("project", t("trees/p"))]);
    assert_eq!(after.get_int("file_count"), Some(2), "{after:?}");
    assert_eq!(after.get_int("dir_count"), Some(1), "{after:?}");
    // the size leaves the hidden file out too (R2 N8)
    assert_eq!(after.get_int("size"), Some(logical - 3), "{after:?}");
    // A removed directory hides its whole subtree.
    call(
        &home,
        "project.hide_path",
        &[("project", t("trees/p")), ("path", t("/sub"))],
    );
    let nested = call(&home, "project.stats", &[("project", t("trees/p"))]);
    assert_eq!(nested.get_int("file_count"), Some(1), "{nested:?}");
    assert_eq!(nested.get_int("dir_count"), Some(0), "{nested:?}");
    assert_eq!(nested.get_int("size"), Some(2), "{nested:?}");
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}

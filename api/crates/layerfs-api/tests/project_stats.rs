//! Namespace counts are path based, pinned and explicit about work exhaustion.
use layerfs_bridge::control::SdkValue;
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
fn call(home: &Path, method: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    layerfs_api::call(
        Some(home),
        method,
        SdkValue::Map(
            fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
        ),
        &mut |_| {},
    )
    .unwrap_or_else(|r| panic!("{method}: {r:?}"))
}
fn check_counts(value: &SdkValue, files: i64, dirs: i64, links: i64, logical: i64) {
    for (key, n) in [
        ("files", files),
        ("directories", dirs),
        ("symlinks", links),
        ("others", 0),
        ("logical", logical),
    ] {
        assert_eq!(value.get_int(key), Some(n), "{key}: {value:?}");
    }
    assert_eq!(value.get("complete"), Some(&SdkValue::Bool(true)));
    assert!(value.get("reason").is_none());
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
    let pin = stats.get("at").unwrap();
    assert_eq!(pin.get_text("kind"), Some("Layer"));
    let layer = pin.get_text("id").unwrap().to_owned();
    call(
        home,
        "branch.fork",
        &[
            ("project", t("trees/p")),
            ("name", t("dev")),
            ("from_branch", t("main")),
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
    assert_eq!(saved.get("at").unwrap().get_text("kind"), Some("Commit"));
    let commit = saved.get("at").unwrap().get_text("id").unwrap().to_owned();
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
            ("commit", t(&commit)),
        ],
    );
    assert_eq!(pinned, saved);
    let base = call(
        home,
        "project.stats",
        &[
            ("project", t("trees/p")),
            ("branch", t("main")),
            ("commit", t(&layer)),
        ],
    );
    assert_eq!(base, stats);
    let bad = layerfs_api::call(
        Some(home),
        "project.stats",
        SdkValue::map([
            ("project", t("trees/p")),
            ("branch", t("main")),
            ("commit", t(&commit)),
        ]),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(
        matches!(bad.code.as_str(), "NoSuchCommit" | "NotInHistory"),
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
    layerfs_api::close_idle(std::time::Duration::ZERO);
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
#[cfg(feature = "postgresql")]
fn postgresql_counts_the_same_saved_tree_definition() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL unset");
        return;
    };
    let home = folder("pg-home");
    call(
        &home,
        "store.connect",
        &[
            ("name", t("trees")),
            ("url", t(&url)),
            ("synchronous_commit", t("off")),
        ],
    );
    check(&home, "pg");
    std::fs::remove_dir_all(home).unwrap();
}
#[test]
fn budget_exhaustion_returns_partial_counts_with_a_reason() {
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
    assert_eq!(value.get_int("files"), Some(10_000));
    assert_eq!(value.get("complete"), Some(&SdkValue::Bool(false)));
    assert!(value.get_text("reason").is_some());
    assert_eq!(value.get_int("logical"), Some(0));
    layerfs_api::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(source).unwrap();
}

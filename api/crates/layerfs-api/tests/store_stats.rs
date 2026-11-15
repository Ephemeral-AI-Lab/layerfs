//! Store statistics through native SQLite, a real worker, and PostgreSQL.
use layerfs_api::home::{paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phase137-stats-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}
fn call(home: &Path, method: &str, fields: &[(&str, SdkValue)]) -> SdkValue {
    let args = SdkValue::Map(
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), method, args, &mut |_| {})
        .unwrap_or_else(|r| panic!("{method}: {r:?}"))
}
fn check(home: &Path, kind: &str, test: &str) {
    let stats = || call(home, "store.stats", &[("name", t("stats"))]);
    let empty = stats();
    assert_eq!(empty.get_text("kind"), Some(kind));
    // a previous project's bytes stay after its soft delete, so later runs
    // measure from the Store's current history
    let base = empty.get_int("history_logical").unwrap();
    assert_eq!(
        empty.get("free") == Some(&SdkValue::Null),
        kind == "postgresql"
    );
    assert!(empty.get("dedupe").is_none());
    let name = format!("stats/{test}");
    let source = folder(&format!("input-{kind}-{test}"));
    std::fs::write(source.join("file"), "hello").unwrap();
    call(
        home,
        "project.init",
        &[
            ("project", t(&name)),
            ("folder", t(source.to_str().unwrap())),
        ],
    );
    let initial = stats();
    assert_eq!(initial.get_int("history_logical"), Some(base + 5));
    assert!(initial.get_int("canonical").unwrap() > 0);
    assert!(initial.get_int("stored").unwrap() > 0);
    assert!(initial.get_int("file").unwrap() > 0);
    call(
        home,
        "branch.fork",
        &[
            ("project", t(&name)),
            ("name", t("dev")),
            ("from_branch", t("main")),
        ],
    );
    call(
        home,
        "file.write",
        &[
            ("project", t(&name)),
            ("branch", t("dev")),
            ("path", t("/file")),
            ("content", t("abcdef")),
        ],
    );
    call(
        home,
        "branch.publish",
        &[("project", t(&name)), ("branch", t("dev"))],
    );
    let published = stats();
    assert_eq!(
        published.get_int("history_logical"),
        Some(base + 17),
        "Commit and Layer count their snapshots"
    );
    // `project.delete` retires the Stack and removes the home entry; the
    // marker is the commit point, so the history bytes stay until a
    // `store.compact` collects them and the counters do not drop.
    call(home, "project.delete", &[("project", t(&name))]);
    let removed = stats();
    assert_eq!(removed.get_int("history_logical"), Some(base + 17));
    assert_eq!(removed.get("canonical"), published.get("canonical"));
    assert_eq!(removed.get("stored"), published.get("stored"));
    let gone = layerfs_api::call(
        Some(home),
        "project.get",
        SdkValue::map([("project", t(&name))]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(gone.code, "NoSuchProject");
    assert!(!paths::project_record(home, "stats", test).exists());
    layerfs_api::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(source).unwrap();
}

#[test]
fn native_sqlite_and_worker_statistics_use_the_same_counters() {
    let home = folder("sqlite-home");
    call(&home, "store.create", &[("name", t("stats"))]);
    check(&home, "sqlite", "p");
    let root = folder("worker-root");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "phase137".to_owned(), guest));
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("phase137"))]),
    )
    .unwrap();
    call(
        &home,
        "store.push",
        &[("name", t("stats")), ("machine", t("box"))],
    );
    check(&home, "sqlite", "w");
    // A stopped worker refuses rather than starting anything.
    layerfs_api::close_idle(std::time::Duration::ZERO);
    std::fs::remove_file(paths::worker_record(&home, "box")).unwrap();
    let refused = layerfs_api::call(
        Some(&home),
        "store.stats",
        SdkValue::map([("name", t("stats"))]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refused.code, "Unavailable");
    std::fs::remove_dir_all(home).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(feature = "postgresql")]
fn postgresql_statistics_have_null_free_and_count_publications() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL unset");
        return;
    };
    let home = folder("pg-home");
    call(
        &home,
        "store.connect",
        &[
            ("name", t("stats")),
            ("url", t(&url)),
            ("synchronous_commit", t("off")),
        ],
    );
    check(&home, "postgresql", "p");
    std::fs::remove_dir_all(home).unwrap();
}

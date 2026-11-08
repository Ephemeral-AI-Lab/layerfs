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
fn check(home: &Path, kind: &str) {
    let stats = || call(home, "store.stats", &[("name", t("stats"))]);
    let empty = stats();
    assert_eq!(empty.get_text("kind"), Some(kind));
    assert_eq!(empty.get_int("history_logical"), Some(0));
    assert_eq!(
        empty.get("free") == Some(&SdkValue::Null),
        kind == "postgresql"
    );
    assert!(empty.get("dedupe").is_none());
    let source = folder(&format!("input-{kind}"));
    std::fs::write(source.join("file"), "hello").unwrap();
    call(
        home,
        "project.init",
        &[
            ("project", t("stats/p")),
            ("folder", t(source.to_str().unwrap())),
        ],
    );
    let initial = stats();
    assert_eq!(initial.get_int("history_logical"), Some(5));
    assert!(initial.get_int("canonical").unwrap() > 0);
    assert!(initial.get_int("stored").unwrap() > 0);
    assert!(initial.get_int("file").unwrap() > 0);
    call(
        home,
        "branch.fork",
        &[
            ("project", t("stats/p")),
            ("name", t("dev")),
            ("from_branch", t("main")),
        ],
    );
    call(
        home,
        "file.write",
        &[
            ("project", t("stats/p")),
            ("branch", t("dev")),
            ("path", t("/file")),
            ("content", t("abcdef")),
        ],
    );
    call(
        home,
        "branch.publish",
        &[("project", t("stats/p")), ("branch", t("dev"))],
    );
    let published = stats();
    assert_eq!(
        published.get_int("history_logical"),
        Some(17),
        "Commit and Layer count their snapshots"
    );
    if kind == "postgresql" {
        let refused = layerfs_api::call(
            Some(home),
            "project.remove",
            SdkValue::map([("project", t("stats/p"))]),
            &mut |_| {},
        )
        .unwrap_err();
        assert_eq!(refused.code, "Unsupported");
    } else {
        call(home, "project.remove", &[("project", t("stats/p"))]);
        let removed = stats();
        assert_eq!(removed.get_int("history_logical"), Some(0));
        assert_eq!(removed.get("canonical"), published.get("canonical"));
        assert_eq!(removed.get("stored"), published.get("stored"));
    }
    layerfs_api::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(source).unwrap();
}

#[test]
fn native_sqlite_and_worker_statistics_use_the_same_counters() {
    let home = folder("sqlite-home");
    call(&home, "store.create", &[("name", t("stats"))]);
    check(&home, "sqlite");
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
    check(&home, "sqlite");
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
    check(&home, "postgresql");
    std::fs::remove_dir_all(home).unwrap();
}

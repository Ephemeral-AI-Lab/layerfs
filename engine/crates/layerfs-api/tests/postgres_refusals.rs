//! `store.connect` refusals that need no PostgreSQL server (#533 review
//! P1-15): a password anywhere in the URL, and the same server and
//! database already connected under another name, are refused before any
//! request and write nothing. The URLs name 127.0.0.1 port 1, where
//! nothing listens, so a request would fail with another code.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-pg-refusal-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn connect(home: &Path, name: &str, url: &str) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::map([("name", SdkValue::text(name)), ("url", SdkValue::text(url))]);
    calls::call(Some(home), "store.connect", args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

#[test]
fn a_password_in_the_url_is_refused_before_any_effect() {
    let home = tempdir("password");
    home::open(&home).unwrap();
    for url in [
        "postgresql://alice:secret@127.0.0.1:1/app",
        "postgresql://alice:@127.0.0.1:1/app",
        "postgresql://alice@127.0.0.1:1/app?password=secret",
        "postgres://alice@127.0.0.1:1/app?sslmode=disable&PassWord=secret",
        "postgresql://alice@127.0.0.1:1/app?pass%77ord=secret",
        "postgresql://alice@127.0.0.1:1/app?sslpassword=secret",
    ] {
        let refused = connect(&home, "pg", url).unwrap_err();
        assert_eq!(refused.0, "InvalidInput", "{url}: {refused:?}");
        assert!(refused.1.contains("password"), "{url}: {refused:?}");
        assert!(!paths::store_dir(&home, "pg").exists(), "{url}");
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn the_same_database_under_a_second_name_is_exists() {
    let home = tempdir("twice");
    home::open(&home).unwrap();
    write_record(
        &paths::store_record(&home, "first"),
        &SdkValue::map([
            ("name", SdkValue::text("first")),
            ("kind", SdkValue::text("postgresql")),
            ("url", SdkValue::text("postgresql://alice@127.0.0.1:1/app")),
            ("schema", SdkValue::text("layerfs")),
            ("synchronous_commit", SdkValue::text("off")),
            ("binding", SdkValue::Bytes(vec![7; 32])),
            ("cursor", SdkValue::Bytes(vec![9; 32])),
        ]),
    )
    .unwrap();
    for url in [
        "postgresql://alice@127.0.0.1:1/app",
        "postgres://ALICE@127.0.0.1:1/app?sslmode=disable",
        "postgresql:///app?host=127.0.0.1&port=1&user=bob",
        "postgresql://127.0.0.1:1?dbname=app",
        "postgresql://app@127.0.0.1:1",
    ] {
        let refused = connect(&home, "second", url).unwrap_err();
        assert_eq!(refused.0, "Exists", "{url}: {refused:?}");
        assert!(refused.1.contains("first"), "{url}: {refused:?}");
        assert!(!paths::store_dir(&home, "second").exists(), "{url}");
    }
    // the case of a database name counts; another database is not the same
    for url in [
        "postgresql://alice@127.0.0.1:1/other",
        "postgresql://alice@127.0.0.1:1/App",
        "postgresql://alice@127.0.0.1:2/app",
    ] {
        let failed = connect(&home, "second", url).unwrap_err();
        assert_ne!(failed.0, "Exists", "{url}: {failed:?}");
        assert!(!paths::store_dir(&home, "second").exists(), "{url}");
    }
    let _ = std::fs::remove_dir_all(home);
}

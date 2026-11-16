//! `store.connect` refusals that need no PostgreSQL server (#533 review
//! P1-15): a password anywhere in the URL, and the same server and
//! database already connected under another name, are refused before any
//! request and write nothing. The URLs name 127.0.0.1 port 1, where
//! nothing listens, so a request would fail with another code. A
//! PostgreSQL Store's calls take its lease (#536 §8), so `store.disconnect`
//! refuses `InUse` while another process holds it.
#![cfg(unix)] // unix stat/lock semantics; the Windows face is its own port

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
    layerfs_api::call(Some(home), "store.connect", args, &mut |_| {})
        .map_err(|r| (r.code, r.message))
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

/// Records PostgreSQL Store `name` at `url` (port 1: nothing listens).
fn connected(home: &Path, name: &str, url: &str) {
    write_record(
        &paths::store_record(home, name),
        &SdkValue::map([
            ("name", SdkValue::text(name)),
            ("kind", SdkValue::text("postgresql")),
            ("url", SdkValue::text(url)),
            ("schema", SdkValue::text("layerfs")),
            ("synchronous_commit", SdkValue::text("off")),
            ("binding", SdkValue::Bytes(vec![7; 32])),
            ("cursor", SdkValue::Bytes(vec![9; 32])),
        ]),
    )
    .unwrap();
}

#[test]
fn the_same_database_under_a_second_name_is_exists() {
    let home = tempdir("twice");
    home::open(&home).unwrap();
    connected(&home, "first", "postgresql://alice@127.0.0.1:1/app");
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

#[test]
fn disconnect_refuses_a_postgresql_store_another_process_holds() {
    let home = tempdir("held");
    home::open(&home).unwrap();
    connected(&home, "pg", "postgresql://alice@127.0.0.1:1/app");
    // process 1 is always alive: its lease holds the Store
    let lease = paths::lease_dir(&home, "pg").join("1");
    std::fs::create_dir_all(lease.parent().unwrap()).unwrap();
    std::fs::write(&lease, "1\n").unwrap();
    let args = SdkValue::map([("name", SdkValue::text("pg"))]);
    let disconnect = |home: &Path| {
        layerfs_api::call(Some(home), "store.disconnect", args.clone(), &mut |_| {})
            .map_err(|r| (r.code, r.message))
    };
    let (code, message) = disconnect(&home).unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    assert!(paths::store_record(&home, "pg").is_file());
    std::fs::remove_file(&lease).unwrap();
    disconnect(&home).unwrap();
    assert!(!paths::store_dir(&home, "pg").exists());
    let _ = std::fs::remove_dir_all(home);
}

/// A Store call on a PostgreSQL Store holds its lease while it runs: the
/// "server" accepts and answers nothing until the lease is seen.
#[cfg(feature = "postgresql")]
#[test]
fn a_postgresql_store_call_holds_the_stores_lease() {
    use std::time::{Duration, Instant};
    let home = tempdir("lease");
    home::open(&home).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    connected(
        &home,
        "pg",
        &format!("postgresql://alice@127.0.0.1:{port}/app"),
    );
    write_record(
        &paths::project_record(&home, "pg", "app"),
        &SdkValue::map([("name", SdkValue::text("app"))]),
    )
    .unwrap();
    let running = {
        let home = home.clone();
        std::thread::spawn(move || {
            let args = SdkValue::map([
                ("project", SdkValue::text("pg/app")),
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text("/a")),
            ]);
            layerfs_api::call(Some(&home), "file.read", args, &mut |_| {}).map_err(|r| r.code)
        })
    };
    let (stream, _) = listener.accept().unwrap();
    let lease = paths::lease_dir(&home, "pg").join(std::process::id().to_string());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !lease.is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(lease.is_file(), "the running call holds the lease");
    drop(stream);
    assert!(running.join().unwrap().is_err(), "nothing answered");
    assert!(!lease.exists(), "a Store that did not open keeps no lease");
    let _ = std::fs::remove_dir_all(home);
}

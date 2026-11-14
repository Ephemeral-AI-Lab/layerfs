//! #65: a PostgreSQL Sandbox create's reach check reads the schema's binding
//! and finds the project on one host session, before anything starts: a
//! project the schema does not hold is `NoSuchProject`, and a record whose
//! binding is another Store's is `Unavailable`, both as before. Needs
//! LAYERFS_TEST_PG_URL (a server URL without a password, to a database with
//! no `layerfs` schema; the test leaves one behind) and
//! LAYERFS_TEST_PG_PASSWORD; without them the test says so and passes.
#![cfg(unix)] // unix stat/lock semantics; the Windows face is its own port

#![cfg(all(feature = "postgresql", feature = "machine"))]
use layerfs_api::home::{self, paths};

use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-pgreach-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

#[test]
fn the_reach_check_finds_the_binding_and_the_project_on_one_session() {
    let (Ok(url), Ok(_)) = (
        std::env::var("LAYERFS_TEST_PG_URL"),
        std::env::var("LAYERFS_TEST_PG_PASSWORD"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_URL / LAYERFS_TEST_PG_PASSWORD unset");
        return;
    };
    let home = tempdir("home");
    home::open(&home).unwrap();
    call(
        &home,
        "store.connect",
        &[
            ("name", t("pg")),
            ("url", t(&url)),
            ("password", t("env:LAYERFS_TEST_PG_PASSWORD")),
        ],
    )
    .unwrap();
    // a project record whose project the schema does not hold
    home::write_record(
        &paths::project_record(&home, "pg", "ghost"),
        &SdkValue::map([("name", t("ghost"))]),
    )
    .unwrap();
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[("project", t("pg/ghost")), ("name", t("g1"))],
    )
    .unwrap_err();
    assert_eq!(code, "NoSuchProject", "{message}");
    assert_eq!(
        message, "no project ghost in the PostgreSQL Store's schema layerfs",
        "{message}"
    );
    assert!(!paths::sandbox_dir(&home, "g1").exists(), "nothing started");

    // the same server under a record whose binding is another Store's
    let path = paths::store_record(&home, "pg");
    let record = home::read_record(&path).unwrap().unwrap();
    let Some(SdkValue::Bytes(mut binding)) = record.get("binding").cloned() else {
        panic!("the record holds its binding");
    };
    binding[0] ^= 1;
    home::write_record(
        &path,
        &home::with_field(record, "binding", SdkValue::Bytes(binding)),
    )
    .unwrap();
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[("project", t("pg/ghost")), ("name", t("g2"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("holds another Store than pg"), "{message}");
}

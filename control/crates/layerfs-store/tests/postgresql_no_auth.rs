//! A PostgreSQL Store recorded before saved logins (a `password` reference
//! and no `auth`) is not dialled with that reference (CUT, owner
//! 2026-11-19): every call is refused `InvalidInput` and its state is
//! `broken` `outdated`, each with the fix that saves a login and names it.
//! The record is only read, never converted.
use layerfs_bridge::control::SdkValue;
use layerfs_home::state::{Reason, State};
use layerfs_store::record::{url, url_on, StoreRecord};
use std::path::Path;

const FIX: &str = "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store update pg --auth <auth>";

fn older(home: &Path) -> StoreRecord {
    let fields = [
        ("name", SdkValue::text("pg")),
        ("kind", SdkValue::text("postgresql")),
        (
            "url",
            SdkValue::text("postgresql://owner@db.example:5432/layerfs_db"),
        ),
        ("schema", SdkValue::text("layerfs")),
        ("password", SdkValue::text("env:LAYERFS_TEST_PG_PASSWORD")),
        (
            "machines",
            SdkValue::map([(
                "lab",
                SdkValue::map([("password", SdkValue::text("file:/no/such/file"))]),
            )]),
        ),
    ];
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v));
    StoreRecord::from_record("pg", SdkValue::Map(fields.collect())).in_home(home)
}

#[test]
fn a_record_with_no_saved_login_is_refused_with_the_save_auth_fix() {
    let home = std::env::temp_dir().join(format!("layerfs-pg-no-auth-{}", std::process::id()));
    let record = older(&home);
    for refused in [
        url(&record).unwrap_err(),
        url_on(&record, "lab").unwrap_err(),
    ] {
        assert_eq!(refused.code, "InvalidInput", "{}", refused.message);
        assert_eq!(refused.detail.get_text("fix"), Some(FIX));
    }
    // a password reference is no address of a machine's own
    assert!(record.own_connections().is_empty());
}

#[test]
fn its_state_is_broken_outdated_with_the_same_fix() {
    let home = std::env::temp_dir().join(format!("layerfs-pg-no-auth-s-{}", std::process::id()));
    let state = layerfs_store::state::recorded_under(&older(&home), None).unwrap();
    assert_eq!(state, State::broken(Reason::Outdated, Some(FIX)));
    // a record naming its login reads ready
    let named = older(&home).with("auth", SdkValue::text("pg-app"));
    let state = layerfs_store::state::recorded_under(&named, None).unwrap();
    assert_eq!(state, State::Ready);
}

/// A record naming its saved login is dialled with it: the login's user
/// and password go into the record's URL (which keeps neither), for the
/// Store and for a machine's own address; nothing needs a server.
#[test]
fn a_saved_login_goes_into_the_dialled_url() {
    let home = std::env::temp_dir().join(format!("layerfs-pg-login-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    layerfs_home::home::ensure(&home).unwrap();
    layerfs_store::postgresql::login::save(&home, "pg-app", "alice", "s3 cret").unwrap();
    let record = StoreRecord::connected(
        "pg",
        "postgresql://db.example:5432/layerfs_db",
        "layerfs",
        "local",
        Some("pg-app"),
    )
    .in_home(&home);
    assert_eq!(
        url(&record).unwrap(),
        "postgresql://alice:s3%20cret@db.example:5432/layerfs_db"
    );
    assert_eq!(
        url_on(&record, "lab").unwrap(),
        "postgresql://alice:s3%20cret@db.example:5432/layerfs_db"
    );
    std::fs::remove_dir_all(home).unwrap();
}

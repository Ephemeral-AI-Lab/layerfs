//! `store.upgrade` and `--with format` (#804 §11 families A, B and M) on
//! the frozen fixtures, written by a real old build:
//!
//! - A, golden upgrade: the SQLite schema-2 fixture (and the PostgreSQL
//!   schema-3 one) upgrades to the latest format with every Branch head,
//!   every Commit id and the Commit count unchanged and every file at every
//!   head byte-identical (sha256), as the old build's manifest says;
//! - B, works afterwards: the upgraded Store takes a Commit, a merge and a
//!   compaction, and a second upgrade answers `current` with nothing done;
//! - M, format display: `store get --with format` and each `store list`
//!   item for current, outdated, unsupported and newer Stores, including a
//!   Store this build refuses to open, where `--with stats` fails.
//!
//! INTEGRATION: these need the engine and control halves (#804 W1, W2); on a
//! build without them each test says NOT_RUN and passes. The PostgreSQL
//! cases also need `LAYERFS_TEST_POSTGRES_URL` and `LAYERFS_TEST_PSQL` (see
//! `upgrade_support`). A mount of the upgraded Store is not tested here: it
//! needs a Linux daemon; B covers Commit, merge and compact.
#[path = "../support/upgrade.rs"]
mod upgrade_support;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;
use upgrade_support::*;

/// `store.upgrade fx`'s long answer and the phases it sent.
fn upgrade(home: &Path) -> (SdkValue, Vec<String>) {
    let (outcome, events) = call_events(home, "store.upgrade", &[("store", t("fx")), DETAIL]);
    let answer = outcome.unwrap_or_else(|e| panic!("store.upgrade: {e:?}"));
    (answer, phases(&events))
}

/// `store get fx --with format`'s section.
fn format(home: &Path) -> SdkValue {
    let got = ok(
        home,
        "store.get",
        &[("store", t("fx")), with(&["format"]), DETAIL],
    );
    got.get("format").cloned().unwrap_or_default()
}

fn section(version: Option<i64>, latest: i64, status: &str, fix: Option<&str>) -> SdkValue {
    SdkValue::map([
        ("version", version.map(SdkValue::Int).unwrap_or_default()),
        ("latest", SdkValue::Int(latest)),
        ("status", t(status)),
        ("fix", fix.map(t).unwrap_or_default()),
    ])
}

/// A then B on a fixture home whose Store `fx` is of `engine` at
/// `before`, upgraded to `after` with `phases`.
fn golden_then_works(
    home: &Path,
    engine: &str,
    manifest_name: &str,
    before: i64,
    after: i64,
    want_phases: &[&str],
) {
    let manifest = manifest(manifest_name);
    // M on a Store this build refuses to open: outdated, with the fix
    let fix = "layerfs store upgrade fx";
    assert_eq!(
        format(home),
        section(Some(before), after, "outdated", Some(fix))
    );
    let stats = call(
        home,
        "store.get",
        &[("store", t("fx")), with(&["stats"]), DETAIL],
    );
    let refused = stats.unwrap_err();
    assert_eq!(refused.code, "Unsupported", "{refused:?}");
    assert_eq!(refused.detail.get_text("code"), Some("OldStore"));
    assert_eq!(refused.detail.get_text("fix"), Some(fix));
    // A: one upgrade, then the manifest holds
    let (answer, sent) = upgrade(home);
    assert_eq!(
        answer,
        SdkValue::map([
            ("outcome", t("upgraded")),
            ("store", t("fx")),
            ("engine", t(engine)),
            ("format_before", SdkValue::Int(before)),
            ("format_after", SdkValue::Int(after)),
            (
                "steps",
                SdkValue::List(vec![t(&format!("{before}->{after}"))])
            ),
            ("warnings", SdkValue::List(vec![])),
        ])
    );
    assert_eq!(sent, want_phases);
    assert_eq!(format(home), section(Some(after), after, "current", None));
    matches_manifest(home, &manifest, true);
    // B: a second upgrade answers current and does nothing
    let (again, sent) = upgrade(home);
    assert_eq!(again.get_text("outcome"), Some("current"), "{again:?}");
    assert_eq!(again.get_int("format_before"), Some(after));
    assert_eq!(again.get_int("format_after"), Some(after));
    assert_eq!(again.get("steps"), Some(&SdkValue::List(vec![])));
    assert_eq!(sent.first().map(String::as_str), Some("check"), "{sent:?}");
    assert!(!sent.iter().any(|phase| phase == "commit"), "{sent:?}");
    // the short form
    let short = ok(home, "store.upgrade", &[("store", t("fx"))]);
    assert_eq!(short, SdkValue::map([("outcome", t("current"))]));
    // B: a Commit, a fork, a merge and a compaction on the upgraded Store
    let written = ok(
        home,
        "file.write",
        &[
            ("project", t("fx/web")),
            ("branch", t("feature")),
            ("path", t("/after-upgrade.txt")),
            ("content", t("written after the upgrade\n")),
            ("message", t("after upgrade")),
        ],
    );
    let commit = written.get_text("commit").unwrap().to_owned();
    ok(
        home,
        "branch.fork",
        &[
            ("project", t("fx/web")),
            ("name", t("upgraded")),
            ("from", t("other")),
        ],
    );
    ok(
        home,
        "file.write",
        &[
            ("project", t("fx/web")),
            ("branch", t("upgraded")),
            ("path", t("/on-upgraded.txt")),
            ("content", t("a Branch forked after the upgrade\n")),
            ("message", t("on upgraded")),
        ],
    );
    let merged = ok(
        home,
        "branch.merge",
        &[
            ("project", t("fx/web")),
            ("branch", t("feature")),
            ("from", t("upgraded")),
            ("message", t("merge after upgrade")),
            DETAIL,
        ],
    );
    assert_eq!(
        merged.get_text("parent"),
        Some(commit.as_str()),
        "{merged:?}"
    );
    for path in ["/after-upgrade.txt", "/on-upgraded.txt", "/other.txt"] {
        ok(
            home,
            "file.stat",
            &[
                ("project", t("fx/web")),
                ("branch", t("feature")),
                ("path", t(path)),
            ],
        );
    }
    ok(home, "store.compact", &[("store", t("fx"))]);
    let read = ok(
        home,
        "file.read",
        &[
            ("project", t("fx/web")),
            ("branch", t("feature")),
            ("path", t("/after-upgrade.txt")),
        ],
    );
    assert_eq!(read.get_text("text"), Some("written after the upgrade\n"));
    // the rest of the history is as the old build left it
    let (again, _) = upgrade(home);
    assert_eq!(again.get_text("outcome"), Some("current"));
    let lib_only = SdkValue::map([(
        "projects",
        SdkValue::map([(
            "lib",
            manifest
                .get("projects")
                .unwrap()
                .get("lib")
                .unwrap()
                .clone(),
        )]),
    )]);
    matches_manifest(home, &lib_only, false);
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
}

/// INTEGRATION. Families A and B on the SQLite schema-2 fixture.
#[test]
fn sqlite_v2_fixture_upgrades_keeping_everything_and_works_afterwards() {
    let Some(home) = sqlite_fixture_home("sqlite-golden") else {
        return;
    };
    if !built(&home, "fx") {
        return;
    }
    golden_then_works(
        &home,
        "sqlite",
        "sqlite/v2",
        2,
        3,
        &["check", "convert", "verify", "commit"],
    );
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}

/// INTEGRATION. Families A and B on the PostgreSQL schema-3 fixture.
#[test]
fn postgresql_v3_fixture_upgrades_keeping_everything_and_works_afterwards() {
    let Some(server) = Postgres::from_env() else {
        return;
    };
    let db = format!("lfs_upgrade_{}", std::process::id());
    let home = postgresql_fixture_home(&server, "pg-golden", &db);
    if built(&home, "fx") {
        golden_then_works(
            &home,
            "postgresql",
            "postgresql/v3",
            3,
            4,
            &["check", "convert"],
        );
    }
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
    server.drop_database(&db);
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}

/// Sets `PRAGMA user_version` of Store `store`'s closed file: the header
/// alone, which is all `--with format` reads.
fn set_user_version(home: &Path, store: &str, version: i64) {
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
    let file = home.join("store").join(store).join("store.sqlite");
    rusqlite::Connection::open(file)
        .unwrap()
        .execute_batch(&format!("PRAGMA user_version={version}"))
        .unwrap();
}

/// INTEGRATION. Family M: the format section of current, outdated,
/// unsupported (schema 1 by name, a transient version, an unrecognized
/// one) and newer SQLite Stores, from the header alone, in `store get` and
/// in each `store list` item; plain get and list stay records-only.
#[test]
fn the_format_section_reads_each_status_from_the_header_alone() {
    let home = tempdir("format").join("home");
    ok(&home, "home.init", &[]);
    if !{
        ok(&home, "store.create", &[("name", t("fx"))]);
        built(&home, "fx")
    } {
        return;
    }
    // a fresh Store is current
    assert_eq!(format(&home), section(Some(3), 3, "current", None));
    // plain get and list answer no format
    let plain = ok(&home, "store.get", &[("store", t("fx")), DETAIL]);
    assert!(plain.get("format").is_none(), "{plain:?}");
    let remove = "layerfs store remove fx";
    let cases: [(i64, SdkValue); 6] = [
        (
            36,
            section(Some(2), 3, "outdated", Some("layerfs store upgrade fx")),
        ),
        (
            44,
            section(Some(2), 3, "outdated", Some("layerfs store upgrade fx")),
        ),
        (16, section(Some(1), 3, "unsupported", Some(remove))),
        (45, section(None, 3, "unsupported", Some(remove))),
        (5, section(None, 3, "unsupported", Some(remove))),
        (70, section(None, 3, "newer", None)),
    ];
    for (version, want) in cases {
        set_user_version(&home, "fx", version);
        let have = format(&home);
        // a transient or retired version may name its version or not; its
        // status and fix are what the section promises
        if want.get_text("status") == Some("unsupported") || version == 70 {
            for key in ["latest", "status", "fix"] {
                assert_eq!(
                    have.get(key),
                    want.get(key),
                    "user_version {version}: {key}"
                );
            }
        } else {
            assert_eq!(have, want, "user_version {version}");
        }
        // the Store does not open: stats fails, format answers. (`--with
        // live` on a native sqlite Store reads the file's presence and the
        // leases without opening it, so it answers `ready` here, as on the
        // base build; it is not asserted.)
        let stats = call(&home, "store.get", &[("store", t("fx")), with(&["stats"])]);
        assert!(stats.is_err(), "user_version {version}: stats {stats:?}");
        // a newer Store is never called outdated
        if version == 70 {
            let refused = stats.unwrap_err();
            assert!(!refused.message.contains("outdated"), "{refused:?}");
        }
    }
    // store list: each item's format, beside a current Store
    ok(&home, "store.create", &[("name", t("shop"))]);
    set_user_version(&home, "fx", 36);
    let listed = ok(&home, "store.list", &[with(&["format"]), DETAIL]);
    let items = list(listed.get("items"));
    let of = |name: &str| {
        items
            .iter()
            .find(|item| item.get_text("store") == Some(name))
            .and_then(|item| item.get("format").cloned())
            .unwrap_or_default()
    };
    assert_eq!(
        of("fx"),
        section(Some(2), 3, "outdated", Some("layerfs store upgrade fx"))
    );
    assert_eq!(of("shop"), section(Some(3), 3, "current", None));
    let plain = ok(&home, "store.list", &[DETAIL]);
    assert!(list(plain.get("items"))
        .iter()
        .all(|item| item.get("format").is_none()));
    // the short form keeps the section's news: no null fix
    let short = ok(
        &home,
        "store.get",
        &[("store", t("shop")), with(&["format"])],
    );
    assert_eq!(
        short.get("format"),
        Some(&SdkValue::map([
            ("version", SdkValue::Int(3)),
            ("latest", SdkValue::Int(3)),
            ("status", t("current")),
        ]))
    );
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}

/// INTEGRATION. Family M on PostgreSQL: a connected current Store reads
/// `current` 4, the fixture `outdated` 3 with the upgrade as its fix.
#[test]
fn the_postgresql_format_section_reads_current_and_outdated() {
    let Some(server) = Postgres::from_env() else {
        return;
    };
    let db = format!("lfs_format_{}", std::process::id());
    let home = postgresql_fixture_home(&server, "pg-format", &db);
    if built(&home, "fx") {
        assert_eq!(
            format(&home),
            section(Some(3), 4, "outdated", Some("layerfs store upgrade fx"))
        );
        let fresh = format!("{db}_fresh");
        server.drop_database(&fresh);
        server
            .client("postgres")
            .batch_execute(&format!("CREATE DATABASE {fresh}"))
            .unwrap();
        let (url, user) = server.store_url(&fresh);
        ok(
            &home,
            "store.connect",
            &[
                ("store", t("cur")),
                ("url", t(&url)),
                ("user", t(&user)),
                ("password", t("unused")),
            ],
        );
        let got = ok(
            &home,
            "store.get",
            &[("store", t("cur")), with(&["format"]), DETAIL],
        );
        assert_eq!(
            got.get("format"),
            Some(&section(Some(4), 4, "current", None))
        );
        layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
        server.drop_database(&fresh);
    }
    layerfs_store::admin::with_users::close_idle(std::time::Duration::ZERO);
    server.drop_database(&db);
    let _ = std::fs::remove_dir_all(home.parent().unwrap());
}

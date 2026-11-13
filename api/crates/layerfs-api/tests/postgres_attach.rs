//! `store.attach` and `store.detach` (supersede #536 C4, take up H9): a
//! PostgreSQL Store's Sandboxes run on the machines it is attached to,
//! each with the Store's connection or one of its own, checked from that
//! machine before it is recorded.
//!
//! The first test needs no server: its URLs name 127.0.0.1 port 1, where
//! nothing listens. The second needs LAYERFS_TEST_PG_URL (a server URL
//! without a password, to a database with no `layerfs` schema; the test
//! leaves one behind) and LAYERFS_TEST_PG_PASSWORD; with
//! LAYERFS_TEST_PG_LIMITED_URL and LAYERFS_TEST_PG_LIMITED_PASSWORD (a role
//! on that database that may read LayerFS's tables but not write them) it
//! also checks the privilege refusal. Without them it says so and passes.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-pgattach-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
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

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn machines(store: &SdkValue) -> Vec<String> {
    match store.get("machines") {
        Some(SdkValue::Map(machines)) => machines.iter().map(|(m, _)| m.clone()).collect(),
        other => panic!("no machines: {other:?}"),
    }
}

fn record_text(home: &Path, store: &str) -> String {
    std::fs::read_to_string(paths::store_record(home, store)).unwrap()
}

#[test]
fn attach_and_detach_refusals_need_no_server() {
    let home = tempdir("refusals");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    // a record written before attachments: attached to native alone
    write_record(
        &paths::store_record(&home, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgresql://alice@127.0.0.1:1/app")),
            ("schema", t("layerfs")),
            ("synchronous_commit", t("off")),
            ("binding", SdkValue::Bytes(vec![7; 32])),
            ("cursor", SdkValue::Bytes(vec![9; 32])),
        ]),
    )
    .unwrap();
    write_record(
        &paths::project_record(&home, "pg", "app"),
        &SdkValue::map([("name", t("app"))]),
    )
    .unwrap();
    write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([("name", t("shop")), ("kind", t("sqlite"))]),
    )
    .unwrap();
    let shown = ok(&home, "store.get", &[("name", t("pg"))]);
    assert_eq!(machines(&shown), ["native"]);
    let native = ok(&home, "machine.get", &[("name", t("native"))]);
    assert_eq!(
        native.get("postgresql"),
        Some(&SdkValue::List(vec![t("pg")]))
    );
    let before = record_text(&home, "pg");

    let attach = |args: &[(&str, SdkValue)]| call(&home, "store.attach", args).unwrap_err();
    let (code, message) = attach(&[("name", t("nope")), ("machine", t("native"))]);
    assert_eq!(code, "NotFound", "{message}");
    let (code, message) = attach(&[("name", t("shop")), ("machine", t("native"))]);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("`store push shop <machine>`"), "{message}");
    let (code, message) = attach(&[("name", t("pg")), ("machine", t("nowhere"))]);
    assert_eq!(code, "NotFound", "{message}");
    let (code, message) = attach(&[
        ("name", t("pg")),
        ("machine", t("native")),
        ("url", t("postgresql://alice:secret@127.0.0.1:1/app")),
    ]);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(!message.contains("secret"), "{message}");
    let (code, message) = attach(&[
        ("name", t("pg")),
        ("machine", t("native")),
        ("password", t("env:LAYERFS_TEST_ATTACH_UNSET_PASSWORD")),
    ]);
    // the reference is resolved before any connection, as connect's is
    assert!(
        message.contains("LAYERFS_TEST_ATTACH_UNSET_PASSWORD is not set"),
        "{code}: {message}"
    );
    // the check from native finds no server: nothing is recorded
    let (code, message) = attach(&[("name", t("pg")), ("machine", t("native"))]);
    if cfg!(feature = "postgresql") {
        assert_eq!(code, "Unavailable", "{message}");
        assert!(
            message.starts_with(
                "machine native cannot reach PostgreSQL Store pg's server at 127.0.0.1: "
            ),
            "{message}"
        );
    } else {
        assert_eq!(code, "Unsupported", "{message}");
    }
    assert_eq!(
        record_text(&home, "pg"),
        before,
        "a refused attach writes nothing"
    );

    let detach = |machine: &str| {
        call(
            &home,
            "store.detach",
            &[("name", t("pg")), ("machine", t(machine))],
        )
    };
    let (code, message) = detach("docker").unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert_eq!(message, "Store pg is not attached to machine docker");
    // a Sandbox recorded on native holds the attachment
    write_record(
        &paths::sandbox_record(&home, "s"),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", t("s")),
            ("project", t("pg/app")),
            ("machine", t("native")),
        ]),
    )
    .unwrap();
    let (code, message) = detach("native").unwrap_err();
    assert_eq!(code, "InUse", "{message}");
    std::fs::remove_dir_all(paths::sandbox_dir(&home, "s")).unwrap();
    let shown = detach("native").unwrap();
    assert!(machines(&shown).is_empty(), "{shown:?}");
    let native = ok(&home, "machine.get", &[("name", t("native"))]);
    assert_eq!(native.get("postgresql"), Some(&SdkValue::List(Vec::new())));
    // a machine it is not attached to runs none of its Sandboxes
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[("project", t("pg/app")), ("name", t("t"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unsupported", "{message}");
    assert_eq!(
        message,
        "PostgreSQL Store pg is not attached to machine native; `layerfs store attach pg native`"
    );
    assert!(!paths::sandbox_dir(&home, "t").exists());
    let _ = std::fs::remove_dir_all(home);
}

#[cfg(feature = "postgresql")]
#[test]
fn attach_checks_the_machine_s_connection_before_it_records_it() {
    let (Ok(url), Ok(_)) = (
        std::env::var("LAYERFS_TEST_PG_URL"),
        std::env::var("LAYERFS_TEST_PG_PASSWORD"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_URL / LAYERFS_TEST_PG_PASSWORD unset");
        return;
    };
    let home = tempdir("live");
    home::open(&home).unwrap();
    let connected = ok(
        &home,
        "store.connect",
        &[
            ("name", t("pg")),
            ("url", t(&url)),
            ("password", t("env:LAYERFS_TEST_PG_PASSWORD")),
        ],
    );
    // connect attaches native, its check recorded
    let native = connected.get("machines").and_then(|m| m.get("native"));
    let checked = native.and_then(|n| n.get("checked"));
    assert!(
        checked.and_then(|c| c.get_text("server_version")).is_some(),
        "{connected:?}"
    );
    // native again with a connection of its own: the same server, the
    // password by reference; the record holds no password
    let shown = ok(
        &home,
        "store.attach",
        &[
            ("name", t("pg")),
            ("machine", t("native")),
            ("url", t(&url)),
            ("password", t("env:LAYERFS_TEST_PG_PASSWORD")),
        ],
    );
    let native = shown.get("machines").and_then(|m| m.get("native")).unwrap();
    assert_eq!(native.get_text("url"), Some(url.as_str()));
    assert_eq!(
        native.get_text("password"),
        Some("env:LAYERFS_TEST_PG_PASSWORD")
    );
    let secret = std::env::var("LAYERFS_TEST_PG_PASSWORD").unwrap();
    assert!(!record_text(&home, "pg").contains(&secret));
    // another database is not this Store: refused, nothing recorded
    let before = record_text(&home, "pg");
    let other = match url.rsplit_once('/') {
        Some((server, _)) => format!("{server}/postgres"),
        None => url.clone(),
    };
    let (code, message) = call(
        &home,
        "store.attach",
        &[
            ("name", t("pg")),
            ("machine", t("native")),
            ("url", t(&other)),
            ("password", t("env:LAYERFS_TEST_PG_PASSWORD")),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("holds another Store than pg"), "{message}");
    // the server answered: no "cannot reach" and no reach hint
    assert!(!message.contains("cannot reach"), "{message}");
    assert_eq!(record_text(&home, "pg"), before);
    // a role that may not write is refused naming what it lacks
    if let (Ok(limited), Ok(_)) = (
        std::env::var("LAYERFS_TEST_PG_LIMITED_URL"),
        std::env::var("LAYERFS_TEST_PG_LIMITED_PASSWORD"),
    ) {
        let (code, message) = call(
            &home,
            "store.attach",
            &[
                ("name", t("pg")),
                ("machine", t("native")),
                ("url", t(&limited)),
                ("password", t("env:LAYERFS_TEST_PG_LIMITED_PASSWORD")),
            ],
        )
        .unwrap_err();
        assert_eq!(code, "Denied", "{message}");
        assert!(message.contains(" lacks "), "{message}");
        assert_eq!(record_text(&home, "pg"), before);
    } else {
        eprintln!("NOT_RUN (privileges): LAYERFS_TEST_PG_LIMITED_URL unset");
    }
    ok(&home, "store.disconnect", &[("name", t("pg"))]);
    let _ = std::fs::remove_dir_all(home);
}

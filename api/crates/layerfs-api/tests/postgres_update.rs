//! A PostgreSQL Store's connections (#536 C4 again, with H9): its
//! Sandboxes run on every machine whose platform runs them, with no step
//! before the create; `store.update` changes the Store's own connection, or
//! with `--machine` gives one machine its own, checked from that machine
//! before it is saved.
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
    let dir = std::env::temp_dir().join(format!("layerfs-pgupdate-{tag}-{}", std::process::id()));
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

fn own(store: &SdkValue) -> Vec<String> {
    match store.get("machines") {
        Some(SdkValue::Map(machines)) => machines.iter().map(|(m, _)| m.clone()).collect(),
        None => Vec::new(),
        other => panic!("machines: {other:?}"),
    }
}

fn record_text(home: &Path, store: &str) -> String {
    std::fs::read_to_string(paths::store_record(home, store)).unwrap()
}

#[test]
fn update_and_check_refusals_need_no_server() {
    let home = tempdir("refusals");
    home::open(&home).unwrap();
    ok(&home, "machine.list", &[]);
    // a record written while machines were attached one by one: native
    // attached with a check, docker with a connection of its own
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
            (
                "machines",
                SdkValue::map([
                    (
                        "native",
                        SdkValue::map([("checked", SdkValue::map([("at", SdkValue::Int(1))]))]),
                    ),
                    (
                        "docker",
                        SdkValue::map([("url", t("postgresql://bob@127.0.0.1:1/app"))]),
                    ),
                ]),
            ),
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
    // only a machine with its own connection shows, without the old check
    // (`store.get` answers them as `connections` rows: the Store's own,
    // with no machine, then each machine's own)
    let detail = ("detail", SdkValue::Bool(true));
    let shown = ok(&home, "store.get", &[("store", t("pg")), detail]);
    let Some(SdkValue::List(connections)) = shown.get("connections") else {
        panic!("{shown:?}")
    };
    let machines: Vec<_> = (connections.iter())
        .filter_map(|row| row.get_text("machine"))
        .collect();
    assert_eq!(machines, ["docker"], "{shown:?}");
    let docker = &connections[1];
    assert_eq!(
        docker.get_text("url"),
        Some("postgresql://bob@127.0.0.1:1/app")
    );
    assert!(docker.get("checked").is_none(), "{shown:?}");
    let native = ok(&home, "machine.get", &[("name", t("native"))]);
    assert!(native.get("postgresql").is_none(), "{native:?}");
    let before = record_text(&home, "pg");

    let update = |args: &[(&str, SdkValue)]| call(&home, "store.update", args).unwrap_err();
    let on = |machine: &str| [("name", t("pg")), ("machine", t(machine))];
    let (code, message) = update(&[("name", t("nope")), ("url", t("postgresql://a@h/d"))]);
    assert_eq!(code, "NotFound", "{message}");
    let (code, message) = update(&[("name", t("shop")), ("machine", t("native"))]);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("sqlite Store"), "{message}");
    let (code, message) = update(&[("name", t("pg"))]);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.starts_with("nothing to update"), "{message}");
    let (code, message) = update(&on("native"));
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.starts_with("nothing to update"), "{message}");
    let (code, message) = update(&[("name", t("pg")), ("clear", SdkValue::Bool(true))]);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("--machine"), "{message}");
    let mut args = on("native").to_vec();
    args.push(("synchronous_commit", t("off")));
    let (code, message) = update(&args);
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("the Store's setting"), "{message}");
    let mut args = on("docker").to_vec();
    args.extend([
        ("clear", SdkValue::Bool(true)),
        ("url", t("postgresql://a@h/d")),
    ]);
    let (code, message) = update(&args);
    assert_eq!(code, "InvalidInput", "{message}");
    let mut args = on("native").to_vec();
    args.push(("clear", SdkValue::Bool(true)));
    let (code, message) = update(&args);
    assert_eq!(code, "InvalidInput", "{message}");
    assert_eq!(
        message,
        "machine native has no connection of its own to Store pg"
    );
    let mut args = on("nowhere").to_vec();
    args.push(("url", t("postgresql://alice@127.0.0.1:1/app")));
    let (code, message) = update(&args);
    assert_eq!(code, "NotFound", "{message}");
    for machine in [None, Some("native")] {
        let mut args = vec![("name", t("pg"))];
        args.extend(machine.map(|m| ("machine", t(m))));
        args.push(("url", t("postgresql://alice:secret@127.0.0.1:1/app")));
        let (code, message) = update(&args);
        assert_eq!(code, "InvalidInput", "{message}");
        assert!(!message.contains("secret"), "{message}");
        args.pop();
        args.push(("password", t("env:LAYERFS_TEST_UPDATE_UNSET_PASSWORD")));
        let (code, message) = update(&args);
        // the reference is resolved before any connection, as connect's is
        assert!(
            message.contains("LAYERFS_TEST_UPDATE_UNSET_PASSWORD is not set"),
            "{code}: {message}"
        );
    }
    let (code, message) = update(&[("name", t("pg")), ("synchronous_commit", t("sometimes"))]);
    assert_eq!(code, "InvalidInput", "{message}");
    // the check from native finds no server: nothing is saved
    let mut args = on("native").to_vec();
    args.push(("url", t("postgresql://alice@127.0.0.1:1/app")));
    let (code, message) = update(&args);
    assert_eq!(code, "Unavailable", "{message}");
    assert!(
        message
            .starts_with("machine native cannot reach PostgreSQL Store pg's server at 127.0.0.1: "),
        "{message}"
    );
    // nor does the Store's own new URL reach one
    let (code, message) = update(&[
        ("name", t("pg")),
        ("url", t("postgresql://alice@127.0.0.1:1/other")),
    ]);
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.starts_with("PostgreSQL Store pg: "), "{message}");
    assert_eq!(
        record_text(&home, "pg"),
        before,
        "a refused update writes nothing"
    );

    // synchronous_commit alone dials nothing
    let shown = ok(
        &home,
        "store.update",
        &[("name", t("pg")), ("synchronous_commit", t("local"))],
    );
    assert_eq!(shown.get_text("synchronous_commit"), Some("local"));
    assert_eq!(own(&shown), ["docker"]);
    // clearing a machine's own connection dials nothing either, and the
    // record keeps no `machines` once none has one
    let mut args = on("docker").to_vec();
    args.push(("clear", SdkValue::Bool(true)));
    let shown = ok(&home, "store.update", &args);
    assert!(own(&shown).is_empty(), "{shown:?}");
    assert!(!record_text(&home, "pg").contains("machines"));

    // a Sandbox needs no attachment: the create goes on to its reach check,
    // which finds no server
    let (code, message) = call(
        &home,
        "sandbox.create",
        &[("project", t("pg/app")), ("name", t("t"))],
    )
    .unwrap_err();
    assert!(!message.contains("not attached"), "{code}: {message}");
    assert!(!paths::sandbox_dir(&home, "t").exists());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_machine_s_own_connection_is_checked_before_it_is_saved() {
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
    // connect records no machine: every machine dials the Store's connection
    assert!(connected.get("machines").is_none(), "{connected:?}");
    assert!(!record_text(&home, "pg").contains("machines"));
    let native = [("name", t("pg")), ("machine", t("native"))];
    // native with a connection of its own: the same server, the password
    // by reference; the record holds no password
    let mut args = native.to_vec();
    args.extend([
        ("url", t(&url)),
        ("password", t("env:LAYERFS_TEST_PG_PASSWORD")),
    ]);
    let shown = ok(&home, "store.update", &args);
    let own_native = shown.get("machines").and_then(|m| m.get("native")).unwrap();
    assert_eq!(own_native.get_text("url"), Some(url.as_str()));
    assert_eq!(
        own_native.get_text("password"),
        Some("env:LAYERFS_TEST_PG_PASSWORD")
    );
    assert!(own_native.get("checked").is_none(), "{shown:?}");
    let secret = std::env::var("LAYERFS_TEST_PG_PASSWORD").unwrap();
    assert!(!record_text(&home, "pg").contains(&secret));
    // another database is not this Store: refused, nothing saved, for the
    // machine's connection and for the Store's own
    let before = record_text(&home, "pg");
    let other = match url.rsplit_once('/') {
        Some((server, _)) => format!("{server}/postgres"),
        None => url.clone(),
    };
    let mut args = native.to_vec();
    args.push(("url", t(&other)));
    let (code, message) = call(&home, "store.update", &args).unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("holds another Store than pg"), "{message}");
    // the server answered: no "cannot reach" and no reach hint
    assert!(!message.contains("cannot reach"), "{message}");
    let (code, message) = call(
        &home,
        "store.update",
        &[("name", t("pg")), ("url", t(&other))],
    )
    .unwrap_err();
    assert!(
        matches!(code.as_str(), "Unavailable" | "Failed"),
        "{code}: {message}"
    );
    assert_eq!(record_text(&home, "pg"), before);
    // a role that may not write is refused naming what it lacks
    if let (Ok(limited), Ok(_)) = (
        std::env::var("LAYERFS_TEST_PG_LIMITED_URL"),
        std::env::var("LAYERFS_TEST_PG_LIMITED_PASSWORD"),
    ) {
        let mut args = native.to_vec();
        args.extend([
            ("url", t(&limited)),
            ("password", t("env:LAYERFS_TEST_PG_LIMITED_PASSWORD")),
        ]);
        let (code, message) = call(&home, "store.update", &args).unwrap_err();
        assert_eq!(code, "Unavailable", "{message}");
        assert!(message.contains(" lacks "), "{message}");
        assert_eq!(record_text(&home, "pg"), before);
    } else {
        eprintln!("NOT_RUN (privileges): LAYERFS_TEST_PG_LIMITED_URL unset");
    }
    // the Store's own connection, to the same database again: saved, and
    // its calls still open it
    let shown = ok(
        &home,
        "store.update",
        &[
            ("name", t("pg")),
            ("url", t(&url)),
            ("synchronous_commit", t("off")),
        ],
    );
    assert_eq!(shown.get_text("synchronous_commit"), Some("off"));
    ok(
        &home,
        "store.get",
        &[("store", t("pg")), ("with", t("stats"))],
    );
    let mut args = native.to_vec();
    args.push(("clear", SdkValue::Bool(true)));
    let shown = ok(&home, "store.update", &args);
    assert!(own(&shown).is_empty(), "{shown:?}");
    ok(&home, "store.disconnect", &[("name", t("pg"))]);
    let _ = std::fs::remove_dir_all(home);
}

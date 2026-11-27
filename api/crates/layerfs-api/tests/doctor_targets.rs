//! The six doctors (call surface 2.0) against temporary homes, records
//! only: each answers the one report in its row's shape, short and long; a
//! thing that is not there is refused; no doctor makes or converts a home;
//! and one progress event goes out per finished thing. The home's and a
//! machine's doctors, which ask a provider, are `doctor_home_machine.rs`'s.
use layerfs_api::home::{paths, write_entity};
use layerfs_home::json;
use layerfs_protocol::provision::install::SdkValue;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-doctor-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn t(text: &str) -> SdkValue {
    SdkValue::text(text)
}

fn args(pairs: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        (pairs.iter())
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

/// One call: its answer or `code: message`, and its progress events.
fn call(
    home: &Path,
    name: &str,
    pairs: &[(&str, SdkValue)],
) -> (Result<SdkValue, String>, Vec<SdkValue>) {
    let mut events = Vec::new();
    let answer = layerfs_api::call(Some(home), name, args(pairs), &mut |event| {
        events.push(event)
    })
    .map_err(|refusal| format!("{}: {}", refusal.code, refusal.message));
    (answer, events)
}

fn short(home: &Path, name: &str, pairs: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, pairs)
        .0
        .unwrap_or_else(|why| panic!("{name}: {why}"))
}

fn long(home: &Path, name: &str, pairs: &[(&str, SdkValue)]) -> SdkValue {
    let mut pairs = pairs.to_vec();
    pairs.push(("detail", SdkValue::Bool(true)));
    short(home, name, &pairs)
}

fn keys(value: &SdkValue) -> Vec<&str> {
    match value {
        SdkValue::Map(fields) => fields.iter().map(|(key, _)| key.as_str()).collect(),
        _ => Vec::new(),
    }
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

const LONG: [&str; 7] = [
    "ok",
    "findings",
    "not_checked",
    "status",
    "counts",
    "observed_at",
    "tree",
];

/// Checks a doctor's pair of answers against the one report: the long form
/// has its seven keys, the short form only `ok` alone or `findings` and
/// `not_checked` (each present only when it has entries), its entries
/// without the long form's `evidence` and nulls, and `counts` counts every
/// thing in the tree once.
fn check_pair(short: &SdkValue, long: &SdkValue) {
    assert_eq!(keys(long), LONG, "{long:?}");
    let clean = list(long, "findings").is_empty() && list(long, "not_checked").is_empty();
    assert_eq!(long.get("ok"), Some(&SdkValue::Bool(clean)));
    if clean {
        assert_eq!(json::print(short), r#"{"ok":true}"#);
    } else {
        assert!(short.get("ok").is_none(), "{short:?}");
        for key in keys(short) {
            assert!(["findings", "not_checked"].contains(&key), "{short:?}");
        }
    }
    for (short, long) in list(short, "findings").iter().zip(list(long, "findings")) {
        assert_eq!(
            &keys(long)[1..],
            ["check", "severity", "message", "fix", "from", "evidence"]
        );
        assert_eq!(keys(short)[0], keys(long)[0], "the thing's own word");
        assert!(!keys(short).contains(&"evidence"));
    }
    for (short, long) in list(short, "not_checked")
        .iter()
        .zip(list(long, "not_checked"))
    {
        assert_eq!(&keys(long)[1..], ["check", "blocked_by", "reason"]);
        let said = (
            short.get("blocked_by").is_some(),
            short.get("reason").is_some(),
        );
        assert!(said == (true, false) || said == (false, true), "{short:?}");
    }
    let tree = long.get("tree").unwrap();
    let mut things = 1;
    for (key, value) in match tree {
        SdkValue::Map(fields) => fields.as_slice(),
        _ => &[],
    } {
        if let SdkValue::List(nodes) = value {
            if key != "checks" {
                things += nodes.len();
            }
        }
    }
    let counted: i64 = match long.get("counts") {
        Some(SdkValue::Map(fields)) => fields
            .iter()
            .map(|(_, n)| match n {
                SdkValue::Int(n) => *n,
                _ => 0,
            })
            .sum(),
        _ => 0,
    };
    assert_eq!(counted, things as i64, "{long:?}");
}

#[test]
fn a_home_that_does_not_exist_is_reported_and_stays_absent() {
    let home = tempdir("absent");
    let (answer, events) = call(&home, "home.doctor", &[("detail", SdkValue::Bool(true))]);
    let long = answer.unwrap();
    assert!(!home.exists(), "the doctor made the home");
    assert_eq!(keys(&long), LONG);
    // nothing to check under it, and no layout or records to read
    let tree = long.get("tree").unwrap();
    assert_eq!(
        keys(tree),
        [
            "home",
            "status",
            "coverage",
            "checks",
            "blocked_by",
            "reason",
            "machines",
            "stores",
            "buckets"
        ]
    );
    assert!(list(tree, "machines").is_empty());
    let ran: Vec<&str> = (list(tree, "checks").iter())
        .filter_map(|row| row.get_text("check"))
        .collect();
    assert!(
        !ran.contains(&"home.layout") && !ran.contains(&"home.records"),
        "{ran:?}"
    );
    assert_eq!(events.len(), 1, "one thing, one event");
    let short = short(&home, "home.doctor", &[]);
    check_pair(&short, &long);
    assert!(!home.exists());
}

/// A sqlite Store record `name` on `machine`, with a made-up identity.
fn sqlite_store(home: &Path, name: &str, machine: &str) {
    write_entity(
        &paths::store_record(home, name),
        &SdkValue::map([
            ("name", t(name)),
            ("kind", t("sqlite")),
            ("machine", t(machine)),
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![7; 32])),
        ]),
    )
    .unwrap();
}

#[test]
fn a_project_s_record_is_checked_against_its_store() {
    let home = tempdir("project");
    short(&home, "home.init", &[]);
    sqlite_store(&home, "shop", layerfs_home::names::HOST_MACHINE);
    let manifest = layerfs_protocol::provision::manifest::StoreManifest {
        provider: layerfs_protocol::provision::manifest::ProviderKind::Sqlite,
        locator: "/layerfs-store/global/store.sqlite".into(),
        profile: layerfs_protocol::provision::manifest::StoreProfile::Normal,
        binding: vec![1; 16],
        cursor_key: [7; 32],
        stack: [2; 17],
        branch: [3; 17],
        root: [4; 32],
        bytes: 4096,
        host_sqlite: "3.45.1".to_owned(),
        daemon_sqlite: None,
    };
    let mut installed = manifest.clone();
    installed.daemon_sqlite = Some("3.45.1".to_owned());
    write_entity(
        &paths::project_record(&home, "shop", "old"),
        &SdkValue::map([
            ("name", t("old")),
            ("manifest", SdkValue::Bytes(manifest.encode().unwrap())),
            ("installed", SdkValue::Bytes(installed.encode().unwrap())),
        ]),
    )
    .unwrap();
    let (answer, events) = call(
        &home,
        "project.doctor",
        &[("project", t("shop/old")), ("detail", SdkValue::Bool(true))],
    );
    let report = answer.unwrap();
    let tree = report.get("tree").unwrap();
    assert_eq!(
        keys(tree),
        [
            "project",
            "status",
            "coverage",
            "checks",
            "blocked_by",
            "reason"
        ]
    );
    let rows = list(tree, "checks");
    assert_eq!(rows[0].get_text("check"), Some("project.record"));
    assert_eq!(rows[0].get_text("outcome"), Some("pass"));
    assert_eq!(rows[1].get_text("check"), Some("project.listed"));
    // the Store's file is not there: the listing waits on the Store
    assert_eq!(
        json::print(&list(&report, "not_checked")[0]),
        r#"{"project":"shop/old","check":"project.listed","blocked_by":{"store":"shop","reason":"missing"},"reason":null}"#,
        "{report:?}"
    );
    assert_eq!(events.len(), 1);
    // `--full` reads no data the Store's list does not reach: the same
    // waiting entry, and no `project.data` row
    let (full, _) = call(
        &home,
        "project.doctor",
        &[
            ("project", t("shop/old")),
            ("full", SdkValue::Bool(true)),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    let full = full.unwrap();
    let checks: Vec<_> = list(full.get("tree").unwrap(), "checks")
        .iter()
        .map(|row| row.get_text("check").map(str::to_owned))
        .collect();
    assert_eq!(
        checks,
        [
            Some("project.record".to_owned()),
            Some("project.listed".to_owned())
        ]
    );
    assert_eq!(list(&full, "not_checked"), list(&report, "not_checked"));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_bucket_s_record_is_its_first_check() {
    let home = tempdir("bucket");
    short(&home, "home.init", &[]);
    write_entity(
        &paths::bucket_record(&home, "b1"),
        &SdkValue::map([("name", t("b1"))]),
    )
    .unwrap();
    let (answer, events) = call(&home, "backup.doctor", &[("bucket", t("b1"))]);
    let short_form = answer.unwrap();
    let long_form = long(&home, "backup.doctor", &[("bucket", t("b1"))]);
    check_pair(&short_form, &long_form);
    let found = &list(&short_form, "findings")[0];
    assert_eq!(found.get_text("bucket"), Some("b1"));
    assert_eq!(found.get_text("check"), Some("bucket.configuration"));
    assert_eq!(found.get_text("severity"), Some("error"));
    assert!(events.is_empty(), "backup doctor sends no progress");
    assert_eq!(long_form.get("ok"), Some(&SdkValue::Bool(false)));
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn what_is_not_there_is_refused_and_no_home_is_made() {
    let home = tempdir("missing");
    for (name, pairs) in [
        ("machine.doctor", vec![("machine", t("dev"))]),
        ("store.doctor", vec![("store", t("shop"))]),
        ("project.doctor", vec![("project", t("shop/api"))]),
        ("sandbox.doctor", vec![("sandbox", t("sb"))]),
        ("backup.doctor", vec![("bucket", t("r2"))]),
    ] {
        let refused = call(&home, name, &pairs).0.unwrap_err();
        assert!(refused.starts_with("NotFound: "), "{name}: {refused}");
    }
    assert!(!home.exists(), "a doctor made the home");
    // `--machine` probes a PostgreSQL Store; a sqlite Store is refused
    short(&home, "home.init", &[]);
    sqlite_store(&home, "shop", layerfs_home::names::HOST_MACHINE);
    let refused = call(
        &home,
        "store.doctor",
        &[("store", t("shop")), ("machine", t("lab"))],
    )
    .0;
    assert!(refused.unwrap_err().starts_with("InvalidInput: "));
    // the old names and options are gone
    for gone in ["doctor.run", "home.check"] {
        assert!(call(&home, gone, &[]).0.is_err());
    }
    let refused = call(&home, "home.doctor", &[("timeout", t("1s"))])
        .0
        .unwrap_err();
    assert!(refused.starts_with("InvalidInput: "), "{refused}");
    let _ = std::fs::remove_dir_all(home);
}

/// A Store whose record alone makes it broken (UI pass 2, F6): the doctor's
/// finding carries the fix its state gives in `store.list` (a record of a
/// kind this build does not read: `store remove`). A PostgreSQL Store that
/// cannot be dialled fails `store.connection` before anything is dialled,
/// with its refusal's fix: its password not saved in this home, the
/// connect that saves it; its record naming no user, none.
#[test]
fn a_store_s_broken_record_gives_its_state_s_fix() {
    let home = tempdir("store-fix");
    short(&home, "home.init", &[]);
    for (store, user) in [("pg", None), ("gone", Some(("user", t("owner"))))] {
        let mut fields = vec![
            ("name", t(store)),
            ("kind", t("postgresql")),
            ("url", t("postgresql://127.0.0.1:1/layerfs_db")),
            ("schema", t("layerfs")),
            ("synchronous_commit", t("local")),
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![7; 32])),
        ];
        fields.extend(user);
        let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        write_entity(&paths::store_record(&home, store), &SdkValue::Map(fields)).unwrap();
    }
    write_entity(
        &paths::store_record(&home, "odd"),
        &SdkValue::map([("name", t("odd")), ("kind", t("mysql"))]),
    )
    .unwrap();
    let listed = short(&home, "store.list", &[]);
    let odd = (list(&listed, "items").iter())
        .find(|item| item.get_text("store") == Some("odd"))
        .cloned()
        .unwrap_or_else(|| panic!("{listed:?}"));
    assert_eq!(odd.get_text("fix"), Some("layerfs store remove odd"));
    let gone_fix = "layerfs store connect gone <url> --password-stdin";
    for (store, check, fix) in [
        ("pg", "store.connection", None),
        ("gone", "store.connection", Some(gone_fix)),
        ("odd", "store.record", Some("layerfs store remove odd")),
    ] {
        let short_form = short(&home, "store.doctor", &[("store", t(store))]);
        let long_form = long(&home, "store.doctor", &[("store", t(store))]);
        check_pair(&short_form, &long_form);
        for report in [&short_form, &long_form] {
            let found = (list(report, "findings").iter())
                .find(|f| f.get_text("check") == Some(check))
                .unwrap_or_else(|| panic!("{store}: {report:?}"));
            assert_eq!(found.get_text("store"), Some(store), "{found:?}");
            assert_eq!(found.get_text("severity"), Some("error"), "{found:?}");
            assert_eq!(found.get_text("fix"), fix, "{found:?}");
        }
    }
    let _ = std::fs::remove_dir_all(home);
}

/// A server that accepts the socket and then says nothing (a paused
/// container, area 4): `store doctor` fails `store.connection` within the
/// handshake limit, inside its 10 s budget, and records this computer's
/// reach as `failed` without dialling twice; `store get --with live` reads
/// the Store `broken` `unreachable` within the same limit.
#[test]
fn a_silent_server_fails_its_connection_within_the_handshake_limit() {
    use std::time::{Duration, Instant};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // holds every socket it accepts and never answers; it ends with the
    // test process, and no other thread waits on it
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            held.push(stream);
        }
    });
    let home = tempdir("silent");
    short(&home, "home.init", &[]);
    layerfs_store::postgresql::login::save(&home, "silent", "pw").unwrap();
    let url = format!("postgresql://127.0.0.1:{port}/layerfs_db?sslmode=disable");
    write_entity(
        &paths::store_record(&home, "silent"),
        &SdkValue::map([
            ("name", t("silent")),
            ("kind", t("postgresql")),
            ("url", t(&url)),
            ("user", t("owner")),
            ("schema", t("layerfs")),
            ("synchronous_commit", t("local")),
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![7; 32])),
        ]),
    )
    .unwrap();

    let started = Instant::now();
    let report = long(&home, "store.doctor", &[("store", t("silent"))]);
    let took = started.elapsed();
    assert!(took < Duration::from_secs(9), "{took:?}");
    let found = (list(&report, "findings").iter())
        .find(|f| f.get_text("check") == Some("store.connection"))
        .unwrap_or_else(|| panic!("{report:?}"));
    assert_eq!(found.get_text("severity"), Some("error"), "{found:?}");
    let message = found.get_text("message").unwrap();
    assert!(message.contains("did not answer within 5 s"), "{message}");
    assert!(list(&report, "not_checked")
        .iter()
        .all(|n| n.get_text("check") != Some("store.connection")));

    let got = long(&home, "store.get", &[("store", t("silent"))]);
    let here = got.get("reach").unwrap_or_else(|| panic!("{got:?}"));
    assert_eq!(here.get("ok"), Some(&SdkValue::Bool(false)), "{here:?}");

    let started = Instant::now();
    let live = long(
        &home,
        "store.get",
        &[
            ("store", t("silent")),
            ("with", SdkValue::List(vec![t("live")])),
        ],
    );
    let took = started.elapsed();
    assert!(took < Duration::from_secs(9), "{took:?}");
    assert_eq!(live.get_text("state"), Some("broken"), "{live:?}");
    assert_eq!(live.get_text("reason"), Some("unreachable"), "{live:?}");
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_sqlite_project_without_a_manifest_fails_project_doctor() {
    let home = tempdir("missing-manifest");
    short(&home, "home.init", &[]);
    sqlite_store(&home, "shop", layerfs_home::names::HOST_MACHINE);
    write_entity(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", t("api")),
            ("stack", t("0101010101010101010101010101010101")),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
    for full in [false, true] {
        let report = long(
            &home,
            "project.doctor",
            &[("project", t("shop/api")), ("full", SdkValue::Bool(full))],
        );
        assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
        let found = list(&report, "findings");
        assert!(
            found
                .iter()
                .any(|f| f.get_text("check") == Some("project.record")
                    && f.get_text("severity") == Some("error")),
            "full={full}: {report:?}"
        );
        // the record's failure hides no other check: the Store's list is
        // still checked
        let rows = list(report.get("tree").unwrap(), "checks");
        assert!(
            rows.iter()
                .any(|row| row.get_text("check") == Some("project.listed")),
            "full={full}: {report:?}"
        );
    }

    // PostgreSQL project records hold no manifest; project.record still passes.
    layerfs_store::postgresql::login::save(&home, "pg", "pw").unwrap();
    write_entity(
        &paths::store_record(&home, "pg"),
        &SdkValue::map([
            ("name", t("pg")),
            ("kind", t("postgresql")),
            ("url", t("postgresql://db.invalid:5432/layerfs_db")),
            ("user", t("owner")),
            ("schema", t("layerfs")),
            ("synchronous_commit", t("local")),
            ("binding", SdkValue::Bytes(vec![1; 16])),
            ("cursor", SdkValue::Bytes(vec![7; 32])),
        ]),
    )
    .unwrap();
    write_entity(
        &paths::project_record(&home, "pg", "app"),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", t("app")),
            ("stack", t("0101010101010101010101010101010101")),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
    let pg_report = long(&home, "project.doctor", &[("project", t("pg/app"))]);
    let rows = list(pg_report.get("tree").unwrap(), "checks");
    assert_eq!(rows[0].get_text("check"), Some("project.record"));
    assert_eq!(rows[0].get_text("outcome"), Some("pass"), "{pg_report:?}");
    let _ = std::fs::remove_dir_all(home);
}

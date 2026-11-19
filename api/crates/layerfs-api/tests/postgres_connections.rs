//! Who connects to a PostgreSQL Store's database (spec v23), through the
//! call table against a real server: `store.connect --machine` and
//! `store.doctor` record each side's reach, `store.get` lists every side
//! that connects with its word (`reached`) in both forms and the Store's
//! `binding`, `store.disconnect --machine` forgets a machine seen only by a
//! reach, a record with old fields reads `broken` `outdated` until `store
//! update --auth` writes it clean, a login that is not saved reads
//! `missing`, and `store.update --auth` racing `store.remove_auth` never
//! leaves a Store naming a removed login. Needs LAYERFS_TEST_PG_URL (a
//! server URL with its user and no password, to a database with no
//! `layerfs` schema; the test leaves one behind) and
//! LAYERFS_TEST_PG_PASSWORD; without them it says so and passes.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths};
use layerfs_bridge::control::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-pgconn-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call_as(
    home: &Path,
    name: &str,
    args: &[(&str, SdkValue)],
    detail: bool,
) -> Result<SdkValue, (String, String)> {
    let mut args: Vec<(String, SdkValue)> = args
        .iter()
        .map(|(n, v)| ((*n).to_owned(), v.clone()))
        .collect();
    args.push(("detail".to_owned(), SdkValue::Bool(detail)));
    layerfs_api::call(Some(home), name, SdkValue::Map(args), &mut |_| {})
        .map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call_as(home, name, args, true).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn rows(answer: &SdkValue) -> Vec<SdkValue> {
    match answer.get("connections") {
        Some(SdkValue::List(rows)) => rows.clone(),
        _ => panic!("connections: {answer:?}"),
    }
}

fn row<'a>(rows: &'a [SdkValue], machine: Option<&str>) -> &'a SdkValue {
    let mut found = rows.iter();
    found
        .find(|row| row.get_text("machine") == machine)
        .unwrap_or_else(|| panic!("{machine:?}: {rows:?}"))
}

/// The tests share one database: one at a time.
static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct Server {
    user: String,
    password: String,
    url: String,
}

fn server() -> Option<Server> {
    let (Ok(url), Ok(password)) = (
        std::env::var("LAYERFS_TEST_PG_URL"),
        std::env::var("LAYERFS_TEST_PG_PASSWORD"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_URL / LAYERFS_TEST_PG_PASSWORD unset");
        return None;
    };
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    Some(Server {
        user: user.to_owned(),
        password,
        url: format!("{scheme}://{address}"),
    })
}

fn save(home: &Path, server: &Server, auth: &str) {
    ok(
        home,
        "store.save_auth",
        &[
            ("auth", t(auth)),
            ("user", t(&server.user)),
            ("password", t(&server.password)),
        ],
    );
}

fn edit(home: &Path, store: &str, change: impl FnOnce(SdkValue) -> SdkValue) {
    let path = paths::store_record(home, store);
    let record = home::read_record(&path).unwrap().unwrap();
    home::write_record(&path, &change(record)).unwrap();
}

#[test]
fn every_side_that_connects_is_listed_with_what_was_seen() {
    let Some(server) = server() else { return };
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempdir("rows");
    home::open(&home).unwrap();
    save(&home, &server, "pg");
    let url = server.url.clone();
    ok(
        &home,
        "store.connect",
        &[("store", t("pg")), ("url", t(&url)), ("auth", t("pg"))],
    );
    // nothing seen yet: this computer's row only, with no word
    let got = ok(&home, "store.get", &[("store", t("pg"))]);
    let binding = got.get_text("binding").expect("binding");
    assert_eq!(binding.len(), 16, "{binding}");
    assert!(binding.chars().all(|c| c.is_ascii_hexdigit()), "{binding}");
    let listed = rows(&got);
    assert_eq!(listed.len(), 1, "{listed:?}");
    assert_eq!(row(&listed, None).get("reached"), Some(&SdkValue::Null));

    // the doctor records this computer's reach, measured
    ok(&home, "store.doctor", &[("store", t("pg"))]);
    let listed = rows(&ok(&home, "store.get", &[("store", t("pg"))]));
    let here = row(&listed, None);
    assert_eq!(here.get_text("reached"), Some("ok"), "{here:?}");
    let reach = here.get("reach").unwrap();
    assert!(reach.get_int("latency_ms").is_some(), "{reach:?}");
    assert!(reach.get_text("server_version").is_some(), "{reach:?}");

    // a machine's own address records its check
    ok(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("machine", t("native")),
        ],
    );
    let listed = rows(&ok(&home, "store.get", &[("store", t("pg"))]));
    let native = row(&listed, Some("native"));
    assert_eq!(native.get("own"), Some(&SdkValue::Bool(true)));
    assert_eq!(native.get_text("reached"), Some("ok"), "{native:?}");

    // a failed reach seen from a machine shows in the short form too
    let file = home::observed::file(&paths::store_record(&home, "pg"));
    let mut seen = home::read_record(&file).unwrap().unwrap();
    let failed = SdkValue::map([
        ("at", SdkValue::Int(1)),
        ("ok", SdkValue::Bool(false)),
        ("error", t("timed out")),
    ]);
    let mut reaches = seen
        .get("reach")
        .cloned()
        .unwrap_or(SdkValue::Map(Vec::new()));
    reaches = home::with_field(reaches, "lab", failed);
    seen = home::with_field(seen, "reach", reaches);
    home::write_record(&file, &seen).unwrap();
    let short = call_as(&home, "store.get", &[("store", t("pg"))], false).unwrap();
    let lab = row(&rows(&short), Some("lab")).clone();
    assert_eq!(lab.get_text("reached"), Some("failed"), "{lab:?}");
    assert!(lab.get("reach").is_none(), "short form: {lab:?}");
    // the list's rows carry it too
    let list = ok(&home, "store.list", &[]);
    let Some(SdkValue::List(items)) = list.get("items") else {
        panic!("{list:?}");
    };
    assert_eq!(
        row(&rows(&items[0]), Some("lab")).get_text("reached"),
        Some("failed")
    );

    // a machine seen only by a reach is forgotten by its disconnect
    ok(
        &home,
        "store.disconnect",
        &[("store", t("pg")), ("machine", t("lab"))],
    );
    let listed = rows(&ok(&home, "store.get", &[("store", t("pg"))]));
    assert!(listed.iter().all(|r| r.get_text("machine") != Some("lab")));
    let (code, _) = call_as(
        &home,
        "store.disconnect",
        &[("store", t("pg")), ("machine", t("lab"))],
        true,
    )
    .unwrap_err();
    assert_eq!(code, "NotFound");
    // the password is in the secret folder only
    for path in [paths::store_record(&home, "pg"), file] {
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&server.password), "{}", path.display());
    }
    ok(&home, "store.disconnect", &[("store", t("pg"))]);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_old_record_reads_outdated_until_its_update_writes_it_clean() {
    let Some(server) = server() else { return };
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempdir("old");
    home::open(&home).unwrap();
    save(&home, &server, "pg");
    ok(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&server.url)),
            ("auth", t("pg")),
        ],
    );
    let (scheme, address) = server.url.split_once("://").unwrap();
    let named = format!("{scheme}://{}@{address}", server.user);
    edit(&home, "pg", |record| {
        let record = home::with_field(record, "password", t("file:/no/such"));
        let record = home::with_field(record, "url", t(&named));
        let checked = SdkValue::map([("checked", SdkValue::map([("at", SdkValue::Int(1))]))]);
        home::with_field(record, "machines", SdkValue::map([("lab", checked)]))
    });
    let got = ok(&home, "store.get", &[("store", t("pg"))]);
    assert_eq!(got.get_text("state"), Some("broken"), "{got:?}");
    assert_eq!(got.get_text("reason"), Some("outdated"));
    assert_eq!(
        got.get_text("fix"),
        Some("layerfs store update pg --auth pg")
    );
    // the doctor reports the record outdated, with the same fix (area 15)
    let report = ok(&home, "store.doctor", &[("store", t("pg"))]);
    assert_eq!(report.get("ok"), Some(&SdkValue::Bool(false)), "{report:?}");
    let found = match report.get("findings") {
        Some(SdkValue::List(found)) => found.clone(),
        _ => panic!("findings: {report:?}"),
    };
    let record = (found.iter())
        .find(|f| f.get_text("check") == Some("store.record"))
        .unwrap_or_else(|| panic!("{report:?}"));
    assert_eq!(
        record.get_text("fix"),
        Some("layerfs store update pg --auth pg")
    );
    ok(
        &home,
        "store.update",
        &[("store", t("pg")), ("auth", t("pg"))],
    );
    let got = ok(&home, "store.get", &[("store", t("pg"))]);
    assert_eq!(got.get_text("state"), Some("ready"), "{got:?}");
    let text = std::fs::read_to_string(paths::store_record(&home, "pg")).unwrap();
    for gone in ["password", "checked", &format!("{}@", server.user)] {
        assert!(!text.contains(gone), "{gone}: {text}");
    }

    // a login the home does not hold: `missing`, with the save that names it
    edit(&home, "pg", |record| {
        home::with_field(record, "auth", t("gone"))
    });
    let got = ok(&home, "store.get", &[("store", t("pg"))]);
    assert_eq!(got.get_text("reason"), Some("missing"), "{got:?}");
    assert!(
        got.get_text("fix")
            .unwrap()
            .starts_with("layerfs store save-auth gone --user"),
        "{got:?}"
    );
    edit(&home, "pg", |record| {
        home::with_field(record, "auth", t("pg"))
    });
    ok(&home, "store.disconnect", &[("store", t("pg"))]);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn an_update_racing_a_login_removal_never_names_a_removed_login() {
    let Some(server) = server() else { return };
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempdir("race");
    home::open(&home).unwrap();
    save(&home, &server, "pg");
    ok(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&server.url)),
            ("auth", t("pg")),
        ],
    );
    for round in 0..8 {
        let next = format!("next-{round}");
        save(&home, &server, &next);
        let (updated, removed) = std::thread::scope(|scope| {
            let update = scope.spawn(|| {
                call_as(
                    &home,
                    "store.update",
                    &[("store", t("pg")), ("auth", t(&next))],
                    true,
                )
            });
            let remove =
                scope.spawn(|| call_as(&home, "store.remove_auth", &[("auth", t(&next))], true));
            (update.join().unwrap(), remove.join().unwrap())
        });
        assert!(
            updated.is_ok() != removed.is_ok(),
            "round {round}: update {updated:?}, remove {removed:?}"
        );
        let got = ok(&home, "store.get", &[("store", t("pg"))]);
        assert_eq!(
            got.get_text("state"),
            Some("ready"),
            "round {round}: {got:?}"
        );
        if updated.is_err() {
            assert_eq!(got.get_text("auth"), Some("pg"));
        } else {
            // it names `next`, which stays: put the Store back on `pg`
            ok(
                &home,
                "store.update",
                &[("store", t("pg")), ("auth", t("pg"))],
            );
            ok(&home, "store.remove_auth", &[("auth", t(&next))]);
        }
    }
    ok(&home, "store.disconnect", &[("store", t("pg"))]);
    std::fs::remove_dir_all(home).unwrap();
}

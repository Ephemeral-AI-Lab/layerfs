//! A PostgreSQL Store's addresses and login (call surface 2.0, `pg.html`):
//! `store.connect --machine` gives one machine its own address, proven from
//! that machine before it is saved (another database is refused, nothing
//! saved); `store.connect` without a machine replaces the address every
//! machine dials, proven from this computer; `store.update` changes the
//! saved login (checked on the Store's address) and `synchronous_commit`;
//! `store.disconnect --machine` removes a machine's own address. Needs
//! LAYERFS_TEST_PG_URL (a server URL with its user and no password, to a
//! database with no `layerfs` schema; the test leaves one behind) and
//! LAYERFS_TEST_PG_PASSWORD; without them it says so and passes.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths};
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
    let mut args: Vec<(String, SdkValue)> = args
        .iter()
        .map(|(n, v)| ((*n).to_owned(), v.clone()))
        .collect();
    args.push(("detail".to_owned(), SdkValue::Bool(true)));
    layerfs_api::call(Some(home), name, SdkValue::Map(args), &mut |_| {})
        .map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn record_text(home: &Path, store: &str) -> String {
    std::fs::read_to_string(paths::store_record(home, store)).unwrap()
}

/// `(machine, url)` of each address an answer shows.
fn addresses(answer: &SdkValue) -> Vec<(Option<String>, String)> {
    let Some(SdkValue::List(rows)) = answer.get("connections") else {
        panic!("connections: {answer:?}");
    };
    rows.iter()
        .map(|row| {
            let machine = row.get_text("machine").map(str::to_owned);
            (machine, row.get_text("url").unwrap().to_owned())
        })
        .collect()
}

#[test]
fn addresses_and_logins_are_proven_before_they_are_saved() {
    let (Ok(url), Ok(password)) = (
        std::env::var("LAYERFS_TEST_PG_URL"),
        std::env::var("LAYERFS_TEST_PG_PASSWORD"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_PG_URL / LAYERFS_TEST_PG_PASSWORD unset");
        return;
    };
    let home = tempdir("live");
    home::open(&home).unwrap();
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    let url = format!("{scheme}://{address}");
    for auth in ["pg", "pg-2"] {
        ok(
            &home,
            "store.save_auth",
            &[
                ("auth", t(auth)),
                ("user", t(user)),
                ("password", t(&password)),
            ],
        );
    }
    let connected = ok(
        &home,
        "store.connect",
        &[("store", t("pg")), ("url", t(&url)), ("auth", t("pg"))],
    );
    assert_eq!(addresses(&connected), [(None, url.clone())]);
    // native's own address: the same server, proven from native
    let native = ok(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&url)),
            ("machine", t("native")),
        ],
    );
    assert_eq!(native.get_text("outcome"), Some("updated"));
    assert_eq!(
        addresses(&native),
        [
            (None, url.clone()),
            (Some("native".to_owned()), url.clone())
        ]
    );
    assert!(!record_text(&home, "pg").contains(&password));
    // another database is not this Store: refused, nothing saved, for the
    // machine's address and for every machine's
    let before = record_text(&home, "pg");
    let other = match url.rsplit_once('/') {
        Some((server, _)) => format!("{server}/postgres"),
        None => url.clone(),
    };
    let (code, message) = call(
        &home,
        "store.connect",
        &[
            ("store", t("pg")),
            ("url", t(&other)),
            ("machine", t("native")),
        ],
    )
    .unwrap_err();
    assert_eq!(code, "Unavailable", "{message}");
    assert!(message.contains("holds another Store than pg"), "{message}");
    assert!(!message.contains("cannot reach"), "{message}");
    let (code, message) = call(
        &home,
        "store.connect",
        &[("store", t("pg")), ("url", t(&other))],
    )
    .unwrap_err();
    assert!(
        matches!(code.as_str(), "Unavailable" | "Failed"),
        "{code}: {message}"
    );
    assert_eq!(record_text(&home, "pg"), before);
    // the address every machine dials, to the same database again: saved
    let readdressed = ok(
        &home,
        "store.connect",
        &[("store", t("pg")), ("url", t(&url))],
    );
    assert_eq!(readdressed.get_text("outcome"), Some("updated"));
    // another login with the same role, and the setting
    let updated = ok(
        &home,
        "store.update",
        &[
            ("store", t("pg")),
            ("auth", t("pg-2")),
            ("synchronous_commit", t("off")),
        ],
    );
    assert_eq!(updated.get_text("auth"), Some("pg-2"));
    assert_eq!(updated.get_text("synchronous_commit"), Some("off"));
    let got = ok(&home, "store.get", &[("store", t("pg"))]);
    assert_eq!(got.get_text("auth"), Some("pg-2"), "{got:?}");
    // the login a Store uses cannot be removed; the other one can
    assert_eq!(
        call(&home, "store.remove_auth", &[("auth", t("pg-2"))])
            .unwrap_err()
            .0,
        "InUse"
    );
    ok(&home, "store.remove_auth", &[("auth", t("pg"))]);
    // native's own address removed: it dials the Store's again
    let removed = ok(
        &home,
        "store.disconnect",
        &[("store", t("pg")), ("machine", t("native"))],
    );
    assert_eq!(removed.get_text("machine"), Some("native"));
    assert!(!record_text(&home, "pg").contains("machines"));
    assert_eq!(
        call(
            &home,
            "store.disconnect",
            &[("store", t("pg")), ("machine", t("native"))]
        )
        .unwrap_err()
        .0,
        "NotFound"
    );
    ok(&home, "store.disconnect", &[("store", t("pg"))]);
    let _ = std::fs::remove_dir_all(home);
}

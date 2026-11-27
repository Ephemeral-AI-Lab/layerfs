//! What was seen of a PostgreSQL Store's reach (spec v23 V23-2), and a
//! PostgreSQL Store's record (owner, 2026-11-24), with no server:
//! `store/<s>/observed.json` keeps each side's last reach beside the `at`
//! a live read writes, two writers lose no entry, a write after the Store
//! is gone leaves no folder, and a removed machine's attachment and reach
//! are forgotten; `machines` lists the attached machines with their reach;
//! a Store whose password is not saved is not dialled. A failed check is `not_connected` (the one case a machine's
//! loopback hint is added) only when no server answered.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, observed, paths, write_entity};
use layerfs_store::observed::{self as seen, Reach};
use layerfs_store::postgresql::connect::{machines, shown};
use layerfs_store::postgresql::login;
use layerfs_store::record::StoreRecord;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-observed-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    home::ensure(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// Writes PostgreSQL Store `name`'s record as `store.connect` does, user
/// `alice`, with `extra` fields.
fn store(home: &Path, name: &str, extra: &[(&str, SdkValue)]) -> StoreRecord {
    let fields = vec![
        ("name", t(name)),
        ("kind", t("postgresql")),
        ("url", t("postgresql://db.example:6543/app")),
        ("user", t("alice")),
        ("schema", t("layerfs")),
        ("synchronous_commit", t("local")),
        ("binding", SdkValue::Bytes((0..32).collect())),
    ];
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v));
    let record = (extra.iter()).fold(SdkValue::Map(fields.collect()), |record, (k, v)| {
        home::with_field(record, k, v.clone())
    });
    write_entity(&paths::store_record(home, name), &record).unwrap();
    StoreRecord::of(home, name).unwrap()
}

fn attached(names: &[&str]) -> (&'static str, SdkValue) {
    (
        "attached",
        SdkValue::List(names.iter().map(|n| t(n)).collect()),
    )
}

#[test]
fn reaches_are_kept_beside_the_live_read_and_merged() {
    let home = tempdir("merge");
    store(&home, "pg", &[]);
    let measured = Reach {
        ok: true,
        latency_ms: Some(3),
        server_version: Some("17.6".to_owned()),
        error: None,
    };
    seen::record(&home, "pg", Some("lab"), &measured);
    seen::record(&home, "pg", None, &Reach::failed("refused"));
    // a live read's `at` keeps both
    seen::seen_at(&home, "pg", 42);
    let file = paths::store_record(&home, "pg");
    assert_eq!(observed::read(&file), Some(42));
    let got = seen::read(&home, "pg");
    let lab = got.of("lab").expect("lab's reach");
    assert_eq!(lab.get("ok"), Some(&SdkValue::Bool(true)));
    assert_eq!(lab.get_int("latency_ms"), Some(3));
    assert_eq!(lab.get_text("server_version"), Some("17.6"));
    assert!(lab.get_text("at").is_some(), "{lab:?}");
    let here = got.here.expect("this computer's reach");
    assert_eq!(here.get("ok"), Some(&SdkValue::Bool(false)));
    assert_eq!(here.get_text("error"), Some("refused"));
    // the next one replaces it
    seen::record(&home, "pg", Some("lab"), &Reach::reached());
    let lab = seen::read(&home, "pg").of("lab").unwrap();
    assert_eq!(lab.get("latency_ms"), Some(&SdkValue::Null));
    // `native` is a machine like any other, apart from this computer
    seen::record(
        &home,
        "pg",
        Some(layerfs_home::names::HOST_MACHINE),
        &Reach::reached(),
    );
    let got = seen::read(&home, "pg");
    let native = got
        .of(layerfs_home::names::HOST_MACHINE)
        .expect("native's reach");
    assert_eq!(native.get("ok"), Some(&SdkValue::Bool(true)));
    assert_eq!(got.here.unwrap().get("ok"), Some(&SdkValue::Bool(false)));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn concurrent_writers_lose_no_entry() {
    let home = tempdir("race");
    store(&home, "pg", &[]);
    let names: Vec<String> = (0..12).map(|i| format!("m{i}")).collect();
    std::thread::scope(|scope| {
        for name in &names {
            let home = &home;
            scope.spawn(move || seen::record(home, "pg", Some(name), &Reach::reached()));
        }
    });
    let got = seen::read(&home, "pg");
    for name in &names {
        assert!(got.of(name).is_some(), "{name} lost: {got:?}");
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_write_after_the_store_is_gone_leaves_no_folder() {
    let home = tempdir("gone");
    store(&home, "pg", &[]);
    std::fs::remove_dir_all(paths::store_dir(&home, "pg")).unwrap();
    seen::record(&home, "pg", Some("lab"), &Reach::reached());
    seen::record(&home, "pg", None, &Reach::reached());
    seen::seen_at(&home, "pg", 1);
    assert!(!paths::store_dir(&home, "pg").exists());
    // and a sqlite Store's record is left alone
    write_entity(
        &paths::store_record(&home, "s"),
        &SdkValue::map([("name", t("s")), ("kind", t("sqlite"))]),
    )
    .unwrap();
    seen::record(&home, "s", Some("lab"), &Reach::reached());
    assert!(!observed::file(&paths::store_record(&home, "s")).exists());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_removed_machine_is_forgotten() {
    let home = tempdir("forget");
    store(&home, "pg", &[attached(&["lab", "other"])]);
    seen::record(&home, "pg", Some("lab"), &Reach::reached());
    seen::record(&home, "pg", Some("other"), &Reach::reached());
    layerfs_store::postgresql::check::forget_machine(&home, "lab").unwrap();
    let got = seen::read(&home, "pg");
    assert!(got.of("lab").is_none());
    assert!(got.of("other").is_some());
    let record = StoreRecord::of(&home, "pg").unwrap();
    assert_eq!(record.attached(), ["other"]);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn machines_list_the_attached_machines_with_their_reach() {
    let home = tempdir("rows");
    let record = store(&home, "pg", &[attached(&["zeta", "lab"])]);
    seen::record(&home, "pg", Some("lab"), &Reach::failed("timed out"));
    seen::record(&home, "pg", Some("box"), &Reach::reached());
    seen::record(&home, "pg", None, &Reach::reached());
    // names in record order, for a `store.list` item
    assert_eq!(
        machines(&record, false),
        SdkValue::List(vec![t("zeta"), t("lab")])
    );
    let SdkValue::List(rows) = machines(&record, true) else {
        panic!("rows");
    };
    // only attached machines: `box`'s reach is not a row
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].get_text("machine"), Some("zeta"));
    assert_eq!(rows[0].get("reached"), Some(&SdkValue::Null));
    assert_eq!(rows[0].get("reach"), Some(&SdkValue::Null));
    // a failed reach is a word, not a missing key
    assert_eq!(rows[1].get_text("reached"), Some("failed"));
    assert_eq!(
        rows[1].get("reach").and_then(|r| r.get_text("error")),
        Some("timed out")
    );
    // this computer's reach is the Store's own
    let here = layerfs_store::postgresql::connect::reach(&record);
    assert_eq!(here.get("ok"), Some(&SdkValue::Bool(true)));
    // attach and detach keep the order of the others
    let record = record
        .with_attached("box", true)
        .with_attached("zeta", false);
    assert_eq!(record.attached(), ["lab", "box"]);
    assert!(record.is_attached("box") && !record.is_attached("zeta"));
    let none = record
        .with_attached("lab", false)
        .with_attached("box", false);
    assert!(none.field("attached").is_none(), "{:?}", none.to_record());
    std::fs::remove_dir_all(home).unwrap();
}

/// A Store whose password this home does not hold is not dialled: the
/// refusal names the connect that saves it; a record naming no user is
/// refused too. Once saved, it dials with its user and password put into
/// its address.
#[test]
fn a_missing_password_is_refused_and_a_saved_one_is_dialled() {
    let home = tempdir("missing");
    let record = store(&home, "pg", &[("url", t("postgres://db.example/app%23x"))]);
    let fix = "layerfs store connect pg <url> --password-stdin";
    let refusal = layerfs_store::record::url(&record).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput");
    assert_eq!(refusal.detail.get_text("fix"), Some(fix));
    login::save(&home, "pg", "s3 cret").unwrap();
    let nameless = record.clone().without("user");
    assert_eq!(
        layerfs_store::record::url(&nameless).unwrap_err().code,
        "InvalidInput"
    );
    // the URL dialled is the address alone with the login put in, never
    // one holding an `@` or `#` of its own
    assert_eq!(
        layerfs_store::record::url(&record).unwrap(),
        "postgres://alice:s3%20cret@db.example/app%23x"
    );
    // the password is in the home's secret folder, not the record
    let written = std::fs::read_to_string(paths::store_record(&home, "pg")).unwrap();
    assert!(!written.contains("s3 cret"), "{written}");
    let saved = login::read(&home, "pg").unwrap().unwrap();
    assert_eq!(saved.password(), "s3 cret");
    assert!(saved.changed_ms.is_some());
    login::forget(&home, "pg").unwrap();
    assert!(login::read(&home, "pg").unwrap().is_none());
    std::fs::remove_dir_all(home).unwrap();
}

/// A URL is shown exactly as written, or hidden whole when it may hold a
/// login: no login is cut out of it (the e0aa26e149 Decision; the rows
/// marked were shown without their login before).
#[test]
fn urls_are_shown_as_written_or_hidden_whole() {
    use layerfs_store::postgresql::connect::HIDDEN;
    for (url, as_shown) in [
        // the client does not read `sslpassword` or `PassFile`
        (
            "postgresql://a:b@h:1/d?sslmode=require&user=x&sslpassword=y&PassFile=%2Fp#f",
            HIDDEN,
        ),
        // a `user` the client reads (was `postgres://h/d`)
        ("postgres://h/d?user=x", HIDDEN),
        // a `password` key (was `postgresql://h/d%23x`)
        ("postgresql://h/d#x?password=y", HIDDEN),
        // the client refuses the `sslmode` `require#f`
        ("postgresql://h/d?sslmode=require#f", HIDDEN),
        ("postgresql://h/d", "postgresql://h/d"),
        // a `#` holding no login is shown as written (was `%23`)
        ("postgresql://h/d#x", "postgresql://h/d#x"),
        // the client knows no `sslkey` or `sslcert`
        ("postgresql://h/d?sslkey=%2Fk&sslcert=%2Fc", HIDDEN),
        (
            "postgresql://h/d?sslmode=require&application_name=a%40b",
            "postgresql://h/d?sslmode=require&application_name=a%40b",
        ),
    ] {
        assert_eq!(shown(url), as_shown, "{url}");
    }
}

/// The words of a check that failed `url`.
fn failed_check(url: &str) -> String {
    match layerfs_persistence::postgresql_access(url, "layerfs") {
        Ok(_) => panic!("the check of {url} passed"),
        Err(error) => error.to_string(),
    }
}

/// `not_connected` holds for what a dial no server answered says: nothing
/// listening, a name that does not resolve, the session's own words for
/// both. A server that answered (a missing database, a setting it refused)
/// was reached, so no loopback hint applies.
#[test]
fn only_a_dial_no_server_answered_is_not_connected() {
    use layerfs_store::postgresql::open::not_connected;
    let refused = failed_check("postgres://lfs@127.0.0.1:1/db?connect_timeout=5");
    assert!(not_connected(&refused), "{refused}");
    let unresolved =
        failed_check("postgres://lfs@layerfs-no-such-host.invalid/db?connect_timeout=5");
    assert!(not_connected(&unresolved), "{unresolved}");
    assert!(not_connected(
        "no connection to the server was made (it refused the connection, did not answer, or closed it)"
    ));
    assert!(!not_connected(
        "the server refused the session: the database does not exist, or the user or password is wrong"
    ));
    let Some(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok() else {
        eprintln!("NOT_RUN (server half): LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    // the test URL is a postgres:// URL or key=value words
    let (setting, database) = match url.split_once("://") {
        Some(_) => {
            let and = if url.contains('?') { "&" } else { "?" };
            (
                format!("{url}{and}options=-c%20no_such_setting%3D1"),
                format!("{url}{and}dbname=layerfs_no_such_db"),
            )
        }
        None => (
            format!("{url} options='-c no_such_setting=1'"),
            format!("{url} dbname=layerfs_no_such_db"),
        ),
    };
    for answered in [
        // a setting the server refuses at login
        failed_check(&setting),
        // a database the server does not hold
        failed_check(&database),
    ] {
        // in the server's own words, which name what it refused
        assert!(answered.contains("no_such"), "{answered}");
        assert!(!not_connected(&answered), "{answered}");
    }
}

/// A password holding `@` (M1 of the 3cf2cfc246 review). The daemon's
/// client reads a URL's user part up to its first `@` anywhere. A URL
/// holding an `@`, or a login key, is hidden whole and never rewritten
/// (e0aa26e149 Decision): nothing shown keeps any part of the password (no
/// refusal, no fix line, no state's fix), and it is never dialled.
#[test]
fn a_password_holding_at_never_shows_and_is_never_dialled() {
    use layerfs_store::postgresql::connect::{refuse_login, refuse_url, target, HIDDEN};
    const PASSWORD: [&str; 2] = ["Zq7", "Xw9"];
    let leaks = |text: &str| PASSWORD.iter().any(|part| text.contains(part));
    let home = tempdir("at-password");
    login::save(&home, "pg", "s3 cret").unwrap();
    for given in [
        // the client's user part holds a query (its host is `Xw9`)
        "postgres://h/db?user=u&password=Zq7@Xw9",
        "postgres://h/db?sslpassword=Zq7@Xw9",
        // an `@` left after the client's user part (its host `Xw9@db.example`)
        "postgres://u:Zq7@Xw9@db.example/app",
        // the client's user part is the wider one: a `/` in the password
        "postgres://u:Zq7/Xw9@db.example/app",
        // a `#` ends no parameter for the client
        "postgres://h:6543/db?password=Zq7#Xw9",
        "postgres://h:6543/db?sslmode=require&password=Zq7@Xw9",
        // the client's user part holds a password parameter
        "postgres://h/db?password=Zq7&x=a@db.example",
    ] {
        assert!(refuse_login(given).is_err(), "{given}");
        assert_eq!(shown(given), HIDDEN, "{given}");
        assert!(target(given).is_err(), "{given}");
        for new in [true, false] {
            let refusal = refuse_url("pg", given, new).unwrap_err();
            let text = format!("{} {:?}", refusal.message, refusal.detail);
            assert!(!leaks(&text), "{given}: {text}");
            let fix = refusal.detail.get_text("fix").unwrap_or_default();
            assert_eq!(
                fix, "layerfs store connect pg <url> --user <user> --password-stdin",
                "{given}"
            );
        }
        // a record holding it is never dialled, and nothing it answers
        // names any part of it
        let record = store(&home, "pg", &[("url", t(given))]);
        let refusal = layerfs_store::record::url(&record).unwrap_err();
        let text = format!("{} {:?}", refusal.message, refusal.detail);
        assert_eq!(refusal.code, "InvalidInput", "{given}");
        assert!(!leaks(&text), "{given}: {text}");
    }
    // a `#` alone holds no login: a connected Store's fix needs none
    let refusal = refuse_url("pg", "postgresql://h/d#x", false).unwrap_err();
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs store connect pg <url>")
    );
    std::fs::remove_dir_all(home).unwrap();
}

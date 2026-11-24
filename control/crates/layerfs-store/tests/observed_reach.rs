//! What was seen of a PostgreSQL Store's reach (spec v23 V23-2, V23-3,
//! V23-6, V23-8), with no server: `store/<s>/observed.json` keeps each
//! side's last reach beside the `at` a live read writes, two writers lose
//! no entry, a write after the Store is gone leaves no folder, and a
//! machine's entry is forgotten; `connections` lists every side that
//! connects; an old record reads `outdated` and a clean write drops its old
//! fields; a missing login reads `missing`; `store auths` lists a login
//! file it cannot read as a broken row. A failed check is `not_connected`
//! (the one case a machine's loopback hint is added) only when no server
//! answered.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, observed, paths, write_entity};
use layerfs_home::state::{Reason, State};
use layerfs_store::observed::{self as seen, Reach};
use layerfs_store::postgresql::connect::{connections, without_login};
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

/// Writes PostgreSQL Store `name`'s record, naming login `pg`, with
/// `extra` fields.
fn store(home: &Path, name: &str, extra: &[(&str, SdkValue)]) -> StoreRecord {
    let fields = vec![
        ("name", t(name)),
        ("kind", t("postgresql")),
        ("url", t("postgresql://db.example:6543/app")),
        ("schema", t("layerfs")),
        ("synchronous_commit", t("local")),
        ("auth", t("pg")),
        ("binding", SdkValue::Bytes((0..32).collect())),
    ];
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v));
    let record = (extra.iter()).fold(SdkValue::Map(fields.collect()), |record, (k, v)| {
        home::with_field(record, k, v.clone())
    });
    write_entity(&paths::store_record(home, name), &record).unwrap();
    StoreRecord::of(home, name).unwrap()
}

fn rows(record: &StoreRecord, sandboxes: &[&str]) -> Vec<SdkValue> {
    let sandboxes: Vec<String> = sandboxes.iter().map(|s| (*s).to_owned()).collect();
    match connections(record, &sandboxes) {
        SdkValue::List(rows) => rows,
        other => panic!("{other:?}"),
    }
}

fn machines(rows: &[SdkValue]) -> Vec<Option<&str>> {
    rows.iter().map(|row| row.get_text("machine")).collect()
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
    // `native` with no address of its own dials as this computer
    seen::record(&home, "pg", Some("native"), &Reach::reached());
    let got = seen::read(&home, "pg");
    assert!(got.of("native").is_none());
    assert_eq!(got.here.unwrap().get("ok"), Some(&SdkValue::Bool(true)));
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
    let own = SdkValue::map([("url", t("postgresql://10.0.0.2:6543/app"))]);
    store(&home, "pg", &[("machines", SdkValue::map([("lab", own)]))]);
    seen::record(&home, "pg", Some("lab"), &Reach::reached());
    seen::record(&home, "pg", Some("other"), &Reach::reached());
    layerfs_store::postgresql::check::forget_machine(&home, "lab").unwrap();
    let got = seen::read(&home, "pg");
    assert!(got.of("lab").is_none());
    assert!(got.of("other").is_some());
    let record = StoreRecord::of(&home, "pg").unwrap();
    assert!(record.own_connection("lab").is_none());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn connections_list_every_side_that_connects() {
    let home = tempdir("rows");
    let own = SdkValue::map([("url", t("postgresql://10.0.0.2:7000/app"))]);
    let record = store(&home, "pg", &[("machines", SdkValue::map([("zeta", own)]))]);
    seen::record(&home, "pg", Some("lab"), &Reach::failed("timed out"));
    seen::record(&home, "pg", None, &Reach::reached());
    let rows = rows(&record, &["box", "native", "lab"]);
    // this computer first, then by name; native is this computer's row
    assert_eq!(
        machines(&rows),
        [None, Some("box"), Some("lab"), Some("zeta")]
    );
    let row = |i: usize| &rows[i];
    assert_eq!(row(0).get_text("reached"), Some("ok"));
    assert_eq!(row(0).get("own"), Some(&SdkValue::Bool(false)));
    assert_eq!(
        row(0).get("server"),
        Some(&SdkValue::map([
            ("host", t("db.example")),
            ("port", SdkValue::Int(6543)),
            ("database", t("app")),
        ]))
    );
    // a Sandbox only: the Store's address, nothing seen
    assert_eq!(row(1).get("reached"), Some(&SdkValue::Null));
    assert_eq!(row(1).get("reach"), Some(&SdkValue::Null));
    assert_eq!(
        row(1).get_text("url"),
        Some("postgresql://db.example:6543/app")
    );
    // a failed reach is a word, not a missing key
    assert_eq!(row(2).get_text("reached"), Some("failed"));
    assert_eq!(
        row(2).get("reach").and_then(|r| r.get_text("error")),
        Some("timed out")
    );
    // its own address
    assert_eq!(row(3).get("own"), Some(&SdkValue::Bool(true)));
    assert_eq!(
        row(3).get("server").and_then(|s| s.get("port")),
        Some(&SdkValue::Int(7000))
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn old_fields_read_outdated_and_a_clean_write_drops_them() {
    let home = tempdir("old");
    layerfs_store::postgresql::login::save(&home, "pg", "postgres", "pw").unwrap();
    let checked = SdkValue::map([("checked", SdkValue::map([("at", SdkValue::Int(1))]))]);
    let own = SdkValue::map([
        ("url", t("postgresql://postgres@10.0.0.2/app")),
        ("checked", SdkValue::Int(1)),
    ]);
    let old = store(
        &home,
        "pg",
        &[
            (
                "url",
                t("postgresql://postgres@db.example:6543/app?sslmode=disable"),
            ),
            ("password", t("file:/no/such")),
            ("machines", SdkValue::map([("lab", checked), ("box", own)])),
        ],
    );
    assert!(old.outdated_fields());
    let fix = "layerfs store update pg --auth pg";
    assert_eq!(
        layerfs_store::state::recorded_under(&old, None).unwrap(),
        State::broken(Reason::Outdated, Some(fix))
    );
    let clean = old.cleaned();
    assert!(!clean.outdated_fields(), "{:?}", clean.to_record());
    assert!(clean.field("password").is_none());
    assert_eq!(
        clean.text("url"),
        Some("postgresql://db.example:6543/app?sslmode=disable")
    );
    assert_eq!(
        clean.own_connection("box").unwrap().url.as_deref(),
        Some("postgresql://10.0.0.2/app")
    );
    assert!(clean.own_connection("lab").is_none());
    assert_eq!(
        layerfs_store::state::recorded_under(&clean, None).unwrap(),
        State::Ready
    );
    // each kind of old field alone is outdated
    for extra in [
        ("password", t("env:X")),
        ("url", t("postgresql://db.example/app?user=bob")),
        ("url", t("postgresql://db.example/app?passfile=/p")),
    ] {
        assert!(
            store(&home, "pg", &[extra.clone()]).outdated_fields(),
            "{extra:?}"
        );
    }
    assert!(!store(&home, "pg", &[]).outdated_fields());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn a_missing_login_reads_missing_and_wins_over_old_fields() {
    let home = tempdir("missing");
    let record = store(&home, "pg", &[("password", t("file:/x"))]);
    let fix = "layerfs store save-auth pg --user <user> --password-stdin && layerfs store update pg --auth pg";
    assert_eq!(
        layerfs_store::state::recorded_under(&record, None).unwrap(),
        State::broken(Reason::Missing, Some(fix))
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn urls_lose_their_login_and_keep_the_rest() {
    use layerfs_store::postgresql::connect::HIDDEN;
    for (url, clean) in [
        // the client does not read `sslpassword` or `PassFile`: it reads
        // no servers to compare a form with, so none is shown
        (
            "postgresql://a:b@h:1/d?sslmode=require&user=x&sslpassword=y&PassFile=%2Fp#f",
            HIDDEN,
        ),
        ("postgres://h/d?user=x", "postgres://h/d"),
        // the client's database is `d#x`: kept, its `#` written `%23`
        ("postgresql://h/d#x?password=y", "postgresql://h/d%23x"),
        // the client refuses the `sslmode` `require#f`
        ("postgresql://h/d?sslmode=require#f", HIDDEN),
        ("postgresql://h/d", "postgresql://h/d"),
        // the client knows no `sslkey` or `sslcert`
        ("postgresql://h/d?sslkey=%2Fk&sslcert=%2Fc", HIDDEN),
        (
            "postgresql://h/d?sslmode=require&application_name=a%40b",
            "postgresql://h/d?sslmode=require&application_name=a%40b",
        ),
    ] {
        assert_eq!(without_login(url), clean, "{url}");
    }
}

#[test]
fn a_login_file_that_cannot_be_read_is_a_broken_row() {
    let home = tempdir("auths");
    layerfs_store::postgresql::login::save(&home, "good", "alice", "pw").unwrap();
    layerfs_store::postgresql::login::save(&home, "bad", "bob", "pw").unwrap();
    let bad = std::fs::read_dir(home.join("secret/auth"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("bad"))
        .unwrap();
    std::fs::write(&bad, "{not json").unwrap();
    let listed = layerfs_store::postgresql::login::list(&home).unwrap();
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}");
    };
    let row = |auth: &str| {
        items
            .iter()
            .find(|item| item.get_text("auth") == Some(auth))
            .unwrap_or_else(|| panic!("{auth}: {items:?}"))
    };
    assert_eq!(row("good").get_text("state"), Some("ready"));
    assert_eq!(row("good").get_text("user"), Some("alice"));
    assert_eq!(row("bad").get_text("state"), Some("broken"));
    assert_eq!(row("bad").get_text("reason"), Some("failed"));
    assert_eq!(row("bad").get("user"), Some(&SdkValue::Null));
    assert_eq!(
        row("bad").get_text("fix"),
        Some("layerfs store save-auth bad --user <user> --password-stdin")
    );
    std::fs::remove_dir_all(home).unwrap();
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
        assert!(answered.contains("db error"), "{answered}");
        assert!(!not_connected(&answered), "{answered}");
    }
}

/// A password holding `@` (M1 of the 3cf2cfc246 review). The daemon's
/// client reads a URL's user part up to its first `@` anywhere, and a URL
/// is read only as the client reads it (aa83393447 review, Decision).
/// Nothing shown or written down keeps any part of the password: not the
/// address or `--user` a fix line names, a `connections` row, a refusal's
/// server, a state's fix, nor the record a write cleans, which then reads
/// current. A URL whose only form the client reads as the original keeps
/// part of the password, or whose user part holds a query (so the login
/// cannot be told from the address), is hidden and never rewritten: the
/// record stays as it was, `outdated`, and is never dialled.
#[test]
fn a_password_holding_at_never_shows_and_dialling_keeps_the_client_split() {
    use layerfs_home::outcome::shell_word;
    use layerfs_store::postgresql::connect::{
        refuse_login, refuse_login_with_fix, target, url_user, HIDDEN,
    };
    use layerfs_store::postgresql::login;
    const PASSWORD: [&str; 2] = ["Zq7", "Xw9"];
    let leaks = |text: &str| PASSWORD.iter().any(|part| text.contains(part));
    let home = tempdir("at-password");
    login::save(&home, "pg", "alice", "s3 cret").unwrap();
    // (given, the address shown and kept, the user a fix names, the host)
    for (given, shown, user, host) in [
        // the client's user part holds a query: hidden (its host is `Xw9`)
        ("postgres://h/db?user=u&password=Zq7@Xw9", None, None, None),
        ("postgres://h/db?sslpassword=Zq7@Xw9", None, None, None),
        // an `@` left after the client's user part (its host `Xw9@db.example`)
        ("postgres://u:Zq7@Xw9@db.example/app", None, None, None),
        // the client's user part is the wider one: a `/` in the password
        (
            "postgres://u:Zq7/Xw9@db.example/app",
            Some("postgres://db.example/app"),
            Some("u"),
            Some("db.example"),
        ),
        // a `#` ends no parameter for the client
        (
            "postgres://h:6543/db?password=Zq7#Xw9",
            Some("postgres://h:6543/db"),
            None,
            Some("h"),
        ),
        (
            "postgres://h:6543/db?sslmode=require&password=Zq7@Xw9",
            None,
            None,
            None,
        ),
        // the client's user part holds a password parameter: hidden
        (
            "postgres://h/db?password=Zq7&x=a@db.example",
            None,
            None,
            None,
        ),
    ] {
        assert!(refuse_login(given).is_err(), "{given}");
        assert_eq!(without_login(given), shown.unwrap_or(HIDDEN), "{given}");
        if let Some(shown) = shown {
            assert!(refuse_login(shown).is_ok(), "{given}");
        }
        assert_eq!(url_user(given).as_deref(), user, "{given}");
        assert_eq!(target(given).ok().map(|t| t.0).as_deref(), host, "{given}");
        for (first, machine) in [(true, None), (false, None), (false, Some("lab"))] {
            let refusal = refuse_login_with_fix("pg", given, first, machine).unwrap_err();
            let text = format!("{} {:?}", refusal.message, refusal.detail);
            assert!(!leaks(&text), "{given}: {text}");
            let fix = refusal.detail.get_text("fix").unwrap_or_default();
            let address = shown.map_or("<url>".to_owned(), shell_word);
            assert!(fix.contains(&address), "{given}: {fix}");
        }
        // an older record holding it, as the Store's address and a machine's own
        let own = SdkValue::map([("url", t(given))]);
        let record = store(
            &home,
            "pg",
            &[
                ("url", t(given)),
                ("machines", SdkValue::map([("lab", own)])),
            ],
        );
        assert!(record.outdated_fields(), "{given}");
        let shown_rows = format!("{:?}", rows(&record, &["lab"]));
        assert!(!leaks(&shown_rows), "{given}: {shown_rows}");
        let fixes = format!(
            "{:?} {} {}",
            layerfs_store::state::recorded_under(&record, None),
            login::no_auth_fix(&record),
            login::missing_fix(&record, "pg")
        );
        assert!(!leaks(&fixes), "{given}: {fixes}");
        // never dialled: its fix replaces it, and names no part of it
        let refusal = layerfs_store::record::url(&record).unwrap_err();
        let text = format!("{} {:?}", refusal.message, refusal.detail);
        assert_eq!(refusal.code, "InvalidInput", "{given}");
        assert!(!leaks(&text), "{given}: {text}");
        // one write cleans it: nothing of the password on disk, and current;
        // or, hidden, the record is left as it was and still outdated
        record.cleaned().write(&home).unwrap();
        let written = std::fs::read_to_string(paths::store_record(&home, "pg")).unwrap();
        let clean = StoreRecord::of(&home, "pg").unwrap();
        let kept = shown.unwrap_or(given);
        assert_eq!(!leaks(&written), shown.is_some(), "{given}: {written}");
        assert_eq!(
            clean.outdated_fields(),
            shown.is_none(),
            "{given}: {written}"
        );
        assert_eq!(clean.text("url"), Some(kept), "{given}");
        let own = clean.own_connection("lab").unwrap().url;
        assert_eq!(own.as_deref(), Some(kept), "{given}");
    }
    // the URL dialled is the address alone with the login put in, never
    // one holding an `@` or `#` of its own
    let record = store(&home, "pg", &[("url", t("postgres://db.example/app%23x"))]);
    assert_eq!(
        layerfs_store::record::url(&record).unwrap(),
        "postgres://alice:s3%20cret@db.example/app%23x"
    );
    std::fs::remove_dir_all(home).unwrap();
}

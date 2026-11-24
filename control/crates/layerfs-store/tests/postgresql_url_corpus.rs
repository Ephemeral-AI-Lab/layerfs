//! A PostgreSQL Store's URL is read only as its client reads it
//! (`postgres::Config`, tokio-postgres 0.7.18's `UrlParser`), never by a
//! parser of LayerFS's own (the Decision of the aa83393447 review). Over
//! the corpora of the 3cf2cfc246 and aa83393447 reviews, earlier tests and
//! generated URLs, compared with the client itself:
//! (a) nothing shown, no fix line and no record a write leaves holds three
//! bytes of the password the client reads, nor of the password the URL's
//! writer meant; (b) a URL shown or written reads, by the client, the same
//! hosts, host addresses, ports and database as the original and no login;
//! (c) the strip is idempotent; (d) a URL dialled reads exactly the saved
//! login and the address's endpoints; (e) a fix's `--user` is the client's
//! user. A record whose URL has no clean form is shown hidden and never
//! rewritten, and `store update --auth` refuses it with its fix.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths, write_entity};
use layerfs_home::names::StoreName;
use layerfs_home::outcome::shell_word;
use layerfs_store::postgresql::connect::{
    connections, refuse_login, refuse_login_with_fix, target, url_user, without_login,
};
use layerfs_store::postgresql::login;
use layerfs_store::record::StoreRecord;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// What is shown of a URL with no clean form.
const HIDDEN: &str = "postgresql://<hidden: not an address alone>";

/// The saved login's password: every byte a URL treats specially.
const LOGIN_PASSWORD: &str = "Lp4@Kv8 /?#:&=%x";

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-url-corpus-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    home::ensure(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

/// The client's reading of `url`; key=value words are not a URL here.
fn client(url: &str) -> Option<postgres::Config> {
    let url_like = url.starts_with("postgres://") || url.starts_with("postgresql://");
    url_like.then(|| url.parse().ok()).flatten()
}

type Endpoints = (
    Vec<postgres::config::Host>,
    Vec<IpAddr>,
    Vec<u16>,
    Option<String>,
);

fn endpoints(read: &postgres::Config) -> Endpoints {
    (
        read.get_hosts().to_vec(),
        read.get_hostaddrs().to_vec(),
        read.get_ports().to_vec(),
        read.get_dbname().map(str::to_owned),
    )
}

/// Every three-byte piece of `secret`.
fn pieces(secret: &[u8]) -> Vec<Vec<u8>> {
    secret.windows(3).map(<[u8]>::to_vec).collect()
}

/// The piece of `secret` that `text` holds, if any.
fn leak(text: &str, secret: &[u8]) -> Option<String> {
    let found = pieces(secret)
        .into_iter()
        .find(|piece| text.as_bytes().windows(3).any(|t| t == piece.as_slice()));
    found.map(|piece| String::from_utf8_lossy(&piece).into_owned())
}

/// The words of a fix line taken from a URL: every word but the fixed
/// ones (the command, its flags, placeholders, the Store and machine
/// names), each URL without its scheme.
fn derived(fix: &str) -> String {
    const FIXED: [&str; 16] = [
        "layerfs",
        "store",
        "save-auth",
        "--user",
        "--password-stdin",
        "&&",
        "connect",
        "update",
        "disconnect",
        "--auth",
        "--machine",
        "<auth>",
        "<user>",
        "<url>",
        "pg",
        "lab",
    ];
    let words = fix.split(' ').filter(|word| !FIXED.contains(word));
    let words = words.map(|word| {
        let word = word.trim_matches('\'');
        let word = word.strip_prefix("postgresql://").unwrap_or(word);
        word.strip_prefix("postgres://").unwrap_or(word).to_owned()
    });
    words.collect::<Vec<_>>().join(" ")
}

/// The `--user` a fix line names.
fn fix_user(fix: &str) -> &str {
    let after = fix.split_once("--user ").map_or("", |(_, after)| after);
    after
        .split_once(" --password-stdin")
        .map_or("", |(user, _)| user)
}

/// The fixed messages of a refused URL: no part of the URL is in one.
const MESSAGES: [&str; 5] = [
    "the URL names a user or holds a password (or a password file); give the address alone and the login with --auth (`layerfs store save-auth` saves one)",
    "the URL names a user or holds a password (or a password file); give the address alone and the login with --auth (`layerfs store save-auth` saves one); an `@` or `#` that is part of the address is written %40 or %23",
    "the URL is not one its PostgreSQL client reads, or names a user or holds a password (or a password file) under a key it does not know: give the address alone, postgresql://host[:port][,host[:port]...]/database?key=value&... with keys the client knows (not `passfile`, `sslpassword`, `sslkey` or `sslcert`), and the login with --auth (`layerfs store save-auth` saves one)",
    "the URL holds a `#`; one that is part of the address is written %23",
    "a PostgreSQL URL starts postgresql:// or postgres://",
];

/// Writes PostgreSQL Store `pg` with `url` as its address and machine
/// `lab`'s own, naming login `pg`.
fn store(home: &Path, url: &str) -> StoreRecord {
    let own = SdkValue::map([("url", t(url))]);
    let fields = [
        ("name", t("pg")),
        ("kind", t("postgresql")),
        ("url", t(url)),
        ("schema", t("layerfs")),
        ("synchronous_commit", t("local")),
        ("auth", t("pg")),
        ("binding", SdkValue::Bytes((0..32).collect())),
        ("machines", SdkValue::map([("lab", own)])),
    ];
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    write_entity(&paths::store_record(home, "pg"), &SdkValue::Map(fields)).unwrap();
    StoreRecord::of(home, "pg").unwrap()
}

/// The URL-derived texts of `connections` rows: each `url` (without its
/// scheme) and its `server`'s host and database.
fn row_texts(record: &StoreRecord) -> Vec<String> {
    let SdkValue::List(rows) = connections(record, &["lab".to_owned()]) else {
        panic!("connections");
    };
    let mut texts = Vec::new();
    for row in rows {
        // the hidden form is fixed text, none of the URL's
        let url = row.get_text("url").unwrap_or_default();
        texts.extend((url != HIDDEN).then(|| derived(url)));
        if let Some(server) = row.get("server").filter(|s| **s != SdkValue::Null) {
            texts.push(server.get_text("host").unwrap_or_default().to_owned());
            texts.push(server.get_text("database").unwrap_or_default().to_owned());
        }
    }
    texts
}

/// The reviewers' corpora (3cf2cfc246, aa83393447), earlier tests' inputs
/// and new cases, each with the password its writer meant.
const CORPUS: &[(&str, &str)] = &[
    // review 3cf2cfc246 M1
    ("postgres://h/db?user=u&password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h/db?sslpassword=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://u:Zq7@Xw9@db.example/app", "Zq7@Xw9"),
    // review aa83393447 M1: `?` in the password, a login-looking tail
    ("postgres://u:Hello?Password1@h/db", "Hello?Password1"),
    ("postgres://u:S3cr3t?mypassword@h/db", "S3cr3t?mypassword"),
    ("postgres://u:Abc?user=x@h/db", "Abc?user=x"),
    ("postgres://:SEC?@passwordhost", "SEC?"),
    // M2: a `user=` pair holding the client's password
    ("postgres://h/db?user=u:Zq7xQw@h2", "Zq7xQw"),
    ("postgres://h/db?user=bob:xyzw&password=Zq7@Xw9", "Zq7@Xw9"),
    // M3: an `@` in a value or the path of an older record
    (
        "postgres://u:Zq7Xw9q@realhost/db?application_name=me@laptop.example",
        "Zq7Xw9q",
    ),
    ("postgres://u:Zq7Xw9q@realhost/team@corp", "Zq7Xw9q"),
    ("postgres://u:Zq7Xw9q@h?application_name=a@b/db", "Zq7Xw9q"),
    // S1, S2: `#` before `?`, `dbname=` and `host=` parameters, host lists
    ("postgres://h/d#x?sslmode=require", ""),
    ("postgres://h/d?dbname=a#b", ""),
    ("postgres://h/d#x", ""),
    ("postgres://h/db?host=other", ""),
    ("postgres://h1,h2:5433/db", ""),
    ("postgres://h/d#x?password=Zq7Xw9q", "Zq7Xw9q"),
    // N3, N4
    ("postgresql://:pw@host/db?user=bob", "pw"),
    ("postgres://u:Zq7Xw9q@[::1@x]:5432/db", "Zq7Xw9q"),
    // the earlier tests' inputs
    ("postgres://u:Zq7/Xw9@db.example/app", "Zq7/Xw9"),
    ("postgres://h:6543/db?password=Zq7#Xw9", "Zq7#Xw9"),
    (
        "postgres://h:6543/db?sslmode=require&password=Zq7@Xw9",
        "Zq7@Xw9",
    ),
    ("postgres://h/db?password=Zq7&x=a@db.example", "Zq7"),
    (
        "postgresql://a:Zq7Xw9q@h:1/d?sslmode=require&user=x&sslpassword=y&PassFile=%2Fp#f",
        "Zq7Xw9q",
    ),
    ("postgres://h/d?user=x", ""),
    ("postgresql://h/d?sslmode=require#f", ""),
    ("postgresql://h/d?sslkey=%2Fk&sslcert=%2Fc", ""),
    (
        "postgresql://alice:Zq7Xw9q@127.0.0.1:1/app?sslmode=disable&passfile=/p",
        "Zq7Xw9q",
    ),
    ("postgresql://127.0.0.1:1/app?password=Zq7Xw9q", "Zq7Xw9q"),
    (
        "postgresql://127.0.0.1:2/app?sslpassword=Zq7Xw9q",
        "Zq7Xw9q",
    ),
    ("postgresql://127.0.0.1:2/app#x?password=Zq7Xw9q", "Zq7Xw9q"),
    ("postgres://localhost?x=a@b", ""),
    ("postgres://localhost/db?x=a@b", ""),
    ("postgres://a@b@c/db", ""),
    ("postgresql://owner@db.example:5432/layerfs_db", ""),
    ("postgres://a%20b:Zq7Xw9q@db.example/layerfs_db", "Zq7Xw9q"),
    // percent-encoded and differently cased login keys
    ("postgres://h/db?pass%77ord=Zq7Xw9q", "Zq7Xw9q"),
    ("postgres://h/db?PASSWORD=Zq7Xw9q", "Zq7Xw9q"),
    (
        "postgres://h/db?Password=Zq7Xw9q&sslmode=disable",
        "Zq7Xw9q",
    ),
    ("postgres://h/db?us%65r=bobby&sslmode=disable", ""),
    // encoded bytes, IPv6, lists, sockets, empty values, duplicated `?`
    (
        "postgres://alice:Zq7%40Xw9@db.example:6543/app?sslmode=require",
        "Zq7@Xw9",
    ),
    ("postgres://u%40x:Zq7Xw9q@h/db", "Zq7Xw9q"),
    ("postgres://u:Zq7#Xw9@h/db", "Zq7#Xw9"),
    ("postgres://u:Zq7/Xw9?@h/db", "Zq7/Xw9?"),
    ("postgres://u:Zq7Xw9q@[::1]:5432/db", "Zq7Xw9q"),
    (
        "postgres://u:Zq7Xw9q@[2001:db8::1]:5432,h2:5433/db?target_session_attrs=any",
        "Zq7Xw9q",
    ),
    ("postgres://u:Zq7Xw9q@h1:5432,h2,h3:5434/db", "Zq7Xw9q"),
    ("postgres://u:Zq7Xw9q@%2Ftmp%2Fsock/db", "Zq7Xw9q"),
    ("postgres://u:Zq7Xw9q@/db?host=%2Ftmp", "Zq7Xw9q"),
    ("postgres://u:Zq7Xw9q@h/db?hostaddr=10.0.0.1", "Zq7Xw9q"),
    ("postgres://u:Zq7Xw9q@h/db?host=other&port=5440", "Zq7Xw9q"),
    ("postgres://u:Zq7Xw9q@h/db?password=&user=", "Zq7Xw9q"),
    ("postgres://h/db?password=", ""),
    (
        "postgres://h/db?application_name=a?b&password=Zq7Xw9q",
        "Zq7Xw9q",
    ),
    ("postgres://h/db?sslmode=disable?x", ""),
    (
        "postgres://h/db?options=-c%20x%3D1&password=Zq7Xw9q",
        "Zq7Xw9q",
    ),
    ("postgres://Zq7user@db.example/app", ""),
    ("postgres://u:Zq7Xw9q@", "Zq7Xw9q"),
    ("postgres:///db", ""),
    ("postgresql://db.example:6543/app", ""),
    ("postgresql://db.example/app?sslmode=verify-full", ""),
    ("postgres://postgres:postgres@h/postgres", "postgres"),
    ("postgresq://alice:Zq7Xw9q@h/db", "Zq7Xw9q"),
    ("host=h password=Zq7Xw9q", "Zq7Xw9q"),
];

/// Generated URLs: each user and password written raw into the user part
/// and into `user`/`password` parameters, before several addresses.
fn generated() -> Vec<(String, String)> {
    let users = ["u", "Zq7@a", "a?b", "a/b", "a#b"];
    let passwords = [
        "Zq7@Xw9",
        "Hello?Password1",
        "Abc?user=x",
        "Zq7/Xw9",
        "Zq7#Xw9",
        "Zq7:Xw9",
    ];
    let tails = [
        "h/db",
        "h:6543/db?sslmode=require",
        "[::1]:5432/db",
        "h1,h2:5433/db?application_name=me",
        "h/db?dbname=x%23y",
    ];
    let mut urls = Vec::new();
    for user in users {
        for password in passwords {
            for tail in tails {
                let and = if tail.contains('?') { '&' } else { '?' };
                urls.push((
                    format!("postgres://{user}:{password}@{tail}"),
                    password.to_owned(),
                ));
                urls.push((
                    format!("postgres://{tail}{and}user={user}&password={password}"),
                    password.to_owned(),
                ));
            }
        }
    }
    urls
}

/// One URL through every path that shows, writes or dials it.
fn check(home: &Path, given: &str, meant: &str) {
    let read = client(given);
    let password = (read.as_ref())
        .and_then(|r| r.get_password())
        .unwrap_or_default()
        .to_vec();
    // the texts taken from the URL: none may hold the client's password,
    // nor the one its writer meant
    let mut texts: Vec<String> = Vec::new();
    // (b), (c): the shown form
    let shown = without_login(given);
    assert_eq!(without_login(&shown), shown, "(c) {given}");
    if shown != HIDDEN {
        let read = read
            .as_ref()
            .unwrap_or_else(|| panic!("(b) {given}: shown {shown}"));
        let again = client(&shown).unwrap_or_else(|| panic!("(b) {given}: {shown} unread"));
        assert!(again.get_user().is_none(), "(b) {given}: {shown}");
        assert!(again.get_password().is_none(), "(b) {given}: {shown}");
        assert_eq!(endpoints(&again), endpoints(read), "(b) {given}: {shown}");
        // and every other setting it was given
        let settings = |r: &postgres::Config| {
            let (name, options) = (r.get_application_name(), r.get_options());
            format!(
                "{name:?} {options:?} {:?} {:?}",
                r.get_ssl_mode(),
                r.get_target_session_attrs()
            )
        };
        assert_eq!(settings(&again), settings(read), "(b) {given}: {shown}");
        assert!(refuse_login(&shown).is_ok(), "(b) {given}: {shown}");
        let (host, port, database) = target(given).unwrap();
        assert_eq!(database, read.get_dbname().unwrap_or(""), "{given}");
        let ports: Vec<String> = read.get_ports().iter().map(u16::to_string).collect();
        if !ports.is_empty() {
            assert_eq!(port, ports.join(","), "{given}");
        }
        texts.extend([derived(&shown), host, database]);
    } else {
        assert!(target(given).is_err(), "{given}");
    }
    // (e) and the fix lines of a refused connect
    if let Err(refusal) = refuse_login(given) {
        assert!(
            MESSAGES.contains(&refusal.message.as_str()),
            "{given}: {}",
            refusal.message
        );
        let named = (read.as_ref())
            .and_then(|r| r.get_user())
            .filter(|user| !user.is_empty() && shown != HIDDEN);
        assert_eq!(url_user(given).as_deref(), named, "(e) {given}");
        for (first, machine) in [(true, None), (false, None), (false, Some("lab"))] {
            let Err(refusal) = refuse_login_with_fix("pg", given, first, machine) else {
                panic!("{given}");
            };
            assert!(MESSAGES.contains(&refusal.message.as_str()), "{given}");
            let Some(fix) = refusal.detail.get_text("fix") else {
                let scheme = given.starts_with("postgres://") || given.starts_with("postgresql://");
                assert!(!scheme, "{given}: no fix");
                continue;
            };
            let user = named.map_or("<user>".to_owned(), shell_word);
            assert_eq!(fix_user(fix), user, "(e) {given}: {fix}");
            let address = if shown == HIDDEN {
                "<url>".to_owned()
            } else {
                shell_word(&shown)
            };
            assert!(fix.contains(&format!(" {address}")), "{given}: {fix}");
            texts.push(derived(fix));
        }
    }
    // an older record holding it, as the Store's address and a machine's own
    let record = store(home, given);
    assert_eq!(
        record.outdated_fields(),
        refuse_login(given).is_err(),
        "{given}"
    );
    texts.extend(row_texts(&record));
    let state = layerfs_store::state::recorded_under(&record, None).unwrap();
    let state = SdkValue::Map(
        state
            .fields()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect(),
    );
    texts.push(derived(state.get_text("fix").unwrap_or_default()));
    texts.push(derived(&login::no_auth_fix(&record)));
    texts.push(derived(&login::missing_fix(&record, "pg")));
    // (d): dialled only as an address alone, the login read back exactly
    match layerfs_store::record::url(&record) {
        Ok(dialled) => {
            assert!(refuse_login(given).is_ok(), "(d) {given}: dialled");
            let back = client(&dialled).unwrap_or_else(|| panic!("(d) {given}"));
            assert_eq!(back.get_user(), Some("alice"), "(d) {given}");
            assert_eq!(
                back.get_password(),
                Some(LOGIN_PASSWORD.as_bytes()),
                "(d) {given}"
            );
            assert_eq!(
                endpoints(&back),
                endpoints(read.as_ref().unwrap()),
                "(d) {given}"
            );
        }
        Err(refusal) => {
            assert!(
                refuse_login(given).is_err(),
                "(d) {given}: {}",
                refusal.message
            );
            let message = (refusal.message)
                .strip_prefix("PostgreSQL Store pg's URL: ")
                .unwrap_or(&refusal.message);
            assert!(MESSAGES.contains(&message), "{given}: {message}");
            texts.push(derived(refusal.detail.get_text("fix").unwrap_or_default()));
        }
    }
    // a write: a clean form, current; or the URL kept as it was, outdated
    let clean = record.clone().cleaned();
    assert_eq!(
        clean.clone().cleaned().to_record(),
        clean.to_record(),
        "(c) {given}"
    );
    let kept = clean.text("url").unwrap_or_default().to_owned();
    let own = clean.own_connection("lab").and_then(|own| own.url);
    assert_eq!(own.as_deref(), Some(kept.as_str()), "{given}");
    if shown == HIDDEN {
        assert_eq!(kept, given, "never rewritten: {given}");
        assert!(clean.outdated_fields(), "{given}");
    } else {
        assert_eq!(kept, shown, "{given}");
        assert!(!clean.outdated_fields(), "{given}");
        texts.push(derived(&kept));
    }
    // (a)
    for text in &texts {
        assert_eq!(leak(text, &password), None, "(a) {given}: {text}");
        assert_eq!(
            leak(text, meant.as_bytes()),
            None,
            "(a, meant) {given}: {text}"
        );
    }
}

#[test]
fn every_url_is_shown_written_and_dialled_as_its_client_reads_it() {
    let home = tempdir("corpus");
    login::save(&home, "pg", "alice", LOGIN_PASSWORD).unwrap();
    for (given, meant) in CORPUS {
        check(&home, given, meant);
    }
    for (given, meant) in generated() {
        check(&home, &given, &meant);
    }
    std::fs::remove_dir_all(home).unwrap();
}

/// The forms the corpus pins: what is shown and the host a row names.
#[test]
fn the_reviewers_urls_show_what_their_client_reads() {
    for (given, shown, host) in [
        // M1: the client's password `Hello?Password1` is gone with its user part
        ("postgres://u:Hello?Password1@h/db", HIDDEN, None),
        ("postgres://u:Abc?user=x@h/db", HIDDEN, None),
        // M3: an `@` left after the user part is never read as a new one
        (
            "postgres://u:pw@realhost/db?application_name=me@laptop.example",
            HIDDEN,
            None,
        ),
        ("postgres://u:pw@realhost/team@corp", HIDDEN, None),
        // S1, S2: the database and hosts the client reads
        (
            "postgres://h/d#x?password=y",
            "postgres://h/d%23x",
            Some(("h", "5432", "d#x")),
        ),
        (
            "postgres://h/d?dbname=a#b",
            "postgres://h/d?dbname=a%23b",
            Some(("h", "5432", "a#b")),
        ),
        (
            "postgres://h/db?host=other",
            "postgres://h/db?host=other",
            Some(("h,other", "5432", "db")),
        ),
        (
            "postgres://h1,h2:5433/db",
            "postgres://h1,h2:5433/db",
            Some(("h1,h2", "5432,5433", "db")),
        ),
        // a login dropped, the rest kept as written
        (
            "postgres://u:Zq7/Xw9@db.example/app",
            "postgres://db.example/app",
            Some(("db.example", "5432", "app")),
        ),
        (
            "postgres://h/db?pass%77ord=x&sslmode=disable",
            "postgres://h/db?sslmode=disable",
            Some(("h", "5432", "db")),
        ),
    ] {
        assert_eq!(without_login(given), shown, "{given}");
        let target = target(given).ok();
        let target = (target.as_ref()).map(|(h, p, d)| (h.as_str(), p.as_str(), d.as_str()));
        assert_eq!(target, host, "{given}");
    }
}

/// M3 / item 6: an older record whose URL holds an `@` in a value is never
/// rewritten to another host: `store update --auth` refuses it with the
/// connect that replaces its address, and writes nothing.
#[test]
fn an_update_never_rewrites_a_url_to_another_host() {
    let home = tempdir("update");
    login::save(&home, "pg", "alice", LOGIN_PASSWORD).unwrap();
    let given = "postgres://u:Zq7Xw9q@realhost.invalid/db?application_name=me@laptop.example";
    store(&home, given);
    let before = std::fs::read(paths::store_record(&home, "pg")).unwrap();
    let request = layerfs_store::postgresql::update::StoreUpdate {
        store: StoreName::parse("pg").unwrap(),
        auth: Some("pg".to_owned()),
        synchronous_commit: None,
    };
    let refusal = layerfs_store::postgresql::update::update(&home, &request).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs store disconnect pg --machine lab && layerfs store connect pg <url>")
    );
    assert!(!refusal.message.contains("laptop") && !refusal.message.contains("Zq7"));
    let after = std::fs::read(paths::store_record(&home, "pg")).unwrap();
    assert_eq!(before, after, "the record was written");
    std::fs::remove_dir_all(home).unwrap();
}

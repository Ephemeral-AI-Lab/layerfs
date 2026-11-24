//! A PostgreSQL Store's URL is never cleaned, stripped or rewritten (the
//! Decision of the e0aa26e149 review, owner-confirmed 2026-11-24): a URL that may
//! hold a login (an `@` after its scheme, a user or password its client
//! reads, a login key and `=` in its percent-decoded text, or one its
//! client does not read) is never shown, dialled or rewritten, and its
//! record keeps it byte for byte with a fix the user runs; any other URL is
//! shown and stored exactly as written. Over the corpora of the
//! 3cf2cfc246, aa83393447, d194c7e9df, 57d0aea5ad and e0aa26e149 reviews,
//! earlier tests and generated URLs, compared with the client itself
//! (`postgres::Config`, tokio-postgres 0.7.18's `UrlParser`):
//! (a) nothing shown, no refusal, no fix line and no record a write leaves
//! holds three bytes (all, when shorter) of the password the client reads,
//! of any value after a login key and `=`, or of the password the URL's
//! writer meant, as written or percent-decoded, except the kept bytes of
//! the record itself; (b) a URL shown equals the original byte for byte and
//! holds no login; (c) a URL dialled reads back exactly the saved login and
//! the address's endpoints, and holds no `#`; (d) no record changes on a
//! read or a refused connect, and a write that is not given a new URL for
//! it (`store attach` and `store detach` of a machine, `machine remove`)
//! keeps every URL byte for byte; (e) a refusal names only fixed words, a
//! URL holding a `#` gets the `%23` hint, a fix gives the login apart
//! exactly when the URL may hold one (or the Store is new), and every fix
//! names the address `<url>` and the user `<user>`, never text of a URL.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::{self, paths, write_entity};
use layerfs_home::names::StoreName;
use layerfs_store::postgresql::connect::{holds_login, refuse_login, refuse_url, shown, target};
use layerfs_store::postgresql::{check, login};
use layerfs_store::record::StoreRecord;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

/// The fix of a refused URL that gives the login apart.
const LOGIN_FIX: &str = "layerfs store connect pg <url> --user <user> --password-stdin";

/// What is shown of a URL that may hold a login.
const HIDDEN: &str = "postgresql://<hidden: not an address alone>";

/// The saved login's password: every byte a URL treats specially.
const LOGIN_PASSWORD: &str = "Lp4@Kv8 /?#:&=%x";

/// The login keys a URL holding one names (the client's `user` and
/// `password`, libpq 18's keys of a password file, a client key or
/// certificate and its password, and an OAuth client secret).
const LOGIN_KEYS: [&str; 7] = [
    "user",
    "password",
    "passfile",
    "sslkey",
    "sslcert",
    "sslpassword",
    "oauth_client_secret",
];

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

/// Every three-byte piece of `secret`, or all of a shorter one.
fn pieces(secret: &[u8]) -> Vec<Vec<u8>> {
    let size = secret.len().min(3);
    match size {
        0 => Vec::new(),
        size => secret.windows(size).map(<[u8]>::to_vec).collect(),
    }
}

/// `text` with each `%XX` decoded.
fn decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes
            .get(at + 1..at + 3)
            .filter(|h| h.iter().all(u8::is_ascii_hexdigit));
        match (bytes[at], hex) {
            (b'%', Some(hex)) => {
                out.push(u8::from_str_radix(std::str::from_utf8(hex).unwrap(), 16).unwrap());
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    out
}

/// The piece of `secret` that `text` holds, as written or decoded, if any.
fn leak(text: &str, secret: &[u8]) -> Option<String> {
    let forms = [text.as_bytes().to_vec(), decode(text)];
    let found = pieces(secret).into_iter().find(|piece| {
        let size = piece.len();
        forms
            .iter()
            .any(|form| form.windows(size).any(|t| t == piece.as_slice()))
    });
    found.map(|piece| String::from_utf8_lossy(&piece).into_owned())
}

/// The values after each login key and `=` in `url`, as written and
/// decoded, each to the next `&` or the end: what a URL holding one may
/// hold of a login.
fn login_values(url: &str) -> Vec<Vec<u8>> {
    let mut values = Vec::new();
    for text in [url.as_bytes().to_vec(), decode(url)] {
        let lower = text.to_ascii_lowercase();
        for key in LOGIN_KEYS {
            let key = format!("{key}=");
            let size = key.len();
            for at in (0..lower.len().saturating_sub(size - 1))
                .filter(|at| lower[*at..*at + size] == *key.as_bytes())
            {
                let rest = &text[at + size..];
                let end = rest.iter().position(|b| *b == b'&').unwrap_or(rest.len());
                values.push(rest[..end].to_vec());
            }
        }
    }
    values
}

/// The words of a fix line that are not fixed: every word but the
/// commands, flags, placeholders and the Store's name. Empty for every
/// fix: no fix names text of a URL.
fn derived(fix: &str) -> String {
    const FIXED: [&str; 9] = [
        "layerfs",
        "store",
        "--user",
        "--password-stdin",
        "&&",
        "connect",
        "disconnect",
        "<user>",
        "<url>",
    ];
    let words = (fix.split(' ')).filter(|word| !FIXED.contains(word) && *word != "pg");
    words.collect::<Vec<_>>().join(" ")
}

/// The fixed messages of a refused URL: no part of the URL is in one.
const MESSAGES: [&str; 5] = [
    "the URL names a user or holds a password (or a password file); give the address alone and the login with --user and --password-stdin",
    "the URL names a user or holds a password (or a password file); give the address alone and the login with --user and --password-stdin; an `@` or `#` that is part of the address is written %40 or %23",
    "the URL is not one its PostgreSQL client reads, or names a user or holds a password (or a password file) under a key it does not know: give the address alone, postgresql://host[:port][,host[:port]...]/database?key=value&... with keys the client knows (not `passfile`, `sslpassword`, `sslkey` or `sslcert`), and the login with --user and --password-stdin",
    "the URL holds a `#`; one that is part of the address is written %23",
    "a PostgreSQL URL starts postgresql:// or postgres://",
];

/// What a refusal of a URL holding a `#` ends with.
const HASH: &str = "; a `#` that is part of the address is written %23";

/// What the `sslrootcert` refusal adds (review 57d0aea5ad N4, e0aa26e149
/// N3: `SSL_CERT_FILE` reaches only the process that dials).
const PRIVATE_CA: &str = "; a private CA is trusted where the Store is dialled: on Linux (OpenSSL) in the system's trusted roots, or by naming its file in `SSL_CERT_FILE` for calls in that process only (a Sandbox's daemon starts with a cleared environment), on macOS by trusting it in the login or system keychain";

/// The refusal of a value the client reads but for the `#` in it (review
/// 57d0aea5ad N3).
fn hash_in(key: &str) -> String {
    format!("the URL's value of `{key}` holds a `#`, which its PostgreSQL client reads as part of the value: end the value before the `#`")
}

/// The refusal of a TLS key or `sslmode` value the client does not read.
fn tls(what: &str) -> String {
    format!("the URL's {what} is not read by its PostgreSQL client, which takes `sslmode` disable, prefer or require and no certificate, key or revocation file; `sslmode=require` verifies the server's certificate chain and host name against the trusted roots: give `sslmode=require` and keys the client knows")
}

/// The keys the client reads.
const KEYS: [&str; 19] = [
    "user",
    "password",
    "dbname",
    "options",
    "application_name",
    "sslmode",
    "sslnegotiation",
    "host",
    "hostaddr",
    "port",
    "connect_timeout",
    "tcp_user_timeout",
    "keepalives",
    "keepalives_idle",
    "keepalives_interval",
    "keepalives_retries",
    "target_session_attrs",
    "channel_binding",
    "load_balance_hosts",
];

/// Every message a refused URL may get: [`MESSAGES`] and the ones naming a
/// key or value from fixed lists, each also with the `#` hint.
fn messages() -> Vec<String> {
    let mut all: Vec<String> = MESSAGES.iter().map(|m| (*m).to_owned()).collect();
    all.push(format!("{}{PRIVATE_CA}", tls("`sslrootcert`")));
    for mode in ["allow", "verify-ca", "verify-full"] {
        all.push(tls(&format!("`sslmode={mode}`")));
    }
    all.push(tls("`sslmode` value"));
    for key in [
        "sslcert",
        "sslkey",
        "sslcrl",
        "sslcrldir",
        "sslsni",
        "sslcompression",
        "sslcertmode",
        "ssl_min_protocol_version",
        "ssl_max_protocol_version",
    ] {
        all.push(tls(&format!("`{key}`")));
    }
    for key in ["passfile", "sslpassword"] {
        all.push(format!("the URL names a user or holds a password (or a password file) in `{key}`, which its PostgreSQL client does not read; give the address alone and the login with --user and --password-stdin"));
    }
    for key in KEYS {
        all.push(format!("the URL's value of `{key}` is not one its PostgreSQL client reads: give the address with values the client reads"));
        all.push(hash_in(key));
    }
    let hinted: Vec<String> = all.iter().map(|m| format!("{m}{HASH}")).collect();
    all.extend(hinted);
    all
}

/// Writes PostgreSQL Store `pg` with `url` as its address, user `alice`
/// and machine `lab` attached, as `store.connect` and `store.attach` write
/// it.
fn store(home: &Path, url: &str) -> StoreRecord {
    let fields = vec![
        ("name", t("pg")),
        ("kind", t("postgresql")),
        ("url", t(url)),
        ("user", t("alice")),
        ("schema", t("layerfs")),
        ("synchronous_commit", t("local")),
        ("binding", SdkValue::Bytes((0..32).collect())),
        ("attached", SdkValue::List(vec![t("lab")])),
    ];
    let fields = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    write_entity(&paths::store_record(home, "pg"), &SdkValue::Map(fields)).unwrap();
    StoreRecord::of(home, "pg").unwrap()
}

/// The texts `store.get` shows of `record`'s URL.
fn row_texts(record: &StoreRecord) -> Vec<String> {
    let home = record.home().unwrap();
    let state = layerfs_home::state::State::Ready;
    let got = layerfs_store::get::summary(home, record, &state, None, true).unwrap();
    let url = got.get_text("url").unwrap_or_default();
    // the hidden form is fixed text, none of the URL's
    (url != HIDDEN)
        .then(|| url.to_owned())
        .into_iter()
        .collect()
}

/// The state fix of `record`.
fn state_fix(record: &StoreRecord) -> String {
    let state = layerfs_store::state::recorded_under(record, None).unwrap();
    let fields = state.fields().into_iter().map(|(k, v)| (k.to_owned(), v));
    let state = SdkValue::Map(fields.collect());
    state.get_text("fix").unwrap_or_default().to_owned()
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
    // review d194c7e9df S2: a user part running over the address and a query
    (
        "postgres://h/db&user=alice&password=Zq7Kx@db.example",
        "Zq7Kx",
    ),
    ("postgres://h/db&password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:5432/db&password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:6543/db&user=alice:Zq7Kx@db.example", "Zq7Kx"),
    ("postgres://h/db#password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://alice&password=Zq7Kx@db.example/app", "Zq7Kx"),
    ("postgres://h/db=Zq7Kx@db.example", "Zq7Kx"),
    (
        "postgres://h%2Fdb%26password%3DZq7Kx@db.example/app",
        "Zq7Kx",
    ),
    // N1: a short password, and one the decoded database or host holds
    ("postgres://pg:pg@h/db", "pg"),
    ("postgres://u:d@h/db", "d"),
    ("postgres://u:x@h/db", "x"),
    ("postgres://u:Zq7Xw9@h/%5Aq7x", "Zq7Xw9"),
    ("postgres://u:Zq7Xw9@%5Aq7.example/db", "Zq7Xw9"),
    // S1, N5: TLS settings the client does not read, a `#` with them
    (
        "postgresql://db.example/app?sslmode=verify-ca&sslrootcert=%2Fca.pem",
        "",
    ),
    (
        "postgresql://db.example/app?sslcert=%2Fc&sslmode=require",
        "",
    ),
    ("postgres://h/db?sslmode=allow#f", ""),
    ("postgres://h/db?port=x", ""),
    (
        "postgres://h/db?sslmode=verify-full&password=Zq7Xw9q",
        "Zq7Xw9q",
    ),
    ("postgres://h/db?sslmode=verify-full&foo=Zq7Xw9q", "Zq7Xw9q"),
    ("postgres://h/db?password=Zq7Xw9q#f", "Zq7Xw9q"),
    // a `password` with no `=`, a login value the client cannot decode
    ("postgres://h/db?sslmode=require&password", ""),
    ("postgres://h/db?sslmode=verify-full&password", ""),
    ("postgres://h/db?user=%db&sslmode=verify-full", ""),
    ("postgres://h/db?password=Zq7%db&sslmode=verify-full", "Zq7"),
    // review 57d0aea5ad S2: a user part of a host with a port, an IPv6 host
    // or a host list running into a login, with no path
    ("postgres://h:5432&password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://[::1]:5432&password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h1,h2:5433&password=Zq7@Xw9", "Zq7@Xw9"),
    (
        "postgres://h1,h2:5433&user=alice&password=Zq7@Xw9",
        "Zq7@Xw9",
    ),
    ("postgres://h:5432#password=Zq7@Xw9", "Zq7@Xw9"),
    // and `;` or `,` where `?` belongs, with and without a path
    ("postgres://h:5432;password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:5432/db;password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:5432/db,password=Zq7@Xw9", "Zq7@Xw9"),
    // a raw base64 password of a pre-2.0 user part: hidden like any other
    // URL holding an `@` (it was cleaned before the e0aa26e149 Decision)
    (
        "postgres://app_user:EhJc4g0i9YdUxm/xWMIlq0==@db.example.com/appdb",
        "EhJc4g0i9YdUxm/xWMIlq0==",
    ),
    // S3: `&` or `#` where `?` belongs, with no `@`: the client's database,
    // host or value holds the password
    ("postgres://h/db&password=Zq7", "Zq7"),
    ("postgres://h/db#password=Zq7", "Zq7"),
    ("postgres://h/db%26password=Zq7", "Zq7"),
    ("postgres://h:5432/db&user=alice&password=Zq7Xw9", "Zq7Xw9"),
    ("postgres://h&password=Zq7/db", "Zq7"),
    ("postgres://h/db?application_name=me#password=Zq7", "Zq7"),
    ("postgres://h/db?sslmode=require#password=Zq7", "Zq7"),
    ("postgres://h/db&sslmode=require", ""),
    // N3: a `#` in a value, and a user part with no `@host`
    ("postgres://h/db?sslmode=require#f", ""),
    ("postgres://alice:Secret9/db?sslmode=verify-full", "Secret9"),
    // review e0aa26e149 S1: a host with a port whose path runs into a login
    // through a separator no list named
    ("postgres://h:5432/db/password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:5432/db password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:5432/db%20password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:5432/db%26password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:5432/db%3Bpassword=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://[::1]:5432/db/password=Zq7@Xw9", "Zq7@Xw9"),
    ("postgres://h:5432/password=Zq7@Xw9", "Zq7@Xw9"),
    // S2: `?`, `;` or whitespace before a login key, with no `@`
    (
        "postgres://h/db?application_name=me?password=Zq7Xw9",
        "Zq7Xw9",
    ),
    (
        "postgres://h/db?application_name=me;password=Zq7Xw9",
        "Zq7Xw9",
    ),
    (
        "postgres://h/db?application_name=me%3Bpassword=Zq7Xw9",
        "Zq7Xw9",
    ),
    (
        "postgres://h/db?application_name=me%3Fpassword=Zq7Xw9",
        "Zq7Xw9",
    ),
    (
        "postgres://h/db?application_name=me password=Zq7Xw9",
        "Zq7Xw9",
    ),
    ("postgres://h/db;password=Zq7Xw9", "Zq7Xw9"),
    ("postgres://h/db%3Bpassword=Zq7Xw9", "Zq7Xw9"),
    ("postgres://h/db%3Fpassword=Zq7Xw9", "Zq7Xw9"),
    ("postgres://h/db password=Zq7Xw9", "Zq7Xw9"),
    ("postgres://h;password=Zq7Xw9/db", "Zq7Xw9"),
    ("postgres://h/db?sslmode=require?password=Zq7Xw9", "Zq7Xw9"),
    // every login key, in a value, the database and a host
    (
        "postgres://h/db?application_name=x%26sslkey%3DZq7Xw9",
        "Zq7Xw9",
    ),
    ("postgres://h/db%3Fsslcert=Zq7Xw9", "Zq7Xw9"),
    (
        "postgres://h/db?options=-c%20OAUTH_CLIENT_SECRET=Zq7Xw9",
        "Zq7Xw9",
    ),
    ("postgres://passfile=Zq7Xw9/db", "Zq7Xw9"),
    (
        "postgres://h/db?application_name=SslPassword%3DZq7Xw9",
        "Zq7Xw9",
    ),
    // N2: a pre-2.0 user part with a symbol password
    (
        "postgres://app_user:6WVOqXoS;53tsR4U@db.example.com:5432/appdb",
        "6WVOqXoS;53tsR4U",
    ),
    // N4 and the left-to-the-client shapes: text the client reads as part
    // of a host, database or value, no login key in it, is shown as written
    (
        "postgresql://db.example.com/app?application_name=ci%23host%3Dx",
        "",
    ),
    (
        "postgresql://db.example.com/app?application_name=a%26Port%3D1",
        "",
    ),
    (
        "postgres://h/db?options=-c%20search_path%3Dapp%2Cpublic",
        "",
    ),
    ("postgres://h/db?options=-c%20application_name%3Dx", ""),
    ("postgres://h/db/sslmode=require", ""),
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
    let held = holds_login(given);
    // the secrets a text taken from the URL may not hold: the client's
    // password, every value after a login key, and the writer's
    let mut secrets: Vec<Vec<u8>> = login_values(given);
    secrets.extend(
        (read.as_ref())
            .and_then(|r| r.get_password())
            .map(<[u8]>::to_vec),
    );
    secrets.push(meant.as_bytes().to_vec());
    let mut texts: Vec<String> = Vec::new();
    // (b) shown as written, or hidden; hidden exactly when it may hold a
    // login: an `@`, a user or password the client reads, a login key, or
    // one the client does not read
    let after = given.split_once("://").map_or("", |(_, after)| after);
    let scheme = given.starts_with("postgres://") || given.starts_with("postgresql://");
    let decoded = String::from_utf8_lossy(&decode(given)).to_ascii_lowercase();
    let keyed = LOGIN_KEYS
        .iter()
        .any(|k| decoded.contains(&format!("{k}=")));
    let client_login = read
        .as_ref()
        .is_none_or(|r| r.get_user().is_some() || r.get_password().is_some());
    let expected = !scheme || after.contains('@') || client_login || keyed;
    assert_eq!(held, expected, "(b) {given}");
    let shown = shown(given);
    if held {
        assert_eq!(shown, HIDDEN, "(b) {given}");
        assert!(target(given).is_err(), "{given}");
    } else {
        assert_eq!(shown, given, "(b) {given}");
        let read = read.as_ref().unwrap();
        let (host, port, database) = target(given).unwrap();
        assert_eq!(database, read.get_dbname().unwrap_or(""), "{given}");
        let ports: Vec<String> = read.get_ports().iter().map(u16::to_string).collect();
        if !ports.is_empty() {
            assert_eq!(port, ports.join(","), "{given}");
        }
        texts.extend([shown.clone(), host, database]);
    }
    // (e) a refusal and the fix lines of a refused connect
    let messages = messages();
    match refuse_login(given) {
        Ok(()) => assert!(!held && !after.contains('#'), "(c) {given} accepted"),
        Err(refusal) => {
            assert!(held || after.contains('#'), "{given} refused");
            assert!(
                messages.contains(&refusal.message),
                "(e) {given}: {}",
                refusal.message
            );
            if scheme && after.contains('#') {
                assert!(refusal.message.contains("%23"), "(e) {given}");
            }
            for new in [true, false] {
                let Err(refusal) = refuse_url("pg", given, new) else {
                    panic!("{given}");
                };
                assert!(messages.contains(&refusal.message), "{given}");
                let Some(fix) = refusal.detail.get_text("fix") else {
                    assert!(!scheme, "{given}: no fix");
                    continue;
                };
                assert_eq!(
                    fix.contains("--password-stdin"),
                    held || new,
                    "(e) {given}: {fix}"
                );
                assert!(fix.contains(" <url>"), "(e) {given}: {fix}");
                assert_eq!(derived(fix), "", "(e) {given}: {fix}");
            }
        }
    }
    // a record holding it as the Store's address
    let record = store(home, given);
    let bytes = || std::fs::read(paths::store_record(home, "pg")).unwrap();
    let before = bytes();
    texts.extend(row_texts(&record));
    let fixes = [state_fix(&record), login::password_fix(&record)];
    // a fix names only fixed words (checked empty here), so no secret
    for fix in fixes {
        assert_eq!(derived(&fix), "", "(e) {given}: {fix}");
        texts.push(derived(&fix));
    }
    // (c): dialled only as an address alone, the login read back exactly
    match layerfs_store::record::url(&record) {
        Ok(dialled) => {
            assert!(refuse_login(given).is_ok(), "(c) {given}: dialled");
            let back = client(&dialled).unwrap_or_else(|| panic!("(c) {given}"));
            assert_eq!(back.get_user(), Some("alice"), "(c) {given}");
            assert_eq!(
                back.get_password(),
                Some(LOGIN_PASSWORD.as_bytes()),
                "(c) {given}"
            );
            assert_eq!(
                endpoints(&back),
                endpoints(read.as_ref().unwrap()),
                "(c) {given}"
            );
        }
        Err(refusal) => {
            assert!(
                refuse_login(given).is_err(),
                "(c) {given}: {}",
                refusal.message
            );
            assert!(
                messages.contains(&refusal.message),
                "{given}: {}",
                refusal.message
            );
            let fix = refusal.detail.get_text("fix").unwrap_or_default();
            assert_eq!(derived(fix), "", "(e) {given}: {fix}");
            texts.push(derived(fix));
        }
    }
    // (d) no read wrote the record
    assert_eq!(bytes(), before, "(d) {given}: a read wrote");
    // (d) another machine is attached and detached, and a machine
    // removed: every URL stays byte for byte
    let name = StoreName::parse("pg").unwrap();
    assert!(check::attach(home, &name, "other", record.binding()).unwrap());
    let kept = StoreRecord::of(home, "pg").unwrap();
    assert_eq!(kept.text("url"), Some(given), "(d) {given}");
    check::detach(home, &name, "other", &|_| Ok(())).unwrap();
    check::forget_machine(home, "other").unwrap();
    assert_eq!(bytes(), before, "(d) {given}: rewritten");
    // (a)
    for text in &texts {
        for secret in &secrets {
            assert_eq!(leak(text, secret), None, "(a) {given}: {text}");
        }
    }
}

/// The rv_sep shapes of the e0aa26e149 review: 6 host forms, 32
/// separators, 4 key forms, the writer's password `Zq7@Xw9`.
fn separated() -> Vec<(String, String)> {
    let seps = [
        "/", " ", "%20", "+", "|", ":", "!", "$", "'", "(", "*", "~", "-", "_", ".", "%2F", "%3F",
        "%26", "%3B", "%2C", "\t", "&", "#", ";", ",", "?", "=", "\\", "^", "`", "{", "%7C",
    ];
    let hosts = [
        "h:5432",
        "h",
        "[::1]:5432",
        "h1:5432,h2:5433",
        "h:5432/db",
        "h/db",
    ];
    let keys = ["password=", "user=alice&password=", "sslpassword=", ""];
    let mut urls = Vec::new();
    for host in hosts {
        for sep in seps {
            for key in keys {
                let url = format!("postgres://{host}{sep}{key}Zq7@Xw9");
                urls.push((url, "Zq7@Xw9".to_owned()));
            }
        }
    }
    urls
}

#[test]
fn every_url_is_shown_written_and_dialled_as_the_rule_says() {
    let home = tempdir("corpus");
    login::save(&home, "pg", LOGIN_PASSWORD).unwrap();
    for (given, meant) in CORPUS {
        check(&home, given, meant);
    }
    for (given, meant) in generated().into_iter().chain(separated()) {
        check(&home, &given, &meant);
    }
    std::fs::remove_dir_all(home).unwrap();
}

/// The forms the corpus pins: what is shown and the host a row names. A
/// URL is shown as written or hidden, never cut to a form of its own.
#[test]
fn the_reviewers_urls_show_what_their_client_reads() {
    for (given, shown_as, host) in [
        // M1: the client's password `Hello?Password1`
        ("postgres://u:Hello?Password1@h/db", HIDDEN, None),
        ("postgres://u:Abc?user=x@h/db", HIDDEN, None),
        // M3: an `@` left after the user part
        (
            "postgres://u:pw@realhost/db?application_name=me@laptop.example",
            HIDDEN,
            None,
        ),
        ("postgres://u:pw@realhost/team@corp", HIDDEN, None),
        // S1, S2: shown as written, the database and hosts the client reads
        (
            "postgres://h/d#x",
            "postgres://h/d#x",
            Some(("h", "5432", "d#x")),
        ),
        (
            "postgres://h/d?dbname=a#b",
            "postgres://h/d?dbname=a#b",
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
        // a login is never cut out: the URL is hidden whole (it was shown
        // without its login before the e0aa26e149 Decision)
        ("postgres://u:Zq7/Xw9@db.example/app", HIDDEN, None),
        ("postgres://h/db?pass%77ord=x&sslmode=disable", HIDDEN, None),
        ("postgres://h/d#x?password=y", HIDDEN, None),
        // text the client reads as a database is the client's to refuse
        (
            "postgres://h/db&sslmode=require",
            "postgres://h/db&sslmode=require",
            Some(("h", "5432", "db&sslmode=require")),
        ),
    ] {
        assert_eq!(shown(given), shown_as, "{given}");
        let target = target(given).ok();
        let target = (target.as_ref()).map(|(h, p, d)| (h.as_str(), p.as_str(), d.as_str()));
        assert_eq!(target, host, "{given}");
    }
}

/// M3 / item 6: a record whose URL holds an `@` in a value is never
/// rewritten to another host: a connect of the Store with an address that
/// does not answer writes nothing.
#[test]
fn a_connect_never_rewrites_an_older_url_to_another_host() {
    let home = tempdir("update");
    login::save(&home, "pg", LOGIN_PASSWORD).unwrap();
    let given = "postgres://u:Zq7Xw9q@realhost.invalid/db?application_name=me@laptop.example";
    store(&home, given);
    let before = std::fs::read(paths::store_record(&home, "pg")).unwrap();
    let refusal = connect_again(&home).unwrap_err();
    assert_eq!(refusal.code, "Unavailable", "{}", refusal.message);
    assert!(!refusal.message.contains("laptop") && !refusal.message.contains("Zq7"));
    let after = std::fs::read(paths::store_record(&home, "pg")).unwrap();
    assert_eq!(before, after, "the record was written");
    std::fs::remove_dir_all(home).unwrap();
}

/// A connect of connected Store `pg` with an address alone where nothing
/// listens, and no new login.
fn connect_again(home: &Path) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    let request = layerfs_store::postgresql::connect::StoreConnect {
        store: StoreName::parse("pg").unwrap(),
        url: "postgres://127.0.0.1:1/app?connect_timeout=5".to_owned(),
        user: None,
        password: None,
        synchronous_commit: None,
    };
    layerfs_store::postgresql::connect::update(home, &request)
}

/// The rows of the d194c7e9df review: what each shows, names and fixes.
#[test]
fn the_d194c7e9df_review_rows_show_name_and_fix_as_the_rule_says() {
    // S2: a user part running over the address and a query is hidden, and
    // no `--user` is cut from it
    for given in [
        "postgres://h/db&user=alice&password=Zq7Kx@db.example",
        "postgres://h/db&password=Zq7@Xw9",
        "postgres://h:5432/db&password=Zq7@Xw9",
    ] {
        assert_eq!(shown(given), HIDDEN, "{given}");
        assert!(target(given).is_err(), "{given}");
        let fix = refuse_url("pg", given, true).unwrap_err();
        let fix = fix.detail.get_text("fix").unwrap().to_owned();
        assert_eq!(fix, LOGIN_FIX);
    }
    // N1: a password of one or two bytes, and any user part, is hidden
    // whole (the short user part was shown without its login before)
    for given in [
        "postgres://pg:pg@h/db",
        "postgres://u:Zq7Xw9@h/%5Aq7x",
        "postgres://ab:x@h/db",
    ] {
        assert_eq!(shown(given), HIDDEN, "{given}");
    }
    // S1: the TLS setting the client does not read is named, with what
    // `require` checks; the URL may hold a login (the client does not read
    // it), so the fix saves one (it saved none before the e0aa26e149
    // Decision)
    for (given, new, message, fix) in [
        (
            "postgresql://db.example/app?sslmode=verify-full",
            true,
            tls("`sslmode=verify-full`"),
            LOGIN_FIX,
        ),
        (
            "postgresql://db.example/app?sslrootcert=%2Fca.pem&sslmode=require",
            false,
            format!("{}{PRIVATE_CA}", tls("`sslrootcert`")),
            LOGIN_FIX,
        ),
        // N5: a `#` is named with any other refusal, and one refused only
        // for its `#` on a connected Store is connected again with the
        // address alone (`<url>`, not a form of LayerFS's own)
        (
            "postgres://h/db?sslmode=require#f",
            false,
            format!("{}{HASH}", hash_in("sslmode")),
            LOGIN_FIX,
        ),
        ("postgres://h/d#x", true, MESSAGES[3].to_owned(), LOGIN_FIX),
        (
            "postgres://h/d#x",
            false,
            MESSAGES[3].to_owned(),
            "layerfs store connect pg <url>",
        ),
        (
            "postgres://h/db?password=Zq7Xw9q#f",
            true,
            format!("{}{HASH}", MESSAGES[0]),
            LOGIN_FIX,
        ),
        // a login is refused first, and a key no list names may hold one
        (
            "postgres://h/db?sslmode=verify-full&password=Zq7Xw9q",
            true,
            MESSAGES[0].to_owned(),
            LOGIN_FIX,
        ),
        (
            "postgres://h/db?sslmode=verify-full&foo=Zq7Xw9q",
            true,
            tls("`sslmode=verify-full`"),
            LOGIN_FIX,
        ),
        // a `password` with no `=`, and a `user` the client cannot decode,
        // are a login all the same (found by fuzzing)
        (
            "postgres://h/db?sslmode=verify-full&password",
            true,
            MESSAGES[0].to_owned(),
            LOGIN_FIX,
        ),
        (
            "postgres://h/db?user=%db&sslmode=verify-full",
            true,
            MESSAGES[0].to_owned(),
            LOGIN_FIX,
        ),
    ] {
        let refusal = refuse_url("pg", given, new).unwrap_err();
        assert_eq!(refusal.message, message, "{given}");
        assert_eq!(refusal.detail.get_text("fix"), Some(fix), "{given}");
    }
}

/// The rows of the 57d0aea5ad review: S2 (a user part of a host with a
/// port, an IPv6 host or a host list), S3 (`&` or `#` where `?` belongs,
/// with no `@`), N1 (a raw base64 password) and N3, N4 (refusal words).
#[test]
fn the_57d0aea5ad_review_rows_are_hidden_refused_and_named() {
    // S2: hidden, the fix's address `<url>` and user `<user>`
    for given in [
        "postgres://h:5432&password=Zq7@Xw9",
        "postgres://[::1]:5432&password=Zq7@Xw9",
        "postgres://h1,h2:5433&user=alice&password=Zq7@Xw9",
        "postgres://h:5432#password=Zq7@Xw9",
        "postgres://h:5432/db;password=Zq7@Xw9",
    ] {
        assert_eq!(shown(given), HIDDEN, "{given}");
        assert!(target(given).is_err(), "{given}");
        let fix = refuse_url("pg", given, true).unwrap_err();
        assert_eq!(fix.detail.get_text("fix"), Some(LOGIN_FIX), "{given}");
    }
    // N1: a raw base64 password in a pre-2.0 user part is hidden whole
    // (it was cleaned before the e0aa26e149 Decision)
    let given = "postgres://app_user:EhJc4g0i9YdUxm/xWMIlq0==@db.example.com/appdb";
    assert_eq!(shown(given), HIDDEN);
    // S3: a login key after `&` or `#` is refused as a login and hidden;
    // `&` before another key is the client's database, shown as written
    // (it was refused before the e0aa26e149 Decision)
    for given in [
        "postgres://h/db&password=Zq7",
        "postgres://h/db#password=Zq7",
    ] {
        let refusal = refuse_url("pg", given, true).unwrap_err();
        assert!(refusal.message.starts_with(MESSAGES[0]), "{given}");
        assert_eq!(refusal.detail.get_text("fix"), Some(LOGIN_FIX), "{given}");
        assert_eq!(shown(given), HIDDEN, "{given}");
        assert!(target(given).is_err(), "{given}");
    }
    assert!(refuse_login("postgres://h/db&sslmode=require").is_ok());
    // S3: `store connect` refuses it before any dial, and records nothing
    let home = tempdir("smuggled");
    login::save(&home, "pg", LOGIN_PASSWORD).unwrap();
    for given in [
        "postgres://h/db&password=Zq7",
        "postgres://h/db#password=Zq7",
    ] {
        let request = layerfs_store::postgresql::connect::StoreConnect {
            store: StoreName::parse("pg").unwrap(),
            url: given.to_owned(),
            user: Some("alice".to_owned()),
            password: Some(LOGIN_PASSWORD.to_owned()),
            synchronous_commit: None,
        };
        let refusal = layerfs_store::postgresql::connect::connect(&home, &request).unwrap_err();
        assert!(refusal.message.starts_with(MESSAGES[0]), "{given}");
        let fix = refusal.detail.get_text("fix").unwrap();
        assert!(!fix.contains("Zq7") && fix.contains(" <url> "), "{fix}");
        assert!(
            !paths::store_record(&home, "pg").exists(),
            "{given} recorded"
        );
        // an older record holding it is hidden and never rewritten
        let record = store(&home, given);
        assert!(!row_texts(&record).iter().any(|text| text.contains("Zq7")));
        let dialled = layerfs_store::record::url(&record).unwrap_err();
        assert!(!format!("{dialled:?}").contains("Zq7"), "{dialled:?}");
        std::fs::remove_file(paths::store_record(&home, "pg")).unwrap();
    }
    std::fs::remove_dir_all(home).unwrap();
    // N3: a `#` in a value is named as the fault; a user part with no
    // `@host` and a TLS setting the client does not read may hold a login
    // (the client does not read them), so their fix saves one; N4:
    // `sslrootcert` names where a private CA is trusted
    let save = LOGIN_FIX;
    for (given, message, fix) in [
        (
            "postgres://h/db?sslmode=require#f",
            format!("{}{HASH}", hash_in("sslmode")),
            save,
        ),
        (
            "postgres://h/db?sslmode=verify-full#f",
            format!("{}{HASH}", tls("`sslmode=verify-full`")),
            save,
        ),
        (
            "postgres://alice:Secret9/db?sslmode=verify-full",
            tls("`sslmode=verify-full`"),
            save,
        ),
        (
            "postgresql://db.example/app?sslrootcert=%2Fca.pem&sslmode=require",
            format!("{}{PRIVATE_CA}", tls("`sslrootcert`")),
            save,
        ),
    ] {
        let refusal = refuse_url("pg", given, true).unwrap_err();
        assert_eq!(refusal.message, message, "{given}");
        assert_eq!(refusal.detail.get_text("fix"), Some(fix), "{given}");
    }
}

/// The rows of the e0aa26e149 review: S1 (a host with a port whose path
/// runs into a login through any separator), S2 (`?`, `;` or whitespace
/// before a login key: accepted and dialled before), N2 (symbol passwords)
/// and N3 (`SSL_CERT_FILE` reaches only the process that dials). Each is
/// hidden, refused before any dial, and its record kept byte for byte.
#[test]
fn the_e0aa26e149_review_rows_are_hidden_refused_and_kept() {
    let home = tempdir("e0");
    login::save(&home, "pg", LOGIN_PASSWORD).unwrap();
    for given in [
        "postgres://h:5432/db/password=Zq7@Xw9",
        "postgres://h:5432/db password=Zq7@Xw9",
        "postgres://h:5432/db%20password=Zq7@Xw9",
        "postgres://h:5432/db%26password=Zq7@Xw9",
        "postgres://h:5432/db%3Bpassword=Zq7@Xw9",
        "postgres://[::1]:5432/db/password=Zq7@Xw9",
        "postgres://h:5432/password=Zq7@Xw9",
        "postgres://h/db?application_name=me?password=Zq7Xw9",
        "postgres://h/db?application_name=me;password=Zq7Xw9",
        "postgres://h/db?application_name=me%3Bpassword=Zq7Xw9",
        "postgres://h/db?application_name=me%3Fpassword=Zq7Xw9",
        "postgres://h/db?application_name=me password=Zq7Xw9",
        "postgres://h/db;password=Zq7Xw9",
        "postgres://h/db%3Bpassword=Zq7Xw9",
        "postgres://h/db%3Fpassword=Zq7Xw9",
        "postgres://h/db password=Zq7Xw9",
        "postgres://h;password=Zq7Xw9/db",
        "postgres://h/db?sslmode=require?password=Zq7Xw9",
        "postgres://app_user:6WVOqXoS;53tsR4U@db.example.com:5432/appdb",
    ] {
        assert!(holds_login(given), "{given}");
        assert_eq!(shown(given), HIDDEN, "{given}");
        assert!(target(given).is_err(), "{given}");
        // named as a login, or (`sslmode=require?password=`, which the client
        // does not read) by the key the client refuses, in fixed words
        let refusal = refuse_login(given).unwrap_err();
        assert!(messages().contains(&refusal.message), "{given}");
        let record = store(&home, given);
        let before = std::fs::read(paths::store_record(&home, "pg")).unwrap();
        assert!(
            layerfs_store::record::url(&record).is_err(),
            "{given} dialled"
        );
        assert!(
            row_texts(&record).iter().all(|text| text == HIDDEN),
            "{given}"
        );
        let refusal = connect_again(&home).unwrap_err();
        assert_eq!(refusal.code, "Unavailable", "{given}: {refusal:?}");
        let after = std::fs::read(paths::store_record(&home, "pg")).unwrap();
        assert_eq!(before, after, "{given}: written");
    }
    // N3: `SSL_CERT_FILE` is named for the calling process only, and both
    // macOS keychains
    let refusal = refuse_login("postgresql://db.example/app?sslrootcert=%2Fca.pem").unwrap_err();
    assert!(
        refusal.message.contains("for calls in that process only")
            && refusal.message.contains("cleared environment")
            && refusal.message.contains("login or system keychain"),
        "{}",
        refusal.message
    );
    std::fs::remove_dir_all(home).unwrap();
}

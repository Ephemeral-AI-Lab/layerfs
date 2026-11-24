//! A PostgreSQL Store's URL is read only as its client reads it
//! (`postgres::Config`, tokio-postgres 0.7.18's `UrlParser`), never by a
//! parser of LayerFS's own (the Decision of the aa83393447 review). Over
//! the corpora of the 3cf2cfc246 and aa83393447 reviews, earlier tests and
//! generated URLs, compared with the client itself:
//! (a) nothing shown, no fix line and no record a write leaves holds three
//! bytes (all, when shorter) of the password the client reads, nor of the
//! password the URL's writer meant, as written or percent-decoded; (b) a URL shown or written reads, by the client, the same
//! hosts, host addresses, ports and database as the original and no login;
//! (c) the strip is idempotent; (d) a URL dialled reads exactly the saved
//! login and the address's endpoints; (e) a fix's `--user` is the client's
//! user, unless it holds an address or a query; (f) a refusal names only
//! fixed words, a URL holding a `#` gets the `%23` hint, and a fix saves a
//! login only when the URL may hold one; (g) a URL whose client reads `&` or
//! `#` before a client or login key in its host, database or a text value
//! is refused and hidden. A record whose URL has no clean form is shown
//! hidden and never rewritten, and `store update --auth` refuses it with a
//! fix each step of which dials with a login that exists.
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

/// What a refusal of a URL holding a `#` ends with.
const HASH: &str = "; a `#` that is part of the address is written %23";

/// The refusal of a URL whose client reads `&` or `#` before a key in its
/// host, database or a text value (review 57d0aea5ad S3).
const SMUGGLED: &str = "the URL's host, database or a value holds `&` or `#` (or %26 or %23) before a key its PostgreSQL client reads or a login key, and the client reads it as part of that text, not as a setting: give the address alone with its settings after a `?`, and the login with --auth (`layerfs store save-auth` saves one)";

/// What the `sslrootcert` refusal adds (review 57d0aea5ad N4).
const PRIVATE_CA: &str = "; a private CA is trusted where the Store is dialled: on Linux (OpenSSL) by naming its file in `SSL_CERT_FILE`, on macOS by trusting it in the system keychain";

/// The refusal of a value the client reads but for the `#` in it (review
/// 57d0aea5ad N3).
fn hash_in(key: &str) -> String {
    format!("the URL's value of `{key}` holds a `#`, which its PostgreSQL client reads as part of the value: end the value before the `#`")
}

/// The refusal of a TLS key or `sslmode` value the client does not read.
fn tls(what: &str) -> String {
    format!("the URL's {what} is not read by its PostgreSQL client, which takes `sslmode` disable, prefer or require and no certificate, key or revocation file; `sslmode=require` verifies the server's certificate chain and host name against the trusted roots: give `sslmode=require` and keys the client knows")
}

/// Every message a refused URL may get: [`MESSAGES`] and the ones naming a
/// key or value from fixed lists, each also with the `#` hint.
fn messages() -> Vec<String> {
    let mut all: Vec<String> = MESSAGES.iter().map(|m| (*m).to_owned()).collect();
    all.push(SMUGGLED.to_owned());
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
        all.push(format!("the URL names a user or holds a password (or a password file) in `{key}`, which its PostgreSQL client does not read; give the address alone and the login with --auth (`layerfs store save-auth` saves one)"));
    }
    for key in [
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
    ] {
        all.push(format!("the URL's value of `{key}` is not one its PostgreSQL client reads: give the address with values the client reads"));
        all.push(hash_in(key));
    }
    let hinted: Vec<String> = all.iter().map(|m| format!("{m}{HASH}")).collect();
    all.extend(hinted);
    all
}

/// The bytes that end a user, a host list or a path, or split pairs.
const ADDRESS: [char; 5] = ['/', '?', '&', '=', '#'];

/// The keys a no-login refusal's URL may hold: the client's, and libpq's
/// TLS keys it does not read.
const KNOWN: [&str; 29] = [
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
    "sslrootcert",
    "sslcert",
    "sslkey",
    "sslcrl",
    "sslcrldir",
    "sslsni",
    "sslcompression",
    "sslcertmode",
    "ssl_min_protocol_version",
    "ssl_max_protocol_version",
];

/// The query pairs of `after` (a URL after its scheme, holding no `@`) as
/// the client splits them: from the first `?`, a key to its `=`, a value
/// to the next `&`.
fn client_pairs(after: &str) -> Vec<&str> {
    let mut query = after.split_once('?').map_or("", |(_, query)| query);
    let mut pairs = Vec::new();
    while !query.is_empty() {
        let end = (query.find('='))
            .and_then(|eq| query[eq..].find('&').map(|amp| eq + amp))
            .unwrap_or(query.len());
        pairs.push(&query[..end]);
        query = query.get(end + 1..).unwrap_or("");
    }
    pairs
}

/// Writes PostgreSQL Store `pg` with `url` as its address and machine
/// `lab`'s own, naming login `pg`.
fn store(home: &Path, url: &str) -> StoreRecord {
    store_with(home, url, url, Some("pg"))
}

/// Writes PostgreSQL Store `pg` with address `url`, machine `lab`'s own
/// address `lab`, naming login `auth` when given.
fn store_with(home: &Path, url: &str, lab: &str, auth: Option<&str>) -> StoreRecord {
    let own = SdkValue::map([("url", t(lab))]);
    let mut fields = vec![
        ("name", t("pg")),
        ("kind", t("postgresql")),
        ("url", t(url)),
        ("schema", t("layerfs")),
        ("synchronous_commit", t("local")),
        ("binding", SdkValue::Bytes((0..32).collect())),
        ("machines", SdkValue::map([("lab", own)])),
    ];
    fields.extend(auth.map(|auth| ("auth", t(auth))));
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
    // a raw base64 password of a pre-2.0 user part is cleaned, not hidden
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
    // (g) a URL whose client reads `&` or `#` before a client or login key
    // in its host, database or a text value is refused and hidden
    if let Some(read) = read.as_ref() {
        let mut values: Vec<String> = (read.get_hosts().iter())
            .map(|host| match host {
                postgres::config::Host::Tcp(name) => name.clone(),
                #[cfg(unix)]
                postgres::config::Host::Unix(path) => path.display().to_string(),
            })
            .collect();
        let texts = [
            read.get_dbname(),
            read.get_options(),
            read.get_application_name(),
        ];
        values.extend(texts.into_iter().flatten().map(str::to_owned));
        let keys = KNOWN[..19].iter().chain(["passfile", "sslpassword"].iter());
        let keys: Vec<&str> = keys.copied().collect();
        let held = values.iter().any(|value| {
            let value = value.to_ascii_lowercase();
            (keys.iter())
                .any(|k| value.contains(&format!("&{k}=")) || value.contains(&format!("#{k}=")))
        });
        if held {
            assert!(refuse_login(given).is_err(), "(g) {given}");
            assert_eq!(shown, HIDDEN, "(g) {given}");
        }
    }
    // (e) and the fix lines of a refused connect
    let messages = messages();
    let after = given.split_once("://").map_or("", |(_, after)| after);
    if let Err(refusal) = refuse_login(given) {
        assert!(
            messages.contains(&refusal.message),
            "(f) {given}: {}",
            refusal.message
        );
        // (f) a URL holding a `#` is told how to write one
        let scheme = given.starts_with("postgres://") || given.starts_with("postgresql://");
        if scheme && after.contains('#') {
            assert!(refusal.message.contains("%23"), "(f) {given}");
        }
        let named = (read.as_ref())
            .and_then(|r| r.get_user())
            .filter(|user| !user.is_empty() && shown != HIDDEN && !user.contains(ADDRESS));
        assert_eq!(url_user(given).as_deref(), named, "(e) {given}");
        for (first, machine) in [(true, None), (false, None), (false, Some("lab"))] {
            let Err(refusal) = refuse_login_with_fix("pg", given, first, machine) else {
                panic!("{given}");
            };
            assert!(messages.contains(&refusal.message), "{given}");
            let Some(fix) = refusal.detail.get_text("fix") else {
                assert!(!scheme, "{given}: no fix");
                continue;
            };
            // (f) a fix saves a login only when the URL may hold one: one
            // holding no `@`, whose client reads no login, refused for its
            // `#` or a TLS setting, only connects the address alone
            if !fix.starts_with("layerfs store save-auth ") {
                assert!(!after.contains('@'), "(f) {given}: {fix}");
                // and every pair, as the client splits them, is a key=value
                // under a key the client reads or a TLS key, none a login's
                for pair in client_pairs(after) {
                    let key = pair.split_once('=').map(|(key, _)| decode(key));
                    let key = key.unwrap_or_else(|| panic!("(f) {given}: {fix}"));
                    let key = String::from_utf8_lossy(&key).into_owned();
                    let login = ["user", "password", "passfile", "sslpassword"];
                    assert!(!login.contains(&key.as_str()), "(f) {given}: {fix}");
                    assert!(KNOWN.contains(&key.as_str()), "(f) {given}: {fix}");
                }
                assert!(
                    read.as_ref()
                        .is_none_or(|r| r.get_user().is_none() && r.get_password().is_none()),
                    "(f) {given}: {fix}"
                );
                assert!(!fix.contains("--user"), "(f) {given}: {fix}");
                assert!(
                    fix.starts_with("layerfs store connect pg "),
                    "(f) {given}: {fix}"
                );
            } else {
                let user = named.map_or("<user>".to_owned(), shell_word);
                assert_eq!(fix_user(fix), user, "(e) {given}: {fix}");
            }
            if refusal.message == MESSAGES[3] {
                assert!(!fix.contains("save-auth"), "(f) {given}: {fix}");
            }
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
            assert!(messages.iter().any(|m| m == message), "{given}: {message}");
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
    // the machine's own address is connected again, not left on the
    // Store's (review d194c7e9df N2)
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs store connect pg <url> --machine lab && layerfs store connect pg <url>")
    );
    assert!(!refusal.message.contains("laptop") && !refusal.message.contains("Zq7"));
    let after = std::fs::read(paths::store_record(&home, "pg")).unwrap();
    assert_eq!(before, after, "the record was written");
    std::fs::remove_dir_all(home).unwrap();
}

/// The rows of the d194c7e9df review: what each shows, names and fixes.
#[test]
fn the_d194c7e9df_review_rows_show_name_and_fix_as_their_client_reads() {
    // S2: a user part running over the address and a query is hidden, and
    // no `--user` is cut from it
    for given in [
        "postgres://h/db&user=alice&password=Zq7Kx@db.example",
        "postgres://h/db&password=Zq7@Xw9",
        "postgres://h:5432/db&password=Zq7@Xw9",
    ] {
        assert_eq!(without_login(given), HIDDEN, "{given}");
        assert_eq!(url_user(given), None, "{given}");
        assert!(target(given).is_err(), "{given}");
        let fix = refuse_login_with_fix("pg", given, true, None).unwrap_err();
        let fix = fix.detail.get_text("fix").unwrap().to_owned();
        assert_eq!(
            fix,
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>"
        );
    }
    // N1: a password of one or two bytes is checked too
    for given in ["postgres://pg:pg@h/db", "postgres://u:Zq7Xw9@h/%5Aq7x"] {
        assert_eq!(without_login(given), HIDDEN, "{given}");
        assert_eq!(url_user(given), None, "{given}");
    }
    assert_eq!(without_login("postgres://ab:x@h/db"), "postgres://h/db");
    assert_eq!(url_user("postgres://ab:x@h/db").as_deref(), Some("ab"));
    // S1: the TLS setting the client does not read is named, with what
    // `require` checks; no login is saved for it
    for (given, first, machine, message, fix) in [
        (
            "postgresql://db.example/app?sslmode=verify-full",
            true,
            None,
            tls("`sslmode=verify-full`"),
            "layerfs store connect pg <url> --auth <auth>",
        ),
        (
            "postgresql://db.example/app?sslrootcert=%2Fca.pem&sslmode=require",
            false,
            None,
            format!("{}{PRIVATE_CA}", tls("`sslrootcert`")),
            "layerfs store connect pg <url>",
        ),
        // N5: a `#` is named with any other refusal, and one refused only
        // for its `#` is connected again with it written %23
        (
            "postgres://h/db?sslmode=require#f",
            false,
            Some("lab"),
            format!("{}{HASH}", hash_in("sslmode")),
            "layerfs store connect pg <url> --machine lab",
        ),
        (
            "postgres://h/d#x",
            true,
            None,
            MESSAGES[3].to_owned(),
            "layerfs store connect pg postgres://h/d%23x --auth <auth>",
        ),
        (
            "postgres://h/db?password=Zq7Xw9q#f",
            true,
            None,
            format!("{}{HASH}", MESSAGES[0]),
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg postgres://h/db --auth <auth>",
        ),
        // a login is refused first, and a key no list names may hold one
        (
            "postgres://h/db?sslmode=verify-full&password=Zq7Xw9q",
            true,
            None,
            MESSAGES[0].to_owned(),
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>",
        ),
        (
            "postgres://h/db?sslmode=verify-full&foo=Zq7Xw9q",
            true,
            None,
            tls("`sslmode=verify-full`"),
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>",
        ),
        // a `password` with no `=`, and a `user` the client cannot decode,
        // are a login all the same (found by fuzzing)
        (
            "postgres://h/db?sslmode=verify-full&password",
            true,
            None,
            MESSAGES[0].to_owned(),
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>",
        ),
        (
            "postgres://h/db?user=%db&sslmode=verify-full",
            true,
            None,
            MESSAGES[0].to_owned(),
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>",
        ),
    ] {
        let refusal = refuse_login_with_fix("pg", given, first, machine).unwrap_err();
        assert_eq!(refusal.message, message, "{given}");
        assert_eq!(refusal.detail.get_text("fix"), Some(fix), "{given}");
    }
}

/// N2, N3 of the d194c7e9df review: a machine's own address with no clean
/// form is connected again as the address alone (after its disconnect when
/// the record names no login), and `store update --auth`'s fix names the
/// login it was given, built from the record as it is (review 57d0aea5ad
/// S1: the record's login `old` is not saved, so no step dials with it).
#[test]
fn a_machine_address_is_connected_again_and_an_update_names_its_login() {
    let home = tempdir("machine-fix");
    login::save(&home, "new", "alice", LOGIN_PASSWORD).unwrap();
    let (clean, unclean) = (
        "postgres://db.example/app",
        "postgres://u:Zq7@Xw9@db.example/app",
    );
    let named = store_with(&home, clean, unclean, Some("new"));
    assert_eq!(
        login::outdated_fix(&named, "new"),
        "layerfs store connect pg <url> --machine lab && layerfs store update pg --auth new"
    );
    let none = store_with(&home, clean, unclean, None);
    assert_eq!(
        login::no_auth_fix(&none),
        "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store disconnect pg --machine lab && layerfs store update pg --auth <auth> && layerfs store connect pg <url> --machine lab"
    );
    // the whole Store goes with an unclean address: every machine's own
    // address is connected again after it
    let both = store_with(&home, unclean, clean, None);
    assert_eq!(
        login::no_auth_fix(&both),
        "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store disconnect pg && layerfs store connect pg <url> --auth <auth> && layerfs store connect pg <url> --machine lab"
    );
    // N3: the record names `old`, not saved; the update gives `new`; its fix
    // is a record naming no login's, naming `new` and saving nothing
    store_with(&home, clean, unclean, Some("old"));
    let before = std::fs::read(paths::store_record(&home, "pg")).unwrap();
    let request = layerfs_store::postgresql::update::StoreUpdate {
        store: StoreName::parse("pg").unwrap(),
        auth: Some("new".to_owned()),
        synchronous_commit: None,
    };
    let refusal = layerfs_store::postgresql::update::update(&home, &request).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    assert_eq!(
        refusal.detail.get_text("fix"),
        Some("layerfs store disconnect pg --machine lab && layerfs store update pg --auth new && layerfs store connect pg <url> --machine lab")
    );
    let after = std::fs::read(paths::store_record(&home, "pg")).unwrap();
    assert_eq!(before, after, "the record was written");
    std::fs::remove_dir_all(home).unwrap();
}

/// `store update pg --auth new` of record `pg` (address `url`, machine
/// `lab`'s own `lab`, login `auth`): its refusal's fix, the record left as
/// it was.
fn update_fix(home: &Path, url: &str, lab: &str, auth: Option<&str>, new: &str) -> String {
    store_with(home, url, lab, auth);
    let before = std::fs::read(paths::store_record(home, "pg")).unwrap();
    let request = layerfs_store::postgresql::update::StoreUpdate {
        store: StoreName::parse("pg").unwrap(),
        auth: Some(new.to_owned()),
        synchronous_commit: None,
    };
    let refusal = layerfs_store::postgresql::update::update(home, &request).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    let after = std::fs::read(paths::store_record(home, "pg")).unwrap();
    assert_eq!(before, after, "the record was written");
    refusal.detail.get_text("fix").unwrap().to_owned()
}

/// S1 of the 57d0aea5ad review: `store update --auth new`'s fix is built
/// from the record as it is, so each step dials with a login that exists:
/// the record's own when it is saved (its URLs replaced first, `update
/// --auth new` last), else none (a machine's own address disconnected
/// first, which dials nothing, or the whole Store, connected again with
/// `--auth new`). Every step is one the call guide accepts; a Store address
/// with no clean form still ends in `--auth new`.
#[test]
fn an_update_fix_dials_only_with_a_login_that_exists() {
    let home = tempdir("update-fix");
    login::save(&home, "new", "alice", LOGIN_PASSWORD).unwrap();
    login::save(&home, "old", "bob", LOGIN_PASSWORD).unwrap();
    let (clean, unclean) = (
        "postgres://db.invalid/app",
        "postgres://u:Zq7@Xw9@db.invalid/app",
    );
    for (url, lab, auth, fix) in [
        // the record's login `old` is saved: it dials with it until the end
        (
            clean,
            unclean,
            Some("old"),
            "layerfs store connect pg <url> --machine lab && layerfs store update pg --auth new",
        ),
        (
            unclean,
            clean,
            Some("old"),
            "layerfs store connect pg <url> && layerfs store update pg --auth new",
        ),
        (
            unclean,
            unclean,
            Some("old"),
            "layerfs store connect pg <url> --machine lab && layerfs store connect pg <url> && layerfs store update pg --auth new",
        ),
        // the update names the record's own login: no second update
        (
            clean,
            unclean,
            Some("new"),
            "layerfs store connect pg <url> --machine lab && layerfs store update pg --auth new",
        ),
        (
            unclean,
            clean,
            Some("new"),
            "layerfs store connect pg <url>",
        ),
        // `gone` is not saved: nothing dials with it
        (
            clean,
            unclean,
            Some("gone"),
            "layerfs store disconnect pg --machine lab && layerfs store update pg --auth new && layerfs store connect pg <url> --machine lab",
        ),
        (
            unclean,
            clean,
            Some("gone"),
            "layerfs store disconnect pg && layerfs store connect pg <url> --auth new && layerfs store connect pg <url> --machine lab",
        ),
        // a record naming no login (one from before saved logins)
        (
            clean,
            unclean,
            None,
            "layerfs store disconnect pg --machine lab && layerfs store update pg --auth new && layerfs store connect pg <url> --machine lab",
        ),
        (
            unclean,
            unclean,
            None,
            "layerfs store disconnect pg && layerfs store connect pg <url> --auth new && layerfs store connect pg <url> --machine lab",
        ),
    ] {
        let got = update_fix(&home, url, lab, auth, "new");
        assert_eq!(got, fix, "{url} {lab} {auth:?}");
        assert!(!got.contains("save-auth") && !got.contains("Zq7"), "{got}");
        // a fix that ends anywhere but `--auth new` would leave the old
        // login; one already naming `new` keeps it
        let last = got.rsplit(" && ").find(|step| !step.contains("--machine"));
        assert!(auth == Some("new") || last.unwrap().ends_with("--auth new"), "{got}");
    }
    // the first step of a fix naming no login dials nothing and leaves the
    // update to pass the address check: `store disconnect --machine` is
    // `check::save_own` with no connection
    let record = store_with(&home, clean, unclean, Some("gone"));
    let own = layerfs_store::record::OwnConnection::default();
    let name = StoreName::parse("pg").unwrap();
    layerfs_store::postgresql::check::save_own(&home, &name, "lab", &own, record.binding())
        .unwrap();
    let request = layerfs_store::postgresql::update::StoreUpdate {
        store: name,
        auth: Some("new".to_owned()),
        synchronous_commit: None,
    };
    // its next step dials `new` on the Store's address: `db.invalid` is not
    // reached, and the refusal is no longer the URL's
    let refusal = layerfs_store::postgresql::update::update(&home, &request).unwrap_err();
    assert!(
        !refusal.message.contains("has no form"),
        "{}",
        refusal.message
    );
    std::fs::remove_dir_all(home).unwrap();
}

/// The rows of the 57d0aea5ad review: S2 (a user part of a host with a
/// port, an IPv6 host or a host list), S3 (`&` or `#` where `?` belongs,
/// with no `@`), N1 (a raw base64 password) and N3, N4 (refusal words).
#[test]
fn the_57d0aea5ad_review_rows_are_hidden_refused_and_named() {
    // S2: hidden, no `--user`, the fix's address `<url>`
    for given in [
        "postgres://h:5432&password=Zq7@Xw9",
        "postgres://[::1]:5432&password=Zq7@Xw9",
        "postgres://h1,h2:5433&user=alice&password=Zq7@Xw9",
        "postgres://h:5432#password=Zq7@Xw9",
        "postgres://h:5432/db;password=Zq7@Xw9",
    ] {
        assert_eq!(without_login(given), HIDDEN, "{given}");
        assert_eq!(url_user(given), None, "{given}");
        assert!(target(given).is_err(), "{given}");
        let fix = refuse_login_with_fix("pg", given, true, None).unwrap_err();
        assert_eq!(
            fix.detail.get_text("fix"),
            Some("layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>"),
            "{given}"
        );
    }
    // N1: a raw base64 password in a pre-2.0 user part is cleaned
    let given = "postgres://app_user:EhJc4g0i9YdUxm/xWMIlq0==@db.example.com/appdb";
    assert_eq!(without_login(given), "postgres://db.example.com/appdb");
    assert_eq!(url_user(given).as_deref(), Some("app_user"));
    // S3: refused with fixed words, the fix's address `<url>`, a login
    // saved first when the key is a login's
    for (given, fix) in [
        (
            "postgres://h/db&password=Zq7",
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>",
        ),
        (
            "postgres://h/db#password=Zq7",
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>",
        ),
        (
            "postgres://h/db&sslmode=require",
            "layerfs store connect pg <url> --auth <auth>",
        ),
    ] {
        let refusal = refuse_login_with_fix("pg", given, true, None).unwrap_err();
        assert_eq!(refusal.message, SMUGGLED, "{given}");
        assert_eq!(refusal.detail.get_text("fix"), Some(fix), "{given}");
        assert_eq!(without_login(given), HIDDEN, "{given}");
        assert!(target(given).is_err(), "{given}");
    }
    // S3: `store connect` refuses it before any dial, and records nothing
    let home = tempdir("smuggled");
    login::save(&home, "pg", "alice", LOGIN_PASSWORD).unwrap();
    for given in [
        "postgres://h/db&password=Zq7",
        "postgres://h/db#password=Zq7",
    ] {
        let request = layerfs_store::postgresql::connect::StoreConnect {
            store: StoreName::parse("pg").unwrap(),
            url: given.to_owned(),
            auth: Some("pg".to_owned()),
            synchronous_commit: None,
        };
        let refusal = layerfs_store::postgresql::connect::connect(&home, &request).unwrap_err();
        assert_eq!(refusal.message, SMUGGLED, "{given}");
        let fix = refusal.detail.get_text("fix").unwrap();
        assert!(!fix.contains("Zq7") && fix.contains(" <url> "), "{fix}");
        assert!(
            !paths::store_record(&home, "pg").exists(),
            "{given} recorded"
        );
        // an older record holding it is hidden and never rewritten
        let record = store(&home, given);
        assert!(record.outdated_fields(), "{given}");
        let kept = record.clone().cleaned();
        assert_eq!(kept.text("url"), Some(given), "{given} rewritten");
        assert!(!row_texts(&record).iter().any(|text| text.contains("Zq7")));
        let dialled = layerfs_store::record::url(&record).unwrap_err();
        let fix = dialled.detail.get_text("fix").unwrap();
        assert!(!fix.contains("Zq7") && fix.contains(" <url>"), "{fix}");
        std::fs::remove_file(paths::store_record(&home, "pg")).unwrap();
    }
    std::fs::remove_dir_all(home).unwrap();
    // N3: a `#` in a value is named as the fault; a user part with no
    // `@host` may hold a login, so its fix saves one; N4: `sslrootcert`
    // names where a private CA is trusted
    for (given, message, fix) in [
        (
            "postgres://h/db?sslmode=require#f",
            format!("{}{HASH}", hash_in("sslmode")),
            "layerfs store connect pg <url> --auth <auth>",
        ),
        (
            "postgres://h/db?sslmode=verify-full#f",
            format!("{}{HASH}", tls("`sslmode=verify-full`")),
            "layerfs store connect pg <url> --auth <auth>",
        ),
        (
            "postgres://alice:Secret9/db?sslmode=verify-full",
            tls("`sslmode=verify-full`"),
            "layerfs store save-auth <auth> --user <user> --password-stdin && layerfs store connect pg <url> --auth <auth>",
        ),
        (
            "postgresql://db.example/app?sslrootcert=%2Fca.pem&sslmode=require",
            format!("{}{PRIVATE_CA}", tls("`sslrootcert`")),
            "layerfs store connect pg <url> --auth <auth>",
        ),
    ] {
        let refusal = refuse_login_with_fix("pg", given, true, None).unwrap_err();
        assert_eq!(refusal.message, message, "{given}");
        assert_eq!(refusal.detail.get_text("fix"), Some(fix), "{given}");
    }
}

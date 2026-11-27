//! A PostgreSQL Store's URL is never cleaned, stripped or rewritten (the
//! Decision of the e0aa26e149 review: five rounds of a grammar of LayerFS's
//! own each mishandled a new shape). A URL that may hold a login
//! ([`holds_login`]) is never shown ([`HIDDEN`]) and never dialled; its
//! record keeps it byte for byte until the user connects an address
//! holding none. Any other URL is shown and stored exactly as written. A
//! URL is dialled only when it is an address alone ([`refuse_login`]), with
//! the Store's login put in and read back by the client ([`with_login`]):
//! tokio-postgres 0.7, [`layerfs_persistence::PostgresConfig`], the reading
//! every Store session dials with.
use layerfs_protocol::provision::install::SdkRefusal;
use layerfs_home::outcome::refused;
use layerfs_persistence::{PostgresConfig as Config, PostgresHost as Host};
use std::net::IpAddr;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What is shown of a URL that may hold a login ([`holds_login`]): none of
/// its text.
pub const HIDDEN: &str = "postgresql://<hidden: not an address alone>";

/// The refusal of a URL holding a login (spec v23 V23-13).
const LOGIN: &str = "the URL names a user or holds a password (or a password file); give the address alone and the login with --user and --password-stdin";

/// The refusal of a URL the client does not read, when no key or value
/// of it can be named ([`unread`]).
const UNREAD: &str = "the URL is not one its PostgreSQL client reads, or names a user or holds a password (or a password file) under a key it does not know: give the address alone, postgresql://host[:port][,host[:port]...]/database?key=value&... with keys the client knows (not `passfile`, `sslpassword`, `sslkey` or `sslcert`), and the login with --user and --password-stdin";

/// What a refusal of a URL holding a `#` ends with.
const HASH: &str = "; a `#` that is part of the address is written %23";

/// The refusal of a URL whose only fault is a `#`.
const HASH_ONLY: &str = "the URL holds a `#`; one that is part of the address is written %23";

/// The keys the client reads (tokio-postgres 0.7.18 `Config::param`).
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

/// libpq's `sslmode` values the client does not read: it reads `disable`,
/// `prefer` and `require`.
const TLS_MODES: [&str; 3] = ["allow", "verify-ca", "verify-full"];

/// libpq's TLS keys the client does not read: it takes no certificate,
/// key or revocation file.
const TLS_KEYS: [&str; 10] = [
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

/// The keys of a login, any of which followed by `=` anywhere in a URL's
/// percent-decoded text, in any case, makes it hold one ([`holds_login`]):
/// the client's `user` and `password` (tokio-postgres 0.7.18
/// `Config::param`), and libpq's keys of a password, a password file, a
/// client key or certificate and its password, an OAuth client secret, and
/// a SCRAM client or server key (libpq 18: `Database-User`,
/// `Database-Password`, `Database-Password-File`, `SSL-Client-Key`,
/// `SSL-Client-Cert`, `SSL-Client-Key-Password`, `OAuth-Client-Secret`,
/// `scram_client_key`, `scram_server_key`; a ClientKey alone authenticates
/// as the role). A fixed list, no guess at a grammar: `password=` in a
/// database name or a value is counted too.
const LOGIN_KEYS: [&str; 9] = [
    "user",
    "password",
    "passfile",
    "sslkey",
    "sslcert",
    "sslpassword",
    "oauth_client_secret",
    "scram_client_key",
    "scram_server_key",
];

/// libpq's keys of a login the client does not read, which a refusal of a
/// URL the client does not read names.
const UNREAD_LOGIN_KEYS: [&str; 2] = ["passfile", "sslpassword"];

/// What the refusal of `sslrootcert` adds: where a private CA is trusted
/// instead. The engine's `native-tls` uses OpenSSL on Linux, whose roots
/// `openssl-probe` reads from `SSL_CERT_FILE` (and `SSL_CERT_DIR`) in the
/// process that dials (a Sandbox's daemon starts with a cleared
/// environment, so it reads none), and the Security framework on macOS,
/// which reads no such variable and trusts the keychains' roots.
const PRIVATE_CA: &str = "; a private CA is trusted where the Store is dialled: on Linux (OpenSSL) in the system's trusted roots, or by naming its file in `SSL_CERT_FILE` for calls in that process only (a Sandbox's daemon starts with a cleared environment), on macOS by trusting it in the login or system keychain";

/// The refusal of a TLS key or `sslmode` value the client does not read,
/// `what` a name from the lists here. With the engine's connector
/// (`postgres-native-tls` over `native-tls`, built with its defaults),
/// `require` verifies the certificate chain and the host name.
fn tls(what: &str) -> String {
    format!("the URL's {what} is not read by its PostgreSQL client, which takes `sslmode` disable, prefer or require and no certificate, key or revocation file; `sslmode=require` verifies the server's certificate chain and host name against the trusted roots: give `sslmode=require` and keys the client knows")
}

/// `(scheme, after)` of a PostgreSQL URL: its scheme and the text after
/// `postgresql://` or `postgres://`.
pub fn scheme(url: &str) -> Result<(&str, &str)> {
    url.split_once("://")
        .filter(|(scheme, _)| matches!(*scheme, "postgresql" | "postgres"))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "a PostgreSQL URL starts postgresql:// or postgres://",
            )
        })
}

/// The client's reading of `url`, `None` when it refuses it. Only a
/// `postgres://` or `postgresql://` URL is read: the client would read any
/// other text as libpq's key=value words.
fn reading(url: &str) -> Option<Config> {
    scheme(url).ok()?;
    url.parse().ok()
}

/// The servers and the database a reading dials.
fn endpoints(read: &Config) -> (&[Host], &[IpAddr], &[u16], Option<&str>) {
    let (hosts, addrs) = (read.get_hosts(), read.get_hostaddrs());
    (hosts, addrs, read.get_ports(), read.get_dbname())
}

/// Whether a reading has a user or a password, or a host holding an `@`:
/// a login written percent-encoded (`alice%3Asecret%40db`) is read as part
/// of a host, which no real host name or socket directory holds, and would
/// otherwise be shown and resolved.
fn has_login(read: &Config) -> bool {
    let at = |host: &Host| match host {
        Host::Tcp(name) => name.contains('@'),
        #[cfg(unix)]
        Host::Unix(path) => path.as_os_str().as_encoded_bytes().contains(&b'@'),
    };
    read.get_user().is_some() || read.get_password().is_some() || read.get_hosts().iter().any(at)
}

/// `text` with each `%XX` decoded, as the client decodes a URL's parts;
/// any other `%` kept.
fn decoded(text: &str) -> Vec<u8> {
    let (bytes, mut out, mut at) = (text.as_bytes(), Vec::new(), 0);
    while at < bytes.len() {
        let hex = bytes.get(at + 1..at + 3).and_then(|hex| {
            let hex = std::str::from_utf8(hex).ok()?;
            u8::from_str_radix(hex, 16)
                .ok()
                .filter(|_| hex.bytes().all(|b| b.is_ascii_hexdigit()))
        });
        let (byte, size) = match (bytes[at], hex) {
            (b'%', Some(byte)) => (byte, 3),
            (byte, _) => (byte, 1),
        };
        out.push(byte);
        at += size;
    }
    out
}

/// Whether `url`, percent-decoded (which keeps every byte that is not part
/// of a `%XX`), holds a key of [`LOGIN_KEYS`] followed by `=`, in any case.
fn names_login_key(url: &str) -> bool {
    let text = decoded(url).to_ascii_lowercase();
    LOGIN_KEYS.iter().any(|key| {
        let key = format!("{key}=");
        text.windows(key.len()).any(|at| at == key.as_bytes())
    })
}

/// Whether `url` may hold a login, and so is never shown, dialled or
/// rewritten: it is no PostgreSQL URL; or it has an `@` after its scheme;
/// or the client does not read it; or the client reads a user, a password
/// or a host holding an `@` in it ([`has_login`]); or it holds a login key
/// and `=` ([`LOGIN_KEYS`]).
pub fn holds_login(url: &str) -> bool {
    let Ok((_, after)) = scheme(url) else {
        return true;
    };
    after.contains('@') || reading(url).is_none_or(|read| has_login(&read)) || names_login_key(url)
}

/// The query pairs of `query` as the client splits them: a key runs to its
/// `=`, its value to the next `&`.
fn pairs(mut query: &str) -> Vec<&str> {
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

/// The refusal of a URL the client does not read (`after`, its text after
/// the scheme, holding no `@`), named only in words of the lists here,
/// never in other text of the URL: a pair read alone as a login, or under a
/// login key, is refused as a login; else the first pair the client
/// refuses alone under a TLS key (`sslrootcert`), `sslmode` (by its value
/// when that is libpq's, `sslmode=verify-full`) or another key it reads
/// (the value of `port`) is named; a value that runs into a `#` the client
/// reads as part of it is named by that `#` when the value before it is
/// read; else [`UNREAD`]. Every such URL holds a login all the same
/// ([`holds_login`]): what the client does not read cannot be told apart.
fn unread(scheme: &str, after: &str) -> String {
    let query = after.split_once('?').map_or("", |(_, query)| query);
    let pairs = pairs(query);
    let alone = |pair: &str| reading(&format!("{scheme}://?{pair}"));
    let key = |pair: &'_ str| pair.split_once('=').map_or(pair, |(key, _)| key).to_owned();
    let as_login = |pair: &&str| {
        let key = key(pair);
        // `user` or `password` with a value the client cannot decode is a
        // login all the same
        let login_key = UNREAD_LOGIN_KEYS.contains(&key.as_str())
            || matches!(key.as_str(), "user" | "password");
        alone(pair).is_some_and(|read| has_login(&read)) || login_key
    };
    if let Some(pair) = pairs.iter().copied().find(as_login) {
        return match UNREAD_LOGIN_KEYS.iter().find(|login| **login == key(pair)) {
            Some(login) => format!("the URL names a user or holds a password (or a password file) in `{login}`, which its PostgreSQL client does not read; give the address alone and the login with --user and --password-stdin"),
            None => LOGIN.to_owned(),
        };
    }
    // a pair with no `=` is no key the client reads: it is not named
    let named = |pair: &str| {
        let (key, value) = pair.split_once('=')?;
        let known = KEYS.iter().find(|known| **known == key);
        // `sslmode=require#f`: the `#` is the fault, not the value given
        let before = value.split_once('#').map(|(before, _)| before);
        if let (Some(known), Some(before)) = (known, before) {
            if alone(&format!("{key}={before}")).is_some() {
                return Some(format!("the URL's value of `{known}` holds a `#`, which its PostgreSQL client reads as part of the value: end the value before the `#`"));
            }
        }
        let mode = TLS_MODES
            .iter()
            .find(|mode| **mode == before.unwrap_or(value));
        match (known, mode) {
            (Some(&"sslmode"), Some(mode)) => Some(tls(&format!("`sslmode={mode}`"))),
            (Some(&"sslmode"), None) => Some(tls("`sslmode` value")),
            (Some(known), _) => Some(format!("the URL's value of `{known}` is not one its PostgreSQL client reads: give the address with values the client reads")),
            (None, _) => TLS_KEYS.iter().find(|tls_key| **tls_key == key).map(|tls_key| match *tls_key {
                "sslrootcert" => format!("{}{PRIVATE_CA}", tls("`sslrootcert`")),
                _ => tls(&format!("`{tls_key}`")),
            }),
        }
    };
    let refused = pairs.iter().copied().filter(|pair| alone(pair).is_none());
    refused
        .filter_map(named)
        .next()
        .unwrap_or_else(|| UNREAD.to_owned())
}

/// Refuses a URL that is not an address alone, the one kind dialled and
/// recorded, or one that is not PostgreSQL's: one that may hold a login
/// ([`holds_login`]: an `@`, a user, password or host holding an `@` the
/// client reads, a login key, one the client does not read, [`unread`]),
/// or holding a `#` (the client reads it as text, other readers end the
/// URL there), which every refusal of a URL holding one also names. An `@`
/// or `#` of the address itself (in a database name or a value: no host
/// holds an `@`) is written `%40` or `%23`, which the client decodes, so
/// no reading of an accepted URL can differ from the client's. Text the
/// client reads as part of a host, database or value (`/db&sslmode=require`
/// is the database `db&sslmode=require`) is the client's to refuse.
pub fn refuse_login(url: &str) -> Result<()> {
    let (scheme, after) = scheme(url)?;
    let message = if after.contains('@') {
        let hint = "; an `@` or `#` that is part of the address is written %40 or %23";
        format!("{LOGIN}{hint}")
    } else {
        match reading(url) {
            Some(read) if has_login(&read) || names_login_key(url) => LOGIN.to_owned(),
            Some(_) if after.contains('#') => HASH_ONLY.to_owned(),
            Some(_) => return Ok(()),
            None => unread(scheme, after),
        }
    };
    let message = match after.contains('#') && !message.contains("%23") {
        true => format!("{message}{HASH}"),
        false => message,
    };
    Err(refused("InvalidInput", message))
}

/// `url` as a row, a refusal or an answer shows it: exactly as written, or
/// [`HIDDEN`] when it may hold a login ([`holds_login`]).
pub fn shown(url: &str) -> String {
    match holds_login(url) {
        true => HIDDEN.to_owned(),
        false => url.to_owned(),
    }
}

/// `(host, port, database)` the client reads in `url`, a URL holding no
/// login: its hosts (host addresses when it names none) and ports, each
/// list joined with `,` (port 5432 when none is given), a host name in
/// lower case; the database, empty when none is given. What a row's
/// `server` and a refusal name; a URL that may hold a login
/// ([`holds_login`]) is `InvalidInput`, naming none.
pub fn target(url: &str) -> Result<(String, String, String)> {
    let read = reading(url).filter(|_| !holds_login(url)).ok_or_else(|| {
        refused(
            "InvalidInput",
            "the URL has no form that can be shown (it may hold a login, or its client does not read it)",
        )
    })?;
    Ok(fields(&read))
}

/// [`target`]'s `(host, port, database)` of a reading.
fn fields(read: &Config) -> (String, String, String) {
    let mut hosts: Vec<String> = (read.get_hosts().iter())
        .map(|host| match host {
            Host::Tcp(name) => name.to_ascii_lowercase(),
            #[cfg(unix)]
            Host::Unix(path) => path.display().to_string(),
        })
        .collect();
    if hosts.is_empty() {
        hosts = read.get_hostaddrs().iter().map(IpAddr::to_string).collect();
    }
    let ports: Vec<String> = read.get_ports().iter().map(u16::to_string).collect();
    let port = match ports.is_empty() {
        true => "5432".to_owned(),
        false => ports.join(","),
    };
    let database = read.get_dbname().unwrap_or("").to_owned();
    (hosts.join(","), port, database)
}

/// Appends `text` percent-encoded to `out`, one byte at a time with no
/// temporary string: `out` is sized by the caller, so a password written
/// here leaves no copy behind in a grown buffer.
fn percent(out: &mut String, text: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => {
                out.push('%');
                out.push(HEX[usize::from(b >> 4)] as char);
                out.push(HEX[usize::from(b & 15)] as char);
            }
        }
    }
}

/// `url`, an address alone ([`refuse_login`]), with the login `user` and
/// `password` put in front of it, each percent-encoded. The URL made is
/// read back by the client and dialled only when it reads exactly that
/// user and password and the address's hosts, host addresses, ports and
/// database; otherwise it is refused, never dialled. It is never written
/// down.
pub fn with_login(url: &str, user: &str, password: &str) -> Result<String> {
    refuse_login(url)?;
    let (scheme, after) = scheme(url)?;
    // sized for the longest encoding, so it never grows while it holds the
    // password
    let capacity = scheme.len() + 5 + 3 * (user.len() + password.len()) + after.len();
    let mut dialled = String::with_capacity(capacity);
    dialled.push_str(scheme);
    dialled.push_str("://");
    percent(&mut dialled, user);
    dialled.push(':');
    percent(&mut dialled, password);
    dialled.push('@');
    dialled.push_str(after);
    match (reading(url), reading(&dialled)) {
        (Some(given), Some(read))
            if read.get_user() == Some(user)
                && read.get_password() == Some(password.as_bytes())
                && endpoints(&read) == endpoints(&given) =>
        {
            Ok(dialled)
        }
        _ => Err({
            layerfs_home::wipe::text(dialled);
            refused(
            "InvalidInput",
            "the saved login cannot be put into the URL so that its client reads it and the address as given; nothing was dialled",
            )
        }),
    }
}

//! A PostgreSQL Store's URL read only as its client reads it (tokio-postgres
//! 0.7, [`layerfs_persistence::PostgresConfig`], the reading every Store
//! session dials with), never by a parser of LayerFS's own: two rounds of
//! hand-written splitting each leaked a password or moved a Store (the
//! 3cf2cfc246 and aa83393447 reviews). A URL is dialled only when it is an
//! address alone ([`refuse_login`]), with the login put in and read back
//! ([`with_login`]); it is shown or written down only in a form the client
//! reads with the same servers and database, no login and no part of the
//! password ([`clean`]), and otherwise as [`HIDDEN`], never rewritten.
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;
use layerfs_persistence::{PostgresConfig as Config, PostgresHost as Host};
use std::net::IpAddr;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What is shown of a URL with no clean form ([`clean`]): none of its text.
pub const HIDDEN: &str = "postgresql://<hidden: not an address alone>";

/// The refusal of a URL holding a login (spec v23 V23-13).
const LOGIN: &str = "the URL names a user or holds a password (or a password file); give the address alone and the login with --auth (`layerfs store save-auth` saves one)";

/// The refusal of a URL the client does not read, when no key or value
/// of it can be named ([`unread`]).
const UNREAD: &str = "the URL is not one its PostgreSQL client reads, or names a user or holds a password (or a password file) under a key it does not know: give the address alone, postgresql://host[:port][,host[:port]...]/database?key=value&... with keys the client knows (not `passfile`, `sslpassword`, `sslkey` or `sslcert`), and the login with --auth (`layerfs store save-auth` saves one)";

/// What a refusal of a URL holding a `#` ends with.
const HASH: &str = "; a `#` that is part of the address is written %23";

/// The refusal of a URL whose only fault is a `#`.
const HASH_ONLY: &str = "the URL holds a `#`; one that is part of the address is written %23";

/// The refusal of a URL whose host, database or a value the client reads
/// holds `&` or `#` before a key of [`KEYS`] or [`LOGIN_KEYS`] ([`smuggled`]):
/// `postgres://h/db&password=p` is the database `db&password=p` to the
/// client, a `?` mistyped. Fixed words: no text of the URL is named.
const SMUGGLED: &str = "the URL's host, database or a value holds `&` or `#` (or %26 or %23) before a key its PostgreSQL client reads or a login key, and the client reads it as part of that text, not as a setting: give the address alone with its settings after a `?`, and the login with --auth (`layerfs store save-auth` saves one)";

/// The bytes that end a user, a host list or a path, or split pairs: a
/// user holding one is an address or a query the client read as a user.
const ADDRESS: [char; 5] = ['/', '?', '&', '=', '#'];

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

/// libpq's keys of a login the client does not read.
const LOGIN_KEYS: [&str; 2] = ["passfile", "sslpassword"];

/// Whether `key` names a login: `user`, `password` or one of [`LOGIN_KEYS`].
fn login_key(key: &str) -> bool {
    matches!(key, "user" | "password") || LOGIN_KEYS.contains(&key)
}

/// The keys of [`KEYS`] and [`LOGIN_KEYS`] that `text` holds right after a
/// `&` or a `#` and before an `=` (`db&password=`, in any case): a fixed
/// list, no guess at a grammar. `text` is what the client decoded, so
/// `%26` and `%23` count too.
fn smuggled(text: &[u8]) -> Vec<&'static str> {
    let found = |key: &&str| {
        let size = key.len() + 2;
        text.windows(size).any(|at| {
            matches!(at[0], b'&' | b'#')
                && at[size - 1] == b'='
                && at[1..size - 1].eq_ignore_ascii_case(key.as_bytes())
        })
    };
    KEYS.iter()
        .chain(LOGIN_KEYS.iter())
        .copied()
        .filter(found)
        .collect()
}

/// [`smuggled`] in the texts a reading holds: its hosts, its database and
/// the values it keeps as text (`options`, `application_name`); the other
/// values the client reads are numbers or words of its own lists.
fn smuggled_in(read: &Config) -> Vec<&'static str> {
    let hosts = read.get_hosts().iter().map(|host| match host {
        Host::Tcp(name) => name.clone(),
        #[cfg(unix)]
        Host::Unix(path) => path.to_string_lossy().into_owned(),
    });
    let texts = [
        read.get_dbname(),
        read.get_options(),
        read.get_application_name(),
    ];
    let texts = hosts.chain(texts.into_iter().flatten().map(str::to_owned));
    texts.flat_map(|text| smuggled(text.as_bytes())).collect()
}

/// What the refusal of `sslrootcert` adds: where a private CA is trusted
/// instead. The engine's `native-tls` uses OpenSSL on Linux, whose roots
/// `openssl-probe` reads from `SSL_CERT_FILE` (and `SSL_CERT_DIR`), and
/// the Security framework on macOS, which reads no such variable and
/// trusts the keychain's roots.
const PRIVATE_CA: &str = "; a private CA is trusted where the Store is dialled: on Linux (OpenSSL) by naming its file in `SSL_CERT_FILE`, on macOS by trusting it in the system keychain";

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

/// Whether a reading has a user or a password.
fn has_login(read: &Config) -> bool {
    read.get_user().is_some() || read.get_password().is_some()
}

/// Why a URL is not an address alone ([`refuse_login`]): the refusal's
/// text, and whether what it refuses may be a login (its fix then saves
/// one first).
struct Why {
    message: String,
    login: bool,
}

/// [`Why`] `url` is not an address alone, `None` when it is one: holding
/// an `@` (the client ends a user part at the first, wherever it is); one
/// the client does not read ([`unread`]); one whose client reads a user or
/// a password (a `user` or `password` parameter); one whose hosts, database
/// or a text value hold `&` or `#` before a key the client reads or a login
/// key ([`SMUGGLED`], a login when that key is one); holding a `#` (the
/// client reads it as text, other readers end the URL there), which every
/// other refusal of a URL holding one also names.
fn why(url: &str) -> Result<Option<Why>> {
    let (scheme, after) = scheme(url)?;
    let hash = |message: String| match after.contains('#') && !message.contains("%23") {
        true => format!("{message}{HASH}"),
        false => message,
    };
    let (message, login) = if after.contains('@') {
        let hint = "; an `@` or `#` that is part of the address is written %40 or %23";
        (format!("{LOGIN}{hint}"), true)
    } else if let Some(read) = reading(url) {
        let smuggled = smuggled_in(&read);
        match (has_login(&read), smuggled.is_empty(), after.contains('#')) {
            (true, _, _) => (LOGIN.to_owned(), true),
            (false, false, _) => (SMUGGLED.to_owned(), smuggled.into_iter().any(login_key)),
            (false, true, true) => (HASH_ONLY.to_owned(), false),
            (false, true, false) => return Ok(None),
        }
    } else {
        unread(scheme, after)
    };
    let message = hash(message);
    Ok(Some(Why { message, login }))
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
/// the scheme, holding no `@`), and whether it may hold a login. What the
/// client refuses is named only in words of the lists here, never in other
/// text of the URL: a pair read alone as a login, or under a login key
/// (`passfile`, or `user` or `password` with a value it cannot decode), is
/// refused as a login; else the first pair the client
/// refuses alone under a TLS key (`sslrootcert`), `sslmode` (by its value
/// when that is libpq's, `sslmode=verify-full`) or another key it reads
/// (the value of `port`) is named; a value that runs into a `#` the client
/// reads as part of it is named by that `#` when the value before it is
/// read. A pair with no `=` or under a key no list names may hold a login,
/// and so may a URL whose pairs all read alone (the hosts or the path are
/// refused): [`UNREAD`]. A named refusal may hold a login too when the
/// client does not read the URL's hosts and path alone (its user part,
/// `postgres://alice:secret/db`, an `@host` left out), or when they or a
/// value hold `&` or `#` before a login key ([`smuggled`]).
fn unread(scheme: &str, after: &str) -> (String, bool) {
    let (base, query) = after.split_once('?').unwrap_or((after, ""));
    let pairs = pairs(query);
    let alone = |pair: &str| reading(&format!("{scheme}://?{pair}"));
    let key = |pair: &'_ str| pair.split_once('=').map_or(pair, |(key, _)| key).to_owned();
    let as_login = |pair: &&str| {
        let key = key(pair);
        // `user` or `password` with a value the client cannot decode is a
        // login all the same
        let login_key =
            LOGIN_KEYS.contains(&key.as_str()) || matches!(key.as_str(), "user" | "password");
        alone(pair).is_some_and(|read| has_login(&read)) || login_key
    };
    if let Some(pair) = pairs.iter().copied().find(as_login) {
        return match LOGIN_KEYS.iter().find(|login| **login == key(pair)) {
            Some(login) => (format!("the URL names a user or holds a password (or a password file) in `{login}`, which its PostgreSQL client does not read; give the address alone and the login with --auth (`layerfs store save-auth` saves one)"), true),
            None => (LOGIN.to_owned(), true),
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
    let refused: Vec<&str> = pairs
        .iter()
        .copied()
        .filter(|pair| alone(pair).is_none())
        .collect();
    let values = pairs
        .iter()
        .filter_map(|pair| pair.split_once('='))
        .map(|(_, v)| v);
    let hidden = reading(&format!("{scheme}://{base}")).is_none()
        || (std::iter::once(base).chain(values))
            .any(|text| smuggled(&decoded(text)).into_iter().any(login_key));
    match refused.iter().find_map(|pair| named(pair)) {
        Some(message) => {
            let login = hidden || refused.iter().any(|pair| named(pair).is_none());
            (message, login)
        }
        None => (UNREAD.to_owned(), true),
    }
}

/// Refuses a URL that is not an address alone, the one kind dialled and
/// recorded ([`why`]), or one that is not PostgreSQL's. An `@` or `#` of
/// the address itself is written `%40` or `%23`, which the client decodes,
/// so no reading of an accepted URL can differ from the client's.
pub fn refuse_login(url: &str) -> Result<()> {
    match why(url)? {
        Some(why) => Err(refused("InvalidInput", why.message)),
        None => Ok(()),
    }
}

/// Whether [`refuse_login`] refuses `url` for what may be a login (an `@`,
/// a user or a password the client reads, a login key, a key it does not
/// know): its fix then saves a login first. One refused only for a `#` or
/// a setting the client does not read is not.
pub fn holds_login(url: &str) -> bool {
    matches!(why(url), Ok(Some(Why { login: true, .. })))
}

/// `rest`, a URL after its client's user part, without the query pairs
/// the client reads as a login, each pair asked of the client alone (one
/// holding an `@` is kept, and [`clean`] then refuses the form); the
/// rest of the text as it was. The query starts at the first `?`, where the
/// client ends a host list and a path ([`pairs`]).
fn without_login_pairs(scheme: &str, rest: &str) -> String {
    let Some((base, query)) = rest.split_once('?') else {
        return rest.to_owned();
    };
    let pairs = pairs(query);
    // a pair holding an `@` is kept: alone, the client would read a user
    // part in it, and in the form it leaves that form refused
    let login = |pair: &&str| {
        !pair.contains('@')
            && reading(&format!("{scheme}://?{pair}")).is_some_and(|r| has_login(&r))
    };
    let kept: Vec<&str> = pairs.iter().copied().filter(|p| !login(p)).collect();
    match (kept.len() == pairs.len(), kept.is_empty()) {
        (true, _) => rest.to_owned(),
        (false, true) => base.to_owned(),
        (false, false) => format!("{base}?{}", kept.join("&")),
    }
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

/// Whether `text` holds a piece of `secret`: any run of three of its bytes,
/// or all of a shorter one.
fn holds(secret: &[u8], text: &[u8]) -> bool {
    let size = secret.len().min(3);
    size > 0 && (secret.windows(size)).any(|piece| text.windows(size).any(|t| t == piece))
}

/// `url` as it may be shown or written down, with the client's reading of
/// that form: the URL after its client's user part, without the query pairs
/// the client reads as a login, each `#` written `%23`. It is kept only when
/// the client reads it with no user and no password and the same hosts,
/// host addresses, ports and database as the original, and no piece of the
/// original's password (as the client reads it, [`holds`]) is in it,
/// encoded or decoded, in what [`target`] shows of it, or in the original's
/// user. `None` (show [`HIDDEN`], write nothing) when the client does not
/// read the original; when its user part holds an address or a query, so
/// the login cannot be told from the address: one of `? & # ; ,` anywhere
/// in it, where a `?` or one written in its place runs the address into
/// the user part (`postgres://h/db?password=p@ss` has the user
/// `h/db?password=p` and the host `ss` to the client;
/// `postgres://h:5432&password=p@ss` the user `h` and the password
/// `5432&password=p`; `;` and `,` the same) or a user holding one of
/// [`ADDRESS`] (`postgres://h/db=p@ss`); when its hosts, database or a
/// text value hold `&` or `#` before a key ([`smuggled`],
/// `postgres://h/db&password=p`); or when the form fails a check (an `@`
/// left, which the client would read as a user part, so a URL holding two
/// is never rewritten to another host).
pub fn clean(url: &str) -> Option<(String, Config)> {
    let (scheme, after) = scheme(url).ok()?;
    let original = reading(url)?;
    let (userinfo, rest) = after.split_once('@').unwrap_or(("", after));
    let named = userinfo.split(':').next().unwrap_or_default();
    let address = |user: &[u8]| user.iter().any(|b| ADDRESS.contains(&char::from(*b)));
    let query = userinfo.contains(['?', '&', '#', ';', ',']);
    if query || address(&decoded(named)) || !smuggled_in(&original).is_empty() {
        return None;
    }
    let rest = without_login_pairs(scheme, rest).replace('#', "%23");
    let shown = format!("{scheme}://{rest}");
    let read = reading(&shown)?;
    let password = original.get_password().unwrap_or_default();
    let (hosts, _, database) = fields(&read);
    let user = original.get_user().unwrap_or_default();
    let texts = [
        rest.as_bytes().to_vec(),
        decoded(&rest),
        hosts.clone().into_bytes(),
        percent(&hosts).into_bytes(),
        database.clone().into_bytes(),
        percent(&database).into_bytes(),
        user.as_bytes().to_vec(),
        named.as_bytes().to_vec(),
    ];
    let leaks = texts.iter().any(|text| holds(password, text));
    let same = !has_login(&read) && endpoints(&read) == endpoints(&original);
    (same && !leaks).then_some((shown, read))
}

/// `url` as a row, a fix or a record write shows it: its [`clean`] form,
/// else [`HIDDEN`]. A form that is shown is clean again as it is.
pub fn without_login(url: &str) -> String {
    clean(url).map_or_else(|| HIDDEN.to_owned(), |(shown, _)| shown)
}

/// The user the client reads in `url` (a user part or a `user`
/// parameter), when the URL has a clean form and the user is not empty and
/// holds none of [`ADDRESS`] (it would be an address or a query the client
/// read as a user): a fix's `--user`. A 2.0 record's URL names none; one
/// recorded before saved logins may.
pub fn url_user(url: &str) -> Option<String> {
    clean(url)?;
    let user = reading(url)?.get_user()?.to_owned();
    (!user.is_empty() && !user.contains(ADDRESS)).then_some(user)
}

/// `(host, port, database)` the client reads in `url`'s clean form: its
/// hosts (host addresses when it names none) and ports, each list joined
/// with `,` (port 5432 when none is given), a host name in lower case; the
/// database, empty when none is given. What a row's `server` and a
/// refusal name; a URL with no clean form is `InvalidInput`, naming none.
pub fn target(url: &str) -> Result<(String, String, String)> {
    let (_, read) = clean(url).ok_or_else(|| {
        refused(
            "InvalidInput",
            "the URL has no form that can be shown (it holds a login, or its client does not read it)",
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

fn percent(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
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
    let dialled = format!("{scheme}://{}:{}@{after}", percent(user), percent(password));
    match (reading(url), reading(&dialled)) {
        (Some(given), Some(read))
            if read.get_user() == Some(user)
                && read.get_password() == Some(password.as_bytes())
                && endpoints(&read) == endpoints(&given) =>
        {
            Ok(dialled)
        }
        _ => Err(refused(
            "InvalidInput",
            "the saved login cannot be put into the URL so that its client reads it and the address as given; nothing was dialled",
        )),
    }
}

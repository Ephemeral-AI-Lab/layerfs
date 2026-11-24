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

/// The refusal of a URL the client does not read.
const UNREAD: &str = "the URL is not one its PostgreSQL client reads, or names a user or holds a password (or a password file) under a key it does not know: give the address alone, postgresql://host[:port][,host[:port]...]/database?key=value&... with keys the client knows (not `passfile`, `sslpassword`, `sslkey` or `sslcert`), and the login with --auth (`layerfs store save-auth` saves one)";

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

/// Refuses a URL that is not an address alone, the one kind dialled and
/// recorded: one that is not PostgreSQL's; holding an `@` (the client ends
/// a user part at the first, wherever it is); one the client does not
/// read; one whose client reads a user or a password (a `user` or
/// `password` parameter); holding a `#` (the client reads it as text, other
/// readers end the URL there). An `@` or `#` of the address itself is
/// written `%40` or `%23`, which the client decodes, so no reading of an
/// accepted URL can differ from the client's.
pub fn refuse_login(url: &str) -> Result<()> {
    let (_, after) = scheme(url)?;
    if after.contains('@') {
        return Err(refused(
            "InvalidInput",
            format!("{LOGIN}; an `@` or `#` that is part of the address is written %40 or %23"),
        ));
    }
    let read = reading(url).ok_or_else(|| refused("InvalidInput", UNREAD))?;
    if has_login(&read) {
        return Err(refused("InvalidInput", LOGIN));
    }
    if after.contains('#') {
        return Err(refused(
            "InvalidInput",
            "the URL holds a `#`; one that is part of the address is written %23",
        ));
    }
    Ok(())
}

/// `rest`, a URL after its client's user part, without the query pairs
/// the client reads as a login, each pair asked of the client alone (one
/// holding an `@` is kept, and [`clean`] then refuses the form); the
/// rest of the text as it was. The query starts at the first `?`, where the
/// client ends a host list and a path; a key runs to its `=`, its value to
/// the next `&`.
fn without_login_pairs(scheme: &str, rest: &str) -> String {
    let Some((base, mut query)) = rest.split_once('?') else {
        return rest.to_owned();
    };
    let mut pairs = Vec::new();
    while !query.is_empty() {
        let end = (query.find('='))
            .and_then(|eq| query[eq..].find('&').map(|amp| eq + amp))
            .unwrap_or(query.len());
        pairs.push(&query[..end]);
        query = query.get(end + 1..).unwrap_or("");
    }
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

/// `url` as it may be shown or written down, with the client's reading of
/// that form: the URL after its client's user part, without the query pairs
/// the client reads as a login, each `#` written `%23`. It is kept only when
/// the client reads it with no user and no password and the same hosts,
/// host addresses, ports and database as the original, and no three bytes
/// of the original's password (as the client reads it) are in it or in the
/// original's user. `None` (show [`HIDDEN`], write nothing) when the client
/// does not read the original, when its user part holds a `?` (a query, so
/// the login cannot be told from the address: `postgres://h/db?password=
/// p@ss` has the user `h/db?password=p` and the host `ss` to the client),
/// or when the form fails a check (an `@` left, which the client would read
/// as a user part, so a URL holding two is never rewritten to another host).
pub fn clean(url: &str) -> Option<(String, Config)> {
    let (scheme, after) = scheme(url).ok()?;
    let original = reading(url)?;
    let (userinfo, rest) = after.split_once('@').unwrap_or(("", after));
    if userinfo.contains('?') {
        return None;
    }
    let rest = without_login_pairs(scheme, rest).replace('#', "%23");
    let shown = format!("{scheme}://{rest}");
    let read = reading(&shown)?;
    let password = original.get_password().unwrap_or_default();
    let leaks = |text: &[u8]| {
        password
            .windows(3)
            .any(|part| text.windows(3).any(|t| t == part))
    };
    let user = original.get_user().unwrap_or_default().as_bytes();
    let same = !has_login(&read) && endpoints(&read) == endpoints(&original);
    (same && !leaks(rest.as_bytes()) && !leaks(user)).then_some((shown, read))
}

/// `url` as a row, a fix or a record write shows it: its [`clean`] form,
/// else [`HIDDEN`]. A form that is shown is clean again as it is.
pub fn without_login(url: &str) -> String {
    clean(url).map_or_else(|| HIDDEN.to_owned(), |(shown, _)| shown)
}

/// The user the client reads in `url` (a user part or a `user`
/// parameter), when the URL has a clean form and the user is not empty: a
/// fix's `--user`. A 2.0 record's URL names none; one recorded before saved
/// logins may.
pub fn url_user(url: &str) -> Option<String> {
    clean(url)?;
    let user = reading(url)?.get_user()?.to_owned();
    (!user.is_empty()).then_some(user)
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
    Ok((hosts.join(","), port, database))
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

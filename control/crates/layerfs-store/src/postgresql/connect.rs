//! PostgreSQL Stores (#533 step 10a, call surface 2.0 `pg.html`): `store
//! connect` records a database as a Store in the fixed `layerfs` schema,
//! or sets the address every machine dials; `store disconnect` forgets it
//! (its data stays). The record keeps the URL with no user and no password
//! and names its saved login (`auth`, [`super::login`]), put into the URL on
//! each call; the full URL is never written down. A machine's own address
//! is `layerfs-control`'s part of the call, proven from that machine.
use super::login;
use crate::open_args::{OpenArgs, PgSelection};
use crate::project::listing;
use crate::project::record::ProjectRecord;
use crate::record::url;
use crate::record::{StoreKind, StoreRecord};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_home::home::{self, paths};
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{failed, refused, shell_word, with_fix, Outcome};
use layerfs_home::secrets::parts;
use std::ops::Range;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The one schema a 2.0 PostgreSQL Store lives in.
pub const SCHEMA: &str = "layerfs";

/// What a call on Store `record` dials: its URL with its saved login.
pub fn selection(record: &StoreRecord) -> Result<PgSelection> {
    Ok(selection_at(record, url(record)?))
}

/// [`selection`] dialling `url`: an address of the record with its login
/// in it, as a machine reaches it (#535 §7.4).
pub fn selection_at(record: &StoreRecord, url: String) -> PgSelection {
    PgSelection {
        url: Some(url),
        schema: Some(SCHEMA.to_owned()),
        synchronous_commit: Some(
            record
                .text("synchronous_commit")
                .unwrap_or(crate::postgresql::open::DEFAULT_SYNCHRONOUS_COMMIT)
                .to_owned(),
        ),
    }
}

/// `store.connect(store, url, { auth, synchronous_commit })` without a
/// machine: a Store's first connect, or a new address every machine dials.
#[derive(Clone, Debug)]
pub struct StoreConnect {
    /// The Store's name.
    pub store: StoreName,
    /// The database's URL, with no user and no password.
    pub url: String,
    /// The saved login; required on the first connect, refused afterwards.
    pub auth: Option<String>,
    /// `synchronous_commit`, `local` by default; first connect only.
    pub synchronous_commit: Option<String>,
}

/// `text` with its `%XX` escapes decoded; a malformed escape stays as is.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let pair = bytes
            .get(i + 1..i + 3)
            .and_then(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok());
        match (bytes[i], pair) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `(name, value)` of each query parameter, both decoded.
fn query(rest: &str) -> Vec<(String, String)> {
    let Some((_, query)) = rest.split_once('?') else {
        return Vec::new();
    };
    let query = query.split('#').next().unwrap_or("");
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(name), decode(value))
        })
        .collect()
}

/// Refuses a URL that names a user or holds a password, in its user part
/// or as a `user` or any `password` (`sslpassword`) parameter: the login
/// is a saved one, named with `--auth`. Any `@` is a user part to the
/// daemon's client (its first one ends it, [`parts`]), so a URL holding
/// one is refused whichever reading finds the user part, as is a login
/// parameter of the query either reading finds ([`Shown`]).
pub fn refuse_login(url: &str) -> Result<()> {
    let shown = Shown::of(url)?;
    if shown.after.contains('@') || !shown.pairs.is_empty() {
        return Err(refused(
            "InvalidInput",
            "the URL names a user or holds a password (or a password file); give the address alone and the login with --auth (`layerfs store save-auth` saves one)",
        ));
    }
    Ok(())
}

/// [`refuse_login`]'s refusal for `store connect <store> <url>`, with its
/// fix (spec v23 V23-13): the login saved by name, then the address alone.
/// A first connect names the saved login with `--auth`; a connected Store
/// takes no `--auth` on connect, so its fix names the login with `store
/// update --auth` before the address alone (for `machine`'s own connection
/// when given). The URL's user fills `--user` when it names one.
pub fn refuse_login_with_fix(
    store: &str,
    url: &str,
    first: bool,
    machine: Option<&str>,
) -> Result<()> {
    // a URL that is not a PostgreSQL one is refused without a fix: its login
    // cannot be told apart, so no line may repeat it
    parts(url)?;
    refuse_login(url).map_err(|refusal| {
        let address = shell_word(&without_login(url));
        let user = url_user(url).map_or("<user>".to_owned(), |user| shell_word(&user));
        let store = shell_word(store);
        let save = format!("layerfs store save-auth <auth> --user {user} --password-stdin");
        let fix = match (first, machine) {
            (true, _) => format!("{save} && layerfs store connect {store} {address} --auth <auth>"),
            (false, machine) => {
                let on = machine.map_or(String::new(), |m| format!(" --machine {}", shell_word(m)));
                format!("{save} && layerfs store update {store} --auth <auth> && layerfs store connect {store} {address}{on}")
            }
        };
        with_fix(refusal, fix)
    })
}

/// Whether query parameter `name` carries a login: `user`, any
/// `password` (`sslpassword`), or `passfile` (spec v23 V23-13). A client
/// certificate's `sslkey` and `sslcert` name files, not secrets, and pass.
fn login_parameter(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "user" || name == "passfile" || name.contains("password")
}

/// The byte ranges of `after` (a URL after its `scheme://`) holding a
/// pair of the query of `after[start..]` that carries a login
/// ([`login_parameter`]). The query is read as the daemon's client reads
/// it: from the first `?` to the end, its pairs split at `&`; a `#` ends
/// no value.
fn login_pairs(after: &str, start: usize) -> Vec<Range<usize>> {
    let Some(mark) = after[start..].find('?') else {
        return Vec::new();
    };
    let mut pairs = Vec::new();
    let mut at = start + mark + 1;
    for pair in after[at..].split('&') {
        let name = pair.split_once('=').map_or(pair, |(name, _)| name);
        if !pair.is_empty() && login_parameter(&decode(name)) {
            pairs.push(at..at + pair.len());
        }
        at += pair.len() + 1;
    }
    pairs
}

/// A PostgreSQL URL read for showing or writing down (M1 of the
/// 3cf2cfc246 review). Its user part has two readings: the daemon's
/// client's, to the first `@` anywhere ([`parts`], what a login is put
/// into), and the URL's own authority's, to the last `@` before the first
/// `/`. What is shown or kept drops the widest user part either reading
/// finds and every login pair the query with no user part, or after
/// either reading's, holds; an `@` inside such a pair ends no user part
/// (the pair is dropped whole, so no part of its value is left as a host).
struct Shown<'a> {
    scheme: &'a str,
    /// The URL after `scheme://`.
    after: &'a str,
    /// Where the widest user part ends: its `@`.
    user_end: Option<usize>,
    /// Every login pair either reading finds, as byte ranges of `after`.
    pairs: Vec<Range<usize>>,
}

impl<'a> Shown<'a> {
    fn of(url: &'a str) -> Result<Self> {
        let (scheme, _, _) = parts(url)?;
        let after = &url[scheme.len() + "://".len()..];
        let first = after.find('@');
        let own = after[..after.find('/').unwrap_or(after.len())].rfind('@');
        let mut pairs = login_pairs(after, 0);
        for at in [first, own].into_iter().flatten() {
            pairs.extend(login_pairs(after, at + 1));
        }
        pairs.sort_by_key(|pair| (pair.start, pair.end));
        pairs.dedup();
        let user_end = ([first, own].into_iter().flatten())
            .filter(|at| !pairs.iter().any(|pair| pair.contains(at)))
            .max();
        Ok(Self {
            scheme,
            after,
            user_end,
            pairs,
        })
    }

    /// Whether `range` of `after` shares a byte with a login pair other
    /// than itself.
    fn overlaps(&self, range: &Range<usize>) -> bool {
        (self.pairs.iter())
            .any(|pair| pair != range && pair.start < range.end && range.start < pair.end)
    }

    /// The URL after its user part, with no login pair and no empty pair;
    /// a fragment before the query is dropped with the query.
    fn rest(&self) -> String {
        let mut at = self.user_end.map_or(0, |end| end + 1);
        let mut kept = String::new();
        for pair in &self.pairs {
            if pair.start > at {
                kept.push_str(&self.after[at..pair.start]);
            }
            at = at.max(pair.end);
        }
        kept.push_str(&self.after[at..]);
        let Some((path, query)) = kept.split_once('?') else {
            return kept;
        };
        // a fragment holding the query (`#x?…`) goes with it, as its
        // parameters may have held a login
        if let Some((path, _)) = path.split_once('#') {
            return path.to_owned();
        }
        let query: Vec<&str> = query.split('&').filter(|pair| !pair.is_empty()).collect();
        match query.is_empty() {
            true => path.to_owned(),
            false => format!("{path}?{}", query.join("&")),
        }
    }

    /// The user the URL names, decoded: its widest user part's (before
    /// any password) unless that holds part of a login pair, else a `user`
    /// pair that shares nothing with another login pair.
    fn user(&self) -> Option<String> {
        let user = match self.user_end {
            Some(end) => {
                let user = self.after[..end].split(':').next().unwrap_or("");
                (!self.overlaps(&(0..user.len()))).then(|| decode(user))
            }
            None => None,
        };
        let user = user.or_else(|| {
            (self.pairs.iter())
                .filter(|pair| !self.overlaps(pair))
                .find_map(|pair| {
                    let (name, value) = self.after[pair.clone()].split_once('=')?;
                    decode(name)
                        .eq_ignore_ascii_case("user")
                        .then(|| decode(value))
                })
        });
        user.filter(|user| !user.is_empty())
    }
}

/// `url` without a login: the widest user part dropped, and every query
/// parameter that carries one ([`refuse_login`]'s), as [`Shown`] reads
/// them, until no reading finds one. What a row shows and what a
/// PostgreSQL record write keeps of an old URL (V23-6); a URL that is not
/// a PostgreSQL one is kept as it is.
pub fn without_login(url: &str) -> String {
    let mut clean = url.to_owned();
    // each pass only drops text; one whose result still holds an `@` or a
    // login pair drops more, so what is left is refused by no reading
    while let Ok(shown) = Shown::of(&clean) {
        let next = format!("{}://{}", shown.scheme, shown.rest());
        if next == clean {
            break;
        }
        clean = next;
    }
    clean
}

/// The user a URL names, decoded ([`Shown::user`]): its user part's, or
/// else its `user` parameter's. A 2.0 record's URL names none; one recorded
/// before saved logins may, and its no-login fix names that user. No part
/// of a password is ever answered as the user.
pub fn url_user(url: &str) -> Option<String> {
    Shown::of(url).ok()?.user()
}

/// The server and database a URL names, as libpq reads it: `(host, port,
/// database)` from the authority and path, overridden by `host`, `port`
/// and `dbname` parameters; port 5432 by default; the host in lower case.
/// What a refusal names, never the URL: read from the URL
/// [`without_login`] leaves, so no part of a login is named.
pub fn target(url: &str) -> Result<(String, String, String)> {
    parts(url)?;
    let clean = without_login(url);
    let (_, _, rest) = parts(&clean)?;
    let path = rest.split(['?', '#']).next().unwrap_or("");
    let (authority, database) = path.split_once('/').unwrap_or((path, ""));
    let (mut host, mut port) = match authority.strip_prefix('[') {
        Some(v6) => {
            let (host, after) = v6.split_once(']').unwrap_or((v6, ""));
            (host.to_owned(), after.trim_start_matches(':').to_owned())
        }
        None if authority.contains(',') => (authority.to_owned(), String::new()),
        None => {
            let (host, port) = authority.rsplit_once(':').unwrap_or((authority, ""));
            (host.to_owned(), port.to_owned())
        }
    };
    let mut database = decode(database);
    for (name, value) in query(rest) {
        match name.as_str() {
            "host" => host = value,
            "port" => port = value,
            "dbname" => database = value,
            _ => {}
        }
    }
    if port.is_empty() {
        port = "5432".to_owned();
    }
    Ok((decode(&host).to_ascii_lowercase(), port, database))
}

/// Refuses a `synchronous_commit` that is not one of PostgreSQL's five.
pub fn synchronous_commit(sync: &str) -> Result<()> {
    if matches!(
        sync,
        "off" | "local" | "remote_write" | "on" | "remote_apply"
    ) {
        return Ok(());
    }
    Err(refused(
        "InvalidInput",
        format!("synchronous_commit is off, local, remote_write, on or remote_apply, not {sync}"),
    ))
}

/// Refuses `Exists` when a PostgreSQL Store of this home other than `name`
/// holds the database whose id is `binding`: a Store is its data, never
/// its address.
pub fn unclaimed_by_others(home: &Path, name: &str, binding: &[u8]) -> Result<()> {
    for record in crate::record::stores(home)? {
        if record.name() == name || record.kind()? != StoreKind::Postgresql {
            continue;
        }
        if record.binding() == Some(binding) {
            return Err(refused(
                "Exists",
                format!(
                    "Store {} connects the same database (the id in its schema {SCHEMA} is the same)",
                    record.name()
                ),
            ));
        }
    }
    Ok(())
}

/// Every side that connects to PostgreSQL Store `record` (spec v23
/// V23-3), as answers show them: this computer first (`machine` null, the
/// Store's address), then by name each machine with an address of its own,
/// a reach seen from it ([`crate::observed`]) or a Sandbox of the Store
/// (`sandboxes`, the machines the caller found). `native` without an
/// address of its own dials as this computer does and is its row. Each row
/// is `{ machine, url, own, server, reached, reach }`: the URL it dials
/// (no login), whether it is its own, the `{ host, port, database }` it
/// names, and the last reach seen with its word (`ok`, `failed`, null).
pub fn connections(record: &StoreRecord, sandboxes: &[String]) -> SdkValue {
    let seen = (record.home())
        .map(|home| crate::observed::read(home, record.name()))
        .unwrap_or_default();
    let own = record.own_connections();
    let own_url = |machine: &str| {
        let mut found = own.iter();
        found
            .find(|(m, _)| m == machine)
            .and_then(|(_, own)| own.url.clone())
    };
    let mut machines: Vec<String> = (own.iter().map(|(m, _)| m.clone()))
        .chain(seen.machines.iter().map(|(m, _)| m.clone()))
        .chain(sandboxes.iter().cloned())
        .filter(|m| m != "native" || own_url(m).is_some())
        .collect();
    machines.sort();
    machines.dedup();
    let store_url = record.text("url").map(str::to_owned);
    let mut rows = vec![row(None, store_url.clone(), false, seen.here.clone())];
    for machine in machines {
        let mine = own_url(&machine);
        let reach = seen.of(&machine);
        let own = mine.is_some();
        rows.push(row(Some(machine), mine.or(store_url.clone()), own, reach));
    }
    SdkValue::List(rows)
}

/// One `connections` row.
fn row(
    machine: Option<String>,
    url: Option<String>,
    own: bool,
    reach: Option<SdkValue>,
) -> SdkValue {
    let server = (url.as_deref()).and_then(|url| target(url).ok()).map_or(
        SdkValue::Null,
        |(host, port, database)| {
            let port = port
                .parse::<i64>()
                .map_or(SdkValue::text(&port), SdkValue::Int);
            SdkValue::map([
                ("host", SdkValue::text(host)),
                ("port", port),
                ("database", SdkValue::text(database)),
            ])
        },
    );
    SdkValue::map([
        ("machine", SdkValue::opt_text(machine)),
        // an outdated record may still hold a login in its URL: a row never shows it (V23-3)
        ("url", SdkValue::opt_text(url.as_deref().map(without_login))),
        ("own", SdkValue::Bool(own)),
        ("server", server),
        ("reached", crate::observed::reached(reach.as_ref())),
        ("reach", reach.unwrap_or_default()),
    ])
}

/// `store.connect`'s answer for `record`: `outcome` (`connected` or
/// `updated`), the projects found on a first connect (the first 20 names,
/// and the count only when there were more), the login, the addresses and the setting.
pub fn answer(
    record: &StoreRecord,
    outcome: &str,
    projects: Option<&[String]>,
    sandboxes: &[String],
) -> SdkValue {
    let (names, count) = match projects {
        Some(found) => layerfs_home::helpers::shown_names(found),
        None => (SdkValue::Null, SdkValue::Null),
    };
    let text = |key: &str| record.text(key).map_or(SdkValue::Null, SdkValue::text);
    SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("projects", names),
        ("projects_count", count),
        ("outcome", SdkValue::text(outcome)),
        ("store", SdkValue::text(record.name())),
        ("engine", SdkValue::text("postgresql")),
        ("auth", text("auth")),
        ("connections", connections(record, sandboxes)),
        ("synchronous_commit", text("synchronous_commit")),
    ])
}

/// `store.connect` of a new Store: records a PostgreSQL database as Store
/// `request.store`, its login `auth` required and saved. Every refusal of
/// its input comes before any effect. The connection is made from this
/// process, which gives an empty database the schema and a new id or reads
/// the id the schema holds, and lists its projects, before the home lock;
/// the lock covers the re-checks and the record writes only. A database
/// whose id another Store of this home holds is `Exists`.
pub fn connect(home_dir: &Path, request: &StoreConnect) -> Outcome {
    let name = request.store.as_str();
    let given = request.url.as_str();
    refuse_login_with_fix(name, given, true, None)?;
    target(given)?;
    let auth = request.auth.as_deref().ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("connecting Store {name} needs its login: --auth <auth> (`layerfs store save-auth` saves one)"),
        )
    })?;
    login::saved(home_dir, auth)?;
    let sync = request.synchronous_commit.as_deref().unwrap_or("local");
    synchronous_commit(sync)?;
    if paths::store_record(home_dir, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    let record = StoreRecord::connected(name, given, SCHEMA, sync, Some(auth)).in_home(home_dir);
    let probe = selection(&record)?;
    // an empty database gets the schema, with a new binding (#533 step
    // 10b); an existing LayerFS schema keeps its binding, read once. The
    // cursor key is this home's own either way.
    let fresh = crate::identity::Identity {
        kind: crate::identity::Kind::Postgresql {
            schema: SCHEMA.to_owned(),
            synchronous_commit: sync.to_owned(),
        },
        binding: layerfs_home::helpers::random()?.to_vec(),
        cursor_key: layerfs_home::helpers::random()?,
    };
    let binding = if crate::project::postgres_init::create_schema(&probe, &fresh)? {
        fresh.binding
    } else {
        crate::postgresql::open::read_binding(&probe)?
    };
    unclaimed_by_others(home_dir, name, &binding)?;
    let record = record.with("binding", SdkValue::Bytes(binding.clone()));
    let record = record.with("cursor", SdkValue::Bytes(fresh.cursor_key.to_vec()));
    let directory = paths::store_dir(home_dir, name);
    // the projects the schema holds become this home's project records; a
    // schema that does not open is not connected
    let open = OpenArgs::dialled(home_dir, name, &record, probe);
    let listed = crate::service::with_service(&open, |service| listing::listed(service, false))?;
    let _lock = home::lock::take(home_dir)?;
    if paths::store_record(home_dir, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    unclaimed_by_others(home_dir, name, &binding)?;
    login::saved(home_dir, auth)?;
    home::private_dir(&directory)?;
    record.write(home_dir)?;
    let mut projects = Vec::new();
    for project in listed {
        let stack = SdkValue::text(hex(&project.stack.to_bytes()));
        ProjectRecord::found(name, &project.name, stack).write(home_dir)?;
        projects.push(project.name);
    }
    Ok(answer(&record, "connected", Some(&projects), &[]))
}

/// `store.connect` of a connected Store without a machine: `request.url`
/// becomes the address every machine dials, proven from this process (it
/// must read this Store's id: `Unavailable` otherwise). `--auth` and
/// `--synchronous-commit` are refused (`store update` changes both). The
/// record is written under the home lock and the Store's, `Conflict` when
/// it was disconnected or connected to another database meanwhile; this
/// process's open sessions on it are closed. A running Sandbox keeps the
/// connection it started with.
pub fn readdress(home_dir: &Path, request: &StoreConnect, sandboxes: &[String]) -> Outcome {
    let name = request.store.as_str();
    let record = super::check::postgresql_record(home_dir, name)?;
    refuse_settings(name, request)?;
    refuse_login_with_fix(name, &request.url, false, None)?;
    target(&request.url)?;
    record.unblocked()?;
    let trial = record.clone().with("url", SdkValue::text(&request.url));
    super::check::same_store(&trial, &selection(&trial)?).map_err(|refusal| {
        refused(
            &refusal.code,
            format!("PostgreSQL Store {name}: {}", refusal.message),
        )
    })?;
    let home_lock = home::lock::take(home_dir)?;
    let store_lock = crate::record::lock(home_dir, name)?;
    let current = StoreRecord::read(home_dir, name)?.filter(|r| r.binding() == record.binding());
    let Some(current) = current else {
        return Err(refused(
            "Conflict",
            format!("Store {name} changed while its address was checked; connect it again"),
        ));
    };
    current.unblocked()?;
    crate::open::close(&current.postgresql_key(home_dir, name))?;
    let current = current.with("url", SdkValue::text(&request.url)).cleaned();
    current.write(home_dir)?;
    drop((store_lock, home_lock));
    Ok(answer(&current, "updated", None, sandboxes))
}

/// Refuses `--auth` and `--synchronous-commit` on a connected Store's
/// connect: a machine never has its own login, and both are the Store's,
/// changed with `store update`.
pub fn refuse_settings(name: &str, request: &StoreConnect) -> Result<()> {
    let given = match (&request.auth, &request.synchronous_commit) {
        (Some(_), _) => "--auth",
        (None, Some(_)) => "--synchronous-commit",
        (None, None) => return Ok(()),
    };
    Err(refused(
        "InvalidInput",
        format!(
            "Store {name} is connected: a connect sets an address only and takes no {given}; `layerfs store update {name}` changes its login or setting"
        ),
    ))
}

/// `store.disconnect` of a whole Store: forgets a PostgreSQL Store; its
/// data stays. Refused `InUse` while a process holds it or a Sandbox of its
/// projects exists; a sqlite Store is `InvalidInput`, naming `store remove`.
pub fn disconnect(home_dir: &Path, name: &StoreName, related: Related) -> Outcome {
    // a record this build does not read: its records alone go
    if StoreRecord::read(home_dir, name)?.is_some_and(|record| record.store_ref().is_err()) {
        crate::record::forget_unread(home_dir, name, related)?;
        return Ok(disconnected(name, None));
    }
    let _lock = home::lock::take(home_dir)?;
    let _store = crate::record::lock(home_dir, name)?;
    let record = StoreRecord::read(home_dir, name)?
        .ok_or_else(|| refused("NotFound", format!("no Store named {name}")))?;
    if record.kind()? != StoreKind::Postgresql {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is a sqlite Store; `layerfs store remove {name}` deletes it"),
        ));
    }
    // leases, and Sandboxes: a Sandbox's daemon dials the server from its
    // machine, which holds no lease in this home (#535 §7.4)
    (related.store_idle)(home_dir, name)?;
    crate::open::close(&record.postgresql_key(home_dir, name))?;
    std::fs::remove_dir_all(paths::store_dir(home_dir, name)).map_err(failed)?;
    layerfs_home::home::generation::moved(home_dir);
    Ok(disconnected(name, None))
}

/// `store.disconnect`'s answer: `{ ok, outcome: disconnected, store,
/// machine }`.
pub fn disconnected(store: &str, machine: Option<&str>) -> SdkValue {
    SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("disconnected")),
        ("store", SdkValue::text(store)),
        ("machine", machine.map_or(SdkValue::Null, SdkValue::text)),
    ])
}

/// `store get --with live` of a PostgreSQL Store: whether its server answers, its
/// version and the schema's size.
pub fn status(record: &StoreRecord) -> SdkValue {
    let url = match url(record) {
        Ok(url) => url,
        Err(refusal) => return SdkValue::map([("unavailable", SdkValue::Text(refusal.message))]),
    };
    crate::postgresql::open::status(&url, &[SCHEMA.to_owned()])
        .unwrap_or_else(|refusal| SdkValue::map([("unavailable", SdkValue::Text(refusal.message))]))
}

/// The Store's compaction operation as `store get --with live` (T10.5) renders it:
/// `{id, name, phase, started, uncertain, state, phases}`, or null when the
/// Store has none. One status query reads the Store-side `gc_op` and the
/// compaction `gc_intent`; no Store session is opened, no schema is changed
/// and no artifact or scratch schema is read. An unreachable server or a
/// query failure is null here: the `server` field already reports reach.
pub fn compact_status(record: &StoreRecord) -> SdkValue {
    let Ok(url) = url(record) else {
        return SdkValue::Null;
    };
    match layerfs_persistence::postgresql_compact_status(&url, SCHEMA) {
        Ok(Some(op)) => crate::report::compact_status(&op.status()),
        _ => SdkValue::Null,
    }
}

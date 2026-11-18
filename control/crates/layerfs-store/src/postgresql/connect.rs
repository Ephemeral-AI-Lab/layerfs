//! PostgreSQL Stores (#533 step 10a, call surface 2.0 `pg.html`): `store
//! connect` records a database as a Store in the fixed `layerfs` schema,
//! or sets the address every machine dials; `store disconnect` forgets it
//! (its data stays). The record keeps the URL with no user and no password
//! and names its saved login (`auth`, [`super::login`]), put into the URL on
//! each call; the full URL is never written down. A machine's own address
//! is `layerfs-control`'s part of the call, proven from that machine.
use super::login;
use crate::open_args::PgSelection;
use crate::project::record::ProjectRecord;
use crate::record::url;
use crate::record::{StoreKind, StoreRecord};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::secrets::parts;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The one schema a 2.0 PostgreSQL Store lives in.
pub const SCHEMA: &str = "layerfs";

/// How many project names an answer shows.
const SHOWN: usize = 20;

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
                .unwrap_or("off")
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
/// is a saved one, named with `--auth`.
pub fn refuse_login(url: &str) -> Result<()> {
    let (_, user, rest) = parts(url)?;
    let in_query = query(rest).iter().any(|(name, _)| {
        let name = name.to_ascii_lowercase();
        name == "user" || name.contains("password")
    });
    if user.is_some() || in_query {
        return Err(refused(
            "InvalidInput",
            "the URL names a user or holds a password; give the address alone and the login with --auth (`layerfs store save-auth` saves one)",
        ));
    }
    Ok(())
}

/// The server and database a URL names, as libpq reads it: `(host, port,
/// database)` from the authority and path, overridden by `host`, `port`
/// and `dbname` parameters; port 5432 by default; the host in lower case.
/// What a refusal names, never the URL.
pub fn target(url: &str) -> Result<(String, String, String)> {
    let (_, _, rest) = parts(url)?;
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

/// The addresses of PostgreSQL Store `record` as answers show them: the one
/// every machine dials (`machine` null) first, then each machine's own.
pub fn connections(record: &StoreRecord) -> SdkValue {
    let mut found = Vec::new();
    if let Some(url) = record.text("url") {
        found.push(SdkValue::map([
            ("machine", SdkValue::Null),
            ("url", SdkValue::text(url)),
        ]));
    }
    for (machine, own) in record.own_connections() {
        if let Some(url) = own.url {
            found.push(SdkValue::map([
                ("machine", SdkValue::Text(machine)),
                ("url", SdkValue::Text(url)),
            ]));
        }
    }
    SdkValue::List(found)
}

/// `store.connect`'s answer for `record`: `outcome` (`connected` or
/// `updated`), the projects found on a first connect (the first 20 names
/// and the count), the login, the addresses and the setting.
pub fn answer(record: &StoreRecord, outcome: &str, projects: Option<&[String]>) -> SdkValue {
    let (names, count) = match projects {
        Some(found) => (
            SdkValue::List(found.iter().take(SHOWN).map(SdkValue::text).collect()),
            SdkValue::Int(found.len() as i64),
        ),
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
        ("connections", connections(record)),
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
    refuse_login(given)?;
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
    let listed = crate::identity::list_projects(&crate::open_args::OpenArgs {
        home: Some(home_dir.to_path_buf()),
        store: Some(name.to_owned()),
        store_record: Some(record.to_record().clone()),
        postgresql: Some(probe),
        store_path: Some(record.postgresql_key(home_dir, name).display().to_string()),
        ..Default::default()
    })?;
    let _lock = home::lock::take(home_dir)?;
    if paths::store_record(home_dir, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    unclaimed_by_others(home_dir, name, &binding)?;
    login::saved(home_dir, auth)?;
    home::private_dir(&directory)?;
    record.write(home_dir)?;
    let mut projects = Vec::new();
    if let SdkValue::List(items) = &listed {
        for item in items {
            if let Some(project) = item.get_text("name") {
                let stack = item.get("stack").cloned().unwrap_or_default();
                ProjectRecord::found(name, project, stack).write(home_dir)?;
                projects.push(project.to_owned());
            }
        }
    }
    Ok(answer(&record, "connected", Some(&projects)))
}

/// `store.connect` of a connected Store without a machine: `request.url`
/// becomes the address every machine dials, proven from this process (it
/// must read this Store's id: `Unavailable` otherwise). `--auth` and
/// `--synchronous-commit` are refused (`store update` changes both). The
/// record is written under the home lock and the Store's, `Conflict` when
/// it was disconnected or connected to another database meanwhile; this
/// process's open sessions on it are closed. A running Sandbox keeps the
/// connection it started with.
pub fn readdress(home_dir: &Path, request: &StoreConnect) -> Outcome {
    let name = request.store.as_str();
    let record = super::check::postgresql_record(home_dir, name)?;
    refuse_settings(name, request)?;
    refuse_login(&request.url)?;
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
    let current = current.with("url", SdkValue::text(&request.url));
    current.write(home_dir)?;
    drop((store_lock, home_lock));
    Ok(answer(&current, "updated", None))
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

/// `store.status` of a PostgreSQL Store: whether its server answers, its
/// version and the schema's size.
pub fn status(record: &StoreRecord) -> SdkValue {
    let url = match url(record) {
        Ok(url) => url,
        Err(refusal) => return SdkValue::map([("unavailable", SdkValue::Text(refusal.message))]),
    };
    crate::postgresql::open::status(&url, &[SCHEMA.to_owned()])
        .unwrap_or_else(|refusal| SdkValue::map([("unavailable", SdkValue::Text(refusal.message))]))
}

/// The Store's compaction operation as `store.status` (T10.5) renders it:
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

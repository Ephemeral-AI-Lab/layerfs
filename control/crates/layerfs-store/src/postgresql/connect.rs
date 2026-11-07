//! PostgreSQL Stores (#533 step 10a, H §7.4): `store connect` records a
//! database as a Store in the fixed `layerfs` schema, `store disconnect`
//! forgets it (its data stays). The record keeps the URL without a password
//! and the password as a reference (`env:NAME` or `file:/path`), resolved on
//! each call; the full URL is never written down.
use crate::project::record::ProjectRecord;
use crate::record::url;
use crate::record::{StoreKind, StoreRecord};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::secrets::{parts, resolve};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The one schema a 2.0 PostgreSQL Store lives in.
pub const SCHEMA: &str = "layerfs";

/// The host operations' `postgresql` argument for a Store record.
pub fn selection(record: &StoreRecord) -> Result<SdkValue> {
    Ok(selection_at(record, url(record)?))
}

/// [`selection`] dialling `url`: the record's URL with its password, as a
/// machine reaches it (#535 §7.4).
pub fn selection_at(record: &StoreRecord, url: String) -> SdkValue {
    SdkValue::map([
        ("url", SdkValue::Text(url)),
        ("schema", SdkValue::text(SCHEMA)),
        (
            "synchronousCommit",
            SdkValue::text(record.text("synchronous_commit").unwrap_or("off")),
        ),
    ])
}

/// `store.connect`: attach a PostgreSQL database as a Store.
pub fn connect(home: &Path, args: &SdkValue) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    layerfs_home::names::StoreName::parse(name)?;
    connect_named(home, name, args)
}

/// `store.disconnect`: forget a PostgreSQL Store; its data stays.
pub fn disconnect(home: &Path, args: &SdkValue, related: Related) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    layerfs_home::names::StoreName::parse(name)?;
    disconnect_named(home, name, related)
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

/// Refuses a URL that holds a password, in its user part or as any
/// `password` (or `sslpassword`) parameter: a password is a reference.
fn refuse_password(url: &str) -> Result<()> {
    let (_, user, rest) = parts(url)?;
    let in_query = query(rest)
        .iter()
        .any(|(name, _)| name.to_ascii_lowercase().contains("password"));
    if user.is_some_and(|u| u.contains(':')) || in_query {
        return Err(refused(
            "InvalidInput",
            "the URL holds a password; give it as a reference with --password env:NAME or file:/path",
        ));
    }
    Ok(())
}

/// The server and database a URL names, as libpq reads it: `(host, port,
/// database)` from the authority and path, overridden by `host`, `port`,
/// `dbname` and `user` parameters; port 5432 and the database named as
/// the user by default; the host in lower case. Every 2.0 Store is in the
/// one schema, so this names the Store's data.
fn target(url: &str) -> Result<(String, String, String)> {
    let (_, user, rest) = parts(url)?;
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
    let mut user = user.map(decode).unwrap_or_default();
    for (name, value) in query(rest) {
        match name.as_str() {
            "host" => host = value,
            "port" => port = value,
            "dbname" => database = value,
            "user" => user = value,
            _ => {}
        }
    }
    if port.is_empty() {
        port = "5432".to_owned();
    }
    if database.is_empty() {
        database = user;
    }
    Ok((decode(&host).to_ascii_lowercase(), port, database))
}

/// Refuses `Exists` when a Store is named `name`, or when another
/// PostgreSQL Store of this home holds the same data: the same server and
/// database by its URL, or, once read, the same schema binding.
fn unclaimed(
    home: &Path,
    name: &str,
    target_of: &(String, String, String),
    binding: Option<&[u8]>,
) -> Result<()> {
    if paths::store_record(home, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    for other in paths::names(&home.join("store")).map_err(failed)? {
        let Some(record) = StoreRecord::read(home, &other)? else {
            continue;
        };
        if record.kind()? != StoreKind::Postgresql {
            continue;
        }
        let same_url = record
            .text("url")
            .and_then(|url| target(url).ok())
            .is_some_and(|found| found == *target_of);
        let same_binding = binding.is_some_and(|binding| record.binding() == Some(binding));
        if same_url || same_binding {
            return Err(refused(
                "Exists",
                format!(
                    "Store {other} connects the same database ({}:{}/{}, schema {SCHEMA})",
                    target_of.0, target_of.1, target_of.2
                ),
            ));
        }
    }
    Ok(())
}

/// `store.connect(name, url, { password, synchronous_commit })`. Every
/// refusal of its input comes before any effect. The server's schema and
/// projects are read before the home lock; the lock covers the re-checks
/// and the record writes only.
fn connect_named(home: &Path, name: &str, args: &SdkValue) -> Outcome {
    let given = args.get_text("url").unwrap_or("");
    refuse_password(given)?;
    let sync = args.get_text("synchronous_commit").unwrap_or("off");
    if !matches!(
        sync,
        "off" | "local" | "remote_write" | "on" | "remote_apply"
    ) {
        return Err(refused(
            "InvalidInput",
            format!(
                "synchronous_commit is off, local, remote_write, on or remote_apply, not {sync}"
            ),
        ));
    }
    let password = args.get_text("password");
    if let Some(password) = password {
        resolve(password)?;
    }
    let record = StoreRecord::connected(name, given, SCHEMA, sync, password);
    let selection = selection(&record)?;
    let target_of = target(given)?;
    unclaimed(home, name, &target_of, None)?;
    // an empty database gets the schema, with a new binding (#533 step
    // 10b); an existing LayerFS schema keeps its binding, read once. The
    // cursor key is this home's own either way.
    let probe = SdkValue::map([("postgresql", selection.clone())]);
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
    let record = record.with("binding", SdkValue::Bytes(binding.clone()));
    let record = record.with("cursor", SdkValue::Bytes(fresh.cursor_key.to_vec()));
    let directory = paths::store_dir(home, name);
    // the projects the schema holds become this home's project records; a
    // schema that does not open is not connected
    let listed = crate::identity::list_projects(&crate::open_args::OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(name.to_owned()),
        store_record: Some(record.to_record().clone()),
        postgresql: Some(crate::open_args::PgSelection::read(&selection)),
        store_path: Some(record.postgresql_key(home, name).display().to_string()),
        ..Default::default()
    })?;
    let _lock = home::lock::take(home)?;
    unclaimed(home, name, &target_of, Some(&binding))?;
    home::private_dir(&directory)?;
    record.write(home)?;
    let mut projects = Vec::new();
    if let SdkValue::List(items) = &listed {
        for item in items {
            if let Some(project) = item.get_text("name") {
                let stack = item.get("stack").cloned().unwrap_or_default();
                ProjectRecord::found(name, project, stack).write(home)?;
                projects.push(SdkValue::text(project));
            }
        }
    }
    Ok(SdkValue::map([
        ("name", SdkValue::text(name)),
        ("kind", SdkValue::text("postgresql")),
        ("schema", SdkValue::text(SCHEMA)),
        ("synchronous_commit", SdkValue::text(sync)),
        ("projects", SdkValue::List(projects)),
    ]))
}

/// `store.disconnect(name)`: forgets a PostgreSQL Store; its data stays.
fn disconnect_named(home: &Path, name: &str, related: Related) -> Outcome {
    layerfs_home::names::StoreName::parse(name)?;
    let _lock = home::lock::take(home)?;
    let _store = crate::record::lock(home, name)?;
    let record = StoreRecord::read(home, name)?
        .ok_or_else(|| refused("NotFound", format!("no Store named {name}")))?;
    if record.kind()? != StoreKind::Postgresql {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is a sqlite Store; `store remove` deletes it"),
        ));
    }
    // leases, and Sandboxes: a Sandbox's daemon dials the server from its
    // machine, which holds no lease in this home (#535 §7.4)
    (related.store_idle)(home, name)?;
    crate::open::close(&record.postgresql_key(home, name))?;
    std::fs::remove_dir_all(paths::store_dir(home, name)).map_err(failed)?;
    Ok(SdkValue::map([("disconnected", SdkValue::text(name))]))
}

/// `store.status` of a PostgreSQL Store: whether its server answers, its
/// version and the schema's size.
pub fn status(record: &StoreRecord) -> SdkValue {
    let url = match url(record) {
        Ok(url) => url,
        Err(refusal) => return SdkValue::map([("unavailable", SdkValue::Text(refusal.message))]),
    };
    let args = SdkValue::map([
        ("url", SdkValue::text(url)),
        ("schemas", SdkValue::List(vec![SdkValue::text(SCHEMA)])),
    ]);
    crate::postgresql::open::status(&args)
        .unwrap_or_else(|refusal| SdkValue::map([("unavailable", SdkValue::Text(refusal.message))]))
}

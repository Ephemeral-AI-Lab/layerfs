//! PostgreSQL Stores (#533 step 10a, H §7.4): `store connect` records a
//! database as a Store in the fixed `layerfs` schema, `store disconnect`
//! forgets it (its data stays). The record keeps the URL without a password
//! and the password as a reference (`env:NAME` or `file:/path`), resolved on
//! each call; the full URL is never written down.
use crate::calls::Context;
use crate::home::{self, paths, read_record, write_record};
use crate::host::outcome::{failed, refused, Outcome};
use crate::store::lease;
use crate::store::store_ref::{kind_of, StoreKind};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The one schema a 2.0 PostgreSQL Store lives in.
pub const SCHEMA: &str = "layerfs";

/// `(scheme://, userinfo, rest)` of a PostgreSQL URL.
fn parts(url: &str) -> Result<(&str, Option<&str>, &str)> {
    let (scheme, after) = url
        .split_once("://")
        .filter(|(scheme, _)| matches!(*scheme, "postgresql" | "postgres"))
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "a PostgreSQL URL starts postgresql:// or postgres://",
            )
        })?;
    let authority_end = after.find('/').unwrap_or(after.len());
    match after[..authority_end].rfind('@') {
        Some(at) => Ok((scheme, Some(&after[..at]), &after[at + 1..])),
        None => Ok((scheme, None, after)),
    }
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

/// A password reference's value: `env:NAME` or `file:/absolute/path`.
pub(crate) fn resolve(reference: &str) -> Result<String> {
    if let Some(name) = reference.strip_prefix("env:") {
        return std::env::var(name)
            .ok()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| {
                refused(
                    "Unavailable",
                    format!("the password variable {name} is not set"),
                )
            });
    }
    if let Some(path) = reference
        .strip_prefix("file:")
        .filter(|p| Path::new(p).is_absolute())
    {
        let text = std::fs::read_to_string(path)
            .map_err(|e| refused("Unavailable", format!("{path}: {e}")))?;
        return Ok(text.trim_end_matches(['\n', '\r']).to_owned());
    }
    Err(refused(
        "InvalidInput",
        "a password is a reference: env:NAME or file:/absolute/path",
    ))
}

/// The full URL of a PostgreSQL Store's record, its password resolved now.
pub(crate) fn url(record: &SdkValue, store: &str) -> Result<String> {
    let url = record.get_text("url").ok_or_else(|| {
        refused(
            "Failed",
            format!("PostgreSQL Store {store}'s record has no url"),
        )
    })?;
    with_password(url, record.get_text("password"))
}

fn with_password(url: &str, password: Option<&str>) -> Result<String> {
    let Some(reference) = password else {
        return Ok(url.to_owned());
    };
    let (scheme, user, rest) = parts(url)?;
    let user = user.ok_or_else(|| {
        refused(
            "InvalidInput",
            "a URL with a password reference names its user",
        )
    })?;
    Ok(format!(
        "{scheme}://{user}:{}@{rest}",
        percent(&resolve(reference)?)
    ))
}

/// The host operations' `postgresql` argument for a Store record.
pub(crate) fn selection(record: &SdkValue, store: &str) -> Result<SdkValue> {
    Ok(SdkValue::map([
        ("url", SdkValue::text(url(record, store)?)),
        ("schema", SdkValue::text(SCHEMA)),
        (
            "synchronousCommit",
            SdkValue::text(record.get_text("synchronous_commit").unwrap_or("off")),
        ),
    ]))
}

/// Runs `store.connect` or `store.disconnect`.
pub(crate) fn run(context: &mut Context<'_>, call: &str, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    crate::names::name("store", name).map_err(|why| refused("InvalidInput", why))?;
    match call {
        "connect" => connect(&home, name, args),
        _ => disconnect(&home, name),
    }
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
        let Some(record) = read_record(&paths::store_record(home, &other))? else {
            continue;
        };
        if kind_of(&record)? != StoreKind::Postgresql {
            continue;
        }
        let same_url = record
            .get_text("url")
            .and_then(|url| target(url).ok())
            .is_some_and(|found| found == *target_of);
        let same_binding = binding.is_some_and(
            |binding| matches!(record.get("binding"), Some(SdkValue::Bytes(b)) if b == binding),
        );
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
fn connect(home: &Path, name: &str, args: &SdkValue) -> Outcome {
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
    let mut record = SdkValue::map([
        ("name", SdkValue::text(name)),
        ("kind", SdkValue::text("postgresql")),
        ("url", SdkValue::text(given)),
        ("schema", SdkValue::text(SCHEMA)),
        ("synchronous_commit", SdkValue::text(sync)),
        ("created", SdkValue::Int(home::now_ms())),
    ]);
    if let Some(password) = args.get_text("password") {
        resolve(password)?;
        record = home::with_field(record, "password", SdkValue::text(password));
    }
    let selection = selection(&record, name)?;
    let target_of = target(given)?;
    unclaimed(home, name, &target_of, None)?;
    // an empty database gets the schema, with a new binding (#533 step
    // 10b); an existing LayerFS schema keeps its binding, read once. The
    // cursor key is this home's own either way.
    let probe = SdkValue::map([("postgresql", selection.clone())]);
    let fresh = crate::host::stores::Identity {
        kind: crate::host::stores::Kind::Postgresql {
            schema: SCHEMA.to_owned(),
            synchronous_commit: sync.to_owned(),
        },
        binding: crate::host::registry::random()?.to_vec(),
        cursor_key: crate::host::registry::random()?,
    };
    let binding = if crate::host::postgres_init::create_schema(&probe, &fresh)? {
        fresh.binding
    } else {
        crate::host::postgres::read_binding(&probe)?
    };
    let record = home::with_field(record, "binding", SdkValue::Bytes(binding.clone()));
    let record = home::with_field(record, "cursor", SdkValue::Bytes(fresh.cursor_key.to_vec()));
    let directory = paths::store_dir(home, name);
    // the projects the schema holds become this home's project records; a
    // schema that does not open is not connected
    let listed = crate::host::stores::list_projects(&SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("store", SdkValue::text(name)),
        ("store_record", record.clone()),
        ("postgresql", selection),
        (
            "storePath",
            SdkValue::text(directory.join("postgresql").display().to_string()),
        ),
    ]))?;
    let _lock = home::lock::take(home)?;
    unclaimed(home, name, &target_of, Some(&binding))?;
    home::private_dir(&directory)?;
    write_record(&paths::store_record(home, name), &record)?;
    let mut projects = Vec::new();
    if let SdkValue::List(items) = &listed {
        for item in items {
            if let Some(project) = item.get_text("name") {
                write_record(
                    &paths::project_record(home, name, project),
                    &SdkValue::map([
                        ("name", SdkValue::text(project)),
                        ("stack", item.get("stack").cloned().unwrap_or_default()),
                    ]),
                )?;
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
fn disconnect(home: &Path, name: &str) -> Outcome {
    let _lock = home::lock::take(home)?;
    let record = read_record(&paths::store_record(home, name))?
        .ok_or_else(|| refused("NotFound", format!("no Store named {name}")))?;
    if kind_of(&record)? != StoreKind::Postgresql {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is a sqlite Store; `store remove` deletes it"),
        ));
    }
    let holders = lease::holders(home, name)?;
    if !holders.is_empty() {
        return Err(refused(
            "InUse",
            format!("Store {name} is open in process {holders:?}"),
        ));
    }
    crate::store::open::close(&paths::store_dir(home, name).join("postgresql"))?;
    std::fs::remove_dir_all(paths::store_dir(home, name)).map_err(failed)?;
    Ok(SdkValue::map([("disconnected", SdkValue::text(name))]))
}

/// `store.status` of a PostgreSQL Store: whether its server answers, its
/// version and the schema's size.
pub(crate) fn status(record: &SdkValue, name: &str) -> SdkValue {
    let url = match url(record, name) {
        Ok(url) => url,
        Err(refusal) => return SdkValue::map([("unavailable", SdkValue::Text(refusal.message))]),
    };
    let args = SdkValue::map([
        ("url", SdkValue::text(url)),
        ("schemas", SdkValue::List(vec![SdkValue::text(SCHEMA)])),
    ]);
    crate::host::postgres::status(&args)
        .unwrap_or_else(|refusal| SdkValue::map([("unavailable", SdkValue::Text(refusal.message))]))
}

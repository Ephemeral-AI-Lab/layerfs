//! PostgreSQL Stores (#533 step 10a, H §7.4): `store connect` records a
//! database as a Store in the fixed `layerfs` schema, `store disconnect`
//! forgets it (its data stays). The record keeps the URL without a password
//! and the password as a reference (`env:NAME` or `file:/path`), resolved on
//! each call; the full URL is never written down.
use crate::calls::Context;
use crate::home::{self, paths, read_record, write_record};
use crate::host::outcome::{failed, refused, Outcome};
use crate::store::lease;
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

/// `store.connect(name, url, { password, synchronous_commit })`.
fn connect(home: &Path, name: &str, args: &SdkValue) -> Outcome {
    let given = args.get_text("url").unwrap_or("");
    let (_, user, _) = parts(given)?;
    if user.is_some_and(|u| u.contains(':')) {
        return Err(refused(
            "InvalidInput",
            "the URL holds a password; give it as a reference with --password env:NAME or file:/path",
        ));
    }
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
    let _lock = home::lock::take(home)?;
    if paths::store_record(home, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
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
    let record = home::with_field(record, "binding", SdkValue::Bytes(binding));
    let record = home::with_field(record, "cursor", SdkValue::Bytes(fresh.cursor_key.to_vec()));
    let directory = paths::store_dir(home, name);
    home::private_dir(&directory)?;
    write_record(&paths::store_record(home, name), &record)?;
    // the projects the schema holds become this home's project records
    let listed = crate::host::stores::list_projects(&SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("store", SdkValue::text(name)),
        ("store_record", record.clone()),
        ("postgresql", selection),
        (
            "storePath",
            SdkValue::text(directory.join("postgresql").display().to_string()),
        ),
    ]));
    let listed = match listed {
        Ok(listed) => listed,
        Err(refusal) => {
            // a schema that does not open is not connected
            let _ = std::fs::remove_dir_all(&directory);
            return Err(refusal);
        }
    };
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
    if record.get_text("kind") != Some("postgresql") {
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

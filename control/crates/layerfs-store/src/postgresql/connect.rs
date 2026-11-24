//! PostgreSQL Stores (#533 step 10a; owner, 2026-11-24): `store connect`
//! records a database as a Store in the fixed `layerfs` schema, or changes
//! a connected Store's address, user, password or `synchronous_commit`;
//! `store disconnect` forgets it (its data stays). The host is the one
//! place a Store's URL and login are saved: the record keeps the URL as
//! given, an address alone, and the user; the password is a secret of the
//! home ([`super::login`]), put into the URL on each call. The full URL is
//! never written down, and no recorded URL is ever rewritten. Machines
//! attach to a Store (`layerfs-control`'s `store.attach`) and dial both.
use super::address::{self, with_login};
pub use super::address::{holds_login, refuse_login, shown, target, HIDDEN};
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
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The one schema a 2.0 PostgreSQL Store lives in.
pub const SCHEMA: &str = "layerfs";

/// What a call on Store `record` dials: its URL with its login.
pub fn selection(record: &StoreRecord) -> Result<PgSelection> {
    Ok(selection_at(record, url(record)?))
}

/// [`selection`] dialling `url`, the record's address with a login in it.
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

/// `store.connect(store, url, { user, password, synchronous_commit })`.
/// No `Debug`: it holds the password.
#[derive(Clone)]
pub struct StoreConnect {
    /// The Store's name.
    pub store: StoreName,
    /// The database's URL, an address alone: no user, no password.
    pub url: String,
    /// The PostgreSQL role; required for a new Store, else the saved one.
    pub user: Option<String>,
    /// Its password; required for a new Store, else the saved one.
    pub password: Option<String>,
    /// `synchronous_commit`; `local` for a new Store, else the saved one.
    pub synchronous_commit: Option<String>,
}

/// [`refuse_login`]'s refusal for `store connect <store> <url>`, with its
/// fix, which names the address `<url>` and the user `<user>`, never text
/// of the URL: the address alone, the login given apart (`new`: the
/// Store's first connect, which needs it; on a connected Store only when
/// the URL may hold one).
pub fn refuse_url(store: &str, url: &str, new: bool) -> Result<()> {
    // a URL that is not a PostgreSQL one is refused without a fix: its login
    // cannot be told apart, so no line may repeat it
    address::scheme(url)?;
    refuse_login(url).map_err(|refusal| {
        let login = match new || holds_login(url) {
            true => " --user <user> --password-stdin",
            false => "",
        };
        let fix = format!("layerfs store connect {} <url>{login}", shell_word(store));
        with_fix(refusal, fix)
    })
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

/// This computer's last reach of PostgreSQL Store `record`, `{ at, ok,
/// latency_ms, server_version, error }`, or null when none was seen.
pub fn reach(record: &StoreRecord) -> SdkValue {
    let seen = (record.home()).map(|home| crate::observed::read(home, record.name()));
    seen.and_then(|seen| seen.here).unwrap_or_default()
}

/// The machines attached to PostgreSQL Store `record`, as answers show
/// them: with `rows`, `{ machine, reached, reach }` each, the last reach
/// seen from it and its word (`ok`, `failed`, null); else their names.
pub fn machines(record: &StoreRecord, rows: bool) -> SdkValue {
    let attached = record.attached();
    if !rows {
        return SdkValue::List(attached.iter().map(SdkValue::text).collect());
    }
    let seen = (record.home())
        .map(|home| crate::observed::read(home, record.name()))
        .unwrap_or_default();
    let rows = attached.into_iter().map(|machine| {
        let reach = seen.of(&machine);
        SdkValue::map([
            ("machine", SdkValue::text(machine)),
            ("reached", crate::observed::reached(reach.as_ref())),
            ("reach", reach.unwrap_or_default()),
        ])
    });
    SdkValue::List(rows.collect())
}

/// When Store `record`'s password was last saved, as an answer's time.
pub fn password_changed(record: &StoreRecord) -> (bool, SdkValue) {
    let saved = (record.home()).and_then(|home| login::read(home, record.name()).ok().flatten());
    let at = saved.as_ref().and_then(|saved| saved.changed_ms);
    (saved.is_some(), crate::get::at(at))
}

/// `store.connect`'s answer for `record`: `outcome` (`connected` or
/// `updated`), the projects found on a first connect (the first 20 names,
/// and the count only when there were more), the address, the user, the
/// setting and the attached machines; never the password.
pub fn answer(record: &StoreRecord, outcome: &str, projects: Option<&[String]>) -> SdkValue {
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
        ("url", text("url")),
        ("user", text("user")),
        ("synchronous_commit", text("synchronous_commit")),
        ("machines", machines(record, false)),
    ])
}

/// A given user or password that is empty, refused.
fn given<'a>(value: Option<&'a str>, what: &str) -> Result<Option<&'a str>> {
    match value {
        Some("") => Err(refused("InvalidInput", format!("the {what} is empty"))),
        other => Ok(other),
    }
}

/// `store.connect`: a new Store ([`connect`]), or a connected one's
/// address, login or setting ([`update`]). A sqlite Store of the name is
/// `Exists`.
pub fn run(home_dir: &Path, request: &StoreConnect) -> Outcome {
    let name = request.store.as_str();
    let Some(record) = StoreRecord::read(home_dir, name)? else {
        return connect(home_dir, request);
    };
    if record.kind()? != StoreKind::Postgresql {
        return Err(refused(
            "Exists",
            format!("a sqlite Store named {name} exists"),
        ));
    }
    update(home_dir, request)
}

/// `store.connect` of a new Store: records a PostgreSQL database as Store
/// `request.store`, its user and password required. Every refusal of its
/// input comes before any effect. The connection is made from this
/// process with the given login, which gives an empty database the schema
/// and a new id or reads the id the schema holds, and lists its projects,
/// before the home lock; the lock covers the re-checks and the writes
/// only: the record, then the password. A database whose id another
/// Store of this home holds is `Exists`.
pub fn connect(home_dir: &Path, request: &StoreConnect) -> Outcome {
    let name = request.store.as_str();
    let given_url = request.url.as_str();
    refuse_url(name, given_url, true)?;
    target(given_url)?;
    let needs = |what: &str| {
        refused(
            "InvalidInput",
            format!("connecting Store {name} needs its login: --user <user> --password-stdin (no {what} was given)"),
        )
    };
    let user = given(request.user.as_deref(), "user")?.ok_or_else(|| needs("user"))?;
    let password =
        given(request.password.as_deref(), "password")?.ok_or_else(|| needs("password"))?;
    let sync = request.synchronous_commit.as_deref().unwrap_or("local");
    synchronous_commit(sync)?;
    if paths::store_record(home_dir, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    let record = StoreRecord::connected(name, given_url, user, SCHEMA, sync).in_home(home_dir);
    let probe = selection_at(&record, with_login(given_url, user, password)?);
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
    home::private_dir(&directory)?;
    record.write(home_dir)?;
    login::save(home_dir, name, password)?;
    let mut projects = Vec::new();
    for project in listed {
        let stack = SdkValue::text(hex(&project.stack.to_bytes()));
        ProjectRecord::found(name, &project.name, stack).write(home_dir)?;
        projects.push(project.name);
    }
    Ok(answer(&record, "connected", Some(&projects)))
}

/// `store.connect` of a connected Store: its address `request.url`, and
/// any of a new user, password and `synchronous_commit` (those not given
/// are kept). The new values are dialled once from this process and must
/// read this Store's id (`Unavailable` otherwise) before anything is
/// saved. The record (and a new password) are written under
/// the home lock and the Store's, `Conflict` when it was disconnected or
/// connected to another database meanwhile; this process's open sessions
/// on it are closed. A running Sandbox keeps the connection it started
/// with.
pub fn update(home_dir: &Path, request: &StoreConnect) -> Outcome {
    let name = request.store.as_str();
    let record = super::check::postgresql_record(home_dir, name)?;
    refuse_url(name, &request.url, false)?;
    target(&request.url)?;
    record.unblocked()?;
    let user = given(request.user.as_deref(), "user")?.or(record.text("user"));
    let user = user.map(str::to_owned).ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("Store {name}'s record names no user: give it with --user"),
        )
    })?;
    let new_password = given(request.password.as_deref(), "password")?;
    let saved = login::read(home_dir, name)?;
    let password = match (new_password, &saved) {
        (Some(password), _) => password.to_owned(),
        (None, Some(saved)) => saved.password().to_owned(),
        (None, None) => {
            return Err(refused(
                "InvalidInput",
                format!(
                "Store {name}'s password is not saved in this home: give it with --password-stdin"
            ),
            ))
        }
    };
    if let Some(sync) = &request.synchronous_commit {
        synchronous_commit(sync)?;
    }
    let changed = |record: StoreRecord| {
        let record = record
            .with("url", SdkValue::text(&request.url))
            .with("user", SdkValue::text(&user));
        match &request.synchronous_commit {
            Some(sync) => record.with("synchronous_commit", SdkValue::text(sync)),
            None => record,
        }
    };
    let trial = changed(record.clone());
    let probe = selection_at(&trial, with_login(&request.url, &user, &password)?);
    super::check::same_store(&trial, &probe).map_err(|refusal| {
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
            format!("Store {name} changed while its connection was checked; connect it again"),
        ));
    };
    current.unblocked()?;
    crate::open::close(&current.postgresql_key(home_dir, name))?;
    let current = changed(current);
    current.write(home_dir)?;
    if let Some(password) = new_password {
        login::save(home_dir, name, password)?;
    }
    drop((store_lock, home_lock));
    Ok(answer(&current, "updated", None))
}

/// `store.disconnect`: forgets a PostgreSQL Store, its saved password and
/// its attachments; its data stays. Refused `InUse` while a process holds
/// it or a Sandbox of its projects exists; a sqlite Store is
/// `InvalidInput`, naming `store remove`.
pub fn disconnect(home_dir: &Path, name: &StoreName, related: Related) -> Outcome {
    // a record this build does not read: its records alone go
    if StoreRecord::read(home_dir, name)?.is_some_and(|record| record.store_ref().is_err()) {
        crate::record::forget_unread(home_dir, name, related)?;
        login::forget(home_dir, name)?;
        return Ok(disconnected(name));
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
    login::forget(home_dir, name)?;
    layerfs_home::home::generation::moved(home_dir);
    Ok(disconnected(name))
}

/// `store.disconnect`'s answer: `{ ok, outcome: disconnected, store }`.
pub fn disconnected(store: &str) -> SdkValue {
    SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("disconnected")),
        ("store", SdkValue::text(store)),
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

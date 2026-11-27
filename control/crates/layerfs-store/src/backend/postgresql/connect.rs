//! PostgreSQL Stores (#533 step 10a; owner, 2026-11-24): `store connect`
//! records a database as a Store in the fixed `layerfs` schema, or changes
//! a connected Store's address, user, password or `synchronous_commit`;
//! `store disconnect` forgets it (its data stays). The host is the one
//! place a Store's URL and login are saved: the record keeps the URL as
//! given, an address alone, and the user; the password is a secret of the
//! home ([`super::login`]), put into the URL on each call. The full URL is
//! never written down, and no recorded URL is ever rewritten. Machines
//! attach to a Store (`layerfs-api`'s `store.attach`) and dial both.
use super::address::{self, with_login};
pub use super::address::{holds_login, refuse_login, shown, target, HIDDEN};
use super::check;
use super::login;
use crate::admin::open::args::{OpenArgs, PgSelection};
use crate::admin::record::store::url;
use crate::admin::record::store::{StoreKind, StoreRecord};
use crate::data::project::record::project::ProjectRecord;
use crate::data::project::status::listing;
use crate::Related;
use layerfs_home::disk::{self, paths, with_field};
use layerfs_home::names::StoreName;
use layerfs_home::reply::outcome::{failed, refused, shell_word, with_fix, Outcome};
use layerfs_persistence::diagnostic::ConnectionStep;
use layerfs_protocol::sdk::hex;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
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
                .unwrap_or(crate::backend::postgresql::open::DEFAULT_SYNCHRONOUS_COMMIT)
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
    /// Check as the connect would, save nothing and answer `tested`.
    pub dry_run: bool,
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
    for record in crate::admin::record::store::stores(home)? {
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
    let seen =
        (record.home()).map(|home| crate::admin::status::observed::read(home, record.name()));
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
        .map(|home| crate::admin::status::observed::read(home, record.name()))
        .unwrap_or_default();
    let rows = attached.into_iter().map(|machine| {
        let reach = seen.of(&machine);
        SdkValue::map([
            ("machine", SdkValue::text(machine)),
            (
                "reached",
                crate::admin::status::observed::reached(reach.as_ref()),
            ),
            ("reach", reach.unwrap_or_default()),
        ])
    });
    SdkValue::List(rows.collect())
}

/// When Store `record`'s password was last saved, as an answer's time.
pub fn password_changed(record: &StoreRecord) -> (bool, SdkValue) {
    let saved = (record.home()).and_then(|home| login::read(home, record.name()).ok().flatten());
    let at = saved.as_ref().and_then(|saved| saved.changed_ms);
    (saved.is_some(), crate::admin::status::get::at(at))
}

/// `store.connect`'s answer for `record`: `outcome` (`connected` or
/// `updated`), the projects found on a first connect (the first 20 names,
/// and the count only when there were more), the address, the user, the
/// setting and the attached machines; never the password.
pub fn answer(record: &StoreRecord, outcome: &str, projects: Option<&[String]>) -> SdkValue {
    let (names, count) = match projects {
        Some(found) => layerfs_home::reply::values::shown_names(found),
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

/// `store.connect` of a connected Store, checked from this computer
/// ([`Update`]), or `None` when no Store of the name exists (a first
/// connect, [`connect`]). A sqlite Store of the name is `Exists`.
pub fn prepare(home_dir: &Path, request: &StoreConnect) -> Result<Option<Update>> {
    let name = request.store.as_str();
    let Some(record) = StoreRecord::read(home_dir, name)? else {
        return Ok(None);
    };
    if record.kind()? != StoreKind::Postgresql {
        return Err(refused(
            "Exists",
            format!("a sqlite Store named {name} exists"),
        ));
    }
    update(home_dir, request).map(Some)
}

/// `store.connect` of a new Store: records a PostgreSQL database as Store
/// `request.store`, its user and password required. Every refusal of its
/// input comes before any effect. The connection is checked from this
/// process with the given login ([`check::first`], each refusal naming
/// its step); an empty database is then given the schema and a new id, or
/// the id the schema holds is kept, and its projects are listed, before
/// the home lock; the lock covers the re-checks and the writes only: the
/// record, then the password. A database whose id another Store of this
/// home holds is `Exists`. Once saved, the check's reach is this
/// computer's (`here`); a refused connect records nothing. A dry run stops
/// after the check, before any schema is made, and answers [`tested`]:
/// `here.empty` says the database has no Store schema yet.
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
    let checked = check::first(&probe);
    if request.dry_run {
        let here = match checked {
            Ok((answer, Some(binding))) => {
                unclaimed_by_others(home_dir, name, &binding)?;
                check::tested(&Ok(answer))
            }
            Ok((answer, None)) => {
                with_field(check::tested(&Ok(answer)), "empty", SdkValue::Bool(true))
            }
            Err(refusal) => check::tested(&Err(refusal)),
        };
        return Ok(tested(name, given_url, here, Vec::new()));
    }
    let (checked, found) = checked?;
    // an empty database gets the schema, with a new binding (#533 step
    // 10b); an existing LayerFS schema keeps its binding, read once. The
    // cursor key is this home's own either way.
    let fresh = crate::admin::record::identity::Identity {
        kind: crate::admin::record::identity::Kind::Postgresql {
            schema: SCHEMA.to_owned(),
            synchronous_commit: sync.to_owned(),
        },
        binding: layerfs_home::record::token::random()?.to_vec(),
        cursor_key: layerfs_home::record::token::random()?,
    };
    let binding = match found {
        Some(binding) => binding,
        None => {
            let made = crate::data::project::init::postgresql::create_schema(&probe, &fresh)
                .map_err(|refusal| check::at_step(refusal, ConnectionStep::Schema))?;
            match made {
                true => fresh.binding.clone(),
                false => crate::backend::postgresql::open::read_binding(&probe)
                    .map_err(|refusal| check::at_step(refusal, ConnectionStep::Identity))?,
            }
        }
    };
    let reach = crate::admin::status::observed::Reach::of(&Ok(checked));
    unclaimed_by_others(home_dir, name, &binding)?;
    let record = record.with("binding", SdkValue::Bytes(binding.clone()));
    let record = record.with("cursor", SdkValue::Bytes(fresh.cursor_key.to_vec()));
    let record = record.with_new_login_revision()?;
    let directory = paths::store_dir(home_dir, name);
    // the projects the schema holds become this home's project records; a
    // schema that does not open is not connected
    let open = OpenArgs::dialled(home_dir, name, &record, probe);
    let listed = crate::admin::open::service::with_service(&open, |service| {
        listing::listed(service, false)
    })?;
    let lock = layerfs_home::lock::home::take(home_dir)?;
    if paths::store_record(home_dir, name).exists() {
        return Err(refused("Exists", format!("a Store named {name} exists")));
    }
    unclaimed_by_others(home_dir, name, &binding)?;
    disk::private_dir(&directory)?;
    // the password first: a login revision is never visible without it
    login::save(home_dir, name, password)?;
    if let Err(mut refusal) = record.write(home_dir) {
        // a forget that fails is said: the home then holds a password for
        // a Store it has no record of, which the next connect replaces
        if let Err(back) = login::forget(home_dir, name) {
            refusal.message = format!(
                "{}; its saved password could not be removed either ({})",
                refusal.message, back.message
            );
        }
        return Err(refusal);
    }
    let mut projects = Vec::new();
    for project in listed {
        let stack = SdkValue::text(hex(&project.stack.to_bytes()));
        ProjectRecord::found(name, &project.name, stack).write(home_dir)?;
        projects.push(project.name);
    }
    drop(lock);
    crate::admin::status::observed::record(home_dir, name, None, &reach);
    Ok(answer(&record, "connected", Some(&projects)))
}

/// A dry run's answer: `{ ok, outcome: tested, store, url, here, machines
/// }`, `here` this computer's check ([`check::tested`]) and `machines`
/// each attached machine's, with its `machine`; `ok` when every check
/// passed. Nothing was saved or recorded.
pub fn tested(store: &str, url: &str, here: SdkValue, machines: Vec<SdkValue>) -> SdkValue {
    let passed = |row: &SdkValue| row.get("ok") == Some(&SdkValue::Bool(true));
    let ok = passed(&here) && machines.iter().all(passed);
    SdkValue::map([
        ("ok", SdkValue::Bool(ok)),
        ("outcome", SdkValue::text("tested")),
        ("store", SdkValue::text(store)),
        ("url", SdkValue::text(url)),
        ("here", here),
        ("machines", SdkValue::List(machines)),
    ])
}

/// `store.connect` of a connected Store, checked from this computer and
/// not yet saved: its address, and any of a new user, password and
/// `synchronous_commit` (those not given are kept). No `Debug`: it holds
/// the password.
pub struct Update {
    /// The record as it was read.
    record: StoreRecord,
    /// The record with the new values, its attachments as they were.
    trial: StoreRecord,
    /// The new address with the new login: what this computer dialled and
    /// each attached machine dials.
    dial: String,
    /// A new password, saved with the record.
    new_password: Option<String>,
    /// Whether the address, user or password changes.
    redials: bool,
    /// This computer's check of the new values ([`check::same_store`]):
    /// `{ at, server_version, latency_ms }`, or its refusal naming its
    /// step.
    pub here: Result<SdkValue>,
}

/// Validates a `store.connect` of a connected Store and dials the new
/// values once from this process, where they must read this Store's id
/// (`Unavailable` otherwise, naming the step); nothing is saved
/// ([`Update::save`]).
fn update(home_dir: &Path, request: &StoreConnect) -> Result<Update> {
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
    let redials = record.text("url") != Some(request.url.as_str())
        || record.text("user") != Some(user.as_str())
        || saved.as_ref().map(login::Saved::password) != Some(password.as_str());
    let trial = changed(record.clone(), request, &user);
    let dial = with_login(&request.url, &user, &password);
    layerfs_home::secret::wipe::text(password);
    let dial = dial?;
    let mut probe = selection_at(&trial, dial.clone());
    let here = super::check::same_store(&trial, &probe).map_err(|refusal| SdkRefusal {
        message: format!("PostgreSQL Store {name}: {}", refusal.message),
        ..refusal
    });
    if let Some(url) = probe.url.take() {
        layerfs_home::secret::wipe::text(url);
    }
    Ok(Update {
        record,
        trial,
        dial,
        new_password: new_password.map(str::to_owned),
        redials,
        here,
    })
}

/// `record` with a connect's new address, user and setting.
fn changed(record: StoreRecord, request: &StoreConnect, user: &str) -> StoreRecord {
    let record = record
        .with("url", SdkValue::text(&request.url))
        .with("user", SdkValue::text(user));
    match &request.synchronous_commit {
        Some(sync) => record.with("synchronous_commit", SdkValue::text(sync)),
        None => record,
    }
}

impl Drop for Update {
    fn drop(&mut self) {
        layerfs_home::secret::wipe::text(std::mem::take(&mut self.dial));
        if let Some(password) = self.new_password.take() {
            layerfs_home::secret::wipe::text(password);
        }
    }
}

impl Update {
    /// The record with the new values, its attachments as they were.
    pub fn trial(&self) -> &StoreRecord {
        &self.trial
    }

    /// The new address with the new login, as each machine dials it.
    pub fn dial(&self) -> &str {
        &self.dial
    }

    /// Whether the address, user or password changes (machines dial
    /// them); a change of `synchronous_commit` alone does not.
    pub fn redials(&self) -> bool {
        self.redials
    }

    /// Saves the checked change `request`, refused as this computer's
    /// check was. The record (and a new password) are written under the
    /// home lock and the Store's, `Conflict` when it was disconnected or
    /// connected to another database meanwhile; this process's open
    /// sessions on it are closed. A running Sandbox keeps the connection
    /// it started with. Once saved, the check's reach is this computer's
    /// (`here`).
    pub fn save(mut self, home_dir: &Path, request: &StoreConnect) -> Outcome {
        let name = request.store.as_str();
        let here = std::mem::replace(&mut self.here, Ok(SdkValue::Null));
        let reach = crate::admin::status::observed::Reach::of(&Ok(here?));
        let home_lock = layerfs_home::lock::home::take(home_dir)?;
        let store_lock = crate::admin::record::store::lock(home_dir, name)?;
        let binding = self.record.binding();
        let current = StoreRecord::read(home_dir, name)?.filter(|r| r.binding() == binding);
        let Some(current) = current else {
            return Err(refused(
                "Conflict",
                format!("Store {name} changed while its connection was checked; connect it again"),
            ));
        };
        current.unblocked()?;
        crate::admin::open::close::close(&current.postgresql_key(home_dir, name))?;
        let current = changed(
            current,
            request,
            self.trial.text("user").unwrap_or_default(),
        );
        // every saved change is a new login for the host worker (#819 P-B)
        let replaced = current.login_revision().map(str::to_owned);
        let current = current.with_new_login_revision()?;
        // the password first: a login revision is never visible without it;
        // a record that cannot be written puts the login it replaced back
        let mut before = None;
        if let Some(password) = &self.new_password {
            let kept = login::kept(home_dir, name)?;
            if let Err(refusal) = login::save(home_dir, name, password) {
                if let Some(kept) = kept {
                    layerfs_home::secret::wipe::value(kept);
                }
                return Err(refusal);
            }
            before = Some(kept);
        }
        let written = current.write(home_dir);
        let put = match (&written, &before) {
            (Err(_), Some(kept)) => login::put_back(home_dir, name, kept.as_ref()),
            _ => Ok(()),
        };
        if let Some(Some(kept)) = before {
            layerfs_home::secret::wipe::value(kept);
        }
        if let Err(mut refusal) = written {
            if let Err(back) = put {
                refusal.message = format!(
                    "{}; the password it replaced could not be put back either ({}): \
                     Store {name} now holds the new password with its old login, and \
                     connecting it again settles it",
                    refusal.message, back.message
                );
            }
            return Err(refusal);
        }
        drop((store_lock, home_lock));
        crate::admin::host_worker::postgresql::retire(home_dir, name, replaced.as_deref());
        crate::admin::status::observed::record(home_dir, name, None, &reach);
        Ok(answer(&current, "updated", None))
    }
}

/// `store.disconnect`: forgets a PostgreSQL Store, its saved password and
/// its attachments; its data stays. Refused `InUse` while a process holds
/// it or a Sandbox of its projects exists; a sqlite Store is
/// `InvalidInput`, naming `store remove`.
pub fn disconnect(home_dir: &Path, name: &StoreName, related: Related) -> Outcome {
    // a record this build does not read: its records alone go
    if StoreRecord::read(home_dir, name)?.is_some_and(|record| record.store_ref().is_err()) {
        crate::admin::record::store::forget_unread(home_dir, name, related)?;
        login::forget(home_dir, name)?;
        return Ok(disconnected(name));
    }
    let lock = layerfs_home::lock::home::take(home_dir)?;
    let store = crate::admin::record::store::lock(home_dir, name)?;
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
    crate::admin::open::close::close(&record.postgresql_key(home_dir, name))?;
    std::fs::remove_dir_all(paths::store_dir(home_dir, name)).map_err(failed)?;
    login::forget(home_dir, name)?;
    layerfs_home::record::generation::moved(home_dir);
    // the worker's retire reads the record without a lock; it is sent once
    // this call's locks are released, so no call waits on them meanwhile
    drop((store, lock));
    crate::admin::host_worker::postgresql::retire(home_dir, name, record.login_revision());
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
    crate::backend::postgresql::open::status(&url, &[SCHEMA.to_owned()])
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
        Ok(Some(op)) => crate::admin::maintenance::compact_report::compact_status(&op.status()),
        _ => SdkValue::Null,
    }
}

/// The Sandbox route's `reached` dialling `url`, the Store's address with a login in it
/// (`store.connect` checks a new address or login from each attached
/// machine before it saves them), with the refusal of a URL the machine
/// cannot dial (`layerfs_machine::platform::reach`: a loopback or socket
/// of this host it does not share, or a URL it cannot read to check) kept
/// apart, as `Ok(Err(refusal))`, from every other failure (an
/// unreadable machine record), which is `Err`.
pub fn dialled(
    home: &Path,
    machine: &str,
    record: &StoreRecord,
    url: String,
) -> Result<Result<(PgSelection, Option<String>)>> {
    // a build without machines (the guest's) runs its own Sandboxes on the
    // machine it is, which shares its loopback
    #[cfg(feature = "machine")]
    let (url, hint) = {
        let spec =
            layerfs_machine::machine_spec(home, &layerfs_machine::machine_of(home, machine)?)?;
        let store = record.name();
        let hint = layerfs_machine::platform::loopback_hint(&spec, store, &url);
        match layerfs_machine::platform::reach(&spec, store, &url) {
            Ok(url) => (url, hint),
            Err(error) => return Ok(Err(layerfs_machine::machine_refusal(error))),
        }
    };
    #[cfg(not(feature = "machine"))]
    let hint = {
        let _ = (home, machine);
        None
    };
    Ok(Ok((selection_at(record, url), hint)))
}

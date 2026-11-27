//! A Store's record, `store/<name>/record.json`, read once (#535 §6.1, R4;
//! #536 §9.2): [`StoreRecord`] holds the record and its [`StoreRef`] (its
//! kind, the machine a sqlite Store is on, a PostgreSQL Store's server).
//! Every decision on a Store's kind matches on [`StoreRef`] or
//! [`StoreKind`]; no caller compares a record's `kind` text, and no caller
//! reads or writes a Store record's fields but through [`StoreRecord`].
use crate::admin::record::intent;
use crate::backup::bucket::record::StoreLink;
use layerfs_home::home::entity_lock::{self, EntityLock};
use layerfs_home::home::{self, paths, read_record, with_field, write_entity};
use layerfs_home::op::{self, Op};
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{failed, refused, with_fix, with_next};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
pub use layerfs_protocol::StoreKind;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The record field of a PostgreSQL Store's [`StoreRecord::login_revision`].
const LOGIN_REVISION: &str = "login_revision";

/// The kind a Store record names; a record with no `kind`, or any other
/// text, is refused, never read as sqlite.
pub fn kind_of(record: &SdkValue) -> Result<StoreKind> {
    match record.get("kind") {
        Some(SdkValue::Text(text)) => {
            StoreKind::parse(text).ok_or_else(|| refused("InvalidInput", "Store record kind"))
        }
        _ => Err(refused("InvalidInput", "Store record kind")),
    }
}

/// A PostgreSQL Store's server, as its record names it (no password: the
/// record names its user, and the password is a secret of the home,
/// `postgresql::login`, put into the URL per call). A field the record
/// lacks is refused only by the step that needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PgServer {
    pub schema: Option<String>,
    pub synchronous_commit: Option<String>,
}

/// One Store, decided once from `store/<name>/record.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreRef {
    /// A sqlite Store on `machine` (the machine whose calls run in this
    /// process when the record names none).
    Sqlite {
        name: String,
        machine: String,
    },
    Postgres {
        name: String,
        server: PgServer,
    },
}

/// The fix of Store `name` whose record this build does not read (no
/// `kind`, one it does not know, or a `machine` that is not text):
/// `store remove`, which then removes its records alone.
pub fn unread_fix(name: &str) -> String {
    format!("layerfs store remove {}", shell_word(name))
}

impl StoreRef {
    /// Parses Store `name`'s record; its kind, or a sqlite Store's
    /// `machine` that is not text, refuses it, naming [`unread_fix`].
    pub fn from_record(name: &str, record: &SdkValue) -> Result<Self> {
        let unread = |refusal| with_fix(refusal, unread_fix(name));
        let kind = kind_of(record).map_err(unread)?;
        let name = name.to_owned();
        Ok(match kind {
            StoreKind::Sqlite => Self::Sqlite {
                // none: the Store's file is in this process's home, on the
                // machine whose calls run here (refused on a host with none)
                machine: match record.get("machine") {
                    None => layerfs_machine::in_process()
                        .ok_or_else(|| {
                            unread(layerfs_machine::none_in_process(
                                "a sqlite Store record that names no machine",
                            ))
                        })?
                        .to_owned(),
                    Some(SdkValue::Text(machine)) => machine.clone(),
                    Some(_) => return Err(unread(refused("InvalidInput", "Store record machine"))),
                },
                name,
            },
            StoreKind::Postgresql => Self::Postgres {
                name,
                server: PgServer {
                    schema: record.get_text("schema").map(str::to_owned),
                    synchronous_commit: record.get_text("synchronous_commit").map(str::to_owned),
                },
            },
        })
    }

    pub fn kind(&self) -> StoreKind {
        match self {
            Self::Sqlite { .. } => StoreKind::Sqlite,
            Self::Postgres { .. } => StoreKind::Postgresql,
        }
    }

    /// The machine a sqlite Store is on. A PostgreSQL Store is on none.
    pub fn machine(&self) -> Result<&str> {
        match self {
            Self::Sqlite { machine, .. } => Ok(machine),
            Self::Postgres { name, .. } => Err(refused(
                "InvalidInput",
                format!("Store {name} is a PostgreSQL database; it is on no machine"),
            )),
        }
    }
}

/// A field a step needs, refused as a malformed Store record when absent.
pub fn required(field: Option<String>, key: &str) -> Result<String> {
    field.ok_or_else(|| refused("InvalidInput", format!("Store record {key}")))
}

/// One Store's record as read: its fields in their written order, kept
/// whole so a result shows every field it showed before, and its
/// [`StoreRef`], parsed once. A record whose kind is not one LayerFS writes
/// is read; only a step that needs the kind refuses it, as before. A
/// record's `v` (written from layout 3) is read past and never shown.
#[derive(Clone, Debug)]
pub struct StoreRecord {
    name: String,
    parsed: Result<StoreRef>,
    fields: SdkValue,
    /// The home it was read from, where a PostgreSQL Store's password is
    /// saved; none for a record a worker was sent.
    home: Option<std::path::PathBuf>,
}

/// Takes Store `name`'s lock, `store/<name>/lock` (#536 §8): it guards the
/// Store's record changes and lease grants. Taken after the home lock,
/// never before it, and never held across a worker call or a byte
/// transfer. A Store with no folder is `NotFound`.
pub fn lock(home: &Path, name: &str) -> Result<EntityLock> {
    take_lock(home, name, entity_lock::WAIT)
}

/// A management slot must be available for other work while a record is held.
pub(crate) fn lock_for_host_close(home: &Path, name: &str) -> Result<EntityLock> {
    take_lock(home, name, std::time::Duration::ZERO).map_err(|error| {
        if error.code != "Unavailable" {
            return error;
        }
        let error = refused("InUse", format!("Store {name}'s record lock is held"));
        let error = with_next(error, "store", SdkValue::text(name));
        with_next(error, "state", SdkValue::text("record-locked"))
    })
}

fn take_lock(home: &Path, name: &str, wait: std::time::Duration) -> Result<EntityLock> {
    let dir = paths::store_dir(home, name);
    if !dir.is_dir() {
        return Err(refused("NotFound", format!("no Store named {name}")));
    }
    let held = format!("Store {name}'s lock was held");
    entity_lock::take(&dir.join("lock"), wait, &held)
}

/// The record field listing the machines attached to a PostgreSQL Store.
pub const ATTACHED: &str = "attached";

/// The profile label a new sqlite Store's record is written with: the WAL
/// Store opened with NORMAL synchronization (owner, 2026-11-23).
pub const PROFILE: &str = "normal";

/// A sqlite Store record's profile label as callers show it. A record written
/// before 2026-11-23 says `disposable`, the earlier name of the same Store;
/// it reads as `normal`, and the record is never rewritten for it.
pub fn profile(record: &StoreRecord) -> Option<&str> {
    match record.text("profile") {
        Some("disposable") => Some(PROFILE),
        other => other,
    }
}

impl StoreRecord {
    /// Store `name`'s record `value`, as read.
    pub fn from_record(name: &str, value: SdkValue) -> Self {
        Self {
            name: name.to_owned(),
            parsed: StoreRef::from_record(name, &value),
            fields: value,
            home: None,
        }
    }

    /// Store `name`'s record in `home` (or a worker's root), if it has one.
    pub fn read(home: &Path, name: &str) -> Result<Option<Self>> {
        let read = read_record(&paths::store_record(home, name))?;
        Ok(read.map(|v| Self::from_record(name, v).in_home(home)))
    }

    /// This record, as one of `home`'s: its password is read there.
    pub fn in_home(self, home: &Path) -> Self {
        Self {
            home: Some(home.to_path_buf()),
            ..self
        }
    }

    /// The home it was read from, if it was.
    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    /// Store `name`'s record; a bad name is `InvalidInput`, no record
    /// `NotFound`.
    pub fn of(home: &Path, name: &str) -> Result<Self> {
        layerfs_home::names::StoreName::parse(name)?;
        Self::read(home, name)?.ok_or_else(|| refused("NotFound", format!("no Store named {name}")))
    }

    /// The Store's name (its folder's).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The parsed Store; a kind LayerFS does not write is refused here.
    pub fn store_ref(&self) -> Result<&StoreRef> {
        self.parsed.as_ref().map_err(Clone::clone)
    }

    /// Checks the admission fence of this Store's local host, when it has one.
    pub(crate) fn admit_machine(&self, home: &Path) -> Result<()> {
        if let StoreRef::Sqlite { machine, .. } = self.store_ref()? {
            layerfs_machine::record::MachineRecord::admit_host(home, machine)?;
        }
        Ok(())
    }

    /// The Store's kind.
    pub fn kind(&self) -> Result<StoreKind> {
        Ok(self.store_ref()?.kind())
    }

    /// The machine a sqlite Store is on (see [`StoreRef::machine`]).
    pub fn machine(&self) -> Result<String> {
        self.store_ref()?.machine().map(str::to_owned)
    }

    /// The record as written, for writing it or sending it to a worker.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The record with field `field` set to `value` (added when absent).
    pub fn with(self, field: &str, value: SdkValue) -> Self {
        Self {
            home: self.home,
            ..Self::from_record(&self.name, with_field(self.fields, field, value))
        }
    }

    /// Writes the record to `store/<name>/record.json` under `home`.
    pub fn write(&self, home: &Path) -> Result<()> {
        write_entity(&paths::store_record(home, &self.name), &self.fields)
    }

    /// The record's field `field` as written.
    pub fn field(&self, field: &str) -> Option<&SdkValue> {
        self.fields.get(field)
    }

    /// The text field `field` (a PostgreSQL Store's `url`, `user`,
    /// `synchronous_commit`).
    pub fn text(&self, field: &str) -> Option<&str> {
        self.fields.get_text(field)
    }

    /// The path a process's open-Store cache keys this PostgreSQL Store by
    /// under `home` (a path no file has): its name and its schema binding,
    /// so a name connected again to another database never reuses the
    /// service still open on the old one (lane K2, terminal pass).
    pub fn postgresql_key(&self, home: &Path, store: &str) -> std::path::PathBuf {
        let binding = self.binding().unwrap_or_default();
        let bound = layerfs_protocol::sdk::hex(&binding[..binding.len().min(8)]);
        paths::store_dir(home, store).join(format!("postgresql-{bound}"))
    }

    /// The Store's binding, as its record holds it.
    pub fn binding(&self) -> Option<&[u8]> {
        match self.fields.get("binding") {
            Some(SdkValue::Bytes(bytes)) => Some(bytes),
            _ => None,
        }
    }

    /// A PostgreSQL Store's login revision (#819 P-B): a non-secret token
    /// `store connect` writes with every login it saves and an upgrade
    /// renews. A host worker serves a call only at the revision the record
    /// holds now, and never on a session opened at another one.
    pub fn login_revision(&self) -> Option<&str> {
        self.text(LOGIN_REVISION)
            .filter(|revision| !revision.is_empty())
    }

    /// The record with a new [`Self::login_revision`].
    pub fn with_new_login_revision(self) -> Result<Self> {
        let token = layerfs_protocol::sdk::hex(&layerfs_home::helpers::random()?[..16]);
        Ok(self.with(LOGIN_REVISION, SdkValue::text(token)))
    }

    /// The record without field `field`.
    pub fn without(self, field: &str) -> Self {
        let fields = match self.fields {
            SdkValue::Map(fields) => fields.into_iter().filter(|(n, _)| n != field).collect(),
            _ => Vec::new(),
        };
        Self {
            home: self.home,
            ..Self::from_record(&self.name, SdkValue::Map(fields))
        }
    }

    /// The operation the record holds (its `op`), if any.
    pub fn op(&self) -> Option<Op> {
        Op::of(&self.fields)
    }

    /// Refuses a lease or a Store call while the record's `op` holds the
    /// Store to itself (a move or a remove, and any operation this build
    /// does not name): `Busy` while its process lives (this process
    /// included: a move's own steps take no lease and make no Store call,
    /// so nothing else in its process may either), `Unresolved` once a gone
    /// process left its outcome unknown. A gone process's certain failure
    /// holds nothing, and neither does a compaction or a backup: each is
    /// recorded so every client sees `busy`, and the Store's own admission
    /// decides what may run beside it.
    pub fn unblocked(&self) -> Result<()> {
        match self.op() {
            Some(op) if intent::blocks(&op) => self.refuse(&op),
            _ => Ok(()),
        }
    }

    /// Refuses a new operation while the record's `op` holds the Store,
    /// whatever the operation: [`Self::unblocked`], and a compaction or a
    /// backup too.
    pub fn unheld(&self) -> Result<()> {
        self.op().map_or(Ok(()), |op| self.refuse(&op))
    }

    /// The refusal `op` makes of a new operation: `Busy` or `Unresolved`,
    /// naming the call that clears an unresolved one as its `fix`, with the
    /// operation as the caller's next step reads it.
    fn refuse(&self, op: &Op) -> Result<()> {
        let clear = intent::clearing(op, &self.name);
        op.blocks("store", &self.name, "store get", "store remove")
            .map_err(|mut refusal| {
                let named = format!("`layerfs store remove {}`", self.name);
                refusal.message = refusal
                    .message
                    .replace(&named, &format!("`layerfs {clear}`"));
                if refusal.code == "Unresolved" {
                    refusal = with_fix(refusal, format!("layerfs {clear}"));
                }
                with_next(refusal, op::FIELD, intent::shown(op))
            })
    }

    /// The machines attached to a PostgreSQL Store (its record's
    /// `attached`), in record order: the machines its Sandboxes may run on
    /// besides this computer, each dialling the Store's URL with its login.
    pub fn attached(&self) -> Vec<String> {
        match self.fields.get(ATTACHED) {
            Some(SdkValue::List(machines)) => (machines.iter())
                .filter_map(|machine| match machine {
                    SdkValue::Text(machine) => Some(machine.clone()),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether `machine` is attached to the Store.
    pub fn is_attached(&self, machine: &str) -> bool {
        self.attached().iter().any(|m| m == machine)
    }

    /// The record with `machine` attached (`attach`) or not; no
    /// `attached` field when none is.
    pub fn with_attached(self, machine: &str, attach: bool) -> Self {
        let mut machines: Vec<String> = self.attached();
        machines.retain(|m| m != machine);
        if attach {
            machines.push(machine.to_owned());
        }
        match machines.is_empty() {
            true => self.without(ATTACHED),
            false => {
                let machines = machines.into_iter().map(SdkValue::Text).collect();
                self.with(ATTACHED, SdkValue::List(machines))
            }
        }
    }

    fn raw_links(&self) -> &[(String, SdkValue)] {
        match self.fields.get("backups") {
            Some(SdkValue::Map(links)) => links,
            _ => &[],
        }
    }

    /// The Store's link to each bucket, `(bucket, link)` in record order.
    pub fn links(&self) -> Vec<(String, StoreLink)> {
        let links = self.raw_links().iter();
        links
            .map(|(bucket, link)| (bucket.clone(), StoreLink::from_record(link.clone())))
            .collect()
    }

    /// The Store's link to `bucket`.
    pub fn link(&self, bucket: &str) -> Option<StoreLink> {
        let link = self.fields.get("backups").and_then(|b| b.get(bucket));
        link.cloned().map(StoreLink::from_record)
    }

    /// The record with its link to `bucket` set to `link`.
    pub fn with_link(self, bucket: &str, link: &StoreLink) -> Self {
        let links = SdkValue::Map(self.raw_links().to_vec());
        let links = with_field(links, bucket, link.to_record().clone());
        self.with("backups", links)
    }

    /// The record without a link to `bucket`, or `None` when it has none.
    pub fn without_link(self, bucket: &str) -> Option<Self> {
        if !self.raw_links().iter().any(|(b, _)| b == bucket) {
            return None;
        }
        let kept = self
            .raw_links()
            .iter()
            .filter(|(b, _)| b != bucket)
            .cloned();
        let kept = SdkValue::Map(kept.collect());
        Some(self.with("backups", kept))
    }

    /// A new sqlite Store's record on `machine`: this record (its identity)
    /// with its machine, profile and creation time; its SQLite version is
    /// added once its file is made ([`Self::with`] `sqlite`).
    pub fn created_on(self, machine: &str) -> Self {
        let mut record = self;
        for (field, value) in [
            ("machine", SdkValue::text(machine)),
            ("profile", SdkValue::text(PROFILE)),
            ("created", SdkValue::Int(home::now_ms())),
        ] {
            record = record.with(field, value);
        }
        record
    }

    /// A sqlite Store restored from a bucket as a new Store on `machine`,
    /// one whose calls run in this process.
    pub fn restored(name: &str, machine: &str, binding: &[u8], cursor: &[u8; 32]) -> Self {
        let fields = SdkValue::map([
            ("name", SdkValue::text(name)),
            ("kind", SdkValue::text("sqlite")),
            ("binding", SdkValue::Bytes(binding.to_vec())),
            ("cursor", SdkValue::Bytes(cursor.to_vec())),
            ("machine", SdkValue::text(machine)),
            ("profile", SdkValue::text(PROFILE)),
            ("created", SdkValue::Int(home::now_ms())),
        ]);
        Self::from_record(name, fields)
    }

    /// A PostgreSQL Store `store.connect` records: its URL without a user
    /// or password, its user, the schema and `synchronous_commit` (its
    /// binding and cursor key are added once read).
    pub fn connected(
        name: &str,
        url: &str,
        user: &str,
        schema: &str,
        synchronous_commit: &str,
    ) -> Self {
        let fields = SdkValue::map([
            ("name", SdkValue::text(name)),
            ("kind", SdkValue::text("postgresql")),
            ("url", SdkValue::text(url)),
            ("user", SdkValue::text(user)),
            ("schema", SdkValue::text(schema)),
            ("synchronous_commit", SdkValue::text(synchronous_commit)),
            ("created", SdkValue::Int(home::now_ms())),
        ]);
        Self::from_record(name, fields)
    }
}

/// The full URL of a PostgreSQL Store's record, with its login put in
/// now: what this computer and every attached machine dial.
pub fn url(record: &StoreRecord) -> Result<String> {
    let url = record.text("url").ok_or_else(|| {
        refused(
            "Failed",
            format!("PostgreSQL Store {}'s record has no url", record.name()),
        )
    })?;
    crate::backend::postgresql::login::dial(record, url)
}

/// Every Store record, by name.
pub fn stores(home: &Path) -> Result<Vec<StoreRecord>> {
    let mut found = Vec::new();
    for store in paths::names(&home.join("store")).map_err(failed)? {
        found.extend(StoreRecord::read(home, &store)?);
    }
    Ok(found)
}

/// Removes the records of Store `name` whose record this build does not
/// read (no `kind`, one it does not know, or a `machine` that is not text):
/// under the home lock and the Store's, once nothing uses it, its record
/// and its projects' records go. Nothing it names is opened or deleted: any
/// other file in its folder (a Store file) stays where it is, and the
/// answer (`left`) says so, with the line that removes it.
pub(crate) fn forget_unread(
    home: &Path,
    name: &str,
    related: crate::Related,
) -> Result<Vec<String>> {
    let _lock = home::lock::take(home)?;
    let _store = lock(home, name)?;
    let now = StoreRecord::of(home, name)?;
    if now.store_ref().is_ok() {
        return Err(refused(
            "Conflict",
            format!("Store {name}'s record changed meanwhile; nothing was removed"),
        ));
    }
    (related.store_idle)(home, name)?;
    std::fs::remove_file(paths::store_record(home, name)).map_err(failed)?;
    let projects = paths::store_dir(home, name).join("project");
    match std::fs::remove_dir_all(projects) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(failed(error)),
    }
    layerfs_home::home::generation::moved(home);
    let file = paths::store_file(home, name);
    if !file.exists() {
        return Ok(Vec::new());
    }
    let shown = shell_word(&file.display().to_string());
    Ok(vec![format!(
        "Store {name}'s file {} was kept, since its record was not read: `rm -f {shown}*` \
         removes it and its -wal and -shm files",
        file.display()
    )])
}

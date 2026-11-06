//! A Store's record, `store/<name>/record.json`, read once (#535 §6.1, R4;
//! #536 §9.2): [`StoreRecord`] holds the record and its [`StoreRef`] (its
//! kind, the machine a sqlite Store is on, a PostgreSQL Store's server).
//! Every decision on a Store's kind matches on [`StoreRef`] or
//! [`StoreKind`]; no caller compares a record's `kind` text, and no caller
//! reads or writes a Store record's fields but through [`StoreRecord`].
use crate::backup::record::StoreLink;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
pub(crate) use layerfs_bridge::StoreKind;
use layerfs_home::home::entity_lock::{self, EntityLock};
use layerfs_home::home::{self, paths, read_record, with_field, write_entity};
use layerfs_home::op::{self, Op, Standing};
use layerfs_home::outcome::{refused, View};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The kind a Store record names. A record with no `kind` is a sqlite
/// Store (the records #506 wrote before PostgreSQL); any other text is
/// refused, never read as sqlite.
pub(crate) fn kind_of(record: &SdkValue) -> Result<StoreKind> {
    match record.get("kind") {
        None => Ok(StoreKind::Sqlite),
        Some(SdkValue::Text(text)) => {
            StoreKind::parse(text).ok_or_else(|| refused("InvalidInput", "Store record kind"))
        }
        Some(_) => Err(refused("InvalidInput", "Store record kind")),
    }
}

/// Where a sqlite Store's record places it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    /// On this machine (`native` when the record names none).
    Machine(String),
    /// A 1.1.0 home's Store whose machine the conversion could not tell.
    Unknown,
}

/// A PostgreSQL Store's server, as its record names it (no password: the
/// record holds only a reference, resolved per call). A field the record
/// lacks is refused only by the step that needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PgServer {
    pub(crate) schema: Option<String>,
    pub(crate) synchronous_commit: Option<String>,
}

/// One Store, decided once from `store/<name>/record.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StoreRef {
    Sqlite { name: String, machine: Placement },
    Postgres { name: String, server: PgServer },
}

impl StoreRef {
    /// Parses Store `name`'s record; only its kind can refuse it.
    pub(crate) fn from_record(name: &str, record: &SdkValue) -> Result<Self> {
        let name = name.to_owned();
        Ok(match kind_of(record)? {
            StoreKind::Sqlite => Self::Sqlite {
                name,
                machine: match record.get("machine") {
                    None => Placement::Machine("native".to_owned()),
                    Some(SdkValue::Text(machine)) => Placement::Machine(machine.clone()),
                    Some(_) => Placement::Unknown,
                },
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

    pub(crate) fn kind(&self) -> StoreKind {
        match self {
            Self::Sqlite { .. } => StoreKind::Sqlite,
            Self::Postgres { .. } => StoreKind::Postgresql,
        }
    }

    /// The machine a sqlite Store is on. A PostgreSQL Store is on none, and
    /// a 1.1.0 Store whose machine is unknown is refused naming the fix.
    pub(crate) fn machine(&self) -> Result<&str> {
        match self {
            Self::Sqlite {
                machine: Placement::Machine(machine),
                ..
            } => Ok(machine),
            Self::Sqlite {
                name,
                machine: Placement::Unknown,
            } => Err(refused(
                "Unavailable",
                format!(
                    "Store {name} came from a 1.1.0 home and its machine is not known; \
                     run `layerfs import-env`"
                ),
            )),
            Self::Postgres { name, .. } => Err(refused(
                "InvalidInput",
                format!("Store {name} is a PostgreSQL database; it is on no machine"),
            )),
        }
    }
}

/// A field a step needs, refused as a malformed Store record when absent.
pub(crate) fn required(field: Option<String>, key: &str) -> Result<String> {
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
}

/// What a Store's result shows beside its record.
#[derive(Clone, Debug, Default)]
pub struct StoreDerived {
    /// The names of its projects' records, sorted.
    pub projects: Vec<String>,
    /// Its state, shown by a list item (a get shows none).
    pub state: Option<StoreState>,
}

/// A Store's state (#536 §7.2), as `store.status` and each `store.list`
/// item show it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreState {
    /// No other process holds it.
    Idle,
    /// Another live process holds its lease.
    InUse,
    /// A live process runs an operation on it (its record's `op`).
    Busy,
    /// Its machine did not answer (a sqlite Store's file could not be read).
    Down,
    /// A gone process left an operation on it whose outcome is not known.
    Unresolved,
}

impl StoreState {
    /// The state's name in results.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::InUse => "in_use",
            Self::Busy => "busy",
            Self::Down => "down",
            Self::Unresolved => "unresolved",
        }
    }
}

/// Takes Store `name`'s lock, `store/<name>/lock` (#536 §8): it guards the
/// Store's record changes and lease grants. Taken after the home lock,
/// never before it, and never held across a worker call or a byte
/// transfer. A Store with no folder is `NotFound`.
pub(crate) fn lock(home: &Path, name: &str) -> Result<EntityLock> {
    let dir = paths::store_dir(home, name);
    if !dir.is_dir() {
        return Err(refused("NotFound", format!("no Store named {name}")));
    }
    let held = format!("Store {name}'s lock was held");
    entity_lock::take(&dir.join("lock"), entity_lock::WAIT, &held)
}

impl StoreRecord {
    /// Store `name`'s record `value`, as read.
    pub fn from_record(name: &str, value: SdkValue) -> Self {
        Self {
            name: name.to_owned(),
            parsed: StoreRef::from_record(name, &value),
            fields: value,
        }
    }

    /// Store `name`'s record in `home` (or a worker's root), if it has one.
    pub(crate) fn read(home: &Path, name: &str) -> Result<Option<Self>> {
        Ok(read_record(&paths::store_record(home, name))?.map(|v| Self::from_record(name, v)))
    }

    /// Store `name`'s record; a bad name is `InvalidInput`, no record
    /// `NotFound`.
    pub(crate) fn of(home: &Path, name: &str) -> Result<Self> {
        layerfs_home::names::StoreName::parse(name)?;
        Self::read(home, name)?.ok_or_else(|| refused("NotFound", format!("no Store named {name}")))
    }

    /// The Store's name (its folder's).
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The parsed Store; a kind LayerFS does not write is refused here.
    pub(crate) fn store_ref(&self) -> Result<&StoreRef> {
        self.parsed.as_ref().map_err(Clone::clone)
    }

    /// The Store's kind.
    pub(crate) fn kind(&self) -> Result<StoreKind> {
        Ok(self.store_ref()?.kind())
    }

    /// The machine a sqlite Store is on (see [`StoreRef::machine`]).
    pub(crate) fn machine(&self) -> Result<String> {
        self.store_ref()?.machine().map(str::to_owned)
    }

    /// The record as written, for writing it or sending it to a worker.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The record with field `field` set to `value` (added when absent).
    pub(crate) fn with(self, field: &str, value: SdkValue) -> Self {
        Self::from_record(&self.name, with_field(self.fields, field, value))
    }

    /// Writes the record to `store/<name>/record.json` under `home`.
    pub(crate) fn write(&self, home: &Path) -> Result<()> {
        write_entity(&paths::store_record(home, &self.name), &self.fields)
    }

    /// The record's field `field` as written.
    pub(crate) fn field(&self, field: &str) -> Option<&SdkValue> {
        self.fields.get(field)
    }

    /// The text field `field` (a PostgreSQL Store's `url`, `password`,
    /// `synchronous_commit`).
    pub(crate) fn text(&self, field: &str) -> Option<&str> {
        self.fields.get_text(field)
    }

    /// The path a process's open-Store cache keys this PostgreSQL Store by
    /// under `home` (a path no file has): its name and its schema binding,
    /// so a name connected again to another database never reuses the
    /// service still open on the old one (lane K2, terminal pass).
    pub(crate) fn postgresql_key(&self, home: &Path, store: &str) -> std::path::PathBuf {
        let binding = self.binding().unwrap_or_default();
        let bound = layerfs_bridge::sdk_values::hex(&binding[..binding.len().min(8)]);
        paths::store_dir(home, store).join(format!("postgresql-{bound}"))
    }

    /// The Store's binding, as its record holds it.
    pub(crate) fn binding(&self) -> Option<&[u8]> {
        match self.fields.get("binding") {
            Some(SdkValue::Bytes(bytes)) => Some(bytes),
            _ => None,
        }
    }

    /// The record without field `field`.
    pub(crate) fn without(self, field: &str) -> Self {
        let fields = match self.fields {
            SdkValue::Map(fields) => fields.into_iter().filter(|(n, _)| n != field).collect(),
            _ => Vec::new(),
        };
        Self::from_record(&self.name, SdkValue::Map(fields))
    }

    /// The operation the record holds (its `op`), if any.
    pub(crate) fn op(&self) -> Option<Op> {
        Op::of(&self.fields)
    }

    /// Refuses a new operation, a lease or a Store call while the record's
    /// `op` holds the Store: `Busy` while its process lives (this process
    /// included: a push's or pull's own steps take no lease and make no Store call,
    /// so nothing else in its process may either), `Unresolved` once a gone
    /// process left its outcome unknown. A gone process's certain failure
    /// holds nothing. An unresolved push or pull names a new one as the
    /// call that clears it (H6), anything else `store remove`.
    pub(crate) fn unblocked(&self) -> Result<()> {
        let Some(op) = self.op() else {
            return Ok(());
        };
        let clear = clearing_call(&op, &self.name);
        op.blocks("store", &self.name, "store status", "store remove")
            .map_err(|mut refusal| {
                let named = format!("`layerfs store remove {}`", self.name);
                refusal.message = refusal
                    .message
                    .replace(&named, &format!("`layerfs {clear}`"));
                refusal
            })
    }

    fn raw_links(&self) -> &[(String, SdkValue)] {
        match self.fields.get("backups") {
            Some(SdkValue::Map(links)) => links,
            _ => &[],
        }
    }

    /// The Store's state from its record, whether its machine answered
    /// (`down`) and the live processes other than this one that hold it.
    pub fn state(&self, down: bool, holders: &[u32]) -> StoreState {
        match self.op().map(|op| op.standing()) {
            Some(Standing::Running) => return StoreState::Busy,
            Some(Standing::Unresolved) => return StoreState::Unresolved,
            _ => {}
        }
        if down {
            StoreState::Down
        } else if holders.is_empty() {
            StoreState::Idle
        } else {
            StoreState::InUse
        }
    }

    /// The Store's link to each bucket, `(bucket, link)` in record order.
    pub(crate) fn links(&self) -> Vec<(String, StoreLink)> {
        let links = self.raw_links().iter();
        links
            .map(|(bucket, link)| (bucket.clone(), StoreLink::from_record(link.clone())))
            .collect()
    }

    /// The Store's link to `bucket`.
    pub(crate) fn link(&self, bucket: &str) -> Option<StoreLink> {
        let link = self.fields.get("backups").and_then(|b| b.get(bucket));
        link.cloned().map(StoreLink::from_record)
    }

    /// The record with its link to `bucket` set to `link`.
    pub(crate) fn with_link(self, bucket: &str, link: &StoreLink) -> Self {
        let links = SdkValue::Map(self.raw_links().to_vec());
        let links = with_field(links, bucket, link.to_record().clone());
        self.with("backups", links)
    }

    /// The record without a link to `bucket`, or `None` when it has none.
    pub(crate) fn without_link(self, bucket: &str) -> Option<Self> {
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

    /// The Store as `store.get` and each `store.list` item show it: the
    /// record's fields with `name` first, never its cursor key, binding or
    /// `v`, its `op` as H6 shows it, then its projects; a list item then
    /// its `state`.
    pub fn to_sdk(&self, view: View, derived: &StoreDerived) -> Result<SdkValue> {
        let SdkValue::Map(fields) = &self.fields else {
            return Err(refused(
                "Failed",
                format!("Store {}'s record is not a record", self.name),
            ));
        };
        let mut out: Vec<(String, SdkValue)> = fields
            .iter()
            .filter(|(field, _)| !matches!(field.as_str(), "cursor" | "binding" | "v"))
            .cloned()
            .collect();
        if let (Some(op), Some(slot)) = (self.op(), out.iter_mut().find(|(f, _)| f == op::FIELD)) {
            slot.1 = op.to_sdk();
        }
        out.retain(|(field, _)| field != "name");
        out.insert(0, ("name".to_owned(), SdkValue::text(&self.name)));
        let names = derived.projects.iter().map(SdkValue::text).collect();
        out.push(("projects".to_owned(), SdkValue::List(names)));
        if let (View::List, Some(state)) = (view, derived.state) {
            out.push(("state".to_owned(), SdkValue::text(state.as_str())));
        }
        Ok(SdkValue::Map(out))
    }

    /// A new sqlite Store's record on `native`: this record (its identity)
    /// with its machine, profile, creation time and SQLite version.
    pub(crate) fn created_on_native(self, sqlite_version: &str) -> Self {
        let mut record = self;
        for (field, value) in [
            ("machine", SdkValue::text("native")),
            ("profile", SdkValue::text("disposable")),
            ("created", SdkValue::Int(home::now_ms())),
            ("sqlite", SdkValue::text(sqlite_version)),
        ] {
            record = record.with(field, value);
        }
        record
    }

    /// A sqlite Store restored from a bucket as a new Store on `native`.
    pub(crate) fn restored(name: &str, binding: &[u8], cursor: &[u8; 32]) -> Self {
        let fields = SdkValue::map([
            ("name", SdkValue::text(name)),
            ("kind", SdkValue::text("sqlite")),
            ("binding", SdkValue::Bytes(binding.to_vec())),
            ("cursor", SdkValue::Bytes(cursor.to_vec())),
            ("machine", SdkValue::text("native")),
            ("profile", SdkValue::text("disposable")),
            ("created", SdkValue::Int(home::now_ms())),
        ]);
        Self::from_record(name, fields)
    }

    /// A PostgreSQL Store `store.connect` records: its URL without a
    /// password, the schema, `synchronous_commit` and the password's
    /// reference, if any (its binding and cursor key are added once read).
    pub(crate) fn connected(
        name: &str,
        url: &str,
        schema: &str,
        synchronous_commit: &str,
        password: Option<&str>,
    ) -> Self {
        let fields = SdkValue::map([
            ("name", SdkValue::text(name)),
            ("kind", SdkValue::text("postgresql")),
            ("url", SdkValue::text(url)),
            ("schema", SdkValue::text(schema)),
            ("synchronous_commit", SdkValue::text(synchronous_commit)),
            ("created", SdkValue::Int(home::now_ms())),
        ]);
        let record = Self::from_record(name, fields);
        match password {
            Some(password) => record.with("password", SdkValue::text(password)),
            None => record,
        }
    }
}

/// The call that clears Store `name`'s intent `op` once its process is
/// gone, with its arguments: a new `store push <name> <machine>` or `store
/// pull <name>` for a push or pull (it deletes the copy the record does not
/// name first), else `store remove <name>`.
pub(crate) fn clearing_call(op: &Op, name: &str) -> String {
    match (op.name(), op.args().get_text("to")) {
        ("store.push", Some(to)) => format!("store push {name} {to}"),
        ("store.pull", _) => format!("store pull {name}"),
        _ => format!("store remove {name}"),
    }
}

/// The full URL of a PostgreSQL Store's record, its password resolved now.
pub(crate) fn url(record: &StoreRecord) -> Result<String> {
    let url = record.text("url").ok_or_else(|| {
        refused(
            "Failed",
            format!("PostgreSQL Store {}'s record has no url", record.name()),
        )
    })?;
    layerfs_home::secrets::with_password(url, record.text("password"))
}

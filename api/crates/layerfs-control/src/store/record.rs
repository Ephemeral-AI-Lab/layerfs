//! A Store's record, `store/<name>/record.json`, read once (#535 §6.1, R4;
//! #536 §9.2): [`StoreRecord`] holds the record and its [`StoreRef`] (its
//! kind, the machine a sqlite Store is on, a PostgreSQL Store's server).
//! Every decision on a Store's kind matches on [`StoreRef`] or
//! [`StoreKind`]; no caller compares a record's `kind` text, and no caller
//! reads or writes a Store record's fields but through [`StoreRecord`].
use crate::backup::record::StoreLink;
use crate::common::home::{self, lease, paths, read_record, with_field, write_record};
use crate::common::outcome::{refused, View};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
pub(crate) use layerfs_bridge::StoreKind;
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
                     run `layerfs import-env` or `layerfs store move {name} <machine>`"
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

/// What a Store's result shows beside its record: its projects.
#[derive(Clone, Debug, Default)]
pub struct StoreDerived {
    /// The names of its projects' records, sorted.
    pub projects: Vec<String>,
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
        crate::common::names::StoreName::parse(name)?;
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
        write_record(&paths::store_record(home, &self.name), &self.fields)
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

    /// The Store's binding, as its record holds it.
    pub(crate) fn binding(&self) -> Option<&[u8]> {
        match self.fields.get("binding") {
            Some(SdkValue::Bytes(bytes)) => Some(bytes),
            _ => None,
        }
    }

    /// The record's `in_progress`: the move or remove that marked it.
    pub(crate) fn in_progress(&self) -> Option<&SdkValue> {
        self.fields.get("in_progress")
    }

    /// Refuses `InUse` while a live process moves or removes the Store (its
    /// record's `in_progress`), this process included: a move's own steps
    /// take no lease and make no Store call, so nothing else in its process
    /// may either. A move whose process is gone does not block.
    pub(crate) fn moving(&self) -> Result<()> {
        let Some(progress @ SdkValue::Map(_)) = self.in_progress() else {
            return Ok(());
        };
        if lease::recorded_alive(progress) {
            let pid = progress.get_int("pid").unwrap_or(0);
            let doing = match progress.get_text("op") {
                Some("remove") => "removed",
                _ => "moved",
            };
            return Err(refused(
                "InUse",
                format!("Store {} is being {doing} by process {pid}", self.name),
            ));
        }
        Ok(())
    }

    fn raw_links(&self) -> &[(String, SdkValue)] {
        match self.fields.get("backups") {
            Some(SdkValue::Map(links)) => links,
            _ => &[],
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
    /// `v`, then its projects.
    pub fn to_sdk(&self, _view: View, derived: &StoreDerived) -> Result<SdkValue> {
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
        out.retain(|(field, _)| field != "name");
        out.insert(0, ("name".to_owned(), SdkValue::text(&self.name)));
        let names = derived.projects.iter().map(SdkValue::text).collect();
        out.push(("projects".to_owned(), SdkValue::List(names)));
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

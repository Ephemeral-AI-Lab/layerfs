//! A project's record, `store/<store>/project/<project>/record.json` (#536
//! §9.2): its name, its LayerStack (a PostgreSQL Store's) and when it was
//! made. The record also holds the sqlite project's manifest, which
//! `sandbox/files.rs` reads and writes with the rest kept.
use crate::common::home::{now_ms, paths, read_record, with_field, write_entity};
use crate::common::outcome::View;
use crate::store::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// One project's record as read (fields kept whole and in order).
#[derive(Clone, Debug)]
pub struct ProjectRecord {
    store: String,
    name: String,
    fields: SdkValue,
}

impl ProjectRecord {
    /// Project `name` of Store `store`'s record `value`, as read; a missing
    /// record is `SdkValue::Null` (a summary then shows no `created`).
    pub fn from_record(store: &str, name: &str, value: SdkValue) -> Self {
        Self {
            store: store.to_owned(),
            name: name.to_owned(),
            fields: value,
        }
    }

    /// The record of project `name` of Store `store` in `home`, if any.
    pub(crate) fn read(home: &Path, store: &str, name: &str) -> Result<Option<Self>> {
        Ok(read_record(&paths::project_record(home, store, name))?
            .map(|value| Self::from_record(store, name, value)))
    }

    /// The record a Store's listing finds: its name and LayerStack.
    pub(crate) fn found(store: &str, name: &str, stack: SdkValue) -> Self {
        let fields = SdkValue::map([("name", SdkValue::text(name)), ("stack", stack)]);
        Self::from_record(store, name, fields)
    }

    /// The record a PostgreSQL Init writes: its name, LayerStack and now.
    pub(crate) fn initialised(store: &str, name: &str, stack: SdkValue) -> Self {
        Self::found(store, name, stack).created_now()
    }

    /// The record with `created` set to now.
    pub(crate) fn created_now(self) -> Self {
        let fields = with_field(self.fields, "created", SdkValue::Int(now_ms()));
        Self { fields, ..self }
    }

    /// Writes the record under `home`.
    pub(crate) fn write(&self, home: &Path) -> Result<()> {
        write_entity(
            &paths::project_record(home, &self.store, &self.name),
            &self.fields,
        )
    }

    /// This record with each of `other`'s fields set over its own.
    pub(crate) fn with_fields_of(self, other: &Self) -> Self {
        let mut fields = self.fields;
        if let SdkValue::Map(given) = &other.fields {
            for (name, value) in given {
                fields = with_field(fields, name, value.clone());
            }
        }
        Self { fields, ..self }
    }

    /// When the project was made, as its record holds it.
    pub(crate) fn created(&self) -> Option<&SdkValue> {
        self.fields.get("created")
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The project as `project.get`, each `project.list` item and Init
    /// show it, from its Store's record `store`: a Store kind LayerFS does
    /// not write shows none, a sqlite Store's machine is `native` when its
    /// record names none.
    pub fn to_sdk(&self, _view: View, store: &StoreRecord) -> SdkValue {
        let kind = store
            .kind()
            .map(|kind| SdkValue::text(kind.as_str()))
            .unwrap_or_default();
        SdkValue::map([
            (
                "project",
                SdkValue::text(format!("{}/{}", self.store, self.name)),
            ),
            ("store", SdkValue::text(&self.store)),
            ("name", SdkValue::text(&self.name)),
            ("kind", kind),
            (
                "machine",
                store
                    .field("machine")
                    .cloned()
                    .unwrap_or_else(|| SdkValue::text("native")),
            ),
            ("created", self.created().cloned().unwrap_or_default()),
        ])
    }
}

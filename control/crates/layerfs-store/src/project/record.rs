//! A project's record, `store/<store>/project/<project>/record.json` (#536
//! §9.2): its name, its LayerStack (a PostgreSQL Store's) and when it was
//! made. The record also holds the sqlite project's manifest, which
//! `sandbox/files.rs` reads and writes with the rest kept.
use crate::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_bridge::sdk_values::unhex;
use layerfs_home::home::{now_ms, paths, read_record, with_field, write_entity};
use layerfs_home::outcome::View;
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
    pub fn read(home: &Path, store: &str, name: &str) -> Result<Option<Self>> {
        Ok(read_record(&paths::project_record(home, store, name))?
            .map(|value| Self::from_record(store, name, value)))
    }

    /// The record a Store's listing finds: its name and LayerStack.
    pub fn found(store: &str, name: &str, stack: SdkValue) -> Self {
        let fields = SdkValue::map([("name", SdkValue::text(name)), ("stack", stack)]);
        Self::from_record(store, name, fields)
    }

    /// The record a PostgreSQL Init writes: its name, LayerStack and now.
    pub fn initialised(store: &str, name: &str, stack: SdkValue) -> Self {
        Self::found(store, name, stack).created_now()
    }

    /// The record with `created` set to now.
    pub fn created_now(self) -> Self {
        let fields = with_field(self.fields, "created", SdkValue::Int(now_ms()));
        Self { fields, ..self }
    }

    /// Writes the record under `home`.
    pub fn write(&self, home: &Path) -> Result<()> {
        write_entity(
            &paths::project_record(home, &self.store, &self.name),
            &self.fields,
        )?;
        layerfs_home::home::generation::moved(home);
        Ok(())
    }

    /// This record with each of `other`'s fields set over its own.
    pub fn with_fields_of(self, other: &Self) -> Self {
        let mut fields = self.fields;
        if let SdkValue::Map(given) = &other.fields {
            for (name, value) in given {
                fields = with_field(fields, name, value.clone());
            }
        }
        Self { fields, ..self }
    }

    /// When the project was made, as its record holds it.
    pub fn created(&self) -> Option<&SdkValue> {
        self.fields.get("created")
    }

    /// The project's LayerStack identity as its record holds it (S §4.2):
    /// the `manifest`'s stack for a local SQLite record, the record's
    /// `stack` for a PostgreSQL or worker record. A record with neither has
    /// none; its Store's filtered listing is then matched by name.
    pub fn stack(&self) -> Option<Vec<u8>> {
        if let Some(SdkValue::Bytes(bytes)) = self.fields.get("manifest") {
            if let Ok(manifest) = StoreManifest::decode(bytes) {
                return Some(manifest.stack.to_vec());
            }
        }
        self.fields.get_text("stack").and_then(unhex)
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

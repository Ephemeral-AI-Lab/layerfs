//! A project's record, `store/<store>/project/<project>/record.json` (#536
//! §9.2): its name, its LayerStack (a PostgreSQL Store's) and when it was
//! made. The record also holds the sqlite project's manifest, which
//! `sandbox/files.rs` reads and writes with the rest kept.
//!
//! A local sqlite record is usable only when that `manifest` decodes and,
//! once installed, its `installed` manifest does too: [`ProjectRecord::complete`]
//! and [`ProjectRecord::ready`] are the one decision `project.get`/`list`, the
//! project doctor and the home tree share (c7).
use crate::place::{self, Executor};
use crate::record::StoreRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_bridge::sdk_values::unhex;
use layerfs_history::LayerStackId;
use layerfs_home::home::{now_ms, paths, read_record, with_field, write_entity};
use layerfs_home::op::Op;
use layerfs_home::outcome::{refused, shell_word, with_fix};
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
        )
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

    /// The delete that clears a record this build cannot use: the project is
    /// on record, so its entry (not a bare folder) is what `project prune`
    /// refuses to touch.
    fn delete_fix(&self) -> String {
        format!(
            "layerfs project delete {store}/{name}",
            store = shell_word(&self.store),
            name = shell_word(&self.name)
        )
    }

    /// Whether this record is one that must carry a Store manifest: a
    /// sqlite Store this computer runs locally. A PostgreSQL record holds
    /// its `stack` instead, and a worker-placed sqlite record is written and
    /// read on its machine, so both stay valid here (c7).
    fn local_sqlite(&self, home: &Path, store: &StoreRecord) -> bool {
        let Ok(kind) = store.store_ref() else {
            return false;
        };
        matches!(kind, crate::record::StoreRef::Sqlite { .. })
            && matches!(place::plan(home, kind), Ok(Executor::Local))
    }

    /// The record's `field` manifest, decoded; a missing field or one that
    /// does not decode is `Failed`, naming the field and the decode error,
    /// with this project's delete as its fix.
    fn decode_field(&self, field: &str) -> Result<StoreManifest> {
        match self.fields.get(field) {
            Some(SdkValue::Bytes(bytes)) => StoreManifest::decode(bytes).map_err(|error| {
                with_fix(
                    refused(
                        "Failed",
                        format!("the project record's `{field}` does not decode: {error}"),
                    ),
                    self.delete_fix(),
                )
            }),
            _ => Err(with_fix(
                refused(
                    "Failed",
                    format!("the project record's `{field}` is missing"),
                ),
                self.delete_fix(),
            )),
        }
    }

    /// Whether this record can be used at all (c7): a local sqlite project
    /// has a `manifest` that decodes, and an `installed` it carries decodes
    /// too. A PostgreSQL record and a worker-placed sqlite record are always
    /// complete here (see [`Self::local_sqlite`]).
    pub fn complete(&self, home: &Path, store: &StoreRecord) -> Result<()> {
        if !self.local_sqlite(home, store) {
            return Ok(());
        }
        self.decode_field("manifest")?;
        if matches!(self.fields.get("installed"), Some(SdkValue::Bytes(_))) {
            self.decode_field("installed")?;
        }
        Ok(())
    }

    /// Whether this record is complete *and* carries the `installed`
    /// manifest a Sandbox opens: a local sqlite project without it is not
    /// `ready` (`sandbox create` refuses until it is imported again). This is
    /// the decision `project.get`/`project.list`, the project doctor and the
    /// home tree share.
    pub fn ready(&self, home: &Path, store: &StoreRecord) -> Result<()> {
        if !self.local_sqlite(home, store) {
            return Ok(());
        }
        self.complete(home, store)?;
        if matches!(self.fields.get("installed"), Some(SdkValue::Bytes(_))) {
            return Ok(());
        }
        Err(with_fix(
            refused("Failed", "the project record's `installed` is missing"),
            self.delete_fix(),
        ))
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

    /// The project's LayerStack as its record holds it ([`stack`](Self::stack)),
    /// if that is one.
    pub fn stack_id(&self) -> Option<LayerStackId> {
        LayerStackId::from_slice(&self.stack()?).ok()
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The operation this record holds while an Init or a delete of the
    /// project runs (`op`), if any.
    pub fn op(&self) -> Option<Op> {
        Op::of(&self.fields)
    }

    /// This record with `field` set to `value`.
    pub fn with(self, field: &str, value: SdkValue) -> Self {
        let fields = with_field(self.fields, field, value);
        Self { fields, ..self }
    }

    /// This record without `field`.
    pub fn without(self, field: &str) -> Self {
        let fields = match self.fields {
            SdkValue::Map(fields) => SdkValue::Map(
                fields
                    .into_iter()
                    .filter(|(name, _)| name != field)
                    .collect(),
            ),
            other => other,
        };
        Self { fields, ..self }
    }

    /// What every project answer says of the project, from its Store's
    /// record `store` (call surface 2.0): `project` (`<store>/<name>`),
    /// `store`, `engine` (none for a kind LayerFS does not read), `machine`
    /// (a sqlite Store's, `native` when its record names none; none for a
    /// PostgreSQL Store, which is on no machine) and `created_at`.
    pub fn answer(&self, store: &StoreRecord) -> Vec<(&'static str, SdkValue)> {
        let engine = store.kind().ok().map(|kind| kind.as_str().to_owned());
        let machine = crate::state::placed(store).map(str::to_owned);
        let created = self.fields.get_int("created").map(layerfs_home::time::iso);
        vec![
            (
                "project",
                SdkValue::text(format!("{}/{}", self.store, self.name)),
            ),
            ("store", SdkValue::text(&self.store)),
            ("engine", SdkValue::opt_text(engine)),
            ("machine", SdkValue::opt_text(machine)),
            ("created_at", SdkValue::opt_text(created)),
        ]
    }
}

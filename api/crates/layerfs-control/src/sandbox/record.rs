//! A Sandbox's record in the home (#536 §9.2), layout 2:
//! `machine/<machine>/sandbox/<name>/record.json` with its name, project,
//! machine, the state its create left (`creating` with the creator's pid,
//! `running`, `unknown`) and, once started, its runtime id, platform,
//! runtime and image. Kept whole, so `sandbox.list` and `sandbox.get` show
//! the record as before. Layout 3 (F2) moves it to `sandbox/<name>/`.
use crate::common::home::{self, paths, read_record, with_field, write_record};
use crate::common::outcome::View;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// One Sandbox's record as read, with the machine whose folder holds it.
#[derive(Clone, Debug)]
pub struct SandboxRecord {
    machine: String,
    name: String,
    fields: SdkValue,
}

impl SandboxRecord {
    /// Sandbox `name`'s record `value`, found under machine `machine`.
    pub fn from_record(machine: &str, name: &str, value: SdkValue) -> Self {
        Self {
            machine: machine.to_owned(),
            name: name.to_owned(),
            fields: value,
        }
    }

    /// Sandbox `name`'s record under machine `machine`, if it has one.
    pub(crate) fn read(home: &Path, machine: &str, name: &str) -> Result<Option<Self>> {
        let path = paths::sandbox_dir(home, machine, name).join("record.json");
        Ok(read_record(&path)?.map(|value| Self::from_record(machine, name, value)))
    }

    /// The record a create claims the name with: the project, the machine,
    /// `creating`, now and the creating process.
    pub(crate) fn creating(name: &str, project: &str, machine: &str) -> Self {
        let fields = SdkValue::map([
            ("name", SdkValue::text(name)),
            ("project", SdkValue::text(project)),
            ("machine", SdkValue::text(machine)),
            ("state", SdkValue::text("creating")),
            ("created", SdkValue::Int(home::now_ms())),
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ]);
        Self::from_record(machine, name, fields)
    }

    /// The record of a started Sandbox: what its create answered (`started`).
    pub(crate) fn started(self, started: &SdkValue) -> Self {
        let mut record = self;
        for (field, value) in [
            ("id", started.get("sandbox").cloned().unwrap_or_default()),
            (
                "platform",
                started.get("platform").cloned().unwrap_or_default(),
            ),
            (
                "runtime",
                started.get("runtime").cloned().unwrap_or_default(),
            ),
            ("image", started.get("image").cloned().unwrap_or_default()),
            ("state", SdkValue::text("running")),
        ] {
            record = record.with(field, value);
        }
        record
    }

    /// The record of a create whose outcome is not known.
    pub(crate) fn unknown(self) -> Self {
        self.with("state", SdkValue::text("unknown"))
    }

    fn with(self, field: &str, value: SdkValue) -> Self {
        let fields = with_field(self.fields, field, value);
        Self { fields, ..self }
    }

    /// The Sandbox's folder in the home.
    pub(crate) fn dir(&self, home: &Path) -> PathBuf {
        paths::sandbox_dir(home, &self.machine, &self.name)
    }

    /// Writes the record into its folder under `home`.
    pub(crate) fn write(&self, home: &Path) -> Result<()> {
        write_record(&self.dir(home).join("record.json"), &self.fields)
    }

    /// The Sandbox's name (its folder's).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The machine whose folder holds the record.
    pub fn machine(&self) -> &str {
        &self.machine
    }

    /// Its project, `<store>/<project>`, as recorded.
    pub fn project(&self) -> Option<&str> {
        self.fields.get_text("project")
    }

    /// The runtime's id of a started Sandbox.
    pub fn id(&self) -> Option<&str> {
        self.fields.get_text("id")
    }

    /// The image it runs, as recorded.
    pub fn image(&self) -> Option<&str> {
        self.fields.get_text("image")
    }

    /// The state its create recorded (`creating`, `running`, `unknown`).
    pub fn recorded_state(&self) -> Option<&str> {
        self.fields.get_text("state")
    }

    /// The process that created it, as recorded.
    pub fn pid(&self) -> Option<i64> {
        self.fields.get_int("pid")
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The Sandbox as each `sandbox.list` item and the start of
    /// `sandbox.get` show it: its record (never a `v`).
    pub fn to_sdk(&self, _view: View) -> SdkValue {
        match &self.fields {
            SdkValue::Map(fields) => {
                SdkValue::Map(fields.iter().filter(|(n, _)| n != "v").cloned().collect())
            }
            other => other.clone(),
        }
    }
}

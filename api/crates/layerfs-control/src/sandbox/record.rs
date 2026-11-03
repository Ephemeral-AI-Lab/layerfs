//! A Sandbox's record, `sandbox/<name>/record.json` (layout 3, #536 §5):
//! its name, project, machine, the state its create left (`creating` with
//! the creator's pid, `running`, `unknown`, or `gone` for one a conversion
//! found ended) and, once started, its backend, kernel route, runtime id,
//! platform, runtime and image. On `native` the side that runs the Sandbox
//! adds what reopens it (`daemon`) to the same file; its daemon keys are in
//! `control.json` beside it. The name is the folder's: one read finds it.
use crate::common::home::{self, paths, read_record, with_field, write_entity};
use crate::common::outcome::View;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Fields a record keeps that its call results never show.
const HIDDEN: [&str; 3] = ["v", "runtime_id", "daemon"];

/// One Sandbox's record as read.
#[derive(Clone, Debug)]
pub struct SandboxRecord {
    name: String,
    fields: SdkValue,
}

impl SandboxRecord {
    /// Sandbox `name`'s record `value`.
    pub fn from_record(name: &str, value: SdkValue) -> Self {
        Self {
            name: name.to_owned(),
            fields: value,
        }
    }

    /// Sandbox `name`'s record, if it has one.
    pub(crate) fn read(home: &Path, name: &str) -> Result<Option<Self>> {
        let path = paths::sandbox_record(home, name);
        Ok(read_record(&path)?.map(|value| Self::from_record(name, value)))
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
        Self::from_record(name, fields)
    }

    /// The record of a started Sandbox: what its create answered (`started`).
    pub(crate) fn started(self, started: &SdkValue) -> Self {
        let mut record = self;
        let copied = |name: &str| started.get(name).cloned().unwrap_or_default();
        for (field, value) in [
            ("id", copied("sandbox")),
            ("platform", copied("platform")),
            ("runtime", copied("runtime")),
            ("image", copied("image")),
            ("state", SdkValue::text("running")),
            ("backend", copied("runtime")),
            ("route", copied("route")),
            ("runtime_id", copied("runtime_id")),
        ] {
            record = record.with(field, value);
        }
        record
    }

    /// The record of a create whose outcome is not known.
    pub(crate) fn unknown(self) -> Self {
        self.with("state", SdkValue::text("unknown"))
    }

    /// `field` set to `value`.
    pub(crate) fn with(self, field: &str, value: SdkValue) -> Self {
        let fields = with_field(self.fields, field, value);
        Self { fields, ..self }
    }

    /// The Sandbox's folder in the home.
    pub(crate) fn dir(&self, home: &Path) -> PathBuf {
        paths::sandbox_dir(home, &self.name)
    }

    /// Writes the record into its folder under `home`, with its version.
    pub(crate) fn write(&self, home: &Path) -> Result<()> {
        write_entity(&paths::sandbox_record(home, &self.name), &self.fields)
    }

    /// The Sandbox's name (its folder's).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The machine it runs on, as recorded.
    pub fn machine(&self) -> &str {
        self.fields.get_text("machine").unwrap_or("")
    }

    /// Its project, `<store>/<project>`, as recorded.
    pub fn project(&self) -> Option<&str> {
        self.fields.get_text("project")
    }

    /// Its id once started: the name its machine knows it by.
    pub fn id(&self) -> Option<&str> {
        self.fields.get_text("id")
    }

    /// The image it runs, as recorded.
    pub fn image(&self) -> Option<&str> {
        self.fields.get_text("image")
    }

    /// The state its create recorded (`creating`, `running`, `unknown`,
    /// `gone`).
    pub fn recorded_state(&self) -> Option<&str> {
        self.fields.get_text("state")
    }

    /// The Sandbox's state (#536 §7.2) from its record: the state its
    /// create recorded, `creating` when none. F3 adds the daemon's answer
    /// and `op` (`closing`, `unresolved`).
    pub fn state(&self) -> &str {
        self.recorded_state().unwrap_or("creating")
    }

    /// The live process creating it: its recorded creator while the record
    /// says `creating` and that process is alive.
    pub fn creator(&self) -> Option<i64> {
        let pid = self.fields.get_int("pid").filter(|pid| *pid > 0)?;
        let alive = crate::common::home::lease::alive(pid as u32);
        (self.state() == "creating" && alive).then_some(pid)
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The Sandbox as each `sandbox.list` item and the start of
    /// `sandbox.get` show it: its record without what only reopens it
    /// (`v`, `runtime_id`, `daemon`); a list item whose record holds no
    /// `state` gains its derived one.
    pub fn to_sdk(&self, view: View) -> SdkValue {
        let SdkValue::Map(fields) = &self.fields else {
            return self.fields.clone();
        };
        let mut shown: Vec<_> = (fields.iter())
            .filter(|(n, _)| !HIDDEN.contains(&n.as_str()))
            .cloned()
            .collect();
        if view == View::List && self.recorded_state().is_none() {
            shown.push(("state".to_owned(), SdkValue::text(self.state())));
        }
        SdkValue::Map(shown)
    }
}

//! A Sandbox's record, `sandbox/<name>/record.json` (layout 3, #536 §5):
//! facts and at most one `op` intent (§7.1). A create claims the name with
//! its project, machine and an `op` (`sandbox.create`); its success records
//! the backend, kernel route, runtime id, platform, runtime and image and
//! drops the `op` in one write. A close records its `op` (`sandbox.close`)
//! before it stops the runtime. On `native` the side that runs the Sandbox
//! adds what reopens it (`daemon`) to the same file; its daemon keys are in
//! `control.json` beside it. The name is the folder's: one read finds it.
//! No state is stored: [`SandboxRecord::state`] derives it (§7.2).
use crate::common::contract::{self, Capacity};
use crate::common::home::entity_lock::{self, EntityLock};
use crate::common::home::{self, paths, read_record, with_field, write_entity};
use crate::common::isolation::{isolation_sdk, legacy_class, scheduling_sdk};
use crate::common::op::{self, Op, Standing};
use crate::common::outcome::View;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::{ExecMode, IsolationClass};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Fields a record keeps that its call results never show as written: what
/// only reopens it, and the intent (shown as [`Op::to_sdk`]).
const HIDDEN: [&str; 5] = ["v", "runtime_id", "daemon", "state", op::FIELD];

/// Takes Sandbox `name`'s lock, `sandbox/<name>/lock` (#536 §8): it guards
/// the Sandbox's record changes and its Workspace records, taken after the
/// home lock and never held across a worker or daemon call.
pub(crate) fn lock(home: &Path, name: &str) -> Result<EntityLock> {
    lock_in(&paths::sandbox_dir(home, name), name)
}

/// [`lock`] of the Sandbox folder `dir` (on `native` the home's, on a
/// machine the worker's own), named `name` in its refusal.
pub(crate) fn lock_in(dir: &Path, name: &str) -> Result<EntityLock> {
    let held = format!("Sandbox {name}'s record was being changed by another process");
    entity_lock::take(&dir.join("lock"), entity_lock::WAIT, &held)
}

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
    /// the #551 isolation request and answer, now and the create's intent
    /// `op`.
    pub(crate) fn creating(
        name: &str,
        project: &str,
        machine: &str,
        requested: IsolationClass,
        effective: IsolationClass,
        mode: ExecMode,
        op: &Op,
    ) -> Self {
        let fields = SdkValue::map([
            ("name", SdkValue::text(name)),
            ("schema_version", SdkValue::Int(contract::SCHEMA_VERSION)),
            ("project", SdkValue::text(project)),
            ("machine", SdkValue::text(machine)),
            ("requested_isolation", SdkValue::text(requested.as_str())),
            ("effective_isolation", SdkValue::text(effective.as_str())),
            ("exec_mode", SdkValue::text(mode.as_str())),
            ("capacity", Capacity::CONTRACT.to_sdk()),
            ("created", SdkValue::Int(home::now_ms())),
            (op::FIELD, op.to_record().clone()),
        ]);
        Self::from_record(name, fields)
    }

    /// The record of a started Sandbox: what its create answered
    /// (`started`), with its intent dropped.
    pub(crate) fn started(self, started: &SdkValue) -> Self {
        let mut record = self.without(op::FIELD);
        let copied = |name: &str| started.get(name).cloned().unwrap_or_default();
        for (field, value) in [
            ("id", copied("sandbox")),
            ("platform", copied("platform")),
            ("runtime", copied("runtime")),
            ("image", copied("image")),
            ("backend", copied("runtime")),
            ("route", copied("route")),
            ("runtime_id", copied("runtime_id")),
        ] {
            record = record.with(field, value);
        }
        record
    }

    /// The intent the record holds, if any.
    pub fn op(&self) -> Option<Op> {
        Op::of(&self.fields)
    }

    /// The record holding intent `op`.
    pub(crate) fn with_op(self, op: &Op) -> Self {
        self.with(op::FIELD, op.to_record().clone())
    }

    /// The record with no intent.
    pub(crate) fn without_op(self) -> Self {
        self.without(op::FIELD)
    }

    /// Refuses a new operation on this Sandbox while its intent holds it:
    /// `Busy` while its owner lives, `Unresolved` (naming `sandbox get` and
    /// `sandbox close`) once a gone owner left it uncertain.
    pub(crate) fn free(&self) -> Result<()> {
        match self.op() {
            None => Ok(()),
            Some(op) => op.blocks("sandbox", &self.name, "sandbox get", "sandbox close"),
        }
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

    /// The isolation class the create asked for (#551 B0-149). A record
    /// written before the contract falls back to its effective class,
    /// which is what it ran with.
    pub fn requested_isolation(&self) -> Option<IsolationClass> {
        self.text("requested_isolation")
            .and_then(IsolationClass::parse)
            .or_else(|| self.effective_isolation())
    }

    /// The isolation class the Sandbox actually runs behind (#551 B0-149).
    /// The record's own field wins; a record written before the contract
    /// falls back to its platform or backend name.
    pub fn effective_isolation(&self) -> Option<IsolationClass> {
        self.text("effective_isolation")
            .and_then(IsolationClass::parse)
            .or_else(|| self.text("platform").and_then(legacy_class))
            .or_else(|| self.text("backend").and_then(legacy_class))
    }

    /// Where its work runs (#551 B0-146); a record without the field is a
    /// worker.
    pub fn exec_mode(&self) -> ExecMode {
        self.text("exec_mode")
            .and_then(ExecMode::parse)
            .unwrap_or(ExecMode::Worker)
    }

    /// The record's `schema_version`; a record written before #551 reads
    /// this build's (#551 B0-152).
    pub fn schema_version(&self) -> i64 {
        contract::schema_version(&self.fields)
    }

    /// The capacity contract the record carries (#551 B0-151), the build's
    /// default for a record written before it.
    pub fn capacity(&self) -> Capacity {
        let Some(value) = self.fields.get("capacity") else {
            return Capacity::CONTRACT;
        };
        Capacity {
            per_store_quota: value
                .get_int("per_store_quota")
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(Capacity::CONTRACT.per_store_quota),
            in_flight: value
                .get_int("in_flight_limit")
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(Capacity::CONTRACT.in_flight),
            queue: value
                .get_int("queue_limit")
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(Capacity::CONTRACT.queue),
        }
    }

    /// The record's capacity as a read model shows it: the bounds, and what
    /// this Sandbox measures now (`measured`), or zeros when this process
    /// does not own it.
    pub fn capacity_sdk(&self, measured: Option<(u32, u32)>) -> SdkValue {
        let (in_flight, queued) = measured.unwrap_or((0, 0));
        self.capacity().measured(in_flight, queued)
    }

    /// The record with the #551 fields it is missing filled in, for a write
    /// that must migrate an old record: `schema_version`, `capacity`, the
    /// effective class and the exec mode. A read never calls it.
    pub fn with_contract(self) -> Self {
        let mut record = self;
        if record.fields.get("schema_version").is_none() {
            record = record.with(
                "schema_version",
                SdkValue::Int(contract::SCHEMA_VERSION),
            );
        }
        if record.fields.get("capacity").is_none() {
            record = record.with("capacity", Capacity::CONTRACT.to_sdk());
        }
        if record.text("effective_isolation").is_none() {
            if let Some(class) = record.effective_isolation() {
                record = record.with("effective_isolation", SdkValue::text(class.as_str()));
            }
        }
        if record.text("requested_isolation").is_none() {
            if let Some(class) = record.requested_isolation() {
                record = record.with("requested_isolation", SdkValue::text(class.as_str()));
            }
        }
        record
    }

    /// The `text` field `field`.
    fn text(&self, field: &str) -> Option<&str> {
        self.fields.get_text(field)
    }

    /// The Sandbox's state (#536 §7.2), from its record, its `op` and,
    /// where the call asked, the daemon's answer (`gone`: its machine
    /// answered that no such Sandbox runs): `creating` or `closing` while
    /// that intent's owner lives, `unresolved` once a gone owner left it
    /// uncertain, `gone` with no id (its create never recorded one, or a
    /// conversion found it ended) or when answered so, else `running`.
    pub fn state(&self, gone: bool) -> &'static str {
        let op = self.op();
        match op.as_ref().map(|op| (op.standing(), op.name())) {
            Some((Standing::Running, "sandbox.close")) => "closing",
            Some((Standing::Running, _)) => "creating",
            Some((Standing::Unresolved, _)) => "unresolved",
            _ if gone || self.id().is_none() => "gone",
            _ => "running",
        }
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The Sandbox as each `sandbox.list` item and the start of
    /// `sandbox.get` show it: its record without what only reopens it
    /// (`v`, `runtime_id`, `daemon`), its derived `state` and its `op`
    /// (H6) when it has one. Both views show the same fields.
    pub fn to_sdk(&self, view: View) -> SdkValue {
        let _ = view;
        let SdkValue::Map(fields) = &self.fields else {
            return self.fields.clone();
        };
        let mut shown: Vec<_> = (fields.iter())
            .filter(|(n, _)| {
                !HIDDEN.contains(&n.as_str())
                    && !matches!(
                        n.as_str(),
                        "requested_isolation" | "effective_isolation" | "exec_mode"
                    )
            })
            .cloned()
            .collect();
        let requested = self.requested_isolation();
        let effective = self.effective_isolation();
        if let Some(requested) = requested {
            shown.push((
                "requested_isolation".to_owned(),
                SdkValue::text(requested.as_str()),
            ));
        }
        if let Some(effective) = effective {
            shown.push((
                "effective_isolation".to_owned(),
                SdkValue::text(effective.as_str()),
            ));
            shown.push(("isolation".to_owned(), isolation_sdk(requested, effective)));
        }
        shown.push(("scheduling".to_owned(), scheduling_sdk(self.exec_mode())));
        shown.push(("schema_version".to_owned(), SdkValue::Int(self.schema_version())));
        shown.push((
            "capacity".to_owned(),
            self.capacity_sdk(None),
        ));
        shown.push(("state".to_owned(), SdkValue::text(self.state(false))));
        if let Some(op) = self.op() {
            shown.push((op::FIELD.to_owned(), op.to_sdk()));
        }
        SdkValue::Map(shown)
    }

    /// The record without `field`.
    fn without(self, field: &str) -> Self {
        let fields = match self.fields {
            SdkValue::Map(mut fields) => {
                fields.retain(|(name, _)| name != field);
                SdkValue::Map(fields)
            }
            other => other,
        };
        Self { fields, ..self }
    }
}

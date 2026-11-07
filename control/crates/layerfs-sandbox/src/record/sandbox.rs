//! A Sandbox's record, `sandbox/<name>/record.json` (layout 3, #536 §5):
//! facts and at most one `op` intent (§7.1). A create claims the name with
//! its project, machine and an `op` (`sandbox.create`); its success records
//! the backend, kernel route, runtime id, platform, runtime and image and
//! drops the `op` in one write. A close records its `op` (`sandbox.close`)
//! before it stops the runtime. On `native` the side that runs the Sandbox
//! adds what reopens it (`daemon`) to the same file; its daemon keys are in
//! `control.json` beside it. The name is the folder's: one read finds it.
//! No state is stored: [`SandboxRecord::state`] derives it (§7.2).
//!
//! The record is typed (layerfs-project#108): each known field has its
//! type, and the codec here ([`SandboxRecord::from_record`],
//! [`SandboxRecord::to_record`]) is the only place it meets a value. A
//! field this build does not know, or one stored as null or another type,
//! is kept as read, and the file's order is kept, so a record read and
//! written back is the same JSON.
use super::daemon::DaemonRecord;
use crate::host::Started;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::entity_lock::{self, EntityLock};
use layerfs_home::home::fields::{self, Fields};
use layerfs_home::home::{self, paths, read_record, write_entity};
use layerfs_home::op::{self, Op, Standing};
use layerfs_home::outcome::{failed, View};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Fields a record keeps that its call results never show as written: what
/// only reopens it, and the intent (shown as [`Op::to_sdk`]).
const HIDDEN: [&str; 5] = ["v", "runtime_id", "daemon", "state", op::FIELD];

/// Takes Sandbox `name`'s lock, `sandbox/<name>/lock` (#536 §8): it guards
/// the Sandbox's record changes and its Workspace records, taken after the
/// home lock and never held across a worker or daemon call.
pub fn lock(home: &Path, name: &str) -> Result<EntityLock> {
    lock_in(&paths::sandbox_dir(home, name), name)
}

/// [`lock`] of the Sandbox folder `dir` (on `native` the home's, on a
/// machine the worker's own), named `name` in its refusal.
pub fn lock_in(dir: &Path, name: &str) -> Result<EntityLock> {
    let held = format!("Sandbox {name}'s record was being changed by another process");
    entity_lock::take(&dir.join("lock"), entity_lock::WAIT, &held)
}

/// The fields a Sandbox record knows.
#[derive(Clone, Debug, PartialEq)]
enum Field {
    V(i64),
    Name(String),
    Project(String),
    Machine(String),
    Created(i64),
    Op(Op),
    Id(String),
    Platform(String),
    Runtime(String),
    Image(String),
    Backend(String),
    Route(String),
    RuntimeId(String),
    Daemon(DaemonRecord),
    Left(String),
}

impl fields::Field for Field {
    fn decode(name: &str, value: &SdkValue) -> Option<Self> {
        let text = || fields::text(value);
        Some(match name {
            "v" => Self::V(fields::int(value)?),
            "name" => Self::Name(text()?),
            "project" => Self::Project(text()?),
            "machine" => Self::Machine(text()?),
            "created" => Self::Created(fields::int(value)?),
            op::FIELD => Self::Op(Op::decode(value)?),
            "id" => Self::Id(text()?),
            "platform" => Self::Platform(text()?),
            "runtime" => Self::Runtime(text()?),
            "image" => Self::Image(text()?),
            "backend" => Self::Backend(text()?),
            "route" => Self::Route(text()?),
            "runtime_id" => Self::RuntimeId(text()?),
            "daemon" => Self::Daemon(DaemonRecord::decode(value)?),
            "left" => Self::Left(text()?),
            _ => return None,
        })
    }

    fn name(&self) -> &'static str {
        match self {
            Self::V(_) => "v",
            Self::Name(_) => "name",
            Self::Project(_) => "project",
            Self::Machine(_) => "machine",
            Self::Created(_) => "created",
            Self::Op(_) => op::FIELD,
            Self::Id(_) => "id",
            Self::Platform(_) => "platform",
            Self::Runtime(_) => "runtime",
            Self::Image(_) => "image",
            Self::Backend(_) => "backend",
            Self::Route(_) => "route",
            Self::RuntimeId(_) => "runtime_id",
            Self::Daemon(_) => "daemon",
            Self::Left(_) => "left",
        }
    }

    fn encode(&self) -> SdkValue {
        match self {
            Self::V(n) | Self::Created(n) => SdkValue::Int(*n),
            Self::Op(op) => op.to_record().clone(),
            Self::Daemon(daemon) => daemon.to_record(),
            Self::Name(text)
            | Self::Project(text)
            | Self::Machine(text)
            | Self::Id(text)
            | Self::Platform(text)
            | Self::Runtime(text)
            | Self::Image(text)
            | Self::Backend(text)
            | Self::Route(text)
            | Self::RuntimeId(text)
            | Self::Left(text) => SdkValue::text(text),
        }
    }
}

/// One Sandbox's record as read.
#[derive(Clone, Debug)]
pub struct SandboxRecord {
    name: String,
    fields: Fields<Field>,
}

impl SandboxRecord {
    /// Sandbox `name`'s record `value` (the record codec's read).
    pub fn from_record(name: &str, value: SdkValue) -> Self {
        Self {
            name: name.to_owned(),
            fields: Fields::decode(&value),
        }
    }

    /// Sandbox `name`'s record, if it has one.
    pub fn read(home: &Path, name: &str) -> Result<Option<Self>> {
        Self::read_in(&paths::sandbox_dir(home, name), name)
    }

    /// The record in Sandbox folder `dir` (on `native` the home's, on a
    /// machine the worker's own), if it has one.
    pub fn read_in(dir: &Path, name: &str) -> Result<Option<Self>> {
        let path = dir.join("record.json");
        Ok(read_record(&path)?.map(|value| Self::from_record(name, value)))
    }

    /// The record a create claims the name with: the project, the machine,
    /// now and the create's intent `op`.
    pub fn creating(name: &str, project: &str, machine: &str, op: &Op) -> Self {
        let fields = Fields::of([
            Field::Name(name.to_owned()),
            Field::Project(project.to_owned()),
            Field::Machine(machine.to_owned()),
            Field::Created(home::now_ms()),
            Field::Op(op.clone()),
        ]);
        Self {
            name: name.to_owned(),
            fields,
        }
    }

    /// The record of a started Sandbox: what its create answered
    /// (`started`), with its intent dropped.
    pub fn started(mut self, started: &Started) -> Self {
        self.fields.remove(op::FIELD);
        for (field, value) in [
            ("id", &started.sandbox),
            ("platform", &started.platform),
            ("runtime", &started.runtime),
            ("image", &started.image),
            ("backend", &started.runtime),
            ("route", &started.route),
            ("runtime_id", &started.runtime_id),
        ] {
            self.fields.put(field, SdkValue::opt_text(value.clone()));
        }
        self
    }

    /// What the side that runs a Sandbox records once it started (on
    /// `native`, in the home's record): its name, `project`, backend,
    /// kernel route and runtime id (`facts`) and what reopens it (`daemon`),
    /// the record's other fields kept.
    pub fn with_start(mut self, project: String, facts: [String; 3], daemon: DaemonRecord) -> Self {
        let [backend, route, runtime_id] = facts;
        for field in [
            Field::Name(self.name.clone()),
            Field::Project(project),
            Field::Backend(backend),
            Field::Route(route),
            Field::RuntimeId(runtime_id),
            Field::Daemon(daemon),
        ] {
            self.fields.set(field);
        }
        self
    }

    /// The intent the record holds, if any.
    pub fn op(&self) -> Option<Op> {
        self.fields
            .get(|field| match field {
                Field::Op(op) => Some(op),
                _ => None,
            })
            .cloned()
    }

    /// The record holding intent `op`.
    pub fn with_op(mut self, op: &Op) -> Self {
        self.fields.set(Field::Op(op.clone()));
        self
    }

    /// The record with no intent.
    pub fn without_op(mut self) -> Self {
        self.fields.remove(op::FIELD);
        self
    }

    /// The record with what reopens it, `daemon`.
    pub fn with_daemon(mut self, daemon: DaemonRecord) -> Self {
        self.fields.set(Field::Daemon(daemon));
        self
    }

    /// The record of a Sandbox whose runtime ended: its `id` null, so
    /// layout 3 derives `gone` (#536 H10).
    pub fn ended(mut self) -> Self {
        self.fields.put("id", SdkValue::Null);
        self
    }

    /// Refuses a new operation on this Sandbox while its intent holds it:
    /// `Busy` while its owner lives, `Unresolved` (naming `sandbox get` and
    /// `sandbox close`) once a gone owner left it uncertain.
    pub fn free(&self) -> Result<()> {
        match self.op() {
            None => Ok(()),
            Some(op) => op.blocks("sandbox", &self.name, "sandbox get", "sandbox close"),
        }
    }

    /// The Sandbox's folder in the home.
    pub fn dir(&self, home: &Path) -> PathBuf {
        paths::sandbox_dir(home, &self.name)
    }

    /// Writes the record into its folder under `home`, with its version.
    pub fn write(&self, home: &Path) -> Result<()> {
        self.write_in(&self.dir(home))
    }

    /// Writes the record into Sandbox folder `dir`, with its version.
    pub fn write_in(&self, dir: &Path) -> Result<()> {
        write_entity(&dir.join("record.json"), &self.to_record())
    }

    /// The Sandbox's name (its folder's).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// One text field, as recorded.
    fn text(&self, pick: fn(&Field) -> Option<&String>) -> Option<&str> {
        self.fields.get(pick).map(String::as_str)
    }

    /// The machine it runs on, as recorded.
    pub fn machine(&self) -> &str {
        self.text(|f| match f {
            Field::Machine(text) => Some(text),
            _ => None,
        })
        .unwrap_or("")
    }

    /// Its project, `<store>/<project>`, as recorded.
    pub fn project(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Project(text) => Some(text),
            _ => None,
        })
    }

    /// Its id once started: the name its machine knows it by.
    pub fn id(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Id(text) => Some(text),
            _ => None,
        })
    }

    /// The image it runs, as recorded.
    pub fn image(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Image(text) => Some(text),
            _ => None,
        })
    }

    /// The platform it names (#529), as recorded.
    pub fn platform(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Platform(text) => Some(text),
            _ => None,
        })
    }

    /// Its runtime, as recorded.
    pub fn runtime(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Runtime(text) => Some(text),
            _ => None,
        })
    }

    /// Its backend, as recorded.
    pub fn backend(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Backend(text) => Some(text),
            _ => None,
        })
    }

    /// Why a layout 2 Sandbox's runtime was found ended, as its conversion
    /// recorded it.
    pub fn left(&self) -> Option<&str> {
        self.text(|f| match f {
            Field::Left(text) => Some(text),
            _ => None,
        })
    }

    /// What reopens it, when its record keeps that (`daemon`).
    pub fn daemon(&self) -> Option<&DaemonRecord> {
        self.fields.get(|f| match f {
            Field::Daemon(daemon) => Some(daemon),
            _ => None,
        })
    }

    /// The container of a Sandbox recorded under the retired docker
    /// backend (D-8, layerfs-project#111): `backend` or `runtime` `docker`,
    /// or a started Sandbox of the docker machine with no `runtime` (a
    /// record converted from before runtimes were kept). The name is the
    /// one its daemon record keeps, else the one the docker backend gave
    /// it, `lfs-<home id prefix>-<name>`. `None` for every other record.
    pub fn retired_container(&self, home: &Path) -> Result<Option<String>> {
        let runtime = self.runtime();
        let retired = runtime == Some("docker")
            || self.backend() == Some("docker")
            || (runtime.is_none() && self.machine() == "docker" && self.id().is_some());
        if !retired {
            return Ok(None);
        }
        let kept = (self.daemon())
            .and_then(DaemonRecord::name)
            .map(str::to_owned);
        Ok(Some(match kept {
            Some(name) => name,
            None => {
                // a layout 2 home is read by its conversion before it opens
                let id = match layerfs_home::home::layout::detect(home) {
                    Ok(layerfs_home::home::layout::Layout::Previous(file)) => file.id,
                    _ => layerfs_home::home::opened(home)?.id,
                };
                format!("lfs-{}-{}", &id[..8.min(id.len())], self.name)
            }
        }))
    }

    /// The Sandbox's state (#536 §7.2), from its record, its `op` and,
    /// where the call asked, the daemon's answer (`gone`: its machine
    /// answered that no such Sandbox runs): `creating` or `closing` while
    /// that intent's owner lives, `unresolved` once a gone owner left it
    /// uncertain, `gone` with no id (its create never recorded one, or a
    /// conversion found it ended), `lost` when it has an id and no close
    /// intent and its machine answered that its runtime is not there
    /// (compute-plane 2.0 P-1: it ended behind control's back; its
    /// `sandbox.close` clears it), else `running`.
    pub fn state(&self, gone: bool) -> &'static str {
        let op = self.op();
        match op.as_ref().map(|op| (op.standing(), op.name())) {
            Some((Standing::Running, "sandbox.close")) => "closing",
            Some((Standing::Running, _)) => "creating",
            Some((Standing::Unresolved, _)) => "unresolved",
            _ if self.id().is_none() => "gone",
            _ if gone => "lost",
            _ => "running",
        }
    }

    /// The record as written (the record codec's write).
    pub fn to_record(&self) -> SdkValue {
        self.fields.encode()
    }

    /// The Sandbox as each `sandbox.list` item and the start of
    /// `sandbox.get` show it: its record without what only reopens it
    /// (`v`, `runtime_id`, `daemon`), its derived `state` and its `op`
    /// (H6) when it has one. Both views show the same fields.
    pub fn to_sdk(&self, view: View) -> SdkValue {
        let _ = view;
        let mut shown = self.fields.encode_without(&HIDDEN);
        shown.push(("state".to_owned(), SdkValue::text(self.state(false))));
        if let Some(op) = self.op() {
            shown.push((op::FIELD.to_owned(), op.to_sdk()));
        }
        SdkValue::Map(shown)
    }
}

/// Every Sandbox record, by name.
pub fn sandboxes(home: &Path) -> Result<Vec<SandboxRecord>> {
    let mut found = Vec::new();
    for name in paths::names(&home.join("sandbox")).map_err(failed)? {
        found.extend(SandboxRecord::read(home, &name)?);
    }
    Ok(found)
}

/// The Sandbox named `name`: one read of `sandbox/<name>/record.json`.
pub fn sandbox(home: &Path, name: &str) -> Result<Option<SandboxRecord>> {
    SandboxRecord::read(home, name)
}

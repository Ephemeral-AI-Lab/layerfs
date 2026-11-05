//! A machine's record, `machine/<name>/record.json` (#536 §9.2): its
//! platform, size, images, ssh host, lasting guest state, the state a
//! create, start or stop left, the `op` of a create, connect, remove or
//! disconnect in flight (#536 §7.1), and the built-in machines' settings. Read
//! once and kept whole, so `machine.get` and `machine.list` show what they
//! showed before. Built without the `machine` feature too (the guest
//! worker reads its own `native` and `docker` records); the platform and
//! the summary need the adapters.
use crate::common::contract;
use crate::common::home::{self, paths, read_record, with_field, write_entity};
#[cfg(not(feature = "machine"))]
use crate::common::isolation::legacy_class;
#[cfg(feature = "machine")]
use crate::common::isolation::{isolation_sdk, scheduling_sdk};
use crate::common::op::{self, Op, Standing};
use crate::common::outcome::refused;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::{ExecMode, IsolationClass, KernelScope};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The intents a machine's record holds (#536 §7.1).
pub const CREATE: &str = "machine.create";
/// An ssh machine's connect.
pub const CONNECT: &str = "machine.connect";
/// A VM machine's remove.
pub const REMOVE: &str = "machine.remove";
/// An ssh machine's disconnect.
pub const DISCONNECT: &str = "machine.disconnect";

/// One machine's record as read (fields kept whole and in order).
#[derive(Clone, Debug)]
pub struct MachineRecord {
    name: String,
    fields: SdkValue,
}

impl MachineRecord {
    /// Machine `name`'s record `value`, as read.
    pub fn from_record(name: &str, value: SdkValue) -> Self {
        Self {
            name: name.to_owned(),
            fields: value,
        }
    }

    /// Machine `name`'s record in `home`, if it has one.
    pub fn read(home: &Path, name: &str) -> Result<Option<Self>> {
        Ok(read_record(&paths::machine_record(home, name))?.map(|v| Self::from_record(name, v)))
    }

    /// Machine `name`'s record as it is, whatever `op` it holds; a bad name
    /// is `InvalidInput`, none `NotFound`.
    pub fn of(home: &Path, name: &str) -> Result<Self> {
        crate::common::names::MachineName::parse(name)?;
        Self::read(home, name)?
            .ok_or_else(|| refused("NotFound", format!("no machine named {name}")))
    }

    /// A built-in machine's first record: its name, platform, the #551
    /// isolation contract and now.
    pub fn built_in(name: &str) -> Self {
        Self::from_record(
            name,
            SdkValue::map([
                ("name", SdkValue::text(name)),
                ("platform", SdkValue::text(name)),
                ("created", SdkValue::Int(home::now_ms())),
            ]),
        )
        .with_contract()
    }

    /// The isolation class a machine of this platform provides on this host
    /// (#551 B0-144). The record's own `isolation_class` wins; a record
    /// written before the contract falls back to its platform, and an
    /// unknown platform has none.
    pub fn isolation_class(&self) -> Option<IsolationClass> {
        self.text("isolation_class")
            .and_then(IsolationClass::parse)
            .or_else(|| self.platform_isolation())
    }

    /// The isolation class the record itself declares, if it has one. A
    /// read that must not derive `doctor` uses this.
    pub fn declared_isolation(&self) -> Option<IsolationClass> {
        self.text("isolation_class").and_then(IsolationClass::parse)
    }

    /// Where this machine's Sandbox work runs (#551 B0-146), from the
    /// record's `exec_mode` or the platform's declaration.
    pub fn exec_mode(&self) -> Option<ExecMode> {
        self.text("exec_mode")
            .and_then(ExecMode::parse)
            .or_else(|| self.platform_exec_mode())
    }

    /// Whether a Sandbox on this machine shares the caller's kernel (#551
    /// B0-144), from the record's `kernel` or its isolation class.
    pub fn kernel_scope(&self) -> Option<KernelScope> {
        self.text("kernel")
            .and_then(KernelScope::parse)
            .or_else(|| self.isolation_class().map(KernelScope::of_class))
    }

    /// This record with the #551 fields it is missing, for a create, a
    /// connect or a built-in record. Fields already present stay as they
    /// are; an unknown platform is left alone.
    pub fn with_contract(self) -> Self {
        let name = self.name.clone();
        let record = Self::from_record(&name, contract::with_schema_version(self.fields));
        let Some(class) = record.isolation_class() else {
            return record;
        };
        let mode = record.exec_mode().unwrap_or(ExecMode::Worker);
        let kernel = record
            .kernel_scope()
            .unwrap_or(KernelScope::of_class(class));
        let mut record = record;
        if record.field("isolation_class").is_none() {
            record = record.with("isolation_class", SdkValue::text(class.as_str()));
        }
        if record.field("exec_mode").is_none() {
            record = record.with("exec_mode", SdkValue::text(mode.as_str()));
        }
        if record.field("kernel").is_none() {
            record = record.with("kernel", SdkValue::text(kernel.as_str()));
        }
        record
    }

    /// The platform's declared class, for a record written before #551.
    fn platform_isolation(&self) -> Option<IsolationClass> {
        let name = self.platform_name()?;
        #[cfg(feature = "machine")]
        {
            let kind = layerfs_machine::Platform::parse(name)?;
            kind.isolation(layerfs_machine::HostOs::current())
        }
        #[cfg(not(feature = "machine"))]
        {
            legacy_class(name)
        }
    }

    /// The platform's declared exec mode, for a record written before #551.
    fn platform_exec_mode(&self) -> Option<ExecMode> {
        let name = self.platform_name()?;
        #[cfg(feature = "machine")]
        {
            let kind = layerfs_machine::Platform::parse(name)?;
            Some(kind.exec_mode(layerfs_machine::HostOs::current()))
        }
        #[cfg(not(feature = "machine"))]
        {
            let _ = legacy_class(name)?;
            Some(ExecMode::Worker)
        }
    }

    /// The machine's name (its folder's).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The record as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// The record with field `field` set to `value` (added when absent).
    pub fn with(self, field: &str, value: SdkValue) -> Self {
        Self::from_record(&self.name, with_field(self.fields, field, value))
    }

    /// Writes the record to `machine/<name>/record.json` under `home`.
    pub fn write(&self, home: &Path) -> Result<()> {
        write_entity(&paths::machine_record(home, &self.name), &self.fields)
    }

    /// The record's field `field` as written.
    pub fn field(&self, field: &str) -> Option<&SdkValue> {
        self.fields.get(field)
    }

    /// The text field `field` (`vm_name`, `image_archive`, `os_image`, ...).
    pub fn text(&self, field: &str) -> Option<&str> {
        self.fields.get_text(field)
    }

    /// The whole-number field `field` (`cpus`, `memory`, `disk`).
    pub fn int(&self, field: &str) -> Option<i64> {
        self.fields.get_int(field)
    }

    /// The platform's name, as written (`platform()` parses it).
    pub fn platform_name(&self) -> Option<&str> {
        self.fields.get_text("platform")
    }

    /// The state a create, connect, start or stop recorded (`running`,
    /// `stopped`, `failed`).
    pub fn recorded_state(&self) -> Option<&str> {
        self.fields.get_text("state")
    }

    /// The intent of a create, connect, remove or disconnect, if the record
    /// holds one.
    pub fn op(&self) -> Option<Op> {
        Op::of(&self.fields)
    }

    /// The record holding intent `op`.
    pub fn with_op(self, op: &Op) -> Self {
        self.with(op::FIELD, op.to_record().clone())
    }

    /// The record with no intent.
    pub fn without_op(self) -> Self {
        let fields = match self.fields {
            SdkValue::Map(fields) => fields.into_iter().filter(|(n, _)| n != op::FIELD),
            other => return Self::from_record(&self.name, other),
        };
        Self::from_record(&self.name, SdkValue::Map(fields.collect()))
    }

    /// Whether a create, connect, remove or disconnect holds the machine:
    /// its intent's owner lives, or is gone with the outcome unknown. A list
    /// shows its state and probes nothing.
    pub fn in_flight(&self) -> bool {
        self.op().is_some_and(|op| op.standing() != Standing::Stale)
    }

    /// Refuses a new operation on the machine while an intent holds it:
    /// `Busy` while its owner lives, `Unresolved` once a gone owner left
    /// it uncertain, naming `machine status <name>` and the call that
    /// clears it (`machine remove`, or `machine disconnect` for ssh).
    pub fn not_held(&self) -> Result<()> {
        self.op().map_or(Ok(()), |op| self.blocked_by(&op))
    }

    /// Refuses only while a live process holds the machine (`Busy`): what
    /// a remove or disconnect, which clears an unresolved intent, checks.
    pub fn not_running(&self) -> Result<()> {
        match self.op() {
            Some(op) if op.standing() == Standing::Running => self.blocked_by(&op),
            _ => Ok(()),
        }
    }

    fn blocked_by(&self, op: &Op) -> Result<()> {
        op.blocks(
            "machine",
            &self.name,
            "machine status",
            self.clearing_call(),
        )
    }

    /// The call that clears an unresolved intent: `machine disconnect` for
    /// an ssh machine, else `machine remove`.
    pub fn clearing_call(&self) -> &'static str {
        match self.platform_name() {
            Some("ssh") => "machine disconnect",
            _ => "machine remove",
        }
    }

    /// The Sandbox image the record names (a digest, or a file).
    pub fn image(&self) -> Option<&str> {
        self.fields.get_text("image")
    }

    /// The docker machine's Docker socket.
    pub fn docker(&self) -> Option<&str> {
        self.fields.get_text("docker")
    }

    /// The daemon a built-in machine's Sandboxes run (`import-env` writes it).
    pub fn daemon(&self) -> Option<&str> {
        self.fields.get_text("daemon")
    }

    /// A Firecracker machine's Lima host.
    pub fn host(&self) -> Option<&str> {
        self.fields.get_text("host")
    }

    /// Whether a Lima machine was made `--nested`.
    pub fn nested(&self) -> bool {
        matches!(self.fields.get("nested"), Some(SdkValue::Bool(true)))
    }

    /// An ssh machine's host settings.
    pub fn ssh(&self) -> Option<&SdkValue> {
        self.fields.get("ssh")
    }

    /// The identity of the host an ssh machine reaches.
    pub fn host_id(&self) -> Option<&str> {
        self.ssh().and_then(|ssh| ssh.get_text("host_id"))
    }

    /// The record with its ssh settings' `host_id` set.
    pub fn with_host_id(self, identity: &str) -> Self {
        let ssh = self.ssh().cloned().unwrap_or_default();
        let ssh = with_field(ssh, "host_id", SdkValue::text(identity));
        self.with("ssh", ssh)
    }

    /// The record with `state` and the `error` that left it there.
    pub fn failed_as(self, state: &str, error: &str) -> Self {
        let record = self.with("state", SdkValue::text(state));
        record.with("error", SdkValue::text(error))
    }

    /// The docker machine's loaded default image, `(id, bundle hash)`.
    pub fn loaded_image(&self) -> Option<(&str, &str)> {
        let loaded = self.fields.get("loaded_image")?;
        Some((loaded.get_text("id")?, loaded.get_text("bundle")?))
    }

    /// The record with the default image `id` loaded from bundle `bundle`.
    pub fn with_loaded_image(self, id: &str, bundle: &str) -> Self {
        let loaded = SdkValue::map([
            ("id", SdkValue::text(id)),
            ("bundle", SdkValue::text(bundle)),
        ]);
        self.with("loaded_image", loaded)
    }
}

/// What a machine's result shows beside its record.
#[cfg(feature = "machine")]
#[derive(Clone, Debug, Default)]
pub struct MachineDerived {
    /// Its live state and, when that could not be read, why; `None` keeps
    /// the record's state (a built-in machine, or one being made or removed).
    pub live: Option<(String, Option<String>)>,
    /// The sqlite Stores on it.
    pub stores: Vec<String>,
    /// The Sandboxes recorded on it.
    pub sandboxes: Vec<String>,
    /// Its worker as `worker.json` records it, read without dialing
    /// (#551 B8-156).
    pub worker: Option<SdkValue>,
    /// Its `start.lock` lease, read without taking it (#551 B8-156).
    pub lease: Option<SdkValue>,
}

#[cfg(feature = "machine")]
impl MachineRecord {
    /// The machine's platform; a record naming none LayerFS knows is
    /// refused `Failed`.
    pub fn platform(&self) -> Result<layerfs_machine::Platform> {
        self.platform_name()
            .and_then(layerfs_machine::Platform::parse)
            .ok_or_else(|| refused("Failed", "a machine record names no known platform"))
    }

    /// The machine's state (#536 §7.2), from its record, `op`, `error` and
    /// live probe: `creating` or `removing` while a live process creates
    /// (connects) or removes (disconnects) it, `unresolved` once a gone one
    /// left that outcome unknown; else its live probe's answer (`live`:
    /// `running`, `stopped`, `failed`, `missing`, or `unknown` when the
    /// probe could not answer), else the state its record holds (`failed`
    /// with its `error` after a create that kept its VM), else `ready` for
    /// a built-in machine and `stopped` for any other. A gone owner's
    /// certain intent changes nothing.
    pub fn state<'a>(&'a self, live: Option<&'a str>) -> Result<&'a str> {
        let fallback = match self.platform()?.built_in() {
            true => "ready",
            false => "stopped",
        };
        if let Some(op) = self.op() {
            match op.standing() {
                Standing::Unresolved => return Ok("unresolved"),
                Standing::Running if op.name() == REMOVE || op.name() == DISCONNECT => {
                    return Ok("removing")
                }
                Standing::Running => return Ok("creating"),
                Standing::Stale => {}
            }
        }
        Ok(live.or(self.recorded_state()).unwrap_or(fallback))
    }

    /// The machine as `machine.get`, each `machine.list` item, `start`,
    /// `stop`, `update`, `create` and `connect` show it: name, platform,
    /// backend, `supports`, state, size, Stores and Sandboxes, then the
    /// settings it has (`image`, `host`, `nested`, `docker`, `wsl`,
    /// `error`, `ssh`).
    pub fn to_sdk(
        &self,
        _view: crate::common::outcome::View,
        derived: &MachineDerived,
    ) -> Result<SdkValue> {
        let kind = self.platform()?;
        let live = derived.live.as_ref();
        let state = self.state(live.map(|(state, _)| state.as_str()))?;
        let error = live.and_then(|(_, error)| error.clone());
        let list = |items: &[String]| SdkValue::List(items.iter().map(SdkValue::text).collect());
        let isolation = self.isolation_class();
        let mode = self.exec_mode().unwrap_or(ExecMode::Worker);
        let kernel = self.kernel_scope();
        #[cfg(feature = "machine")]
        let capability = super::capability_of(self);
        #[cfg(not(feature = "machine"))]
        let capability = SdkValue::map([
            ("schema_version", SdkValue::Int(contract::SCHEMA_VERSION)),
            (
                "isolation",
                SdkValue::opt_text(isolation.map(|class| class.as_str())),
            ),
            (
                "kernel",
                SdkValue::opt_text(kernel.map(|scope| scope.as_str())),
            ),
        ]);
        let mut fields = vec![
            ("name", SdkValue::text(&self.name)),
            // #551: `kind` is the isolation class only; the adapter is
            // `platform` (a constructor, not a layer).
            (
                "kind",
                SdkValue::opt_text(isolation.map(|class| class.as_str())),
            ),
            ("platform", SdkValue::text(kind.name())),
            ("backend", SdkValue::text(super::spec::backend_name(kind))),
            ("supports", super::platforms::supports(kind)),
            (
                "schema_version",
                SdkValue::Int(contract::schema_version(&self.fields)),
            ),
            (
                "isolation_class",
                SdkValue::opt_text(isolation.map(|class| class.as_str())),
            ),
            ("exec_mode", SdkValue::text(mode.as_str())),
            (
                "kernel",
                SdkValue::opt_text(kernel.map(|scope| scope.as_str())),
            ),
            ("state", SdkValue::text(state)),
            ("cpus", self.field("cpus").cloned().unwrap_or_default()),
            ("memory", self.field("memory").cloned().unwrap_or_default()),
            ("disk", self.field("disk").cloned().unwrap_or_default()),
            ("stores", list(&derived.stores)),
            ("sandboxes", list(&derived.sandboxes)),
            (
                "isolation",
                isolation
                    .map(|class| isolation_sdk(None, class))
                    .unwrap_or_default(),
            ),
            ("scheduling", scheduling_sdk(mode)),
            ("capability", capability),
            (
                "health",
                SdkValue::map([
                    ("ok", SdkValue::Bool(matches!(state, "ready" | "running"))),
                    ("state", SdkValue::text(state)),
                    ("reason", SdkValue::opt_text(error.clone())),
                ]),
            ),
            (
                "worker",
                derived
                    .worker
                    .clone()
                    .unwrap_or_else(|| SdkValue::map([("state", SdkValue::text("none"))])),
            ),
            (
                "lease",
                derived
                    .lease
                    .clone()
                    .unwrap_or_else(|| SdkValue::map([("held", SdkValue::Bool(false))])),
            ),
        ];
        for extra in [
            "image", "host", "nested", "docker", "wsl", "daemon", "error",
        ] {
            if let Some(value) = self
                .field(extra)
                .filter(|_| extra != "error" || error.is_none())
            {
                fields.push((extra, value.clone()));
            }
        }
        if let Some(error) = error {
            fields.push(("error", SdkValue::Text(error)));
        }
        if let Some(ssh) = self.ssh() {
            fields.push(("ssh", ssh.clone()));
        }
        Ok(SdkValue::Map(
            fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
        ))
    }
}

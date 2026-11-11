//! A machine's record, `machine/<name>/record.json` (#536 §9.2): its
//! platform, size, images, lasting guest state, the state a create, start
//! or stop left, the `op` of a create or remove in flight (#536 §7.1), and
//! the built-in machines' settings. A record may still name a platform
//! removed in 2.0 (`firecracker`, `microsandbox`, `ssh`): it is shown with
//! that reason, every other call on it is refused `Unsupported`, and
//! `machine remove` forgets it. Read once and kept whole, so `machine.get`
//! and `machine.list` show what they showed before. Built without the `machine` feature too (the guest
//! worker reads its own `native` and `docker` records); the platform and
//! the summary need the adapters.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths, read_record, with_field, write_entity};
use layerfs_home::op::{self, Op, Standing};
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The intents a machine's record holds (#536 §7.1).
pub const CREATE: &str = "machine.create";
/// A VM machine's remove.
pub const REMOVE: &str = "machine.remove";

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
        layerfs_home::names::MachineName::parse(name)?;
        Self::read(home, name)?
            .ok_or_else(|| refused("NotFound", format!("no machine named {name}")))
    }

    /// A built-in machine's first record: its name, platform and now.
    pub fn built_in(name: &str) -> Self {
        Self::from_record(
            name,
            SdkValue::map([
                ("name", SdkValue::text(name)),
                ("platform", SdkValue::text(name)),
                ("created", SdkValue::Int(home::now_ms())),
            ]),
        )
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

    /// The state a create, start or stop recorded (`running`,
    /// `stopped`, `failed`).
    pub fn recorded_state(&self) -> Option<&str> {
        self.fields.get_text("state")
    }

    /// The intent of a create or remove, if the record holds one.
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

    /// Whether a create or remove holds the machine:
    /// its intent's owner lives, or is gone with the outcome unknown. A list
    /// shows its state and probes nothing.
    pub fn in_flight(&self) -> bool {
        self.op().is_some_and(|op| op.standing() != Standing::Stale)
    }

    /// Refuses a new operation on the machine while an intent holds it:
    /// `Busy` while its owner lives, `Unresolved` once a gone owner left
    /// it uncertain, naming `machine status <name>` and the call that
    /// clears it (`machine remove`).
    pub fn not_held(&self) -> Result<()> {
        self.op().map_or(Ok(()), |op| self.blocked_by(&op))
    }

    /// Refuses only while a live process holds the machine (`Busy`): what
    /// a remove, which clears an unresolved intent, checks.
    pub fn not_running(&self) -> Result<()> {
        match self.op() {
            Some(op) if op.standing() == Standing::Running => self.blocked_by(&op),
            _ => Ok(()),
        }
    }

    fn blocked_by(&self, op: &Op) -> Result<()> {
        op.blocks("machine", &self.name, "machine status", "machine remove")
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
}

#[cfg(feature = "machine")]
impl MachineRecord {
    /// The machine's platform; a record naming one removed in 2.0 is
    /// refused `Unsupported` with [`Self::removed`]'s reason, one naming
    /// none LayerFS knows `Failed`.
    pub fn platform(&self) -> Result<layerfs_platform::Platform> {
        if let Some(reason) = self.removed() {
            return Err(refused("Unsupported", reason));
        }
        self.platform_name()
            .and_then(layerfs_platform::Platform::parse)
            .ok_or_else(|| refused("Failed", "a machine record names no known platform"))
    }

    /// Why the record's platform is gone, when it names one removed in 2.0
    /// (`firecracker`, `microsandbox`, `ssh`).
    pub fn removed(&self) -> Option<&'static str> {
        self.platform_name()
            .and_then(layerfs_platform::platform_policy::removed_reason)
    }

    /// The machine's state (#536 §7.2), from its record, `op`, `error` and
    /// live probe: `creating` or `removing` while a live process creates or
    /// removes it, `unresolved` once a gone one
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
                Standing::Running if op.name() == REMOVE => return Ok("removing"),
                Standing::Running => return Ok("creating"),
                Standing::Stale => {}
            }
        }
        Ok(live.or(self.recorded_state()).unwrap_or(fallback))
    }

    /// The machine as `machine.get`, each `machine.list` item, `start`,
    /// `stop`, `update` and `create` show it: name, platform, backend,
    /// `supports`, state, size, Stores and Sandboxes, then the settings it
    /// has (`image`, `docker`, `wsl`, `error`, and a removed platform's
    /// `ssh`, `host` or `vm_name`). A machine of a removed
    /// platform has backend `-`, supports nothing, state `unsupported` and
    /// the removal as its `error`.
    pub fn to_sdk(
        &self,
        _view: layerfs_home::outcome::View,
        derived: &MachineDerived,
    ) -> Result<SdkValue> {
        let live = derived.live.as_ref();
        let mut error = live.and_then(|(_, error)| error.clone());
        let (platform, backend, supports, state) = match self.removed() {
            Some(reason) => {
                error = Some(reason.to_owned());
                let supports = SdkValue::map([
                    ("sqlite", SdkValue::Bool(false)),
                    ("postgresql_sandbox", SdkValue::Bool(false)),
                    ("reason", SdkValue::text(reason)),
                ]);
                let name = self.platform_name().unwrap_or_default();
                (name, "-", supports, "unsupported")
            }
            None => {
                let kind = self.platform()?;
                let state = self.state(live.map(|(state, _)| state.as_str()))?;
                let supports = super::platforms::supports(kind);
                (
                    kind.name(),
                    super::spec::backend_name(kind),
                    supports,
                    state,
                )
            }
        };
        let list = |items: &[String]| SdkValue::List(items.iter().map(SdkValue::text).collect());
        let mut fields = vec![
            ("name", SdkValue::text(&self.name)),
            ("platform", SdkValue::text(platform)),
            ("backend", SdkValue::text(backend)),
            ("supports", supports),
            ("state", SdkValue::text(state)),
            ("cpus", self.field("cpus").cloned().unwrap_or_default()),
            ("memory", self.field("memory").cloned().unwrap_or_default()),
            ("disk", self.field("disk").cloned().unwrap_or_default()),
            ("stores", list(&derived.stores)),
            ("sandboxes", list(&derived.sandboxes)),
        ];
        for extra in ["image", "os_image", "docker", "wsl", "daemon", "error"] {
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
        if self.removed().is_some() {
            for extra in ["ssh", "host", "vm_name"] {
                if let Some(value) = self.field(extra) {
                    fields.push((extra, value.clone()));
                }
            }
        }
        Ok(SdkValue::Map(
            fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
        ))
    }
}

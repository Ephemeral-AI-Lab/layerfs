//! A machine's record, `machine/<name>/record.json` (#536 §9.2): its
//! platform, size, images, lasting guest state, what a create, start, stop
//! or live read last found (`state`, and `reason` with `error` while it is
//! broken; `state.rs` derives the machine's state from them), the `op` of
//! a create, start, stop or remove in flight (#536 §7.1), and the built-in
//! machines' settings. A record may still name a platform removed in 2.0
//! (`firecracker`, `microsandbox`, `ssh`): it is shown `broken`
//! `unsupported`, every other call on it is refused `Unsupported`, and
//! `machine remove` removes its record alone. Read once and kept whole. Built without
//! the `machine` feature too (the guest worker reads its own `native` and
//! `docker` records); the platform and the summary need the adapters.
use layerfs_home::home::{self, paths, read_record, with_field, write_entity};
use layerfs_home::op::{self, Op, Standing};
use layerfs_home::outcome::{refused, shell_word, with_fix};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The intents a machine's record holds (#536 §7.1).
pub const CREATE: &str = "machine.create";
/// A VM machine's remove.
pub const REMOVE: &str = "machine.remove";
/// A machine's start.
pub const START: &str = "machine.start";
/// A machine's stop, forced or not.
pub const STOP: &str = "machine.stop";

/// A machine name as provider names take it: every character that is not
/// an ASCII letter or digit becomes `-`.
fn plain(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The provider's name for a new machine's VM or worker container:
/// `layerfs-<home id8>-<machine>`, so two homes on one host never share
/// one (O33; `layerfs-` since layerfs-project#180). Recorded as `vm_name`
/// when the machine is made; read back by [`MachineRecord::provider_name`].
pub fn vm_name(home_id: &str, name: &str) -> String {
    let prefix = layerfs_home::names::provider_prefix(home_id);
    format!("{prefix}{}", plain(name))
}

/// Why machine `name`'s record without `vm_name` is not used: what made
/// it, that no call repairs it, and that nothing outside the home was
/// touched.
pub fn no_vm_name(name: &str) -> String {
    format!(
        "machine {name}'s record has no vm_name: it was made by a build before LayerFS named \
         machines layerfs-<home id8>-<machine>, and this build reads no older name. No call \
         repairs the record; its VM or container and its data volume were not touched"
    )
}

/// What is recorded on a machine, by name, as its remove's `InUse`
/// refusal names them: its Sandboxes, its sqlite Stores and the old
/// Firecracker machine records it hosted. Read by the caller
/// (`layerfs-control`'s inventory) from the records it already holds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Holders<'a> {
    /// The Sandboxes whose record names the machine.
    pub sandboxes: &'a [String],
    /// The sqlite Stores whose record names the machine.
    pub stores: &'a [String],
    /// The old Firecracker machine records naming it as their host.
    pub hosted: &'a [String],
}

/// The fix of a machine whose record has no `vm_name`: `machine remove`,
/// which then removes its record alone (a built-in `docker` is recorded
/// again, with its 2.0 name, by the next call). While things are recorded
/// on it (`held`), that remove is refused `InUse`, so the line forgets
/// them first, each of whose removes takes its records alone too:
/// Sandboxes first (they hold their projects' Stores), then the Stores and
/// the old Firecracker records, then the machine. The one builder of that
/// line: the `InUse` refusal, the machine's state and the doctor's finding
/// show it alike (RT5 note 3).
pub fn no_vm_name_fix(name: &str, held: Holders<'_>) -> String {
    let sandboxes = held.sandboxes.iter().map(|s| ("sandbox", s.as_str()));
    let stores = held.stores.iter().map(|s| ("store", s.as_str()));
    let hosted = held.hosted.iter().map(|m| ("machine", m.as_str()));
    let steps = (sandboxes.chain(stores).chain(hosted))
        .chain(std::iter::once(("machine", name)))
        .map(|(family, name)| format!("layerfs {family} remove {}", shell_word(name)));
    steps.collect::<Vec<_>>().join(" && ")
}

/// One machine's record as read (fields kept whole and in order).
#[derive(Clone, Debug)]
pub struct MachineRecord {
    name: String,
    fields: SdkValue,
}

impl MachineRecord {
    /// Checks recorded host power before a local Store or Sandbox is admitted.
    pub fn admit_host(home: &Path, name: &str) -> Result<()> {
        if name == layerfs_home::names::HOST_MACHINE
            && !layerfs_home::home::guest::is_root(home)
            && Self::read(home, name)?
                .is_some_and(|record| record.recorded_state() == Some("stopped"))
        {
            return Err(layerfs_home::outcome::with_fix(
                refused("Unavailable", format!("host machine {name} is stopped")),
                format!("layerfs machine start {name}"),
            ));
        }
        Ok(())
    }

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

    /// A built-in machine's first record, in the home with id `home_id`:
    /// its name, platform (a built-in machine's has its name) and now, and
    /// where a provider holds it (`docker`'s worker container) its provider
    /// name (`vm_name`), recorded like every created machine's
    /// (layerfs-project#180). `native`'s platform has no provider.
    pub fn built_in(name: &str, home_id: &str) -> Self {
        let platform = layerfs_home::names::built_in_platform(name).unwrap_or(name);
        let record = Self::from_record(
            name,
            SdkValue::map([
                ("name", SdkValue::text(name)),
                ("platform", SdkValue::text(platform)),
                ("created", SdkValue::Int(home::now_ms())),
            ]),
        );
        match crate::local::provided(platform) {
            false => record,
            true => record.with("vm_name", SdkValue::text(vm_name(home_id, name))),
        }
    }

    /// The name the machine's provider knows it by (its VM or worker
    /// container; its volume is `<name>-data`): the record's `vm_name`.
    /// A record without one was made before LayerFS named machines
    /// `layerfs-<home id8>-<machine>` (layerfs-project#180): refused
    /// [`no_vm_name`] with [`no_vm_name_fix`], since this build reads no
    /// older name.
    pub fn provider_name(&self) -> Result<&str> {
        self.text("vm_name").ok_or_else(|| {
            let fix = no_vm_name_fix(&self.name, Holders::default());
            with_fix(refused("Failed", no_vm_name(&self.name)), fix)
        })
    }

    /// Whether the record is one an earlier build made, with no provider
    /// name (`vm_name`): this build reaches nothing of it, so a remove of
    /// the machine, or of a Store or Sandbox recorded on it, takes records
    /// alone. A machine whose platform has no provider (`native`) has no
    /// provider name and is never nameless.
    pub fn nameless(&self) -> bool {
        self.provided() && self.text("vm_name").is_none()
    }

    /// Whether a provider holds the machine, as its platform says
    /// ([`crate::provided`]); a record that names no platform is one an
    /// earlier build made, which had one.
    pub fn provided(&self) -> bool {
        self.platform_name().is_none_or(crate::local::provided)
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

    /// The ordered incarnation the machine's last start recorded: a newer
    /// worker start mints a greater value, which admission compares with the
    /// incarnation a mount intent recorded. A record written before the field
    /// existed has none (no place proof, never death).
    pub fn incarnation(&self) -> Option<&str> {
        self.text("incarnation")
    }

    /// The record with the ordered `incarnation` its start minted.
    pub fn with_incarnation(self, incarnation: &str) -> Self {
        self.with("incarnation", SdkValue::text(incarnation))
    }

    /// The state a create, start or stop recorded (`running`,
    /// `stopped`, `failed`).
    pub fn recorded_state(&self) -> Option<&str> {
        self.fields.get_text("state")
    }

    /// The intent of a create, start, stop or remove, if the record holds one.
    pub fn op(&self) -> Option<Op> {
        Op::of(&self.fields)
    }

    /// The record holding intent `op`.
    pub fn with_op(self, op: &Op) -> Self {
        self.with(op::FIELD, op.to_record().clone())
    }

    /// The record with no intent.
    pub fn without_op(self) -> Self {
        self.without(op::FIELD)
    }

    /// The record without field `field`.
    pub fn without(self, field: &str) -> Self {
        let fields = match self.fields {
            SdkValue::Map(fields) => fields.into_iter().filter(|(n, _)| n != field),
            other => return Self::from_record(&self.name, other),
        };
        Self::from_record(&self.name, SdkValue::Map(fields.collect()))
    }

    /// Refuses a new operation on the machine while an intent holds it:
    /// `Busy` while its owner lives, `Unresolved` once a gone owner left
    /// it uncertain, with the call that settles it as its `fix`.
    pub fn not_held(&self) -> Result<()> {
        self.op().map_or(Ok(()), |op| self.held_by(&op))
    }

    /// Refuses only while a live process holds the machine (`Busy`): what
    /// a remove, which clears an unresolved intent, checks.
    pub fn not_running(&self) -> Result<()> {
        match self.op() {
            Some(op) if op.standing() == Standing::Running => self.held_by(&op),
            _ => Ok(()),
        }
    }

    /// The Sandbox image the record names (a digest, or a file).
    pub fn image(&self) -> Option<&str> {
        self.fields.get_text("image")
    }

    /// The docker machine's Docker socket; `None` when it has none or
    /// `--docker ""` cleared it (the field is then null).
    pub fn docker(&self) -> Option<&str> {
        self.fields.get_text("docker")
    }

    /// The daemon a built-in machine's Sandboxes run (`machine update --daemon` writes it).
    pub fn daemon(&self) -> Option<&str> {
        self.fields.get_text("daemon")
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

#[cfg(feature = "machine")]
impl MachineRecord {
    /// The machine's platform; a record naming one removed in 2.0 is
    /// refused `Unsupported` with [`Self::removed`]'s reason, one naming
    /// none LayerFS knows `Failed`.
    pub fn platform(&self) -> Result<crate::platform::Platform> {
        if let Some(reason) = self.removed() {
            return Err(refused("Unsupported", reason));
        }
        self.platform_name()
            .and_then(crate::platform::Platform::parse)
            .ok_or_else(|| refused("Failed", "a machine record names no known platform"))
    }

    /// Why the record's platform is gone, when it names one removed in 2.0
    /// (`firecracker`, `microsandbox`, `ssh`).
    pub fn removed(&self) -> Option<&'static str> {
        self.platform_name()
            .and_then(crate::platform::removed_platforms::removed_reason)
    }

    /// The machine's state as its record says it ([`Self::status`]), the
    /// record's platform checked against the ones removed in 2.0.
    pub fn recorded(&self, sandboxes: usize) -> layerfs_home::state::State {
        self.status(self.removed().is_some(), sandboxes)
    }

    /// [`Self::recorded`] with what is recorded on the machine named
    /// ([`Self::status_held`]): the state `machine list` and `machine get`
    /// show.
    pub fn recorded_held(&self, held: Holders<'_>) -> layerfs_home::state::State {
        self.status_held(self.removed().is_some(), held)
    }
}

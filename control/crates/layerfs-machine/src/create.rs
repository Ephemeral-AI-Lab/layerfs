//! The machine calls (S §3.7, O11, O34). Record writes take the home lock;
//! a create holds it only to claim and to record, never while
//! the VM boots. The record it claims holds its `op` (#536 §7.1), which
//! keeps every other call off the machine meanwhile; the write that records
//! it running drops the `op`. One attempt: a create that fails before its
//! VM exists removes what it made, one that fails after keeps the VM as
//! broken `failed`, and an `Unknown` outcome keeps everything, its `op`
//! marked uncertain, and is logged. Its phases are `bundle`, `boot` and
//! `worker` (call surface 2.0): the `op` moves through them and each is a
//! `{ phase, done, total }` event on the caller's stream.
use super::bundle::bundle;
use super::endpoint::{endpoint_of, enter};
use super::list::changed;
use super::progress::Phases;
use super::record::{MachineRecord, CREATE};
use super::spec::{self as record, home_id, machine_of, refusal, save_running};
use super::spec::{save_unready, update as update_record};
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, lock, paths};
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::state::Reason;
use layerfs_platform::{self as adapters, Code, Platform};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The longest name `machine.create` takes. A provider takes a VM name of
/// at most 63 characters (`layerfs-platform`'s plain-name rule, checked for
/// every platform but `native`), and `layerfs-<home id8>-` takes 17 of
/// them. Every platform `machine.create` makes is held to it; a machine
/// made earlier with a longer name keeps its recorded VM name.
const NEW_NAME_MAX: usize = 46;

/// A new machine's name: valid, not built in, short enough for its VM name
/// (and, where `kind`'s provider has a limit of its own on this host, for
/// that: [`provider_room`]) and not taken, here or at the provider. A VM name keeps no punctuation
/// (`a.b` and `a_b` both give `...-a-b`), so the second such machine is
/// refused `Exists`: the new machine's VM name, under today's formula or
/// the one before layerfs-project#180, must be no other machine's (its
/// recorded `vm_name`, else that earlier formula's). Checked under the
/// home lock, before any effect.
fn new_name(home: &Path, id: &str, kind: Platform, name: &str) -> Result<()> {
    layerfs_home::names::MachineName::parse(name)?;
    if layerfs_home::names::BUILT_IN.contains(&name) {
        return Err(refused(
            "InvalidInput",
            format!("{name} is a built-in machine"),
        ));
    }
    if name.len() > NEW_NAME_MAX {
        return Err(refused(
            "InvalidInput",
            format!(
                "machine name `{name}` has {} characters; a new machine's name is at most \
                 {NEW_NAME_MAX}: its VM name is at most 63 characters and its prefix \
                 `layerfs-<home id8>-` takes 17",
                name.len()
            ),
        ));
    }
    provider_room(kind, &record::vm_name(id, name), name)?;
    if paths::machine_dir(home, name).exists() {
        return Err(refused("Exists", format!("a machine named {name} exists")));
    }
    let wanted = [
        record::vm_name(id, name),
        super::record::legacy_vm_name(id, name),
    ];
    for other in paths::names(&home.join("machine")).map_err(failed)? {
        let Some(theirs) = MachineRecord::read(home, &other)? else {
            continue;
        };
        let taken = theirs.provider_name(id);
        if wanted.contains(&taken) {
            return Err(refused(
                "Exists",
                format!(
                    "machine {other} (VM name {taken}) and {name} differ only in punctuation, \
                     which a VM name does not keep; choose another name"
                ),
            ));
        }
    }
    Ok(())
}

/// Refuses a name whose VM name `vm` is longer than `kind`'s provider takes
/// on this host (layerfs-project#180): Lima's socket paths under its home
/// make the longest `lima` name shorter than [`NEW_NAME_MAX`] on most
/// hosts. The message states the longest name that fits here and why.
fn provider_room(kind: Platform, vm: &str, name: &str) -> Result<()> {
    let Some(limit) = adapters::name_limit(kind) else {
        return Ok(());
    };
    if vm.len() <= limit.longest {
        return Ok(());
    }
    let prefix = vm.len().saturating_sub(name.len());
    let fits = match limit.longest.saturating_sub(prefix) {
        0 => format!("no {} machine name fits on this host", kind.name()),
        room => format!(
            "on this host a {} machine's name is at most {room}",
            kind.name()
        ),
    };
    Err(refused(
        "InvalidInput",
        format!(
            "machine name `{name}` has {} characters; {fits}: {}, and its VM name's prefix \
             `layerfs-<home id8>-` takes {prefix}",
            name.len(),
            limit.why
        ),
    ))
}

/// The phases of a create.
const PHASES: [&str; 3] = ["bundle", "boot", "worker"];

/// What a create answers beside `ok` and `outcome`.
const ANSWER: [&str; 9] = [
    "machine",
    "platform",
    "backend",
    "cpus",
    "memory_bytes",
    "disk_bytes",
    "image",
    "os_image",
    "created_at",
];

/// The `os_image` that asks for the provider's stock image: the machine
/// records none, as one created before prepared images existed.
pub const STOCK: &str = "stock";

/// A `machine.create` request. The name stays text: it is checked under the
/// home lock, after the platform and the host's tools.
pub struct MachineCreate {
    /// The new machine's name.
    pub name: String,
    /// Its platform's name.
    pub platform: String,
    /// Its CPUs; absent, the default.
    pub cpus: Option<i64>,
    /// Its memory in bytes; absent, the default.
    pub memory: Option<i64>,
    /// Its disk in bytes; absent, the default.
    pub disk: Option<i64>,
    /// Its image.
    pub image: Option<String>,
    /// The VM OS image or prepared machine image, separate from its Sandbox
    /// image; [`STOCK`] for the provider's own, absent for the prepared one
    /// when there is one.
    pub os_image: Option<String>,
}

/// `machine.create(name, platform, { cpus, memory, disk, image })`.
pub fn create(
    home: &Path,
    events: &mut Events<'_>,
    request: &MachineCreate,
    _related: Related,
) -> Outcome {
    let home = home.to_path_buf();
    let name = request.name.as_str();
    let platform = request.platform.as_str();
    let kind = Platform::parse(platform)
        .ok_or_else(|| refused("InvalidInput", format!("no platform named {platform}")))?;
    if !kind.created() {
        return Err(refused(
            "InvalidInput",
            format!("a {platform} machine is built in"),
        ));
    }
    // a platform not on this host is refused in the support table's words
    // (#535 §7.3), before any tool is looked for
    let host = adapters::HostOs::current();
    if let Err(reason) = adapters::sandbox_allowed(kind, host, adapters::StoreKind::Sqlite) {
        return Err(refused("Unsupported", reason));
    }
    let probe = adapters::probe(kind, &record::tools(&home)?);
    if !probe.ready {
        return Err(refusal(adapters::MachineError::missing(
            format!("{platform} cannot be created on this host"),
            probe.missing,
        )));
    }
    let mut phases = Phases::new(events, &PHASES);
    phases.begin("bundle");
    let bundle = bundle()?;
    // `stock` is the provider's own image even when a prepared one exists;
    // absent, the prepared image when there is one, else the stock one
    let os_image = match request.os_image.as_deref() {
        Some(STOCK) => None,
        Some(image) => Some(image.to_owned()),
        None => super::bundle::prepared_os_image(kind, &bundle)?,
    };
    let id = home_id(&home)?;
    let mut fields = vec![
        ("name", SdkValue::text(name)),
        ("platform", SdkValue::text(kind.name())),
        (
            "cpus",
            SdkValue::Int(request.cpus.unwrap_or(record::DEFAULT_CPUS)),
        ),
        (
            "memory",
            SdkValue::Int(request.memory.unwrap_or(record::DEFAULT_MEMORY)),
        ),
        (
            "disk",
            SdkValue::Int(request.disk.unwrap_or(record::DEFAULT_DISK)),
        ),
        ("vm_name", SdkValue::text(record::vm_name(&id, name))),
        ("created", SdkValue::Int(home::now_ms())),
    ];
    if let Some(image) = &request.image {
        fields.push(("image", SdkValue::text(image)));
    }
    if let Some(image) = os_image {
        fields.push(("os_image", SdkValue::text(image)));
    }
    let fields = fields.into_iter().map(|(n, v)| (n.to_owned(), v));
    let op = Op::begin(CREATE, "bundle", SdkValue::Null)?;
    let record = MachineRecord::from_record(name, SdkValue::Map(fields.collect())).with_op(&op);
    // claim the name under the lock; the VM is made outside it
    let spec = {
        let _lock = lock::take(&home)?;
        new_name(&home, &id, kind, name)?;
        record.write(&home)?;
        record::spec(&home, &id, &record)
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(paths::machine_dir(&home, name));
            layerfs_home::home::generation::moved(&home);
            return Err(refusal);
        }
    };
    // a `machine start` waits for its create
    let _start = super::endpoint::start_lock(&home, name)?;
    let op = enter(&home, name, op, "boot", &mut phases)?;
    let outcome = adapters::create(&spec, &bundle);
    let op = match &outcome {
        Ok(running) => match checked(&home, name, op, running, &bundle.dir, &mut phases) {
            Ok(op) => op,
            Err((reason, error)) => {
                let _lock = lock::take(&home)?;
                save_unready(&home, name, running, reason, &error.message)?;
                return Err(error);
            }
        },
        Err(_) => op,
    };
    let answer = finish(&home, name, op, outcome)?;
    phases.end();
    Ok(answer)
}

/// The worker a create just started, asked once: `op` at phase `worker`
/// when it answers and is from this build; else why the machine is broken
/// (its worker does not answer, or its bundle is older than this build).
fn checked(
    home: &Path,
    name: &str,
    op: Op,
    running: &adapters::Running,
    bundle: &Path,
    phases: &mut Phases<'_, '_>,
) -> std::result::Result<Op, (Reason, SdkRefusal)> {
    let op = enter(home, name, op, "worker", phases).map_err(|why| (Reason::Failed, why))?;
    let endpoint = endpoint_of(&running.endpoint);
    let hello = crate::worker::hello(&endpoint).map_err(|why| (Reason::NotAnswering, why))?;
    match hello.stale_bundle(name, bundle, &endpoint.address.to_string()) {
        Some(stale) => Err((Reason::Outdated, stale)),
        None => Ok(op),
    }
}

/// Records the outcome of a create (H §7.1.1 step 5) that `op` names:
/// running drops it; a failure that kept the VM drops it, the machine
/// broken `failed`; an `Unknown` one keeps it, uncertain, with the `error`.
fn finish(
    home: &Path,
    name: &str,
    op: Op,
    outcome: std::result::Result<adapters::Running, adapters::MachineError>,
) -> Outcome {
    let _lock = lock::take(home)?;
    match outcome {
        Ok(running) => {
            save_running(home, name, &running)?;
            changed(&machine_of(home, name)?, "created", &ANSWER)
        }
        Err(error) if error.code == Code::Unknown => {
            let op = op.as_uncertain();
            update_record(home, name, |r| {
                r.with_op(&op).with("error", SdkValue::text(&error.message))
            })?;
            op.log_uncertain(home, "machine", name, &error.message);
            Err(refusal(error))
        }
        Err(error) if error.vm_exists => {
            update_record(home, name, |r| {
                (r.with("state", SdkValue::text("failed")))
                    .broken_as(Reason::Failed, &error.message)
                    .without_op()
            })?;
            Err(refusal(error))
        }
        Err(error) => {
            std::fs::remove_dir_all(paths::machine_dir(home, name)).map_err(failed)?;
            layerfs_home::home::generation::moved(home);
            Err(refusal(error))
        }
    }
}

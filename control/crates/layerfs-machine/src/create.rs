//! The machine calls (S §3.7, O11, O34). Record writes take the home lock;
//! a create holds it only to claim and to record, never while
//! the VM boots. The record it claims holds its `op` (#536 §7.1), which
//! keeps every other call off the machine meanwhile; the write that records
//! it running drops the `op`. One attempt: a create that fails before its
//! VM exists removes what it made, one that fails after keeps the VM as
//! `failed`, and an `Unknown` outcome keeps everything, its `op` marked
//! uncertain, and is logged.
use super::bundle::bundle;
use super::list::summary;
use super::record::{MachineRecord, CREATE};
use super::spec::{self as record, machine_of, refusal, save_running, update as update_record};
use super::status::home_id;
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, lock, paths};
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_platform::{self as adapters, Code, Platform};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A new machine's name: valid, not built in, not taken.
fn new_name(home: &Path, name: &str) -> Result<()> {
    layerfs_home::names::MachineName::parse(name)?;
    if layerfs_home::names::BUILT_IN.contains(&name) {
        return Err(refused(
            "InvalidInput",
            format!("{name} is a built-in machine"),
        ));
    }
    if paths::machine_dir(home, name).exists() {
        return Err(refused("Exists", format!("a machine named {name} exists")));
    }
    Ok(())
}

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
    /// The VM OS image or prepared machine image, separate from its Sandbox image.
    pub os_image: Option<String>,
}

/// `machine.create(name, platform, { cpus, memory, disk, image })`.
pub fn create(
    home: &Path,
    events: &mut Events<'_>,
    request: &MachineCreate,
    related: Related,
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
    let bundle = crate::progress::stage(events, "bundle-validation", bundle)?;
    let os_image = match &request.os_image {
        Some(image) => Some(image.clone()),
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
    let op = Op::begin(CREATE, "create", SdkValue::Null)?;
    let record = MachineRecord::from_record(name, SdkValue::Map(fields.collect())).with_op(&op);
    // claim the name under the lock; the VM is made outside it
    let spec = {
        let _lock = lock::take(&home)?;
        new_name(&home, name)?;
        record.write(&home)?;
        record::spec(&home, &id, &record)
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(paths::machine_dir(&home, name));
            return Err(refusal);
        }
    };
    events.progress(SdkValue::map([
        ("phase", SdkValue::text("create")),
        ("machine", SdkValue::text(name)),
    ]));
    // a `machine start` waits for its create
    let _start = super::endpoint::start_lock(&home, name)?;
    let outcome = crate::progress::stage(events, "provider-and-worker", || {
        adapters::create(&spec, &bundle)
    });
    if let Ok(running) = &outcome {
        let endpoint = crate::worker::Endpoint {
            address: running.endpoint.address,
            secret: running.endpoint.secret.clone(),
        };
        let checked = crate::progress::stage(events, "worker-compatibility", || {
            let hello = crate::worker::hello(&endpoint)?;
            match hello.stale_bundle(name, &bundle.dir, &endpoint.address.to_string()) {
                Some(refusal) => Err(refusal),
                None => Ok(()),
            }
        });
        if let Err(error) = checked {
            let _lock = lock::take(&home)?;
            record::save_unready(&home, name, running, &error.message)?;
            return Err(error);
        }
    }
    finish(&home, name, op, outcome, related)
}

/// Records the outcome of a create (H §7.1.1 step 5) that `op` names: running drops it; a failure that kept the VM drops it with state
/// `failed`; an `Unknown` one keeps it, uncertain, with the `error`.
fn finish(
    home: &Path,
    name: &str,
    op: Op,
    outcome: std::result::Result<adapters::Running, adapters::MachineError>,
    related: Related,
) -> Outcome {
    let _lock = lock::take(home)?;
    match outcome {
        Ok(running) => {
            save_running(home, name, &running)?;
            let record = machine_of(home, name)?;
            summary(home, &record, None, related)
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
                r.failed_as("failed", &error.message).without_op()
            })?;
            Err(refusal(error))
        }
        Err(error) => {
            std::fs::remove_dir_all(paths::machine_dir(home, name)).map_err(failed)?;
            Err(refusal(error))
        }
    }
}

//! The machine calls (S §3.7, O11, O34). Record writes take the home lock;
//! a create or connect holds it only to claim and to record, never while
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
use crate::common::context::Context;
use crate::common::home::{self, lock, paths};
use crate::common::op::Op;
use crate::common::outcome::{failed, refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_platform::{self as adapters, Code, Platform};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// A new machine's name: valid, not built in, not taken.
pub(super) fn new_name(home: &Path, name: &str) -> Result<()> {
    crate::common::names::MachineName::parse(name)?;
    if crate::common::names::BUILT_IN.contains(&name) {
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

/// `machine.create(name, platform, { cpus, memory, disk, image, nested, host })`.
pub fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("").to_owned();
    let platform = args.get_text("platform").unwrap_or("");
    let kind = Platform::parse(platform)
        .ok_or_else(|| refused("InvalidInput", format!("no platform named {platform}")))?;
    if !kind.created() {
        return Err(refused(
            "InvalidInput",
            format!(
                "a {platform} machine is {}",
                if kind == Platform::Ssh {
                    "connected, not created"
                } else {
                    "built in"
                }
            ),
        ));
    }
    if args.get("nested").is_some() && kind != Platform::Lima {
        return Err(refused("InvalidInput", "only a lima machine is nested"));
    }
    if kind == Platform::Firecracker && args.get_text("host").is_none() {
        return Err(refused(
            "InvalidInput",
            "a firecracker machine names its lima --nested host",
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
    let bundle = bundle()?;
    let id = home_id(&home)?;
    let mut fields = vec![
        ("name", SdkValue::text(&name)),
        ("platform", SdkValue::text(kind.name())),
        (
            "cpus",
            SdkValue::Int(args.get_int("cpus").unwrap_or(record::DEFAULT_CPUS)),
        ),
        (
            "memory",
            SdkValue::Int(args.get_int("memory").unwrap_or(record::DEFAULT_MEMORY)),
        ),
        (
            "disk",
            SdkValue::Int(args.get_int("disk").unwrap_or(record::DEFAULT_DISK)),
        ),
        ("vm_name", SdkValue::text(record::vm_name(&id, &name))),
        ("created", SdkValue::Int(home::now_ms())),
    ];
    if let Some(image) = args.get_text("image") {
        fields.push(("image", SdkValue::text(image)));
    }
    if matches!(args.get("nested"), Some(SdkValue::Bool(true))) {
        fields.push(("nested", SdkValue::Bool(true)));
    }
    if let Some(host) = args.get_text("host") {
        fields.push(("host", SdkValue::text(host)));
    }
    let fields = fields.into_iter().map(|(n, v)| (n.to_owned(), v));
    let op = Op::begin(CREATE, "create", SdkValue::Null)?;
    let record = MachineRecord::from_record(&name, SdkValue::Map(fields.collect())).with_op(&op);
    // claim the name under the lock; the VM is made outside it
    let spec = {
        let _lock = lock::take(&home)?;
        new_name(&home, &name)?;
        if let Some(host) = args.get_text("host") {
            let host_record = machine_of(&home, host)?;
            if host_record.platform()? != Platform::Lima || !host_record.nested() {
                return Err(refused(
                    "InvalidInput",
                    format!("{host} is not a lima machine made with --nested"),
                ));
            }
        }
        record.write(&home)?;
        record::spec(&home, &id, &record)
    };
    let spec = match spec {
        Ok(spec) => spec,
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(paths::machine_dir(&home, &name));
            return Err(refusal);
        }
    };
    context.progress(SdkValue::map([
        ("phase", SdkValue::text("create")),
        ("machine", SdkValue::text(&name)),
    ]));
    // a `machine start` waits for its create
    let _start = super::endpoint::start_lock(&home, &name)?;
    finish(&home, &name, op, adapters::create(&spec, &bundle))
}

/// Records the outcome of a create or connect (H §7.1.1 step 5) that `op`
/// names: running drops it; a failure that kept the VM drops it with state
/// `failed`; an `Unknown` one keeps it, uncertain, with the `error`.
pub(super) fn finish(
    home: &Path,
    name: &str,
    op: Op,
    outcome: std::result::Result<adapters::Running, adapters::MachineError>,
) -> Outcome {
    let _lock = lock::take(home)?;
    match outcome {
        Ok(running) => {
            save_running(home, name, &running)?;
            let record = machine_of(home, name)?;
            summary(home, &record, None)
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

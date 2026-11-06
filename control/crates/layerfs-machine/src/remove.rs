//! `machine.remove` and `machine.disconnect`.
use super::record::{MachineRecord, DISCONNECT, REMOVE};
use super::spec::{self as record, refusal};
use super::status::home_id;
use crate::Related;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{lock, paths};
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_platform::{self as adapters, Code, Platform};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The home lock covers only the checks and the record writes: the record
/// holds this call's `op` while the VM is deleted under the machine's start
/// lock, so no call starts it, moves to it or runs on it meanwhile. It is
/// refused `Busy` while a live process holds the machine; it clears a gone
/// one's unresolved intent. A refused delete puts the record back as it
/// was; one whose outcome is unknown keeps its `op`, uncertain, with the
/// `error`, and is logged.
fn take_down(home: &Path, args: &SdkValue, ssh: bool, related: Related) -> Outcome {
    let home = home.to_path_buf();
    let name = args.get_text("name").unwrap_or("");
    MachineRecord::of(&home, name)?.not_running()?;
    let _start = super::endpoint::start_lock(&home, name)?;
    let (record, spec, op) = {
        let _lock = lock::take(&home)?;
        let (record, spec) = removable(&home, name, ssh, related)?;
        let op = Op::begin(
            if ssh { DISCONNECT } else { REMOVE },
            "delete",
            SdkValue::Null,
        )?;
        record.clone().with_op(&op).write(&home)?;
        (record, spec, op)
    };
    if let Err(error) = adapters::remove(&spec) {
        // an unknown outcome says so in the record; a refused one leaves it as it was
        let _lock = lock::take(&home)?;
        if error.code == Code::Unknown {
            let op = op.as_uncertain();
            let message = SdkValue::text(&error.message);
            record.with_op(&op).with("error", message).write(&home)?;
            op.log_uncertain(&home, "machine", name, &error.message);
        } else {
            record.write(&home)?;
        }
        return Err(refusal(error));
    }
    let _lock = lock::take(&home)?;
    std::fs::remove_dir_all(paths::machine_dir(&home, name)).map_err(failed)?;
    Ok(SdkValue::map([
        ("name", SdkValue::text(name)),
        ("removed", SdkValue::Bool(true)),
    ]))
}

/// The checks before a remove or disconnect, under the home lock: the
/// machine's record and spec.
fn removable(
    home: &Path,
    name: &str,
    ssh: bool,
    related: Related,
) -> Result<(MachineRecord, adapters::Spec)> {
    let record = MachineRecord::of(home, name)?;
    record.not_running()?;
    let kind = record.platform()?;
    if kind.built_in() {
        return Err(refused(
            "InvalidInput",
            format!("{name} is built in and never removed"),
        ));
    }
    if (kind == Platform::Ssh) != ssh {
        let call = if ssh {
            "machine remove"
        } else {
            "machine disconnect"
        };
        return Err(refused(
            "InvalidInput",
            format!("{name} is a {} machine; use {call}", kind.name()),
        ));
    }
    (related.can_remove)(home, name)?;
    let spec = record::spec(home, &home_id(home)?, &record)?;
    Ok((record, spec))
}

/// `machine.remove(name)`: deletes a VM and its disk.
pub fn remove(home: &Path, args: &SdkValue, related: Related) -> Outcome {
    take_down(home, args, false, related)
}

/// `machine.disconnect(name)`: forgets an ssh host; its data root stays.
pub fn disconnect(home: &Path, args: &SdkValue, related: Related) -> Outcome {
    take_down(home, args, true, related)
}

//! `machine.remove` and `machine.disconnect`.
use super::record::MachineRecord;
use super::spec::{self as record, machine_of, refusal};
use super::status::home_id;
use crate::common::context::Context;
use crate::common::home::{lock, paths};
use crate::common::outcome::{failed, refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine::{self as adapters, Code, Platform};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The home lock covers only the checks and the record writes: the record
/// says `removing` (with this process) while the VM is deleted under the
/// machine's start lock, so no call starts it, moves to it or runs on it
/// meanwhile. A refused delete puts the record back as it was; one whose
/// outcome is unknown records `unknown`.
fn take_down(context: &mut Context<'_>, args: &SdkValue, ssh: bool) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    machine_of(&home, name)?;
    let _start = super::endpoint::start_lock(&home, name)?;
    let (record, spec) = {
        let _lock = lock::take(&home)?;
        let (record, spec) = removable(&home, name, ssh)?;
        record.clone().marked_removing().write(&home)?;
        (record, spec)
    };
    if let Err(error) = adapters::remove(&spec) {
        // an unknown outcome says so in the record; a refused one leaves it as it was
        let record = match error.code {
            Code::Unknown => record.failed_as("unknown", &error.message),
            _ => record,
        };
        let _lock = lock::take(&home)?;
        record.write(&home)?;
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
fn removable(home: &Path, name: &str, ssh: bool) -> Result<(MachineRecord, adapters::Spec)> {
    let record = machine_of(home, name)?;
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
    crate::inventory::can_remove_machine(home, name)?;
    let spec = record::spec(home, &home_id(home)?, &record)?;
    Ok((record, spec))
}

/// `machine.remove(name)`: deletes a VM and its disk.
pub fn remove(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    take_down(context, args, false)
}

/// `machine.disconnect(name)`: forgets an ssh host; its data root stays.
pub fn disconnect(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    take_down(context, args, true)
}

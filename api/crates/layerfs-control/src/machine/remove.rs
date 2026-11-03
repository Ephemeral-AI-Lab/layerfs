//! `machine.remove` and `machine.disconnect`.
use super::spec::{
    self as record, hosted_on, kind_of, machine_of, refusal, sandboxes_on, stores_on,
};
use super::status::home_id;
use crate::common::context::Context;
use crate::common::home::{lock, paths, with_field, write_record};
use crate::common::outcome::{failed, refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine::{self as adapters, Code, Platform};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `InUse` while the machine holds Stores, Sandboxes or Firecracker machines.
fn idle(home: &Path, name: &str) -> Result<()> {
    let stores = stores_on(home, name)?;
    let sandboxes = sandboxes_on(home, name)?;
    let hosted = hosted_on(home, name)?;
    if stores.is_empty() && sandboxes.is_empty() && hosted.is_empty() {
        return Ok(());
    }
    Err(refused(
        "InUse",
        format!(
            "machine {name} holds Stores [{}], Sandboxes [{}] and hosts [{}]",
            stores.join(", "),
            sandboxes.join(", "),
            hosted.join(", ")
        ),
    ))
}

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
        let marked = with_field(
            record.clone(),
            "removing",
            SdkValue::map([("pid", SdkValue::Int(i64::from(std::process::id())))]),
        );
        write_record(&paths::machine_record(&home, name), &marked)?;
        (record, spec)
    };
    if let Err(error) = adapters::remove(&spec) {
        // an unknown outcome says so in the record; a refused one leaves it as it was
        let record = match error.code {
            Code::Unknown => with_field(
                with_field(record, "state", SdkValue::text("unknown")),
                "error",
                SdkValue::text(&error.message),
            ),
            _ => record,
        };
        let _lock = lock::take(&home)?;
        write_record(&paths::machine_record(&home, name), &record)?;
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
fn removable(home: &Path, name: &str, ssh: bool) -> Result<(SdkValue, adapters::Spec)> {
    let record = machine_of(home, name)?;
    let kind = kind_of(&record)?;
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
    idle(home, name)?;
    let spec = record::spec(home, &home_id(home)?, name, &record)?;
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

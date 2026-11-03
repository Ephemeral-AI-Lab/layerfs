//! `machine.start` and `machine.stop` (#535 P1): for UIs, which start a
//! machine before they use it and never rely on a list or get to start one
//! (P4). `start` starts a stopped machine and its worker and waits until it
//! is ready; a running one answers at once. `stop` stops the worker and,
//! where there is one, the VM; its disks stay. Built-in `native` answers
//! both at once. Each answers the machine's summary as `machine.get` does.
use super::endpoint::{running, start_lock};
use super::record::{self, hosted_on, kind_of, machine_of, refusal, sandboxes_on};
use super::status::home_id;
use crate::common::context::Context;
use crate::common::home::{lock, paths, with_field};
use crate::common::outcome::{failed, refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine::{self as adapters, Platform};
use std::path::Path;

/// `machine.start(name)`: the machine's worker answers its secret, or the
/// machine and its worker are started (under its `start.lock`) and waited
/// for. A readiness wait, not a retry.
pub fn start(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    if kind_of(&machine_of(&context.home, name)?)? != Platform::Native {
        running(&context.home, name)?;
    }
    super::get::get(context, args)
}

/// `machine.stop(name)`: refused `InUse` while Sandboxes run on it (or on a
/// Firecracker machine it hosts), `detail.related` naming each; else its
/// worker and VM stop under its `start.lock`, so no on-demand start runs
/// meanwhile, and `worker.json` goes. Its disks and Stores stay.
pub fn stop(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    let record = machine_of(&home, name)?;
    if kind_of(&record)? == Platform::Native {
        return super::get::get(context, args);
    }
    let _start = start_lock(&home, name)?;
    let spec = {
        let _lock = lock::take(&home)?;
        let record = machine_of(&home, name)?;
        idle(&home, name)?;
        record::spec(&home, &home_id(&home)?, name, &record)?
    };
    adapters::stop(&spec).map_err(refusal)?;
    {
        let _lock = lock::take(&home)?;
        match std::fs::remove_file(paths::worker_record(&home, name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(failed(error)),
        }
        record::update(&home, name, |record| {
            with_field(record, "state", SdkValue::text("stopped"))
        })?;
    }
    super::get::get(context, args)
}

/// `InUse` while a Sandbox is recorded on machine `name` or on a Firecracker
/// machine it hosts; `detail.related` lists them as `{kind: "sandbox",
/// name}`. Under the home lock.
fn idle(home: &Path, name: &str) -> Result<(), SdkRefusal> {
    let mut sandboxes = sandboxes_on(home, name)?;
    for hosted in hosted_on(home, name)? {
        sandboxes.extend(sandboxes_on(home, &hosted)?);
    }
    if sandboxes.is_empty() {
        return Ok(());
    }
    let mut refusal = refused(
        "InUse",
        format!(
            "machine {name} runs Sandboxes [{}]; close them first",
            sandboxes.join(", ")
        ),
    );
    let related = sandboxes
        .into_iter()
        .map(|sandbox| {
            SdkValue::map([
                ("kind", SdkValue::text("sandbox")),
                ("name", SdkValue::Text(sandbox)),
            ])
        })
        .collect();
    refusal.detail = SdkValue::map([("related", SdkValue::List(related))]);
    Err(refusal)
}

//! `machine.start` and `machine.stop` (#535 P1): for UIs, which start a
//! machine before they use it and never rely on a list or get to start one
//! (P4). `start` starts a stopped machine and its worker and waits until it
//! is ready; a running one answers at once. `stop` stops the worker and,
//! where there is one, the VM; its disks stay. Built-in `native` answers
//! both at once. Each answers the machine's summary as `machine.get` does.
use super::endpoint::{running, start_lock};
use super::spec::{self as record, machine_of, refusal};
use super::status::home_id;
use crate::Related;
use layerfs_bridge::control::SdkValue;
use layerfs_home::context::Context;
use layerfs_home::home::{lock, paths};
use layerfs_home::outcome::{failed, Outcome};
use layerfs_platform::{self as adapters, Platform};

/// `machine.start(name)`: the machine's worker answers its secret, or the
/// machine and its worker are started (under its `start.lock`) and waited
/// for. A readiness wait, not a retry.
pub fn start(context: &mut Context<'_>, args: &SdkValue, related: Related) -> Outcome {
    let name = args.get_text("name").unwrap_or("");
    if machine_of(&context.home, name)?.platform()? != Platform::Native {
        running(&context.home, name)?;
    }
    super::get::get(context, args, related)
}

/// `machine.stop(name)`: refused `InUse` while Sandboxes run on it (or on a
/// Firecracker machine it hosts), `detail.related` naming each; else its
/// worker and VM stop under its `start.lock`, so no `machine.start` runs
/// meanwhile, and `worker.json` goes. Its disks and Stores stay.
pub fn stop(context: &mut Context<'_>, args: &SdkValue, related: Related) -> Outcome {
    let home = context.home.clone();
    let name = args.get_text("name").unwrap_or("");
    let record = machine_of(&home, name)?;
    if record.platform()? == Platform::Native {
        return super::get::get(context, args, related);
    }
    let _start = start_lock(&home, name)?;
    let spec = {
        let _lock = lock::take(&home)?;
        let record = machine_of(&home, name)?;
        (related.can_stop)(&home, name)?;
        record::spec(&home, &home_id(&home)?, &record)?
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
            record.with("state", SdkValue::text("stopped"))
        })?;
    }
    super::get::get(context, args, related)
}

//! Explicit provider crash stop without a worker request, VM start or data deletion.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, lock, paths};
use layerfs_home::names::MachineName;
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_machine::platform::{self as adapters, Platform};
use layerfs_machine::record::MachineRecord;
use layerfs_machine::{machine_refusal as refusal, machine_spec};
use layerfs_sandbox::record::SandboxRecord;
use layerfs_sandbox::{direct, files, reopen};
use std::path::Path;
type Result<T> = std::result::Result<T, SdkRefusal>;

pub(crate) fn stop(home: &Path, name: &MachineName) -> Outcome {
    let home = home.to_path_buf();
    let name = name.as_str();
    force_record(&home, name)?;
    let _start = layerfs_machine::force_stop_lock(&home, name)?;
    let intent = Op::begin(
        "machine.stop",
        "force",
        SdkValue::map([("force", SdkValue::Bool(true))]),
    )?;
    let (spec, hosted, sandboxes) = {
        let _home = lock::take(&home)?;
        let machine = force_record(&home, name)?;
        let hosted = inventory::hosted_on(&home, name)?;
        let sandboxes = inventory::sandboxes(&home)?
            .into_iter()
            .filter(|sandbox| {
                sandbox.machine() == name || hosted.iter().any(|child| sandbox.machine() == child)
            })
            .collect::<Vec<_>>();
        for sandbox in &sandboxes {
            if sandbox
                .op()
                .is_some_and(|op| op.name() == "sandbox.create" && op.owner_alive())
            {
                return Err(refused(
                    "Busy",
                    format!(
                        "Sandbox {} is still being created; no machine stop was attempted",
                        sandbox.name()
                    ),
                ));
            }
        }
        let spec = machine_spec(&home, &home_id(&home)?, &machine)?;
        for child in &hosted {
            force_record(&home, child)?;
        }
        for machine in std::iter::once(name).chain(hosted.iter().map(String::as_str)) {
            update(&home, machine, |record| record.with_op(&intent))?;
        }
        (spec, hosted, sandboxes)
    };
    let mut unknown = Vec::new();
    let mut stopped = Vec::new();
    let mut skipped = Vec::new();
    let mut native_attempt = None;
    let attempted = (|| -> Result<()> {
        if spec.kind == Platform::Native {
            let mut first_refusal = None;
            for sandbox in &sandboxes {
                // A refusal before the signal leaves this runtime as it was,
                // so the stop goes on to the next Sandbox: only a sent stop
                // makes an outcome uncertain.
                let runtime = match native_runtime(&home, sandbox) {
                    Ok(runtime) => runtime,
                    Err(refusal) => {
                        skipped.push(sandbox.name().to_owned());
                        first_refusal.get_or_insert(refusal);
                        continue;
                    }
                };
                match runtime {
                    // Nothing ever ran: nothing is stopped or recorded.
                    Native::NeverStarted => continue,
                    Native::Stopped => {}
                    Native::Stop(mut runtime) => {
                        native_attempt = Some(sandbox);
                        runtime.stop(0).map_err(failed)?;
                    }
                }
                mark(&home, sandbox, true)?;
                native_attempt = None;
                stopped.push(sandbox.name().to_owned());
                unknown.extend(workspaces(&home, sandbox)?);
            }
            if let Some(refusal) = first_refusal {
                return Err(refusal);
            }
        } else {
            adapters::stop_force(&spec).map_err(refusal)?;
            for sandbox in &sandboxes {
                mark(&home, sandbox, true)?;
                stopped.push(sandbox.name().to_owned());
                let observed = workspaces(&home, sandbox)?;
                if observed.is_empty() {
                    unknown.push(format!("{}:*", sandbox.name()));
                } else {
                    unknown.extend(observed);
                }
            }
        }
        Ok(())
    })();
    if let Err(mut error) = attempted {
        // Native Sandboxes are recorded one by one as they stop, so only the
        // one being signalled is uncertain; a provider stop covers them all.
        let uncertain = match spec.kind {
            Platform::Native => native_attempt.is_some(),
            _ => error.code != "Unsupported" || !stopped.is_empty(),
        };
        // Recording the outcome must not replace the stop's own refusal: a
        // failure here is reported beside it.
        let mut custody = Vec::new();
        let mut keep = |result: Result<()>| {
            if let Err(failure) = result {
                custody.push(SdkValue::text(format!(
                    "{}: {}",
                    failure.code, failure.message
                )));
            }
        };
        keep((|| {
            let _home = lock::take(&home)?;
            for machine in std::iter::once(name).chain(hosted.iter().map(String::as_str)) {
                update(&home, machine, |record| {
                    if uncertain {
                        record.with_op(&intent.clone().as_uncertain())
                    } else {
                        record.without_op()
                    }
                })?;
            }
            Ok(())
        })());
        // A multi-container provider stop may have completed only part of its work.
        // Keep that uncertainty distinct from both a running or a confirmed gone runtime.
        if spec.kind != Platform::Native && error.code != "Unsupported" {
            for sandbox in &sandboxes {
                if !stopped.iter().any(|name| name == sandbox.name()) {
                    keep(mark(&home, sandbox, false));
                }
            }
        } else if let Some(sandbox) = native_attempt {
            if !stopped.iter().any(|name| name == sandbox.name()) {
                keep(mark(&home, sandbox, false));
            }
        }
        for (key, list) in [
            ("stopped_sandboxes", stopped),
            ("refused_sandboxes", skipped),
        ] {
            error.detail = home::with_field(
                error.detail,
                key,
                SdkValue::List(list.into_iter().map(SdkValue::Text).collect()),
            );
        }
        error.detail = home::with_field(
            error.detail,
            "unknown",
            SdkValue::List(unknown.into_iter().map(SdkValue::Text).collect()),
        );
        if !custody.is_empty() {
            error.detail =
                home::with_field(error.detail, "custody_errors", SdkValue::List(custody));
        }
        return Err(error);
    }
    {
        let _home = lock::take(&home)?;
        for machine in std::iter::once(name).chain(hosted.iter().map(String::as_str)) {
            match std::fs::remove_file(paths::worker_record(&home, machine)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            update(&home, machine, |record| {
                record.without_op().with(
                    "state",
                    SdkValue::text(if machine == "native" {
                        "ready"
                    } else {
                        "stopped"
                    }),
                )
            })?;
        }
    }
    // Do not call get(): a live probe could contact the worker we just stopped.
    let mut out = layerfs_machine::stopped_summary(&home, name, crate::machine::RELATED)?;
    for (key, value) in [
        ("forced", SdkValue::Bool(true)),
        (
            "stopped_sandboxes",
            SdkValue::List(stopped.into_iter().map(SdkValue::Text).collect()),
        ),
        (
            "unknown",
            SdkValue::List(unknown.into_iter().map(SdkValue::Text).collect()),
        ),
    ] {
        out = home::with_field(out, key, value);
    }
    Ok(out)
}

/// A new explicit force call may reconcile an unknown stop that this
/// process left, or one whose owner has ended (`Unresolved`); one a live
/// process still holds stays `Busy`. It never silently retries, nor clears
/// another kind of unresolved operation.
fn force_record(home: &Path, name: &str) -> Result<MachineRecord> {
    let record = MachineRecord::of(home, name)?;
    if record.op().is_some_and(|op| {
        op.name() == "machine.stop" && op.uncertain() && (op.mine() || !op.owner_alive())
    }) {
        return Ok(record);
    }
    record.not_held()?;
    Ok(record)
}

/// Record an observed terminal fact, retaining runtime files and exact identity.
fn mark(home: &Path, sandbox: &SandboxRecord, stopped: bool) -> Result<()> {
    let _home = lock::take(home)?;
    let _sandbox = layerfs_sandbox::record::lock(home, sandbox.name())?;
    if let Some(now) = inventory::sandbox(home, sandbox.name())? {
        if now.to_record().get("runtime_id") != sandbox.to_record().get("runtime_id") {
            return Err(refused(
                "Unknown",
                "the Sandbox runtime identity changed during the stop; its new record was retained",
            ));
        }
        now.with_machine_stop(stopped).write(home)?;
    }
    if stopped {
        direct::release(home, sandbox);
    }
    Ok(())
}

/// Native is the host itself: only this home's recorded runtime processes
/// stop. The Sandbox's recorded native runtime, reopened for a stop without
/// sending anything; `None` when there is nothing left to stop.
/// What a forced stop finds for one native Sandbox.
enum Native {
    /// Its recorded runtime, reopened for the stop; nothing sent yet.
    Stop(Box<layerfs_sandbox_backend::Sandbox>),
    /// Its runtime is already stopped.
    Stopped,
    /// It never started (no id and no runtime record).
    NeverStarted,
}

fn native_runtime(home: &Path, sandbox: &SandboxRecord) -> Result<Native> {
    if sandbox.machine_stopped() {
        return Ok(Native::Stopped);
    }
    let host = layerfs_sandbox::force::native_host(home, sandbox.name())?;
    let Some(daemon) = files::started(&sandbox.dir(home))? else {
        if sandbox.id().is_some() {
            return Err(refused(
                "Unavailable",
                format!(
                    "Sandbox {} has no recorded native runtime; its stop cannot be confirmed",
                    sandbox.name()
                ),
            ));
        }
        sandbox.free()?;
        return Ok(Native::NeverStarted);
    };
    Ok(
        match reopen::recorded(&host, sandbox.name(), &daemon)?.for_stop()? {
            Some(runtime) => Native::Stop(runtime),
            None => Native::Stopped,
        },
    )
}

/// These are lost/unknown uncommitted frontiers, never manufactured commits.
fn workspaces(home: &Path, sandbox: &SandboxRecord) -> Result<Vec<String>> {
    let mut workspaces = Vec::new();
    for (path, record) in files::workspaces(&sandbox.dir(home))? {
        let fallback = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let value = record.name().or_else(|| record.path()).unwrap_or(fallback);
        workspaces.push(format!("{}:{value}", sandbox.name()));
    }
    Ok(workspaces)
}

fn home_id(home: &Path) -> Result<String> {
    Ok(home::opened(home)?.id)
}

fn update(
    home: &Path,
    name: &str,
    change: impl FnOnce(MachineRecord) -> MachineRecord,
) -> Result<()> {
    change(MachineRecord::of(home, name)?).write(home)
}

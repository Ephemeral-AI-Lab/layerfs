//! What uses a machine or a Store from outside its own records, read from
//! the Sandbox and Store records, and the steps a machine call takes on
//! them: this crate's implementation of the machine crate's
//! [`MachineUsers`] and the Store crate's [`StoreUsers`]
//! (layerfs-project#104, #105). Callers pass [`MACHINE`] and [`STORE`]
//! into the machine and Store calls that check their users.
use crate::status::inventory;
use layerfs_home::state::State;
use layerfs_machine::{MachineUsers, Related};
use layerfs_protocol::sdk::SdkRefusal;
use layerfs_store::StoreUsers;
use std::path::Path;
#[cfg(feature = "machine")]
use {
    crate::{
        record::{discovery, reopen, SandboxRecord},
        route::direct,
    },
    layerfs_home::{
        home::{self, lock, paths},
        op::Op,
        outcome::{failed, refused, Outcome},
    },
    layerfs_machine::{
        force_record, machine_refusal as refusal, machine_spec, record::MachineRecord, Phases,
    },
    layerfs_protocol::sdk::SdkValue,
};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What a machine holds and what its calls do to it, from the Sandbox and
/// Store records.
pub const MACHINE: MachineUsers = MachineUsers {
    related: Related {
        stores_on: inventory::stores_on,
        sandboxes_on: inventory::sandboxes_on,
        hosted_on: inventory::hosted_on,
        can_remove: inventory::can_remove_machine,
        can_stop: inventory::can_stop_machine,
        can_change: inventory::can_change_machine,
    },
    reconcile,
    forget,
    host_holders,
    #[cfg(feature = "machine")]
    force_stop,
};

/// Who uses a Store or a project, and the machine reads the Store calls
/// take, from the Sandbox and machine records.
pub const STORE: StoreUsers = StoreUsers {
    related: layerfs_store::Related {
        store_idle: inventory::store_idle,
        store_idle_except: Some(|home, store, owner| {
            inventory::store_idle_with(home, store, Some(owner))
        }),
        can_remove_project: inventory::can_remove_project,
    },
    machine_state,
    sandboxes_on: inventory::sandboxes_on,
    store_idle_on: inventory::store_idle_on,
    nameless_fix,
};

/// A machine's records-only state as the Store reads take it
/// ([`layerfs_store::state::Machines`]): the machine family's one
/// derivation, [`inventory::machine_state`], so a Store's `blocked_by` says
/// what `machine get` says of its machine.
pub fn machine_state(home: &Path, name: &str) -> Result<State> {
    match layerfs_machine::record::MachineRecord::read(home, name) {
        Ok(record) => inventory::machine_state(home, name, record.as_ref()),
        // a record that cannot be read blocks the Stores on it, nothing else
        Err(_) => Ok(layerfs_machine::state::unreadable(name)),
    }
}

/// What the Store reads take from the other entities: each machine's
/// state ([`machine_state`]).
pub fn reads() -> layerfs_store::state::Machines<'static> {
    &machine_state
}

/// `refusal` with the fix of a machine that has no VM name, when it is one.
fn nameless_fix(home: &Path, machine: &str, refusal: SdkRefusal) -> SdkRefusal {
    #[cfg(feature = "machine")]
    {
        MACHINE.related.nameless_fix(home, machine, refusal)
    }
    #[cfg(not(feature = "machine"))]
    {
        let _ = (home, machine);
        refusal
    }
}

/// Settles the mount intents a previous worker incarnation left in each Store
/// on `machine`, with the incarnation this start recorded. Never a live or
/// uncertain owner: the Store-side reconcile applies the same rule admission
/// does. A Store the reconcile cannot reach is left for the next start.
fn reconcile(home: &Path, machine: &str, incarnation: &str) -> Result<()> {
    for store in inventory::stores_on(home, machine)? {
        let _ = layerfs_store::service::reconcile_machine(home, &store, machine, incarnation);
    }
    Ok(())
}

/// A removed machine's attachment to each PostgreSQL Store is forgotten with
/// it, so a later machine of its name is attached to none, and so are the
/// home's image copy locks of it (`run/image-lock/<machine>/`).
fn forget(home: &Path, name: &str) -> Result<()> {
    layerfs_store::postgresql::check::forget_machine(home, name).map_err(|refusal| {
        layerfs_home::outcome::refused(
            &refusal.code,
            format!(
                "machine {name} was removed, but its PostgreSQL Store attachments were not forgotten: {}",
                refusal.message
            ),
        )
    })?;
    // the home's image copy locks of the machine go with it
    let locks = crate::image::copy::lock::ImageLock::dir(home, name);
    match std::fs::remove_dir_all(&locks) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(layerfs_home::outcome::refused(
                "Failed",
                format!(
                    "machine {name} was removed, but its image copy locks in {} were not: {error}",
                    locks.display()
                ),
            ))
        }
        _ => Ok(()),
    }
}

/// What holds host machine `name` once it is stopped: its Sandboxes not
/// stopped, and each Store on it that a process holds. This process's own
/// idle copies close first, as a move's settle does (Store I/O, so before
/// any record lock); a Store that cannot be closed or read is a holder
/// naming why.
fn host_holders(home: &Path, name: &str) -> Result<Vec<String>> {
    let mut blocking = Vec::new();
    for sandbox in inventory::sandboxes(home)? {
        if sandbox.machine() == name && !sandbox.machine_stopped() {
            blocking.push(format!("Sandbox {}", sandbox.name()));
        }
    }
    let stores = inventory::stores_on(home, name)?;
    // this process's own idle copies close first, as a move's settle does
    // (Store I/O, so before any record lock): only other processes and a
    // call running here hold the machine; a copy in use stays a holder
    for store in &stores {
        let file = layerfs_home::home::paths::store_file(home, store);
        match layerfs_store::open::close(&file) {
            Err(error) if error.code != "InUse" => {
                blocking.push(format!("Store {store} (close failed: {})", error.message));
            }
            _ => {}
        }
    }
    for store in stores {
        if let Err(error) = store_holders(home, name, &store, &mut blocking) {
            blocking.push(format!(
                "Store {store} (holders not read: {})",
                error.message
            ));
        }
    }
    Ok(blocking)
}

/// The holders of host Store `store` on machine `name`: this process's
/// lease and every other live one.
fn store_holders(home: &Path, name: &str, store: &str, blocking: &mut Vec<String>) -> Result<()> {
    let _guard = layerfs_store::record::lock(home, store)?;
    let record = layerfs_store::record::StoreRecord::of(home, store)?;
    if !matches!(record.store_ref()?,layerfs_store::record::StoreRef::Sqlite{machine,..} if machine == name)
    {
        return Ok(());
    }
    if layerfs_store::lease::held_here(home, store) {
        blocking.push(format!("Store {store} (pid {})", std::process::id()));
    }
    for pid in layerfs_store::lease::holders(home, store)? {
        blocking.push(format!("Store {store} (pid {pid})"));
    }
    Ok(())
}

/// `machine stop --force` of machine `name`, its machine lock held: the
/// provider crash stop and this home's Sandboxes on it, each recorded.
#[cfg(feature = "machine")]
fn force_stop(home: &Path, name: &str, mut phases: Phases<'_, '_>) -> Outcome {
    let intent = Op::begin(
        "machine.stop",
        "stop",
        SdkValue::map([("force", SdkValue::Bool(true))]),
    )?;
    let (spec, hosted, sandboxes) = {
        let _home = lock::take(home)?;
        let machine = force_record(home, name)?;
        let hosted = if name == layerfs_home::names::HOST_MACHINE {
            Vec::new()
        } else {
            inventory::hosted_on(home, name)?
        };
        let sandboxes = inventory::sandboxes(home)?
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
        let spec = machine_spec(home, &machine)
            .map_err(|r| MACHINE.related.nameless_fix(home, name, r))?;
        for child in &hosted {
            force_record(home, child)?;
        }
        for machine in std::iter::once(name).chain(hosted.iter().map(String::as_str)) {
            update(home, machine, |record| record.with_op(&intent))?;
        }
        (spec, hosted, sandboxes)
    };
    phases.begin("stop");
    let mut unknown = Vec::new();
    let mut stopped = Vec::new();
    let mut skipped = Vec::new();
    let mut native_attempt = None;
    let attempted = (|| -> Result<()> {
        // no provider holds the machine (`native`): no VM to stop, so each
        // Sandbox's runtime on the host's own OS is stopped from here
        if !spec.kind.provided() {
            let mut first_refusal = None;
            for sandbox in &sandboxes {
                // A refusal before the signal leaves this runtime as it was,
                // so the stop goes on to the next Sandbox: only a sent stop
                // makes an outcome uncertain.
                let runtime = match native_runtime(home, sandbox) {
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
                        // its running commands are each marked and cancelled
                        // first (D-10), as a forced close does, so they answer
                        // `cancelled`, not the exit of the runtime's kill
                        if let Err(refusal) = settle(home, sandbox) {
                            skipped.push(sandbox.name().to_owned());
                            first_refusal.get_or_insert(refusal);
                            continue;
                        }
                        native_attempt = Some(sandbox);
                        runtime
                            .stop(0)
                            .map_err(crate::route::daemon::refusal::wire)?;
                    }
                }
                mark(home, sandbox, true)?;
                native_attempt = None;
                stopped.push(sandbox.name().to_owned());
                unknown.extend(workspaces(home, sandbox)?);
            }
            if let Some(refusal) = first_refusal {
                return Err(refusal);
            }
        } else {
            layerfs_machine::platform::stop_force(&spec).map_err(refusal)?;
            for sandbox in &sandboxes {
                mark(home, sandbox, true)?;
                stopped.push(sandbox.name().to_owned());
                let observed = workspaces(home, sandbox)?;
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
        let uncertain = match spec.kind.provided() {
            false => native_attempt.is_some(),
            true => error.code != "Unsupported" || !stopped.is_empty(),
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
            let _home = lock::take(home)?;
            for machine in std::iter::once(name).chain(hosted.iter().map(String::as_str)) {
                update(home, machine, |record| {
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
        if spec.kind.provided() && error.code != "Unsupported" {
            for sandbox in &sandboxes {
                if !stopped.iter().any(|name| name == sandbox.name()) {
                    keep(mark(home, sandbox, false));
                }
            }
        } else if let Some(sandbox) = native_attempt {
            if !stopped.iter().any(|name| name == sandbox.name()) {
                keep(mark(home, sandbox, false));
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
        let _home = lock::take(home)?;
        for machine in std::iter::once(name).chain(hosted.iter().map(String::as_str)) {
            match std::fs::remove_file(paths::worker_record(home, machine)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(failed(error)),
            }
            update(home, machine, |record| {
                // The recorded stop applies to local and provider machines.
                let state = "stopped";
                // a worker that was outdated or did not answer is gone with it
                (record.without_op().with("state", SdkValue::text(state))).mended()
            })?;
        }
    }
    // no probe: it could contact the worker just stopped
    phases.end();
    let done = layerfs_machine::done(name, Some("stopped"));
    Ok(layerfs_machine::with_stopped(
        done, true, &stopped, &unknown,
    ))
}

#[cfg(feature = "machine")]
/// Record an observed terminal fact, retaining runtime files and exact identity.
fn mark(home: &Path, sandbox: &SandboxRecord, stopped: bool) -> Result<()> {
    let _home = lock::take(home)?;
    let _sandbox = crate::record::lock(home, sandbox.name())?;
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

#[cfg(feature = "machine")]
/// What a forced stop finds for one Sandbox on a machine no provider holds
/// (`native`), whose Sandboxes run on the host's own OS: only this home's
/// recorded runtime processes stop.
enum Native {
    /// Its recorded runtime, reopened for the stop; nothing sent yet.
    Stop(Box<crate::Sandbox>),
    /// Its runtime is already stopped.
    Stopped,
    /// It never started (no id and no runtime record).
    NeverStarted,
}

#[cfg(feature = "machine")]
/// The Sandbox's recorded native runtime, reopened for a stop without sending
/// anything, or why there is nothing to stop.
fn native_runtime(home: &Path, sandbox: &SandboxRecord) -> Result<Native> {
    if sandbox.machine_stopped() {
        return Ok(Native::Stopped);
    }
    let host = crate::lifecycle::force_remove::local_host(home, sandbox.name())?;
    let Some(daemon) = discovery::started(&sandbox.dir(home))? else {
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

#[cfg(feature = "machine")]
/// Cancels each running command of native Sandbox `sandbox` before its
/// runtime stops (`exec::cancel::settle` with `force`); stale records are
/// ended and logged. A refusal still precedes the stop's signal.
fn settle(home: &Path, sandbox: &SandboxRecord) -> Result<()> {
    let unreached = &mut crate::exec::settle::unreached;
    crate::exec::settle::settle(&sandbox.dir(home), sandbox.name(), None, true, unreached)
}

#[cfg(feature = "machine")]
/// These are lost/unknown uncommitted frontiers, never manufactured commits.
fn workspaces(home: &Path, sandbox: &SandboxRecord) -> Result<Vec<String>> {
    let mut workspaces = Vec::new();
    for (path, record) in discovery::workspaces(&sandbox.dir(home))? {
        let fallback = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let value = record.name().or_else(|| record.path()).unwrap_or(fallback);
        workspaces.push(format!("{}:{value}", sandbox.name()));
    }
    Ok(workspaces)
}

#[cfg(feature = "machine")]
fn update(
    home: &Path,
    name: &str,
    change: impl FnOnce(MachineRecord) -> MachineRecord,
) -> Result<()> {
    change(MachineRecord::of(home, name)?).write(home)
}

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
    force_stop: crate::lifecycle::machine_stop::force_stop,
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

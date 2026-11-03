//! The checks on what the scan read: broken references (a Sandbox whose
//! Store, project or machine is gone, a Store on a machine with no record,
//! a Firecracker machine whose host is gone), dead and uncertain `op`
//! intents, stale Store leases (read, never removed) and a partial Store
//! copy (`store.sqlite.incoming`) an interrupted transfer left on `native`.
use super::report::Findings;
use super::scan::Scanned;
use crate::common::home::{lease, paths};
use crate::common::op::{Op, Standing};
use crate::store::record::{Placement, StoreRef};
use layerfs_bridge::control::SdkRefusal;
use std::path::Path;

/// `native` and `docker` exist with or without a record.
fn built_in(machine: &str) -> bool {
    matches!(machine, "native" | "docker")
}

/// Runs every check on `scanned`.
pub(super) fn check(
    home: &Path,
    scanned: &Scanned,
    findings: &mut Findings,
) -> Result<(), SdkRefusal> {
    machines(scanned, findings);
    for store in &scanned.stores {
        let name = store.name();
        let entity = ("store", name);
        let op = store.op();
        if let Some(op) = &op {
            let (show, clear) = (
                format!("store status {name}"),
                format!("store remove {name}"),
            );
            intent(entity, op, &show, &clear, findings);
        }
        match store.store_ref() {
            Err(refusal) => findings.error(
                entity,
                format!("Store {name}'s record cannot be read: {}", refusal.message),
                format!(
                    "no call reads it: repair {} by hand",
                    paths::store_record(home, name).display()
                ),
            ),
            Ok(StoreRef::Sqlite {
                machine: Placement::Machine(machine),
                ..
            }) => {
                if !built_in(machine) && !scanned.machine_names.contains(machine) {
                    findings.error(
                        entity,
                        format!("Store {name} is on machine {machine}, which has no record"),
                        format!("no call reaches it: make machine {machine} again (`layerfs machine create {machine} …` or `layerfs machine connect {machine} …`), or delete {} by hand", paths::store_dir(home, name).display()),
                    );
                }
                // a transfer running now owns its partial copy
                if op.is_none_or(|op| op.standing() != Standing::Running) {
                    incoming(home, name, machine == "native", findings);
                }
            }
            Ok(StoreRef::Sqlite {
                machine: Placement::Unknown,
                ..
            }) => findings.warning(
                entity,
                format!("Store {name} came from a 1.1.0 home and its machine is not known"),
                "`layerfs import-env` records where its file is".to_owned(),
            ),
            Ok(StoreRef::Postgres { .. }) => {}
        }
        let (_, stale) = lease::read_holders(home, name)?;
        for pid in stale {
            findings.warning(
                entity,
                format!("Store {name} holds a stale lease of process {pid}, which ended"),
                format!(
                    "nothing reads it; delete `store/{name}/open/{pid}` by hand, or the next \
                     `layerfs store push`, `store pull` or `store remove` of {name} removes it"
                ),
            );
        }
    }
    sandboxes(scanned, findings);
    Ok(())
}

/// Machines: their intents, and a Firecracker machine's Lima host.
fn machines(scanned: &Scanned, findings: &mut Findings) {
    for machine in &scanned.machines {
        let name = machine.name();
        let entity = ("machine", name);
        if let Some(op) = machine.op() {
            let clear = format!("{} {name}", machine.clearing_call());
            intent(
                entity,
                &op,
                &format!("machine status {name}"),
                &clear,
                findings,
            );
        }
        if let Some(host) = machine.host() {
            if !scanned.machine_names.contains(host) {
                findings.error(
                    entity,
                    format!("machine {name}'s Lima host {host} has no record"),
                    format!("`layerfs machine remove {name}`"),
                );
            }
        }
    }
}

/// Sandboxes: their intents, and the Store, project and machine they name.
fn sandboxes(scanned: &Scanned, findings: &mut Findings) {
    for sandbox in &scanned.sandboxes {
        let name = sandbox.name();
        let entity = ("sandbox", name);
        let close = format!("sandbox close {name}");
        if let Some(op) = sandbox.op() {
            intent(
                entity,
                &op,
                &format!("sandbox get {name}"),
                &close,
                findings,
            );
        }
        let fix = format!("`layerfs {close}`");
        match sandbox.project().and_then(|p| p.split_once('/')) {
            None => findings.error(
                entity,
                format!("Sandbox {name}'s record names no project"),
                fix.clone(),
            ),
            Some((store, _)) if !scanned.store_names.contains(store) => findings.error(
                entity,
                format!("Sandbox {name}'s Store {store} is gone"),
                fix.clone(),
            ),
            Some((store, project))
                if !(scanned.projects).contains(&(store.to_owned(), project.to_owned())) =>
            {
                findings.error(
                    entity,
                    format!("Sandbox {name}'s project {store}/{project} is gone"),
                    fix.clone(),
                )
            }
            Some(_) => {}
        }
        let machine = sandbox.machine();
        if !machine.is_empty() && !built_in(machine) && !scanned.machine_names.contains(machine) {
            findings.error(
                entity,
                format!("Sandbox {name} is on machine {machine}, which has no record"),
                fix,
            );
        }
    }
}

/// An intent whose owner is gone: uncertain is an error naming the call
/// that shows it and the one that clears it; certain holds nothing.
fn intent(entity: (&str, &str), op: &Op, show: &str, clear: &str, findings: &mut Findings) {
    let (kind, name) = entity;
    let what = format!("{} ({}, phase {})", op.name(), op.id(), op.phase());
    match op.standing() {
        Standing::Running => {}
        Standing::Unresolved => findings.error(
            entity,
            format!("{kind} {name} has an unresolved {what}: its outcome is not known"),
            format!("`layerfs {show}` shows it; `layerfs {clear}` clears it"),
        ),
        Standing::Stale => findings.warning(
            entity,
            format!("{kind} {name} still records {what}, whose process ended after a certain outcome; it holds nothing"),
            format!("nothing to do: the next operation on {kind} {name} replaces it"),
        ),
    }
}

/// A partial copy an interrupted pull to `native` left
/// (`store.sqlite.incoming`). Other machines' copies are not read: no
/// worker operation reports them.
fn incoming(home: &Path, store: &str, on_native: bool, findings: &mut Findings) {
    let file = paths::store_file(home, store);
    let incoming = file.with_file_name("store.sqlite.incoming");
    if !incoming.exists() {
        return;
    }
    let fix = if on_native {
        format!("delete {} by hand", incoming.display())
    } else {
        format!(
            "`layerfs store pull {store}` replaces it, or delete {} by hand",
            incoming.display()
        )
    };
    findings.warning(
        ("store", store),
        format!(
            "{} is a partial copy an interrupted transfer left; nothing reads it",
            incoming.display()
        ),
        fix,
    );
}

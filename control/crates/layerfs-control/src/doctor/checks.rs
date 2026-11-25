//! The checks on what the scan read: broken references (a Sandbox whose
//! Store, project or machine is gone, a Store on a machine with no record,
//! a built-in machine's recorded daemon that is not there), dead and uncertain `op`
//! intents, stale Store leases (read, never removed), a partial Store
//! copy (`store.sqlite.incoming`) an interrupted transfer left on `native`,
//! and a copy a move left on the machine its record does not name (from
//! the record: no machine is asked).
//!
//! A finding's `fix` is one runnable line in 2.0's call names, or empty
//! when none repairs it (its message then says what to do). An intent a
//! gone owner left after a certain outcome holds nothing and is no
//! finding: every read ignores it, and the next operation replaces it.
use super::records::Findings;
use super::scan::{shell, Scanned};
use crate::home::Build;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::shell_word;
use layerfs_store::lease;
use layerfs_store::record::{StoreRecord, StoreRef};
use std::path::Path;

/// The built-in machines exist with or without a record.
fn built_in(machine: &str) -> bool {
    layerfs_home::names::BUILT_IN.contains(&machine)
}

/// The line that records this build's daemon for machine `machine`: only
/// `native` runs this host's daemon, and only a daemon stamped with
/// `build`'s commit is named (`None`: no daemon of this build is known).
pub(super) fn daemon_fix(machine: &str, build: Build<'_>) -> Option<String> {
    let own = (machine == "native")
        .then(|| crate::home::own_daemon(build))
        .flatten()?;
    Some(format!(
        "layerfs machine update {} --daemon {}",
        shell_word(machine),
        shell(&own)
    ))
}

/// The record checks of the home's records (`home.records`): they read
/// files of this computer only and connect to no machine.
pub(super) fn check_local(
    home: &Path,
    scanned: &Scanned,
    build: Build<'_>,
    findings: &mut Findings,
) -> Result<(), SdkRefusal> {
    machines(scanned, build, findings);
    for store in &scanned.stores {
        let name = store.name();
        let entity = ("store", name);
        let op = store.op();
        if let Some(op) = &op {
            let clear = layerfs_store::intent::clearing(op, name);
            intent(entity, op, &format!("layerfs {clear}"), findings);
        }
        match store.store_ref() {
            Err(refusal) => {
                let record = paths::store_record(home, name);
                // no fix: `store remove` needs a readable record and deletes
                // the Store's data
                findings.error(
                    entity,
                    format!(
                        "Store {name}'s record cannot be read: {}; no call reads it: repair {} by hand",
                        refusal.message,
                        record.display()
                    ),
                    String::new(),
                );
                findings.evidence(SdkValue::map([
                    ("store", SdkValue::text(name)),
                    ("path", SdkValue::text(record.display().to_string())),
                    ("code", SdkValue::text(&refusal.code)),
                ]));
            }
            Ok(StoreRef::Sqlite { machine, .. }) => {
                if !built_in(machine) && !scanned.machine_names.contains(machine) {
                    findings.error(
                        entity,
                        format!("Store {name} is on machine {machine}, which has no record; no call reaches it: restore machine {machine}'s record for inspection and explicit cleanup, or delete {} by hand", paths::store_dir(home, name).display()),
                        String::new(),
                    );
                    // no fix: restoring the record needs the machine's
                    // platform, and deleting the folder loses the Store
                    findings.evidence(SdkValue::map([
                        ("store", SdkValue::text(name)),
                        ("machine", SdkValue::text(machine)),
                        ("machine_record", SdkValue::text("absent")),
                    ]));
                }
                // a transfer running now owns its partial copy
                let standing = op.as_ref().map(Op::standing);
                if standing != Some(Standing::Running) {
                    incoming(home, store, findings);
                }
                // an unresolved intent's own finding names its clearing call
                if standing != Some(Standing::Unresolved) {
                    leftover(store, machine, findings);
                }
            }
            Ok(StoreRef::Postgres { .. }) => {}
        }
        let (_, stale) = lease::read_holders(home, name)?;
        for pid in stale {
            let lease = paths::lease_dir(home, name).join(pid.to_string());
            findings.warning(
                entity,
                format!(
                    "Store {name} holds a stale lease of process {pid}, which ended; nothing reads it, and the next call on {name} removes it"
                ),
                format!("rm {}", shell(&lease)),
            );
            findings.evidence(SdkValue::map([
                ("store", SdkValue::text(name)),
                ("pid", SdkValue::Int(i64::from(pid))),
                ("path", SdkValue::text(lease.display().to_string())),
            ]));
        }
    }
    sandboxes(scanned, findings);
    Ok(())
}

/// Machines: their intents, and a built-in machine's recorded daemon that
/// is not there.
fn machines(scanned: &Scanned, build: Build<'_>, findings: &mut Findings) {
    for machine in &scanned.machines {
        let name = machine.name();
        let entity = ("machine", name);
        // its message names the call that records another
        if let Some(daemon) = machine.daemon().filter(|d| !Path::new(d).is_file()) {
            findings.error(
                entity,
                layerfs_sandbox::deploy::missing_daemon(name, daemon),
                daemon_fix(name, build).unwrap_or_default(),
            );
            findings.evidence(SdkValue::map([
                ("machine", SdkValue::text(name)),
                ("daemon", SdkValue::text(daemon)),
                ("present", SdkValue::Bool(false)),
            ]));
        }
        if let Some(op) = machine.op() {
            let clear = layerfs_machine::state::settle(name, &op);
            intent(entity, &op, &clear, findings);
        }
    }
}

/// Sandboxes: their intents, and the Store, project and machine they name.
fn sandboxes(scanned: &Scanned, findings: &mut Findings) {
    for sandbox in &scanned.sandboxes {
        let name = sandbox.name();
        let entity = ("sandbox", name);
        let fix = layerfs_sandbox::state::remove_fix(name);
        if let Some(op) = sandbox.op() {
            intent(entity, &op, &fix, findings);
        }
        // a reference that is gone: the Sandbox cannot be used, and its
        // removal is the one call left; an error, as its own doctor's
        // reference checks say (conformance R4)
        match sandbox.project().and_then(|p| p.split_once('/')) {
            None => {
                findings.error(
                    entity,
                    "its record names no project".to_owned(),
                    fix.clone(),
                );
                reference(
                    findings,
                    name,
                    "project",
                    "a project of the form store/project",
                );
            }
            Some((store, _)) if !scanned.store_names.contains(store) => {
                findings.error(
                    entity,
                    format!("its record names Store {store}, which has no record"),
                    fix.clone(),
                );
                reference(findings, name, "store", store);
            }
            Some((store, project))
                if !(scanned.projects).contains(&(store.to_owned(), project.to_owned())) =>
            {
                findings.error(
                    entity,
                    format!("its record names project {store}/{project}, which has no record"),
                    fix.clone(),
                );
                reference(findings, name, "project", &format!("{store}/{project}"));
            }
            Some(_) => {}
        }
        let machine = sandbox.machine();
        if !machine.is_empty() && !built_in(machine) && !scanned.machine_names.contains(machine) {
            findings.error(
                entity,
                format!("its record names machine {machine}, which has no record"),
                fix,
            );
            reference(findings, name, "machine", machine);
        }
    }
}

/// The evidence of a Sandbox's reference that has no record: which kind of
/// thing, and which one its record names.
fn reference(findings: &mut Findings, sandbox: &str, reference: &str, expected: &str) {
    findings.evidence(SdkValue::map([
        ("sandbox", SdkValue::text(sandbox)),
        ("reference", SdkValue::text(reference)),
        ("expected", SdkValue::text(expected)),
    ]));
}

/// An intent whose owner is gone with its outcome not known: an error
/// whose fix is `clear`, the call that clears it. A live owner's intent and
/// a certain one hold nothing to report.
fn intent(entity: (&str, &str), op: &Op, clear: &str, findings: &mut Findings) {
    let (kind, name) = entity;
    let what = format!("{} ({}, phase {})", op.name(), op.id(), op.phase());
    if op.standing() == Standing::Unresolved {
        findings.error(
            entity,
            format!("{kind} {name} has an unresolved {what}: its outcome is not known"),
            clear.to_owned(),
        );
        findings.evidence(SdkValue::map([
            ("op", SdkValue::text(op.name())),
            ("phase", SdkValue::text(op.phase())),
            ("op_id", SdkValue::text(op.id())),
            ("standing", SdkValue::text("unresolved")),
        ]));
    }
}

/// A copy an ended move left on the machine its record does not name: the
/// source's whole copy a move did not retire, or what an interrupted or
/// certain-failed move from `native` put on its target (whole or partial).
/// The next `store move` deletes it first. Read from the record: the
/// machine that holds the copy is not asked, so this is the one finding of
/// a target's partial copy.
fn leftover(store: &StoreRecord, machine: &str, findings: &mut Findings) {
    let Some(left) = layerfs_store::sqlite::transfer::resolve::leftover(store) else {
        return;
    };
    let name = store.name();
    let what = match left.on == left.to {
        true => "may have left a whole or partial copy on",
        false => "did not retire the copy on",
    };
    findings.warning(
        ("store", name),
        format!(
            "Store {name}'s {} from {} to {} {what} {}; the record names {machine}, nothing reads that copy, and the next `layerfs store move {name}` deletes it first",
            left.call, left.from, left.to, left.on
        ),
        String::new(),
    );
    // no fix: only the next move of this Store deletes the copy
    findings.evidence(SdkValue::map([
        ("store", SdkValue::text(name)),
        ("call", SdkValue::text(left.call)),
        ("from", SdkValue::text(&left.from)),
        ("to", SdkValue::text(&left.to)),
        ("on", SdkValue::text(&left.on)),
    ]));
}

/// A partial copy an interrupted transfer left (`store.sqlite.incoming`) on
/// `native`, beside the Store's file. One on another machine is
/// [`leftover`]'s, from the record.
fn incoming(home: &Path, store: &StoreRecord, findings: &mut Findings) {
    let name = store.name();
    let file = paths::store_file(home, name);
    let local_incoming = file.with_file_name("store.sqlite.incoming");
    if local_incoming.exists() {
        findings.warning(
            ("store", name),
            format!(
                "{} is a partial copy an interrupted transfer left; nothing reads it",
                local_incoming.display()
            ),
            format!("rm {}", shell(&local_incoming)),
        );
        findings.evidence(SdkValue::map([
            ("store", SdkValue::text(name)),
            ("path", SdkValue::text(local_incoming.display().to_string())),
        ]));
    }
}

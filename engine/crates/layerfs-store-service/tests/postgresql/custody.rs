//! Independent #768 tester (not the author's): advisory-key and intent-row
//! custody of a Workspace mount and unmount through the Store service on
//! PostgreSQL, inspected from a separate plain `postgres` admin connection
//! after every outcome: success, each definite refusal (missing Branch, a
//! Branch removed between the snapshot and the admission, a project key or
//! the Store key held exclusively, a conflicting live removal row, a stale
//! generation, a lock timeout), and the marker order of a path removal
//! racing a mount (paused through the fault proxy before and after the
//! admission message).
//!
//! Invariants: after a failed mount no backend but the blockers holds any
//! of the mount's three keys and no live mount row of the Branch exists; an
//! admitted mount holds all three shared and has its row; after a refusal
//! the same service mounts again once the obstacle is gone.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "mount_tester.rs"]
#[allow(dead_code)]
mod tester768;

use adversary::{tree_of, wait_until, Node, Service, TestStore};
use fault_proxy::{Fault, FaultProxy};
use layerfs_history::{BranchId, HistoryError};
use layerfs_persistence::{CompactRequest, Handles};
use layerfs_protocol::sdk::SdkValue;
use std::time::{Duration, Instant};
use tester768::*;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n"), ("/victim", b"victim\n")];

fn connect(store: &TestStore) -> postgres::Client {
    store.admin()
}

/// No backend outside `exclude` holds a key of a mount of `branch`, and no
/// live mount row of it exists.
fn assert_clean(store: &TestStore, branch: BranchId, exclude: &[i32], label: &str) {
    let mut admin = connect(store);
    let me = pid_of(&mut admin);
    let mut skip = exclude.to_vec();
    skip.push(me);
    let keys = mount_keys(store, branch);
    let held = key_holders(&mut admin, &keys, &skip);
    assert!(held.is_empty(), "{label}: keys still held: {held:?}");
    assert_eq!(live_mounts(&mut admin, branch), 0, "{label}: a live row");
}

/// One admitted mount: its three keys shared on one backend, one row.
fn assert_mounted(store: &TestStore, branch: BranchId, rows: i64, label: &str) -> i32 {
    let mut admin = connect(store);
    let me = pid_of(&mut admin);
    let keys = mount_keys(store, branch);
    let held = key_holders(&mut admin, &keys, &[me]);
    assert_eq!(held.len(), 3, "{label}: three keys: {held:?}");
    assert!(
        held.iter().all(|(_, _, mode)| mode == "ShareLock"),
        "{label}: shared: {held:?}"
    );
    let pid = held[0].1;
    assert!(
        held.iter().all(|(_, p, _)| *p == pid),
        "{label}: one backend"
    );
    assert_eq!(live_mounts(&mut admin, branch), rows, "{label}: rows");
    pid
}

fn warm(service: &Service, branch: BranchId) {
    if let Some(prefilled) = service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    let workspace = service.bind(branch);
    service.unmount(workspace);
}

fn compact(store: &TestStore, op: u8) -> String {
    let request = CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    };
    let Handles::Postgres(handles) = store.handles() else {
        unreachable!()
    };
    let result = handles.compact(&request);
    handles.close();
    match result {
        Ok(_) => "Ok".into(),
        Err(HistoryError::MountsActive(_)) => "MountsActive".into(),
        Err(HistoryError::Busy | HistoryError::BusyOp(_)) => "Busy".into(),
        Err(other) => format!("{other:?}"),
    }
}

fn code(answer: &Result<SdkValue, layerfs_protocol::sdk::SdkRefusal>) -> String {
    match answer {
        Ok(_) => "Ok".into(),
        Err(refusal) => refusal.code.clone(),
    }
}

#[test]
fn success_and_unmount_leave_exact_custody() {
    let Some(store) = TestStore::postgres("t768-ok", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let service = store.open();
    let first = service.bind(branch);
    let writer = assert_mounted(&store, branch, 1, "one mount");
    let second = service.bind(branch);
    assert_eq!(assert_mounted(&store, branch, 2, "two mounts"), writer);
    service.unmount(first);
    assert_mounted(&store, branch, 1, "one unmounted, one left");
    service.unmount(second);
    assert_clean(&store, branch, &[], "both unmounted");
    service.stop();
    assert!(store.no_open_transaction());
}

#[test]
fn a_missing_branch_takes_no_key() {
    let Some(store) = TestStore::postgres("t768-missing", SEED) else {
        return;
    };
    let service = store.open();
    warm(&service, store.fork("work"));
    let never = store.branch("never");
    let error = service.try_bind(never).err().expect("refused");
    println!("T768_MISSING {error}");
    assert!(error.contains("MissingBranch"), "{error}");
    assert_clean(&store, never, &[], "missing Branch");
    let workspace = service.bind(store.branch("work"));
    service.unmount(workspace);
    service.stop();
}

#[test]
fn a_stale_generation_refuses_the_mount_and_keeps_unmount_custody() {
    let Some(store) = TestStore::postgres("t768-stale", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let mut admin = connect(&store);
    // Mount refused at a moved generation: no key, no row.
    let service = store.open();
    warm(&service, branch);
    bump_generation(&mut admin);
    let error = service.try_bind(branch).err().expect("refused");
    println!("T768_STALE_MOUNT {error}");
    assert!(error.contains("Stale"), "{error}");
    assert_clean(&store, branch, &[], "stale mount");
    let again = service.try_bind(branch);
    println!("T768_STALE_MOUNT_AGAIN {:?}", again.as_ref().map(|_| ()));
    assert!(again.is_err(), "a stale service stays stale");
    assert_clean(&store, branch, &[], "stale mount again");
    service.abandon();
    // A fresh service mounts at the new generation.
    let fresh = store.open();
    let workspace = fresh.bind(branch);
    assert_mounted(&store, branch, 1, "fresh");
    // The unmount at a moved generation: refused, the row and the keys kept.
    bump_generation(&mut admin);
    let released = fresh.try_unmount(workspace);
    println!("T768_STALE_UNMOUNT {released:?}");
    let error = released.unwrap_err();
    assert!(error.contains("Stale"), "{error}");
    let writer = assert_mounted(&store, branch, 1, "stale unmount keeps custody");
    // Another fresh service still mounts beside it (shared keys).
    let other = store.open();
    let workspace = other.bind(branch);
    other.unmount(workspace);
    other.stop();
    // Closing the stale service's connection gives its keys back; its row
    // stays for a later settlement.
    fresh.abandon();
    assert!(
        wait_until(Duration::from_secs(10), || !alive(&mut admin, writer)),
        "the writer's backend ends"
    );
    let keys = mount_keys(&store, branch);
    let me = pid_of(&mut admin);
    assert!(key_holders(&mut admin, &keys, &[me]).is_empty());
    let left = live_mounts(&mut admin, branch);
    println!(
        "T768_STALE_ORPHAN {{\"live_rows\":{left},\"rows\":{:?}}}",
        live_rows(&mut admin)
    );
    // What a compaction makes of the orphaned row, reported.
    let compaction = compact(&store, 0x51);
    println!("T768_STALE_ORPHAN_COMPACTION {compaction}");
    let after = live_mounts(&mut admin, branch);
    println!("T768_STALE_ORPHAN_AFTER {{\"live_rows\":{after}}}");
}

/// A blocker connection holding `key` (`exclusive` or shared) refuses the
/// mount; afterwards nothing leaked and the same service mounts.
fn blocked_by_key(tag: &str, key_of: fn(&TestStore, BranchId) -> i64, exclusive: bool) {
    let Some(store) = TestStore::postgres(tag, SEED) else {
        return;
    };
    let branch = store.fork("work");
    let service = store.open();
    warm(&service, branch);
    let mut blocker = connect(&store);
    let blocker_pid = pid_of(&mut blocker);
    let key = key_of(&store, branch);
    let lock = if exclusive {
        "SELECT pg_advisory_lock($1)"
    } else {
        "SELECT pg_advisory_lock_shared($1)"
    };
    blocker.execute(lock, &[&key]).unwrap();
    let started = Instant::now();
    let answer = service.try_bind(branch);
    let took = started.elapsed();
    println!(
        "T768_BLOCKED {{\"tag\":\"{tag}\",\"answer\":{:?},\"ms\":{}}}",
        answer.as_ref().map(|_| ()),
        took.as_millis()
    );
    let error = answer.err().expect("refused while a blocker holds the key");
    assert!(
        error.contains("Busy") || error.contains("MountsActive"),
        "{tag}: {error}"
    );
    assert_clean(&store, branch, &[blocker_pid], tag);
    blocker
        .execute("SELECT pg_advisory_unlock_all()", &[])
        .unwrap();
    let workspace = service.bind(branch);
    assert_mounted(&store, branch, 1, tag);
    service.unmount(workspace);
    assert_clean(&store, branch, &[], tag);
    service.stop();
}

#[test]
fn the_store_key_held_exclusively_refuses_the_mount_without_a_leak() {
    blocked_by_key("t768-storekey", |store, _| store_key(store), true);
}

#[test]
fn the_project_key_held_exclusively_refuses_the_mount_without_a_leak() {
    blocked_by_key(
        "t768-projkey",
        |store, branch| mount_keys(store, branch)[1],
        true,
    );
}

#[test]
fn the_branch_key_held_exclusively_refuses_the_mount_without_a_leak() {
    blocked_by_key(
        "t768-brkey",
        |store, branch| mount_keys(store, branch)[2],
        true,
    );
}

/// A compaction intent row live beside its exclusive Store key, as a running
/// compaction in another process leaves them.
#[test]
fn a_running_compaction_refuses_the_mount_without_a_leak() {
    let Some(store) = TestStore::postgres("t768-compacting", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let service = store.open();
    warm(&service, branch);
    let mut blocker = connect(&store);
    let blocker_pid = pid_of(&mut blocker);
    let catalog = layerfs_history::CatalogId::derive(&store.binding).unwrap();
    blocker
        .execute("SELECT pg_advisory_lock($1)", &[&store_key(&store)])
        .unwrap();
    blocker
        .execute(
            "INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, owner_runtime, owner_boot, \
             state, uncertain, phase, updated_ms) VALUES ($1, 'compaction', 'store', $2, \
             'runtime-other', 'boot-other', 'live', false, 'mark', 1)",
            &[&[0xC1u8; 16].as_slice(), &catalog.as_slice()],
        )
        .unwrap();
    let error = service.try_bind(branch).err().expect("refused");
    println!("T768_COMPACTING {error}");
    assert!(
        error.contains("Busy") || error.contains("MountsActive") || error.contains("Unresolved"),
        "{error}"
    );
    assert_clean(&store, branch, &[blocker_pid], "compacting");
    blocker
        .execute(
            "DELETE FROM gc_intent WHERE op_id = $1",
            &[&[0xC1u8; 16].as_slice()],
        )
        .unwrap();
    blocker
        .execute("SELECT pg_advisory_unlock_all()", &[])
        .unwrap();
    let workspace = service.bind(branch);
    service.unmount(workspace);
    assert_clean(&store, branch, &[], "after the compaction");
    service.stop();
}

/// A live project removal row whose owner cannot be proven dead, and no key
/// held for it: the message refuses (a live conflicting row) and the
/// fallback reports what it finds; either way nothing leaks.
#[test]
fn a_live_removal_row_without_its_key_never_leaks() {
    let Some(store) = TestStore::postgres("t768-removalrow", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let service = store.open();
    warm(&service, branch);
    let mut admin = connect(&store);
    admin
        .execute(
            "INSERT INTO gc_intent (op_id, kind, scope_kind, scope_id, project, owner_runtime, \
             owner_boot, state, uncertain, phase, updated_ms) VALUES ($1, 'removal', 'project', \
             $2, 'adv', 'runtime-other', 'boot-other', 'live', false, 'mark', 1)",
            &[&[0xD1u8; 16].as_slice(), &store.stack.as_slice()],
        )
        .unwrap();
    let answer = service.try_bind(branch);
    println!(
        "T768_REMOVAL_ROW {:?}",
        answer.as_ref().map(|_| ()).map_err(|e| e.clone())
    );
    match answer {
        Ok(workspace) => {
            assert_mounted(&store, branch, 1, "admitted beside a removal row");
            service.unmount(workspace);
        }
        Err(_) => assert_clean(&store, branch, &[], "refused by a removal row"),
    }
    admin
        .execute(
            "DELETE FROM gc_intent WHERE op_id = $1",
            &[&[0xD1u8; 16].as_slice()],
        )
        .unwrap();
    let workspace = service.bind(branch);
    service.unmount(workspace);
    assert_clean(&store, branch, &[], "after the removal row");
    service.stop();
}

/// The intent table locked by another transaction past the mount's lock
/// timeout: a definite refusal, no key or row left, and the same service
/// mounts once the lock is gone.
#[test]
fn a_lock_timeout_on_the_mount_gives_every_key_back() {
    let Some(store) = TestStore::postgres("t768-locktimeout", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let selection = store.with_lock_timeout(&store.selection, 300);
    let service = Service::open(&store, selection);
    warm(&service, branch);
    let mut blocker = connect(&store);
    let blocker_pid = pid_of(&mut blocker);
    blocker
        .batch_execute("BEGIN; LOCK TABLE gc_intent IN ACCESS EXCLUSIVE MODE")
        .unwrap();
    let started = Instant::now();
    let answer = service.try_bind(branch);
    let took = started.elapsed();
    println!(
        "T768_LOCK_TIMEOUT_MOUNT {{\"answer\":{:?},\"ms\":{}}}",
        answer.as_ref().map(|_| ()),
        took.as_millis()
    );
    // pg_locks only: the intent table is locked.
    let mut admin = connect(&store);
    let me = pid_of(&mut admin);
    let keys = mount_keys(&store, branch);
    let held = key_holders(&mut admin, &keys, &[me, blocker_pid]);
    blocker.batch_execute("COMMIT").unwrap();
    let error = answer.err().expect("refused at the lock timeout");
    assert!(error.contains("Busy"), "{error}");
    assert!(held.is_empty(), "keys after the timeout: {held:?}");
    assert_eq!(live_mounts(&mut admin, branch), 0);
    let workspace = service.bind(branch);
    service.unmount(workspace);
    assert_clean(&store, branch, &[], "after the lock");
    service.stop();
    assert!(store.no_open_transaction());
}

/// The unmount while the intent table is locked by another transaction for
/// two seconds: reported (time and answer), and custody is consistent with
/// the answer.
#[test]
fn an_unmount_meeting_a_table_lock() {
    let Some(store) = TestStore::postgres("t768-unmountlock", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let selection = store.with_lock_timeout(&store.selection, 300);
    let service = Service::open(&store, selection);
    warm(&service, branch);
    let workspace = service.bind(branch);
    let writer = assert_mounted(&store, branch, 1, "mounted");
    let mut blocker = connect(&store);
    blocker
        .batch_execute("BEGIN; LOCK TABLE gc_intent IN ACCESS EXCLUSIVE MODE")
        .unwrap();
    let (released, took) = std::thread::scope(|scope| {
        let unmounting = scope.spawn(|| {
            let started = Instant::now();
            let released = service.try_unmount(workspace);
            (released, started.elapsed())
        });
        std::thread::sleep(Duration::from_secs(2));
        blocker.batch_execute("COMMIT").unwrap();
        unmounting.join().unwrap()
    });
    let mut admin = connect(&store);
    let me = pid_of(&mut admin);
    let keys = mount_keys(&store, branch);
    let held = key_holders(&mut admin, &keys, &[me]);
    let rows = live_mounts(&mut admin, branch);
    let rebound = service.try_bind(branch);
    let rebound_ok = rebound.is_ok();
    if let Ok(workspace) = rebound {
        let _ = service.try_unmount(workspace);
    }
    let writer_alive = alive(&mut admin, writer);
    println!(
        "T768_UNMOUNT_LOCKED {{\"answer\":{released:?},\"ms\":{},\"keys\":{},\"rows\":{rows},\"rebound\":{rebound_ok},\"writer_alive\":{writer_alive}}}",
        took.as_millis(),
        held.len()
    );
    match (&released, rebound_ok) {
        (Ok(()), _) => assert_eq!((held.len(), rows), (0, 0), "released"),
        (Err(_), true) => assert_eq!(
            (held.len(), rows),
            (3, 1),
            "a definite refusal keeps custody"
        ),
        (Err(_), false) => {
            assert!(
                wait_until(Duration::from_secs(10), || !alive(&mut admin, writer)),
                "a quarantined writer's backend ends"
            );
            assert!(key_holders(&mut admin, &keys, &[me]).is_empty());
        }
    }
    service.abandon();
    let fresh = store.open();
    let workspace = fresh.bind(branch);
    fresh.unmount(workspace);
    fresh.stop();
}

/// The mount paused through the proxy (`at`: message 0 is the snapshot's
/// first reply, before the admission is sent; `after_snapshot + 1` the
/// admission's first reply, after the server ran it), `act` run from a
/// second, direct service meanwhile, then released.
struct Paused {
    proxy: FaultProxy,
    service: Service,
    /// The index of the admission message's first reply.
    admission: u64,
}

fn paused(store: &TestStore, branch: BranchId) -> Paused {
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let service = Service::open(store, store.selection_via(&proxied));
    warm(&service, branch);
    proxy.arm(None);
    let workspace = service.bind(branch);
    let window = proxy.disarm();
    service.unmount(workspace);
    let kinds: String = window.log.iter().map(|(_, kind)| *kind).collect();
    let connections: std::collections::BTreeSet<usize> =
        window.log.iter().map(|(c, _)| *c).collect();
    println!("T768_MOUNT_FLIGHT {kinds}");
    assert_eq!(
        connections.len(),
        1,
        "a repeated mount is the writer's alone"
    );
    let admission = kinds.find('Z').expect("the snapshot's ReadyForQuery") as u64 + 1;
    assert!(admission < window.seen, "an admission message after it");
    Paused {
        proxy,
        service,
        admission,
    }
}

impl Paused {
    fn run<T: Send>(
        &self,
        branch: BranchId,
        at: u64,
        act: impl FnOnce() -> T + Send,
    ) -> (Result<adversary_bound::Bound, String>, T) {
        self.proxy.arm(Some((at, Fault::Pause)));
        let (bound, acted) = std::thread::scope(|scope| {
            let binding = scope.spawn(|| self.service.try_bind(branch));
            assert!(self.proxy.wait_fired(Duration::from_secs(20)), "paused");
            let acted = act();
            self.proxy.release();
            (binding.join().unwrap(), acted)
        });
        self.proxy.disarm();
        (bound, acted)
    }
}

mod adversary_bound {
    pub type Bound = layerfs_store_service::store::BoundWorkspace;
}

#[test]
fn a_branch_removed_between_snapshot_and_admission_refuses_the_mount() {
    let Some(store) = TestStore::postgres("t768-pause-branch", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let other = store.fork("other");
    let paused = paused(&store, branch);
    let remover = store.open();
    let (bound, removed) = paused.run(branch, 0, || {
        code(&remover.call("branch.remove", vec![("branch", SdkValue::text("work"))]))
    });
    println!(
        "T768_PAUSE_BRANCH {{\"removed\":\"{removed}\",\"mount\":{:?}}}",
        bound.as_ref().map(|_| ())
    );
    assert_eq!(removed, "Ok", "no intent yet: the removal runs");
    let error = bound.err().expect("the removed Branch is not mounted");
    assert!(
        error.contains("MissingBranch") || error.contains("Missing") || error.contains("Retired"),
        "{error}"
    );
    assert_clean(&store, branch, &[], "removed Branch");
    // The same service still mounts.
    let workspace = paused.service.bind(other);
    paused.service.unmount(workspace);
    remover.stop();
    let Paused { service, .. } = paused;
    service.stop();
    assert!(store.no_open_transaction());
}

#[test]
fn a_path_removed_between_snapshot_and_admission_is_hidden_in_that_mount() {
    let Some(store) = TestStore::postgres("t768-pause-path", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let paused = paused(&store, branch);
    let remover = store.open();
    let (bound, removed) = paused.run(branch, 0, || {
        code(&remover.call(
            "project.remove_file",
            vec![("path", SdkValue::text("/victim"))],
        ))
    });
    println!(
        "T768_PAUSE_PATH_BEFORE {{\"removed\":\"{removed}\",\"mount\":{:?}}}",
        bound.as_ref().map(|_| ())
    );
    assert_eq!(removed, "Ok", "no intent yet: the removal lands");
    let workspace = bound.expect("the mount is admitted after the removal");
    let tree = tree_of(&workspace);
    assert!(
        !tree.contains_key("/victim"),
        "the marker committed before the intent is seen by that mount: {tree:?}"
    );
    assert_eq!(tree.get("/seed"), Some(&Node::File(b"seed\n".to_vec())));
    paused.service.unmount(workspace);
    assert_clean(&store, branch, &[], "after");
    remover.stop();
    let Paused { service, .. } = paused;
    service.stop();
}

#[test]
fn exclusive_operations_after_the_admission_meet_the_intent() {
    let Some(store) = TestStore::postgres("t768-pause-after", SEED) else {
        return;
    };
    let branch = store.fork("work");
    let paused = paused(&store, branch);
    let remover = store.open();
    let at = paused.admission;
    let (bound, answers) = paused.run(branch, at, || {
        // The server ran the whole message: wait for the committed row.
        let mut admin = connect(&store);
        assert!(
            wait_until(Duration::from_secs(10), || live_mounts(&mut admin, branch)
                == 1),
            "the intent committed while the reply is held"
        );
        let path = code(&remover.call(
            "project.remove_file",
            vec![("path", SdkValue::text("/victim"))],
        ));
        let gone = code(&remover.call("branch.remove", vec![("branch", SdkValue::text("work"))]));
        let compaction = compact(&store, 0x61);
        (path, gone, compaction)
    });
    println!(
        "T768_PAUSE_AFTER {{\"answers\":{answers:?},\"mount\":{:?}}}",
        bound.as_ref().map(|_| ())
    );
    let (path, gone, compaction) = answers;
    for (what, answer) in [
        ("remove_file", &path),
        ("branch.remove", &gone),
        ("compaction", &compaction),
    ] {
        assert!(
            answer == "MountsActive" || answer == "Busy",
            "{what} beside an admitted mount answered {answer}"
        );
    }
    let workspace = bound.expect("the paused mount completes");
    let tree = tree_of(&workspace);
    assert_eq!(
        tree.get("/victim"),
        Some(&Node::File(b"victim\n".to_vec())),
        "a refused removal hides nothing"
    );
    paused.service.unmount(workspace);
    assert_clean(&store, branch, &[], "after");
    // Now the removal lands and a later mount hides the path.
    assert_eq!(
        code(&remover.call(
            "project.remove_file",
            vec![("path", SdkValue::text("/victim"))],
        )),
        "Ok"
    );
    let workspace = paused.service.bind(branch);
    assert!(!tree_of(&workspace).contains_key("/victim"));
    paused.service.unmount(workspace);
    remover.stop();
    let Paused { service, .. } = paused;
    service.stop();
    assert!(store.no_open_transaction());
}

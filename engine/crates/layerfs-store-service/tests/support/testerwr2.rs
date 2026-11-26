//! Rig of the second independent writer-reopen round (`testerwr2_*`): a
//! PostgreSQL Store, a [`FaultProxy`] to its server, and one writable
//! persistence session dialing through it with reopen enabled and its mount
//! statements prepared, as the Store service opens its writer. The GC
//! registration entry points (`gc_register`, `gc_release` and the
//! `internals` re-exports of the one message and the statement-by-statement
//! registration) are called on that session directly, so a test chooses
//! exactly which writer turn the proxy holds. Every outcome is read from an
//! independent admin connection: `pg_locks`, `pg_stat_activity` and
//! `gc_intent`.
//!
//! Nothing here is product code. PostgreSQL only; NOT_RUN without
//! `LAYERFS_TEST_POSTGRES_URL`.
use super::adversary::{TestStore, CURSOR, PROJECT};
use super::fault_proxy::FaultProxy;
use super::testerwr::{keys_of, LOCK_TIMEOUT_MS, SEED};
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{BranchId, CatalogId, HistoryCatalog};
use layerfs_persistence::{Handles, PostgresHandles};
use std::time::Duration;

/// How long a test lets a spawned call reach the writer's queue before it
/// lets the held turn go on: the queue is first come, first served, so a
/// call queued by then takes the writer's next turn.
pub const QUEUE: Duration = Duration::from_millis(500);

pub struct Rig2 {
    pub store: TestStore,
    pub proxy: FaultProxy,
    pub handles: PostgresHandles,
}

impl Rig2 {
    pub fn history(&self) -> &dyn HistoryCatalog {
        &self.handles.history
    }
}

/// A Store and one reopening writer session through a fault proxy.
pub fn rig2(label: &str) -> Option<Rig2> {
    rig2_with(label, LOCK_TIMEOUT_MS)
}

/// [`rig2`] with another lock timeout (also the lost-backend end bound).
pub fn rig2_with(label: &str, lock_timeout_ms: u32) -> Option<Rig2> {
    let store = TestStore::postgres(label, SEED)?;
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let selection = store.with_lock_timeout(&store.selection_via(&proxied), lock_timeout_ms);
    let Handles::Postgres(handles) =
        Handles::open_writable(&selection, &store.binding, CURSOR).unwrap()
    else {
        unreachable!()
    };
    handles.storage.enable_reopen();
    handles.storage.prepare_mount_writes().unwrap();
    Some(Rig2 {
        store,
        proxy,
        handles,
    })
}

/// A Workspace mount intent of Branch `name` (`branch`), op `[tag; 16]`.
/// Its owner tokens prove nothing dead, so only its owner releases it.
pub fn mount(store: &TestStore, branch: BranchId, name: &str, tag: u8) -> GcIntent {
    GcIntent::shared(
        [tag; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(store.stack.as_slice())
    .named(Some(PROJECT), Some(name))
    .placed(Some([tag; 32]), Some("sandbox-wr2"), Some("machine-wr2"))
    .owned(Some("runtime-wr2"), Some("boot-wr2"))
}

/// A merge into Branch `target` of the project, op `[tag; 16]`: a live
/// operation a mount of the project refuses (`Busy`), registered with
/// shared keys only, so it coexists with live mounts.
pub fn merge(store: &TestStore, target: &str, tag: u8) -> GcIntent {
    GcIntent::shared(
        [tag; 16],
        GcIntentKind::Merge,
        GcScopeKind::Project,
        store.stack.as_slice(),
    )
    .named(Some(PROJECT), Some(target))
    .owned(Some("runtime-wr2"), Some("boot-wr2"))
}

/// The session advisory key of `kind` and `id` (spec 5.2).
pub fn advisory_key(store: &TestStore, kind: u8, id: &[u8]) -> i64 {
    let catalog = CatalogId::derive(&store.binding).unwrap();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/advisory/v1\0");
    hasher.update(catalog.as_slice());
    hasher.update(&[kind]);
    hasher.update(id);
    let mut head = [0u8; 8];
    head.copy_from_slice(&hasher.finalize().as_bytes()[..8]);
    i64::from_be_bytes(head)
}

/// The three keys a mount of `branch` holds (Store, project, Branch), sorted.
pub fn mount_keys(store: &TestStore, branch: BranchId) -> Vec<i64> {
    let catalog = CatalogId::derive(&store.binding).unwrap();
    let mut keys = vec![
        advisory_key(store, 1, catalog.as_slice()),
        advisory_key(store, 2, store.stack.as_slice()),
        advisory_key(store, 3, branch.as_slice()),
    ];
    keys.sort_unstable();
    keys
}

/// Proxied backends still in `pg_stat_activity`.
pub fn live(rig: &Rig2) -> Vec<i32> {
    let pids: Vec<i32> = rig.proxy.pids().into_iter().flatten().collect();
    rig.store
        .admin()
        .query(
            "SELECT pid FROM pg_stat_activity WHERE pid = ANY($1) ORDER BY pid",
            &[&pids],
        )
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

/// The writer's backend: the one live proxied backend (waiting up to 10 s
/// for closed connections' backends to exit).
pub fn writer(rig: &Rig2) -> i32 {
    let mut last = Vec::new();
    super::adversary::wait_until(Duration::from_secs(10), || {
        last = live(rig);
        last.len() == 1
    });
    assert_eq!(last.len(), 1, "one live writer backend: {last:?}");
    last[0]
}

/// The keys the writer's backend holds, sorted.
pub fn writer_keys(rig: &Rig2) -> (i32, Vec<i64>) {
    let pid = writer(rig);
    (pid, keys_of(&rig.store, pid))
}

/// The `gc_intent` states of op `op`.
pub fn op_rows(store: &TestStore, op: &[u8; 16]) -> Vec<String> {
    store
        .admin()
        .query(
            "SELECT state FROM gc_intent WHERE op_id = $1 ORDER BY state",
            &[&op.as_slice()],
        )
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

/// One read on the writer session: a call that takes the writer's turn
/// (and reopens a quarantined writer).
pub fn read(rig: &Rig2, branch: BranchId) -> Result<bool, String> {
    rig.history()
        .branch(branch)
        .map(|found| found.is_some())
        .map_err(|error| format!("{error:?}"))
}

/// The writer's reopens so far.
pub fn reopens(rig: &Rig2) -> u64 {
    rig.handles.diagnostics().unwrap().reopens
}

/// Server messages per kind, `N` (notices) left out, in order.
pub fn kinds(log: &[(usize, char)], connection: usize) -> String {
    log.iter()
        .filter(|(c, kind)| *c == connection && *kind != 'N')
        .map(|(_, kind)| *kind)
        .collect()
}

/// A PostgreSQL backend stopped with `SIGSTOP` inside the test server's
/// container (`LAYERFS_TEST_POSTGRES_CONTAINER`); continued on drop, so a
/// failing test never leaves it stopped.
pub struct Stopped {
    container: String,
    pid: i32,
    continued: bool,
}

/// The test server's container, or `None` with the NOT_RUN note.
pub fn container() -> Option<String> {
    match std::env::var("LAYERFS_TEST_POSTGRES_CONTAINER") {
        Ok(name) if !name.is_empty() => Some(name),
        _ => {
            println!(
                "NOT_RUN: LAYERFS_TEST_POSTGRES_CONTAINER (the server's Docker container) is not set"
            );
            None
        }
    }
}

fn signal(container: &str, pid: i32, name: &str) -> bool {
    std::process::Command::new("docker")
        .args(["exec", container, "kill", name, &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

impl Stopped {
    pub fn stop(container: &str, pid: i32) -> Self {
        assert!(signal(container, pid, "-STOP"), "SIGSTOP {pid}");
        Self {
            container: container.to_owned(),
            pid,
            continued: false,
        }
    }

    pub fn resume(mut self) {
        self.continued = signal(&self.container, self.pid, "-CONT");
        assert!(self.continued, "SIGCONT {}", self.pid);
    }
}

impl Drop for Stopped {
    fn drop(&mut self) {
        if !self.continued {
            let _ = signal(&self.container, self.pid, "-CONT");
        }
    }
}

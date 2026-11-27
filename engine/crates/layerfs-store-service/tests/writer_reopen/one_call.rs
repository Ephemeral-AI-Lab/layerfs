//! Second independent round of writer-reopen tests, T3 (fix round
//! 1d0a121012): the reopen budget, `layerfs_persistence::one_call`. Each
//! probe quarantines a reopening writer session (its backend terminated
//! from an admin connection) while a counting front gate refuses every new
//! connection as a stopped server would, and counts the reopen attempts
//! (connections dialled) a call shape makes:
//!
//! * nested calls share the outer call's one attempt;
//! * a call on a thread spawned inside a call has a budget of its own
//!   (`one_call`'s scope is thread-local);
//! * a caller outside any scope gets one attempt per writer turn;
//! * a spent budget refuses (`Unknown`) a later turn of the same call even
//!   once the server accepts again, and the next call reopens.
//!
//! These record the budget's behaviour; the assertions pin what was
//! observed so a change to it is seen.
//!
//! PostgreSQL only. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/verify/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "../support/fault/proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "../support/verify/reopen_rig.rs"]
#[allow(dead_code)]
mod testerwr;

use adversary::{wait_until, TestStore, CURSOR};
use fault_proxy::{proxied_url, FaultProxy};
use layerfs_history::{BranchId, HistoryCatalog};
use layerfs_persistence::{one_call, Handles, PostgresHandles};
use std::time::Duration;
use testerwr::{kill, FrontGate, LOCK_TIMEOUT_MS, SEED};

struct Gated {
    store: TestStore,
    proxy: FaultProxy,
    gate: FrontGate,
    handles: PostgresHandles,
    branch: BranchId,
}

fn gated(label: &str) -> Option<Gated> {
    let store = TestStore::postgres(label, SEED)?;
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let gate = FrontGate::start(proxy.port);
    let (via, _) = proxied_url(&url, gate.port);
    let selection = store.with_lock_timeout(&store.selection_via(&via), LOCK_TIMEOUT_MS);
    let Handles::Postgres(handles) =
        Handles::open_writable(&selection, &store.binding, CURSOR).unwrap()
    else {
        unreachable!()
    };
    handles.storage.enable_reopen();
    let branch = store.fork("a");
    Some(Gated {
        store,
        proxy,
        gate,
        handles,
        branch,
    })
}

impl Gated {
    fn read(&self) -> Result<(), String> {
        let history: &dyn HistoryCatalog = &self.handles.history;
        history
            .branch(self.branch)
            .map(drop)
            .map_err(|error| format!("{error:?}"))
    }

    /// Terminates the session's backend and refuses new connections.
    fn down(&self) {
        let backend = self.backend();
        kill(&self.store, backend);
        self.gate.refuse(true);
    }

    /// The session's backend: the one live proxied backend.
    fn backend(&self) -> i32 {
        let pids: Vec<i32> = self.proxy.pids().into_iter().flatten().collect();
        let mut admin = self.store.admin();
        let mut found = Vec::new();
        wait_until(Duration::from_secs(10), || {
            found = admin
                .query(
                    "SELECT pid FROM pg_stat_activity WHERE pid = ANY($1)",
                    &[&pids],
                )
                .unwrap()
                .iter()
                .map(|row| row.get::<_, i32>(0))
                .collect();
            found.len() == 1
        });
        assert_eq!(found.len(), 1, "one live backend: {found:?}");
        found[0]
    }

    fn attempts(&self) -> u64 {
        self.gate.attempts()
    }
}

/// Runs `probe` with the session quarantined and the server refusing;
/// returns the connection attempts it made and what it answered.
fn probe<T>(rig: &Gated, probe: impl FnOnce(&Gated) -> T) -> (u64, T) {
    rig.down();
    let before = rig.attempts();
    let answer = probe(rig);
    (rig.attempts() - before, answer)
}

/// Brings the server back and reopens the session outside any scope.
fn back(rig: &Gated) {
    rig.gate.refuse(false);
    rig.read().expect("reopens once the server accepts");
}

#[test]
fn t3_one_call_budget_shapes() {
    let Some(rig) = gated("wr2_one_call") else {
        return;
    };
    rig.read().unwrap();
    // Nested scopes share the outer one's attempt.
    let (nested, answers) = probe(&rig, |rig| {
        one_call(|| {
            let inner = one_call(|| rig.read());
            let outer = rig.read();
            (inner, outer)
        })
    });
    println!("TESTERWR2_ONE_CALL nested: attempts {nested}, answers {answers:?}");
    back(&rig);
    // A thread spawned inside a call.
    let (spawned, answers) = probe(&rig, |rig| {
        one_call(|| {
            let thread = std::thread::scope(|scope| scope.spawn(|| rig.read()).join().unwrap());
            let caller = rig.read();
            (thread, caller)
        })
    });
    println!("TESTERWR2_ONE_CALL spawned thread: attempts {spawned}, answers {answers:?}");
    back(&rig);
    // A thread spawned inside a call that itself scopes its work.
    let (spawned_scoped, _) = probe(&rig, |rig| {
        one_call(|| {
            let thread =
                std::thread::scope(|scope| scope.spawn(|| one_call(|| rig.read())).join().unwrap());
            (thread, rig.read())
        })
    });
    println!("TESTERWR2_ONE_CALL spawned scoped thread: attempts {spawned_scoped}");
    back(&rig);
    // No scope: one attempt per writer turn.
    let (unscoped, answers) = probe(&rig, |rig| (rig.read(), rig.read(), rig.read()));
    println!("TESTERWR2_ONE_CALL no scope, three turns: attempts {unscoped}, answers {answers:?}");
    back(&rig);
    // A spent budget: the server accepts again within the call.
    let (spent, answers) = probe(&rig, |rig| {
        one_call(|| {
            let refused = rig.read();
            rig.gate.refuse(false);
            let later = rig.read();
            (refused, later)
        })
    });
    let next = rig.read();
    println!("TESTERWR2_ONE_CALL spent: attempts {spent}, answers {answers:?}; next call {next:?}");
    assert_eq!(nested, 1, "nested scopes share one attempt");
    assert_eq!(spawned, 2, "a spawned thread has a budget of its own");
    assert_eq!(
        spawned_scoped, 2,
        "a spawned thread's own scope: its own attempt"
    );
    assert_eq!(unscoped, 3, "outside a scope: one attempt per turn");
    assert_eq!(spent, 1, "a spent budget makes no second attempt");
    assert!(answers.1.is_err(), "refused once the budget is spent");
    assert!(next.is_ok(), "the next call reopens");
    rig.handles.close();
}

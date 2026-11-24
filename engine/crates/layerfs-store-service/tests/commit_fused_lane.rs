//! #768 stage 2: the fused Workspace Commit (one pipelined PostgreSQL
//! message: the Save's last publication, the hints and the Commit in one
//! transaction) stopped by server-side lock waits at points in its message,
//! then disturbed there.
//!
//! The message takes the lane row and the barrier itself (step 3), reads the
//! Branch and the project epoch (step 4), then writes the publication's rows
//! (`store_totals`, step 5), the next history serial (`history_meta`, step
//! 7), the Branch advance (`branch`, step 9) and re-reads the epoch `FOR
//! SHARE` (step 10). A blocker transaction holds one of those rows, so the
//! server stops the message exactly there (checked: the waiting backend's
//! tuple lock names that row's table; only that step touches it), with
//! every earlier step done and the lane
//! held from step 3 on. Then one of:
//!
//! * the writer is killed: no reply, an unknown outcome; nothing written
//!   (counts and totals unchanged), no fallback, the Workspace's next
//!   Commit refused, the lane free again;
//! * the blocker is killed: the message completes and the Commit is fused;
//! * (after step 4) the project epoch is bumped meanwhile: `Conflict`
//!   ("project epoch"), nothing written, and no fallback;
//! * another service's `branch.remove` (which bumps the epoch under the
//!   lane) runs with a short lane wait: `Busy`, and the Commit lands; or it
//!   lands, and the Commit is refused `Conflict` with nothing written.
//!
//! Each case checks the Branch from fresh sessions (`check_attempt`), no
//! backend left in a transaction, a later Commit publishing well inside the
//! lane wait, and the audit. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN
//! without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;

use adversary::{
    check_attempt, commit, verdict, wait_until, write, Node, Service, TestStore, Tree, Verdict,
};
use layerfs_bridge::control::SdkValue;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
/// The Commit's lane wait: longer than any case, so only the case decides.
const LOCK_TIMEOUT_MS: u32 = 20_000;
const BOUND: Duration = Duration::from_secs(30);

/// The row a blocker holds, and so the step the message waits at.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Blocker {
    /// `store_lane`: step 3, before the lane is held.
    Lane,
    /// `store_totals`: step 5, the publication's rows.
    Totals,
    /// `history_meta`: step 7, the next history serial.
    Serial,
    /// `branch`: step 9, the Branch advance.
    Branch,
    /// `project_epoch`: step 10, the epoch re-read `FOR SHARE`.
    Epoch,
}

impl Blocker {
    fn lock(self) -> &'static str {
        match self {
            Self::Lane => "SELECT 1 FROM store_lane FOR UPDATE",
            Self::Totals => "SELECT 1 FROM store_totals FOR UPDATE",
            Self::Serial => "SELECT 1 FROM history_meta FOR UPDATE",
            Self::Branch => "SELECT 1 FROM branch FOR UPDATE",
            Self::Epoch => "SELECT 1 FROM project_epoch FOR UPDATE",
        }
    }
    /// The table whose row the waiting backend's tuple lock names.
    fn table(self) -> &'static str {
        match self {
            Self::Lane => "store_lane",
            Self::Totals => "store_totals",
            Self::Serial => "history_meta",
            Self::Branch => "branch",
            Self::Epoch => "project_epoch",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    KillWriter,
    KillBlocker,
    BumpEpoch,
    RemoveBranch,
}

/// Everything a Commit could have written, read with admin SQL.
fn written(admin: &mut postgres::Client) -> String {
    let row = admin
        .query_one(
            "SELECT (SELECT count(*) FROM \"commit\")::text || '/' || \
             (SELECT count(*) FROM commit_lineage)::text || '/' || \
             (SELECT count(*) FROM object_location)::text || '/' || \
             (SELECT count(*) FROM pack)::text || '/' || \
             (SELECT count(*) FROM binding_hint_block)::text || '/' || \
             (SELECT next_history_serial FROM history_meta WHERE id = 1)::text || '/' || \
             (SELECT canonical::text || ':' || stored::text || ':' || metadata::text \
              FROM store_totals WHERE id = 1)",
            &[],
        )
        .unwrap();
    row.get(0)
}

fn terminate(admin: &mut postgres::Client, pid: i32) {
    admin
        .execute("SELECT pg_terminate_backend($1)", &[&pid])
        .unwrap();
    assert!(
        wait_until(Duration::from_secs(10), || admin
            .query_one(
                "SELECT count(*) FROM pg_stat_activity WHERE pid = $1",
                &[&pid]
            )
            .unwrap()
            .get::<_, i64>(0)
            == 0),
        "backend {pid} is gone"
    );
}

/// One case on a new Branch `name`: a warm-up Commit, then the Commit of
/// `/x` stopped at `blocker` and disturbed by `action`.
fn case(store: &TestStore, remover: &Service, blocker: Blocker, action: Action) {
    let name = format!("{blocker:?}-{action:?}").to_lowercase();
    let gone = format!("gone-{name}");
    store.fork(&gone);
    let branch = store.fork(&name);
    let service = Service::open(
        store,
        store.with_lock_timeout(&store.selection, LOCK_TIMEOUT_MS),
    );
    let workspace = service.bind(branch);
    write(&workspace, "/warm", b"warm\n");
    let old = match verdict(&commit(&workspace)) {
        Verdict::Committed(id) => id,
        other => panic!("{name}: warm-up {other:?}"),
    };
    write(&workspace, "/x", b"x\n");
    let mut observer = store.admin();
    let before = written(&mut observer);
    let mut holder = store.admin();
    let holder_pid: i32 = holder
        .query_one("SELECT pg_backend_pid()", &[])
        .unwrap()
        .get(0);
    holder
        .batch_execute(&format!("BEGIN; {}", blocker.lock()))
        .unwrap();
    let counts = service.service.store.fused_counts();
    let (sent, answer) = mpsc::channel();
    let (first, removal, after_first) = std::thread::scope(|scope| {
        let workspace = &workspace;
        scope.spawn(move || {
            let _ = sent.send(verdict(&commit(workspace)));
        });
        // The message stops at the blocker's row: the backend blocked by
        // the holder holds or waits for a tuple lock on that row's table.
        // (`pg_stat_activity.query` is not used: in a pipeline it may show
        // another statement of the message, and only its first 1 KiB.)
        let mut waiting: Option<(i32, String, String)> = None;
        let stopped = wait_until(Duration::from_secs(10), || {
            waiting = observer
                .query(
                    "SELECT a.pid, coalesce(a.query, ''), coalesce((SELECT string_agg(c.relname, ',') \
                     FROM pg_locks l JOIN pg_class c ON c.oid = l.relation \
                     WHERE l.pid = a.pid AND l.locktype = 'tuple'), '') \
                     FROM pg_stat_activity a WHERE $1 = ANY(pg_blocking_pids(a.pid))",
                    &[&holder_pid],
                )
                .unwrap()
                .first()
                .map(|row| (row.get(0), row.get(1), row.get(2)));
            waiting
                .as_ref()
                .is_some_and(|(_, _, table)| !table.is_empty())
        });
        assert!(
            stopped,
            "{name}: the message waits for the held row: {waiting:?}"
        );
        let (writer, statement, table) = waiting.unwrap();
        assert_eq!(table, blocker.table(), "{name}: waits in {statement}");
        let mut removal = None;
        match action {
            Action::KillWriter => {
                terminate(&mut observer, writer);
                holder.batch_execute("ROLLBACK").unwrap();
            }
            Action::KillBlocker => terminate(&mut observer, holder_pid),
            Action::BumpEpoch if blocker == Blocker::Epoch => holder
                .batch_execute("UPDATE project_epoch SET epoch = epoch + 1; COMMIT")
                .unwrap(),
            Action::BumpEpoch => {
                observer
                    .batch_execute("UPDATE project_epoch SET epoch = epoch + 1")
                    .unwrap();
                holder.batch_execute("ROLLBACK").unwrap();
            }
            Action::RemoveBranch => {
                removal = Some(
                    match remover.call("branch.remove", vec![("branch", SdkValue::text(&gone))]) {
                        Ok(_) => "Ok".to_owned(),
                        Err(refusal) => refusal.code,
                    },
                );
                holder.batch_execute("ROLLBACK").unwrap();
            }
        }
        let first = answer.recv_timeout(BOUND).expect("the Commit answers");
        (first, removal, written(&mut observer))
    });
    let fused = service.service.store.fused_counts();
    let counts = (fused.0 - counts.0, fused.1 - counts.1);
    let follow = verdict(&commit(&workspace));
    println!(
        "FUSED_LANE {{\"case\":\"{name}\",\"first\":\"{first:?}\",\"counts\":\"{counts:?}\",\"removal\":\"{removal:?}\",\"follow\":\"{follow:?}\"}}"
    );
    let _ = service.try_unmount(workspace);
    service.abandon();
    assert_eq!(counts.1, 0, "{name}: no fallback");
    let label = format!("{name}: first {first:?}, removal {removal:?}");
    let nothing_written = || assert_eq!(after_first, before, "{label}: nothing written");
    match (action, removal.as_deref()) {
        (Action::KillWriter, _) => {
            assert!(
                matches!(first, Verdict::Unknown(_) | Verdict::Refused(_)),
                "{label}"
            );
            assert_eq!(counts, (0, 0), "{label}");
            nothing_written();
        }
        (Action::KillBlocker, _) | (Action::RemoveBranch, Some("Busy")) => {
            assert!(matches!(first, Verdict::Committed(_)), "{label}");
            assert_eq!(counts, (1, 0), "{label}: fused");
        }
        (Action::BumpEpoch, _) | (Action::RemoveBranch, Some("Ok")) => {
            match &first {
                Verdict::Refused(text) => {
                    assert!(text.contains("project epoch"), "{label}: {text}")
                }
                other => panic!("{label}: an epoch bump inside the lane answered {other:?}"),
            }
            assert_eq!(counts, (0, 0), "{label}");
            nothing_written();
        }
        (Action::RemoveBranch, other) => panic!("{label}: branch.remove answered {other:?}"),
    }
    let mut expected: Tree = Tree::new();
    expected.insert("/seed".into(), Node::File(b"seed\n".to_vec()));
    expected.insert("/warm".into(), Node::File(b"warm\n".to_vec()));
    expected.insert("/x".into(), Node::File(b"x\n".to_vec()));
    check_attempt(store, branch, Some(old), &expected, &first, &follow, &label);
    let handles = store.handles();
    let present = adversary::history(&handles)
        .branch(store.branch(&gone))
        .unwrap()
        .is_some();
    handles.close();
    assert_eq!(present, removal.as_deref() != Some("Ok"), "{label}: {gone}");
    assert!(
        store.no_open_transaction(),
        "{label}: a backend left in a transaction"
    );
}

fn blocker_cases(tag: &str, blocker: Blocker, actions: &[Action]) {
    let Some(store) = TestStore::postgres(tag, SEED) else {
        return;
    };
    let remover = Service::open(&store, store.with_lock_timeout(&store.selection, 300));
    for action in actions {
        case(&store, &remover, blocker, *action);
    }
    remover.stop();
    // Nothing kept the lane: a new Commit publishes well inside the wait.
    let started = Instant::now();
    let after = store.fork("after");
    let service = store.open();
    let workspace = service.bind(after);
    write(&workspace, "/after", b"after\n");
    assert!(matches!(
        verdict(&commit(&workspace)),
        Verdict::Committed(_)
    ));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the lane is free"
    );
    service.unmount(workspace);
    service.stop();
    let failures = store.integrity_failures();
    assert!(failures.is_empty(), "audit: {failures:#?}");
}

const AFTER_HEAD_READ: [Action; 4] = [
    Action::KillWriter,
    Action::KillBlocker,
    Action::BumpEpoch,
    Action::RemoveBranch,
];

#[test]
fn the_fused_message_waiting_for_the_lane_row() {
    blocker_cases(
        "fused_lane_row",
        Blocker::Lane,
        &[
            Action::KillWriter,
            Action::KillBlocker,
            Action::RemoveBranch,
        ],
    );
}

#[test]
fn the_fused_message_stopped_at_its_publication_rows() {
    blocker_cases("fused_totals", Blocker::Totals, &AFTER_HEAD_READ);
}

#[test]
fn the_fused_message_stopped_at_its_history_serial() {
    blocker_cases("fused_serial", Blocker::Serial, &AFTER_HEAD_READ);
}

#[test]
fn the_fused_message_stopped_at_its_branch_advance() {
    blocker_cases("fused_branch", Blocker::Branch, &AFTER_HEAD_READ);
}

#[test]
fn the_fused_message_stopped_at_its_epoch_check() {
    blocker_cases("fused_epoch", Blocker::Epoch, &AFTER_HEAD_READ);
}

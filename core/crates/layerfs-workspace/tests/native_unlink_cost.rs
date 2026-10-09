//! The exact SQL one created and removed file costs a mount: CREATE with an
//! open descriptor, a two-byte WRITE, RELEASE, UNLINK under the kernel's
//! lookup reference, FORGET of that reference, and the maintenance steps the
//! removal leaves behind. The jobs run through the public visit API over a
//! real canonical base and Overlay, beside N and 4N kept files, in an engine
//! that has never made an orphan, in the same engine after its orphans were
//! reclaimed, and in a second Workspace opened on it later. Reclamation of a closed namespace is
//! not measured here.
mod common;
mod harness;
use common::name;
use harness::{create, unlink, Bench, T1};
use layerfs_overlay::{
    DatabaseWork, MaintenanceCursor, NativeMount, OpenFile, Overlay, StatementKind, StatementWork,
};
use layerfs_workspace::{
    CanonicalClient, JobOutcome, NativeInput, NativeReadDecision, NativeReadOperation,
    NativeVisitRequest, Need, Refusal, VisitFacts, Workspace,
};
use std::{cell::Cell, sync::Arc};

/// Statement families in the order a receipt prints them.
const FAMILIES: [(StatementKind, &str); 14] = [
    (StatementKind::Startup, "Startup"),
    (StatementKind::Begin, "Begin"),
    (StatementKind::Commit, "Commit"),
    (StatementKind::Rollback, "Rollback"),
    (StatementKind::Workspace, "Workspace"),
    (StatementKind::Inode, "Inode"),
    (StatementKind::DirectoryEntry, "DirectoryEntry"),
    (StatementKind::Payload, "Payload"),
    (StatementKind::Frontier, "Frontier"),
    (StatementKind::Capture, "Capture"),
    (StatementKind::OperationRecord, "OperationRecord"),
    (StatementKind::Lease, "Lease"),
    (StatementKind::Explain, "Explain"),
    (StatementKind::Reclaim, "Reclaim"),
];
/// (family, attempts, executions) of every family one job touched.
type Cost = Vec<(&'static str, u64, u64)>;

fn cost(work: &DatabaseWork) -> Cost {
    FAMILIES
        .iter()
        .map(|(kind, label)| (*label, work.statements[*kind as usize]))
        .filter(|(_, family)| family.attempts != 0)
        .map(|(label, family)| (label, family.attempts, family.executions))
        .collect()
}
/// One job's work, which must contain no scan, sort, automatic index or
/// second preparation.
fn measured<T>(overlay: &Overlay, job: impl FnOnce() -> T) -> (T, DatabaseWork) {
    let before = overlay.diagnostics();
    let value = job();
    let work = overlay.diagnostics().since(&before);
    let total: StatementWork = work.total();
    assert_eq!(
        (
            total.fullscan_steps,
            total.sorts,
            total.autoindex_rows,
            total.reprepares
        ),
        (0, 0, 0, 0),
        "{work:?}"
    );
    (value, work)
}
/// One mounted Workspace of the engine under test.
struct Mounted<'a> {
    b: &'a Bench,
    workspace: &'a Workspace,
    resident: Arc<CanonicalClient>,
    mount: NativeMount,
    requests: Cell<u64>,
}
/// What one created and removed file cost.
#[derive(Debug, Eq, PartialEq)]
struct Removed {
    /// Negative LOOKUP, CREATE, WRITE, RELEASE, UNLINK and FORGET.
    jobs: [Cost; 6],
    /// Maintenance turns that did work, before the turn that found none.
    steps: u64,
    /// Rows those steps reported.
    rows: u64,
    /// Every maintenance turn, including the ones that found nothing.
    maintenance: Cost,
    /// Inode-family attempts of the six jobs.
    inode: u64,
}
impl Mounted<'_> {
    fn request(&self) -> u64 {
        self.requests.set(self.requests.get() + 1);
        self.requests.get()
    }
    /// Every ready maintenance step, one transaction each, and their rows.
    fn drain(&self) -> (u64, u64, DatabaseWork) {
        let overlay = &self.b.overlay;
        let ((steps, rows), work) = measured(overlay, || {
            let mut cursor = MaintenanceCursor::default();
            let (mut steps, mut rows) = (0, 0);
            while let Some(step) = overlay.maintain(cursor).unwrap() {
                assert!(step.work.rows <= 64 && step.work.data_bytes <= 65536);
                assert!(steps < 64, "maintenance does not settle");
                steps += 1;
                rows += step.work.rows;
                cursor = step.cursor;
            }
            (steps, rows)
        });
        (steps, rows, work)
    }
    fn mutation(&self, serial: u64, handle: Option<u64>, input: NativeInput) -> NativeVisitRequest {
        NativeVisitRequest {
            mount: self.mount,
            request: self.request(),
            serial,
            handle,
            input,
            open: None,
            now: T1,
            fresh: None,
            facts: Arc::new(VisitFacts::default()),
        }
    }
    /// A file that keeps its name and its kernel reference.
    fn keep(&self, child: &str) {
        let overlay = &self.b.overlay;
        let serial = self.workspace.next_serial(&self.b.allocator).unwrap();
        let mut request = self.mutation(1, None, NativeInput::Named(create(1, child)));
        (request.open, request.fresh) = (Some(true), Some(serial));
        let visit = self
            .workspace
            .native_mutation_visit(self.resident.clone(), request)
            .unwrap();
        let outcome = visit.perform(overlay);
        let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
            panic!("CREATE was not published: {outcome:?}")
        };
        overlay.reply_tickets().attempted(publication).unwrap();
        let file = outcome.file.expect("the created file is open");
        overlay
            .close_native_file(self.mount, serial, file.owner_id())
            .unwrap();
    }
    /// One file created, written, closed, removed and forgotten. With
    /// `early` the owner's maintenance runs between UNLINK and FORGET, as it
    /// does behind a serial client; otherwise FORGET arrives first.
    fn removed(&self, child: &str, early: bool) -> Removed {
        let overlay = &self.b.overlay;
        let mut jobs: [Cost; 6] = Default::default();
        let mut maintenance = DatabaseWork::default();
        let (mut steps, mut rows) = (0, 0);

        let facts = Arc::new(VisitFacts::default());
        let visit = self
            .workspace
            .native_read_visit(
                self.resident.clone(),
                self.mount,
                1,
                None,
                NativeReadOperation::Lookup {
                    parent: 1,
                    name: name(child),
                },
                facts.clone(),
            )
            .unwrap();
        let (outcome, work) = measured(overlay, || visit.perform(overlay));
        assert!(matches!(
            outcome.decision,
            Some(NativeReadDecision::Refused(Refusal::Missing))
        ));
        jobs[0] = cost(&work);

        let serial = self.workspace.next_serial(&self.b.allocator).unwrap();
        let mut request = self.mutation(1, None, NativeInput::Named(create(1, child)));
        (request.open, request.fresh, request.facts) = (Some(true), Some(serial), facts);
        let visit = self
            .workspace
            .native_mutation_visit(self.resident.clone(), request)
            .unwrap();
        let (outcome, work) = measured(overlay, || visit.perform(overlay));
        let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
            panic!("CREATE was not published: {outcome:?}")
        };
        let file: OpenFile = outcome.file.expect("the created file is open");
        jobs[1] = cost(&work);
        overlay.reply_tickets().attempted(publication).unwrap();

        let write = NativeInput::Write {
            offset: 0,
            data: b"1\n".as_slice().into(),
            cached: false,
        };
        let visit = self
            .workspace
            .native_mutation_visit(
                self.resident.clone(),
                self.mutation(serial, Some(file.owner_id()), write),
            )
            .unwrap();
        let (outcome, work) = measured(overlay, || visit.perform(overlay));
        let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
            panic!("WRITE was not published: {outcome:?}")
        };
        jobs[2] = cost(&work);
        overlay.reply_tickets().attempted(publication).unwrap();

        let ((), work) = measured(overlay, || {
            overlay
                .close_native_file(self.mount, serial, file.owner_id())
                .unwrap()
        });
        jobs[3] = cost(&work);
        // A live file with a kernel reference leaves nothing to maintain.
        assert_eq!(self.drain().0, 0);

        let visit = self
            .workspace
            .native_mutation_visit(
                self.resident.clone(),
                self.mutation(1, None, NativeInput::Named(unlink(1, child))),
            )
            .unwrap();
        let (outcome, work) = measured(overlay, || visit.perform(overlay));
        let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
            panic!("UNLINK was not published: {outcome:?}")
        };
        jobs[4] = cost(&work);
        overlay.reply_tickets().attempted(publication).unwrap();
        if early {
            let (s, r, work) = self.drain();
            (steps, rows) = (steps + s, rows + r);
            maintenance.accumulate(work);
        }

        let ((), work) = measured(overlay, || {
            overlay.forget_native(self.mount, serial, 1).unwrap()
        });
        jobs[5] = cost(&work);
        assert_eq!(
            overlay.native_lookup_count(self.mount, serial).unwrap(),
            None
        );
        let (s, r, work) = self.drain();
        (steps, rows) = (steps + s, rows + r);
        maintenance.accumulate(work);
        // The orphan is gone from this engine.
        assert_eq!(overlay.resources(None).unwrap().counts.orphan_rows, 0);

        let inode = jobs
            .iter()
            .flatten()
            .filter(|(family, ..)| *family == "Inode")
            .map(|(_, attempts, _)| attempts)
            .sum();
        Removed {
            jobs,
            steps,
            rows,
            maintenance: cost(&maintenance),
            inode,
        }
    }
}

#[test]
fn one_removed_file_costs_exactly_this_beside_any_files_and_after_any_orphans() {
    const FIRST: u64 = 25;
    const LAST: u64 = 4 * FIRST;
    let b = Bench::new("native-unlink-cost");
    VisitFacts::default()
        .supply(
            &b.workspace.base().unwrap(),
            &[Need::Inode(1), Need::Name(1, name("f0"))],
            None,
        )
        .unwrap();
    let resident = Arc::new(CanonicalClient::resident(b.cache.clone()));
    let demand = b.demand();
    let one = Mounted {
        b: &b,
        workspace: &b.workspace,
        resident: resident.clone(),
        mount: b.overlay.create_native_mount(b.route(), 1).unwrap(),
        requests: Cell::new(0),
    };

    // The directory gets its local row; no orphan has been made yet.
    one.keep("k0");
    let fresh = one.removed("f0", false);
    println!("NATIVE_UNLINK_COST fresh {fresh:?}");
    let mut sized = Vec::new();
    for index in 1..=LAST {
        one.keep(&format!("k{index}"));
        if index == FIRST || index == LAST {
            let late = one.removed(&format!("f{index}"), false);
            let early = one.removed(&format!("e{index}"), true);
            println!("NATIVE_UNLINK_COST files={index} forget-first {late:?}");
            println!("NATIVE_UNLINK_COST files={index} maintenance-first {early:?}");
            sized.push((late, early));
        }
    }
    // A second Workspace of the same engine, mounted after those orphans.
    let client = Arc::new(CanonicalClient::with_cache(
        Arc::new(b.fixture.store.clone()),
        Some(Arc::new(b.fixture.store.clone())),
        b.cache.clone(),
    ));
    let second = Workspace::open(
        &b.overlay,
        client,
        b.fixture.root,
        b.fixture.scope,
        [43; 32],
    )
    .unwrap();
    let two = Mounted {
        b: &b,
        workspace: &second,
        resident,
        mount: b.overlay.create_native_mount(second.route(), 1).unwrap(),
        requests: Cell::new(0),
    };
    two.keep("k0");
    let later = two.removed("f0", false);
    println!("NATIVE_UNLINK_COST second Workspace {later:?}");
    assert_eq!(b.demand(), demand, "an owner visit asked the provider");

    // Four times as many kept files beside it: nothing changes.
    assert_eq!(sized[0], sized[1]);
    let (late, early) = &sized[0];
    let expected = [
        LOOKUP.to_vec(),
        CREATE.to_vec(),
        WRITE.to_vec(),
        RELEASE.to_vec(),
        UNLINK.to_vec(),
        FORGET.to_vec(),
    ];
    assert_eq!(fresh.jobs, expected);
    assert_eq!(fresh.inode, INODE);
    let turns = |removed: &Removed| (removed.steps, removed.rows, removed.maintenance.clone());
    assert_eq!(
        turns(&fresh),
        (FORGET_FIRST.0, FORGET_FIRST.1, FORGET_FIRST.2.to_vec())
    );
    assert_eq!(
        turns(early),
        (
            MAINTENANCE_FIRST.0,
            MAINTENANCE_FIRST.1,
            MAINTENANCE_FIRST.2.to_vec()
        )
    );
    // Inode reads probe the orphan domain only while the engine holds an
    // orphan. Each of these orphans was reclaimed, so the files after them
    // and a Workspace mounted after them pay what the fresh engine paid.
    assert_eq!(late, &fresh);
    assert_eq!(early.jobs, fresh.jobs);
    assert_eq!(later, fresh);

    b.overlay.revoke_native_mount(one.mount).unwrap();
    b.overlay.revoke_native_mount(two.mount).unwrap();
}
/// The six jobs' Inode-family attempts for one created and removed file.
const INODE: u64 = 21;
/// The negative LOOKUP: the fence; the directory's row and one seek of the
/// name's rows, for each of its two evaluations.
const LOOKUP: [(&str, u64, u64); 3] = [
    ("Workspace", 1, 1),
    ("Inode", 2, 2),
    ("DirectoryEntry", 2, 2),
];
/// CREATE, WRITE and RELEASE as `native_visit_cost` explains them.
const CREATE: [(&str, u64, u64); 7] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Inode", 8, 9),
    ("DirectoryEntry", 5, 6),
    ("Lease", 6, 12),
];
const WRITE: [(&str, u64, u64); 6] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Inode", 3, 3),
    ("Payload", 2, 3),
];
const RELEASE: [(&str, u64, u64); 5] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 1, 1),
    ("Lease", 3, 7),
];
/// UNLINK under the kernel's lookup reference, in order. Reads: the fence
/// [Workspace]; the directory's row, the name's rows and the file's row,
/// then the directory's row and the name's rows again for the change
/// [3 Inode, 2 DirectoryEntry]. Then one transaction: the file's active
/// layer row and its tombstone, the directory's and its update [4 Inode,
/// the tombstone with its count trigger]; the name's rows and the binding's
/// removal [2 DirectoryEntry, with its trigger]; the orphan's row, absent
/// [Lease]; the retirement of the file's active layer, queued [Reclaim,
/// with its trigger]; the file's references [Lease]; the orphan's row
/// [Lease, with its trigger] and its orphan-domain inode row [Inode]; the
/// orphan's own item, queued [Reclaim, with its trigger]; the frontier
/// [Workspace].
const UNLINK: [(&str, u64, u64); 8] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Inode", 8, 9),
    ("DirectoryEntry", 4, 5),
    ("Lease", 3, 4),
    ("Reclaim", 2, 4),
];
/// FORGET of the last kernel reference, in order: the Workspace's row and
/// the mount's [Workspace, Lease]; the lookup's row, its deletion and its
/// owner's [3 Lease, each deletion with its trigger]; the file reference
/// dropped by a statement that returns what remains [Lease]; the orphan's
/// item queued again, which conflicts, and made ready [2 Reclaim, the
/// second with its trigger]; the Workspace's row for a pending close
/// [Workspace].
const FORGET: [(&str, u64, u64); 6] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Lease", 5, 7),
    ("Reclaim", 2, 3),
];
/// Maintenance when FORGET arrives before the owner's first step: steps,
/// rows, and the SQL of every turn. The orphan releases its lower layer;
/// that layer's retirement drops the file's cell; the orphan finds no cell
/// of its own, then deletes its inode row, its row and the custody row and
/// reads the engine's count of orphan rows.
const FORGET_FIRST: (u64, u64, [(&str, u64, u64); 7]) = (
    4,
    5,
    [
        ("Startup", 4, 4),
        ("Begin", 4, 4),
        ("Commit", 4, 4),
        ("Workspace", 2, 2),
        ("Inode", 2, 2),
        ("Lease", 11, 13),
        ("Reclaim", 20, 26),
    ],
);
/// Maintenance when the owner runs between UNLINK and FORGET, as it does
/// behind a serial client: the cell is first moved into the orphan domain
/// and its lower layer retired, then deleted again after FORGET.
const MAINTENANCE_FIRST: (u64, u64, [(&str, u64, u64); 8]) = (
    7,
    7,
    [
        ("Startup", 7, 7),
        ("Begin", 7, 7),
        ("Commit", 7, 7),
        ("Workspace", 6, 6),
        ("Inode", 6, 6),
        ("Payload", 3, 4),
        ("Lease", 19, 21),
        ("Reclaim", 33, 43),
    ],
);

//! The exact SQL one created and removed file costs a mount: CREATE with an
//! open descriptor, a two-byte WRITE, RELEASE, UNLINK under the kernel's
//! lookup reference, FORGET of that reference, and the maintenance steps the
//! removal leaves behind. The jobs run through the public visit API over a
//! real canonical base and Overlay, beside N and 4N kept files, in an engine
//! that has never made an orphan, in the same engine after its orphans were
//! reclaimed, and in a second Workspace opened on it later. The job that
//! drops the last reference reclaims the file itself when that fits one
//! maintenance step's page; a larger file and a file removed while open are
//! pinned beside it. Reclamation of a closed namespace is not measured here.
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
/// A removed file that is larger than one step's page, or open at its UNLINK.
#[derive(Debug, Eq, PartialEq)]
struct Special {
    /// Whether maintenance was pending after UNLINK, then the steps and rows
    /// the owner ran before the next request.
    unlinked: (bool, u64, u64),
    /// The file's payload cells before and after its FORGET.
    cells: (u64, u64),
    forget: Cost,
    /// Whether maintenance was pending after FORGET, then its steps and rows.
    forgotten: (bool, u64, u64),
}
/// The last reference of a file that keeps its name.
#[derive(Debug, Eq, PartialEq)]
struct Live {
    job: Cost,
    /// Whether maintenance was pending afterwards, then its steps and rows.
    after: (bool, u64, u64),
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
    /// A created file, open, with its name and its kernel reference.
    fn opened(&self, child: &str) -> (u64, OpenFile) {
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
        (serial, outcome.file.expect("the created file is open"))
    }
    /// A file that keeps its name and its kernel reference.
    fn keep(&self, child: &str) -> u64 {
        let (serial, file) = self.opened(child);
        self.b
            .overlay
            .close_native_file(self.mount, serial, file.owner_id())
            .unwrap();
        serial
    }
    /// The last reference of a file that keeps its name: its FORGET after
    /// its RELEASE, or its RELEASE after its FORGET.
    fn live(&self, child: &str, forget_last: bool) -> Live {
        let overlay = &self.b.overlay;
        let queued = overlay.resources(None).unwrap().counts.maintenance_targets;
        let (serial, file) = self.opened(child);
        let close = || {
            overlay
                .close_native_file(self.mount, serial, file.owner_id())
                .unwrap()
        };
        let forget = || overlay.forget_native(self.mount, serial, 1).unwrap();
        let ((), work) = if forget_last {
            close();
            measured(overlay, forget)
        } else {
            forget();
            measured(overlay, close)
        };
        let pending = overlay.maintenance_pending();
        let (steps, rows, _) = self.drain();
        // Nothing of the file is left queued.
        assert_eq!(
            overlay.resources(None).unwrap().counts.maintenance_targets,
            queued
        );
        Live {
            job: cost(&work),
            after: (pending, steps, rows),
        }
    }
    /// A file removed under the kernel's lookup reference and not forgotten.
    fn parked(&self, child: &str) -> u64 {
        let overlay = &self.b.overlay;
        let serial = self.keep(child);
        let visit = self
            .workspace
            .native_mutation_visit(
                self.resident.clone(),
                self.mutation(1, None, NativeInput::Named(unlink(1, child))),
            )
            .unwrap();
        let outcome = visit.perform(overlay);
        let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
            panic!("UNLINK was not published: {outcome:?}")
        };
        overlay.reply_tickets().attempted(publication).unwrap();
        serial
    }
    /// One file of `cells` payload cells, removed under the kernel's lookup
    /// reference. With `open` its descriptor is released after the UNLINK,
    /// once the owner has run; FORGET is the last reference either way.
    fn special(&self, child: &str, cells: usize, open: bool) -> Special {
        let overlay = &self.b.overlay;
        let stored = || overlay.resources(None).unwrap().counts.payload_cells;
        let base = stored();
        let orphans = overlay.resources(None).unwrap().counts.orphan_rows;
        let queued = overlay.resources(None).unwrap().counts.maintenance_targets;
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
        let file: OpenFile = outcome.file.expect("the created file is open");
        for cell in 0..cells as u64 {
            let write = NativeInput::Write {
                offset: cell * 4096,
                data: vec![7; 4096].into(),
                cached: false,
            };
            let visit = self
                .workspace
                .native_mutation_visit(
                    self.resident.clone(),
                    self.mutation(serial, Some(file.owner_id()), write),
                )
                .unwrap();
            let outcome = visit.perform(overlay);
            let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
                panic!("WRITE was not published: {outcome:?}")
            };
            overlay.reply_tickets().attempted(publication).unwrap();
        }
        let close = || {
            overlay
                .close_native_file(self.mount, serial, file.owner_id())
                .unwrap()
        };
        if !open {
            close();
        }
        assert_eq!(self.drain().0, 0);
        let visit = self
            .workspace
            .native_mutation_visit(
                self.resident.clone(),
                self.mutation(1, None, NativeInput::Named(unlink(1, child))),
            )
            .unwrap();
        let outcome = visit.perform(overlay);
        let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
            panic!("UNLINK was not published: {outcome:?}")
        };
        overlay.reply_tickets().attempted(publication).unwrap();
        let pending = overlay.maintenance_pending();
        let (steps, rows, _) = self.drain();
        let unlinked = (pending, steps, rows);
        if open {
            // The kernel's lookup reference still holds the file.
            close();
            assert_eq!((stored() - base, self.drain().0), (cells as u64, 0));
        }
        let before = stored() - base;
        let ((), work) = measured(overlay, || {
            overlay.forget_native(self.mount, serial, 1).unwrap()
        });
        let cells = (before, stored() - base);
        let pending = overlay.maintenance_pending();
        let (steps, rows, _) = self.drain();
        let counts = overlay.resources(None).unwrap().counts;
        assert_eq!((counts.orphan_rows, counts.payload_cells), (orphans, base));
        // No item of the file is left, parked or ready.
        assert_eq!(counts.maintenance_targets, queued);
        Special {
            unlinked,
            cells,
            forget: cost(&work),
            forgotten: (pending, steps, rows),
        }
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
        // Under the kernel's lookup alone the removal queues nothing.
        assert!(!overlay.maintenance_pending());
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
        // The FORGET reclaimed the file and left the owner nothing to do.
        assert!(!overlay.maintenance_pending());
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
    // The last reference of a file that keeps its name, in an engine with
    // no orphan: its FORGET, then another file's RELEASE.
    let live = [one.live("l0", true), one.live("l1", false)];
    println!("NATIVE_UNLINK_COST live file, FORGET last {:?}", live[0]);
    println!("NATIVE_UNLINK_COST live file, RELEASE last {:?}", live[1]);
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
    // The second Workspace keeps an orphan under its lookup while the first
    // Workspace's releases reclaim: its rows are another namespace's.
    let parked = two.parked("p");
    let orphans = || b.overlay.resources(None).unwrap().counts.orphan_rows;
    assert_eq!((orphans(), b.overlay.maintenance_pending()), (1, false));
    // While the engine holds that orphan, a live file's last reference
    // probes for its own orphan row and nothing more.
    let beside = [one.live("l2", true), one.live("l3", false)];
    println!("NATIVE_UNLINK_COST live file beside an orphan {beside:?}");
    let large = one.special("large", 4 * 14, false);
    let open = one.special("open", 1, true);
    println!("NATIVE_UNLINK_COST four pages {large:?}");
    println!("NATIVE_UNLINK_COST open at unlink {open:?}");
    assert_eq!(orphans(), 1);
    b.overlay.forget_native(two.mount, parked, 1).unwrap();
    assert_eq!((orphans(), b.overlay.maintenance_pending()), (0, false));

    // A live file's last reference deletes its custody row itself and
    // leaves the owner nothing.
    let done = |job: &[(&'static str, u64, u64)]| Live {
        job: job.to_vec(),
        after: (false, 0, 0),
    };
    assert_eq!(live, [done(&LIVE_FORGET), done(&LIVE_RELEASE)]);
    let probed = |live: &Live| Live {
        job: (live.job.iter())
            .map(|&(family, attempts, executions)| match family {
                "Lease" => (family, attempts + 1, executions + 1),
                _ => (family, attempts, executions),
            })
            .collect(),
        after: live.after,
    };
    assert_eq!(beside, [probed(&live[0]), probed(&live[1])]);
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
    // Neither order leaves the owner a step: the FORGET did the work.
    let turns = |removed: &Removed| (removed.steps, removed.rows, removed.maintenance.clone());
    assert_eq!(turns(&fresh), (0, 0, Vec::new()));
    assert_eq!(turns(early), (0, 0, Vec::new()));
    // Four pages: the FORGET drops one page, the queue the other three.
    assert_eq!(
        large,
        Special {
            unlinked: (false, 0, 0),
            cells: (56, 42),
            forget: FORGET_PAGE.to_vec(),
            forgotten: (true, 5, 46),
        }
    );
    // Open at its UNLINK: the owner migrates the cell as before, then
    // parks the orphan; the FORGET after RELEASE drops it.
    assert_eq!(
        open,
        Special {
            unlinked: (true, 4, 3),
            cells: (1, 0),
            forget: FORGET_MIGRATED.to_vec(),
            forgotten: (false, 0, 0),
        }
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
/// The six jobs' Inode-family attempts for one created and removed file:
/// 11 of the requests' own, and the two of the reclamation in FORGET.
const INODE: u64 = 13;
/// The negative LOOKUP: the fence; the directory's row, read once, and one
/// seek of the name's rows for each of its two evaluations.
const LOOKUP: [(&str, u64, u64); 3] = [
    ("Workspace", 1, 1),
    ("Inode", 1, 1),
    ("DirectoryEntry", 2, 2),
];
/// CREATE, WRITE and RELEASE as `native_visit_cost` explains them.
const CREATE: [(&str, u64, u64); 7] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Inode", 3, 4),
    ("DirectoryEntry", 5, 6),
    ("Lease", 3, 6),
];
const WRITE: [(&str, u64, u64); 6] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Inode", 2, 2),
    ("Payload", 2, 3),
];
const RELEASE: [(&str, u64, u64); 5] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 1, 1),
    ("Lease", 2, 3),
];
/// UNLINK under the kernel's lookup reference, in order. Reads: the fence
/// [Workspace]; the directory's row, the name's rows and the file's row,
/// then the name's rows again for the change [2 Inode, 2 DirectoryEntry].
/// Then one transaction: the file's tombstone and the directory's time and
/// entry counts, each an update over the row the job read [2 Inode]; the
/// name's rows and the binding's removal [2 DirectoryEntry, with its
/// trigger]; the orphan's row, absent [Lease]; the file's references
/// [Lease]; the orphan's row [Lease, with its trigger] and its
/// orphan-domain inode row [Inode, with its count trigger]; the frontier
/// [Workspace]. Nothing is queued: only the kernel's lookup holds the file.
const UNLINK: [(&str, u64, u64); 7] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Inode", 5, 6),
    ("DirectoryEntry", 4, 5),
    ("Lease", 3, 4),
];
/// FORGET of the last kernel reference, which reclaims the one-cell file
/// in its own job and writes no queue item. Its own part: the Workspace's
/// row, kept for the pending-close decision, and the mount's [Workspace,
/// Lease]; the lookup's row and its deletion [2 Lease, the deletion with
/// its trigger]; the file reference dropped by a statement
/// that returns what remains [Lease]; the orphan's row [Lease]. Then the
/// four steps the owner ran, without their items. The orphan releases its
/// lower layer: that layer's row, the wait row's deletion, the orphan's
/// update [Inode, 2 Lease]. The layer's retirement: its generation's
/// holders, its cells and the cell's drop [Lease, 2 Reclaim]. The orphan
/// ends: its own cells and shrink rows, none; its inode row, its row, the
/// engine's count of orphan rows and the custody row [5 Reclaim, Lease];
/// the item an earlier descriptor or reader may have queued, deleted
/// [Reclaim]. The retirement ends: the holders again, cells and shrink
/// rows, none, the Workspace's route and row and the file's rows, where the
/// tombstone stays [Lease, 2 Reclaim, 2 Workspace, Inode].
const FORGET: [(&str, u64, u64); 7] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 3, 3),
    ("Inode", 2, 2),
    ("Lease", 10, 13),
    ("Reclaim", 10, 13),
];
/// FORGET of a file of four pages: the orphan releases its lower layer and
/// that layer's retirement drops one page of 14 cells; then the orphan's
/// item and the layer's are queued, ready [4 Reclaim].
const FORGET_PAGE: [(&str, u64, u64); 7] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 1, 1),
    ("Inode", 1, 1),
    ("Lease", 8, 9),
    ("Reclaim", 19, 37),
];
/// FORGET of a file whose cell the owner had migrated while it was open:
/// the orphan drops that cell, then deletes its rows and its parked item.
const FORGET_MIGRATED: [(&str, u64, u64); 6] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 1, 1),
    ("Lease", 6, 9),
    ("Reclaim", 8, 12),
];
/// FORGET as the last reference of a file that keeps its name: the
/// Workspace's row and the mount's [Workspace, Lease]; the lookup's row and
/// its deletion [2 Lease]; the file reference dropped by a statement that
/// returns what remains [Lease]; the custody row, deleted [Lease, with its
/// trigger]. Nothing is queued.
const LIVE_FORGET: [(&str, u64, u64); 5] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 1, 1),
    ("Lease", 5, 8),
];
/// RELEASE as the last reference of such a file: the fence [Workspace]; the
/// descriptor's row and the file reference [2 Lease]; the custody row,
/// deleted [Lease, with its trigger]. Nothing is queued.
const LIVE_RELEASE: [(&str, u64, u64); 5] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 1, 1),
    ("Lease", 3, 6),
];

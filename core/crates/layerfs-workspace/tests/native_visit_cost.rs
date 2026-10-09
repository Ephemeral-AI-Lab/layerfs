//! The exact SQL of the five owner jobs one created file costs a mount:
//! GETATTR of the directory, a negative LOOKUP of the name, CREATE with an
//! open descriptor, a two-byte WRITE through it and its RELEASE. The jobs run
//! through the public visit API over a real canonical base and Overlay, in
//! steady state: the directory is the base root with a local row, its
//! objects are resident, and the kernel keeps the created inode's lookup
//! reference after the close.
mod common;
mod harness;
use common::name;
use harness::{create, Bench, T1};
use layerfs_overlay::{DatabaseWork, NativeMount, OpenFile, StatementKind, StatementWork};
use layerfs_workspace::{
    CanonicalClient, JobOutcome, NativeInput, NativeReadDecision, NativeReadOperation,
    NativeVisitRequest, Need, Refusal, VisitFacts,
};
use std::sync::Arc;

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
fn measured<T>(b: &Bench, job: impl FnOnce() -> T) -> (T, DatabaseWork) {
    let before = b.overlay.diagnostics();
    let value = job();
    let work = b.overlay.diagnostics().since(&before);
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
struct Cycle {
    jobs: [Cost; 5],
    total: StatementWork,
}
/// The five jobs of one created file, in the order the kernel sends them.
fn cycle(b: &Bench, resident: &Arc<CanonicalClient>, mount: NativeMount, index: u64) -> Cycle {
    let child = format!("f{index}");
    let facts = Arc::new(VisitFacts::default());
    let mut total = DatabaseWork::default();
    let mut jobs: [Cost; 5] = Default::default();
    let mut record = |slot: usize, work: DatabaseWork| {
        jobs[slot] = cost(&work);
        total.accumulate(work);
    };

    // GETATTR of the directory.
    let visit = b
        .workspace
        .native_read_visit(
            resident.clone(),
            mount,
            1,
            None,
            NativeReadOperation::Getattr { serial: 1 },
            facts.clone(),
        )
        .unwrap();
    let (outcome, work) = measured(b, || visit.perform(&b.overlay));
    assert!(matches!(outcome.result, Ok(None)), "{outcome:?}");
    let Some(NativeReadDecision::Value(root)) = outcome.decision else {
        panic!("GETATTR was not decided: {:?}", outcome.decision)
    };
    assert_eq!(root.serial, 1);
    record(0, work);

    // The name is not bound.
    let visit = b
        .workspace
        .native_read_visit(
            resident.clone(),
            mount,
            1,
            None,
            NativeReadOperation::Lookup {
                parent: 1,
                name: name(&child),
            },
            facts.clone(),
        )
        .unwrap();
    let (outcome, work) = measured(b, || visit.perform(&b.overlay));
    assert!(matches!(outcome.result, Ok(None)), "{outcome:?}");
    assert!(matches!(
        outcome.decision,
        Some(NativeReadDecision::Refused(Refusal::Missing))
    ));
    record(1, work);

    // CREATE with an open descriptor.
    let serial = b.workspace.next_serial(&b.allocator).unwrap();
    let visit = b
        .workspace
        .native_mutation_visit(
            resident.clone(),
            NativeVisitRequest {
                mount,
                request: 3 * index + 1,
                serial: 1,
                handle: None,
                input: NativeInput::Named(create(1, &child)),
                open: Some(true),
                now: T1,
                fresh: Some(serial),
                facts: facts.clone(),
            },
        )
        .unwrap();
    let (outcome, work) = measured(b, || visit.perform(&b.overlay));
    let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
        panic!("CREATE was not published: {outcome:?}")
    };
    let file: OpenFile = outcome.file.expect("the created file is open");
    record(2, work);
    // The reply attempt is recorded from the replying thread, with no job.
    b.overlay.reply_tickets().attempted(publication).unwrap();

    // Two bytes through the descriptor.
    let visit = b
        .workspace
        .native_mutation_visit(
            resident.clone(),
            NativeVisitRequest {
                mount,
                request: 3 * index + 2,
                serial,
                handle: Some(file.owner_id()),
                input: NativeInput::Write {
                    offset: 0,
                    data: b"1\n".as_slice().into(),
                    cached: false,
                },
                open: None,
                now: T1,
                fresh: None,
                facts,
            },
        )
        .unwrap();
    let (outcome, work) = measured(b, || visit.perform(&b.overlay));
    let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
        panic!("WRITE was not published: {outcome:?}")
    };
    record(3, work);
    b.overlay.reply_tickets().attempted(publication).unwrap();

    // The last close; the kernel still holds the inode's lookup reference.
    let ((), work) = measured(b, || {
        b.overlay
            .close_native_file(mount, serial, file.owner_id())
            .unwrap()
    });
    record(4, work);
    assert_eq!(
        b.overlay.native_lookup_count(mount, serial).unwrap(),
        Some(1)
    );
    Cycle {
        jobs,
        total: total.total(),
    }
}

#[test]
fn the_five_jobs_of_one_created_file_cost_exactly_this_at_any_directory_size() {
    const FIRST: u64 = 10;
    const LAST: u64 = 4_000;
    let b = Bench::new("native-visit-cost");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    // The root's canonical objects become resident through the ordinary client.
    VisitFacts::default()
        .supply(
            &b.workspace.base().unwrap(),
            &[Need::Inode(1), Need::Name(1, name("f0"))],
            None,
        )
        .unwrap();
    let resident = Arc::new(CanonicalClient::resident(b.cache.clone()));
    let demand = b.demand();

    let mut first = None;
    let mut last = None;
    for index in 1..=LAST {
        let measured = cycle(&b, &resident, mount, index);
        if index == FIRST {
            first = Some(measured);
        } else if index == LAST {
            last = Some(measured);
        }
    }
    assert_eq!(b.demand(), demand, "an owner visit asked the provider");
    let (first, last) = (first.unwrap(), last.unwrap());
    println!(
        "NATIVE_VISIT_COST file={FIRST} jobs={:?} total={:?}",
        first.jobs, first.total
    );
    println!(
        "NATIVE_VISIT_COST file={LAST} jobs={:?} total={:?}",
        last.jobs, last.total
    );

    // One fence, then the directory's local row.
    let getattr: Cost = vec![("Workspace", 1, 1), ("Inode", 1, 1)];
    // The fence; then the directory's row, and one seek of the name's rows
    // twice: the first evaluation needs the base's answer for the name, takes
    // it from resident objects, and the second evaluation reads the name's
    // rows again. The job keeps the directory's row it read.
    let lookup: Cost = vec![
        ("Workspace", 1, 1),
        ("Inode", 1, 1),
        ("DirectoryEntry", 2, 2),
    ];
    let expected = [
        getattr,
        lookup,
        CREATE.to_vec(),
        WRITE.to_vec(),
        RELEASE.to_vec(),
    ];
    for (job, (seen, wanted)) in first.jobs.iter().zip(&expected).enumerate() {
        assert_eq!(seen, wanted, "job {job} of file {FIRST}");
    }
    // The directory holds 400 times as many names: nothing changes.
    assert_eq!(last.jobs, first.jobs);
    for (label, a, z) in [
        ("attempts", first.total.attempts, last.total.attempts),
        ("executions", first.total.executions, last.total.executions),
        (
            "rows changed",
            first.total.rows_changed,
            last.total.rows_changed,
        ),
        (
            "rows returned",
            first.total.rows_returned,
            last.total.rows_returned,
        ),
    ] {
        assert_eq!(a, z, "{label} of file {FIRST} and of file {LAST}");
    }
    assert_eq!(
        (first.total.attempts, first.total.executions),
        TOTAL,
        "{:?}",
        first.jobs
    );
    b.overlay.revoke_native_mount(mount).unwrap();
}
/// CREATE, in order. Reads: the fence [Workspace]; the directory's row, once
/// for the job [Inode]; the name's rows for the first evaluation, again for
/// the second and again for the published binding's inheritance [3
/// DirectoryEntry]. Then one transaction, begun by its admission pragma
/// [Startup] and BEGIN: the new inode, inserted without a read of its
/// reserved serial [Inode, with its count trigger]; the directory's time and
/// entry counts, updated over the row the job read [Inode]; the name's rows
/// and its binding [2 DirectoryEntry, the binding with its trigger], the
/// frontier [Workspace], and the reply's kernel custody [Lease]: the lookup
/// row, the descriptor row, which names its mount and request, and one
/// custody row for both references, each with its trigger. COMMIT.
const CREATE: [(&str, u64, u64); 7] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Inode", 3, 4),
    ("DirectoryEntry", 5, 6),
    ("Lease", 3, 6),
];
/// WRITE, in order. Reads: the fence with the descriptor [Workspace]; the
/// file's row, which carries its layer columns [Inode]. Then one
/// transaction: the inode's new size and time, updated over that row
/// [Inode], the one partly covered cell read and written [2 Payload, the
/// write with its trigger] and the frontier [Workspace].
const WRITE: [(&str, u64, u64); 6] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 2, 2),
    ("Inode", 2, 2),
    ("Payload", 2, 3),
];
/// RELEASE, in order. The fence with the descriptor [Workspace]; then one
/// transaction [Lease]: the descriptor row is deleted (with its trigger)
/// and the file reference is dropped by a statement that returns the
/// references that remain.
const RELEASE: [(&str, u64, u64); 5] = [
    ("Startup", 1, 1),
    ("Begin", 1, 1),
    ("Commit", 1, 1),
    ("Workspace", 1, 1),
    ("Lease", 2, 3),
];
/// Attempts and executions of the whole cycle: 28 statements of the five
/// jobs and the admission pragma, BEGIN and COMMIT of three transactions.
const TOTAL: (u64, u64) = (37, 44);

//! Native requests served by owner visits that record no request source,
//! over a real canonical namespace and Overlay. A visit decides from current
//! rows, from the request's facts of the Workspace's present base, and from
//! canonical objects already in memory; it never reaches the provider.
mod common;
mod harness;
use common::name;
use harness::{create, mkdir, Bench, T1, T2};
use layerfs_content::{
    filesystem::{
        update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects,
        FilesystemResources, PathName,
    },
    AuthenticatedObjects, ContentError, ObjectId,
};
use layerfs_overlay::{
    Inode, InodeKind, NativeMount, OpenFile, OverlayError, StatementKind, StoredCounts,
    WorkspaceState,
};
use layerfs_workspace::{
    CanonicalCache, CanonicalClient, FileLengths, JobOutcome, NativeInput, NativeMutationOutcome,
    NativeReadDecision, NativeReadOperation, NativeVisitRequest, Need, Operation, Refusal, Time,
    VisitFacts, WorkspaceError,
};
use std::sync::Arc;

const CACHE: usize = 4 * 1024 * 1024;
/// A memory-only client over a cache that holds nothing.
fn empty() -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::resident(Arc::new(CanonicalCache::new(
        CACHE,
    ))))
}
/// The memory-only client of the cache the Workspace's own client fills.
fn resident(b: &Bench) -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::resident(b.cache.clone()))
}
fn lookup(parent: u64, child: &str) -> NativeReadOperation {
    NativeReadOperation::Lookup {
        parent,
        name: name(child),
    }
}
/// Everything a visit could leave behind: the Workspace row, the ownership
/// rows and the transactions begun so far.
fn snapshot(b: &Bench) -> (WorkspaceState, StoredCounts, u64) {
    (
        b.overlay.state(b.route()).unwrap(),
        b.overlay.resources(Some(b.route())).unwrap().counts,
        b.overlay.diagnostics().statements[StatementKind::Begin as usize].executions,
    )
}
#[derive(Debug, Eq, PartialEq)]
enum Seen {
    Needs(Vec<Need>),
    Value(Inode),
    Refused(Refusal),
}
/// One read visit as one owner job. The job itself asks the provider nothing.
fn observe(
    b: &Bench,
    client: Arc<CanonicalClient>,
    mount: NativeMount,
    serial: u64,
    operation: NativeReadOperation,
    facts: &VisitFacts,
) -> Seen {
    let visit = b
        .workspace
        .native_read_visit(
            client,
            mount,
            serial,
            None,
            operation,
            Arc::new(facts.clone()),
        )
        .unwrap();
    assert_eq!(visit.mount(), mount);
    let demand = b.demand();
    let outcome = visit.perform(&b.overlay);
    assert_eq!(b.demand(), demand, "an owner visit asked the provider");
    assert!(matches!(outcome.result, Ok(None)), "{:?}", outcome.result);
    assert!(outcome.open_candidate.is_none());
    assert!(outcome.directory_candidate.is_none());
    match outcome.decision {
        Some(NativeReadDecision::Needs(needs)) => Seen::Needs(needs),
        Some(NativeReadDecision::Value(inode)) => Seen::Value(inode),
        Some(NativeReadDecision::Refused(refusal)) => Seen::Refused(refusal),
        other => panic!("undecided read visit: {other:?}"),
    }
}
/// One OPEN visit as one owner job: what it decided and the descriptor it
/// recorded. The job itself asks the provider nothing.
fn open(
    b: &Bench,
    client: Arc<CanonicalClient>,
    mount: NativeMount,
    request: u64,
    serial: u64,
    writable: bool,
    facts: &VisitFacts,
) -> (Seen, Option<OpenFile>) {
    let visit = b
        .workspace
        .native_open_visit(
            client,
            mount,
            request,
            serial,
            writable,
            Arc::new(facts.clone()),
        )
        .unwrap();
    let demand = b.demand();
    let outcome = visit.perform(&b.overlay);
    assert_eq!(b.demand(), demand, "an owner visit asked the provider");
    assert!(matches!(outcome.result, Ok(None)), "{:?}", outcome.result);
    assert!(outcome.directory_candidate.is_none());
    let seen = match outcome.decision {
        Some(NativeReadDecision::Needs(needs)) => Seen::Needs(needs),
        Some(NativeReadDecision::Value(inode)) => Seen::Value(inode),
        Some(NativeReadDecision::Refused(refusal)) => Seen::Refused(refusal),
        other => panic!("undecided open visit: {other:?}"),
    };
    assert_eq!(
        outcome.open_candidate.is_some(),
        matches!(seen, Seen::Value(_)),
        "a descriptor exactly when the visit decided a file"
    );
    (seen, outcome.open_candidate)
}
/// LOOKUP of a root name until it is decided, reading what each undecided
/// visit asks for: the kernel's reference on the found inode.
fn reference(b: &Bench, mount: NativeMount, child: &str) -> Inode {
    let mut facts = VisitFacts::default();
    for _ in 0..4 {
        match observe(b, empty(), mount, 1, lookup(1, child), &facts) {
            Seen::Value(inode) => return inode,
            Seen::Needs(needs) => facts
                .supply(&b.workspace.base().unwrap(), &needs, None)
                .unwrap(),
            Seen::Refused(refusal) => panic!("lookup {child}: {refusal:?}"),
        }
    }
    panic!("lookup {child} was not decided in four visits")
}
fn named(
    mount: NativeMount,
    request: u64,
    operation: Operation,
    fresh: Option<u64>,
    now: Time,
    facts: &VisitFacts,
) -> NativeVisitRequest {
    NativeVisitRequest {
        mount,
        request,
        serial: 1,
        handle: None,
        input: NativeInput::Named(operation),
        open: None,
        now,
        fresh,
        facts: Arc::new(facts.clone()),
    }
}
/// One mutation visit as one owner job. The job itself asks the provider
/// nothing.
fn mutate(
    b: &Bench,
    client: Arc<CanonicalClient>,
    request: NativeVisitRequest,
) -> NativeMutationOutcome {
    let mount = request.mount;
    let visit = b.workspace.native_mutation_visit(client, request).unwrap();
    assert_eq!(visit.mount(), mount);
    let demand = b.demand();
    let outcome = visit.perform(&b.overlay);
    assert_eq!(b.demand(), demand, "an owner visit asked the provider");
    outcome
}
/// Installs the present base with `changes` to its root directory as the
/// next base, through the actual canonical builder and install port.
fn install(b: &Bench, changes: Vec<(PathName, Option<u64>)>) {
    let store = &b.fixture.store;
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let updates = [DirectoryUpdate { parent: 1, changes }];
    let next = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(b.workspace.base().unwrap().identity()),
            scope: b.fixture.scope,
            root_serial: 1,
            directories: &updates,
            inodes: &[],
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let capture = b.overlay.capture(b.route()).unwrap();
    let prepared = b.workspace.prepare_base_install(capture, next).unwrap();
    b.workspace
        .install_prepared_base(&b.overlay, prepared)
        .unwrap();
}

#[test]
fn a_read_visit_decides_in_one_job_from_resident_objects_or_names_the_facts_it_needs() {
    let b = Bench::new("native-visit-read");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let base = b.workspace.base().unwrap();
    let wanted = vec![Need::Inode(1), Need::Name(1, name(".git"))];

    // Nothing resident: the visit is undecided and has changed nothing.
    let before = snapshot(&b);
    assert_eq!(
        observe(
            &b,
            empty(),
            mount,
            1,
            lookup(1, ".git"),
            &VisitFacts::default()
        ),
        Seen::Needs(wanted.clone())
    );
    assert_eq!(snapshot(&b), before);
    assert_eq!(b.overlay.native_lookup_count(mount, 4).unwrap(), None);

    // The facts are read outside the owner, from the Workspace's base.
    let mut facts = VisitFacts::default();
    assert_eq!(facts.charge(), 0);
    let demand = b.demand();
    facts.supply(&base, &wanted, None).unwrap();
    assert!(b.demand() > demand);
    assert!(facts.charge() > 0);

    // The next visit decides from them, with nothing resident, and takes the
    // answer's kernel reference in its one transaction.
    let Seen::Value(found) = observe(&b, empty(), mount, 1, lookup(1, ".git"), &facts) else {
        panic!("the supplied facts did not decide the lookup")
    };
    assert_eq!((found.serial, found.kind), (4, InodeKind::Directory));
    assert_eq!(b.overlay.native_lookup_count(mount, 4).unwrap(), Some(1));
    let after = snapshot(&b);
    assert_eq!(after.2, before.2 + 1);
    assert_eq!((after.0.base_readers, after.1.source_rows), (0, 0));

    // The same objects are now resident: another request decides in its one
    // visit with no facts of its own and no provider demand.
    let work = b.cache.diagnostics().unwrap();
    let Seen::Value(again) = observe(
        &b,
        resident(&b),
        mount,
        1,
        lookup(1, ".git"),
        &VisitFacts::default(),
    ) else {
        panic!("resident objects did not decide the lookup")
    };
    assert_eq!(again, found);
    assert_eq!(b.overlay.native_lookup_count(mount, 4).unwrap(), Some(2));
    let read = b.cache.diagnostics().unwrap();
    assert!(read.cache_hits > work.cache_hits);
    assert_eq!(
        (read.cache_misses, read.upstream_batches),
        (work.cache_misses, work.upstream_batches)
    );
    // Every canonical object of the regular file is made resident too, with
    // its length answered by an explicit port and therefore not remembered:
    // a second such stat asks the object provider nothing.
    base.stat_with_lengths(2, &b.fixture.store).unwrap();
    let demand = b.demand();
    base.stat_with_lengths(2, &b.fixture.store).unwrap();
    assert_eq!(b.demand(), demand);
    assert_eq!(b.cache.diagnostics().unwrap().file_lengths, 0);
    // GETATTR of the inode the kernel now holds, and a negative LOOKUP in the
    // same directory: one visit each, and neither writes.
    let before = snapshot(&b);
    assert_eq!(
        observe(
            &b,
            resident(&b),
            mount,
            4,
            NativeReadOperation::Getattr { serial: 4 },
            &VisitFacts::default(),
        ),
        Seen::Value(found)
    );
    assert_eq!(
        observe(
            &b,
            resident(&b),
            mount,
            1,
            lookup(1, "absent"),
            &VisitFacts::default(),
        ),
        Seen::Refused(Refusal::Missing)
    );
    assert_eq!(snapshot(&b), before);
    // A regular file's length is not a canonical object: the resident client
    // has no length provider, so while no answer is remembered that fact is
    // left to the caller even though every object of the file is resident.
    assert_eq!(
        observe(
            &b,
            resident(&b),
            mount,
            1,
            lookup(1, "file"),
            &VisitFacts::default(),
        ),
        Seen::Needs(vec![Need::Inode(1), Need::Name(1, name("file"))])
    );
    assert_eq!(snapshot(&b), before);
    // The Workspace's own client asks its length provider once and remembers
    // the answer under the file's content root. From then on the visit reads
    // it from memory and decides in its one job, with no provider demand.
    assert_eq!(b.stat(2).unwrap().logical_len, 10);
    assert_eq!(b.cache.diagnostics().unwrap().file_lengths, 1);
    let Seen::Value(file) = observe(
        &b,
        resident(&b),
        mount,
        1,
        lookup(1, "file"),
        &VisitFacts::default(),
    ) else {
        panic!("the remembered length did not decide the lookup")
    };
    assert_eq!(
        (file.serial, file.kind, file.size),
        (2, InodeKind::File, 10)
    );
    assert_eq!(b.overlay.native_lookup_count(mount, 2).unwrap(), Some(1));
    b.overlay.forget_native(mount, 2, 1).unwrap();
    let before = snapshot(&b);

    // This constructor serves LOOKUP and GETATTR alone (OPEN has its own);
    // the kernel's reference is required.
    for operation in [
        NativeReadOperation::Data { serial: 4 },
        NativeReadOperation::Opendir { serial: 4 },
        NativeReadOperation::Open {
            serial: 4,
            writable: false,
        },
    ] {
        assert!(matches!(
            b.workspace.native_read_visit(
                resident(&b),
                mount,
                4,
                None,
                operation,
                Arc::new(VisitFacts::default()),
            ),
            Err(WorkspaceError::Content(ContentError::InvalidRecord(
                "native visit operation"
            )))
        ));
    }
    let unreferenced = b
        .workspace
        .native_read_visit(
            resident(&b),
            mount,
            5,
            None,
            NativeReadOperation::Getattr { serial: 5 },
            Arc::new(VisitFacts::default()),
        )
        .unwrap()
        .perform(&b.overlay);
    assert!(matches!(unreferenced.result, Err(OverlayError::Stale)));
    assert!(unreferenced.decision.is_none());
    assert_eq!(snapshot(&b), before);
    b.overlay.forget_native(mount, 4, 2).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn an_open_visit_decides_a_regular_file_and_records_its_descriptor_alone() {
    let b = Bench::new("native-visit-open");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let base = b.workspace.base().unwrap();
    let file = reference(&b, mount, "file");
    let directory = reference(&b, mount, ".git");
    let link = reference(&b, mount, "symlink");
    assert_eq!(
        (file.serial, file.kind, file.size),
        (2, InodeKind::File, 10)
    );
    assert_eq!((directory.serial, link.serial), (4, 3));
    let none = VisitFacts::default();

    // Nothing resident and no facts: undecided, no descriptor, nothing
    // written.
    let before = snapshot(&b);
    let (Seen::Needs(needs), None) = open(&b, empty(), mount, 30, 2, false, &none) else {
        panic!("a cold OPEN was decided")
    };
    assert!(!needs.is_empty());
    assert_eq!(snapshot(&b).0, before.0);
    assert_eq!(snapshot(&b).1, before.1);
    assert_eq!(b.overlay.retained_native_file(mount, 30).unwrap(), None);

    // With its facts, read outside the owner, the next visit of the same
    // request decides and records the descriptor in its one transaction:
    // its one row, which names the mount and the request, no source, no
    // base reader.
    let mut facts = VisitFacts::default();
    facts.supply(&base, &needs, None).unwrap();
    let (Seen::Value(opened), Some(first)) = open(&b, empty(), mount, 30, 2, false, &facts) else {
        panic!("the supplied facts did not decide the OPEN")
    };
    assert_eq!(opened, file);
    assert_eq!((first.serial(), first.writable()), (2, false));
    assert_eq!(
        b.overlay.retained_native_file(mount, 30).unwrap(),
        Some(first)
    );
    let after = snapshot(&b);
    assert_eq!(after.0, before.0, "the Workspace row is unchanged");
    assert_eq!(
        (
            after.1.owner_rows,
            after.1.owner_details,
            after.1.source_rows,
            after.0.base_readers
        ),
        (before.1.owner_rows, before.1.owner_details + 1, 0, 0)
    );

    // The file's objects and its length are resident now: another OPEN is
    // decided by its one visit with no facts and no provider demand.
    let begun = snapshot(&b).2;
    let (Seen::Value(again), Some(second)) = open(&b, resident(&b), mount, 31, 2, true, &none)
    else {
        panic!("resident facts did not decide the OPEN")
    };
    assert_eq!(again, file);
    assert!(second.writable() && second.owner_id() != first.owner_id());
    assert_eq!(snapshot(&b).2, begun + 1);

    // A directory and a symbolic link are refused by kind and open nothing.
    let held = snapshot(&b);
    assert_eq!(
        open(&b, resident(&b), mount, 32, 4, false, &none),
        (Seen::Refused(Refusal::IsDirectory), None)
    );
    assert_eq!(
        open(&b, resident(&b), mount, 33, 3, false, &none),
        (Seen::Refused(Refusal::Invalid), None)
    );
    assert_eq!((snapshot(&b).0, snapshot(&b).1), (held.0, held.1));
    // An inode the kernel does not reference is stale before any decision.
    let unreferenced = b
        .workspace
        .native_open_visit(resident(&b), mount, 34, 8, false, Arc::new(none.clone()))
        .unwrap()
        .perform(&b.overlay);
    assert!(matches!(unreferenced.result, Err(OverlayError::Stale)));
    assert!(unreferenced.decision.is_none() && unreferenced.open_candidate.is_none());
    // A mount of another Workspace is refused before any job is made.
    let other = b.overlay.open_workspace([63; 32], [64; 32]).unwrap();
    let elsewhere = b.overlay.create_native_mount(other, 1).unwrap();
    assert!(b
        .workspace
        .native_open_visit(
            resident(&b),
            elsewhere,
            35,
            2,
            false,
            Arc::new(none.clone())
        )
        .is_err());
    assert_eq!((snapshot(&b).0, snapshot(&b).1), (held.0, held.1));

    // A local file needs no base fact: created, then opened again by one
    // visit with nothing resident.
    let serial = b.workspace.next_serial(&b.allocator).unwrap();
    let made = mutate(
        &b,
        resident(&b),
        named(mount, 36, create(1, "local"), Some(serial), T1, &none),
    );
    let Ok(JobOutcome::Applied { publication, .. }) = made.result else {
        panic!("the local file was not created: {made:?}")
    };
    b.overlay.reply_attempted(publication).unwrap();
    let (Seen::Value(local), Some(third)) = open(&b, empty(), mount, 37, serial, true, &none)
    else {
        panic!("a local file was not opened by one visit")
    };
    assert_eq!((local.serial, local.kind), (serial, InodeKind::File));

    // Each descriptor is released by its own RELEASE and by nothing else.
    for descriptor in [first, second, third] {
        b.overlay
            .close_native_file(mount, descriptor.serial(), descriptor.owner_id())
            .unwrap();
    }
    // What remains beyond the start is the created file's kernel reference:
    // its lookup row and its custody row.
    let left = snapshot(&b).1;
    assert_eq!(
        (left.owner_rows, left.owner_details),
        (before.1.owner_rows, before.1.owner_details + 2)
    );
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn facts_of_another_base_are_ignored_and_read_again_for_the_present_one() {
    let b = Bench::new("native-visit-rebase");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let first = b.workspace.base().unwrap();
    let root = first.identity().0.to_bytes();
    let wanted = vec![Need::Inode(1), Need::Name(1, name("file"))];
    assert_eq!(
        observe(
            &b,
            empty(),
            mount,
            1,
            lookup(1, "file"),
            &VisitFacts::default()
        ),
        Seen::Needs(wanted.clone())
    );
    let mut facts = VisitFacts::default();
    facts.supply(&first, &wanted, None).unwrap();

    // The same base asked again for what it already answered: the visit did
    // not accept its facts, and nothing is read a second time.
    let demand = b.demand();
    assert!(matches!(
        facts.supply(&first, &wanted, None),
        Err(WorkspaceError::BaseChanged { expected, actual }) if expected == root && actual == root
    ));
    assert!(matches!(
        facts.supply(&first, &wanted[1..], None),
        Err(WorkspaceError::BaseChanged { .. })
    ));
    assert_eq!(b.demand(), demand);
    // A set with one unanswered need is an ordinary read.
    let more = [wanted[0].clone(), Need::Name(1, name("absent"))];
    facts.supply(&first, &more, None).unwrap();
    assert!(matches!(
        facts.supply(&first, &more, None),
        Err(WorkspaceError::BaseChanged { .. })
    ));

    // A visit made for the first base, with its facts and a resident view of
    // its objects, runs after an install whose base no longer binds the name.
    let made_before = b
        .workspace
        .native_read_visit(
            resident(&b),
            mount,
            1,
            None,
            lookup(1, "file"),
            Arc::new(facts.clone()),
        )
        .unwrap();
    let serial = b.workspace.next_serial(&b.allocator).unwrap();
    let replacing = b
        .workspace
        .native_mutation_visit(
            resident(&b),
            named(mount, 20, mkdir(1, "file"), Some(serial), T1, &facts),
        )
        .unwrap();
    install(&b, vec![(name("file"), None)]);
    let second = b.workspace.base().unwrap();
    assert_ne!(second.identity(), first.identity());
    let before = snapshot(&b);
    assert_eq!(before.0.base_root, second.identity().0.to_bytes());

    // Neither the facts nor the resident view of the first base is used: the
    // visits are undecided instead of answering from a base that is gone.
    let demand = b.demand();
    let outcome = made_before.perform(&b.overlay);
    assert!(matches!(outcome.result, Ok(None)));
    assert!(matches!(
        outcome.decision,
        Some(NativeReadDecision::Needs(ref needs)) if *needs == wanted
    ));
    let outcome = replacing.perform(&b.overlay);
    assert!(matches!(
        outcome.result,
        Ok(JobOutcome::Needs(ref needs)) if *needs == wanted
    ));
    assert_eq!(b.demand(), demand);
    assert_eq!(
        observe(&b, empty(), mount, 1, lookup(1, "file"), &facts),
        Seen::Needs(wanted.clone())
    );
    assert_eq!(snapshot(&b), before);
    assert_eq!(b.overlay.native_lookup_count(mount, 2).unwrap(), None);

    // Supplying from the present base drops the old facts and reads again.
    let stale = facts.clone();
    let demand = b.demand();
    facts.supply(&second, &wanted, None).unwrap();
    assert!(b.demand() > demand);
    // What the first base had answered about another name is gone with it.
    facts
        .supply(&second, &[Need::Name(1, name("absent"))], None)
        .unwrap();
    let present = second.identity().0.to_bytes();
    assert!(matches!(
        facts.supply(&second, &wanted, None),
        Err(WorkspaceError::BaseChanged { expected, actual })
            if expected == present && actual == present
    ));

    // The present base decides: the name is not bound, so the lookup misses
    // and the directory can be made under it.
    assert_eq!(
        observe(&b, empty(), mount, 1, lookup(1, "file"), &facts),
        Seen::Refused(Refusal::Missing)
    );
    assert!(matches!(
        mutate(
            &b,
            empty(),
            named(mount, 21, mkdir(1, "file"), Some(serial), T1, &stale)
        )
        .result,
        Ok(JobOutcome::Needs(ref needs)) if *needs == wanted
    ));
    let made = mutate(
        &b,
        empty(),
        named(mount, 22, mkdir(1, "file"), Some(serial), T1, &facts),
    );
    let Ok(JobOutcome::Applied {
        publication,
        inode: Some(inode),
    }) = made.result
    else {
        panic!("the present base did not decide the mutation: {made:?}")
    };
    assert_eq!((inode.serial, inode.kind), (serial, InodeKind::Directory));
    b.overlay.reply_attempted(publication).unwrap();
    assert_eq!(b.lookup(1, "file").unwrap().serial, serial);
    b.overlay.forget_native(mount, serial, 1).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn a_mutation_visit_publishes_once_with_its_reply_custody_and_holds_no_source() {
    let b = Bench::new("native-visit-mutation");
    let route = b.route();
    let mount = b.overlay.create_native_mount(route, 1).unwrap();
    let directory = b.workspace.next_serial(&b.allocator).unwrap();
    let making = |facts: &VisitFacts| named(mount, 7, mkdir(1, "made"), Some(directory), T1, facts);

    // Nothing resident and no facts: undecided, nothing written.
    let before = snapshot(&b);
    let outcome = mutate(&b, empty(), making(&VisitFacts::default()));
    let Ok(JobOutcome::Needs(needs)) = outcome.result else {
        panic!("a cold mutation was decided: {outcome:?}")
    };
    assert_eq!(needs, vec![Need::Inode(1), Need::Name(1, name("made"))]);
    assert_eq!(outcome.file, None);
    assert_eq!(snapshot(&b), before);

    // With its facts the next visit publishes, in one transaction, with the
    // entry's kernel reference and one reply ticket.
    let mut facts = VisitFacts::default();
    facts
        .supply(&b.workspace.base().unwrap(), &needs, None)
        .unwrap();
    let outcome = mutate(&b, empty(), making(&facts));
    let Ok(JobOutcome::Applied {
        publication,
        inode: Some(inode),
    }) = outcome.result
    else {
        panic!("the supplied facts did not decide the mutation: {outcome:?}")
    };
    assert_eq!(
        (inode.serial, inode.kind),
        (directory, InodeKind::Directory)
    );
    assert_eq!(outcome.file, None);
    let after = snapshot(&b);
    assert_eq!(after.2, before.2 + 1);
    assert_eq!(
        (after.0.revision, after.0.base_readers, after.1.source_rows),
        (before.0.revision + 1, 0, 0)
    );
    assert_eq!(after.1.reply_tickets, before.1.reply_tickets + 1);
    assert_eq!(
        b.overlay.native_lookup_count(mount, directory).unwrap(),
        Some(1)
    );
    assert_eq!(
        b.overlay.pending_publications(route, 0).unwrap(),
        vec![publication]
    );
    b.overlay.reply_attempted(publication).unwrap();
    assert_eq!(b.lookup(1, "made").unwrap().serial, directory);

    // A definite answer without effect needs no base fact at all here: the
    // name is bound locally.
    let before = snapshot(&b);
    let again = b.workspace.next_serial(&b.allocator).unwrap();
    assert!(matches!(
        mutate(
            &b,
            empty(),
            named(
                mount,
                8,
                mkdir(1, "made"),
                Some(again),
                T2,
                &VisitFacts::default()
            )
        )
        .result,
        Ok(JobOutcome::Refused(Refusal::Exists))
    ));
    assert_eq!(snapshot(&b), before);

    // The root's objects are resident now: CREATE with an open descriptor is
    // decided and published by its one visit, with no facts of its own.
    let serial = b.workspace.next_serial(&b.allocator).unwrap();
    let outcome = mutate(
        &b,
        resident(&b),
        NativeVisitRequest {
            open: Some(true),
            ..named(
                mount,
                9,
                create(1, "opened"),
                Some(serial),
                T2,
                &VisitFacts::default(),
            )
        },
    );
    let Ok(JobOutcome::Applied {
        publication,
        inode: Some(inode),
    }) = outcome.result
    else {
        panic!("resident objects did not decide the mutation: {outcome:?}")
    };
    assert_eq!((inode.serial, inode.kind), (serial, InodeKind::File));
    let file = outcome.file.expect("the created file is open");
    assert_eq!((file.serial(), file.writable()), (serial, true));
    assert_eq!(
        b.overlay
            .native_file(mount, serial, file.owner_id())
            .unwrap(),
        file
    );
    assert_eq!(
        b.overlay.retained_native_file(mount, 9).unwrap(),
        Some(file)
    );
    assert_eq!(
        b.overlay.native_lookup_count(mount, serial).unwrap(),
        Some(1)
    );
    let after = snapshot(&b);
    assert_eq!(after.2, before.2 + 1);
    assert_eq!((after.0.base_readers, after.1.source_rows), (0, 0));
    b.overlay.reply_attempted(publication).unwrap();

    // A handle-addressed visit writes through that exact descriptor.
    let through = |request, handle, input, fresh| NativeVisitRequest {
        mount,
        request,
        serial,
        handle,
        input,
        open: None,
        now: T2,
        fresh,
        facts: Arc::new(VisitFacts::default()),
    };
    let write = || NativeInput::Write {
        offset: 0,
        data: b"visited".as_slice().into(),
        cached: false,
    };
    let outcome = mutate(
        &b,
        resident(&b),
        through(10, Some(file.owner_id()), write(), None),
    );
    let Ok(JobOutcome::Applied { publication, .. }) = outcome.result else {
        panic!("the descriptor write was not published: {outcome:?}")
    };
    assert_eq!(outcome.file, None);
    b.overlay.reply_attempted(publication).unwrap();
    assert_eq!(b.content(serial), b"visited");

    // An input and a descriptor that do not agree are refused with no effect:
    // a write without its handle, and a named operation through one.
    let before = snapshot(&b);
    let other = b.workspace.next_serial(&b.allocator).unwrap();
    for request in [
        through(11, None, write(), None),
        through(
            12,
            Some(file.owner_id()),
            NativeInput::Named(mkdir(1, "never")),
            Some(other),
        ),
    ] {
        assert!(matches!(
            mutate(&b, resident(&b), request).result,
            Ok(JobOutcome::Refused(Refusal::Invalid))
        ));
    }
    // A handle that is not this mount's descriptor of that inode is no visit.
    assert!(matches!(
        mutate(
            &b,
            resident(&b),
            through(13, Some(file.owner_id() + 1), write(), None)
        )
        .result,
        Err(WorkspaceError::Overlay(OverlayError::Stale))
    ));
    assert_eq!(snapshot(&b), before);

    // A reserved serial belongs to a creating operation and to nothing else.
    for request in [
        named(mount, 14, mkdir(1, "unreserved"), None, T2, &facts),
        named(mount, 15, mkdir(1, "zero"), Some(0), T2, &facts),
        through(16, Some(file.owner_id()), write(), Some(other)),
    ] {
        assert!(matches!(
            b.workspace.native_mutation_visit(resident(&b), request),
            Err(WorkspaceError::Content(ContentError::InvalidRecord(
                "mutation reserved serial"
            )))
        ));
    }
    assert_eq!(snapshot(&b), before);

    // After revocation every visit is refused before its decision.
    b.overlay
        .close_native_file(mount, serial, file.owner_id())
        .unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
    assert!(matches!(
        mutate(
            &b,
            resident(&b),
            named(mount, 17, mkdir(1, "late"), Some(other), T2, &facts)
        )
        .result,
        Err(WorkspaceError::Overlay(OverlayError::Stale))
    ));
    let late = b
        .workspace
        .native_read_visit(
            resident(&b),
            mount,
            1,
            None,
            lookup(1, "made"),
            Arc::new(facts),
        )
        .unwrap()
        .perform(&b.overlay);
    assert!(matches!(late.result, Err(OverlayError::Stale)));
    assert_eq!(b.lookup(1, "late"), None);
}

#[test]
fn a_visit_through_a_read_only_descriptor_is_refused_and_changes_nothing() {
    let b = Bench::new("native-visit-read-only");
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    // The root's objects become resident through the ordinary client.
    VisitFacts::default()
        .supply(
            &b.workspace.base().unwrap(),
            &[Need::Inode(1), Need::Name(1, name("opened"))],
            None,
        )
        .unwrap();
    let serial = b.workspace.next_serial(&b.allocator).unwrap();
    let opened = mutate(
        &b,
        resident(&b),
        NativeVisitRequest {
            open: Some(false),
            ..named(
                mount,
                1,
                create(1, "opened"),
                Some(serial),
                T1,
                &VisitFacts::default(),
            )
        },
    );
    let Ok(JobOutcome::Applied { publication, .. }) = opened.result else {
        panic!("the file was not created: {opened:?}")
    };
    b.overlay.reply_attempted(publication).unwrap();
    let file = opened.file.expect("the created file is open");
    assert!(!file.writable());

    // The visit's fence read this descriptor with its access mode. A write
    // or a truncation through it is refused before anything is evaluated or
    // written, exactly as the stored row refused it.
    let through = |request, input| NativeVisitRequest {
        mount,
        request,
        serial,
        handle: Some(file.owner_id()),
        input,
        open: None,
        now: T2,
        fresh: None,
        facts: Arc::new(VisitFacts::default()),
    };
    let before = snapshot(&b);
    for input in [
        NativeInput::Write {
            offset: 0,
            data: b"refused".as_slice().into(),
            cached: false,
        },
        NativeInput::Write {
            offset: 0,
            data: b"refused".as_slice().into(),
            cached: true,
        },
        NativeInput::Attributes {
            mode: None,
            mtime: None,
            size: Some(3),
        },
    ] {
        let outcome = mutate(&b, resident(&b), through(2, input));
        assert!(
            matches!(
                outcome.result,
                Err(WorkspaceError::Overlay(OverlayError::Invalid(
                    "read-only descriptor"
                )))
            ),
            "{outcome:?}"
        );
        assert_eq!(outcome.file, None);
    }
    assert_eq!(snapshot(&b), before);
    assert_eq!(b.content(serial), b"");
    // The descriptor still reads attributes and closes.
    let stat = b
        .workspace
        .native_read_visit(
            resident(&b),
            mount,
            serial,
            Some(file.owner_id()),
            NativeReadOperation::Getattr { serial },
            Arc::new(VisitFacts::default()),
        )
        .unwrap()
        .perform(&b.overlay);
    assert!(matches!(
        stat.decision,
        Some(NativeReadDecision::Value(ref inode)) if inode.serial == serial
    ));
    b.overlay
        .close_native_file(mount, serial, file.owner_id())
        .unwrap();
    b.overlay.forget_native(mount, serial, 1).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

#[test]
fn a_resident_client_answers_only_from_memory_and_only_small_objects() {
    const LIMIT: usize = 64 * 1024;
    let f = common::fixture();
    let cache = Arc::new(CanonicalCache::new(CACHE));
    let upstream = CanonicalClient::with_cache(Arc::new(f.store.clone()), None, cache.clone());
    let memory = CanonicalClient::resident(cache.clone());
    let object = |length: usize, salt: u8| {
        let bytes: Vec<u8> = (0..length).map(|i| (i % 251) as u8 ^ salt).collect();
        let id = ObjectId::for_bytes(&bytes);
        f.store.objects.lock().unwrap().insert(id, bytes.clone());
        (id, bytes)
    };
    let (small, small_bytes) = object(100, 1);
    let (edge, edge_bytes) = object(LIMIT, 2);
    let (large, large_bytes) = object(LIMIT + 1, 3);
    let (absent, _) = object(100, 4);
    let demand = |f: &common::Fixture| f.store.demand.load(std::sync::atomic::Ordering::Relaxed);

    // Nothing is resident yet.
    let before = cache.diagnostics().unwrap();
    assert!(matches!(
        memory.read_canonical_batch(&[small]),
        Err(ContentError::ProviderFailure {
            what: "base object not resident"
        })
    ));
    assert_eq!(cache.diagnostics().unwrap(), before);
    assert_eq!(demand(&f), 0);

    // The ordinary client reads three objects from the provider.
    assert_eq!(
        upstream
            .read_canonical_batch(&[small, edge, large])
            .unwrap(),
        vec![small_bytes.clone(), edge_bytes.clone(), large_bytes.clone()]
    );
    let filled = cache.diagnostics().unwrap();
    assert_eq!(
        (
            filled.cache_misses,
            filled.upstream_batches,
            filled.cached_objects
        ),
        (3, 1, 3)
    );
    assert_eq!(demand(&f), 3);

    // Cached objects up to the limit are returned from memory.
    assert_eq!(
        memory.read_canonical_batch(&[small, edge]).unwrap(),
        vec![small_bytes.clone(), edge_bytes]
    );
    let read = cache.diagnostics().unwrap();
    assert_eq!(read.cache_hits, filled.cache_hits + 2);
    // An uncached object fails without a miss, an upstream batch or a demand:
    // the ordinary demand that follows is the one that counts it.
    for ids in [vec![absent], vec![small, absent]] {
        assert!(matches!(
            memory.read_canonical_batch(&ids),
            Err(ContentError::ProviderFailure {
                what: "base object not resident"
            })
        ));
    }
    // A cached object above the limit is not copied inside an owner job.
    assert!(matches!(
        memory.read_canonical_batch(&[large]),
        Err(ContentError::ProviderFailure {
            what: "base object not resident"
        })
    ));
    let refused = cache.diagnostics().unwrap();
    assert_eq!(
        (
            refused.cache_misses,
            refused.upstream_batches,
            refused.cached_objects,
            refused.authenticated_bytes
        ),
        (
            filled.cache_misses,
            filled.upstream_batches,
            filled.cached_objects,
            filled.authenticated_bytes
        )
    );
    assert_eq!(demand(&f), 3);
    // The same cached object is still served to the ordinary client.
    assert_eq!(
        upstream.read_canonical_batch(&[large]).unwrap(),
        vec![large_bytes]
    );
    assert_eq!(demand(&f), 3);
    // It has no length provider either.
    assert!(matches!(
        FileLengths::file_length(&memory, small),
        Err(WorkspaceError::MissingLengthProvider)
    ));
    // The ordinary demand for the uncached object counts its own miss.
    upstream.read_canonical_batch(&[absent]).unwrap();
    let counted = cache.diagnostics().unwrap();
    assert_eq!(
        (counted.cache_misses, counted.upstream_batches),
        (filled.cache_misses + 1, filled.upstream_batches + 1)
    );
    assert_eq!(memory.read_canonical_batch(&[absent]).unwrap().len(), 1);
}

//! Exact child-directory counts, the `subdirs` of a directory's row and of
//! its stat: maintained by every binding change in that change's transaction,
//! and derived for a base directory by listing it in bounded windows. A
//! directory's POSIX link count is this number plus two.
//!
//! Every expectation is checked twice: against the stated number, and against
//! an independent count of the directory's listed children by kind.
mod common;
mod harness;
use common::{file, metadata, name, Fixture, Store};
use harness::{create, link, mkdir, path, rename, rmdir, unlink, Bench, T1, T2};
use layerfs_content::filesystem::{
    build_filesystem, scope_for_seed, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, InodeUpdate, SymlinkTarget,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, FinalizedConsumer, ObjectId};
use layerfs_overlay::{NativeMount, StatementKind, StoredCounts, WorkspaceState};
use layerfs_workspace::{
    CanonicalClient, JobOutcome, NativeInput, NativeReadDecision, NativeReadOperation,
    NativeVisitRequest, Need, Operation, Refusal, ViewStat, VisitFacts, DIRECTORY_COUNT_CAPACITY,
};
use std::{collections::BTreeMap, sync::Arc};

const CACHE: usize = 4 * 1024 * 1024;
/// The one regular file of every tree here; its other names are hard links.
const FILE: u64 = 20;

/// A content-built root described as inodes and names.
struct Plan {
    store: Store,
    inodes: BTreeMap<u64, InodeValue>,
    names: BTreeMap<u64, BTreeMap<String, u64>>,
}
impl Plan {
    fn new() -> Self {
        Self {
            store: Store::default(),
            inodes: BTreeMap::new(),
            names: BTreeMap::new(),
        }
    }
    fn inode(&mut self, serial: u64, kind: InodeKind, content_root: ObjectId) {
        let metadata_root = metadata(&self.store, kind);
        self.inodes.insert(
            serial,
            InodeValue {
                kind,
                content_root,
                metadata_root,
                namespace_ref_count: 0,
            },
        );
    }
    fn directory(&mut self, serial: u64) {
        self.inode(
            serial,
            InodeKind::Directory,
            ObjectId::for_bytes(b"new-directory"),
        );
        self.names.entry(serial).or_default();
    }
    fn regular(&mut self, serial: u64) {
        let root = file(&self.store, b"payload");
        self.inode(serial, InodeKind::RegularFile, root);
    }
    fn symlink(&mut self, serial: u64) {
        let target = SymlinkTarget::new(b"../elsewhere".to_vec())
            .unwrap()
            .finalize()
            .unwrap();
        let root = target.id();
        self.store.clone().accept(target).unwrap();
        self.inode(serial, InodeKind::Symlink, root);
    }
    fn bind(&mut self, parent: u64, child: &str, serial: u64) {
        assert!(self
            .names
            .get_mut(&parent)
            .expect("a declared directory")
            .insert(child.into(), serial)
            .is_none());
    }
    fn build(self) -> Fixture {
        let scope = scope_for_seed([13; 32]);
        let inodes: Vec<_> = self
            .inodes
            .iter()
            .map(|(serial, value)| InodeUpdate {
                serial: *serial,
                value: *value,
            })
            .collect();
        let directories: Vec<_> = self
            .names
            .iter()
            .map(|(parent, names)| DirectoryUpdate {
                parent: *parent,
                changes: names
                    .iter()
                    .map(|(child, serial)| (name(child), Some(*serial)))
                    .collect(),
            })
            .collect();
        let new: Vec<u64> = self.inodes.keys().copied().collect();
        let mut sink = self.store.clone();
        let mut objects = FilesystemObjects::new(&self.store, &mut sink);
        let root = build_filesystem(
            &mut objects,
            &FilesystemInput {
                base: None,
                scope,
                root_serial: 1,
                directories: &directories,
                inodes: &inodes,
                new_inodes: &new,
                resources: FilesystemResources::default(),
            },
            None,
        )
        .unwrap()
        .root;
        Fixture {
            store: self.store,
            root,
            scope,
            bytes: Vec::new(),
        }
    }
}
const DIRS: u64 = 2;
const MIXED: u64 = 3;
const WIDE: u64 = 4;
const EMPTY: u64 = 5;
const WIDE_DIRECTORIES: u64 = 300;
const WIDE_FILES: u64 = 400;
/// `wide` cannot be listed, nor its directories counted, in one window of
/// at most 256 rows.
const _: () = assert!(WIDE_DIRECTORIES > 256 && WIDE_DIRECTORIES + WIDE_FILES > 2 * 256);
/// ```text
/// /            dirs/ mixed/ wide/ empty/ file link            4 of 6
/// /dirs        a/ b/ c/                                       3 of 3
/// /dirs/b      inner
/// /mixed       sub1/ sub2/ f1 f2 l                            2 of 5
/// /mixed/sub2  keep
/// /wide        d000/ .. d299/ f000 .. f399                    300 of 700
/// /empty                                                      0 of 0
/// ```
fn tree() -> Fixture {
    let mut plan = Plan::new();
    for serial in [1, DIRS, MIXED, WIDE, EMPTY, 10, 11, 12, 13, 14] {
        plan.directory(serial);
    }
    plan.regular(FILE);
    plan.symlink(21);
    plan.symlink(22);
    for (child, serial) in [
        ("dirs", DIRS),
        ("mixed", MIXED),
        ("wide", WIDE),
        ("empty", EMPTY),
        ("file", FILE),
        ("link", 21),
    ] {
        plan.bind(1, child, serial);
    }
    for (child, serial) in [("a", 10), ("b", 11), ("c", 12)] {
        plan.bind(DIRS, child, serial);
    }
    plan.bind(11, "inner", FILE);
    for (child, serial) in [
        ("sub1", 13),
        ("sub2", 14),
        ("f1", FILE),
        ("f2", FILE),
        ("l", 22),
    ] {
        plan.bind(MIXED, child, serial);
    }
    plan.bind(14, "keep", FILE);
    for index in 0..WIDE_DIRECTORIES {
        plan.directory(100 + index);
        plan.bind(WIDE, &format!("d{index:03}"), 100 + index);
    }
    for index in 0..WIDE_FILES {
        plan.bind(WIDE, &format!("f{index:03}"), FILE);
    }
    plan.build()
}
/// The independent count: the directory's listed children that stat as a
/// directory, as `readdir` and `lstat` would find them.
fn listed(b: &Bench, serial: u64) -> u64 {
    b.list(serial)
        .0
        .iter()
        .filter(|(_, child)| b.stat(*child).unwrap().kind == InodeKind::Directory)
        .count() as u64
}
fn subdirs(b: &Bench, serial: u64) -> u64 {
    let stat = b.stat(serial).unwrap();
    assert_eq!(stat.kind, InodeKind::Directory, "serial {serial}");
    stat.subdirs
}
/// Each directory reports `expected`, and so does the independent count.
#[track_caller]
fn counts(b: &Bench, expected: &[(u64, u64)]) {
    for (serial, expected) in expected {
        assert_eq!(subdirs(b, *serial), *expected, "stat of {serial}");
        assert_eq!(listed(b, *serial), *expected, "listing of {serial}");
    }
}
fn made(b: &Bench, operation: Operation) -> ViewStat {
    b.applied(operation, T1).expect("the new inode's stat")
}
/// A removed directory keeps a row with no reference and no children: the
/// two facts its reported link count of zero is projected from.
#[track_caller]
fn removed(b: &Bench, serial: u64) {
    let stat = b.stat(serial).unwrap();
    assert_eq!(
        (stat.kind, stat.namespace_refs, stat.subdirs),
        (InodeKind::Directory, 0, 0),
        "removed directory {serial}"
    );
}

#[test]
fn a_base_directory_counts_its_child_directories_and_nothing_else() {
    let b = Bench::over("links-base", tree(), CACHE);
    assert_eq!(b.cache.diagnostics().unwrap().directory_counts, 0);
    counts(
        &b,
        &[
            (1, 4),
            (DIRS, 3),
            (MIXED, 2),
            (WIDE, WIDE_DIRECTORIES),
            (EMPTY, 0),
            (10, 0),
            (11, 0),
            (14, 0),
        ],
    );
    // Files and symlinks are never counted and never carry a count.
    for serial in [FILE, 21, 22] {
        assert_eq!(b.stat(serial).unwrap().subdirs, 0);
    }
    // LOOKUP reports the same value as a stat of the serial.
    assert_eq!(b.lookup(1, "wide").unwrap().subdirs, WIDE_DIRECTORIES);
    assert_eq!(b.lookup(1, "mixed"), Some(b.stat(MIXED).unwrap()));
    assert_eq!(b.lookup(DIRS, "a").unwrap().subdirs, 0);
    // One listing window holds at most 256 rows: 700 rows, 300 of them
    // directories, were counted over several windows.
    // A count is derived once per directory content: every later stat is
    // answered from the remembered value. No local row was written.
    let work = b.cache.diagnostics().unwrap();
    assert!(work.directory_count_scans >= 5, "{work:?}");
    counts(&b, &[(1, 4), (WIDE, WIDE_DIRECTORIES), (MIXED, 2)]);
    let again = b.cache.diagnostics().unwrap();
    assert_eq!(again.directory_count_scans, work.directory_count_scans);
    assert_eq!(again.directory_counts, work.directory_counts);
    for serial in [1, DIRS, MIXED, WIDE, EMPTY] {
        assert_eq!(b.overlay.inode(b.route(), serial).unwrap(), None);
    }
}

#[test]
fn every_binding_change_moves_the_count_of_a_local_directory_exactly() {
    let b = Bench::over("links-local", tree(), CACHE);
    // mkdir: +1 on the parent, and the new directory starts at zero. The
    // root is a base directory: its first local row starts from the base 4.
    let new = made(&b, mkdir(1, "new"));
    assert_eq!((new.kind, new.subdirs), (InodeKind::Directory, 0));
    let new = new.serial;
    counts(&b, &[(1, 5), (new, 0)]);
    let x = made(&b, mkdir(new, "x")).serial;
    let y = made(&b, mkdir(new, "y")).serial;
    counts(&b, &[(new, 2), (x, 0), (y, 0), (1, 5)]);

    // A file, a symlink and a hard link change entries, never the count.
    made(&b, create(new, "f"));
    b.applied(
        Operation::Symlink {
            parent: new,
            name: name("s"),
            target: SymlinkTarget::new(b"x".to_vec()).unwrap(),
        },
        T1,
    );
    b.applied(link(FILE, new, "h"), T1);
    counts(&b, &[(new, 2)]);
    assert_eq!(b.names(new), ["f", "h", "s", "x", "y"]);
    b.applied(unlink(new, "h"), T2);
    counts(&b, &[(new, 2)]);

    // rmdir: -1; the removed directory reports no reference and no child.
    b.applied(rmdir(new, "x"), T2);
    counts(&b, &[(new, 1)]);
    removed(&b, x);

    // A directory renamed within its parent changes nothing.
    b.applied(rename((new, "y"), (new, "z"), false, None), T2);
    counts(&b, &[(new, 1), (1, 5)]);
    // A file renamed over a file in the same parent: one entry fewer, the
    // count unchanged.
    made(&b, create(new, "g"));
    b.applied(rename((new, "f"), (new, "g"), true, None), T2);
    counts(&b, &[(new, 1)]);
    assert_eq!(b.names(new), ["g", "s", "z"]);

    // A directory renamed over an empty directory of the same parent: the
    // replaced one is the only binding the parent loses.
    let p = made(&b, mkdir(new, "p")).serial;
    let q = made(&b, mkdir(new, "q")).serial;
    counts(&b, &[(new, 3)]);
    b.applied(rename((new, "p"), (new, "q"), true, None), T2);
    counts(&b, &[(new, 2)]);
    removed(&b, q);
    assert_eq!(b.lookup(new, "q").unwrap().serial, p);

    // A directory moved to another parent: -1 there, +1 here.
    let other = made(&b, mkdir(1, "other")).serial;
    counts(&b, &[(1, 6), (other, 0)]);
    b.applied(
        rename((new, "z"), (other, "z"), false, path(&["other"])),
        T2,
    );
    counts(&b, &[(new, 1), (other, 1), (1, 6)]);
    // Moved over an empty directory of another parent: the source loses
    // one, the destination gains the moved one and loses the replaced one.
    let t = made(&b, mkdir(other, "t")).serial;
    counts(&b, &[(other, 2)]);
    b.applied(rename((new, "q"), (other, "t"), true, path(&["other"])), T2);
    counts(&b, &[(new, 0), (other, 2), (1, 6)]);
    removed(&b, t);
    // A file moved to another parent changes neither count.
    b.applied(rename((new, "g"), (other, "g"), false, None), T2);
    counts(&b, &[(new, 0), (other, 2)]);
    assert_eq!(b.names(other), ["g", "t", "z"]);

    // A deeper tree: only the direct parent counts a directory.
    let deep = made(&b, mkdir(y, "deep")).serial;
    made(&b, mkdir(deep, "deeper"));
    counts(&b, &[(y, 1), (deep, 1), (other, 2), (1, 6)]);
    // A refused removal publishes nothing and leaves every count.
    assert_eq!(b.refused(rmdir(other, "z")), Refusal::NotEmpty);
    assert_eq!(
        b.refused(rename((other, "t"), (other, "z"), true, None)),
        Refusal::NotEmpty
    );
    counts(&b, &[(y, 1), (other, 2), (new, 0), (1, 6)]);

    // The count is its own column: the reference count of a directory stays
    // one, and the root's zero, as the canonical grammar requires.
    let row = b.overlay.inode(b.route(), other).unwrap().unwrap();
    assert_eq!((row.nlink, row.entries, row.subdirs), (1, 3, 2));
    let root = b.overlay.inode(b.route(), 1).unwrap().unwrap();
    assert_eq!((root.nlink, root.entries, root.subdirs), (0, 8, 6));
}

#[test]
fn a_base_directory_with_children_keeps_its_count_exact_under_every_change() {
    let b = Bench::over("links-inherited", tree(), CACHE);
    let all = |mixed, dirs, empty, wide, root| {
        counts(
            &b,
            &[
                (MIXED, mixed),
                (DIRS, dirs),
                (EMPTY, empty),
                (WIDE, wide),
                (1, root),
            ],
        )
    };
    all(2, 3, 0, WIDE_DIRECTORIES, 4);
    // The first local row of a base directory starts from the base count,
    // which excludes its two file names and its symlink.
    let n = made(&b, mkdir(MIXED, "n")).serial;
    let row = b.overlay.inode(b.route(), MIXED).unwrap().unwrap();
    assert_eq!((row.entries, row.subdirs, row.born), (6, 3, 0));
    all(3, 3, 0, WIDE_DIRECTORIES, 4);
    made(&b, create(MIXED, "g"));
    b.applied(unlink(MIXED, "f1"), T2);
    b.applied(unlink(MIXED, "l"), T2);
    all(3, 3, 0, WIDE_DIRECTORIES, 4);

    // rmdir of a base subdirectory is a whiteout and one directory fewer.
    b.applied(rmdir(MIXED, "sub1"), T2);
    all(2, 3, 0, WIDE_DIRECTORIES, 4);
    removed(&b, 13);
    assert_eq!(b.lookup(MIXED, "sub1"), None);
    // A base subdirectory moved to another base parent.
    b.applied(
        rename((MIXED, "sub2"), (DIRS, "sub2"), false, path(&["dirs"])),
        T2,
    );
    all(1, 4, 0, WIDE_DIRECTORIES, 4);
    assert_eq!(b.names(MIXED), ["f2", "g", "n"]);
    assert_eq!(b.lookup(MIXED, "n").unwrap().serial, n);

    // An empty base directory replaced by a base directory, same parent.
    b.applied(rename((DIRS, "a"), (DIRS, "c"), true, None), T2);
    all(1, 3, 0, WIDE_DIRECTORIES, 4);
    removed(&b, 12);
    // A base directory moved into an empty base directory, whose count
    // starts from zero without any listing.
    b.applied(
        rename((DIRS, "b"), (EMPTY, "b"), false, path(&["empty"])),
        T2,
    );
    all(1, 2, 1, WIDE_DIRECTORIES, 4);
    // And back over the empty directory another parent binds.
    b.applied(rename((EMPTY, "b"), (DIRS, "c"), true, path(&["dirs"])), T2);
    all(1, 2, 0, WIDE_DIRECTORIES, 4);
    removed(&b, 10);
    assert_eq!(b.names(DIRS), ["c", "sub2"]);
    assert_eq!(b.lookup(DIRS, "c").unwrap().serial, 11);

    // A directory wider than one listing window: its first local row holds
    // the count of all its windows.
    b.applied(rmdir(WIDE, "d000"), T2);
    let row = b.overlay.inode(b.route(), WIDE).unwrap().unwrap();
    assert_eq!(
        (row.entries, row.subdirs),
        (WIDE_DIRECTORIES + WIDE_FILES - 1, WIDE_DIRECTORIES - 1)
    );
    all(1, 2, 0, WIDE_DIRECTORIES - 1, 4);
    made(&b, mkdir(WIDE, "zz"));
    b.applied(unlink(WIDE, "f000"), T2);
    all(1, 2, 0, WIDE_DIRECTORIES, 4);
    // Moved up into the root: the root is a directory like any other.
    b.applied(rename((WIDE, "d001"), (1, "d001"), false, path(&[])), T2);
    all(1, 2, 0, WIDE_DIRECTORIES - 1, 5);
    b.applied(rmdir(1, "empty"), T2);
    counts(&b, &[(1, 4), (MIXED, 1), (DIRS, 2)]);
    removed(&b, EMPTY);
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
/// The memory-only client of the cache the Workspace's own client fills.
fn resident(b: &Bench) -> Arc<CanonicalClient> {
    Arc::new(CanonicalClient::resident(b.cache.clone()))
}
fn lookup(child: &str) -> NativeReadOperation {
    NativeReadOperation::Lookup {
        parent: 1,
        name: name(child),
    }
}
/// One LOOKUP in the root as one owner job, which asks the provider nothing.
fn observe(
    b: &Bench,
    mount: NativeMount,
    child: &str,
    facts: &VisitFacts,
) -> Option<NativeReadDecision> {
    let visit = b
        .workspace
        .native_read_visit(
            resident(b),
            mount,
            1,
            None,
            lookup(child),
            Arc::new(facts.clone()),
        )
        .unwrap();
    let (demand, scans) = (
        b.demand(),
        b.cache.diagnostics().unwrap().directory_count_scans,
    );
    let outcome = visit.perform(&b.overlay);
    assert_eq!(b.demand(), demand, "an owner visit asked the provider");
    assert_eq!(
        b.cache.diagnostics().unwrap().directory_count_scans,
        scans,
        "an owner visit listed a directory"
    );
    assert!(matches!(outcome.result, Ok(None)), "{:?}", outcome.result);
    outcome.decision
}
const NOT_REMEMBERED: ContentError = ContentError::ProviderFailure {
    what: "base directory count not resident",
};

#[test]
fn a_visit_that_misses_the_count_is_undecided_writes_nothing_and_decides_once_supplied() {
    let b = Bench::over("links-visit", tree(), CACHE);
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let base = b.workspace.base().unwrap();
    // Every canonical object the lookup of `mixed` reads is made resident
    // through the base's ordinary reads, which derive no count.
    let root = base.stat(1).unwrap().value;
    let mixed = base.stat(MIXED).unwrap().value;
    assert_eq!(base.child(1, &name("mixed")).unwrap().serial, MIXED);
    assert_eq!(
        (base.entries(root).unwrap(), base.entries(mixed).unwrap()),
        (6, 5)
    );
    assert_eq!(b.cache.diagnostics().unwrap().directory_counts, 0);
    let empty = base.stat(EMPTY).unwrap().value;
    assert_eq!(base.entries(empty).unwrap(), 0);
    // The memory-only binding reads all of them and has no count to give.
    let memory = base.with_client(resident(&b));
    let demand = b.demand();
    assert_eq!(memory.stat(1).unwrap().value, root);
    assert_eq!(memory.child(1, &name("mixed")).unwrap().serial, MIXED);
    assert_eq!(memory.entries(mixed).unwrap(), 5);
    assert_eq!(memory.subdirs(root), Err(NOT_REMEMBERED));
    assert_eq!(memory.subdirs(mixed), Err(NOT_REMEMBERED));
    // An empty directory needs neither a listing nor a remembered count.
    assert_eq!(memory.subdirs(empty), Ok(0));
    assert_eq!(b.demand(), demand, "a memory-only read asked the provider");

    // So the visit is undecided, for the count alone, and changed nothing.
    let wanted = vec![Need::Inode(1), Need::Name(1, name("mixed"))];
    let before = snapshot(&b);
    let quiet = b.demand();
    let Some(NativeReadDecision::Needs(needs)) =
        observe(&b, mount, "mixed", &VisitFacts::default())
    else {
        panic!("a visit decided without the root's child-directory count")
    };
    assert_eq!(needs, wanted);
    assert_eq!(snapshot(&b), before);
    assert_eq!(b.overlay.native_lookup_count(mount, MIXED).unwrap(), None);
    let work = b.cache.diagnostics().unwrap();
    assert_eq!((work.directory_counts, work.directory_count_scans), (0, 0));
    // The same holds for a mutation: no row, no reply ticket, no reference.
    let fresh = b.workspace.next_serial(&b.allocator).unwrap();
    let making = |request, facts: &VisitFacts| NativeVisitRequest {
        mount,
        request,
        serial: 1,
        handle: None,
        input: NativeInput::Named(mkdir(1, "made")),
        open: None,
        now: T1,
        fresh: Some(fresh),
        facts: Arc::new(facts.clone()),
    };
    let before = snapshot(&b);
    let undecided = b
        .workspace
        .native_mutation_visit(resident(&b), making(7, &VisitFacts::default()))
        .unwrap()
        .perform(&b.overlay);
    assert!(
        matches!(&undecided.result, Ok(JobOutcome::Needs(needs))
            if *needs == vec![Need::Inode(1), Need::Name(1, name("made"))]),
        "{undecided:?}"
    );
    assert_eq!(snapshot(&b), before);
    assert_eq!(b.overlay.inode(b.route(), 1).unwrap(), None);
    assert_eq!(b.demand(), quiet, "neither visit asked the provider");

    // The facts are read outside the owner: the two directories are listed
    // there, once each, and the counts are remembered.
    let mut facts = VisitFacts::default();
    facts.supply(&base, &wanted, None).unwrap();
    let work = b.cache.diagnostics().unwrap();
    assert_eq!((work.directory_counts, work.directory_count_scans), (2, 2));
    // The next visit of the request decides from the facts it carries.
    let Some(NativeReadDecision::Value(found)) = observe(&b, mount, "mixed", &facts) else {
        panic!("the supplied facts did not decide the lookup")
    };
    assert_eq!(
        (found.serial, found.entries, found.subdirs, found.nlink),
        (MIXED, 5, 2, 1)
    );
    assert_eq!(
        b.overlay.native_lookup_count(mount, MIXED).unwrap(),
        Some(1)
    );
    // Another request, carrying nothing, decides in its one visit: the
    // memory-only path answers from the remembered counts.
    let Some(NativeReadDecision::Value(again)) =
        observe(&b, mount, "mixed", &VisitFacts::default())
    else {
        panic!("the remembered counts did not decide the lookup")
    };
    assert_eq!(again, found);
    assert_eq!(memory.subdirs(root), Ok(4));
    // And the mutation publishes the root's first local row from them.
    let made = b
        .workspace
        .native_mutation_visit(resident(&b), making(8, &VisitFacts::default()))
        .unwrap()
        .perform(&b.overlay);
    let Ok(JobOutcome::Applied {
        publication,
        inode: Some(inode),
    }) = made.result
    else {
        panic!("the remembered counts did not decide the mutation: {made:?}")
    };
    assert_eq!((inode.serial, inode.subdirs), (fresh, 0));
    b.overlay.reply_attempted(publication).unwrap();
    let row = b.overlay.inode(b.route(), 1).unwrap().unwrap();
    assert_eq!((row.entries, row.subdirs, row.nlink), (7, 5, 0));
    counts(&b, &[(1, 5), (MIXED, 2)]);
    b.overlay.forget_native(mount, MIXED, 2).unwrap();
    b.overlay.forget_native(mount, fresh, 1).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

/// A root of `count` non-empty directories `d00000..`, each with its own
/// file name, so each has its own content root. Every 1024th also has two
/// child directories.
fn many(count: u64) -> Fixture {
    let mut plan = Plan::new();
    plan.directory(1);
    plan.regular(FILE);
    let mut next = 100 + count;
    for index in 0..count {
        let serial = 100 + index;
        plan.directory(serial);
        plan.bind(1, &format!("d{index:05}"), serial);
        plan.bind(serial, &format!("f{index:05}"), FILE);
        if index % 1024 == 0 {
            for child in ["x", "y"] {
                plan.directory(next);
                plan.bind(serial, child, next);
                next += 1;
            }
        }
    }
    plan.build()
}

#[test]
fn remembered_counts_are_bounded_and_an_evicted_one_is_derived_again() {
    assert_eq!(DIRECTORY_COUNT_CAPACITY, 16_384);
    let capacity = DIRECTORY_COUNT_CAPACITY as u64;
    // Two more non-empty directories than the capacity, and the root.
    let directories = capacity + 2;
    let b = Bench::over("links-bound", many(directories), CACHE);
    let mount = b.overlay.create_native_mount(b.route(), 1).unwrap();
    let base = b.workspace.base().unwrap();
    let expected = |index: u64| if index % 1024 == 0 { 2 } else { 0 };
    let derive = |index: u64| {
        let value = base.inode(100 + index).unwrap().value;
        assert_eq!(base.subdirs(value).unwrap(), expected(index), "d{index:05}");
    };
    let work = || b.cache.diagnostics().unwrap();

    // One request: undecided, then its facts are read outside the owner.
    // The root and d00000 are listed and remembered; the root's 16,386
    // rows are 65 listing windows.
    let wanted = vec![Need::Inode(1), Need::Name(1, name("d00000"))];
    let Some(NativeReadDecision::Needs(needs)) =
        observe(&b, mount, "d00000", &VisitFacts::default())
    else {
        panic!("a cold visit decided")
    };
    assert_eq!(needs, wanted);
    let mut facts = VisitFacts::default();
    facts.supply(&base, &wanted, None).unwrap();
    assert_eq!(
        (work().directory_counts, work().directory_count_scans),
        (2, 2)
    );

    // Before its second visit, other work derives the counts of `capacity`
    // more directories. The table stays at its capacity, and the two oldest
    // counts, the ones this request's facts came from, are evicted.
    for index in 1..=capacity {
        derive(index);
    }
    assert_eq!(
        (work().directory_counts, work().directory_count_scans),
        (DIRECTORY_COUNT_CAPACITY, capacity + 2)
    );
    derive(capacity + 1);
    assert_eq!(
        (work().directory_counts, work().directory_count_scans),
        (DIRECTORY_COUNT_CAPACITY, capacity + 3)
    );
    // The request carries its facts: the second visit decides without the
    // remembered counts, so an eviction between two visits cannot loop.
    let root = base.inode(1).unwrap().value;
    let Some(NativeReadDecision::Value(found)) = observe(&b, mount, "d00000", &facts) else {
        panic!("the carried facts did not decide the lookup")
    };
    assert_eq!((found.serial, found.entries, found.subdirs), (100, 3, 2));
    assert_eq!(work().directory_count_scans, capacity + 3);

    // Both were evicted: each is derived again, correctly, and remembered
    // again in place of the next oldest. A remembered one is not derived
    // twice.
    derive(0);
    assert_eq!(base.subdirs(root).unwrap(), directories);
    assert_eq!(
        (work().directory_counts, work().directory_count_scans),
        (DIRECTORY_COUNT_CAPACITY, capacity + 5)
    );
    derive(0);
    derive(capacity + 1);
    derive(1024);
    assert_eq!(base.subdirs(root).unwrap(), directories);
    assert_eq!(
        (work().directory_counts, work().directory_count_scans),
        (DIRECTORY_COUNT_CAPACITY, capacity + 5)
    );
    b.overlay.forget_native(mount, 100, 1).unwrap();
    b.overlay.revoke_native_mount(mount).unwrap();
}

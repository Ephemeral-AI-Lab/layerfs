//! Public S4 proofs: ordinary namespace operations over a real content-built
//! root, exact refusals, atomic effects and declared enumeration semantics.
mod common;
mod harness;
use common::name;
use harness::*;
use layerfs_content::filesystem::SymlinkTarget;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::ContentError;
use layerfs_workspace::{Operation, Outcome, Refusal, Time, WorkspaceError, SERIAL_REFILL};
use std::sync::atomic::Ordering;

const BASE_NAMES: [&str; 7] = [
    ".git",
    "alias",
    "cache",
    "file",
    "node_modules",
    "output",
    "symlink",
];

#[test]
fn create_mkdir_symlink_and_link_publish_parent_and_reference_effects_atomically() {
    let b = Bench::new("create");
    let created = b.applied(create(1, "new.txt"), T1).unwrap();
    assert_eq!(
        (
            created.serial,
            created.kind,
            created.namespace_refs,
            created.logical_len
        ),
        (1000, InodeKind::RegularFile, 1, 0)
    );
    assert_eq!(created.metadata.mode, 0o640);
    assert_eq!(
        (
            created.metadata.mtime_seconds,
            created.metadata.mtime_nanoseconds
        ),
        (T1.seconds, T1.nanoseconds)
    );
    assert_eq!(b.lookup(1, "new.txt"), Some(created.clone()));
    // The inherited parent took the same publication: its time and its name.
    let root = b.stat(1).unwrap();
    assert_eq!(root.metadata.mtime_seconds, T1.seconds);
    assert_eq!(root.metadata.mode, 0o1777);
    let state = b.overlay.state(b.route()).unwrap();
    assert_eq!(
        (
            state.revision,
            state.dirty_inodes,
            state.dirty_directory_entries
        ),
        (1, 2, 1)
    );
    let mut expected: Vec<String> = BASE_NAMES.iter().map(|n| n.to_string()).collect();
    expected.push("new.txt".into());
    expected.sort();
    assert_eq!(b.names(1), expected);

    // Bound names refuse without effect, whether inherited or local.
    assert_eq!(b.refused(create(1, "file")), Refusal::Exists);
    assert_eq!(b.refused(mkdir(1, "new.txt")), Refusal::Exists);
    assert_eq!(b.refused(create(2, "under-file")), Refusal::NotDirectory);
    assert_eq!(b.refused(create(999, "nowhere")), Refusal::Missing);
    assert_eq!(
        b.refused(Operation::Create {
            parent: 1,
            name: name("setuid"),
            mode: 0o4755
        }),
        Refusal::NotPermitted
    );
    assert_eq!(
        b.refused(Operation::Mkdir {
            parent: 1,
            name: name("setgid"),
            mode: 0o2755
        }),
        Refusal::NotPermitted
    );
    assert!(matches!(
        b.run(
            create(1, "bad-time"),
            Time {
                seconds: 0,
                nanoseconds: 1_000_000_000
            }
        ),
        Err(WorkspaceError::Refused(Refusal::Invalid))
    ));

    // A directory created above the installed floor has no base children:
    // everything beneath it settles in one owner round with no base demand.
    let dir = b
        .applied(
            Operation::Mkdir {
                parent: 1,
                name: name("built"),
                mode: 0o1755,
            },
            T1,
        )
        .unwrap();
    assert_eq!(
        (dir.kind, dir.namespace_refs, dir.metadata.mode),
        (InodeKind::Directory, 1, 0o1755)
    );
    let demand = b.demand();
    let rounds = b.rounds.get();
    let inner = b.applied(create(dir.serial, "inner"), T2).unwrap();
    assert_eq!(b.rounds.get() - rounds, 1);
    assert_eq!(b.lookup(dir.serial, "inner"), Some(inner));
    assert_eq!(b.lookup(dir.serial, "absent"), None);
    assert_eq!(b.names(dir.serial), vec!["inner"]);
    let target = SymlinkTarget::new(b"../.git/\xffindex".to_vec()).unwrap();
    let symlink = b
        .applied(
            Operation::Symlink {
                parent: dir.serial,
                name: name("ln"),
                target: target.clone(),
            },
            T2,
        )
        .unwrap();
    assert_eq!(
        (symlink.kind, symlink.metadata.mode, symlink.logical_len),
        (InodeKind::Symlink, 0o777, 14)
    );
    assert_eq!(
        b.window(|view| view.readlink(&b.overlay, symlink.serial))
            .unwrap(),
        target
    );
    assert_eq!(b.demand(), demand, "local directories make no base demand");
    assert_eq!(
        b.stat(dir.serial).unwrap().metadata.mtime_seconds,
        T2.seconds
    );
    // The inherited symlink still resolves through the base object.
    assert_eq!(
        b.window(|view| view.readlink(&b.overlay, 3))
            .unwrap()
            .as_bytes(),
        b"../.git/index"
    );
    assert_eq!(
        b.refused(Operation::Symlink {
            parent: dir.serial,
            name: name("empty"),
            target: SymlinkTarget::new(Vec::new()).unwrap()
        }),
        Refusal::Missing
    );

    // A hard link is one more reference of the same inode under every alias.
    let linked = b.applied(link(2, dir.serial, "third"), T1).unwrap();
    assert_eq!((linked.serial, linked.namespace_refs), (2, 3));
    assert_eq!(linked.metadata.mtime_seconds, i64::MAX, "link keeps mtime");
    assert_eq!(linked.logical_len, 10);
    for (parent, alias) in [(1, "file"), (1, "alias"), (dir.serial, "third")] {
        assert_eq!(b.lookup(parent, alias), Some(linked.clone()));
    }
    assert_eq!(b.refused(link(4, 1, "dir-link")), Refusal::NotPermitted);
    assert_eq!(b.refused(link(3, 1, "symlink-link")), Refusal::NotPermitted);
    assert_eq!(b.refused(link(999, 1, "missing-link")), Refusal::Missing);
    assert_eq!(b.refused(link(2, 1, "alias")), Refusal::Exists);
    assert_eq!(b.refused(link(2, 2, "under-file")), Refusal::NotDirectory);

    // One reservation served every creation; refused creations burn serials
    // rather than recycle them.
    assert_eq!(b.allocator.calls.load(Ordering::Relaxed), 1);
    assert!((1000..1000 + SERIAL_REFILL).contains(&symlink.serial));
}

#[test]
fn unlink_and_rmdir_remove_one_reference_and_keep_directory_counts_exact() {
    let b = Bench::new("remove");
    // An inherited alias needs a whiteout; the inode keeps its other name.
    assert_eq!(b.applied(unlink(1, "alias"), T1), None);
    assert_eq!(b.lookup(1, "alias"), None);
    let file = b.lookup(1, "file").unwrap();
    assert_eq!(
        (file.serial, file.namespace_refs, file.logical_len),
        (2, 1, 10)
    );
    assert_eq!(b.stat(1).unwrap().metadata.mtime_seconds, T1.seconds);
    assert_eq!(b.refused(unlink(1, "alias")), Refusal::Missing);
    assert_eq!(b.refused(unlink(1, "never")), Refusal::Missing);
    // The last name leaves a serial-addressed inode with no references.
    b.applied(unlink(1, "file"), T2);
    assert_eq!(b.lookup(1, "file"), None);
    assert_eq!(b.stat(2).unwrap().namespace_refs, 0);
    assert_eq!(b.refused(link(2, 1, "revived")), Refusal::Missing);

    assert_eq!(b.refused(unlink(1, ".git")), Refusal::IsDirectory);
    assert_eq!(b.refused(rmdir(1, "symlink")), Refusal::NotDirectory);
    assert_eq!(b.refused(rmdir(1, ".git")), Refusal::NotEmpty);
    // Emptiness is the exact maintained count: no enumeration is needed even
    // for an inherited directory whose only child was a base binding.
    let shared = b.lookup(4, "index").unwrap();
    assert_eq!((shared.serial, shared.namespace_refs), (8, 4));
    b.applied(unlink(4, "index"), T1);
    assert_eq!(b.lookup(5, "pkg").unwrap().namespace_refs, 3);
    b.applied(rmdir(1, ".git"), T2);
    assert_eq!(b.lookup(1, ".git"), None);
    assert_eq!(b.stat(4).unwrap().namespace_refs, 0);
    assert_eq!(b.refused(create(4, "orphaned")), Refusal::Missing);
    assert!(matches!(
        b.window(|view| view.list(&b.overlay, 4, None)),
        Err(WorkspaceError::Content(ContentError::PathNotFound))
    ));
    assert!(matches!(
        b.window(|view| view.lookup(&b.overlay, 4, &name("index"))),
        Err(WorkspaceError::Content(ContentError::PathNotFound))
    ));
    assert_eq!(
        b.names(1),
        vec!["cache", "node_modules", "output", "symlink"]
    );

    // Created and removed inside one generation: no name row remains, and a
    // new directory's count returns to empty so it can be removed.
    let names = b.overlay.state(b.route()).unwrap().dirty_directory_entries;
    let dir = b.applied(mkdir(1, "operation_record"), T1).unwrap();
    b.applied(create(dir.serial, "a"), T1);
    b.applied(create(dir.serial, "b"), T1);
    assert_eq!(b.refused(rmdir(1, "operation_record")), Refusal::NotEmpty);
    b.applied(unlink(dir.serial, "a"), T1);
    b.applied(unlink(dir.serial, "b"), T1);
    b.applied(rmdir(1, "operation_record"), T1);
    assert_eq!(
        b.overlay.state(b.route()).unwrap().dirty_directory_entries,
        names
    );
    assert_eq!(
        b.overlay
            .directory_entry(b.route(), 1, b"operation_record")
            .unwrap(),
        None
    );
    assert_eq!(b.lookup(1, "operation_record"), None);
    // A removed name can be created again and is a different inode.
    let again = b.applied(mkdir(1, ".git"), T1).unwrap();
    assert_ne!(again.serial, 4);
    assert_eq!(b.names(again.serial), Vec::<String>::new());
    assert_eq!(b.lookup(again.serial, "index"), None);
}

#[test]
fn rename_moves_replaces_and_refuses_cycles_without_descendant_rows() {
    let b = Bench::new("rename");
    // Same-directory move of an inherited file: serial and references stay.
    assert_eq!(
        b.applied(rename((1, "file"), (1, "moved"), true, None), T1),
        None
    );
    assert_eq!(b.lookup(1, "file"), None);
    let moved = b.lookup(1, "moved").unwrap();
    assert_eq!((moved.serial, moved.namespace_refs), (2, 2));
    assert_eq!(moved.metadata.mtime_seconds, i64::MAX);
    assert_eq!(b.stat(1).unwrap().metadata.mtime_seconds, T1.seconds);
    assert_eq!(
        b.refused(rename((1, "file"), (1, "x"), true, None)),
        Refusal::Missing
    );
    assert_eq!(
        b.refused(rename((1, "alias"), (1, "symlink"), false, None)),
        Refusal::Exists
    );
    // Replacement drops exactly one reference of the replaced inode.
    b.applied(rename((1, "moved"), (1, "symlink"), true, None), T1);
    assert_eq!(b.lookup(1, "symlink").unwrap().serial, 2);
    assert_eq!(b.stat(3).unwrap().namespace_refs, 0);
    // Two names of one inode: success with no change and no ticket.
    let before = b.overlay.state(b.route()).unwrap();
    assert!(matches!(
        b.run(rename((1, "alias"), (1, "symlink"), true, None), T2)
            .unwrap(),
        Outcome::Unchanged { .. }
    ));
    assert!(matches!(
        b.run(rename((1, "alias"), (1, "alias"), false, None), T2)
            .unwrap(),
        Outcome::Unchanged { .. }
    ));
    assert_eq!(b.overlay.state(b.route()).unwrap(), before);
    assert_eq!(b.lookup(1, "alias").unwrap().namespace_refs, 2);

    assert_eq!(
        b.refused(rename((1, "cache"), (1, "alias"), true, None)),
        Refusal::NotDirectory
    );
    assert_eq!(
        b.refused(rename((1, "alias"), (1, "cache"), true, None)),
        Refusal::IsDirectory
    );
    assert_eq!(
        b.refused(rename((1, "cache"), (1, "output"), true, None)),
        Refusal::NotEmpty
    );
    // A directory replaces an empty directory; its children follow its serial.
    let empty = b.applied(mkdir(1, "empty"), T1).unwrap();
    let rows = b.overlay.state(b.route()).unwrap();
    b.applied(rename((1, "cache"), (1, "empty"), true, None), T1);
    assert_eq!(b.lookup(1, "empty").unwrap().serial, 6);
    assert_eq!(b.lookup(1, "cache"), None);
    assert_eq!(b.stat(empty.serial).unwrap().namespace_refs, 0);
    assert_eq!(b.lookup(6, "state").unwrap().serial, 8);
    let after = b.overlay.state(b.route()).unwrap();
    // The rows written are the names, the parent and the replaced inode only.
    assert_eq!(after.dirty_inodes, rows.dirty_inodes);
    assert_eq!(
        after.dirty_directory_entries,
        rows.dirty_directory_entries + 1
    );

    // A file crosses directories without ancestry evidence.
    b.applied(rename((4, "index"), (5, "index2"), true, None), T2);
    assert_eq!(b.names(4), Vec::<String>::new());
    assert_eq!(b.names(5), vec!["index2", "pkg"]);
    assert_eq!(b.lookup(5, "index2").unwrap().namespace_refs, 4);
    b.applied(rmdir(1, ".git"), T1);

    // A directory changing parent must prove its destination's ancestry.
    assert_eq!(
        b.refused(rename((1, "output"), (5, "out"), true, None)),
        Refusal::AncestryRequired
    );
    assert_eq!(
        b.refused(rename((1, "output"), (5, "out"), true, path(&["empty"]))),
        Refusal::AncestryMismatch
    );
    assert_eq!(
        b.refused(rename((1, "output"), (5, "out"), true, path(&["absent"]))),
        Refusal::AncestryMismatch
    );
    b.applied(
        rename((1, "output"), (5, "out"), true, path(&["node_modules"])),
        T1,
    );
    assert_eq!(b.lookup(5, "out").unwrap().serial, 7);
    assert_eq!(b.lookup(7, "result").unwrap().serial, 8);
    // Beneath itself, directly or through a directory moved in this overlay.
    assert_eq!(
        b.refused(rename(
            (1, "node_modules"),
            (5, "self"),
            true,
            path(&["node_modules"])
        )),
        Refusal::Invalid
    );
    assert_eq!(
        b.refused(rename(
            (1, "node_modules"),
            (7, "deep"),
            true,
            path(&["node_modules", "out"])
        )),
        Refusal::Invalid
    );
    // The old position of a moved directory is not its ancestry any more.
    assert_eq!(
        b.refused(rename((1, "empty"), (7, "in"), true, path(&["output"]))),
        Refusal::AncestryMismatch
    );
    b.applied(
        rename(
            (1, "empty"),
            (7, "in"),
            true,
            path(&["node_modules", "out"]),
        ),
        T2,
    );
    assert_eq!(b.names(7), vec!["in", "result"]);
    assert_eq!(b.names(1), vec!["alias", "node_modules", "symlink"]);
    assert_eq!(b.lookup(6, "state").unwrap().serial, 8);
}

#[test]
fn chmod_and_utimens_follow_the_portable_grammar_and_keep_one_mtime() {
    let b = Bench::new("attributes");
    let set = |serial, mode, mtime| Operation::SetAttributes {
        serial,
        mode,
        mtime,
        size: None,
    };
    let changed = b.applied(set(2, Some(0o600), None), T1).unwrap();
    assert_eq!(
        (
            changed.metadata.mode,
            changed.namespace_refs,
            changed.logical_len
        ),
        (0o600, 2, 10)
    );
    assert_eq!(
        changed.metadata.mtime_seconds,
        i64::MAX,
        "chmod keeps mtime"
    );
    assert_eq!(b.lookup(1, "alias"), Some(changed));
    // Negative fractional and extreme times are exact portable values.
    let timed = b.applied(set(8, None, Some(T2)), T1).unwrap();
    assert_eq!(
        (
            timed.metadata.mtime_seconds,
            timed.metadata.mtime_nanoseconds
        ),
        (-2, 800_000_000)
    );
    assert_eq!((timed.logical_len, timed.namespace_refs), (400_000, 4));
    let extreme = Time {
        seconds: i64::MIN,
        nanoseconds: 999_999_999,
    };
    let both = b.applied(set(1, Some(0o1700), Some(extreme)), T1).unwrap();
    assert_eq!(
        (
            both.metadata.mode,
            both.metadata.mtime_seconds,
            both.namespace_refs
        ),
        (0o1700, i64::MIN, 0)
    );
    assert_eq!(b.names(1).len(), 7);
    let state = b.overlay.state(b.route()).unwrap();
    assert!(matches!(
        b.run(set(1, Some(0o1700), Some(extreme)), T1).unwrap(),
        Outcome::Unchanged { stat: Some(_) }
    ));
    assert_eq!(b.overlay.state(b.route()).unwrap(), state);
    assert_eq!(b.refused(set(2, Some(0o4755), None)), Refusal::NotPermitted);
    assert_eq!(b.refused(set(2, Some(0o1644), None)), Refusal::NotPermitted);
    assert_eq!(b.refused(set(3, Some(0o644), None)), Refusal::Unsupported);
    assert_eq!(b.refused(set(999, Some(0o644), None)), Refusal::Missing);
    assert_eq!(
        b.refused(set(
            2,
            None,
            Some(Time {
                seconds: 0,
                nanoseconds: 1_000_000_000
            })
        )),
        Refusal::Invalid
    );
    // A symlink's time can change while its target stays the base object.
    b.applied(set(3, None, Some(T2)), T1);
    assert_eq!(
        b.window(|view| view.readlink(&b.overlay, 3))
            .unwrap()
            .as_bytes(),
        b"../.git/index"
    );
}

#[test]
fn unrepresentable_names_targets_and_reservations_are_refused_before_any_job() {
    use layerfs_content::filesystem::PathName;
    use layerfs_workspace::{InodeSerials, WorkspaceResult};
    // The canonical name and target grammars are the operation's input types:
    // nothing outside them can reach an owner job or be silently dropped.
    for invalid in [
        &b"\xff\xfe"[..],
        b"back\\slash",
        b"a/b",
        b".",
        b"..",
        b"",
        b"nul\0name",
    ] {
        assert!(PathName::from_bytes(invalid).is_err(), "{invalid:?}");
    }
    assert!(PathName::from_bytes(&[b'n'; 255]).is_ok());
    assert!(matches!(
        PathName::from_bytes(&[b'n'; 256]),
        Err(ContentError::PathLimitExceeded)
    ));
    assert!(SymlinkTarget::new(vec![b't'; 4096]).is_ok());
    assert!(SymlinkTarget::new(vec![b't'; 4097]).is_err());
    assert!(SymlinkTarget::new(b"a\0b".to_vec()).is_err());

    // A reservation outside the serial space never becomes a local serial.
    struct Reply(u64, u64);
    impl InodeSerials for Reply {
        fn reserve(&self, _: u64) -> WorkspaceResult<(u64, u64)> {
            Ok((self.0, self.1))
        }
    }
    let b = Bench::new("serials");
    for (start, count) in [(0, 8), (5, 0), (i64::MAX as u64, 2), (u64::MAX, 1)] {
        assert!(matches!(
            b.workspace.next_serial(&Reply(start, count)),
            Err(WorkspaceError::Content(ContentError::InvalidRecord(
                "inode serial reservation"
            )))
        ));
    }
    // The last representable serial is usable; a one-serial range leaves none.
    let last = Reply(i64::MAX as u64, 1);
    assert_eq!(b.workspace.next_serial(&last).unwrap(), i64::MAX as u64);
    assert_eq!(b.workspace.next_serial(&Reply(40, 2)).unwrap(), 40);
    assert_eq!(b.workspace.next_serial(&Reply(0, 0)).unwrap(), 41);
    assert!(b.workspace.next_serial(&Reply(0, 0)).is_err());
}

/// Owner ruling P-1 at component scope: the range arithmetic of the early
/// refill, with counting allocators. `main` answers a Workspace that has no
/// local range; `early` answers the early refill. Their windows are disjoint,
/// so every serial names the call that reserved it.
mod low_water {
    use super::*;
    use layerfs_workspace::{InodeSerials, WorkspaceResult};
    use std::cell::Cell;

    /// The original refusal of one counted call.
    #[derive(Debug, Eq, PartialEq)]
    struct Refused(u64);
    impl std::fmt::Display for Refused {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "refused call {}", self.0)
        }
    }
    impl std::error::Error for Refused {}

    /// Ascending windows of exactly the requested count, every call counted.
    struct Windows {
        next: Cell<u64>,
        calls: Cell<u64>,
        refuse: Cell<bool>,
    }
    impl Windows {
        fn from(start: u64) -> Self {
            Self {
                next: Cell::new(start),
                calls: Cell::new(0),
                refuse: Cell::new(false),
            }
        }
    }
    impl InodeSerials for Windows {
        fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
            self.calls.set(self.calls.get() + 1);
            assert_eq!(count, SERIAL_REFILL, "a refill asks for one window");
            if self.refuse.get() {
                return Err(WorkspaceError::Service(Box::new(Refused(self.calls.get()))));
            }
            let start = self.next.get();
            self.next.set(start + count);
            Ok((start, count))
        }
    }
    fn refusal(error: &WorkspaceError) -> Option<&Refused> {
        match error {
            WorkspaceError::Service(error) => error.downcast_ref::<Refused>(),
            _ => None,
        }
    }
    const MAIN: u64 = 5000;
    const EARLY: u64 = 9000;

    /// One call. Asserts that it made at most one attempt and returns the
    /// serial, the early failure and which allocator it called.
    fn take(
        b: &Bench,
        main: &Windows,
        early: &Windows,
        low_water: u64,
    ) -> (u64, Option<WorkspaceError>, (u64, u64)) {
        let before = (main.calls.get(), early.calls.get());
        let (serial, failure) = b
            .workspace
            .next_serial_with_low_water(main, early, low_water)
            .unwrap();
        let made = (main.calls.get() - before.0, early.calls.get() - before.1);
        assert!(made.0 + made.1 <= 1, "two attempts in one call: {made:?}");
        (serial, failure, made)
    }

    #[test]
    fn zero_low_water_never_attempts_early() {
        let b = Bench::new("low-water-zero");
        let (main, early) = (Windows::from(MAIN), Windows::from(EARLY));
        for call in 0..SERIAL_REFILL {
            let (serial, failure, made) = take(&b, &main, &early, 0);
            assert_eq!(serial, MAIN + call);
            assert!(failure.is_none());
            assert_eq!(made, (u64::from(call == 0), 0), "call {call}");
        }
        // The window is exhausted: the next call is the one exhausted attempt.
        let (serial, failure, made) = take(&b, &main, &early, 0);
        assert_eq!((serial, made), (MAIN + SERIAL_REFILL, (1, 0)));
        assert!(failure.is_none());
        println!(
            "LOW_WATER zero: calls={} main_attempts={} early_attempts={}",
            SERIAL_REFILL + 1,
            main.calls.get(),
            early.calls.get()
        );
        assert_eq!((main.calls.get(), early.calls.get()), (2, 0));
    }

    #[test]
    fn an_early_attempt_is_made_exactly_when_the_remainder_is_below_the_low_water() {
        const LOW: u64 = 1000;
        let b = Bench::new("low-water-exact");
        let (main, early) = (Windows::from(MAIN), Windows::from(EARLY));
        // Model: the unconsumed serials in the order they are handed out.
        // The remainder a refill was made for is consumed before the refill.
        let mut model: std::collections::VecDeque<u64> = Default::default();
        let mut attempts = Vec::new();
        for call in 0..2 * SERIAL_REFILL + 100 {
            let exhausted = model.is_empty();
            if exhausted {
                model.extend(MAIN..MAIN + SERIAL_REFILL);
            }
            let expected = model.pop_front().unwrap();
            let below = !exhausted && (model.len() as u64) < LOW;
            if below {
                let start = EARLY + early.calls.get() * SERIAL_REFILL;
                model.extend(start..start + SERIAL_REFILL);
            }
            let (serial, failure, made) = take(&b, &main, &early, LOW);
            assert_eq!(serial, expected, "call {call}");
            assert!(failure.is_none());
            assert_eq!(
                made,
                (u64::from(exhausted), u64::from(below)),
                "call {call}"
            );
            if made != (0, 0) {
                attempts.push((call, serial, made));
            }
        }
        println!("LOW_WATER {LOW}: (call, serial, (main, early)) of every attempt = {attempts:?}");
        // Call 0 reserved [5000, 6024). Call 24 took 5024 and left 999, the
        // first remainder below 1,000: one early window [9000, 10024), kept
        // behind the 999. Call 1048 took 9024 and left 999 again.
        assert_eq!(
            attempts,
            [
                (0, MAIN, (1, 0)),
                (24, MAIN + 24, (0, 1)),
                (1048, EARLY + 24, (0, 1)),
                (2072, EARLY + SERIAL_REFILL + 24, (0, 1)),
            ]
        );
        assert_eq!((main.calls.get(), early.calls.get()), (1, 3));
    }

    #[test]
    fn an_early_failure_is_returned_with_the_serial_and_is_not_asked_again() {
        // Just below the refill window: the second create is already below.
        const LOW: u64 = SERIAL_REFILL - 1;
        let b = Bench::new("low-water-failure");
        let (main, early) = (Windows::from(MAIN), Windows::from(EARLY));
        // No range: the one attempt is the exhausted one, 1,023 are left,
        // which is not below 1,023.
        let (serial, failure, made) = take(&b, &main, &early, LOW);
        assert_eq!((serial, made), (MAIN, (1, 0)));
        assert!(failure.is_none());

        // The early attempt is refused: the serial is still returned, with
        // the original refusal, and the call made that one attempt only.
        early.refuse.set(true);
        let (serial, failure, made) = take(&b, &main, &early, LOW);
        assert_eq!((serial, made), (MAIN + 1, (0, 1)));
        assert_eq!(refusal(failure.as_ref().unwrap()), Some(&Refused(1)));
        // A later call is a new operation with its own single attempt; the
        // serial of the refused one was consumed, not handed out again.
        let (serial, failure, made) = take(&b, &main, &early, LOW);
        assert_eq!((serial, made), (MAIN + 2, (0, 1)));
        assert_eq!(refusal(failure.as_ref().unwrap()), Some(&Refused(2)));

        // Once the allocator answers, that call's attempt extends the range,
        // and the calls after it are above the low-water again.
        early.refuse.set(false);
        let (serial, failure, made) = take(&b, &main, &early, LOW);
        assert_eq!((serial, made), (MAIN + 3, (0, 1)));
        assert!(failure.is_none());
        let (serial, failure, made) = take(&b, &main, &early, LOW);
        assert_eq!((serial, made), (MAIN + 4, (0, 0)));
        assert!(failure.is_none());
        println!(
            "LOW_WATER {LOW} with a refused early refill: main_attempts={} early_attempts={} (refused 2, answered 1)",
            main.calls.get(),
            early.calls.get()
        );
        assert_eq!((main.calls.get(), early.calls.get()), (1, 3));
        // The remainder [5005, 6024) is consumed before the early window.
        for expected in MAIN + 5..MAIN + SERIAL_REFILL {
            assert_eq!(take(&b, &main, &early, 0).0, expected);
        }
        assert_eq!(take(&b, &main, &early, 0).0, EARLY);
        assert_eq!((main.calls.get(), early.calls.get()), (1, 3));
    }

    #[test]
    fn an_exhausted_range_is_one_refused_attempt_and_no_serial() {
        // One serial per reservation, so every second call finds no range.
        struct One(Cell<u64>);
        impl InodeSerials for One {
            fn reserve(&self, _: u64) -> WorkspaceResult<(u64, u64)> {
                self.0.set(self.0.get() + 1);
                Ok((70 + self.0.get(), 1))
            }
        }
        let b = Bench::new("low-water-exhausted");
        let (one, early) = (One(Cell::new(0)), Windows::from(EARLY));
        // No range existed: no early attempt, whatever the low-water.
        let taken = b
            .workspace
            .next_serial_with_low_water(&one, &early, u64::MAX)
            .unwrap();
        assert_eq!((taken.0, one.0.get(), early.calls.get()), (71, 1, 0));
        assert!(taken.1.is_none());

        // The range is exhausted and the one attempt is refused: no serial,
        // the original refusal, and no early attempt beside it.
        let main = Windows::from(MAIN);
        main.refuse.set(true);
        let refused = b
            .workspace
            .next_serial_with_low_water(&main, &early, u64::MAX)
            .unwrap_err();
        assert_eq!(refusal(&refused), Some(&Refused(1)));
        assert_eq!((main.calls.get(), early.calls.get()), (1, 0));

        // An early reservation outside the serial space is that attempt's
        // failure; the serial the call took is still returned.
        main.refuse.set(false);
        assert_eq!(take(&b, &main, &early, 0).0, MAIN);
        struct Zero;
        impl InodeSerials for Zero {
            fn reserve(&self, _: u64) -> WorkspaceResult<(u64, u64)> {
                Ok((0, 8))
            }
        }
        let (serial, failure) = b
            .workspace
            .next_serial_with_low_water(&main, &Zero, u64::MAX)
            .unwrap();
        assert_eq!(serial, MAIN + 1);
        assert!(matches!(
            failure,
            Some(WorkspaceError::Content(ContentError::InvalidRecord(
                "inode serial reservation"
            )))
        ));
        assert_eq!(take(&b, &main, &early, 0).0, MAIN + 2);
        println!(
            "LOW_WATER exhausted: main_attempts={} early_attempts={}",
            main.calls.get(),
            early.calls.get()
        );
        assert_eq!((main.calls.get(), early.calls.get()), (2, 0));
    }
}

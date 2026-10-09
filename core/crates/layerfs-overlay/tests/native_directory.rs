//! Native directory offsets: one read-only visit per window, one row per
//! published reply, reuse on a handle that seeks back, the replies a rewind
//! retires, and bounded retirement.
use layerfs_overlay::{
    Binding, Changes, CleanupState, DirectoryEntryChange, Inode, InodeKind, MaintenanceCursor,
    NativeCookieOffer, NativeDecision, NativeDirectory, NativeDirectoryCursor, NativeDirectoryPage,
    NativeEffect, NativeMount, Overlay, OverlayError, ProfileConfig, StatementKind, PAGE_ROWS,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
struct Fixture {
    db: Option<Overlay>,
    mount: NativeMount,
    directory: NativeDirectory,
    path: PathBuf,
    names: std::cell::Cell<u64>,
}
fn inode(serial: u64, kind: InodeKind) -> Inode {
    Inode {
        serial,
        kind,
        mode: if kind == InodeKind::Directory {
            0o755
        } else {
            0o644
        },
        mtime_seconds: 1,
        mtime_nanoseconds: 0,
        nlink: 1,
        size: 0,
        inherited_cutoff: 0,
        born: 0,
        entries: 0,
        subdirs: 0,
    }
}
impl Fixture {
    /// A mounted Workspace whose root directory (serial 1) is open once.
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-native-directory-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
        let route = db.open_workspace([81; 32], [82; 32]).unwrap();
        let mount = db.create_native_mount(route, 1).unwrap();
        let directory = Self::open(&db, mount, u64::MAX);
        Self {
            db: Some(db),
            mount,
            directory,
            path,
            names: std::cell::Cell::new(0),
        }
    }
    fn open(db: &Overlay, mount: NativeMount, request: u64) -> NativeDirectory {
        let opened = db.opendir_native_visit(mount, request, 1, |_, _| {
            Ok(NativeDecision::Finished {
                inode: Some(inode(1, InodeKind::Directory)),
                value: (),
            })
        });
        assert!(matches!(opened.result, Ok(None)), "{:?}", opened.result);
        opened.directory_candidate.unwrap()
    }
    fn db(&self) -> &Overlay {
        self.db.as_ref().unwrap()
    }
    /// One more name bound in the root: a local file, or its removal.
    fn bind(&self, name: &[u8], serial: Option<u64>) {
        let count = self.names.get() + u64::from(serial.is_some());
        self.names.set(count);
        let applied = self
            .db()
            .mutate_native_visit(self.mount, 1_000_000 + count, 1, None, |rows, _| {
                let root = Inode {
                    entries: count,
                    ..inode(1, InodeKind::Directory)
                };
                let (created, inodes, binding) = match serial {
                    Some(serial) => (
                        Some(serial),
                        vec![
                            root,
                            Inode {
                                born: rows.active().number() as u64,
                                ..inode(serial, InodeKind::File)
                            },
                        ],
                        Binding::Bound {
                            serial,
                            inherited: false,
                        },
                    ),
                    None => (None, vec![root], Binding::Removed { inherited: false }),
                };
                Ok(Some((
                    Changes {
                        created,
                        inodes,
                        directory_entries: vec![DirectoryEntryChange {
                            parent: 1,
                            name: name.to_vec(),
                            binding,
                        }],
                        ..Changes::default()
                    },
                    NativeEffect::None,
                )))
            })
            .unwrap()
            .expect("published");
        self.db().reply_attempted(applied.publication).unwrap();
    }
    fn fill(&self, count: usize) {
        for index in 0..count {
            self.bind(format!("n{index:04}").as_bytes(), Some(100 + index as u64));
        }
    }
    fn read(&self, offset: u64) -> Result<NativeDirectoryPage, OverlayError> {
        self.db().read_native_directory_visit(
            self.mount,
            1,
            self.directory.owner_id(),
            offset,
            None,
        )
    }
    fn offer(&self, page: &NativeDirectoryPage) -> NativeCookieOffer {
        self.db().offer_native_cookies(page).unwrap()
    }
    fn pages(&self) -> u64 {
        self.db()
            .resources(Some(self.mount.route()))
            .unwrap()
            .counts
            .owner_details
    }
    /// One whole enumeration from the first name, reply by reply, every
    /// window name accepted: the names and the number of replies that
    /// published a row.
    fn enumerate(&self) -> (Vec<Vec<u8>>, usize) {
        self.enumerate_from(2)
    }
    /// The same from `start`: offset 0 is a rewind, offset 2 a seek back to
    /// the first name.
    fn enumerate_from(&self, start: u64) -> (Vec<Vec<u8>>, usize) {
        let (mut listed, mut offset, mut published) = (Vec::new(), start, 0);
        loop {
            let page = self.read(offset).unwrap();
            let window = names_of(&page);
            if window.is_empty() {
                return (listed, published);
            }
            let offer = self.offer(&page);
            let first = match offer.existing() {
                Some((first, names)) if names == window.as_slice() => first,
                _ => {
                    self.db().publish_native_cookies(&offer, &window).unwrap();
                    published += 1;
                    offer.first()
                }
            };
            offset = first + window.len() as u64 - 1;
            listed.extend(window);
        }
    }
    fn maintain(&self) -> Vec<u64> {
        let (mut cursor, mut rows) = (MaintenanceCursor::default(), Vec::new());
        for _ in 0..200 {
            let Some(step) = self.db().maintain(cursor).unwrap() else {
                return rows;
            };
            cursor = step.cursor;
            rows.push(step.work.rows);
        }
        panic!("maintenance did not settle: {rows:?}");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.db = None;
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn names(values: &[&str]) -> Vec<Vec<u8>> {
    values
        .iter()
        .map(|value| value.as_bytes().to_vec())
        .collect()
}
fn names_of(page: &NativeDirectoryPage) -> Vec<Vec<u8>> {
    page.local
        .active
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

#[test]
fn only_accepted_names_become_offsets_and_a_closed_handle_reads_and_publishes_nothing() {
    let f = Fixture::new();
    assert_eq!(
        f.db().retained_native_directory(f.mount, u64::MAX).unwrap(),
        Some(f.directory)
    );
    for (name, serial) in [("a", 11), ("b", 12), ("c", 13), ("d", 14)] {
        f.bind(name.as_bytes(), Some(serial));
    }
    let empty = f.pages();

    // The visit: position, parent for `..`, one window of names with kinds.
    let start = f.read(0).unwrap();
    assert_eq!(*start.cursor(), NativeDirectoryCursor::Start);
    assert_eq!((start.parent(), start.after.clone()), (Some(1), None));
    assert_eq!(names_of(&start), names(&["a", "b", "c", "d"]));
    assert_eq!(
        start.local_kinds,
        (11..15)
            .map(|serial| (serial, InodeKind::File))
            .collect::<Vec<_>>()
    );
    assert_eq!(start.source().root(), [82; 32]);
    let dot = f.read(1).unwrap();
    assert_eq!(*dot.cursor(), NativeDirectoryCursor::AfterDot);
    assert_eq!(dot.parent(), Some(1));
    let named = f.read(2).unwrap();
    assert_eq!(*named.cursor(), NativeDirectoryCursor::Names(None));
    assert_eq!(named.parent(), None);
    // Another inode, another mount and a file's serial are stale.
    let foreign = Fixture::new();
    for refused in [
        f.db()
            .read_native_directory_visit(f.mount, 2, f.directory.owner_id(), 0, None),
        f.db()
            .read_native_directory_visit(foreign.mount, 1, f.directory.owner_id(), 0, None),
        f.db()
            .read_native_directory_visit(f.mount, 1, f.directory.owner_id() + 999, 0, None),
    ] {
        assert!(matches!(refused, Err(OverlayError::Stale)), "{refused:?}");
    }

    // Two readers of the handle are offered disjoint fresh ranges. An offer
    // makes no offset valid.
    let (a, b) = (f.offer(&start), f.offer(&named));
    assert!(a.existing().is_none() && b.existing().is_none());
    assert!(a.first() >= 3 && b.first() >= a.first() + PAGE_ROWS as u64);
    assert!(matches!(f.read(a.first()), Err(OverlayError::Stale)));
    assert_eq!(f.pages(), empty);
    // Names that are not one ordered run after the reply's position, an
    // empty prefix and more than one window are refused whole.
    for bad in [
        names(&["b", "a"]),
        names(&["a", "a"]),
        names(&["."]),
        names(&["x/y"]),
        Vec::new(),
        (0..PAGE_ROWS + 1)
            .map(|index| format!("m{index:03}").into_bytes())
            .collect(),
    ] {
        assert!(matches!(
            f.db().publish_native_cookies(&a, &bad),
            Err(OverlayError::Invalid("native cookie names"))
        ));
    }
    assert_eq!(f.pages(), empty);

    // Exactly the accepted prefix of each reply becomes offsets: one row.
    f.db().publish_native_cookies(&a, &names(&["a"])).unwrap();
    f.db()
        .publish_native_cookies(&b, &names(&["a", "b"]))
        .unwrap();
    assert_eq!(f.pages(), empty + 2);
    // A second publication of one offer fails whole.
    assert!(f.db().publish_native_cookies(&a, &names(&["a"])).is_err());
    assert_eq!(f.pages(), empty + 2);
    for (offset, after, window) in [
        (a.first(), "a", vec!["b", "c", "d"]),
        (b.first(), "a", vec!["b", "c", "d"]),
        (b.first() + 1, "b", vec!["c", "d"]),
    ] {
        let page = f.read(offset).unwrap();
        assert_eq!(page.cursor().after_name(), Some(after.as_bytes()));
        assert_eq!((page.parent(), names_of(&page)), (None, names(&window)));
    }
    // Numbers of the ranges that no reply accepted are not offsets.
    for never in [a.first() + 1, b.first() + 2, b.first() + 63, a.first() - 1] {
        assert!(matches!(f.read(never), Err(OverlayError::Stale)));
    }
    // A continuation is never before the offset's own name.
    assert!(matches!(
        f.db().read_native_directory_visit(
            f.mount,
            1,
            f.directory.owner_id(),
            b.first() + 1,
            Some(b"a")
        ),
        Err(OverlayError::Invalid("native directory continuation"))
    ));
    let later = f
        .db()
        .read_native_directory_visit(f.mount, 1, f.directory.owner_id(), b.first(), Some(b"c"))
        .unwrap();
    assert_eq!(names_of(&later), names(&["d"]));

    // A handle that seeks back to its first name is offered its latest
    // reply listed after the same name; a reply after another name has
    // none. At offset 0 the offer is a rewind and reuses nothing; unpublished
    // it changes nothing.
    let again = f.offer(&f.read(2).unwrap());
    assert!(!again.rewinds());
    assert_eq!(
        again.existing(),
        Some((b.first(), names(&["a", "b"]).as_slice()))
    );
    let rewound = f.offer(&f.read(0).unwrap());
    assert!(rewound.rewinds() && rewound.existing().is_none());
    assert_eq!(
        names_of(&f.read(a.first()).unwrap()),
        names(&["b", "c", "d"])
    );
    assert!(f
        .offer(&f.read(b.first() + 1).unwrap())
        .existing()
        .is_none());

    // Names removed and created between two replies: an offset keeps meaning
    // "strictly after its name", whether or not that name is still bound.
    f.bind(b"b", None);
    f.bind(b"bb", Some(15));
    let page = f.read(b.first() + 1).unwrap();
    assert_eq!(names_of(&page), names(&["bb", "c", "d"]));
    assert_eq!(names_of(&f.read(0).unwrap()), names(&["a", "bb", "c", "d"]));

    // A second handle of the same directory has its own offsets.
    let second = Fixture::open(f.db(), f.mount, u64::MAX - 1);
    assert!(matches!(
        f.db()
            .read_native_directory_visit(f.mount, 1, second.owner_id(), b.first(), None),
        Err(OverlayError::Stale)
    ));
    let other = f
        .db()
        .read_native_directory_visit(f.mount, 1, second.owner_id(), 0, None)
        .unwrap();
    let other = f.db().offer_native_cookies(&other).unwrap();
    assert!(other.existing().is_none());
    f.db()
        .publish_native_cookies(&other, &names(&["a"]))
        .unwrap();
    // Two replies of the first handle, the second handle and its one reply.
    assert_eq!(f.pages(), empty + 4);

    // RELEASEDIR racing a READDIR: the reply prepared before the close
    // publishes nothing, and the handle's few replies are deleted with it in
    // the same job. Nothing is queued.
    let racing = f.offer(&f.read(b.first() + 1).unwrap());
    f.db()
        .close_native_directory(f.mount, 1, f.directory.owner_id())
        .unwrap();
    assert!(matches!(
        f.db().publish_native_cookies(&racing, &names(&["bb"])),
        Err(OverlayError::Stale)
    ));
    assert!(matches!(f.read(0), Err(OverlayError::Stale)));
    assert!(matches!(f.read(b.first()), Err(OverlayError::Stale)));
    assert!(f
        .db()
        .close_native_directory(f.mount, 1, f.directory.owner_id())
        .is_err());
    assert_eq!(
        f.db().retained_native_directory(f.mount, u64::MAX).unwrap(),
        None
    );
    assert_eq!(f.maintain(), Vec::<u64>::new(), "nothing was queued");
    // Only the second handle's row and its one reply are left.
    assert_eq!(f.pages(), empty + 1);
    f.db()
        .close_native_directory(f.mount, 1, second.owner_id())
        .unwrap();
    assert_eq!(f.pages(), empty - 1);

    f.db().revoke_native_mount(f.mount).unwrap();
    f.db().close(f.mount.route()).unwrap();
    assert_eq!(
        f.db().cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Held
    );
    f.maintain();
    for _ in 0..100 {
        if f.db().cleanup_state(f.mount.route()).unwrap() == CleanupState::Gone {
            break;
        }
        f.db().reclaim_closed(0).unwrap();
    }
    assert_eq!(
        f.db().cleanup_state(f.mount.route()).unwrap(),
        CleanupState::Gone
    );
    let counts = f.db().resources(None).unwrap().counts;
    assert_eq!(
        (
            counts.owner_details,
            counts.owner_rows,
            counts.source_rows,
            counts.namespaces
        ),
        (0, 0, 0, 0)
    );
}

#[test]
fn a_handle_that_seeks_back_reuses_its_replies_and_a_long_handle_retires_in_bounded_indexed_turns()
{
    let f = Fixture::new();
    let plans = f.db().explain_native_directory(f.directory).unwrap();
    println!("NATIVE_DIRECTORY_PLANS {plans:#?}");
    for label in [
        "cookie-offset",
        "cookie-window",
        "cookie-retire",
        "cookie-after",
        "names-kinds",
        "open-fence",
    ] {
        assert!(
            plans.iter().any(|plan| plan.starts_with(label)),
            "{label}: {plans:?}"
        );
    }
    assert!(
        plans
            .iter()
            .all(|plan| !plan.contains("SCAN") && !plan.contains("TEMP B-TREE")),
        "{plans:?}"
    );
    assert!(
        plans
            .iter()
            .any(|plan| plan.contains("cookie-after") && plan.contains("native_cookie_after")),
        "{plans:?}"
    );
    assert!(
        plans
            .iter()
            .any(|plan| plan.contains("open-fence") && plan.contains("native_directory_open")),
        "{plans:?}"
    );
    f.fill(2 * PAGE_ROWS + 2);
    let before = f.db().diagnostics();
    let empty = f.pages();

    // Three replies list 130 names; each reply is one row.
    let (listed, published) = f.enumerate();
    assert_eq!(listed.len(), 2 * PAGE_ROWS + 2);
    assert!(listed.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!((published, f.pages()), (3, empty + 3));
    // A handle that seeks back to its first name lists the same names at the
    // same offsets and writes nothing, however often.
    let begun = f.db().diagnostics().statements[StatementKind::Begin as usize].executions;
    for _ in 0..3 {
        assert_eq!(f.enumerate(), (listed.clone(), 0));
    }
    assert_eq!(f.pages(), empty + 3);
    assert_eq!(
        f.db().diagnostics().statements[StatementKind::Begin as usize].executions,
        begun
    );
    // One name more in the middle: the replies from there on are listed
    // again under fresh offsets, once; the next seek back reuses them.
    f.bind(b"n0064a", Some(9000));
    let (changed, published) = f.enumerate();
    assert_eq!((changed.len(), published), (2 * PAGE_ROWS + 3, 2));
    assert_eq!(f.enumerate(), (changed, 0));
    assert_eq!(f.pages(), empty + 5);

    // The work of one visit does not depend on how many names it lists.
    let cost = |offset: u64| {
        let before = f.db().diagnostics();
        let page = f.read(offset).unwrap();
        let offer = f.offer(&page);
        let work = f.db().diagnostics().since(&before);
        let family = |kind: StatementKind| work.statements[kind as usize].attempts;
        (
            names_of(&page).len(),
            offer
                .existing()
                .map(|(first, names)| first + names.len() as u64 - 1),
            [
                family(StatementKind::Workspace),
                family(StatementKind::Lease),
                family(StatementKind::Inode),
                family(StatementKind::DirectoryEntry),
                family(StatementKind::Begin),
            ],
        )
    };
    let (count, reused, first) = cost(0);
    // The fence, the parent, the directory's row, the window, and the reply
    // listed from the start, which makes the offer a rewind that reuses none.
    assert_eq!((count, reused, first), (PAGE_ROWS, None, [1, 2, 1, 1, 0]));
    let (count, last, start) = cost(2);
    // The fence, the directory's row, the window, the reuse.
    assert_eq!((count, start), (PAGE_ROWS, [1, 1, 1, 1, 0]));
    let (count, last, middle) = cost(last.unwrap());
    // The fence, the offset's reply, the directory's row, the window, the reuse.
    assert_eq!((count, middle), (PAGE_ROWS, [1, 2, 1, 1, 0]));
    let (count, _, tail) = cost(last.unwrap());
    assert_eq!((count, tail), (3, [1, 2, 1, 1, 0]));

    // A handle with more replies than its RELEASEDIR deletes itself: the
    // header is marked closed and bounded indexed turns retire the rows.
    let mut offset = 2;
    for _ in 0..12 {
        let page = f.read(offset).unwrap();
        let offer = f.offer(&page);
        let one = &names_of(&page)[..1];
        // Every reply here differs from the handle's latest after its name.
        assert!(offer.existing().is_none_or(|(_, names)| names != one));
        f.db().publish_native_cookies(&offer, one).unwrap();
        offset = offer.first();
    }
    assert_eq!(f.pages(), empty + 17);
    f.db()
        .close_native_directory(f.mount, 1, f.directory.owner_id())
        .unwrap();
    assert!(matches!(f.read(0), Err(OverlayError::Stale)));
    assert!(f
        .db()
        .retained_native_directory(f.mount, u64::MAX)
        .unwrap()
        .is_some());
    let turns = f.maintain();
    assert_eq!(turns, [8, 8, 1, 1], "two windows, the last row, the header");
    assert_eq!(
        f.db().retained_native_directory(f.mount, u64::MAX).unwrap(),
        None
    );
    assert_eq!(f.pages(), empty - 1);
    assert_eq!(
        f.db().native_lookup_count(f.mount, 1).unwrap(),
        Some(0),
        "mount remains live"
    );
    let work = f.db().diagnostics().since(&before);
    assert_eq!(work.total().fullscan_steps, 0);
    assert_eq!(work.total().sorts, 0);
    assert_eq!(work.total().autoindex_rows, 0);
    assert_eq!(work.total().reprepares, 0);
    f.db().revoke_native_mount(f.mount).unwrap();
}

#[test]
fn a_handle_read_from_offset_0_again_stores_one_listing_however_often_it_is_rewound() {
    let f = Fixture::new();
    f.fill(10 * PAGE_ROWS);
    let before = f.db().diagnostics();
    let empty = f.pages();
    let (listed, published) = f.enumerate_from(0);
    assert_eq!((listed.len(), published), (10 * PAGE_ROWS, 10));
    assert_eq!(f.pages(), empty + 10);
    // Five rewinds, each after one more name that sorts before every other:
    // every window's boundary moves, so every reply of every listing differs
    // from the one before it. The handle still stores one listing.
    for round in 0..5_u64 {
        f.bind(format!("a{round:03}").as_bytes(), Some(9000 + round));
        let (listed, published) = f.enumerate_from(0);
        assert_eq!(listed.len(), 10 * PAGE_ROWS + 1 + round as usize);
        assert!(listed.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(published, 11, "round {round}");
        assert_eq!(f.pages(), empty + 11, "round {round}");
    }
    // A rewind over an unchanged directory stores its one listing again.
    for _ in 0..3 {
        assert_eq!(f.enumerate_from(0).1, 11);
        assert_eq!(f.pages(), empty + 11);
    }
    // The earlier replies go a bounded window with each reply published
    // after the rewind: 11 rows, then 11 - 8 + 1, then 4 - 3 + 1, then 3.
    let mut offset = 0;
    for stored in [4, 2, 3] {
        let page = f.read(offset).unwrap();
        let offer = f.offer(&page);
        assert!(offer.existing().is_none());
        let window = names_of(&page);
        f.db().publish_native_cookies(&offer, &window).unwrap();
        offset = offer.first() + window.len() as u64 - 1;
        assert_eq!(f.pages(), empty + stored);
    }
    let work = f.db().diagnostics().since(&before);
    assert_eq!(work.total().fullscan_steps, 0);
    assert_eq!(work.total().sorts, 0);
    assert_eq!(work.total().autoindex_rows, 0);
    assert_eq!(work.total().reprepares, 0);
    // What a rewind left is released with the handle: more rows than its
    // RELEASEDIR deletes are retired by the closed handle's item.
    for _ in 0..3 {
        f.enumerate_from(0);
    }
    f.bind(b"a999", Some(9999));
    let page = f.read(0).unwrap();
    let offer = f.offer(&page);
    f.db()
        .publish_native_cookies(&offer, &names_of(&page))
        .unwrap();
    assert_eq!(f.pages(), empty + 11 - 8 + 1);
    let page = f.read(0).unwrap();
    let offer = f.offer(&page);
    f.db()
        .publish_native_cookies(&offer, &names_of(&page))
        .unwrap();
    assert_eq!(f.pages(), empty + 4 - 4 + 1);
    f.enumerate_from(0);
    f.bind(b"a998", Some(9998));
    let page = f.read(0).unwrap();
    let offer = f.offer(&page);
    f.db()
        .publish_native_cookies(&offer, &names_of(&page))
        .unwrap();
    assert_eq!(f.pages(), empty + 11 - 8 + 1);
    f.db()
        .close_native_directory(f.mount, 1, f.directory.owner_id())
        .unwrap();
    assert_eq!(f.maintain(), Vec::<u64>::new(), "four rows: deleted inline");
    assert_eq!(f.pages(), empty - 1);
    f.db().revoke_native_mount(f.mount).unwrap();
}

#[test]
fn an_offset_from_before_a_rewind_is_refused_and_later_offsets_resume_after_their_names() {
    let f = Fixture::new();
    f.fill(20 * PAGE_ROWS);
    let empty = f.pages();
    // One reply, accepted whole, at `offset`: its first cookie.
    let reply = |offset: u64| {
        let page = f.read(offset).unwrap();
        let offer = f.offer(&page);
        let window = names_of(&page);
        f.db().publish_native_cookies(&offer, &window).unwrap();
        (offer.first(), window)
    };
    let (first, window) = reply(0);
    let (second, _) = reply(first + PAGE_ROWS as u64 - 1);
    let old = [first, first + 5, first + 63, second, second + 63];
    f.enumerate();
    assert_eq!(f.pages(), empty + 20);
    for offset in old {
        assert!(matches!(
            f.read(offset).unwrap().cursor(),
            NativeDirectoryCursor::Names(Some(_))
        ));
    }
    // A reading visit at offset 0 writes nothing: without its publication
    // every offset stands and every row stays.
    let unpublished = f.offer(&f.read(0).unwrap());
    assert!(unpublished.rewinds());
    assert_eq!(
        f.read(first + 5).unwrap().cursor().after_name(),
        Some(window[5].as_slice())
    );
    assert_eq!(f.pages(), empty + 20);

    // The rewind: the reply listed from offset 0 is published. Every earlier
    // offset is refused from then on, whether its row is deleted yet or not.
    let later = f.read(second).unwrap();
    let stale = f.offer(&later);
    let (third, again) = reply(0);
    assert_eq!(again, window);
    assert!(third >= unpublished.first() + PAGE_ROWS as u64);
    assert_eq!(f.pages(), empty + 20 - 8 + 1);
    for offset in old {
        let page = f.read(offset).unwrap();
        assert_eq!(*page.cursor(), NativeDirectoryCursor::Rewound);
        assert!(names_of(&page).is_empty() && page.parent().is_none());
    }
    // Offers made before the rewind was published write nothing.
    for (offer, names) in [(&unpublished, &window), (&stale, &names_of(&later))] {
        assert!(matches!(
            f.db().publish_native_cookies(offer, names),
            Err(OverlayError::Stale)
        ));
    }
    assert_eq!(f.pages(), empty + 20 - 8 + 1);
    // A number never handed out is Stale at or above the floor; below it
    // nothing is told apart.
    assert!(matches!(f.read(third + 64), Err(OverlayError::Stale)));
    assert!(matches!(
        f.read(unpublished.first()).unwrap().cursor(),
        NativeDirectoryCursor::Rewound
    ));

    // Offsets handed out since the rewind resume strictly after their names,
    // also by a seek back, and a reply listed again is reused, not stored.
    assert_eq!(
        f.read(third + 9).unwrap().cursor().after_name(),
        Some(window[9].as_slice())
    );
    let (fourth, next) = reply(third + 63);
    assert_eq!(next[0], format!("n{:04}", PAGE_ROWS).into_bytes());
    assert_eq!(f.pages(), empty + 13 - 8 + 1);
    let back = f.read(third + 63).unwrap();
    assert_eq!(names_of(&back), next);
    let offer = f.offer(&back);
    assert!(!offer.rewinds());
    assert_eq!(offer.existing(), Some((fourth, next.as_slice())));
    let from_first_name = f.offer(&f.read(2).unwrap());
    assert!(!from_first_name.rewinds());
    assert_eq!(from_first_name.existing(), Some((third, window.as_slice())));
    f.bind(b"n0063a", Some(8000));
    assert_eq!(
        names_of(&f.read(third + 63).unwrap())[0],
        b"n0063a".to_vec(),
        "strictly after n0063, whatever was bound since"
    );
    assert_eq!(f.pages(), empty + 6, "reading stores nothing");

    // A reply that accepted no name still publishes its rewind: the floor
    // moves and a window of earlier replies goes; no row is added.
    let page = f.read(0).unwrap();
    let offer = f.offer(&page);
    assert!(offer.rewinds());
    f.db().publish_native_cookies(&offer, &[]).unwrap();
    assert_eq!(f.pages(), empty);
    for offset in [third, third + 63, fourth] {
        assert_eq!(
            *f.read(offset).unwrap().cursor(),
            NativeDirectoryCursor::Rewound
        );
    }
    // No reply is left, so the next reading from offset 0 is a first one.
    let fresh = f.offer(&f.read(0).unwrap());
    assert!(!fresh.rewinds() && fresh.existing().is_none());
    assert!(matches!(
        f.db().publish_native_cookies(&fresh, &[]),
        Err(OverlayError::Invalid("native cookie names"))
    ));

    // Another handle of the same directory keeps its own offsets and floor.
    let other = Fixture::open(f.db(), f.mount, u64::MAX - 1);
    let read = |offset: u64| {
        f.db()
            .read_native_directory_visit(f.mount, 1, other.owner_id(), offset, None)
    };
    let theirs = f.db().offer_native_cookies(&read(0).unwrap()).unwrap();
    f.db()
        .publish_native_cookies(&theirs, &window[..3])
        .unwrap();
    reply(0);
    reply(0);
    assert_eq!(
        read(theirs.first() + 2).unwrap().cursor().after_name(),
        Some(window[2].as_slice())
    );

    // A closed handle with rows left by a rewind is retired like any other:
    // 20 replies, a rewind that deletes 8 and adds 1, then RELEASEDIR.
    let mut offset = 0;
    for _ in 0..20 {
        offset = reply(offset).0 + 63;
    }
    reply(0);
    let held = f.pages();
    f.db()
        .close_native_directory(f.mount, 1, f.directory.owner_id())
        .unwrap();
    assert_eq!(f.maintain(), [8, 5, 1], "13 rows and the header");
    assert_eq!(f.pages(), held - 14);
    f.db()
        .close_native_directory(f.mount, 1, other.owner_id())
        .unwrap();
    f.db().revoke_native_mount(f.mount).unwrap();
}

/// The cookie storage of schema 22, which this replaces: one row per listed
/// name and one index entry per listed name.
const BEFORE: &str = "CREATE TABLE native_cookie (
    ns INTEGER NOT NULL,
    owner INTEGER NOT NULL CHECK(owner>0),
    cookie INTEGER NOT NULL CHECK(cookie>=3),
    name BLOB NOT NULL CHECK(length(name)>0 AND length(name)<=255),
    PRIMARY KEY(ns,owner,cookie)
) STRICT, WITHOUT ROWID;
CREATE INDEX native_cookie_name ON native_cookie(ns,owner,name,cookie);";
const NOW: &str = "CREATE TABLE native_cookie (
    ns INTEGER NOT NULL,
    owner INTEGER NOT NULL CHECK(owner>0),
    first_cookie INTEGER NOT NULL CHECK(first_cookie>=3),
    after BLOB NOT NULL CHECK(length(after)<=255),
    names BLOB NOT NULL CHECK(length(names)>=2 AND length(names)<=16384),
    PRIMARY KEY(ns,owner,first_cookie)
) STRICT, WITHOUT ROWID;
CREATE INDEX native_cookie_after ON native_cookie(ns,owner,after,first_cookie);";

#[test]
fn a_directory_listed_once_stores_fewer_rows_and_bytes_than_a_row_and_an_index_entry_per_name() {
    for count in [10_usize, 4000] {
        let mut f = Fixture::new();
        f.fill(count);
        let empty = f.pages();
        let (listed, published) = f.enumerate();
        assert_eq!(listed.len(), count);
        let replies = count.div_ceil(PAGE_ROWS);
        assert_eq!((published, f.pages() - empty), (replies, replies as u64));
        let (owner, ns) = (f.directory.owner_id() as i64, 1_i64);
        // The engine's own rows, read from its closed file.
        f.db = None;
        let raw = rusqlite::Connection::open(f.path.join("overlay")).unwrap();
        let mut statement = raw
            .prepare("SELECT first_cookie,after,names FROM native_cookie WHERE owner=?1 ORDER BY first_cookie")
            .unwrap();
        let rows: Vec<(i64, Vec<u8>, Vec<u8>)> = statement
            .query_map([owner], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows.len(), replies);
        let page_size: i64 = raw
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .unwrap();
        // The same names stored both ways in an empty database of the same
        // page size: entries of both b-trees, their payload bytes and the
        // pages they occupy.
        let stored = |schema: &str, fill: &dyn Fn(&rusqlite::Connection)| {
            let scratch = rusqlite::Connection::open_in_memory().unwrap();
            scratch
                .execute_batch(&format!("PRAGMA page_size={page_size}; {schema}"))
                .unwrap();
            let bare: i64 = scratch
                .query_row("PRAGMA page_count", [], |row| row.get(0))
                .unwrap();
            fill(&scratch);
            let pages: i64 = scratch
                .query_row("PRAGMA page_count", [], |row| row.get(0))
                .unwrap();
            pages - bare
        };
        let before_pages = stored(BEFORE, &|scratch| {
            for (index, name) in listed.iter().enumerate() {
                scratch
                    .execute(
                        "INSERT INTO native_cookie VALUES(?1,?2,?3,?4)",
                        rusqlite::params![ns, owner, 3 + index as i64, name],
                    )
                    .unwrap();
            }
        });
        let now_pages = stored(NOW, &|scratch| {
            for (first, after, names) in &rows {
                scratch
                    .execute(
                        "INSERT INTO native_cookie VALUES(?1,?2,?3,?4,?5)",
                        rusqlite::params![ns, owner, first, after, names],
                    )
                    .unwrap();
            }
        });
        let name_bytes: usize = listed.iter().map(Vec::len).sum();
        // Before: every name in its row and again in its index entry.
        let (before_entries, before_bytes) = (2 * count, 2 * name_bytes);
        // Now: every name once with one length byte; the name a reply was
        // listed after in its row and again in its index entry.
        let after_bytes: usize = rows.iter().map(|(_, after, _)| after.len()).sum();
        let names_bytes: usize = rows.iter().map(|(_, _, names)| names.len()).sum();
        assert_eq!(names_bytes, name_bytes + count);
        let (now_entries, now_bytes) = (2 * replies, names_bytes + 2 * after_bytes);
        println!(
            "COOKIE_STORAGE names={count} before: entries={before_entries} name_bytes={before_bytes} pages={before_pages} now: entries={now_entries} name_bytes={now_bytes} pages={now_pages} page_size={page_size}"
        );
        assert!(now_entries < before_entries && now_bytes < before_bytes);
        assert!(now_pages <= before_pages);
        if count > PAGE_ROWS {
            assert!(now_pages < before_pages);
        }
    }
}

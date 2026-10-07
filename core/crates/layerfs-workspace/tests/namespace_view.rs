//! Public S4 proofs: enumeration under mutation, generations and known
//! install, and operations whose owner rounds interleave with other work.
mod common;
mod harness;
use common::{file, metadata, name};
use harness::*;
use layerfs_content::filesystem::{
    update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    InodeUpdate,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ObjectId;
use layerfs_workspace::{Refusal, WorkspaceError};
use std::collections::BTreeSet;

#[test]
fn enumeration_returns_every_persistent_name_once_across_mutation_and_resume() {
    let b = Bench::new("enumerate");
    let dir = b.applied(mkdir(1, "many"), T1).unwrap().serial;
    for n in 0..200 {
        b.applied(create(dir, &format!("n{n:03}")), T1);
    }
    let page = |after: Option<&[u8]>| b.window(|view| view.list(&b.overlay, dir, after)).unwrap();
    let first = page(None);
    assert_eq!((first.entries.len(), first.visited), (64, 64));
    let mut seen: Vec<String> = first
        .entries
        .iter()
        .map(|(name, _)| String::from_utf8(name.clone()).unwrap())
        .collect();
    // A reply consumed only up to its tenth entry resumes exactly after it.
    let partial = page(Some(first.entries[9].0.as_slice()));
    assert_eq!(partial.entries[..54], first.entries[10..]);
    assert_eq!(partial.entries[54].0, b"n064");

    // Between windows: remove a returned name and an unvisited one, create
    // names before and after the cursor, and rename an unvisited name.
    b.applied(unlink(dir, "n010"), T2);
    b.applied(unlink(dir, "n100"), T2);
    b.applied(create(dir, "n005-late"), T2);
    b.applied(create(dir, "zz-late"), T2);
    b.applied(rename((dir, "n150"), (dir, "n150-renamed"), true, None), T2);
    let mut after = first.continuation.clone();
    while let Some(cursor) = after {
        let next = page(Some(&cursor));
        assert!(next.visited <= 64);
        seen.extend(
            next.entries
                .iter()
                .map(|(name, _)| String::from_utf8(name.clone()).unwrap()),
        );
        after = next.continuation;
    }
    let unique: BTreeSet<&String> = seen.iter().collect();
    assert_eq!(unique.len(), seen.len(), "no name is returned twice");
    for n in 0..200 {
        let persistent = format!("n{n:03}");
        let expected = !matches!(n, 100 | 150);
        assert_eq!(seen.contains(&persistent), expected, "{persistent}");
    }
    assert!(seen.contains(&"zz-late".to_string()) && seen.contains(&"n150-renamed".to_string()));
    assert!(
        !seen.contains(&"n005-late".to_string()),
        "behind the cursor"
    );
    // A fresh enumeration is the exact current membership, in byte order.
    let (all, widest) = b.list(dir);
    assert_eq!(all.len(), 200);
    assert!(widest <= 64);
    assert!(all
        .windows(2)
        .all(|pair| pair[0].0.as_bytes() < pair[1].0.as_bytes()));
    assert!(all.iter().all(|(name, _)| name != "n010" && name != "n100"));
}

#[test]
fn name_and_inode_counts_have_no_cap_and_every_window_stays_bounded() {
    let b = Bench::new("counts");
    let dir = b.applied(mkdir(1, "wide"), T1).unwrap().serial;
    let demand = b.demand();
    let rounds = b.rounds.get();
    const FILES: usize = 3000;
    for n in 0..FILES {
        b.applied(create(dir, &format!("f{n:05}")), T1);
    }
    assert_eq!(
        b.rounds.get() - rounds,
        FILES as u64,
        "one owner round each"
    );
    assert_eq!(b.demand(), demand);
    // Three reservations of the refill window covered 3,001 serials.
    assert_eq!(
        b.allocator.calls.load(std::sync::atomic::Ordering::Relaxed),
        3
    );
    let (all, widest) = b.list(dir);
    assert_eq!(all.len(), FILES);
    assert_eq!(widest, 64);
    let serials: BTreeSet<u64> = all.iter().map(|(_, serial)| *serial).collect();
    assert_eq!(serials.len(), FILES, "every inode has its own serial");
    assert_eq!(b.refused(rmdir(1, "wide")), Refusal::NotEmpty);
    for n in 0..FILES {
        b.applied(unlink(dir, &format!("f{n:05}")), T2);
    }
    b.applied(rmdir(1, "wide"), T2);
    assert_eq!(b.lookup(1, "wide"), None);
    // Removed inside their creating generation: no name row is left behind.
    assert_eq!(
        b.overlay.state(b.route()).unwrap().dirty_directory_entries,
        0
    );
}

#[test]
fn known_install_folds_a_capture_and_keeps_later_namespace_changes() {
    // No immutable-object cache: every base access is visible upstream demand.
    let b = Bench::with_cache("install", 0);
    let built = b.applied(mkdir(1, "built"), T1).unwrap().serial;
    let object = b.applied(create(built, "out.o"), T1).unwrap().serial;
    b.applied(unlink(1, "alias"), T1);
    let capture = b.overlay.capture(b.route()).unwrap();

    // After the seal, the captured generation is immutable lower state.
    let quiet = b.demand();
    assert_eq!(b.lookup(built, "absent"), None);
    assert_eq!(b.demand(), quiet, "still above the installed floor");
    b.applied(unlink(built, "out.o"), T2);
    let later = b.applied(create(built, "later"), T2).unwrap().serial;
    b.applied(create(1, "alias"), T2);
    let sealed: Vec<_> = b
        .overlay
        .captured_directory_entries(capture, None)
        .unwrap()
        .into_iter()
        .map(|row| (row.parent, row.name, row.serial))
        .collect();
    assert_eq!(
        sealed,
        vec![
            (1, b"alias".to_vec(), None),
            (1, b"built".to_vec(), Some(built)),
            (built, b"out.o".to_vec(), Some(object)),
        ]
    );

    // The host publishes exactly the captured state as the next root.
    let store = &b.fixture.store;
    let directory = InodeValue {
        kind: InodeKind::Directory,
        content_root: ObjectId::for_bytes(b"new-directory"),
        metadata_root: metadata(store, InodeKind::Directory),
        namespace_ref_count: 0,
    };
    let regular = InodeValue {
        kind: InodeKind::RegularFile,
        content_root: file(store, b""),
        metadata_root: metadata(store, InodeKind::RegularFile),
        namespace_ref_count: 0,
    };
    let inodes = [
        InodeUpdate {
            serial: built,
            value: directory,
        },
        InodeUpdate {
            serial: object,
            value: regular,
        },
    ];
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("alias"), None), (name("built"), Some(built))],
        },
        DirectoryUpdate {
            parent: built,
            changes: vec![(name("out.o"), Some(object))],
        },
    ];
    let created = [built, object];
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let next = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(b.fixture.root),
            scope: b.fixture.scope,
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &created,
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let prepared = b.workspace.prepare_base_install(capture, next).unwrap();
    b.workspace
        .install_prepared_base(&b.overlay, prepared)
        .unwrap();

    // The view is unchanged: later removals, creations and re-creations stay,
    // now over a base that itself binds the installed names.
    assert_eq!(b.lookup(built, "out.o"), None);
    assert_eq!(b.lookup(built, "later").unwrap().serial, later);
    assert_eq!(b.names(built), vec!["later"]);
    let alias = b.lookup(1, "alias").unwrap();
    assert_ne!(alias.serial, 2);
    assert_eq!(b.lookup(1, "file").unwrap().namespace_refs, 1);
    let demand = b.demand();
    assert_eq!(b.lookup(built, "absent"), None);
    assert!(b.demand() > demand, "an installed directory is inherited");
    // Exact counts survive the fold: one visible entry, then none.
    assert_eq!(b.refused(rmdir(1, "built")), Refusal::NotEmpty);
    b.applied(unlink(built, "later"), T1);
    b.applied(rmdir(1, "built"), T1);
    assert_eq!(b.lookup(1, "built"), None);
    // The serial is inherited now, so removal keeps a whiteout and the name
    // can be bound again to a new inode.
    let again = b.applied(mkdir(1, "built"), T2).unwrap();
    assert_ne!(again.serial, built);
    assert_eq!(b.names(again.serial), Vec::<String>::new());
}

#[test]
fn a_rebound_name_between_owner_rounds_is_resolved_before_the_one_attempt() {
    let b = Bench::new("rebound");
    // unlink(alias) first learns it needs base facts for the root and for
    // `alias -> 2`. Before its second round another complete operation moves
    // the inherited symlink over that name.
    *b.between.borrow_mut() = Some(Box::new(|b: &Bench| {
        b.applied(rename((1, "symlink"), (1, "alias"), true, None), T2);
    }));
    let rounds = b.rounds.get();
    b.applied(unlink(1, "alias"), T1);
    // Own rounds: facts, the rebound target's fact, then the publication. The
    // interleaved rename took two more.
    assert_eq!(b.rounds.get() - rounds, 5);
    // The removed binding is the one visible at the attempt: the symlink.
    assert_eq!(b.lookup(1, "alias"), None);
    assert_eq!(b.lookup(1, "symlink"), None);
    assert_eq!(b.stat(3).unwrap().namespace_refs, 0);
    let file = b.lookup(1, "file").unwrap();
    assert_eq!((file.serial, file.namespace_refs), (2, 1));
    assert_eq!(
        b.names(1),
        vec![".git", "cache", "file", "node_modules", "output"]
    );
    // Two tickets were published and both reply attempts are settled.
    assert_eq!(b.overlay.state(b.route()).unwrap().revision, 2);
    assert!(b.overlay.capture_ready(b.route()).unwrap());
}

#[test]
fn opposing_directory_moves_cannot_form_a_cycle_between_owner_rounds() {
    let b = Bench::new("cycle");
    // cache -> output/c is evaluated; before it publishes, output moves under
    // cache. The destination ancestry is re-proved in the publishing round.
    *b.between.borrow_mut() = Some(Box::new(|b: &Bench| {
        b.applied(rename((1, "output"), (6, "o"), true, path(&["cache"])), T2);
    }));
    assert!(matches!(
        b.run(rename((1, "cache"), (7, "c"), true, path(&["output"])), T1),
        Err(WorkspaceError::Refused(Refusal::AncestryMismatch))
    ));
    // Only the interleaved move was published.
    assert_eq!(b.overlay.state(b.route()).unwrap().revision, 1);
    // With the destination's current path the move is the cycle it would be.
    assert_eq!(
        b.refused(rename((1, "cache"), (7, "c"), true, path(&["cache", "o"]))),
        Refusal::Invalid
    );
    assert_eq!(b.lookup(1, "cache").unwrap().serial, 6);
    assert_eq!(b.lookup(6, "o").unwrap().serial, 7);
    assert_eq!(b.lookup(7, "result").unwrap().serial, 8);
    assert_eq!(b.names(6), vec!["o", "state"]);
}

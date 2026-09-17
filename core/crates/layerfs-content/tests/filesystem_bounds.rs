//! Whole-operation bounds: live owners, changed-path work and read waves.
//!
//! These cases establish what an operation *does not* touch as much as what it
//! does: a small change in a large tree reads its own path and the siblings that
//! prove the partition, a released subtree is traversed in bounded pages, and the
//! declared ceilings cover every owner the operation reports. Internal scratch
//! counters are not presented as a whole-operation memory bound.

mod support;

use std::collections::BTreeSet;

use layerfs_content::filesystem::{
    build_filesystem, update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemRead, FilesystemResources, FilesystemRootId, InodeUpdate, LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ObjectId, ObjectRole};
use support::filesystem::{
    count_role, resources, synthetic, value, CountingProvider, RecordingBacking, Session, TempDir,
    TreeStore,
};

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn regular(content: &str) -> InodeValue {
    value(
        InodeKind::RegularFile,
        synthetic(content),
        synthetic("bounds/meta"),
    )
}

fn dir_value() -> InodeValue {
    value(
        InodeKind::Directory,
        synthetic("bounds/unused"),
        synthetic("bounds/dir-meta"),
    )
}

/// A tree with `per_directory` files in `/d`, `extra` sibling directories, and
/// `depth` nested directories below `/d/deep`.
struct Wide {
    session: Session,
    directory: u64,
    serials: Vec<u64>,
    siblings: Vec<u64>,
    deep: Vec<u64>,
}

fn wide(per_directory: usize, extra: usize, depth: usize) -> Wide {
    let mut session = Session::new(1).expect("empty");
    let directory = session.allocate();
    let serials = (0..per_directory)
        .map(|_| session.allocate())
        .collect::<Vec<_>>();
    let siblings = (0..extra).map(|_| session.allocate()).collect::<Vec<_>>();
    let deep = (0..depth).map(|_| session.allocate()).collect::<Vec<_>>();
    let mut inodes = vec![InodeUpdate {
        serial: directory,
        value: dir_value(),
    }];
    let mut new = vec![directory];
    for (index, serial) in serials.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: regular(&format!("bounds/content-{index:04}")),
        });
        new.push(*serial);
    }
    for (index, serial) in siblings.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: dir_value(),
        });
        new.push(*serial);
        let _ = index;
    }
    for serial in &deep {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: dir_value(),
        });
        new.push(*serial);
    }
    inodes.sort_by_key(|update| update.serial);
    new.sort_unstable();
    let mut root_changes = vec![(name("d"), Some(directory))];
    for (index, serial) in siblings.iter().enumerate() {
        root_changes.push((name(&format!("s{index:03}")), Some(*serial)));
    }
    root_changes.sort_by(|left, right| left.0.cmp(&right.0));
    let mut directories = vec![DirectoryUpdate {
        parent: 1,
        changes: root_changes,
    }];
    let mut changes = serials
        .iter()
        .enumerate()
        .map(|(index, serial)| (name(&format!("f{index:04}")), Some(*serial)))
        .collect::<Vec<_>>();
    if let Some(first) = deep.first() {
        changes.push((name("deep"), Some(*first)));
        changes.sort_by(|left, right| left.0.cmp(&right.0));
    }
    directories.push(DirectoryUpdate {
        parent: directory,
        changes,
    });
    for (position, serial) in deep.iter().enumerate() {
        let mut nested = Vec::new();
        if let Some(next) = deep.get(position + 1) {
            nested.push((name("deep"), Some(*next)));
        }
        directories.push(DirectoryUpdate {
            parent: *serial,
            changes: nested,
        });
    }
    directories.sort_by_key(|update| update.parent);
    session
        .apply(&directories, &inodes, &new)
        .expect("wide tree");
    Wide {
        session,
        directory,
        serials,
        siblings,
        deep,
    }
}

#[test]
fn an_empty_tree_and_a_deep_tree_both_stay_inside_their_declared_ceilings() {
    // Empty: one root directory, no base reads, no ordering work.
    let empty = TreeStore::new();
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: Vec::new(),
    }];
    let inodes = [InodeUpdate {
        serial: 1,
        value: dir_value(),
    }];
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x71; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[1],
        resources: resources(),
    };
    let provider = CountingProvider::new(&empty);
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        build_filesystem(&mut objects, &input, None)
    }
    .expect("empty build");
    assert_eq!(
        provider.demands(),
        0,
        "an initial build reads no stored page"
    );
    assert_eq!(result.counters.references.rows_spilled, 0);
    assert_eq!(result.counters.references.runs.runs_created, 0);
    assert_eq!(result.counters.references.runs.peak_run_bytes, 0);
    assert_eq!(
        result.counters.directories.peak_scratch_bytes, 0,
        "an empty initial directory needs no merge scratch"
    );

    // Deep: eight nested directories, and the operation still fits its ceilings.
    let deep = wide(4, 1, 8);
    let depth = deep.deep.len() as u64;
    assert_eq!(depth, 8);
    let mut session = deep.session;
    let target = *deep.deep.last().expect("deepest");
    let file = session.allocate();
    let temp = TempDir::new("bounds-deep");
    let mut backing = RecordingBacking::new(temp.path());
    let directories = [DirectoryUpdate {
        parent: target,
        changes: vec![(name("f"), Some(file))],
    }];
    let inodes = [InodeUpdate {
        serial: file,
        value: regular("bounds/deep-content"),
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[file],
        resources: resources(),
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("deep update")
    };
    assert!(
        result.counters.directories.pages_read <= depth + 2,
        "a deep update reads its own path, not the tree: {} pages for depth {depth}",
        result.counters.directories.pages_read
    );
    assert!(
        result.counters.inodes.pages_read <= 2,
        "one inode leaf holds this tree"
    );
    assert!(
        provider.peak_wave() <= 32,
        "demand waves stay inside the grouped width"
    );
    assert!(
        result.counters.directories.peak_scratch_bytes
            + result.counters.inodes.peak_scratch_bytes
            + usize::try_from(result.counters.references.runs.peak_run_bytes).unwrap_or(usize::MAX)
            <= input.resources.scratch_bytes
                + usize::try_from(input.resources.ordering_bytes).unwrap_or(usize::MAX),
        "every owner the operation reports fits the declared ceilings"
    );
    assert_eq!(backing.counters().releases, 1);
    assert!(!backing.owns_storage());
}

/// Applies the same three-name rename to one tree and reports what it cost.
fn rename_three(wide: Wide) -> (u64, u64, BTreeSet<ObjectId>, Vec<(InodeValue, u64)>) {
    let session = wide.session;
    let sibling_contents = wide
        .siblings
        .iter()
        .map(|serial| {
            let record = session
                .read()
                .expect("reader")
                .lookup_inodes(&[*serial])
                .expect("lookup")[0]
                .expect("record");
            (record, *serial)
        })
        .collect::<Vec<_>>();
    let temp = TempDir::new("bounds-localized");
    let mut backing = RecordingBacking::new(temp.path());
    let mut changes = (0..3)
        .map(|index| (name(&format!("f{index:04}")), None))
        .collect::<Vec<_>>();
    changes.extend((0..3).map(|index| (name(&format!("r{index:04}")), Some(wide.serials[index]))));
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    let directories = [DirectoryUpdate {
        parent: wide.directory,
        changes,
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("localized update")
    };
    assert_eq!(
        result.counters.directories.change_keys, 6,
        "the merge consumed exactly the supplied changes"
    );
    assert_eq!(
        result.counters.references.rows_touched, 4,
        "reference rows are bounded by the changed bindings and their directory"
    );
    assert_eq!(result.counters.references.rows_spilled, 0);
    assert_eq!(
        result.counters.references.serials_scanned,
        result.counters.references.rows_touched
    );
    assert_eq!(backing.counters().releases, 1);
    assert!(!backing.owns_storage());
    (
        result.counters.directories.pages_read,
        result.counters.objects.objects_read,
        provider.demanded(),
        sibling_contents,
    )
}

#[test]
fn a_small_change_reads_its_own_path_and_never_an_unrelated_subtree() {
    // The same three-name rename against a 500-entry and a 4,000-entry directory:
    // the work must follow the path and the parent fanout, not the tree size.
    let small = rename_three(wide(500, 12, 1));
    let large = rename_three(wide(4_000, 12, 1));
    // Work follows the format's page occupancy, not the entry count: a 100-row
    // inode leaf and a ~450-row directory leaf are the units a verified merge
    // acquires, and the parent's children are its required siblings.
    for (entries, pages, demanded) in [(500_u64, small.0, small.1), (4_000, large.0, large.1)] {
        let inode_leaves = entries / 100 + 1;
        let directory_leaves = entries / 400 + 1;
        assert!(
            demanded <= 3 * (inode_leaves + directory_leaves) + 8,
            "{entries} entries: {demanded} demanded objects exceed the page-derived bound"
        );
        assert!(
            pages <= directory_leaves + 4,
            "{entries} entries: {pages} directory pages exceed the leaf count"
        );
    }
    assert!(
        large.1 < 4_000 / 16,
        "demand stays far below the entry count: {} objects for 4,000 entries",
        large.1
    );
    for (record, serial) in &large.3 {
        assert!(
            !large.2.contains(&record.content_root),
            "sibling directory {serial} was read by an unrelated change"
        );
    }
}

#[test]
fn released_subtrees_are_traversed_in_bounded_pages_and_aliases_survive() {
    // 600 files under /d/sub, one of them also bound at /kept, /d removed.
    let mut session = Session::new(1).expect("empty");
    let directory = session.allocate();
    let sub = session.allocate();
    let kept_target = session.allocate();
    let files = (0..600).map(|_| session.allocate()).collect::<Vec<_>>();
    let mut inodes = vec![
        InodeUpdate {
            serial: directory,
            value: dir_value(),
        },
        InodeUpdate {
            serial: sub,
            value: dir_value(),
        },
        InodeUpdate {
            serial: files[0],
            value: regular("bounds/kept-content"),
        },
    ];
    let mut new = vec![directory, sub, files[0]];
    for (index, serial) in files.iter().enumerate().skip(1) {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: regular(&format!("bounds/released-{index:04}")),
        });
        new.push(*serial);
    }
    inodes.sort_by_key(|update| update.serial);
    new.sort_unstable();
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("d"), Some(directory))],
                },
                DirectoryUpdate {
                    parent: directory,
                    changes: vec![(name("sub"), Some(sub))],
                },
                DirectoryUpdate {
                    parent: sub,
                    changes: files
                        .iter()
                        .enumerate()
                        .map(|(index, serial)| (name(&format!("f{index:04}")), Some(*serial)))
                        .collect(),
                },
            ],
            &inodes,
            &new,
        )
        .expect("subtree");
    let _ = kept_target;

    let temp = TempDir::new("bounds-release");
    let mut backing = RecordingBacking::new(temp.path());
    // Move one file up to the root and delete /d in one operation.
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("d"), None), (name("kept"), Some(files[0]))],
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("subtree removal")
    };
    assert_eq!(
        result.counters.release.released, 601,
        "the removed directory plus its 600 children"
    );
    assert!(
        result.counters.release.pages <= 601 / 64 + 4,
        "release pages are bounded by the released entries: {}",
        result.counters.release.pages
    );
    assert_eq!(
        result.counters.release.peak_depth, 2,
        "the removed directory and its released subdirectory are both traversed"
    );
    assert_eq!(
        result.counters.references.final_removals, 601,
        "every released inode is absent from the new table"
    );
    assert_eq!(
        result.counters.references.serials_scanned, 3,
        "the touched-serial collection is sized by the supplied changes (the root \
         directory, the removed directory and the moved child), not by the subtree"
    );
    assert!(
        result.counters.references.rows_touched >= result.counters.release.released,
        "every released inode contributed an effect row"
    );
    assert!(
        result.counters.references.rows_touched
            <= result.counters.references.serials_scanned + result.counters.release.released,
        "and the only other rows are the supplied changes: {} rows for {} released",
        result.counters.references.rows_touched,
        result.counters.release.released
    );
    // The moved-out alias survives with one binding.
    let mut merged = session.store.clone();
    merged.absorb(&sink);
    let mut read =
        FilesystemRead::new(&merged, FilesystemRootId(result.root.0)).expect("new root readable");
    let kept = read
        .stat(&LogicalPath::new("kept").unwrap())
        .expect("the moved-out child survives");
    assert_eq!(kept.namespace_ref_count, 1);
    assert_eq!(kept.content_root, synthetic("bounds/kept-content"));
    assert!(merged.role(session.root).is_some());
    assert_eq!(backing.counters().releases, 1);
    assert!(!backing.owns_storage());
}

#[test]
fn every_reported_owner_is_nonzero_where_work_happened_and_zero_where_it_did_not() {
    let wide = wide(900, 2, 2);
    let session = wide.session;
    let temp = TempDir::new("bounds-owners");
    let mut backing = RecordingBacking::new(temp.path());
    // Force ordering runs so every owner is exercised at once.
    // A real change: thirty names are renamed, so the merge, the inode table and
    // the root all change.
    let mut changes = (0..30)
        .map(|index| (name(&format!("f{index:04}")), None))
        .collect::<Vec<_>>();
    changes.extend((0..30).map(|index| (name(&format!("g{index:04}")), Some(wide.serials[index]))));
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    let directories = [DirectoryUpdate {
        parent: wide.directory,
        changes,
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources {
            maximum_pending_records: 1,
            ..resources()
        },
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("owners update")
    };
    let observed = backing.counters();
    let references = result.counters.references;
    assert!(references.rows_spilled > 0, "runs were really exercised");
    assert_eq!(references.runs.runs_created, observed.creates);
    assert!(references.runs.merges > 0);
    assert!(
        references.runs.peak_live_runs >= 2,
        "an old and a new run coexist during a merge"
    );
    assert!(references.runs.rows_written >= observed.appends);
    assert!(
        references.runs.rows_read > 0,
        "consolidation reads its runs"
    );
    assert!(
        references.runs.peak_run_bytes >= ROW_FLOOR,
        "simultaneous ordering bytes are reported: {}",
        references.runs.peak_run_bytes
    );
    assert!(references.peak_pending >= 1);
    assert!(result.counters.directories.peak_scratch_bytes > 0);
    assert!(result.counters.inodes.peak_scratch_bytes > 0);
    assert!(result.counters.objects.bytes_emitted > 0);
    assert!(result.counters.objects.objects_emitted > 0);
    assert_eq!(
        result.counters.release.released, 0,
        "no inode was released by this update"
    );
    assert_eq!(result.counters.release.pages, 0);
    assert_eq!(result.counters.references.final_removals, 0);
    assert_eq!(
        result.counters.references.serials_scanned, 31,
        "the one collection holds the thirty renamed inodes and their directory"
    );
    assert!(
        result.counters.references.serials_scanned <= result.counters.references.rows_touched,
        "and never more than the rows this operation touched"
    );
    assert_eq!(count_role(&sink, ObjectRole::FilesystemRoot), 1);
    assert_eq!(backing.counters().releases, 1);
    assert!(!backing.owns_storage());
    assert_eq!(backing.held_bytes(), 0);
}

/// Smallest plausible simultaneous ordering bytes for a run-backed update.
const ROW_FLOOR: u64 = 96;

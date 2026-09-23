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
    count_role, resources, synthetic, value, with_objects, CountingProvider, RecordingBacking,
    Session, TempDir, TreeStore,
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
        provider.peak_wave() <= 256,
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

#[test]
fn validation_reads_are_charged_to_the_operation() {
    // The checks read the base they are about to change. Those reads belong to
    // the operation, so its counters must show them instead of reporting zero
    // while the checks work through pages.
    let (mut session, d, _e, _f) = nested();
    let extra = session.allocate();
    let result = session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("n"), Some(extra))],
                },
                DirectoryUpdate {
                    parent: d,
                    changes: vec![(name("m"), Some(extra))],
                },
            ],
            &[InodeUpdate {
                serial: extra,
                value: regular("bounds/validation"),
            }],
            &[extra],
        )
        .expect("a second name for a regular file is legal");
    let validation = result.counters.validation;
    assert!(
        validation.objects_read > 0,
        "the checks read base records: {validation:?}"
    );
    assert!(
        validation.inode_demands > 0,
        "the checks demanded inode records: {validation:?}"
    );
    assert!(
        validation.read_waves > 0,
        "the checks issued read waves: {validation:?}"
    );
}

/// Builds `/d/e` and `/d/f` as a fresh tree and returns the session with the
/// directory serial, the child directory serial and the file serial.
fn nested() -> (Session, u64, u64, u64) {
    let mut session = Session::new(1).expect("empty");
    let d = session.allocate();
    let e = session.allocate();
    let f = session.allocate();
    session
        .apply(
            &[
                DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("d"), Some(d))],
                },
                DirectoryUpdate {
                    parent: d,
                    changes: vec![(name("e"), Some(e)), (name("f"), Some(f))],
                },
                DirectoryUpdate {
                    parent: e,
                    changes: Vec::new(),
                },
            ],
            &[
                InodeUpdate {
                    serial: d,
                    value: dir_value(),
                },
                InodeUpdate {
                    serial: e,
                    value: dir_value(),
                },
                InodeUpdate {
                    serial: f,
                    value: regular("bounds/f"),
                },
            ],
            &[d, e, f],
        )
        .expect("nested tree");
    (session, d, e, f)
}

#[test]
fn a_backing_too_small_for_the_declared_ceiling_is_refused_up_front() {
    // The declared ordering ceiling is the operation's own promise. A backing
    // whose own ceiling is smaller cannot hold it, so the operation fails before
    // any spill discovers it halfway through.
    let mut session = Session::new(1).expect("empty");
    let extra = session.allocate();
    let temp = TempDir::new("bounds-ordering-capacity");
    let mut backing = RecordingBacking::with_capacity(temp.path(), 4 * 1024);
    let resources = FilesystemResources {
        ordering_bytes: 32 * 1024 * 1024,
        ..resources()
    };
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &[DirectoryUpdate {
            parent: 1,
            changes: vec![(name("x"), Some(extra))],
        }],
        inodes: &[InodeUpdate {
            serial: extra,
            value: regular("bounds/capacity"),
        }],
        new_inodes: &[extra],
        resources,
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let outcome = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing))
    };
    assert!(
        matches!(
            outcome,
            Err(layerfs_content::ContentError::ResourceUnavailable {
                what: "ordering backing capacity"
            })
        ),
        "a backing that cannot hold the declared ceiling must be refused: {outcome:?}"
    );
    assert!(
        !backing.owns_storage(),
        "a refused operation owns no ordering storage"
    );
}

#[test]
fn merge_inputs_and_output_are_covered_by_the_declared_ceiling() {
    // A spill merges older tiers into the new batch. Every input, the output and
    // the batch exist at once, and the operation's own accounting has to cover
    // all of them: it may not count only the surviving run.
    let mut session = Session::new(1).expect("empty");
    let mut changes = Vec::new();
    let mut inodes = Vec::new();
    let mut new_inodes = Vec::new();
    for index in 0..64_u64 {
        let serial = session.allocate();
        changes.push((name(&format!("f{index:04}")), Some(serial)));
        inodes.push(InodeUpdate {
            serial,
            value: regular(&format!("bounds/content-{index}")),
        });
        new_inodes.push(serial);
    }
    new_inodes.sort_unstable();
    let temp = TempDir::new("bounds-ordering-owned");
    let mut backing = RecordingBacking::with_capacity(temp.path(), 8 * 1024 * 1024);
    let resources = FilesystemResources {
        // Small enough that the pending map really spills, large enough to hold
        // the rows of every tier plus the merge output.
        maximum_pending_records: 8,
        ordering_bytes: 4 * 1024 * 1024,
        ..resources()
    };
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &[DirectoryUpdate { parent: 1, changes }],
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources,
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing))
    }
    .expect("a declared ceiling large enough for the work");
    let references = result.counters.references;
    assert!(
        references.rows_spilled > 0,
        "the operation must really spill: {references:?}"
    );
    assert!(
        references.runs.peak_run_bytes > 0,
        "the reported peak must cover the runs: {references:?}"
    );
    assert!(
        references.runs.peak_run_bytes <= resources.ordering_bytes,
        "the reported peak may not exceed the declared ceiling: {references:?}"
    );
    assert!(
        backing.peak_bytes() <= resources.ordering_bytes,
        "the physical owner stayed inside the declared ceiling: {} > {}",
        backing.peak_bytes(),
        resources.ordering_bytes
    );
}

#[test]
fn a_build_has_no_independent_cycle_walk_limit() {
    // A base-less build walks only its supplied bindings. The existing-tree
    // cycle walk still has its separate work limit, but this build has no
    // arbitrary count refusal and charges every supplied entry.
    let limit = layerfs_content::filesystem::validate::MAXIMUM_CYCLE_CHECK_ENTRIES;
    let scope = layerfs_content::filesystem::scope_for_seed([0x6d; 32]);
    let temp = TempDir::new("bounds-cycle-limit");
    let mut backing = RecordingBacking::with_capacity(temp.path(), 64 << 20);
    let store = TreeStore::new();
    let mut build_wide = |entries: usize, scope| {
        let mut changes = Vec::new();
        let mut inodes = vec![InodeUpdate {
            serial: 1,
            value: dir_value(),
        }];
        let mut new_inodes = vec![1_u64];
        for index in 0..entries {
            let serial = index as u64 + 2;
            changes.push((name(&format!("f{index:05}")), Some(serial)));
            inodes.push(InodeUpdate {
                serial,
                value: regular(&format!("bounds/cycle-{index}")),
            });
            new_inodes.push(serial);
        }
        inodes.sort_by_key(|update| update.serial);
        new_inodes.sort_unstable();
        let input = FilesystemInput {
            base: None,
            scope,
            root_serial: 1,
            directories: &[DirectoryUpdate { parent: 1, changes }],
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: resources(),
        };
        let provider = store.clone();
        let mut sink = TreeStore::new();
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        build_filesystem(&mut objects, &input, Some(&mut backing))
    };
    let built = build_wide(limit - 8, scope).expect("a directory just under the limit");
    assert!(
        built.counters.validation.entries_examined > 0,
        "the walk's entries are charged to the build: {:?}",
        built.counters.validation
    );
    // The old boundary is still charged as work, and a larger build succeeds.
    let at_limit = build_wide(
        limit,
        layerfs_content::filesystem::scope_for_seed([0x6f; 32]),
    )
    .expect("a build stating exactly the ceiling is accepted");
    assert_eq!(
        at_limit.counters.validation.entries_examined, limit as u64,
        "the accepted boundary build charges exactly the ceiling's entries: {:?}",
        at_limit.counters.validation
    );
    let beyond = build_wide(
        limit + 1,
        layerfs_content::filesystem::scope_for_seed([0x70; 32]),
    )
    .expect("one binding over the former limit builds");
    assert_eq!(
        beyond.counters.validation.entries_examined,
        (limit + 1) as u64
    );
    let _ = backing.cleanup_failed();
}

/// R2-F7: the declared entry ceiling also caps one operation's rebinding work.
///
/// The ceiling bounds **each** whole-tree walk an operation performs, and a rename
/// of a directory walks that directory's effective subtree. A directory whose
/// subtree exceeds the ceiling therefore cannot be rebound at all, however small
/// the change is - and a tree that large is reachable, because several operations
/// can grow it one under-limit step at a time. Both halves are pinned here: the
/// second operation grows the directory past the ceiling and is accepted, and the
/// rename of the directory it produced is refused by the ceiling.
#[test]
fn a_directory_whose_subtree_exceeds_the_entry_ceiling_cannot_be_rebound() {
    let limit = layerfs_content::filesystem::validate::MAXIMUM_CYCLE_CHECK_ENTRIES;
    let mut session = Session::new(1).expect("empty");
    let parent = session.allocate();
    let child = session.allocate();

    // Step one: the base tree. One operation stays under the ceiling, so it is
    // accepted, and the directory it binds holds `limit` entries.
    let mut directories = vec![DirectoryUpdate {
        parent: 1,
        changes: vec![(name("d"), Some(parent))],
    }];
    let mut inodes = vec![
        InodeUpdate {
            serial: parent,
            value: dir_value(),
        },
        InodeUpdate {
            serial: child,
            value: dir_value(),
        },
    ];
    let mut new_inodes = vec![parent, child];
    let mut changes = Vec::new();
    for index in 0..(limit - 8) {
        let serial = session.allocate();
        changes.push((name(&format!("f{index:05}")), Some(serial)));
        inodes.push(InodeUpdate {
            serial,
            value: regular(&format!("bounds/rebind-{index}")),
        });
        new_inodes.push(serial);
    }
    directories.push(DirectoryUpdate { parent, changes });
    directories.push(DirectoryUpdate {
        parent: child,
        changes: Vec::new(),
    });
    inodes.sort_by_key(|update| update.serial);
    new_inodes.sort_unstable();
    let built = session
        .apply(&directories, &inodes, &new_inodes)
        .expect("the first operation stays under the ceiling");
    assert!(
        built.counters.validation.entries_examined <= limit as u64,
        "one operation never exceeds the ceiling it declares: {:?}",
        built.counters.validation
    );

    // Step two: grow the same directory past the ceiling. Its own subtree is not
    // walked, so this operation is accepted and the directory now holds more
    // entries than the ceiling.
    let mut directories = Vec::new();
    let mut inodes = Vec::new();
    let mut new_inodes = Vec::new();
    let mut changes = Vec::new();
    for index in 0..32 {
        let serial = session.allocate();
        changes.push((name(&format!("g{index:05}")), Some(serial)));
        inodes.push(InodeUpdate {
            serial,
            value: regular(&format!("bounds/grow-{index}")),
        });
        new_inodes.push(serial);
    }
    directories.push(DirectoryUpdate { parent, changes });
    inodes.sort_by_key(|update| update.serial);
    new_inodes.sort_unstable();
    session
        .apply(&directories, &inodes, &new_inodes)
        .expect("growing a large directory does not walk it");

    // Step three: rename the directory. The rename is small, but the effective
    // cycle check walks the directory it rebinds, so the ceiling refuses it with
    // the limit's own message rather than reporting a cycle.
    let directories = vec![DirectoryUpdate {
        parent: 1,
        changes: vec![(name("d"), None), (name("moved"), Some(parent))],
    }];
    let outcome = session.apply(&directories, &[], &[]);
    assert!(
        matches!(
            outcome,
            Err(layerfs_content::ContentError::InvalidRecord(
                "cycle check work limit"
            ))
        ),
        "a rename of a directory over the ceiling is refused by that ceiling, not \
         reported as a cycle: {outcome:?}"
    );
    let _ = child;
}

/// R30: the release frontier reads its base records in waves, not one per demand.
///
/// The frontier used to ask the provider for one inode record per serial it
/// popped, and it did so *after* the page wave that had just read that same record,
/// so a released subtree paid the record twice. A directory holding many small
/// subdirectories is the shape that shows it: every subdirectory the frontier
/// reaches is one extra read. Here the parent holds 64 subdirectories of one file
/// each, so the wave reads 64 records for the parent's page and one for each
/// subdirectory page - 128 - where the per-demand form reads 192.
#[test]
fn the_release_frontier_reads_one_wave_per_batch_not_one_record_per_demand() {
    const WIDTH: usize = 64;
    let mut session = Session::new(1).expect("empty");
    let parent = session.allocate();
    let subs = (0..WIDTH).map(|_| session.allocate()).collect::<Vec<_>>();
    let leaves = (0..WIDTH).map(|_| session.allocate()).collect::<Vec<_>>();
    let mut inodes = vec![InodeUpdate {
        serial: parent,
        value: dir_value(),
    }];
    let mut new = vec![parent];
    for (sub, leaf) in subs.iter().zip(leaves.iter()) {
        inodes.push(InodeUpdate {
            serial: *sub,
            value: dir_value(),
        });
        inodes.push(InodeUpdate {
            serial: *leaf,
            value: regular("bounds/frontier-leaf"),
        });
        new.push(*sub);
        new.push(*leaf);
    }
    inodes.sort_by_key(|update| update.serial);
    new.sort_unstable();
    let mut updates = vec![DirectoryUpdate {
        parent: 1,
        changes: vec![(name("p"), Some(parent))],
    }];
    updates.push(DirectoryUpdate {
        parent,
        changes: subs
            .iter()
            .enumerate()
            .map(|(index, serial)| (name(&format!("s{index:03}")), Some(*serial)))
            .collect(),
    });
    for (index, (sub, leaf)) in subs.iter().zip(leaves.iter()).enumerate() {
        updates.push(DirectoryUpdate {
            parent: *sub,
            changes: vec![(name(&format!("f{index:03}")), Some(*leaf))],
        });
    }
    session
        .apply(&updates, &inodes, &new)
        .expect("wide frontier");

    let temp = TempDir::new("bounds-frontier");
    let mut backing = RecordingBacking::new(temp.path());
    let removal = [DirectoryUpdate {
        parent: 1,
        changes: vec![(name("p"), None)],
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &removal,
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&session.store, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("subtree removal")
    };
    let release = result.counters.release;
    assert_eq!(
        release.released,
        (2 * WIDTH) as u64,
        "the subdirectories the parent bound and the files they bind"
    );
    // The traversal pushes a cursor for every frontier entry before the finished
    // one is popped, so the live depth here is the parent plus each subdirectory
    // that was waiting. That accumulation is pre-existing and unchanged; what this
    // case pins is the read count below.
    assert!(
        release.peak_depth <= WIDTH + 1,
        "cursor depth {} exceeds the frontier it came from",
        release.peak_depth
    );
    println!(
        "release: released {} pages {} base_records {}",
        release.released, release.pages, release.base_records
    );
    // Every record the frontier used was already read by the page wave that found
    // the inode, so a released subtree costs one base record per released inode and
    // nothing more. The per-demand form pays one extra record for every
    // subdirectory it walks into - 64 more here.
    assert!(
        release.base_records <= release.released + 1,
        "the frontier paid a read per demand: {} records for {} released inodes",
        release.base_records,
        release.released
    );
}

#[test]
fn one_batched_demand_is_exactly_one_read_wave() {
    // A grouped demand is one authenticated wave that returns many objects. The
    // boundary charges it as one wave, never two: `read_waves` is the number of
    // provider calls an operation made, and a receipt reads it that way.
    let mut store = TreeStore::new();
    let ids = (0..3)
        .map(|index| store.insert(ObjectRole::DirectoryLeaf, vec![index as u8; 16]))
        .collect::<Vec<_>>();
    let mut sink = TreeStore::new();
    let mut objects = FilesystemObjects::new(&store, &mut sink);
    let values = objects.read_batch(&ids).expect("one grouped wave");
    assert_eq!(values.len(), 3, "every demanded object is returned");
    let work = objects.work();
    assert_eq!(
        work.read_waves, 1,
        "one grouped demand is one wave, not two: {} waves for one call",
        work.read_waves
    );
    assert_eq!(work.objects_read, 3, "the wave's objects are all charged");
    assert_eq!(
        work.bytes_read, 48,
        "and only the bytes the provider actually returned"
    );
    // The point-read path keeps charging one object and one wave per call, so the
    // two paths agree on what a wave is.
    let mut objects = FilesystemObjects::new(&store, &mut sink);
    objects.read(ids[0]).expect("one point read");
    assert_eq!(objects.work().read_waves, 1);
    assert_eq!(objects.work().objects_read, 1);
}

/// Runs the three-name rename over a 4,000-entry directory (the wide shape) and
/// reports what the boundary charged plus the provider's own demand shapes.
struct WideRename {
    pages_read: u64,
    objects_read: u64,
    boundary_waves: u64,
    waves: Vec<usize>,
    demanded: usize,
    /// Root the operation emitted.
    root: ObjectId,
    /// Widest demand wave the provider served.
    peak_wave: usize,
}

impl WideRename {
    /// Objects returned by grouped demand waves (a wave wider than one), which is
    /// exactly the work `batch_children` does.
    fn grouped_objects(&self) -> usize {
        self.waves.iter().copied().filter(|width| *width > 1).sum()
    }

    /// Objects returned by single-object waves (point reads).
    fn point_objects(&self) -> usize {
        self.waves.iter().copied().filter(|width| *width == 1).sum()
    }
}

/// Runs the three-name rename over a 4,000-entry directory (the wide shape) under
/// the default resource ceilings.
fn wide_rename_charges() -> WideRename {
    wide_rename_charges_with(resources())
}

/// The same rename under caller-supplied ceilings, reporting what the boundary
/// charged so a counter test can read the page count and the provider's own
/// demand shapes.
fn wide_rename_charges_with(ceilings: FilesystemResources) -> WideRename {
    let Wide {
        session,
        directory,
        serials,
        ..
    } = wide(4_000, 12, 1);
    let (root, scope) = (session.root, session.scope);
    let temp = TempDir::new("bounds-batched-charges");
    let mut backing = RecordingBacking::new(temp.path());
    let mut changes = (0..3)
        .map(|index| (name(&format!("f{index:04}")), None))
        .collect::<Vec<_>>();
    changes.extend((0..3).map(|index| (name(&format!("r{index:04}")), Some(serials[index]))));
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    let directories = [DirectoryUpdate {
        parent: directory,
        changes,
    }];
    let input = FilesystemInput {
        base: Some(FilesystemRootId(root)),
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: ceilings,
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let mut report = WideRename {
        pages_read: 0,
        objects_read: 0,
        boundary_waves: 0,
        waves: Vec::new(),
        demanded: 0,
        root: ObjectId::for_bytes(b"unset"),
        peak_wave: 0,
    };
    {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        let result = update_filesystem(&mut objects, &input, Some(&mut backing))
            .expect("wide batched update");
        report.pages_read = result.counters.directories.pages_read;
        report.objects_read = result.counters.objects.objects_read;
        report.boundary_waves = result.counters.objects.read_waves;
        report.root = result.root.0;
    }
    report.waves = provider.waves();
    report.demanded = provider.demands();
    report.peak_wave = provider.peak_wave();
    report
}

#[test]
fn a_branch_materialization_reads_children_in_one_wave() {
    // A tree with more inode leaves than one branch page holds forces a level-1
    // merge while it is built: the merge materializes two branch pages and reads
    // their children. Those children used to be point-read one per wave; P1-3
    // reads them in bounded groups instead.
    let mut session = Session::new(1).expect("empty");
    let directory = session.allocate();
    let count = 13_000_usize;
    let serials = (0..count).map(|_| session.allocate()).collect::<Vec<_>>();
    let mut inodes = vec![InodeUpdate {
        serial: directory,
        value: dir_value(),
    }];
    let mut new = vec![directory];
    for (index, serial) in serials.iter().enumerate() {
        inodes.push(InodeUpdate {
            serial: *serial,
            value: regular(&format!("mat/content-{index:05}")),
        });
        new.push(*serial);
    }
    inodes.sort_by_key(|update| update.serial);
    new.sort_unstable();
    let root_changes = vec![(name("d"), Some(directory))];
    let mut directories = vec![DirectoryUpdate {
        parent: 1,
        changes: root_changes,
    }];
    let mut changes = serials
        .iter()
        .enumerate()
        .map(|(index, serial)| (name(&format!("m{index:05}")), Some(*serial)))
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    directories.push(DirectoryUpdate {
        parent: directory,
        changes,
    });
    directories.sort_by_key(|update| update.parent);

    // Phase one stores the base into the session's own store, so the second phase
    // reads the pages this phase emitted.
    let temp = TempDir::new("bounds-materialize");
    let mut backing = RecordingBacking::new(temp.path());
    let build_input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new,
        resources: resources(),
    };
    let built = with_objects(&mut session.store, |objects| {
        update_filesystem(objects, &build_input, Some(&mut backing))
    })
    .expect("wide inode build");
    let base = built.root.0;
    let inode_root = layerfs_content::filesystem::inode::decode_inode_page(
        session
            .store
            .canonical(built.value.inode_table())
            .expect("inode root bytes"),
    )
    .expect("inode root page");
    assert_eq!(
        inode_root.level(),
        2,
        "each parent lookup reads three inode pages"
    );

    // Phase two: unbind the lowest 300 serials. Their inode rows are released, so
    // the inode tree loses three leaves and one stored level-1 branch page drops
    // below the 64-child fill rule. The merge that repairs it materializes two
    // stored branch pages and reads their children — the path P1-3 batches.
    let released = serials[..300]
        .iter()
        .enumerate()
        .map(|(index, _)| (name(&format!("m{index:05}")), None))
        .collect::<Vec<_>>();
    let input = FilesystemInput {
        base: Some(FilesystemRootId(base)),
        scope: session.scope,
        root_serial: 1,
        directories: &[DirectoryUpdate {
            parent: directory,
            changes: released,
        }],
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("release update")
    };
    // Before P1-3 this update issued 170 waves: the release's own batch (50) plus
    // two 64-child materializations, one of which the merge already batched and
    // the other of which `page_from_wire` point-read child by child. After it,
    // both materializations are one wave each. Bounded parent reuse now also
    // eliminates the second directory-parent lookup: one level-2 root, one
    // level-1 branch and one leaf. That removes exactly three point waves and
    // three objects, without changing either materialization or the final root.
    let waves = provider.waves();
    let wide = waves
        .iter()
        .copied()
        .filter(|width| *width > 32)
        .collect::<Vec<_>>();
    assert_eq!(
        wide,
        vec![50, 64, 64],
        "the release batch and both stored branch pages are grouped waves: {waves:?}"
    );
    assert_eq!(
        waves.len(),
        104,
        "107 after P1-3, minus the reused parent's three-page second lookup"
    );
    assert_eq!(
        result.counters.inodes.read_waves, 5,
        "the inode engine's own waves: 68 before P1-3"
    );
    assert_eq!(
        provider.demands(),
        300,
        "303 before bounded parent reuse, minus three repeated inode pages"
    );
    assert_eq!(
        result.counters.inodes.pages_read, 134,
        "and the same pages are read"
    );
    assert_eq!(
        result.root.0.to_string(),
        "78a3b9026cd9d295067684d267051a7ba9246dc9eec7cb6350639dfd4525c259",
        "and the emitted root is the pre-change value"
    );
}

#[test]
fn a_wide_branch_page_reads_its_children_in_one_wave() {
    // A 4,000-entry tree's inode branch has more than 32 children, so the width
    // decides how many waves its children cost. Measured on this fixture:
    //
    //   width 32  (before P1-1)  ... 32, 32, 16   -> 29 waves, boundary 6
    //   width 256 (this commit)  ... 80           -> 27 waves, boundary 4
    //
    // and the work itself is unchanged: the same 119 objects are demanded, 96 are
    // read and the directory engine decodes the same 15 pages. Only the grouping
    // moved, which is what makes this a wave-width change and not a work change.
    let report = wide_rename_charges();
    let wide: Vec<usize> = report
        .waves
        .iter()
        .copied()
        .filter(|width| *width > 32)
        .collect();
    assert_eq!(
        wide,
        vec![80],
        "the 80-child branch is one wave, not ceil(80/32): {:?}",
        report.waves
    );
    assert_eq!(
        report.peak_wave, 80,
        "the widest wave is the branch's own child count"
    );
    // `provider.demands()` is deliberately **not** pinned to a total here: the
    // provider is shared with validation, whose own lookups are demanded through
    // it, and P1-4's record memo legitimately lowers that total without touching
    // the work this test is about. What the operation itself read is charged to
    // its own counters, pinned on either side of this assertion.
    assert!(
        report.demanded >= report.objects_read as usize,
        "every object the operation read was demanded from the provider: {} demands          for {} objects",
        report.demanded,
        report.objects_read
    );
    assert_eq!(report.objects_read, 96, "and the same objects are read");
    assert_eq!(
        report.pages_read, 15,
        "and the same directory pages are decoded"
    );
    assert_eq!(
        report.boundary_waves, 4,
        "four charged waves where the 32-wide batch charged six"
    );
}

#[test]
fn narrowing_keeps_a_small_scratch_working() {
    // A width the operation cannot afford must narrow, never refuse. Under a
    // 512 KiB scratch lease the 80-child branch cannot be reserved at once, so
    // the batch narrows to what fits — and the operation still produces the same
    // root. The reservation arithmetic is one child slot of
    // `MAXIMUM_PAGE_BYTES + associations` (8,280 B) plus one decode slot, so a
    // wave that fits the lease cannot reserve more than `scratch / 8,280` slots.
    let narrow = wide_rename_charges_with(FilesystemResources {
        scratch_bytes: 512 * 1024,
        ..resources()
    });
    let wide = wide_rename_charges();
    assert_eq!(
        narrow.root, wide.root,
        "narrowing changes the reservation, never the result"
    );
    assert_eq!(
        narrow.demanded, wide.demanded,
        "and never the objects demanded"
    );
    assert!(
        narrow.peak_wave < wide.peak_wave,
        "the batch narrowed: {} against {}",
        narrow.peak_wave,
        wide.peak_wave
    );
    assert!(
        narrow.peak_wave * (8_192 + 88) <= 512 * 1024,
        "a wave of {} children cannot reserve {} bytes inside a 512 KiB lease",
        narrow.peak_wave,
        narrow.peak_wave * (8_192 + 88)
    );
}

/// Renames every file of a `k`-entry directory and reports the validation's work.
fn binding_demands(k: usize) -> (u64, u64, u64, ObjectId) {
    let Wide {
        session,
        directory,
        serials,
        ..
    } = wide(k, 0, 0);
    let (root, scope) = (session.root, session.scope);
    let temp = TempDir::new("bounds-binding-demands");
    let mut backing = RecordingBacking::new(temp.path());
    let mut changes = serials
        .iter()
        .enumerate()
        .flat_map(|(index, serial)| {
            [
                (name(&format!("f{index:04}")), None),
                (name(&format!("r{index:04}")), Some(*serial)),
            ]
        })
        .collect::<Vec<_>>();
    changes.sort_by(|left, right| left.0.cmp(&right.0));
    let input = FilesystemInput {
        base: Some(FilesystemRootId(root)),
        scope,
        root_serial: 1,
        directories: &[DirectoryUpdate {
            parent: directory,
            changes,
        }],
        inodes: &[],
        new_inodes: &[],
        resources: resources(),
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("rename every file")
    };
    let validation = result.counters.validation;
    (
        validation.inode_demands,
        validation.read_waves,
        validation.inode_pages_read,
        result.root.0,
    )
}

#[test]
fn binding_lookups_are_batched_per_phase() {
    // An update that binds k stored children demands k base records. Before P1-4
    // each demand was its own root descent, so the validation's wave count grew
    // with k (2k on a two-level inode table); now the phase's demands are one
    // grouped read, so the wave count is the table's height whatever k is — while
    // `inode_demands` still counts every demand.
    let (small_demands, small_waves, small_pages, small_root) = binding_demands(500);
    let (large_demands, large_waves, large_pages, large_root) = binding_demands(1_500);
    // Each file is renamed: two changes, each naming the same stored child.
    assert_eq!(
        small_demands, 1_001,
        "500 renames demand 1,000 children plus the parent"
    );
    assert_eq!(large_demands, 3_001, "the demand count follows k");
    assert_eq!(
        small_waves, large_waves,
        "the wave count does not: {small_waves} against {large_waves}"
    );
    assert!(
        large_waves <= 4,
        "one descent per inode-table level, not one per binding: {large_waves} waves \
         for {large_demands} demands"
    );
    // Pages follow the pages the demands live in, not the demands themselves:
    // each distinct inode page is read once per phase however many demands land
    // on it, where the unbatched path read the whole path per demand.
    assert!(
        large_pages * 10 < large_demands,
        "the grouped read touches each distinct page once: {large_pages} pages for \
         {large_demands} demands"
    );
    assert!(
        small_pages * 10 < small_demands,
        "{small_pages} pages for {small_demands} demands"
    );
    assert!(
        small_waves <= small_pages,
        "each wave reads at least one page: {small_waves} waves, {small_pages} pages"
    );
    assert_ne!(small_root, large_root, "two different trees, two roots");
}

#[test]
fn one_grouped_demand_is_one_charged_wave() {
    // The counter semantics C1 fixed, pinned on single calls: a grouped demand is
    // one wave, a point read is one wave, and neither is charged twice. The
    // operation-level test beside this one cannot pin an exact total, because a
    // whole operation's wave count also includes the merge's batches, whose number
    // is `ceil(children / BATCH_CHILDREN)` — a function of the width P1-1 raised.
    let session = Session::new(1).expect("empty tree");
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let mut objects = FilesystemObjects::new(&provider, &mut sink);
    let root = session.root;

    objects.read_batch(&[root, root]).expect("grouped demand");
    assert_eq!(objects.work().objects_read, 2);
    assert_eq!(
        objects.work().read_waves,
        1,
        "one grouped demand is one wave, not two"
    );
    assert_eq!(provider.waves(), vec![2], "and one provider call");

    objects.read(root).expect("point read");
    assert_eq!(objects.work().objects_read, 3);
    assert_eq!(
        objects.work().read_waves,
        2,
        "a point read is one more wave"
    );
    assert_eq!(provider.waves(), vec![2, 1]);
}

#[test]
fn a_grouped_demand_is_one_wave_and_every_page_it_decoded() {
    // A wide directory's changed-path merge fetches its sibling children as one
    // bounded group and then decodes each one. Two counter readings follow from
    // that single fact: the group is **one** wave (not one per object, and not
    // two), and the children it decoded are **pages read** (the counter's own
    // doc says "including batched ones").
    //
    // Measured on this fixture (4,000-entry directory, the three-name rename):
    // the boundary issues 5 point reads and 1 grouped demand of 14 objects, so
    // `read_waves` is 6 and the grouped call charges 14 decoded pages on top of
    // one point-read page. Before both fixes the same run reported 9 waves (the
    // group counted twice: once for the demand and once for its decode) and 1
    // page (the 14 batched decodes were invisible).
    let report = wide_rename_charges();
    let grouped = report.grouped_objects();
    assert!(
        grouped > 0,
        "the fixture issued a grouped demand wave: {:?}",
        report.waves
    );
    assert_eq!(
        grouped + report.point_objects(),
        report.demanded,
        "every demand the provider served is either point or grouped"
    );
    // The boundary charges at most one wave per provider call it makes, so its
    // own count can never exceed the provider's call count. Before C1 a grouped
    // demand was charged twice and the same run reported 10 charged waves against
    // 6 provider calls. The exact per-call semantics are pinned on single calls by
    // `one_grouped_demand_is_one_charged_wave`: an operation-level total also
    // counts the merge's batches, whose number is `ceil(children / BATCH_CHILDREN)`
    // and therefore a function of the width P1-1 raises.
    assert!(
        report.boundary_waves <= report.waves.len() as u64,
        "the boundary charged {} waves for {} provider calls",
        report.boundary_waves,
        report.waves.len()
    );
    assert!(
        report.boundary_waves < report.objects_read + 2,
        "a grouped demand is one wave for many objects: {} waves for {} objects",
        report.boundary_waves,
        report.objects_read
    );
    assert!(
        report.pages_read >= 15,
        "the merged group's 14 batched children are pages read: {} reported for \
         {} waves",
        report.pages_read,
        report.boundary_waves
    );
    assert!(
        report.objects_read >= grouped as u64,
        "the grouped children are objects read as well: {} for {grouped}",
        report.objects_read
    );
}

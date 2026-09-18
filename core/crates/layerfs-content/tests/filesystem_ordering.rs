//! Ordering grammar, thresholds, tiers, accounting and checked cleanup.
//!
//! The reducer's bounded pending map, its tiered runs and the backing that owns
//! their bytes are exercised through the public API only. Every counter assertion
//! is cross-checked against what the backing itself observed, and every failure
//! assertion requires that the operation reports the failure instead of a root.

mod support;

use std::collections::BTreeMap;

use layerfs_content::filesystem::references::backing::OrderingBacking;
use layerfs_content::filesystem::references::record::{Row, ROW_BYTES};
use layerfs_content::filesystem::references::runs::RunStore;
use layerfs_content::filesystem::{
    build_filesystem, update_filesystem, DirectoryUpdate, FilesystemInput, FilesystemObjects,
    FilesystemResources, FilesystemRoot, FilesystemRootId, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ContentError, ObjectRole};
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
        synthetic("ordering/meta"),
    )
}

fn directory() -> InodeValue {
    value(
        InodeKind::Directory,
        synthetic("ordering/unused"),
        synthetic("ordering/dir-meta"),
    )
}

/// The two-record set the grammar tests use as their golden fixture.
fn golden_rows() -> (Row, Row) {
    let count = Row::Count {
        serial: 7,
        value: Some(InodeValue {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 2,
            content_root: synthetic("ordering/golden-content"),
            metadata_root: synthetic("ordering/golden-meta"),
        }),
        count: 2,
    };
    let effect = Row::Effect {
        serial: 9,
        value: None,
        delta: -1,
    };
    (count, effect)
}

#[test]
fn records_are_fixed_width_and_every_malformed_field_is_rejected() {
    let (count, effect) = golden_rows();
    let bytes = count.encode().expect("encodes");
    assert_eq!(bytes.len(), ROW_BYTES);
    assert_eq!(u64::from_be_bytes(bytes[..8].try_into().unwrap()), 7);
    assert_eq!(bytes[8], 1, "record version");
    assert_eq!(bytes[9], 1, "count tag");
    assert_eq!(
        u16::from_be_bytes([bytes[10], bytes[11]]) as usize,
        ROW_BYTES
    );
    assert_eq!(bytes[12], 1, "typed value present");
    assert_eq!(u64::from_be_bytes(bytes[13..21].try_into().unwrap()), 2);
    assert_eq!(bytes[21], InodeKind::RegularFile.code());
    assert_eq!(
        u64::from_be_bytes(bytes[22..30].try_into().unwrap()),
        2,
        "the typed value keeps its own copy of the count"
    );
    assert_eq!(
        &bytes[30..62],
        synthetic("ordering/golden-content").as_bytes()
    );
    assert_eq!(&bytes[62..94], synthetic("ordering/golden-meta").as_bytes());
    assert_eq!(&bytes[94..96], &[0, 0]);
    assert_eq!(Row::decode(&bytes).expect("decodes"), count);

    let effect_bytes = effect.encode().expect("encodes");
    assert_eq!(effect_bytes[9], 2, "effect tag");
    assert_eq!(effect_bytes[12], 0, "no typed value");
    assert_eq!(
        i64::from_be_bytes(effect_bytes[13..21].try_into().unwrap()),
        -1
    );
    assert_eq!(
        &effect_bytes[21..94],
        &[0_u8; 73],
        "an absent value leaves no bytes"
    );
    assert_eq!(Row::decode(&effect_bytes).expect("decodes"), effect);

    for mutate in [(8_usize, 9_u8), (9, 7), (11, 1), (94, 1), (95, 1)] {
        let mut damaged = bytes;
        damaged[mutate.0] = mutate.1;
        assert!(
            Row::decode(&damaged).is_err(),
            "field {} = {} was accepted",
            mutate.0,
            mutate.1
        );
    }
    let mut zero_serial = bytes;
    zero_serial[..8].copy_from_slice(&0_u64.to_be_bytes());
    assert!(matches!(
        Row::decode(&zero_serial),
        Err(ContentError::InvalidOrderingRecord("serial"))
    ));
    let mut absent_with_content = effect_bytes;
    absent_with_content[40] = 1;
    assert!(matches!(
        Row::decode(&absent_with_content),
        Err(ContentError::InvalidOrderingRecord("absent value"))
    ));
    let mut bad_flag = effect_bytes;
    bad_flag[12] = 2;
    assert!(matches!(
        Row::decode(&bad_flag),
        Err(ContentError::InvalidOrderingRecord("value flag"))
    ));
}

#[test]
fn the_pending_threshold_changes_only_where_the_rows_live() {
    // The same operation with a one-record pending map and with a large one must
    // reach the same root identity: crossing the threshold is planned work, not
    // an alternate result.
    let mut small = Session::new(1).expect("empty");
    let mut large = Session::new(1).expect("empty");
    let file = 2_u64;
    let mut roots = Vec::new();
    for session in [&mut small, &mut large] {
        session
            .apply(
                &[DirectoryUpdate {
                    parent: 1,
                    changes: vec![(name("f"), Some(file))],
                }],
                &[InodeUpdate {
                    serial: file,
                    value: regular("ordering/content"),
                }],
                &[file],
            )
            .expect("creates the file");
        roots.push(session.root);
    }
    assert_eq!(
        roots[0], roots[1],
        "the threshold must not change the result"
    );

    // Now force many crossings on a base tree and compare with a bounded map that
    // never spills.
    let directory_serial = 2_u64;
    let serials = (3..=40_u64).collect::<Vec<_>>();
    let directories = |count: usize| {
        vec![
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("d"), Some(directory_serial))],
            },
            DirectoryUpdate {
                parent: directory_serial,
                changes: (0..count)
                    .map(|index| (name(&format!("f{index:03}")), Some(serials[index])))
                    .collect(),
            },
        ]
    };
    let inodes = std::iter::once(InodeUpdate {
        serial: 1,
        value: directory(),
    })
    .chain(std::iter::once(InodeUpdate {
        serial: directory_serial,
        value: directory(),
    }))
    .chain(
        serials
            .iter()
            .enumerate()
            .map(|(index, serial)| InodeUpdate {
                serial: *serial,
                value: regular(&format!("ordering/content-{index:03}")),
            }),
    )
    .collect::<Vec<_>>();
    let mut new_inodes = vec![1_u64, directory_serial];
    new_inodes.extend(serials.iter().copied());
    new_inodes.sort_unstable();

    let temp = TempDir::new("ordering-threshold");
    let mut backing = RecordingBacking::new(temp.path());
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x61; 32]),
        root_serial: 1,
        directories: &directories(serials.len()),
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources {
            maximum_pending_records: 1,
            ..resources()
        },
    };
    let reader = TreeStore::new();
    let mut sink = TreeStore::new();
    let spilled_result = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        build_filesystem(&mut objects, &input, Some(&mut backing)).expect("spilling build")
    };
    let spilled_sink = sink.clone();
    assert!(
        spilled_result.counters.references.rows_spilled > 0,
        "a one-record map must cross the threshold"
    );
    assert!(
        spilled_result.counters.references.runs.runs_created > 0,
        "crossing the threshold must create runs"
    );
    assert_eq!(
        spilled_result.counters.references.runs.runs_created,
        backing.runs_created(),
        "the reported run count must equal what the backing created"
    );
    assert_eq!(count_role(&spilled_sink, ObjectRole::FilesystemRoot), 1);

    let bounded_reader = TreeStore::new();
    let mut bounded_sink = TreeStore::new();
    let bounded_result = {
        let input = FilesystemInput {
            base: None,
            scope: layerfs_content::filesystem::scope_for_seed([0x61; 32]),
            root_serial: 1,
            directories: &directories(serials.len()),
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: resources(),
        };
        let mut objects = FilesystemObjects::new(&bounded_reader, &mut bounded_sink);
        build_filesystem(&mut objects, &input, None).expect("bounded build")
    };
    assert_eq!(
        bounded_result.counters.references.rows_spilled, 0,
        "a large pending map must not spill for this input"
    );
    assert_eq!(
        bounded_result.root, spilled_result.root,
        "both thresholds must produce the same canonical root"
    );
    assert_eq!(bounded_result.value, spilled_result.value);
    let _ = temp;
}

#[test]
fn tier_carries_keep_the_newest_row_and_accumulate_counts() {
    let temp = TempDir::new("ordering-tiers");
    let mut backing = RecordingBacking::new(temp.path());
    let mut store = RunStore::new(Some(&mut backing), 4096, 4 * 1024 * 1024);
    // Twelve one-row batches: carries must reach at least the second tier.
    for index in 1..=12_u64 {
        let mut pending = BTreeMap::new();
        pending.insert(
            5_u64,
            Row::Count {
                serial: 5,
                value: Some(regular("ordering/tier")),
                count: index,
            },
        );
        pending.insert(
            100 + index,
            Row::Effect {
                serial: 100 + index,
                value: None,
                delta: 1,
            },
        );
        store.spill(&pending).expect("spill");
    }
    let work = store.work();
    assert!(work.merges >= 1, "carries must merge older tiers");
    assert!(work.peak_level >= 1, "carries must reach a higher tier");
    // The newest row for the shared serial wins outright.
    match store.find(5).expect("finds").expect("present") {
        Row::Count { count, .. } => assert_eq!(count, 12, "the newest tier holds the key"),
        other => panic!("unexpected row {other:?}"),
    }
    // An older-only key survives the carries.
    assert!(store.find(101).expect("finds").is_some());
    store.release().expect("checked cleanup");
    drop(store);
    let observed = backing.counters();
    assert_eq!(
        work.runs_created, observed.creates,
        "reported runs must equal observed creates"
    );
    assert!(
        work.rows_written >= observed.appends,
        "every appended row was counted: {} written, {} appended",
        work.rows_written,
        observed.appends
    );
    assert_eq!(observed.releases, 1, "cleanup is explicit, not implicit");
    assert!(
        !backing.owns_storage(),
        "cleanup removes every run it owned"
    );
    assert_eq!(backing.held_bytes(), 0, "released bytes stop being owned");
    assert!(backing.peak_bytes() > 0, "the physical peak was recorded");
    assert!(
        work.peak_run_bytes >= backing.peak_bytes(),
        "the reported peak covers the physical owner's observation"
    );
}

#[test]
fn growth_is_reserved_before_it_happens_and_cleanup_returns_the_bytes() {
    let temp = TempDir::new("ordering-quota");
    let mut backing = RecordingBacking::with_capacity(temp.path(), ROW_BYTES as u64);
    let mut run = backing.create_run().expect("run");
    run.append(&[7; ROW_BYTES]).expect("exactly at the ceiling");
    assert_eq!(backing.held_bytes(), ROW_BYTES as u64);
    assert_eq!(backing.peak_bytes(), ROW_BYTES as u64);
    assert_eq!(run.len(), ROW_BYTES as u64);
    let refused = run.append(&[7; 1]);
    assert!(
        matches!(refused, Err(ContentError::ObjectLimitExceeded { .. })),
        "growth past the declared ceiling must fail before it happens"
    );
    assert_eq!(
        run.len(),
        ROW_BYTES as u64,
        "the failed append wrote nothing"
    );
    assert_eq!(
        backing.held_bytes(),
        ROW_BYTES as u64,
        "a refused reservation is not counted as owned"
    );
    drop(run);
    assert_eq!(backing.held_bytes(), 0, "dropping a run returns its bytes");
    assert!(!backing.owns_storage(), "dropping a run removes its file");
}

#[test]
fn a_removal_that_fails_is_visible_instead_of_being_hidden_in_drop() {
    let temp = TempDir::new("ordering-cleanup-visibility");
    let mut backing = RecordingBacking::new(temp.path());
    let run = backing.create_run().expect("run");
    // Make the run's own path unremovable, then drop the run: the removal fails
    // and the owner must record that rather than reporting a clean finish.
    let path = std::fs::read_dir(temp.path())
        .expect("listing")
        .next()
        .expect("one run file")
        .expect("entry")
        .path();
    std::fs::remove_file(&path).expect("remove the file");
    std::fs::create_dir(&path).expect("occupied directory");
    std::fs::write(path.join("occupied"), b"x").expect("content");
    drop(run);
    let failure = layerfs_content::filesystem::references::backing::FileBacking::with_capacity(
        temp.path(),
        1024,
    );
    assert_eq!(backing.held_bytes(), 0, "a dropped run stops being owned");
    let _ = failure;
    assert!(
        backing.cleanup_failed(),
        "a removal failure must be recorded, not swallowed"
    );
    assert!(std::fs::remove_dir_all(&path).is_ok());
}

#[test]
fn the_operation_ceiling_is_enforced_before_the_rows_are_written() {
    let temp = TempDir::new("ordering-operation-quota");
    let mut backing = RecordingBacking::new(temp.path());
    let serials = (3..=40_u64).collect::<Vec<_>>();
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(2))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: serials
                .iter()
                .enumerate()
                .map(|(index, serial)| (name(&format!("f{index:03}")), Some(*serial)))
                .collect(),
        },
    ];
    let inodes = std::iter::once(InodeUpdate {
        serial: 1,
        value: directory(),
    })
    .chain(std::iter::once(InodeUpdate {
        serial: 2,
        value: directory(),
    }))
    .chain(
        serials
            .iter()
            .enumerate()
            .map(|(index, serial)| InodeUpdate {
                serial: *serial,
                value: regular(&format!("ordering/quota-{index:03}")),
            }),
    )
    .collect::<Vec<_>>();
    let mut new_inodes = vec![1_u64, 2];
    new_inodes.extend(serials.iter().copied());
    new_inodes.sort_unstable();
    let mut sink = TreeStore::new();
    let reader = sink.clone();
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x62; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources {
            maximum_pending_records: 1,
            ordering_bytes: ROW_BYTES as u64,
            ..resources()
        },
    };
    let outcome = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        build_filesystem(&mut objects, &input, Some(&mut backing))
    };
    assert!(
        matches!(outcome, Err(ContentError::ObjectLimitExceeded { .. })),
        "an operation that cannot fit its declared ordering ceiling must fail: {outcome:?}"
    );
    assert_eq!(
        count_role(&sink, ObjectRole::FilesystemRoot),
        0,
        "a failed operation publishes no root object"
    );
}

#[test]
fn append_read_flush_and_release_failures_fail_the_operation_without_a_root() {
    /// Fixture: a three-inode build whose ordering spills with a one-record map.
    fn build(
        backing: &mut RecordingBacking,
        resources: FilesystemResources,
    ) -> (
        layerfs_content::ContentResult<layerfs_content::filesystem::FilesystemResult>,
        TreeStore,
    ) {
        let directories = [
            DirectoryUpdate {
                parent: 1,
                changes: vec![(name("a"), Some(2)), (name("b"), Some(3))],
            },
            DirectoryUpdate {
                parent: 2,
                changes: vec![(name("c"), Some(4))],
            },
        ];
        let inodes = [
            InodeUpdate {
                serial: 1,
                value: directory(),
            },
            InodeUpdate {
                serial: 2,
                value: directory(),
            },
            InodeUpdate {
                serial: 3,
                value: regular("ordering/failure-a"),
            },
            InodeUpdate {
                serial: 4,
                value: regular("ordering/failure-b"),
            },
        ];
        let input = FilesystemInput {
            base: None,
            scope: layerfs_content::filesystem::scope_for_seed([0x63; 32]),
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &[1, 2, 3, 4],
            resources,
        };
        let reader = TreeStore::new();
        let mut sink = TreeStore::new();
        let outcome = {
            let mut objects = FilesystemObjects::new(&reader, &mut sink);
            build_filesystem(&mut objects, &input, Some(backing))
        };
        (outcome, sink)
    }

    let small = FilesystemResources {
        maximum_pending_records: 1,
        ..resources()
    };

    // A failing append: the spill cannot be written.
    let temp = TempDir::new("ordering-fail-append");
    let mut backing = RecordingBacking::new(temp.path());
    backing.fail_append_at = Some(2);
    let (outcome, sink) = build(&mut backing, small);
    assert!(
        matches!(outcome, Err(ContentError::Io)),
        "a failed append must fail the operation: {outcome:?}"
    );
    assert_eq!(count_role(&sink, ObjectRole::FilesystemRoot), 0);
    assert!(
        backing.counters().releases >= 1,
        "the failure path still attempts known-owned cleanup"
    );
    assert!(!backing.owns_storage(), "the attempt's runs were removed");

    // A failing read: an already written tier cannot be read back.
    let temp = TempDir::new("ordering-fail-read");
    let mut backing = RecordingBacking::new(temp.path());
    backing.fail_read_at = Some(1);
    let (outcome, sink) = build(&mut backing, small);
    assert!(
        matches!(outcome, Err(ContentError::Io)),
        "a failed tier read must fail the operation: {outcome:?}"
    );
    assert_eq!(count_role(&sink, ObjectRole::FilesystemRoot), 0);
    assert!(backing.counters().releases >= 1);

    // A failing flush: the batch is not durable for its reader.
    let temp = TempDir::new("ordering-fail-flush");
    let mut backing = RecordingBacking::new(temp.path());
    backing.fail_flush_at = Some(1);
    let (outcome, sink) = build(&mut backing, small);
    assert!(
        matches!(outcome, Err(ContentError::Io)),
        "a failed flush must fail the operation: {outcome:?}"
    );
    assert_eq!(count_role(&sink, ObjectRole::FilesystemRoot), 0);

    // A refusing release: success is impossible while owned storage remains.
    let temp = TempDir::new("ordering-fail-release");
    let mut backing = RecordingBacking::new(temp.path());
    backing.refuse_release = true;
    let (outcome, sink) = build(&mut backing, small);
    assert!(
        matches!(
            outcome,
            Err(ContentError::ResourceUnavailable {
                what: "ordering run cleanup"
            })
        ),
        "a failed checked cleanup must fail the operation: {outcome:?}"
    );
    assert_eq!(
        count_role(&sink, ObjectRole::FilesystemRoot),
        0,
        "no new root may exist when the required cleanup failed"
    );
    assert_eq!(
        backing.counters().releases,
        1,
        "cleanup is attempted exactly once"
    );
}

#[test]
fn a_successful_operation_releases_its_ordering_resources_once() {
    let temp = TempDir::new("ordering-success-cleanup");
    let mut backing = RecordingBacking::new(temp.path());
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(2))],
        },
        DirectoryUpdate {
            parent: 2,
            changes: (0..12)
                .map(|index| (name(&format!("f{index:02}")), Some(index as u64 + 3)))
                .collect(),
        },
    ];
    let inodes = std::iter::once(InodeUpdate {
        serial: 1,
        value: directory(),
    })
    .chain(std::iter::once(InodeUpdate {
        serial: 2,
        value: directory(),
    }))
    .chain((0..12).map(|index| InodeUpdate {
        serial: index + 3,
        value: regular(&format!("ordering/success-{index:02}")),
    }))
    .collect::<Vec<_>>();
    let new_inodes = (1..=14).collect::<Vec<u64>>();
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x64; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: FilesystemResources {
            maximum_pending_records: 1,
            ..resources()
        },
    };
    let reader = TreeStore::new();
    let mut sink = TreeStore::new();
    let result = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        build_filesystem(&mut objects, &input, Some(&mut backing)).expect("build")
    };
    let observed = backing.counters();
    assert!(
        result.counters.references.rows_spilled > 0,
        "this fixture must actually spill"
    );
    assert_eq!(
        result.counters.references.runs.runs_created, observed.creates,
        "returned counters must match the backing's own observation"
    );
    assert!(
        result.counters.references.runs.rows_written >= observed.appends,
        "returned row writes must cover the appends the backing saw"
    );
    assert!(
        result.counters.references.runs.peak_run_bytes > 0,
        "real spills must report a nonzero simultaneous byte peak"
    );
    assert!(
        result.counters.references.runs.peak_live_runs >= 2,
        "a merge owns its inputs and its output at once"
    );
    assert_eq!(
        observed.releases, 1,
        "cleanup runs exactly once, on success"
    );
    assert!(
        !backing.owns_storage(),
        "success means nothing is still owned"
    );
    assert_eq!(backing.held_bytes(), 0);
    assert!(backing.peak_bytes() >= ROW_BYTES as u64);
    assert_eq!(count_role(&sink, ObjectRole::FilesystemRoot), 1);
    // The same input on a base root goes through the update entry point.
    let mut session = Session::new(1).expect("empty");
    let file = session.allocate();
    session
        .apply(
            &[DirectoryUpdate {
                parent: 1,
                changes: vec![(name("f"), Some(file))],
            }],
            &[InodeUpdate {
                serial: file,
                value: regular("ordering/update"),
            }],
            &[file],
        )
        .expect("update through the ordered path");
    let temp = TempDir::new("ordering-update");
    let mut backing = RecordingBacking::new(temp.path());
    let input = FilesystemInput {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: &[DirectoryUpdate {
            parent: 1,
            changes: vec![(name("f"), None)],
        }],
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources {
            maximum_pending_records: 1,
            ..resources()
        },
    };
    let reader = session.store.clone();
    let mut sink = TreeStore::new();
    let updated = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("update")
    };
    assert_ne!(updated.root.0, session.root);
    assert_eq!(
        count_role(&sink, ObjectRole::FilesystemRoot),
        1,
        "the update emits exactly one root"
    );
    assert_eq!(backing.counters().releases, 1);
    assert!(!backing.owns_storage());
    let decoded = FilesystemRoot::decode(sink.canonical(updated.root.0).expect("root bytes"))
        .expect("decode");
    assert_eq!(decoded, updated.value);
}

#[test]
fn a_spilled_lookup_agrees_with_a_full_scan_of_every_tier() {
    // A lookup that keeps its position must answer exactly what a scan from the
    // front answers, for every serial, in every order. This drives the same
    // rename-every-entry shape the external growth probe uses, with the pending
    // map small enough that the operation really spills.
    let entries = 200_usize;
    let session = Session::new(1).expect("empty");
    let mut changes = Vec::new();
    let mut inodes = Vec::new();
    let mut new_inodes = Vec::new();
    for index in 0..entries {
        let serial = entries as u64 + index as u64 + 2;
        changes.push((name(&format!("f{index:05}")), Some(serial)));
        inodes.push(InodeUpdate {
            serial,
            value: regular(&format!("renamed-{index}")),
        });
        new_inodes.push(serial);
    }
    new_inodes.sort_unstable();
    let directory = DirectoryUpdate { parent: 1, changes };
    let resources = FilesystemResources {
        maximum_pending_records: 64,
        ..resources()
    };
    let input = FilesystemInput {
        base: Some(layerfs_content::filesystem::FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: std::slice::from_ref(&directory),
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources,
    };
    let provider = CountingProvider::new(&session.store);
    let mut sink = TreeStore::new();
    let backing_directory = TempDir::new("ordering-lookup-scan");
    let mut backing = RecordingBacking::with_capacity(backing_directory.path(), 64 << 20);
    let result = {
        let mut objects = layerfs_content::filesystem::FilesystemObjects::new(&provider, &mut sink);
        update_filesystem(&mut objects, &input, Some(&mut backing)).expect("spilling update")
    };
    assert!(
        result.counters.references.rows_spilled > 0,
        "the operation must really spill: {}",
        result.counters.references.rows_spilled
    );
    assert!(
        result.counters.references.runs.rows_read > 0,
        "lookups into spilled runs must be charged"
    );
    // The agreement the name promises: the same input with a pending map large
    // enough to hold every row (no spill, no run lookup) must produce the same
    // root as the spilling run, because a lookup into a spilled run answers
    // exactly what the in-memory map would have answered.
    let unspilled_resources = layerfs_content::filesystem::FilesystemResources::default();
    assert!(
        unspilled_resources.maximum_pending_records > entries,
        "the no-spill arm must really hold every row"
    );
    let input = FilesystemInput {
        base: Some(layerfs_content::filesystem::FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: 1,
        directories: std::slice::from_ref(&directory),
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources: unspilled_resources,
    };
    let provider = CountingProvider::new(&session.store);
    let mut unspilled_sink = TreeStore::new();
    let backing_directory = TempDir::new("ordering-lookup-scan-unspilled");
    let mut unspilled_backing = RecordingBacking::with_capacity(backing_directory.path(), 64 << 20);
    let unspilled = {
        let mut objects =
            layerfs_content::filesystem::FilesystemObjects::new(&provider, &mut unspilled_sink);
        update_filesystem(&mut objects, &input, Some(&mut unspilled_backing))
            .expect("no-spill update")
    };
    assert_eq!(
        unspilled.counters.references.rows_spilled, 0,
        "the comparison arm must not spill"
    );
    assert_eq!(
        result.root, unspilled.root,
        "spilled lookups must agree with the in-memory answers: same root required"
    );
}

#[test]
fn run_lookup_answers_every_serial_in_every_order() {
    // The lookup path keeps a position per tier. This compares it with the
    // replay the store already provides: the same spilled rows, walked in
    // ascending, descending and interleaved order, must give the same answer for
    // every serial that runs hold and for serials between them.
    let temp = TempDir::new("ordering-run-lookup");
    let mut backing = RecordingBacking::with_capacity(temp.path(), 64 << 20);
    let mut store = RunStore::new(Some(&mut backing), 4096, 64 << 20);
    // Three spilling batches over overlapping serial ranges, newest last, so a
    // serial can be held by more than one tier and precedence matters.
    for batch in 0..6_u64 {
        let mut pending = BTreeMap::new();
        for index in 0..40_u64 {
            let serial = 1 + (index * 3 + batch) % 90;
            pending.insert(
                serial,
                Row::Effect {
                    serial,
                    value: None,
                    delta: i64::try_from(batch + 1).expect("batch"),
                },
            );
        }
        store.spill(&pending).expect("spill");
    }
    // The truth: what a fresh walk of every live tier reports.
    let mut truth: BTreeMap<u64, i64> = BTreeMap::new();
    store
        .visit_newest_first(|row| {
            if let Row::Effect { serial, delta, .. } = row {
                truth.insert(serial, delta);
            }
            Ok(true)
        })
        .expect("visit");
    assert!(!truth.is_empty(), "the runs must hold rows");
    let keys = truth.keys().copied().collect::<Vec<_>>();
    let mut orders = vec![keys.clone()];
    let mut descending = keys.clone();
    descending.reverse();
    orders.push(descending);
    let mut interleaved = Vec::new();
    for index in 0..keys.len() {
        interleaved.push(keys[index]);
        interleaved.push(keys[keys.len() - 1 - index]);
    }
    orders.push(interleaved);
    orders.push(vec![keys[0]; 8]);
    orders.push(keys.iter().cycle().take(keys.len() * 3).copied().collect());
    for order in orders {
        for serial in order {
            let found = store.find(serial).expect("find");
            let row = found.unwrap_or_else(|| panic!("serial {serial} must be found"));
            assert_eq!(row.serial(), serial);
            match row {
                Row::Effect { delta, .. } => assert_eq!(
                    delta, truth[&serial],
                    "serial {serial} must carry the newest accumulated effect"
                ),
                Row::Count { .. } => panic!("serial {serial} was stored as an effect"),
            }
        }
        // A serial no tier holds is reported absent, never as some other row.
        for serial in [0_u64, 1_000, 5_000] {
            assert!(
                store.find(serial).expect("find").is_none(),
                "serial {serial} is not held by any tier"
            );
        }
    }
    store.release().expect("release");
}

/// R19: the accessors describe what is still on disk, not what removal intended.
///
/// `release` used to zero the held-bytes account whatever `remove_file` returned,
/// so a backing that failed to delete a run reported clean while the run file
/// remained, and `owns_storage` answered `false`. The same held for a run's own
/// drop. Here the run's path is replaced by a directory between the write and the
/// cleanup, so `remove_file` fails with a real error and the only honest answer is
/// "still owned".
#[test]
fn a_failed_run_removal_keeps_the_file_and_its_bytes_in_the_account() {
    use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};

    let dir = TempDir::new("backing-honest");
    let mut backing = FileBacking::new(dir.path());
    let mut run = backing.create_run().expect("run");
    run.append(&[0x5a; 96]).expect("append");
    run.flush().expect("flush");
    assert_eq!(backing.held_bytes(), 96);
    assert!(backing.owns_storage());

    // The written file becomes a directory: `remove_file` now fails with
    // `IsADirectory`, which is not "already gone".
    let path = dir.path().join("layerfs-ordering-00000001.run");
    std::fs::remove_file(&path).expect("the run file is there");
    std::fs::create_dir(&path).expect("a directory takes its place");

    let error = backing.release().expect_err("removal fails");
    assert!(matches!(
        error,
        ContentError::ResourceUnavailable {
            what: "ordering run cleanup"
        }
    ));
    assert!(backing.cleanup_failed());
    assert!(
        backing.owns_storage(),
        "a path this backing could not remove is still owned"
    );
    assert_eq!(
        backing.held_bytes(),
        96,
        "the bytes of a file that is still there are still held"
    );
    assert!(path.exists(), "the failed removal left the path alone");

    // The run's own destructor cannot remove it either, and says so the same way.
    drop(run);
    assert!(backing.cleanup_failed());
    assert!(backing.owns_storage());
    assert_eq!(backing.held_bytes(), 96);
}

/// R20: a stale run file does not make every later backing in that directory fatal.
///
/// Run names used to start at one for every backing, and `create_new` refused a
/// name that existed, so one file left behind by an earlier backing made every
/// later backing in that directory fail for good. The starting token is now taken
/// from the directory, so the stale file is left alone and the new backing works.
#[test]
fn a_stale_run_file_does_not_block_a_later_backing() {
    use layerfs_content::filesystem::references::backing::{FileBacking, OrderingBacking};

    let dir = TempDir::new("backing-stale");
    let stale = dir.path().join("layerfs-ordering-00000001.run");
    std::fs::write(&stale, b"from an earlier attempt").expect("stale run file");
    // Names that are not runs are ignored rather than parsed.
    std::fs::write(dir.path().join("layerfs-ordering-not-a-token.run"), b"x").expect("noise");
    std::fs::write(dir.path().join("unrelated"), b"y").expect("noise");

    let mut backing = FileBacking::new(dir.path());
    let mut run = backing
        .create_run()
        .expect("a stale run file must not block a later backing");
    run.append(&[7_u8; 64]).expect("append");
    run.flush().expect("flush");
    assert_eq!(backing.held_bytes(), 64);
    let live = dir.path().join("layerfs-ordering-00000002.run");
    assert!(live.exists(), "the new run takes the next free token");

    drop(run);
    backing.release().expect("cleanup succeeds");
    assert_eq!(backing.held_bytes(), 0);
    assert!(!backing.owns_storage());
    assert!(!backing.cleanup_failed());
    assert!(
        stale.exists(),
        "the pre-existing run file is left alone, never adopted or truncated"
    );
    assert_eq!(
        std::fs::read(&stale).expect("stale bytes"),
        b"from an earlier attempt"
    );
    assert!(!live.exists());
}

/// One effect row for the scan-retention case.
fn scan_row(serial: u64) -> Row {
    Row::Effect {
        serial,
        value: Some(value(
            InodeKind::RegularFile,
            synthetic(&format!("scan/{serial}")),
            synthetic("scan/meta"),
        )),
        delta: 1,
    }
}

#[test]
fn a_spill_keeps_the_scans_of_tiers_above_its_level() {
    // A spill into level k replaces the runs of tiers `[0, k]` and writes the
    // merged run back into k; every tier above k keeps the run it had, so it must
    // keep the scan of that run too. The reset used to clear all of them, so the
    // next demand on a higher tier restarted from the front of its run and paid
    // the rows the cursor had already passed — the residual P1-5 removes.
    let temp = TempDir::new("ordering-scan-keep");
    let mut backing = RecordingBacking::new(temp.path());
    let mut store = RunStore::new(Some(&mut backing), 4096, 8 * 1024 * 1024);

    const ROWS: u64 = 64;
    // Twelve spills in ascending serial order: the top tier holds serials
    // 1..=512 and the next one 513..=768.
    for batch in 0..12_u64 {
        let mut pending = BTreeMap::new();
        for serial in batch * ROWS + 1..=(batch + 1) * ROWS {
            pending.insert(serial, scan_row(serial));
        }
        store.spill(&pending).expect("spill");
    }
    // An ascending sweep to the middle of the top tier: its cursor is at row 256.
    for serial in 1..=256 {
        assert!(store.find(serial).expect("find").is_some());
    }

    // A thirteenth spill lands at level 0 without cascading: only this batch is
    // written, and the tiers above keep their runs.
    let mut pending = BTreeMap::new();
    for serial in 12 * ROWS + 1..=13 * ROWS {
        pending.insert(serial, scan_row(serial));
    }
    let written_before = store.work().rows_written;
    store.spill(&pending).expect("spill");
    assert_eq!(
        store.work().rows_written - written_before,
        ROWS,
        "a level-0 spill writes only its own batch"
    );

    // The demand continues past the sweep's end, inside the top tier: a kept scan
    // answers it from the cursor, a cleared one re-reads the tier from row 0.
    let read_before = store.work().rows_read;
    assert!(store.find(257).expect("find").is_some());
    let resumed = store.work().rows_read - read_before;
    assert!(
        resumed <= 4,
        "a kept scan resumes at its cursor: {resumed} rows read for one row"
    );
}

/// Builds one operation over `count` files in one directory under `resources` and
/// reports what the reference reducer charged.
fn pending_ceiling_arm(
    count: usize,
    resources: FilesystemResources,
) -> (
    layerfs_content::ContentResult<layerfs_content::filesystem::FilesystemResult>,
    TreeStore,
) {
    let directory_serial = 2_u64;
    let serials = (3..3 + count as u64).collect::<Vec<_>>();
    let directories = vec![
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(directory_serial))],
        },
        DirectoryUpdate {
            parent: directory_serial,
            changes: serials
                .iter()
                .enumerate()
                .map(|(index, serial)| (name(&format!("f{index:04}")), Some(*serial)))
                .collect(),
        },
    ];
    let inodes = std::iter::once(InodeUpdate {
        serial: 1,
        value: directory(),
    })
    .chain(std::iter::once(InodeUpdate {
        serial: directory_serial,
        value: directory(),
    }))
    .chain(
        serials
            .iter()
            .enumerate()
            .map(|(index, serial)| InodeUpdate {
                serial: *serial,
                value: regular(&format!("ordering/content-{index:04}")),
            }),
    )
    .collect::<Vec<_>>();
    let mut new_inodes = vec![1_u64, directory_serial];
    new_inodes.extend(serials.iter().copied());
    new_inodes.sort_unstable();
    let input = FilesystemInput {
        base: None,
        scope: layerfs_content::filesystem::scope_for_seed([0x62; 32]),
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &new_inodes,
        resources,
    };
    let reader = TreeStore::new();
    let mut sink = TreeStore::new();
    let temp = TempDir::new("ordering-pending-ceiling");
    let mut backing = RecordingBacking::new(temp.path());
    let result = {
        let mut objects = FilesystemObjects::new(&reader, &mut sink);
        build_filesystem(&mut objects, &input, Some(&mut backing))
    };
    (result, sink)
}

#[test]
fn a_high_pending_ceiling_runs_spill_free_to_the_byte_bound() {
    // The pending ceiling's spill-free bound is the ordering ceiling divided by the
    // x2 charge the account applies (a pending row plus the run it becomes), i.e.
    // `floor(ordering_bytes / (2 * ROW_BYTES))`. The shape's own row count is
    // calibrated first, so the arithmetic is exercised exactly rather than guessed:
    // the same operation is built under a generous ceiling, and that many rows at
    // the x2 charge is the bound the next three arms sit on.
    const ROW_BYTES: u64 = 96;
    const FILES: usize = 100;
    let (calibrated, _) = pending_ceiling_arm(
        FILES,
        FilesystemResources {
            ordering_bytes: 64 << 20,
            maximum_pending_records: 4_096,
            ..resources()
        },
    );
    let calibrated = calibrated.expect("calibrated build");
    let rows = calibrated.counters.references.peak_pending as u64;
    assert_eq!(
        calibrated.counters.references.rows_spilled, 0,
        "the calibration arm must not spill"
    );
    assert!(rows >= FILES as u64, "{rows} rows for {FILES} files");
    let bound = 2 * ROW_BYTES * rows;

    // Exactly at the bound: every row fits, so nothing spills.
    let (spill_free, _) = pending_ceiling_arm(
        FILES,
        FilesystemResources {
            ordering_bytes: bound,
            maximum_pending_records: rows as usize,
            ..resources()
        },
    );
    let spill_free = spill_free.expect("a spill-free build");
    assert_eq!(
        spill_free.counters.references.rows_spilled, 0,
        "the byte bound is inclusive"
    );
    assert_eq!(spill_free.counters.references.runs.runs_created, 0);
    assert_eq!(spill_free.counters.references.peak_pending as u64, rows);
    assert_eq!(spill_free.root, calibrated.root);

    // One row less of pending map: the same operation spills. The ceiling is
    // widened for this arm because a spill reserves the run it becomes *and* the
    // merge output it may produce, which the tight bound above deliberately cannot
    // hold; the point here is the verdict, not the bytes.
    let (spilled, _) = pending_ceiling_arm(
        FILES,
        FilesystemResources {
            ordering_bytes: 4 * bound,
            maximum_pending_records: rows as usize - 1,
            ..resources()
        },
    );
    let spilled = spilled.expect("a spilling build");
    assert!(
        spilled.counters.references.rows_spilled > 0,
        "one row under the ceiling crosses it"
    );
    assert_eq!(
        spilled.root, spill_free.root,
        "spilling is planned work, not a different result"
    );

    // The same map at the tight ceiling is refused at its first spill: the spill
    // reserves the run it becomes and the merge output it may produce, and a
    // ceiling that cannot hold them fails closed instead of allocating past the
    // declared bound. This is the arm that pins the arithmetic: the map is the
    // same, only the ceiling differs from the arm above.
    let (refused, _) = pending_ceiling_arm(
        FILES,
        FilesystemResources {
            ordering_bytes: bound,
            maximum_pending_records: rows as usize - 1,
            ..resources()
        },
    );
    match refused {
        Err(ContentError::ObjectLimitExceeded { limit, .. }) => {
            assert_eq!(limit, bound as usize);
        }
        other => panic!("a map beyond the byte bound must be refused: {other:?}"),
    }
}

#[test]
fn carried_state_removes_the_zero_count_re_pass() {
    // P1-10: `touched_serials` already reads every row, so the state it carries out
    // must answer the zero-count derivation without a second lookup. Before the
    // change the caller called `state()` per serial, and every run-held serial paid
    // at least one run read for it — on the forced-64 row that was 1,968 reads.
    use layerfs_content::filesystem::references::{PendingState, ReferenceReducer};

    let temp = TempDir::new("ordering-carried-state");
    let mut backing = RecordingBacking::new(temp.path());
    let mut reducer = ReferenceReducer::new(8, Some(&mut backing), 4096, 8 << 20);
    for serial in 1..=200_u64 {
        reducer
            .note_removed_binding(serial)
            .expect("one signed effect per serial");
    }
    let touched = reducer.touched_serials(32).expect("touched serials");
    assert_eq!(touched.len(), 200, "every touched serial is collected once");
    assert!(
        reducer.work().runs.runs_created > 0,
        "an eight-row map must spill this shape"
    );

    // Deriving the counts from the carried states reads nothing.
    let reads = reducer.work().runs.rows_read;
    let mut derived = 0_u64;
    for (_, state) in &touched {
        match state {
            PendingState::New { count, .. } => derived = derived.saturating_add(*count),
            PendingState::Existing { delta, .. } => {
                derived = derived.saturating_add(u64::try_from((*delta).max(0)).unwrap_or(0));
            }
        }
    }
    assert_eq!(derived, 0, "every serial lost its binding");
    assert_eq!(
        reducer.work().runs.rows_read,
        reads,
        "the carried states answer the derivation without a run read"
    );

    // The contrast that makes this a test rather than a tautology: one `state`
    // lookup for a serial the runs hold still charges a run read, which is what
    // the old derivation paid once per serial.
    let before = reducer.work().runs.rows_read;
    let _ = reducer.state(1).expect("state");
    assert!(
        reducer.work().runs.rows_read > before,
        "a re-find still costs a read: {} against {before}",
        reducer.work().runs.rows_read
    );
}

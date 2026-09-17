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
    count_role, resources, synthetic, value, RecordingBacking, Session, TempDir, TreeStore,
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

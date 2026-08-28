use super::*;

#[test]
fn operation_counters_preserve_structural_and_native_facts() {
    let mut counters = OperationCounters::default();
    counters
        .add_rope(layerfs_core::content::rope::RopeCounters {
            cdc_bytes_scanned: 4096,
            nodes_created: 2,
            ..Default::default()
        })
        .unwrap();
    counters
        .add_metadata_rope(layerfs_core::content::rope::RopeCounters {
            cdc_bytes_scanned: 16,
            payload_bytes_written: 16,
            ..Default::default()
        })
        .unwrap();
    counters
        .add_namespace(layerfs_core::namespace::NamespaceCounters {
            nodes_read: 3,
            nodes_created: 1,
        })
        .unwrap();
    counters
        .add_inode_table(layerfs_core::inode::InodeTableCounters {
            nodes_read: 4,
            nodes_created: 2,
        })
        .unwrap();
    counters
        .add_native(NativeOperationCounters {
            route: Some(NativeRoute::ClonePatch),
            bytes_written: 4096,
            patch_bytes: 4096,
            clone_attempts: 1,
            clone_successes: 1,
            ..Default::default()
        })
        .unwrap();

    assert_eq!(counters.rope.cdc_bytes_scanned, 4112);
    assert_eq!(counters.metadata_rope.cdc_bytes_scanned, 16);
    assert_eq!(counters.metadata_rope.payload_bytes_written, 16);
    assert_eq!(counters.rope.nodes_created, 2);
    assert_eq!(counters.namespace.nodes_read, 3);
    assert_eq!(counters.inode_table.nodes_created, 2);
    assert_eq!(counters.native.route, Some(NativeRoute::ClonePatch));
    assert_eq!(counters.native.patch_bytes, 4096);
    assert_eq!(counters.content_payload_bytes_read(), Some(0));
    assert_eq!(counters.content_payload_bytes_written(), Some(0));
}

#[test]
fn content_payload_facts_exclude_metadata_ropes_and_detect_bad_accounting() {
    let counters = OperationCounters {
        rope: layerfs_core::content::rope::RopeCounters {
            payload_bytes_read: 12,
            payload_bytes_written: 20,
            ..Default::default()
        },
        metadata_rope: layerfs_core::content::rope::RopeCounters {
            payload_bytes_read: 4,
            payload_bytes_written: 6,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(counters.content_payload_bytes_read(), Some(8));
    assert_eq!(counters.content_payload_bytes_written(), Some(14));

    let invalid = OperationCounters {
        rope: layerfs_core::content::rope::RopeCounters {
            payload_bytes_read: 3,
            ..Default::default()
        },
        metadata_rope: layerfs_core::content::rope::RopeCounters {
            payload_bytes_read: 4,
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(invalid.content_payload_bytes_read(), None);
}

#[test]
fn sequential_receipts_take_scratch_peak_while_distinct_tables_add() {
    let first = OperationCounters {
        scratch_statements: 3,
        scratch_high_water_bytes: 100,
        ..OperationCounters::default()
    };
    let second = OperationCounters {
        scratch_statements: 4,
        scratch_high_water_bytes: 120,
        ..OperationCounters::default()
    };
    let sequential = first.merge(second).unwrap();
    assert_eq!(sequential.scratch_statements, 7);
    assert_eq!(sequential.scratch_high_water_bytes, 120);

    let mut concurrent = OperationCounters::default();
    concurrent
        .add_scratch(layerfs_workspace::ScratchObservation {
            tables: 1,
            high_water_bytes: 100,
            ..Default::default()
        })
        .unwrap();
    concurrent
        .add_scratch(layerfs_workspace::ScratchObservation {
            tables: 1,
            high_water_bytes: 120,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(concurrent.scratch_tables, 2);
    assert_eq!(concurrent.scratch_high_water_bytes, 220);
}

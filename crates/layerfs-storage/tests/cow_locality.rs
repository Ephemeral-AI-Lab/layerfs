#[cfg(feature = "operation-polymorphism")]
mod cow_owner {
    use layerfs_storage::format::ROOT_DIRECTORY_MODE_SENTINEL_V1;
    use layerfs_storage::qualification::cow::semantic::{
        build_v1, canonical_order_v1, file_replacement_v1, identity_v1, mutate_v1, preflight_v1,
        TreeBuildRequestV1, TreeMutationFaultV1, TreeMutationObservationV1, TreeMutationRequestV1,
    };
    use layerfs_storage::qualification::resources::operation_slot_bytes_v1;
    use layerfs_storage::CoreError;

    #[test]
    fn exact_shape_preflight_covers_empty_fanout_splits_depth_and_one_over_limit() {
        let cases = [
            (0, 0, 0, 0, 0, 1),
            (1, 0, 1, 0, 1, 2),
            (192, 0, 1, 0, 1, 2),
            (193, 1, 2, 1, 3, 4),
            (18_432, 1, 96, 1, 97, 98),
            (18_433, 2, 97, 2, 100, 101),
            (1_000_000, 2, 5_209, 55, 5_265, 5_266),
        ];
        for (entries, depth, leaves, level_one, summaries, objects) in cases {
            let shape = preflight_v1(entries).unwrap();
            assert_eq!(shape.entry_count(), entries as u32);
            assert_eq!(shape.page_depth(), depth);
            assert_eq!(shape.leaf_count(), leaves);
            assert_eq!(shape.level_one_count(), level_one);
            assert_eq!(shape.page_summary_count(), summaries);
            assert_eq!(shape.tree_object_count(), objects);
        }
        assert_eq!(preflight_v1(1_000_001), Err(CoreError::CountCap));
    }

    #[test]
    fn empty_root_and_leaf_boundaries_emit_exact_canonical_tree_records() {
        let empty = build_v1(&TreeBuildRequestV1::new(0)).unwrap();
        assert_eq!(empty.entry_count(), 0);
        assert_eq!(empty.page_depth(), 0);
        assert_eq!(empty.tree_object_count(), 1);
        assert_eq!(empty.wrapper_mode(), ROOT_DIRECTORY_MODE_SENTINEL_V1);
        assert_eq!(empty.wrapper_entry_count(), 0);
        assert_eq!(empty.wrapper_root_page_absent(), true);
        assert_eq!(empty.ledger_admitted_slots(), 0);

        for (count, first, last) in [(1, 1, 1), (192, 192, 192), (193, 192, 1)] {
            let built = build_v1(&TreeBuildRequestV1::explicit(count, 0o755)).unwrap();
            assert_eq!(
                (
                    built.first_leaf_entry_count(),
                    built.last_leaf_entry_count()
                ),
                (first, last)
            );
            let leaf_counts = built.leaf_counts_match_expected();
            assert_eq!(leaf_counts, true);
            assert_eq!(built.ledger_admitted_slots(), 0);
        }
    }

    #[test]
    fn canonical_order_is_permutation_invariant_only_after_sorting_and_rejects_duplicates() {
        let observation = canonical_order_v1().unwrap();
        assert_eq!(
            observation.unsorted_error(),
            Some(CoreError::NonCanonicalOrder)
        );
        assert_eq!(observation.unsorted_sink_begins(), 0);
        assert_eq!(observation.sorted_matches_canonical(), true);
        assert!(matches!(
            observation.duplicate_error(),
            Some(CoreError::NonCanonicalOrder)
        ));
        let ledger = observation;
        assert_eq!(ledger.expected_build_ledger_admitted_slots(), 0);
        assert_eq!(ledger.sorted_build_ledger_admitted_slots(), 0);
        assert_eq!(ledger.duplicate_build_ledger_admitted_slots(), 0);
    }

    #[test]
    fn index_split_at_depth_two_is_canonical() {
        let built = build_v1(&TreeBuildRequestV1::new(18_433)).unwrap();
        assert_eq!(built.page_depth(), 2);
        assert_eq!(built.leaf_count(), 97);
        assert_eq!(built.level_one_count(), 2);
        assert_eq!(built.tree_object_count(), 101);
        assert_eq!(built.last_leaf_entry_count(), 1);
        assert_eq!(built.first_level_one_entry_count(), 18_432);
        assert_eq!(built.last_level_one_entry_count(), 1);
        assert_eq!(built.committed_objects(), 101);
        assert_eq!(built.tree_nodes_created(), 101);
        assert_eq!(built.memory_high_water(), 12_582_912);
        assert_eq!(built.ledger_admitted_slots(), 0);
    }

    #[test]
    fn same_name_replacement_copies_only_the_affected_spine_and_matches_full_rebuild() {
        let observation =
            mutate_v1(TreeMutationRequestV1::replace(300, 200).with_tags(3, 4)).unwrap();
        assert_eq!(observation.changed_leaf_index(), 1);
        assert_eq!(observation.sink_committed_len(), 3);
        assert_eq!(observation.tree_nodes_created(), 3);
        assert_eq!(observation.tree_nodes_reused(), 1);
        assert_eq!(observation.ledger_admitted_slots(), 0);
        assert_eq!(observation.base_build_ledger_admitted_slots(), 0);
        assert_eq!(observation.result_build_ledger_admitted_slots(), 0);
        assert_ne!(observation.emitted_objects(), 0);
        assert!(observation.directory_physical_differs_from_base());
        assert!(observation.directory_logical_differs_from_base());
        assert_ne!(observation.logical_matches_rebuild(), false);
        assert_eq!(observation.logical_matches_rebuild(), true);
        assert_eq!(observation.physical_matches_rebuild(), true);
        assert_ne!(observation.changed_leaf_id_differs(), false);
    }

    #[test]
    fn cow_rejects_same_shape_evidence_that_is_not_bound_to_the_base_root() {
        let observation = mutate_v1(
            TreeMutationRequestV1::replace(300, 200)
                .with_tags(5, 6)
                .with_fault(TreeMutationFaultV1::WrongRoot),
        )
        .unwrap();
        assert_eq!(observation.error(), Some(CoreError::IdMismatch));
        assert_eq!(observation.sink_begins(), 0);
        assert!(observation.sink_committed_len() == 0);
        assert_eq!(observation.ledger_admitted_slots(), 0);
        assert_eq!(observation.base_build_ledger_admitted_slots(), 0);
        assert_eq!(observation.result_build_ledger_admitted_slots(), 0);
    }

    #[test]
    fn cow_rejects_tampered_logical_window_before_private_tree_output() {
        let observation = mutate_v1(
            TreeMutationRequestV1::replace(300, 200)
                .with_tags(5, 6)
                .with_fault(TreeMutationFaultV1::TamperedWindow),
        )
        .unwrap();
        assert_eq!(observation.error(), Some(CoreError::IdMismatch));
        assert_eq!(observation.sink_begins(), 0);
        assert!(observation.sink_committed_len() == 0);
        assert_eq!(observation.ledger_admitted_slots(), 0);
        assert_eq!(observation.base_build_ledger_admitted_slots(), 0);
        assert_eq!(observation.result_build_ledger_admitted_slots(), 0);
    }

    #[test]
    fn depth_two_replacement_reads_one_bounded_window_and_copies_only_its_spine() {
        let observation =
            mutate_v1(TreeMutationRequestV1::replace(18_433, 50 * 192 + 10).with_tags(3, 4))
                .unwrap();
        assert!(observation.proof_window_bytes() <= 65_536);
        assert!(observation.proof_node_count() <= 64);
        assert_eq!(observation.affected_entries(), 192);
        assert_eq!(observation.leaf_group(), 96);
        assert_eq!(observation.level_one_group(), 2);
        assert_eq!(observation.changed_leaf_index(), 50);
        assert_eq!(observation.sink_committed_len(), 4);
        assert_eq!(observation.tree_nodes_created(), 4);
        assert_eq!(observation.tree_nodes_reused(), 96);
        assert_eq!(observation.bytes_read(), observation.proof_window_bytes());
        assert!(observation.bytes_read() <= 65_536);
        assert_eq!(observation.ledger_admitted_slots(), 0);
        assert_eq!(observation.base_build_ledger_admitted_slots(), 0);
        assert_eq!(observation.result_build_ledger_admitted_slots(), 0);
        assert_eq!(
            (
                observation.logical_matches_rebuild(),
                observation.physical_matches_rebuild()
            ),
            (true, true)
        );
    }

    #[test]
    fn add_and_remove_across_the_leaf_split_reuse_the_unchanged_leaf_and_exact_identity() {
        let added = mutate_v1(TreeMutationRequestV1::add(192, 192).with_tags(0, 8)).unwrap();
        assert_eq!(added.result_entries(), 193);
        assert_eq!(added.page_depth(), 1);
        assert_eq!(added.first_changed_leaf(), 1);
        assert_eq!(added.reused_leaves(), 1);
        assert_eq!(added.reused_level_one(), 0);
        assert_eq!(added.emitted_objects(), 3);
        assert_eq!(added.sink_committed_len(), 3);
        assert_eq!(added.tree_nodes_created(), 3);
        assert_eq!(added.tree_nodes_reused(), 1);
        assert_eq!(added.unchanged_leaf_reused(), true);
        assert_eq!(
            (
                added.logical_matches_rebuild(),
                added.physical_matches_rebuild()
            ),
            (true, true)
        );

        let removed = mutate_v1(TreeMutationRequestV1::remove(193, 192).with_tags(0, 8)).unwrap();
        assert_eq!(
            (
                removed.logical_matches_rebuild(),
                removed.physical_matches_rebuild()
            ),
            (true, true)
        );
        assert_eq!(removed.page_depth(), 0);
        assert_eq!(removed.reused_leaves(), 1);
        assert_eq!(removed.emitted_objects(), 1);
        assert_eq!(removed.sink_committed_len(), 1);
        assert_eq!(removed.tree_nodes_created(), 1);
        assert_eq!(removed.tree_nodes_reused(), 1);
        assert_eq!(removed.reused_leaves(), removed.tree_nodes_reused() as u32);
        assert_eq!(removed.ledger_admitted_slots(), 0);
        assert_eq!(added.base_build_ledger_admitted_slots(), 0);
        assert_eq!(added.result_build_ledger_admitted_slots(), 0);
        assert_eq!(removed.base_build_ledger_admitted_slots(), 0);
        assert_eq!(removed.result_build_ledger_admitted_slots(), 0);
    }

    #[test]
    fn cow_add_rejects_duplicate_or_mismatched_result_before_staging() {
        let observation = mutate_v1(
            TreeMutationRequestV1::add(2, 2).with_fault(TreeMutationFaultV1::DuplicateResult),
        )
        .unwrap();
        assert_eq!(observation.error(), Some(CoreError::NonCanonicalOrder));
        assert_eq!(observation.sink_begins(), 0);
        assert_eq!(observation.ledger_admitted_slots(), 0);
        assert_eq!(observation.base_build_ledger_admitted_slots(), 0);
    }

    #[test]
    fn depth_two_tail_add_and_remove_use_only_bounded_evidence_and_suffix_reads() {
        let added = mutate_v1(TreeMutationRequestV1::add(18_433, 18_433).with_tags(0, 8)).unwrap();
        assert!(added.leaf_group() <= 96);
        assert!(added.level_one_group() <= 55);
        assert!(added.proof_node_count() <= 64);
        assert!(added.middle_len() as usize <= 64);
        assert_eq!(added.emitted_objects(), 4);
        assert_eq!(added.sink_committed_len(), 4);
        assert_eq!(added.reused_leaves(), 96);
        assert_eq!(added.reused_level_one(), 1);
        assert!(added.base_reads() < 32);
        assert!(added.result_reads() < 32);
        assert_eq!(
            (
                added.logical_matches_rebuild(),
                added.physical_matches_rebuild()
            ),
            (true, true)
        );

        let removed =
            mutate_v1(TreeMutationRequestV1::remove(18_434, 18_433).with_tags(0, 8)).unwrap();
        assert_eq!(
            (
                removed.logical_matches_rebuild(),
                removed.physical_matches_rebuild()
            ),
            (true, true)
        );
        assert_eq!(removed.emitted_objects(), 4);
        assert_eq!(removed.sink_committed_len(), 4);
        assert!(removed.base_reads() < 32);
        assert!(removed.result_reads() < 32);
        assert_eq!(removed.ledger_admitted_slots(), 0);
        assert_eq!(added.base_build_ledger_admitted_slots(), 0);
        assert_eq!(added.result_build_ledger_admitted_slots(), 0);
        assert_eq!(removed.base_build_ledger_admitted_slots(), 0);
        assert_eq!(removed.result_build_ledger_admitted_slots(), 0);
    }

    #[test]
    fn large_head_and_middle_add_remove_report_exact_suffix_rebuild_locality() {
        let base = build_v1(&TreeBuildRequestV1::new(18_000)).unwrap();
        assert_eq!(base.page_depth(), 1);
        assert_eq!(base.leaf_count(), 94);

        let cases = [
            (true, 0, 9, 0, 96, 784_720, 54_194, 54_191),
            (true, 9_000, 10, 46, 50, 402_369, 27_197, 27_361),
            (false, 0, 0, 0, 96, 784_634, 54_194, 54_185),
            (false, 9_000, 0, 46, 50, 402_282, 27_197, 27_355),
        ];
        for (
            add,
            index,
            changed_tag,
            expected_reused,
            expected_created,
            expected_created_bytes,
            expected_base_reads,
            expected_result_reads,
        ) in cases
        {
            let observation = if add {
                mutate_v1(TreeMutationRequestV1::add(18_000, index).with_tags(0, changed_tag))
            } else {
                mutate_v1(TreeMutationRequestV1::remove(18_000, index).with_tags(0, changed_tag))
            }
            .unwrap();
            assert_eq!(
                (
                    observation.logical_matches_rebuild(),
                    observation.physical_matches_rebuild()
                ),
                (true, true)
            );
            assert_eq!(observation.reused_leaves(), expected_reused);
            assert_eq!(observation.reused_level_one(), 0);
            assert_eq!(observation.emitted_objects(), expected_created);
            assert_eq!(observation.tree_nodes_reused(), u64::from(expected_reused));
            assert_eq!(
                observation.tree_nodes_created(),
                u64::from(expected_created)
            );
            let sink = observation;
            assert_eq!(
                sink.sink_committed_len() as usize,
                expected_created as usize
            );
            assert_eq!(observation.ledger_admitted_slots(), 0);
            assert_eq!(observation.base_build_ledger_admitted_slots(), 0);
            assert_eq!(observation.result_build_ledger_admitted_slots(), 0);
            assert_eq!(observation.created_bytes(), expected_created_bytes);
            assert_eq!(observation.base_reads(), expected_base_reads);
            assert_eq!(observation.result_reads(), expected_result_reads);
        }
    }

    #[test]
    fn depth_two_remove_rejects_corruption_across_every_sparse_proof_region_before_staging() {
        let clean =
            mutate_v1(TreeMutationRequestV1::remove(18_433, 10_001).with_tags(0, 0)).unwrap();
        assert!(clean.first_changed_leaf() > 0);
        assert!(clean.proof_node_count() > 1);
        assert!(clean.result_reads() > 1);
        let old_tail_prefix = clean.old_tail_prefix_nonempty();
        assert!(old_tail_prefix);
        assert!(clean.leaf_group() > 1);
        assert!(clean.level_one_group() > 1);

        let faults = [
            TreeMutationFaultV1::CorruptOldHead,
            TreeMutationFaultV1::CorruptMiddle,
            TreeMutationFaultV1::CorruptTail,
            TreeMutationFaultV1::CorruptLeafSiblings,
            TreeMutationFaultV1::CorruptLevelOneSiblings,
        ];
        for fault in faults {
            let observation = mutate_v1(
                TreeMutationRequestV1::remove(18_433, 10_001)
                    .with_tags(0, 0)
                    .with_fault(fault),
            )
            .unwrap();
            assert!(observation.error().is_some());
            let outcome = observation.outcome();
            assert!(outcome.is_err());
            assert_eq!(observation.sink_begins(), 0);
            assert_eq!(observation.ledger_admitted_slots(), 0);
            assert_eq!(observation.base_build_ledger_admitted_slots(), 0);
            assert_eq!(observation.result_build_ledger_admitted_slots(), 0);
        }
    }

    #[test]
    fn cow_mutation_refuses_uncharged_source_or_sink_residency_before_reads_or_staging() {
        let slot = operation_slot_bytes_v1();
        let source = mutate_v1(
            TreeMutationRequestV1::add(2, 2)
                .with_tags(0, 8)
                .with_residency(slot, 0),
        )
        .unwrap();
        assert_eq!(source.error(), Some(CoreError::ResourceRefused));
        assert_eq!(source.base_reads() + source.result_reads(), 0);
        assert_eq!(source.sink_begins(), 0);

        let sink = mutate_v1(
            TreeMutationRequestV1::add(2, 2)
                .with_tags(0, 8)
                .with_residency(0, slot),
        )
        .unwrap();
        assert_eq!(sink.error(), Some(CoreError::ResourceRefused));
        assert_eq!(sink.base_reads() + sink.result_reads(), 0);
        assert_eq!(sink.sink_begins(), 0);
        assert_eq!(sink.ledger_admitted_slots(), 0);
        assert_eq!(source.base_build_ledger_admitted_slots(), 0);
        assert_eq!(sink.base_build_ledger_admitted_slots(), 0);
    }

    fn require_clean_replacement(observation: &TreeMutationObservationV1) {
        assert_eq!(observation.error(), None);
    }

    #[test]
    fn logical_directory_identity_is_invariant_across_physical_rechunking() {
        let observation = identity_v1().unwrap();
        assert!(matches!(observation.implicit_root(), true));
        assert_eq!(observation.logical_equal(), true);
        assert_eq!(observation.physical_ids_differ(), true);
        let ledger = observation;
        assert_eq!(ledger.first_build_ledger_admitted_slots(), 0);
        assert_eq!(ledger.second_build_ledger_admitted_slots(), 0);
    }

    #[test]
    fn file_update_reuses_unchanged_file_and_chunk_identity_outside_the_affected_spine() {
        let old = *b"old";
        let new = *b"new";
        assert_eq!([old.as_slice()].concat(), b"old");
        assert_eq!([new.as_slice()].concat(), b"new");
        let observation = file_replacement_v1(200, old, new).unwrap();
        require_clean_replacement(&observation);
        assert_eq!(
            (
                observation.logical_matches_rebuild(),
                observation.physical_matches_rebuild()
            ),
            (true, true)
        );
        assert_eq!(observation.unchanged_child_reused(), true);
        assert_eq!(observation.changed_leaf_index(), 1);
        assert_eq!(observation.tree_nodes_created(), 3);
        assert_eq!(observation.tree_nodes_reused(), 1);
        assert_eq!(observation.sink_committed_len(), 3);
        assert_eq!(observation.ledger_admitted_slots(), 0);
        assert_eq!(observation.base_build_ledger_admitted_slots(), 0);
        assert_eq!(observation.result_build_ledger_admitted_slots(), 0);
        assert!(observation.created_objects_are_tree_pages());
    }
}

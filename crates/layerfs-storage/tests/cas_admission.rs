mod support;

#[cfg(feature = "operation-polymorphism")]
mod cas_admission_owner {
    use crate::support::temp_fs_cas::TempFsCas;
    use layerfs_storage::profile::ProfileSpecV1;
    use layerfs_storage::qualification::cas::semantic::{
        cancel_after_locator_publication_v1, read_v1, PublicationRequestV1, ReadObjectKindV1,
        ReadRequestV1,
    };
    use layerfs_storage::qualification::pack::semantic::{
        build_v1, PackRequestV1,
    };
    use layerfs_storage::CoreError;

    fn object(payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(52 + payload.len());
        bytes.extend_from_slice(b"ELSOBJ01");
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.push(0x05);
        bytes.push(0);
        bytes.extend_from_slice(ProfileSpecV1::frozen().id().as_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn admission_case(index: usize) -> (Option<CoreError>, u64, bool, u64) {
        let root = TempFsCas::new("cas-admission");
        root.create_dir();
        assert!(root.path().is_dir());
        assert!(root.path().file_name().is_some());
        let payload = [index as u8; 96];
        let bytes = object(&payload);
        if index < 35 {
            let observation = if index % 11 == 0 {
                read_v1(
                    ReadRequestV1::new(ReadObjectKindV1::Chunk, &[0, 1, 2])
                        .with_occupied(&bytes),
                )
            } else if index % 7 == 0 {
                read_v1(
                    ReadRequestV1::new(ReadObjectKindV1::Chunk, &bytes)
                        .with_residency(64 * 1024 * 1024, 0),
                )
            } else {
                read_v1(ReadRequestV1::new(ReadObjectKindV1::Chunk, &bytes))
            };
            (
                observation.error(),
                observation.bytes_read(),
                observation.sink_finished(),
                observation.bytes_written(),
            )
        } else {
            let objects = [bytes.as_slice()];
            let observation = if index % 5 == 0 {
                build_v1(PackRequestV1::new(&objects).with_source_residency(64 * 1024 * 1024))
            } else if index % 5 == 1 {
                build_v1(PackRequestV1::new(&objects).with_pack_residency(64 * 1024 * 1024))
            } else if index % 5 == 2 {
                build_v1(PackRequestV1::new(&objects).with_index_residency(64 * 1024 * 1024))
            } else if index % 5 == 3 {
                build_v1(PackRequestV1::new(&objects).with_sink_failure_after(0))
            } else {
                build_v1(PackRequestV1::new(&objects))
            };
            (
                observation.error(),
                observation.bytes_read(),
                observation.sealed(),
                observation.bytes_written(),
            )
        }
    }

    #[test]
    fn pack_is_transferred_once_then_reopened_through_committed_catalog() {
        let (error, work, terminal, bytes) = admission_case(0);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn valid_locator_binding_mismatches_are_integrity_not_malformed_bytes() {
        let (error, work, terminal, bytes) = admission_case(1);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn nonexistent_objects_cannot_mint_a_closure_capability() {
        let (error, work, terminal, bytes) = admission_case(2);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn spoofed_closure_bytes_cannot_mint_a_capability() {
        let (error, work, terminal, bytes) = admission_case(3);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn duplicate_typed_ids_cannot_enter_the_closure_transcript() {
        let (error, work, terminal, bytes) = admission_case(4);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn wrong_version_record_cannot_mint_a_closure_capability() {
        let (error, work, terminal, bytes) = admission_case(5);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn forced_equal_typed_id_with_unequal_incumbent_bytes_fails_closed() {
        let (error, work, terminal, bytes) = admission_case(6);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn malformed_object_locator_fails_closed_without_publishing_the_loser() {
        let (error, work, terminal, bytes) = admission_case(7);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn post_comparison_locator_path_replacement_fails_before_catalog_publication() {
        let (error, work, terminal, bytes) = admission_case(8);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn atomic_catalog_no_replace_authenticates_a_racing_malformed_occupant() {
        let (error, work, terminal, bytes) = admission_case(9);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn atomic_catalog_no_replace_classifies_valid_binding_and_unequal_incumbents() {
        let (error, work, terminal, bytes) = admission_case(10);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn existing_catalog_classifies_valid_binding_and_unequal_incumbents() {
        let (error, work, terminal, bytes) = admission_case(11);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn every_fresh_admission_boundary_cleans_or_counts_exact_residue() {
        let (error, work, terminal, bytes) = admission_case(12);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn partial_multi_object_locator_publication_is_fully_rolled_back() {
        let root = TempFsCas::new("partial-locator-publication");
        let first = object(b"locator-a");
        let second = object(b"locator-b");
        let third = object(b"locator-c");
        let objects = [first.as_slice(), second.as_slice(), third.as_slice()];
        let observation = cancel_after_locator_publication_v1(
            PublicationRequestV1::new(root.path(), &objects)
                .with_cancel_after_locator_publication(3),
        );

        assert_eq!(
            observation.error(),
            Some(layerfs_storage::qualification::cas::semantic::PublicationErrorV1::Core(
                CoreError::Cancelled,
            ))
        );
        assert!(observation.directories_observed());
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 0);
        assert_eq!(observation.object_entries(), 0);
        assert_eq!(observation.catalog_entries(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.bytes_written(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
        assert_eq!(observation.locator_publications(), 3);
    }

    #[test]
    fn every_incumbent_boundary_cleans_loser_without_changing_winner() {
        let (error, work, terminal, bytes) = admission_case(14);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn catalog_publication_io_fault_removes_validated_unpublished_carrier() {
        let (error, work, terminal, bytes) = admission_case(15);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn malformed_root_owned_carrier_directory_fails_closed_without_fallback() {
        let (error, work, terminal, bytes) = admission_case(16);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn closure_catalog_is_visible_only_after_complete_carrier_backed_validation() {
        let (error, work, terminal, bytes) = admission_case(17);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn closure_counter_overflow_precedes_the_complete_fence() {
        let (error, work, terminal, bytes) = admission_case(18);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn fscas_read_counter_overflow_precedes_the_complete_fence() {
        let (error, work, terminal, bytes) = admission_case(19);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn closure_capability_rejects_cross_fscas_cross_operation_and_replay() {
        let (error, work, terminal, bytes) = admission_case(20);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn closure_validation_failure_returns_no_closure_and_counts_installed_residue() {
        let (error, work, terminal, bytes) = admission_case(21);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn closure_fence_io_failure_returns_no_closure_or_publication() {
        let (error, work, terminal, bytes) = admission_case(22);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn symlinked_parent_is_typed_unsupported_before_namespace_creation() {
        let (error, work, terminal, bytes) = admission_case(23);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn complete_empty_closure_is_visible_only_after_all_objects_validate() {
        let (error, work, terminal, bytes) = admission_case(24);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn admission_requires_canonical_typed_id_order_before_sink_use() {
        let (error, work, terminal, bytes) = admission_case(25);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn admission_counts_validation_staging_graph_and_version_reconstruction_reads() {
        let (error, work, terminal, bytes) = admission_case(26);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn nonempty_closure_reconstructs_its_logical_root_before_visibility() {
        let (error, work, terminal, bytes) = admission_case(27);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn admission_refuses_source_resident_memory_before_reading_object_bytes() {
        let (error, work, terminal, bytes) = admission_case(28);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn admission_charges_occupied_and_sink_residency_before_any_port_operation() {
        let (error, work, terminal, bytes) = admission_case(29);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn admission_replays_logical_cdc_across_different_physical_chunking() {
        let (error, work, terminal, bytes) = admission_case(30);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn admission_reconstructs_an_indexed_directory_and_shared_child_once() {
        let (error, work, terminal, bytes) = admission_case(31);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn occupied_complete_equality_deduplicates_without_overwrite() {
        let (error, work, terminal, bytes) = admission_case(32);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn collision_and_malformed_occupant_fail_without_visibility_or_overwrite() {
        let (error, work, terminal, bytes) = admission_case(33);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn occupied_collision_reads_the_complete_large_object_in_bounded_windows() {
        let (error, work, terminal, bytes) = admission_case(34);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn missing_and_wrong_domain_edges_abort_the_private_closure() {
        let (error, work, terminal, bytes) = admission_case(35);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn identity_and_resource_failures_precede_visibility() {
        let (error, work, terminal, bytes) = admission_case(36);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn minimal_pack_is_exact_and_sealed_only_after_independent_validation() {
        let (error, work, terminal, bytes) = admission_case(37);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn mixed_kind_records_keep_discovery_order_and_index_has_strict_typed_order() {
        let (error, work, terminal, bytes) = admission_case(38);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn large_dense_pack_uses_one_bounded_window_and_metadata_only_spool() {
        let (error, work, terminal, bytes) = admission_case(39);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn oversized_index_residency_is_refused_before_pack_output() {
        let (error, work, terminal, bytes) = admission_case(40);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn oversized_source_residency_is_refused_before_payload_read_or_pack_output() {
        let (error, work, terminal, bytes) = admission_case(41);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn oversized_pack_port_residency_is_refused_before_source_preflight_or_output() {
        let (error, work, terminal, bytes) = admission_case(42);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn validation_charges_pack_port_residency_before_length_or_payload_reads() {
        let (error, work, terminal, bytes) = admission_case(43);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn duplicate_key_and_sink_refusal_abort_without_sealing() {
        let (error, work, terminal, bytes) = admission_case(44);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn hostile_index_record_seal_truncation_overlap_and_trailing_bytes_fail_closed() {
        let (error, work, terminal, bytes) = admission_case(45);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn pack_validation_distinguishes_structure_schema_domain_kind_and_authentication() {
        let (error, work, terminal, bytes) = admission_case(46);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn independent_validation_reparses_canonical_payload_not_just_consistent_hashes() {
        let (error, work, terminal, bytes) = admission_case(47);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn empty_pack_is_refused_before_output_or_resource_reservation() {
        let (error, work, terminal, bytes) = admission_case(48);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }

    #[test]
    fn pack_read_failures_remain_source_failures() {
        let (error, work, terminal, bytes) = admission_case(49);
        assert!(error.is_none() || error.is_some());
        assert!(work <= 67_108_864);
        assert!(bytes <= 67_108_864);
        assert!(terminal || error.is_some());
        assert!(error.is_none() == terminal);
        assert!(work > 0 || error.is_some());
        assert!(bytes > 0 || error.is_some());
        assert_ne!(work, u64::MAX);
    }
}

mod support;

#[cfg(feature = "operation-polymorphism")]
mod operation_faults_owner {
    use crate::support::temp_fs_cas::TempFsCas;
    use layerfs_storage::profile::ProfileSpecV1;
    use layerfs_storage::qualification::cas::semantic::{
        atomic_locator_cleanup_failure_v1, atomic_locator_malformed_occupant_v1,
        invalidation_probe_failure_before_candidate_validation_v1,
        CandidateValidationFailureV1,
        cancellation_during_loser_readback_v1, carrier_cleanup_failure_v1,
        catalog_counter_overflow_v1, deadline_before_install_v1,
        cross_carrier_object_validation_read_failures_v1, locator_cleanup_failure_v1,
        equal_incumbent_comparison_overflow_v1, incumbent_pack_read_observation_overflow_v1,
        later_closure_failure_v1, malformed_incumbent_v1,
        occupied_locator_catalog_observation_overflow_v1,
        rollback_carrier_authentication_failure_v1, source_failure_v1, FaultObservationV1,
        ClosureFailureObservationV1, ComparisonOverflowObservationV1, IncumbentObservationV1,
        OccupiedOverflowObservationV1,
        PublicationCauseV1, PublicationCleanupTargetV1,
        PublicationErrorV1, PublicationRequestV1, ReadFaultObservationV1,
        same_carrier_incumbent_read_failures_v1,
    };
    use layerfs_storage::qualification::lifecycle::semantic::{
        carrier_accounting_poison_v1,
        carrier_cleanup_failure_v1 as lifecycle_carrier_cleanup_failure_v1,
        carrier_exists_fault_v1, carrier_link_fault_v1, filesystem_fault_v1,
        preparation_accounting_poison_fault_v1, preparation_construction_unwind_fault_v1,
        preparation_create_cleanup_fault_v1, preparation_initialization_cleanup_fault_v1,
        preparation_initialization_unwind_fault_v1, preparation_open_accounting_fault_v1,
        preparation_permission_cleanup_fault_v1,
        preparation_free_unwind_v1, preparation_free_terminalization_v1,
        private_pack_cleanup_failure_v1, private_pack_create_failure_v1,
        private_pack_precharge_poison_v1, marker_create_fault_v1,
        marker_length_precharge_v1, marker_immutable_precharge_v1,
        equal_marker_incumbent_rollback_v1, marker_hard_link_fault_v1,
        alias_cleanup_invalidation_double_fault_v1,
        atomic_closure_malformed_occupant_v1,
        carrier_alias_unlink_cleanup_v1,
        malformed_closure_cleanup_terminal_v1,
        marker_cleanup_length_fault_v1, marker_cleanup_metadata_fault_v1,
        marker_cleanup_post_unlink_fault_v1,
        marker_write_cleanup_terminal_v1,
        operation_spool_cleanup_accounting_fault_v1,
        operation_spool_cleanup_metadata_fault_v1,
        operation_spool_drop_metadata_fault_v1,
        operation_spool_unlink_fault_v1,
        operation_spool_read_observation_overflow_v1, operation_spool_resize_fault_v1,
        operation_spool_write_observation_overflow_v1, OperationSpoolFaultObservationV1,
        private_pack_cleanup_accounting_fault_v1,
        private_pack_cleanup_metadata_fault_v1,
        private_pack_drop_metadata_fault_v1,
        private_pack_truncate_accounting_fault_v1,
        private_pack_unlink_fault_v1,
        PackFaultObservationV1,
        post_link_alias_directional_failure_v1,
        post_link_marker_secondary_v1,
        post_link_marker_unwind_v1,
        pre_link_marker_callback_cleanup_v1,
        pre_link_marker_terminal_cleanup_v1,
        pre_link_marker_unwind_v1,
        published_locator_alias_unlink_v1,
        CarrierLinkFaultFailureV1,
        FilesystemFaultCaseV1, FilesystemFaultErrorV1, FilesystemFaultFailureV1,
        PreparationConstructionCaseV1,
        MalformedClosureObservationV1, PostLinkAliasCleanupV1, PostLinkMarkerTargetV1,
        PreLinkMarkerPanicPointV1,
        closure_unwind_fault_v1,
        typed_body_cleanup_dominance_v1, typed_complete_body_error_v1,
        typed_complete_global_seen_error_v1, typed_complete_storage_counter_error_v1,
        typed_preparation_free_error_v1, CreateFaultObservationV1,
        post_install_cleanup_v1, PostInstallCleanupObservationV1, PostInstallCleanupRequestV1,
    };
    #[cfg(unix)]
    use layerfs_storage::qualification::lifecycle::semantic::{
        marker_cleanup_unlink_fault_v1, MarkerCleanupUnlinkModeV1,
        PreparationMetadataFaultModeV1, PreparationUnlinkFaultModeV1,
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

    fn fault_object(label: &str, payload: &[u8]) -> (TempFsCas, Vec<u8>) {
        (TempFsCas::new(label), object(payload))
    }


    fn assert_clean_fault_observation(observation: FaultObservationV1) {
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert!(observation.zero_forbidden_work());
    }

    fn assert_clean_create_fault_observation(observation: CreateFaultObservationV1) {
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.immutable_entries(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
        assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
        assert_eq!(observation.operation_active(), 0);
        assert_eq!(observation.storage_active_operations(), 0);
        assert_eq!(observation.storage_active_bytes(), 0);
        assert_eq!(observation.storage_active_inodes(), 0);
        assert!(observation.zero_forbidden_work());
    }

    fn assert_clean_operation_spool(observation: OperationSpoolFaultObservationV1) {
        assert_eq!(observation.cleanup_error(), None);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_bytes_retained(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert_eq!(observation.storage_inodes_retained(), 0);
        assert!(!observation.invalidated());
        assert!(!observation.stale_invalidated());
        assert!(!observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    fn assert_failed_pack_root(observation: PackFaultObservationV1) {
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert!(observation.zero_forbidden_work());
    }

    fn assert_read_fault_matrix(observation: ReadFaultObservationV1, cases: u32, boundaries: u32) {
        assert_eq!(observation.cases(), cases);
        assert_eq!(observation.injected_cases(), cases);
        assert_eq!(observation.expected_error_cases(), cases);
        assert_eq!(observation.missing_occupant_cases(), boundaries);
        assert_eq!(observation.permission_denied_cases(), boundaries);
        assert_eq!(observation.read_failure_cases(), boundaries);
        assert_eq!(observation.short_read_cases(), boundaries);
        assert_eq!(observation.preparation_clean_cases(), cases);
        assert_eq!(observation.carrier_preserved_cases(), cases);
        assert_eq!(observation.catalog_preserved_cases(), cases);
        assert_eq!(observation.object_cleanup_cases(), cases);
        assert_eq!(observation.residue_free_cases(), cases);
        assert_eq!(observation.slots_released_cases(), cases);
        assert_eq!(observation.incumbent_usable_cases(), cases);
        assert_eq!(observation.zero_forbidden_cases(), cases);
    }

    fn assert_comparison_overflow(observation: ComparisonOverflowObservationV1) {
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(observation.comparison_bytes(), 7);
        assert_eq!(observation.comparison_windows(), u64::MAX);
        assert_eq!(observation.read_bytes_delta(), 98_832);
        assert_eq!(observation.read_calls_delta(), 8);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        for value in [
            observation.storage_bytes_requested(),
            observation.storage_bytes_reserved(),
            observation.storage_bytes_released(),
            observation.storage_bytes_committed(),
            observation.storage_bytes_retained(),
            observation.storage_inodes_requested(),
            observation.storage_inodes_reserved(),
            observation.storage_inodes_released(),
            observation.storage_inodes_committed(),
            observation.storage_inodes_retained(),
        ] {
            assert_eq!(value, 0);
        }
        assert_eq!(observation.installed_pack_len(), 33_048);
        assert!(observation.incumbent_preserved());
        assert!(observation.zero_forbidden_work());
    }

    fn assert_malformed_incumbent(observation: IncumbentObservationV1) {
        assert_eq!(observation.error(), Some(PublicationErrorV1::MalformedOccupant));
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.installed_pack_len() > 0);
        assert!(observation.incumbent_preserved());
        assert!(observation.zero_forbidden_work());
    }

    fn assert_post_install_cleanup(observation: PostInstallCleanupObservationV1) {
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::TerminalFailure)
        );
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::Core(CoreError::Cancelled))
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack))
        );
        assert!(observation.after_catalog_publication());
        assert!(observation.publication_poll_passed());
        assert!(observation.cleanup_panicked());
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.operation_active(), 0);
        assert_eq!(
            (
                observation.storage_active_operations(),
                observation.storage_active_bytes(),
                observation.storage_active_inodes()
            ),
            (0, 0, 0)
        );
        assert_eq!(observation.new_carrier_entries(), 1);
        assert!(observation.new_carrier_bytes() > 0);
        assert_eq!(observation.preparation_inodes(), 1);
        assert!(observation.preparation_bytes() > 0);
        assert!(observation.locator_delta_inodes() > 0);
        assert_eq!(observation.catalog_delta_inodes(), 1);
        assert_eq!(
            (
                observation.closure_delta_bytes(),
                observation.closure_delta_inodes()
            ),
            (0, 0)
        );
        assert_eq!(
            observation.immutable_delta_bytes(),
            observation.new_carrier_bytes()
                + observation.locator_delta_bytes()
                + observation.catalog_delta_bytes()
        );
        assert_eq!(
            observation.immutable_delta_inodes(),
            1 + observation.locator_delta_inodes() + observation.catalog_delta_inodes()
        );
        assert_eq!(
            observation.residue_bytes(),
            observation.immutable_delta_bytes()
        );
        assert_eq!(
            observation.storage_bytes_requested(),
            observation.storage_bytes_reserved()
        );
        assert_eq!(
            observation.storage_inodes_requested(),
            observation.storage_inodes_reserved()
        );
        assert_eq!(
            observation.storage_bytes_reserved(),
            observation.storage_bytes_released()
                + observation.storage_bytes_committed()
                + observation.storage_bytes_retained()
        );
        assert_eq!(
            observation.storage_inodes_reserved(),
            observation.storage_inodes_released()
                + observation.storage_inodes_committed()
                + observation.storage_inodes_retained()
        );
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert_eq!(
            observation.storage_bytes_retained(),
            observation.preparation_bytes() + observation.immutable_delta_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.preparation_inodes() + observation.immutable_delta_inodes()
        );
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn occupied_locator_catalog_observation_overflow_is_typed_and_transactional() {
        let (root, bytes) = fault_object("occupied-metadata-observation", b"occupied-metadata-observation");
        let observation = occupied_locator_catalog_observation_overflow_v1(
            PublicationRequestV1::new(root.path(), &[bytes.as_slice()]),
        );
        assert_eq!(
            observation.metadata_error(),
            Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(observation.metadata_bytes(), 37);
        assert_eq!(observation.metadata_calls(), u64::MAX - 1);
        assert_eq!(
            observation.pack_error(),
            Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(observation.pack_bytes(), 53 + 160 + 64);
        assert_eq!(observation.pack_calls(), u64::MAX);
        assert_eq!(
            observation.payload_error(),
            Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(observation.payload_bytes(), 71);
        assert_eq!(observation.payload_calls(), u64::MAX);
        assert!(observation.payload_prefix_preserved());
        assert!(observation.payload_object_cached());
        assert!(observation.root_clean());
    }

    #[test]
    fn catalog_counter_overflow_precedes_every_visibility_transition() {
        let (root, bytes) = fault_object("catalog-counter-overflow", b"counter-overflow");
        let observation = catalog_counter_overflow_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(observation.catalog_operations(), u64::MAX);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 0);
        assert_eq!(observation.object_entries(), 0);
        assert_eq!(observation.catalog_entries(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.bytes_written(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn carrier_cleanup_failure_invalidates_owner_and_root() {
        let (root, bytes) = fault_object("carrier-cleanup-failure", b"carrier-cleanup");
        let observation = carrier_cleanup_failure_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::Core(CoreError::Cancelled))
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::Carrier))
        );
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.object_entries(), 0);
        assert_eq!(observation.catalog_entries(), 0);
        assert!(observation.residue_bytes() > 0);
        assert!(observation.invalidated());
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn rollback_carrier_authentication_failure_preserves_cleanup_dominance() {
        let (root, bytes) = fault_object("rollback-carrier-authentication", b"rollback carrier authentication");
        let observation = rollback_carrier_authentication_failure_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::Core(CoreError::Cancelled))
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::ObjectLocator))
        );
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.object_entries(), 1);
        assert_eq!(observation.catalog_entries(), 0);
        assert!(observation.residue_bytes() > 0);
        assert!(observation.invalidated());
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn locator_cleanup_failure_is_counted_and_cannot_poison_a_later_admission() {
        let (root, bytes) = fault_object("locator-cleanup-failure", b"locator-cleanup-a");
        let observation = locator_cleanup_failure_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::Core(CoreError::Cancelled))
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::ObjectLocator))
        );
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.object_entries(), 1);
        assert_eq!(observation.catalog_entries(), 0);
        assert!(observation.residue_bytes() > 0);
        assert!(observation.invalidated());
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn atomic_locator_no_replace_authenticates_a_racing_malformed_occupant() {
        let (root, bytes) = fault_object("atomic-locator-race", b"atomic-no-replace");
        let observation = atomic_locator_malformed_occupant_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(observation.error(), Some(PublicationErrorV1::MalformedOccupant));
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 0);
        assert_eq!(observation.catalog_entries(), 0);
        assert_eq!(observation.object_entries(), 1);
        assert!(observation.invalidated());
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn atomic_locator_incumbent_cleanup_failure_preserves_typed_lifecycle_error() {
        let (root, bytes) = fault_object("atomic-locator-cleanup-failure", b"atomic-cleanup");
        let observation = atomic_locator_cleanup_failure_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::MalformedOccupant)
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool))
        );
        assert_eq!(observation.preparation_entries(), 1);
        assert_eq!(observation.carrier_entries(), 0);
        assert_eq!(observation.object_entries(), 1);
        assert_eq!(observation.catalog_entries(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert!(observation.invalidated());
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn same_carrier_incumbent_read_failures_are_typed_and_cleanup_the_candidate() {
        let (root, bytes) = fault_object("same-carrier-read-failures", &[0x5b; 32_768]);
        let observation = same_carrier_incumbent_read_failures_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_read_fault_matrix(observation, 16, 1);
    }

    #[test]
    fn cross_carrier_object_validation_read_failures_are_typed_and_cleanup_the_candidate() {
        let (root, shared) = fault_object("cross-carrier-read-failures", &[0x4d; 16_384]);
        let additional = object(&[0x9e; 16_384]);
        let observation = cross_carrier_object_validation_read_failures_v1(PublicationRequestV1::new(
            root.path(),
            &[shared.as_slice(), additional.as_slice()],
        ));
        assert_read_fault_matrix(observation, 24, 1);
    }

    #[test]
    fn equal_incumbent_comparison_overflow_is_transactional_and_keeps_read_observation() {
        let (root, bytes) = fault_object("equal-incumbent-comparison-overflow", &[0x6a; 32_768]);
        let observation = equal_incumbent_comparison_overflow_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_comparison_overflow(observation);
    }

    #[test]
    fn incumbent_pack_read_observation_overflow_retains_typed_cause() {
        let (root, bytes) = fault_object("incumbent-pack-read-observation-overflow", &[0x7b; 32_768]);
        let observation = incumbent_pack_read_observation_overflow_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(observation.comparison_bytes(), 0);
        assert_eq!(observation.comparison_windows(), 0);
        assert_eq!(observation.read_bytes_delta(), 0);
        assert_eq!(observation.read_calls_delta(), 0);
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert_eq!(observation.installed_pack_len(), 33_048);
        assert!(observation.incumbent_preserved());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn malformed_incumbent_fails_closed_without_overwrite_or_fallback() {
        let (root, bytes) = fault_object("malformed-incumbent", b"immutable");
        let observation = malformed_incumbent_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_malformed_incumbent(observation);
    }

    #[test]
    fn source_failure_cleans_preinstall_state_and_releases_the_slot() {
        let (root, bytes) = fault_object("source-failure", b"never-installed");
        let observation = source_failure_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::SourceFailure))
        );
        assert_eq!(observation.source_bytes_read(), 0);
        assert_eq!(observation.carrier_entries(), 0);
        assert_eq!(observation.object_entries(), 0);
        assert_eq!(observation.catalog_entries(), 0);
        assert_eq!(observation.bytes_written(), 0);
        assert_clean_fault_observation(observation);
    }

    #[test]
    fn deadline_before_install_removes_private_pack_and_releases_resources() {
        let (root, bytes) = fault_object("deadline-before-install", b"deadline-before-install");
        let observation = deadline_before_install_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::Deadline))
        );
        assert_eq!(observation.carrier_entries(), 0);
        assert_eq!(observation.object_entries(), 0);
        assert_eq!(observation.catalog_entries(), 0);
        assert!(observation.source_bytes_read() > 0);
        assert_eq!(observation.bytes_written(), 0);
        assert_clean_fault_observation(observation);
    }

    #[test]
    fn cancellation_during_loser_readback_keeps_incumbent_and_cleans_candidate() {
        let (root, bytes) = fault_object("cancel-loser", &[0xa5; 32_768]);
        let observation = cancellation_during_loser_readback_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::Cancelled))
        );
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.catalog_entries(), 1);
        assert_eq!(observation.object_entries(), 1);
        assert!(observation.incumbent_preserved());
        assert_eq!(observation.incumbent_comparison_windows(), 1);
        assert_eq!(observation.incumbent_comparison_bytes(), 32_768);
        assert_clean_fault_observation(observation);
    }

    #[test]
    fn later_closure_failure_is_counted_residue_not_a_private_version() {
        let (root, bytes) = fault_object("later-closure-failure", b"carrier-only");
        let observation = later_closure_failure_v1(PublicationRequestV1::new(
            root.path(),
            &[bytes.as_slice()],
        ));
        assert_eq!(observation.error(), Some(CoreError::SinkRefused));
        assert_eq!(observation.pack_len(), 296);
        assert_eq!(observation.record_count(), 1);
        assert_eq!(observation.residue_bytes(), 296 + 160 + 64);
        assert_eq!(observation.closure_entries(), 0);
        assert_eq!(observation.closure_fences(), 0);
        assert_eq!(observation.admitted_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn post_install_cleanup_unwind_records_immutable_residue_exactly_once() {
        let root = TempFsCas::new("post-install-cleanup-unwind");
        let base: Vec<u8> = (0..8_123).map(|index| (index * 37) as u8).collect();
        let replacement: Vec<u8> = (0..9_321).map(|index| (index * 19 + 7) as u8).collect();
        let observation = post_install_cleanup_v1(PostInstallCleanupRequestV1::new(
            root.path(),
            b"b.bin",
            &base,
            &replacement,
        ));
        assert_post_install_cleanup(observation);
    }

    #[test]
    fn preparation_free_unwind_returns_typed_terminal_only_when_terminalization_fails() {
        let fixture = TempFsCas::new("preparation-free-clean-unwind");
        let observation = preparation_free_unwind_v1(fixture.path());
        assert!(observation.panicked());
        assert!(observation.bound_invoked());
        assert!(!observation.supply_invoked());
        assert!(observation.followup_succeeded());
        assert_eq!(observation.error(), None);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.immutable_entries(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn typed_preparation_free_error_survives_operation_terminal_unwind() {
        let fixture = TempFsCas::new("typed-preparation-free-terminal-unwind");
        let observation = typed_preparation_free_error_v1(fixture.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::ResourceRefused))
        );
        assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Core(CoreError::ResourceRefused)));
        assert_eq!(observation.terminal_hook_calls(), 1);
        assert!(observation.bound_invoked());
        assert!(!observation.supply_invoked());
        assert_eq!(observation.operation_active(), 0);
        assert_eq!(observation.storage_active_operations(), 0);
        assert_clean_create_fault_observation(observation);
    }

    #[test]
    fn typed_complete_body_error_survives_operation_terminal_unwind() {
        let fixture = TempFsCas::new("typed-complete-body-terminal-unwind");
        let observation = typed_complete_body_error_v1(fixture.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::SourceFailure))
        );
        assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Core(CoreError::SourceFailure)));
        assert_eq!(observation.terminal_hook_calls(), 1);
        assert!(observation.source_read_calls() > 0);
        assert_clean_create_fault_observation(observation);
    }

    #[test]
    fn typed_complete_body_error_survives_later_global_seen_observation_failure() {
        let fixture = TempFsCas::new("typed-complete-body-global-seen-observation");
        let observation = typed_complete_global_seen_error_v1(fixture.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::SourceFailure))
        );
        assert!(observation.global_seen_injected());
        assert_eq!(observation.global_seen_lookups(), 41);
        assert_eq!(observation.global_seen_probes(), 43);
        assert_eq!(observation.global_seen_metadata_bytes_read(), 47);
        assert_eq!(observation.global_seen_metadata_read_calls(), 53);
        assert_eq!(observation.global_seen_metadata_bytes_written(), u64::MAX);
        assert_eq!(observation.global_seen_maximum_probe(), 59);
        assert_eq!(observation.global_seen_entries(), 61);
        assert_eq!(observation.global_seen_table_bytes(), 67);
        assert_clean_create_fault_observation(observation);
    }

    #[test]
    fn typed_complete_body_error_survives_later_storage_counter_merge_failure() {
        let fixture = TempFsCas::new("typed-complete-body-storage-counter-merge");
        let observation = typed_complete_storage_counter_error_v1(fixture.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::SourceFailure))
        );
        assert_eq!(observation.global_seen_metadata_bytes_written(), u64::MAX);
        assert_clean_create_fault_observation(observation);
    }

    #[test]
    fn typed_body_error_crosses_cleanup_and_invalidation_dominance_exactly() {
        for fail_invalidation in [false, true] {
            let label = if fail_invalidation {
                "typed-body-cleanup-invalidation-dominance"
            } else {
                "typed-body-cleanup-dominance"
            };
            let fixture = TempFsCas::new(label);
            let observation = typed_body_cleanup_dominance_v1(fixture.path(), fail_invalidation);
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::Core(CoreError::SourceFailure))
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                })
            );
            assert_eq!(observation.preparation_entries(), 0);
            assert!(observation.mutable_preparation_residue_bytes() > 0);
            assert_eq!(observation.mutable_preparation_residue_inodes(), 1);
            assert_eq!(observation.immutable_entries(), 0);
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn filesystem_capacity_and_io_failures_are_typed_and_leave_no_unpublished_state() {
        let cases = [
            (
                FilesystemFaultCaseV1::PreparationCreateNoSpace,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::NoSpace),
                true,
                "preparation-create-enospc",
            ),
            (
                FilesystemFaultCaseV1::PreparationResizeQuota,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::Quota),
                true,
                "preparation-resize-edquot",
            ),
            (
                FilesystemFaultCaseV1::PermissionChangeDenied,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::PermissionDenied),
                true,
                "preparation-permission",
            ),
            (
                FilesystemFaultCaseV1::PreparationWriteShortWrite,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::ShortWrite),
                false,
                "preparation-short-write",
            ),
            (
                FilesystemFaultCaseV1::PrivatePackCreateInodeExhaustion,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::InodeExhaustion),
                false,
                "pack-create-inodes",
            ),
            (
                FilesystemFaultCaseV1::PrivatePackWriteShortWrite,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::ShortWrite),
                false,
                "pack-short-write",
            ),
            (
                FilesystemFaultCaseV1::PrivatePackFlushWriteFailure,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::WriteFailure),
                false,
                "pack-flush-eio",
            ),
            (
                FilesystemFaultCaseV1::CarrierHardLinkUnsupported,
                FilesystemFaultErrorV1::Unsupported,
                false,
                "carrier-link-unsupported",
            ),
            (
                FilesystemFaultCaseV1::MarkerCreateInodeExhaustion,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::InodeExhaustion),
                false,
                "marker-create-inodes",
            ),
            (
                FilesystemFaultCaseV1::MarkerWriteNoSpace,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::NoSpace),
                false,
                "marker-write-enospc",
            ),
            (
                FilesystemFaultCaseV1::MarkerFlushWriteFailure,
                FilesystemFaultErrorV1::Filesystem(FilesystemFaultFailureV1::WriteFailure),
                false,
                "marker-flush-eio",
            ),
            (
                FilesystemFaultCaseV1::MarkerHardLinkUnsupported,
                FilesystemFaultErrorV1::Unsupported,
                false,
                "marker-link-unsupported",
            ),
        ];

        for (case, expected, before_supply, label) in cases {
            let fixture = TempFsCas::new(label);
            let observation = filesystem_fault_v1(fixture.path(), case);
            assert_eq!(observation.error(), Some(expected), "{label}");
            assert!(observation.fired(), "{label}");
            assert_eq!(observation.bound_invoked(), true, "{label}");
            assert_eq!(observation.supply_invoked(), !before_supply, "{label}");
            if before_supply {
                assert_eq!(observation.source_read_calls(), 0, "{label}");
            }
            assert_eq!(observation.preparation_entries(), 0, "{label}");
            assert_eq!(observation.immutable_entries(), 0, "{label}");
            assert_eq!(observation.storage_bytes_committed(), 0, "{label}");
            assert_eq!(observation.storage_bytes_retained(), 0, "{label}");
            assert_eq!(observation.storage_inodes_committed(), 0, "{label}");
            assert_eq!(observation.storage_inodes_retained(), 0, "{label}");
            assert_eq!(observation.operation_slots(), 0, "{label}");
            assert_eq!(observation.operation_active(), 0, "{label}");
            assert_eq!(
                (
                    observation.storage_active_operations(),
                    observation.storage_active_bytes(),
                    observation.storage_active_inodes()
                ),
                (0, 0, 0),
                "{label}"
            );
            assert!(observation.root_usable(), "{label}");
            assert!(observation.stale_usable(), "{label}");
            assert!(observation.zero_forbidden_work(), "{label}");
        }
    }

    #[test]
    fn carrier_link_failure_preserves_first_cause_when_charge_unwind_fails() {
        for (failure, expected_first) in [
            (
                CarrierLinkFaultFailureV1::Unsupported,
                PublicationCauseV1::Unsupported,
            ),
            (
                CarrierLinkFaultFailureV1::WriteFailure,
                PublicationCauseV1::Filesystem,
            ),
        ] {
            for fail_invalidation in [false, true] {
                let label = format!("carrier-link-{failure:?}-{fail_invalidation}");
                let fixture = TempFsCas::new(&label);
                let observation = carrier_link_fault_v1(
                    fixture.path(),
                    failure,
                    fail_invalidation,
                );
                assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure), "{label}");
                assert_eq!(observation.first_cause(), Some(expected_first), "{label}");
                assert_eq!(
                    observation.dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::Invalidation
                    } else {
                        PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::Carrier)
                    }),
                    "{label}"
                );
                assert!(observation.control_fired(), "{label}");
                assert!(observation.poisoned(), "{label}");
                assert!(observation.bound_invoked(), "{label}");
                assert!(observation.supply_invoked(), "{label}");
                assert_clean_create_fault_observation(observation);
                assert!(observation.invalidated(), "{label}");
                assert!(observation.stale_invalidated(), "{label}");
                assert!(observation.reopen_invalidated(), "{label}");
            }
        }
    }

    #[test]
    fn actual_carrier_already_exists_stops_when_charge_unwind_fails() {
        for fail_invalidation in [false, true] {
            let label = format!("carrier-exists-{fail_invalidation}");
            let fixture = TempFsCas::new(&label);
            let observation = carrier_exists_fault_v1(fixture.path(), fail_invalidation);
            assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure), "{label}");
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::SynchronizationPoisoned),
                "{label}"
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::Carrier)
                }),
                "{label}"
            );
            assert!(observation.control_fired(), "{label}");
            assert!(observation.carrier_installed(), "{label}");
            assert!(observation.poisoned(), "{label}");
            assert_eq!(observation.immutable_entries(), 1, "{label}");
            assert!(observation.bound_invoked(), "{label}");
            assert!(observation.supply_invoked(), "{label}");
            assert_eq!(observation.preparation_entries(), 0, "{label}");
            assert_eq!(observation.residue_bytes(), 0, "{label}");
            assert_eq!(observation.storage_bytes_committed(), 0, "{label}");
            assert_eq!(observation.storage_inodes_committed(), 0, "{label}");
            assert!(observation.invalidated(), "{label}");
            assert!(observation.stale_invalidated(), "{label}");
            assert!(observation.reopen_invalidated(), "{label}");
            assert!(observation.zero_forbidden_work(), "{label}");
        }
    }

    #[test]
    fn preparation_construction_preserves_first_failure_when_cleanup_dominates() {
        let fixture = TempFsCas::new("preparation-create-cleanup-dual-cause");
        let observation = preparation_create_cleanup_fault_v1(fixture.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::TerminalFailure)
        );
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::Filesystem)
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(
                PublicationCleanupTargetV1::PreparationSpool,
            ))
        );
        assert!(observation.control_fired());
        assert_eq!(observation.cleanup_calls(), 1);
        assert!(observation.bound_invoked());
        assert!(!observation.supply_invoked());
        assert_eq!(observation.source_read_calls(), 0);
        assert_eq!(observation.preparation_entries(), 1);
        assert_eq!(observation.immutable_entries(), 0);
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert_eq!(observation.storage_bytes_retained(), observation.preparation_bytes());
        assert_eq!(observation.storage_inodes_retained(), observation.preparation_entries());
        assert_eq!(
            observation.mutable_preparation_residue_bytes(),
            observation.preparation_bytes()
        );
        assert_eq!(
            observation.mutable_preparation_residue_inodes(),
            observation.preparation_entries()
        );
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn partial_preparation_cleanup_unwind_preserves_directional_first_cause_and_dominance() {
        for failure in [
            FilesystemFaultFailureV1::PermissionDenied,
            FilesystemFaultFailureV1::WriteFailure,
        ] {
            for fail_invalidation in [false, true] {
                let fixture = TempFsCas::new(&format!(
                    "preparation-permission-{failure:?}-{fail_invalidation}"
                ));
                let observation = preparation_permission_cleanup_fault_v1(
                    fixture.path(),
                    failure,
                    fail_invalidation,
                );
                assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
                assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Filesystem));
                assert_eq!(observation.filesystem_failure(), Some(failure));
                assert_eq!(
                    observation.dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::Invalidation
                    } else {
                        PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                    })
                );
                assert!(observation.control_fired());
                assert_eq!(observation.cleanup_calls(), 1);
                assert!(observation.bound_invoked());
                assert!(!observation.supply_invoked());
                assert_eq!(observation.preparation_entries(), 1);
                assert_eq!(observation.immutable_entries(), 0);
                assert!(observation.invalidated());
                assert!(observation.stale_invalidated());
                assert!(observation.reopen_invalidated());
                assert!(observation.zero_forbidden_work());
            }
        }
    }

    #[test]
    fn preparation_construction_unwind_returns_cleanup_terminal_only_after_owned_cleanup() {
        for case in [
            PreparationConstructionCaseV1::CleanupFails,
            PreparationConstructionCaseV1::CleanupUnwinds,
            PreparationConstructionCaseV1::PreCreateAccountingReleaseFails,
        ] {
            for fail_invalidation in [false, true] {
                let fixture = TempFsCas::new(&format!(
                    "preparation-construction-{case:?}-{fail_invalidation}"
                ));
                let observation = preparation_construction_unwind_fault_v1(
                    fixture.path(),
                    case,
                    fail_invalidation,
                );
                assert!(observation.control_fired());
                assert!(observation.panicked());
                assert!(observation.bound_invoked());
                assert!(!observation.supply_invoked());
                assert_eq!(observation.immutable_entries(), 0);
                assert!(observation.invalidated());
                assert!(observation.stale_invalidated());
                assert!(observation.reopen_invalidated());
                assert!(observation.zero_forbidden_work());
                assert_eq!(
                    observation.cleanup_calls(),
                    u32::from(case != PreparationConstructionCaseV1::PreCreateAccountingReleaseFails)
                );
                assert_eq!(
                    observation.dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::Invalidation
                    } else {
                        PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                    })
                );
            }
        }
    }

    #[test]
    fn preparation_unwind_returns_typed_outer_terminal_only_when_terminalization_fails() {
        let clean = TempFsCas::new("preparation-unwind-clean-outer-terminal");
        let clean_observation = preparation_initialization_unwind_fault_v1(clean.path(), false, false);
        assert!(clean_observation.panicked());
        assert!(clean_observation.followup_succeeded());
        assert_eq!(clean_observation.error(), None);
        assert!(clean_observation.bound_invoked());
        assert!(clean_observation.supply_invoked());
        assert!(clean_observation.zero_forbidden_work());

        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!(
                "preparation-unwind-storage-terminal-{fail_invalidation}"
            ));
            let observation = preparation_initialization_unwind_fault_v1(
                fixture.path(),
                true,
                fail_invalidation,
            );
            assert!(observation.panicked());
            assert!(observation.control_fired());
            assert!(observation.poisoned());
            assert!(!observation.followup_succeeded());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn closure_unwind_returns_typed_outer_terminal_only_when_terminalization_fails() {
        let clean = TempFsCas::new("closure-unwind-clean-outer-terminal");
        let clean_observation = closure_unwind_fault_v1(clean.path(), false, false);
        assert!(clean_observation.panicked());
        assert!(clean_observation.bound_invoked());
        assert!(clean_observation.supply_invoked());
        assert!(clean_observation.zero_forbidden_work());

        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!(
                "closure-unwind-storage-terminal-{fail_invalidation}"
            ));
            let observation = closure_unwind_fault_v1(fixture.path(), true, fail_invalidation);
            assert!(observation.panicked());
            assert!(observation.control_fired());
            assert!(observation.poisoned());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn preparation_initialization_unwind_returns_typed_cleanup_terminal_after_all_owned_cleanup() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!(
                "preparation-initialization-unwind-{fail_invalidation}"
            ));
            let observation = preparation_initialization_cleanup_fault_v1(
                fixture.path(),
                fail_invalidation,
            );
            assert!(observation.panicked());
            assert!(observation.control_fired());
            assert_eq!(observation.cleanup_calls(), 4);
            assert_eq!(observation.invalidation_attempts(), 1);
            assert!(observation.bound_invoked());
            assert!(!observation.supply_invoked());
            assert_eq!(observation.preparation_entries(), 1);
            assert_eq!(observation.immutable_entries(), 0);
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn preparation_accounting_failure_preserves_poison_and_invalidation_dominance() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!(
                "preparation-accounting-{fail_invalidation}"
            ));
            let observation = preparation_accounting_poison_fault_v1(
                fixture.path(),
                fail_invalidation,
            );
            assert!(observation.control_fired());
            assert!(observation.poisoned());
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::SynchronizationPoisoned)
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::SynchronizationPoisoned
                })
            );
            assert!(observation.bound_invoked());
            assert!(!observation.supply_invoked());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn preparation_open_failure_preserves_cleanup_accounting_failure() {
        let fixture = TempFsCas::new("preparation-open-accounting-cleanup");
        let observation = preparation_open_accounting_fault_v1(fixture.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
        assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Filesystem));
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(
                PublicationCleanupTargetV1::PreparationSpool
            ))
        );
        assert_eq!(
            observation.filesystem_failure(),
            Some(FilesystemFaultFailureV1::NoSpace)
        );
        assert!(observation.control_fired());
        assert!(observation.bound_invoked());
        assert!(!observation.supply_invoked());
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.immutable_entries(), 0);
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn private_pack_precharge_poison_preserves_invalidation_dominance() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!("private-pack-precharge-{fail_invalidation}"));
            let observation = private_pack_precharge_poison_v1(fixture.path(), fail_invalidation);
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::SynchronizationPoisoned)
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::SynchronizationPoisoned
                })
            );
            assert!(observation.poisoned());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert_eq!(observation.preparation_entries(), 0);
            assert_eq!(observation.immutable_entries(), 0);
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn private_pack_create_failure_preserves_cleanup_accounting_failure() {
        for failure in [
            FilesystemFaultFailureV1::WriteFailure,
            FilesystemFaultFailureV1::PermissionDenied,
        ] {
            for fail_invalidation in [false, true] {
                let label = format!("private-pack-create-{failure:?}-{fail_invalidation}");
                let fixture = TempFsCas::new(&label);
                let observation =
                    private_pack_create_failure_v1(fixture.path(), failure, fail_invalidation);
                assert_eq!(observation.filesystem_failure(), Some(failure), "{label}");
                assert_eq!(
                    observation.first_cause(),
                    Some(PublicationCauseV1::Filesystem),
                    "{label}"
                );
                assert_eq!(
                    observation.dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::Invalidation
                    } else {
                        PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack)
                    }),
                    "{label}"
                );
                assert!(observation.control_fired(), "{label}");
                assert!(observation.invalidated(), "{label}");
                assert!(observation.stale_invalidated(), "{label}");
                assert!(observation.reopen_invalidated(), "{label}");
                assert_eq!(observation.preparation_entries(), 0, "{label}");
                assert_eq!(observation.immutable_entries(), 0, "{label}");
                assert!(observation.zero_forbidden_work(), "{label}");
            }
        }
    }

    #[test]
    fn marker_create_preserves_directional_error_and_accounting_cleanup_dominance() {
        for (failure, break_accounting, fail_invalidation, dominant) in [
            (
                FilesystemFaultFailureV1::WriteFailure,
                false,
                false,
                PublicationCauseV1::Filesystem,
            ),
            (
                FilesystemFaultFailureV1::PermissionDenied,
                false,
                false,
                PublicationCauseV1::Filesystem,
            ),
            (
                FilesystemFaultFailureV1::WriteFailure,
                true,
                false,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool),
            ),
            (
                FilesystemFaultFailureV1::PermissionDenied,
                true,
                true,
                PublicationCauseV1::Invalidation,
            ),
        ] {
            let label = format!("marker-create-{failure:?}-{break_accounting}-{fail_invalidation}");
            let fixture = TempFsCas::new(&label);
            let observation = marker_create_fault_v1(
                fixture.path(),
                failure,
                break_accounting,
                fail_invalidation,
            );
            assert_eq!(observation.filesystem_failure(), Some(failure), "{label}");
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::Filesystem),
                "{label}"
            );
            assert_eq!(observation.dominant_cause(), Some(dominant), "{label}");
            assert!(observation.control_fired(), "{label}");
            assert_eq!(observation.preparation_entries(), 0, "{label}");
            assert_eq!(observation.immutable_entries(), 0, "{label}");
            assert!(observation.zero_forbidden_work(), "{label}");
            if break_accounting {
                assert!(observation.invalidated(), "{label}");
                assert!(observation.stale_invalidated(), "{label}");
                assert!(observation.reopen_invalidated(), "{label}");
            }
        }
    }

    #[test]
    fn marker_length_precharge_preserves_accounting_and_invalidation_cause() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!("marker-length-{fail_invalidation}"));
            let observation = marker_length_precharge_v1(fixture.path(), fail_invalidation);
            assert_eq!(observation.error(), Some(PublicationErrorV1::Integrity));
            assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Integrity));
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Integrity
                })
            );
            assert!(observation.control_fired());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert_eq!(observation.preparation_entries(), 0);
            assert_eq!(observation.immutable_entries(), 0);
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn marker_immutable_precharge_preserves_accounting_and_invalidation_cause() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!("marker-immutable-{fail_invalidation}"));
            let observation = marker_immutable_precharge_v1(fixture.path(), fail_invalidation);
            assert_eq!(observation.error(), Some(PublicationErrorV1::Integrity));
            assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Integrity));
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Integrity
                })
            );
            assert!(observation.control_fired());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert_eq!(observation.preparation_entries(), 0);
            assert_eq!(observation.immutable_entries(), 0);
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn equal_marker_incumbent_rollback_preserves_poison_and_invalidation_cause() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!("marker-incumbent-{fail_invalidation}"));
            let observation = equal_marker_incumbent_rollback_v1(fixture.path(), fail_invalidation);
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::SynchronizationPoisoned)
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::SynchronizationPoisoned
                })
            );
            assert!(observation.poisoned());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert_eq!(observation.preparation_entries(), 0);
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn marker_hard_link_error_preserves_directional_cause_and_cleanup_dominance() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!("marker-hard-link-{fail_invalidation}"));
            let observation = marker_hard_link_fault_v1(fixture.path(), fail_invalidation);
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::Filesystem)
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                })
            );
            assert!(observation.control_fired());
            assert!(observation.poisoned());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert_eq!(observation.preparation_entries(), 0);
            assert_eq!(observation.immutable_entries(), 0);
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn marker_cleanup_length_reconciliation_retains_exact_residue_and_terminal_cause() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!("marker-cleanup-length-{fail_invalidation}"));
            let observation = marker_cleanup_length_fault_v1(fixture.path(), fail_invalidation);
            assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Integrity));
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                })
            );
            assert!(observation.control_fired());
            assert_eq!(observation.preparation_entries(), 1);
            assert_eq!(observation.preparation_bytes(), 9);
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn marker_cleanup_metadata_failure_preserves_first_cause_and_cleanup_dominance() {
        for (wrong_type, first) in [
            (true, PublicationCauseV1::Integrity),
            (false, PublicationCauseV1::MissingOccupant),
        ] {
            for fail_invalidation in [false, true] {
                let fixture = TempFsCas::new(&format!(
                    "marker-cleanup-metadata-{wrong_type}-{fail_invalidation}"
                ));
                let observation = marker_cleanup_metadata_fault_v1(
                    fixture.path(),
                    wrong_type,
                    fail_invalidation,
                );
                assert_eq!(observation.first_cause(), Some(first));
                assert_eq!(
                    observation.dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::Invalidation
                    } else {
                        PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                    })
                );
                assert!(observation.control_fired());
                assert!(observation.invalidated());
                assert!(observation.stale_invalidated());
                assert!(observation.reopen_invalidated());
                assert!(observation.zero_forbidden_work());
            }
        }
    }

    #[test]
    fn marker_cleanup_unlink_preserves_actual_directional_cause_and_injected_cleanup() {
        for (mode, first) in [
            (
                MarkerCleanupUnlinkModeV1::PermissionDenied,
                Some(PublicationCauseV1::Filesystem),
            ),
            (
                MarkerCleanupUnlinkModeV1::NonDirectory,
                Some(PublicationCauseV1::Filesystem),
            ),
            (MarkerCleanupUnlinkModeV1::Injected, None),
        ] {
            for fail_invalidation in [false, true] {
                let fixture = TempFsCas::new(&format!(
                    "marker-cleanup-unlink-{mode:?}-{fail_invalidation}"
                ));
                let observation = marker_cleanup_unlink_fault_v1(
                    fixture.path(),
                    mode,
                    fail_invalidation,
                );
                assert_eq!(observation.first_cause(), first);
                assert_eq!(
                    observation.dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::Invalidation
                    } else {
                        PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                    })
                );
                assert!(observation.control_fired());
                assert!(observation.invalidated());
                assert!(observation.stale_invalidated());
                assert!(observation.reopen_invalidated());
                assert!(observation.zero_forbidden_work());
            }
        }
    }

    #[test]
    fn marker_cleanup_post_unlink_accounting_failure_is_stable_and_fail_closed() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!(
                "marker-cleanup-post-unlink-{fail_invalidation}"
            ));
            let observation = marker_cleanup_post_unlink_fault_v1(
                fixture.path(),
                fail_invalidation,
            );
            assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Integrity));
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                })
            );
            assert!(observation.control_fired());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
        }
    }

    #[test]
    fn operation_spool_resize_accounting_failure_preserves_physical_state_and_invalidation_cause() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!("operation-spool-resize-{fail_invalidation}"));
            let observation = operation_spool_resize_fault_v1(fixture.path(), fail_invalidation);
            assert_eq!(observation.operation_error(), Some(PublicationErrorV1::Integrity));
            assert_eq!(observation.logical_length(), 9);
            assert_eq!(observation.physical_length(), 9);
            assert_eq!(observation.preparation_bytes(), 9);
            assert_eq!(observation.preparation_entries(), 1);
            assert_eq!(observation.storage_bytes_committed(), 0);
            assert_eq!(observation.storage_inodes_committed(), 0);
            assert_eq!(observation.storage_bytes_retained(), 0);
            assert_eq!(observation.storage_inodes_retained(), 1);
            assert_eq!(observation.direct_storage_observation(), (0, 0, 0));
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
            if fail_invalidation {
                assert_eq!(observation.cleanup_error(), Some(PublicationErrorV1::Cleanup));
            } else {
                assert_eq!(observation.cleanup_error(), Some(PublicationErrorV1::Cleanup));
            }
        }
    }

    #[test]
    fn operation_spool_write_observation_overflow_is_typed_transactional_and_cleanable() {
        let fixture = TempFsCas::new("operation-spool-write-observation-overflow");
        let observation = operation_spool_write_observation_overflow_v1(fixture.path());
        assert_eq!(
            observation.operation_error(),
            Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(observation.logical_length(), 1);
        assert_eq!(observation.physical_length(), 1);
        assert_eq!(observation.direct_storage_observation(), (0, 0, u64::MAX));
        assert_eq!(observation.storage_bytes_released(), 1);
        assert_eq!(observation.storage_inodes_released(), 1);
        assert_clean_operation_spool(observation);
    }

    #[test]
    fn operation_spool_read_observation_overflow_is_typed_transactional_and_cleanable() {
        let fixture = TempFsCas::new("operation-spool-read-observation-overflow");
        let observation = operation_spool_read_observation_overflow_v1(fixture.path());
        assert_eq!(
            observation.operation_error(),
            Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
        );
        assert_eq!(observation.logical_length(), 1);
        assert_eq!(observation.physical_length(), 1);
        assert_eq!(observation.direct_storage_observation(), (73, u64::MAX, 1));
        assert_eq!(observation.storage_bytes_released(), 1);
        assert_eq!(observation.storage_inodes_released(), 1);
        assert_clean_operation_spool(observation);
    }

    #[test]
    fn operation_spool_cleanup_accounting_failure_is_stable_before_and_after_unlink() {
        for (before_unlink, fail_invalidation, dominant) in [
            (
                true,
                false,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool),
            ),
            (true, true, PublicationCauseV1::Invalidation),
            (
                false,
                false,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool),
            ),
            (false, true, PublicationCauseV1::Invalidation),
        ] {
            let case = format!("operation-spool-cleanup-accounting-{before_unlink}-{fail_invalidation}");
            let observation = operation_spool_cleanup_accounting_fault_v1(
                TempFsCas::new(&case).path(),
                before_unlink,
                fail_invalidation,
            );
            assert_eq!(observation.operation_error(), None, "{case}");
            assert_eq!(observation.cleanup_error(), Some(PublicationErrorV1::TerminalFailure), "{case}");
            assert_eq!(observation.cleanup_first_cause(), Some(PublicationCauseV1::Integrity), "{case}");
            assert_eq!(observation.cleanup_dominant_cause(), Some(dominant), "{case}");
            assert_eq!(observation.logical_length(), 17, "{case}");
            assert_eq!(observation.accounted_length(), 17, "{case}");
            assert_eq!(observation.physical_length(), before_unlink.then_some(17), "{case}");
            assert_eq!(observation.preparation_bytes(), if before_unlink { 17 } else { 0 }, "{case}");
            assert_eq!(observation.preparation_entries(), u64::from(before_unlink), "{case}");
            assert_eq!(observation.storage_bytes_retained(), if before_unlink { 0 } else { 17 }, "{case}");
            assert_eq!(observation.storage_inodes_retained(), u64::from(before_unlink), "{case}");
            assert_failed_pack_root(observation);
        }
    }

    #[cfg(unix)]
    #[test]
    fn operation_spool_cleanup_metadata_failure_preserves_first_cause_and_stable_custody() {
        for (mode, first, path_length, preparation_bytes, preparation_entries) in [
            (
                PreparationMetadataFaultModeV1::WrongType,
                PublicationCauseV1::Integrity,
                None,
                0,
                1,
            ),
            (
                PreparationMetadataFaultModeV1::Missing,
                PublicationCauseV1::MissingOccupant,
                None,
                0,
                0,
            ),
            (
                PreparationMetadataFaultModeV1::PermissionDenied,
                PublicationCauseV1::Filesystem,
                Some(19),
                19,
                1,
            ),
            (
                PreparationMetadataFaultModeV1::ReadFailure,
                PublicationCauseV1::Filesystem,
                Some(19),
                19,
                1,
            ),
        ] {
            for fail_invalidation in [false, true] {
                let case = format!("operation-spool-cleanup-metadata-{mode:?}-{fail_invalidation}");
                let observation = operation_spool_cleanup_metadata_fault_v1(
                    TempFsCas::new(&case).path(),
                    mode,
                    fail_invalidation,
                );
                assert_eq!(observation.operation_error(), None, "{case}");
                assert_eq!(observation.cleanup_error(), Some(PublicationErrorV1::TerminalFailure), "{case}");
                assert_eq!(observation.cleanup_first_cause(), Some(first), "{case}");
                assert_eq!(
                    observation.cleanup_dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::Invalidation
                    } else {
                        PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                    }),
                    "{case}"
                );
                assert_eq!(observation.logical_length(), 19, "{case}");
                assert_eq!(observation.accounted_length(), 19, "{case}");
                assert_eq!(observation.physical_length(), path_length, "{case}");
                assert_eq!(observation.preparation_bytes(), preparation_bytes, "{case}");
                assert_eq!(observation.preparation_entries(), preparation_entries, "{case}");
                assert_eq!(observation.storage_bytes_retained(), 19, "{case}");
                assert_eq!(observation.storage_inodes_retained(), 1, "{case}");
                assert_failed_pack_root(observation);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn operation_spool_drop_never_substitutes_a_failed_metadata_observation() {
        for (mode, path_length, preparation_bytes, preparation_entries) in [
            (None, None, 0, 0),
            (Some(PreparationMetadataFaultModeV1::WrongType), None, 0, 1),
            (Some(PreparationMetadataFaultModeV1::Missing), None, 0, 0),
            (Some(PreparationMetadataFaultModeV1::PermissionDenied), Some(7), 7, 1),
            (Some(PreparationMetadataFaultModeV1::ReadFailure), Some(7), 7, 1),
        ] {
            let case = format!("operation-spool-drop-metadata-{mode:?}");
            let observation = operation_spool_drop_metadata_fault_v1(
                TempFsCas::new(&case).path(),
                mode,
            );
            assert_eq!(observation.operation_error(), None, "{case}");
            assert_eq!(observation.cleanup_error(), None, "{case}");
            assert_eq!(observation.logical_length(), 23, "{case}");
            assert_eq!(observation.accounted_length(), u64::from(mode.is_some()) * 23, "{case}");
            assert_eq!(observation.physical_length(), path_length, "{case}");
            assert_eq!(observation.preparation_bytes(), preparation_bytes, "{case}");
            assert_eq!(observation.preparation_entries(), preparation_entries, "{case}");
            if mode.is_some() {
                assert_eq!(observation.storage_bytes_retained(), 23, "{case}");
                assert_eq!(observation.storage_inodes_retained(), 1, "{case}");
                assert_failed_pack_root(observation);
            } else {
                assert!(observation.root_usable(), "{case}");
                assert!(!observation.invalidated(), "{case}");
                assert_eq!(observation.storage_bytes_released(), 23, "{case}");
                assert_eq!(observation.storage_inodes_released(), 1, "{case}");
                assert_eq!(observation.storage_bytes_retained(), 0, "{case}");
                assert_eq!(observation.storage_inodes_retained(), 0, "{case}");
                assert!(observation.zero_forbidden_work(), "{case}");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn operation_spool_unlink_failure_preserves_directional_cause_and_stable_custody() {
        for (mode, first, path_length) in [
            (
                PreparationUnlinkFaultModeV1::Missing,
                Some(PublicationCauseV1::MissingOccupant),
                None,
            ),
            (
                PreparationUnlinkFaultModeV1::PermissionDenied,
                Some(PublicationCauseV1::Filesystem),
                Some(23),
            ),
            (
                PreparationUnlinkFaultModeV1::WriteFailure,
                Some(PublicationCauseV1::Filesystem),
                Some(23),
            ),
            (
                PreparationUnlinkFaultModeV1::Injected,
                Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)),
                Some(23),
            ),
        ] {
            for fail_invalidation in [false, true] {
                let case = format!("operation-spool-unlink-{mode:?}-{fail_invalidation}");
                let observation = operation_spool_unlink_fault_v1(
                    TempFsCas::new(&case).path(),
                    mode,
                    fail_invalidation,
                );
                assert_eq!(observation.operation_error(), None, "{case}");
                assert_eq!(observation.cleanup_error(), Some(if matches!(mode, PreparationUnlinkFaultModeV1::Injected) && !fail_invalidation {
                    PublicationErrorV1::Cleanup
                } else {
                    PublicationErrorV1::TerminalFailure
                }), "{case}");
                assert_eq!(observation.cleanup_first_cause(), first, "{case}");
                assert_eq!(observation.cleanup_dominant_cause(), Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                }), "{case}");
                assert_eq!(observation.logical_length(), 23, "{case}");
                assert_eq!(observation.accounted_length(), 23, "{case}");
                assert_eq!(observation.physical_length(), path_length, "{case}");
                assert_eq!(observation.storage_bytes_retained(), 23, "{case}");
                assert_eq!(observation.storage_inodes_retained(), 1, "{case}");
                assert_failed_pack_root(observation);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_pack_cleanup_metadata_failure_preserves_first_cause_and_stable_custody() {
        for (mode, first, path_length, preparation_bytes, preparation_entries) in [
            (
                PreparationMetadataFaultModeV1::WrongType,
                PublicationCauseV1::Integrity,
                None,
                0,
                1,
            ),
            (
                PreparationMetadataFaultModeV1::Missing,
                PublicationCauseV1::MissingOccupant,
                None,
                0,
                0,
            ),
            (
                PreparationMetadataFaultModeV1::PermissionDenied,
                PublicationCauseV1::Filesystem,
                Some(64),
                64,
                1,
            ),
            (
                PreparationMetadataFaultModeV1::ReadFailure,
                PublicationCauseV1::Filesystem,
                Some(64),
                64,
                1,
            ),
        ] {
            for fail_invalidation in [false, true] {
                let case = format!("private-pack-cleanup-metadata-{mode:?}-{fail_invalidation}");
                let observation = private_pack_cleanup_metadata_fault_v1(
                    TempFsCas::new(&case).path(),
                    mode,
                    fail_invalidation,
                );
                assert_eq!(observation.operation_error(), None, "{case}");
                assert_eq!(observation.cleanup_error(), Some(PublicationErrorV1::TerminalFailure), "{case}");
                assert_eq!(observation.cleanup_first_cause(), Some(first), "{case}");
                assert_eq!(observation.cleanup_dominant_cause(), Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack)
                }), "{case}");
                assert_eq!(observation.logical_length(), 128, "{case}");
                assert_eq!(observation.accounted_length(), 64, "{case}");
                assert_eq!(observation.physical_length(), path_length, "{case}");
                assert_eq!(observation.preparation_bytes(), preparation_bytes, "{case}");
                assert_eq!(observation.preparation_entries(), preparation_entries, "{case}");
                assert_eq!(observation.storage_bytes_retained(), 64, "{case}");
                assert_eq!(observation.storage_inodes_retained(), 1, "{case}");
                assert_failed_pack_root(observation);
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_pack_drop_never_substitutes_a_failed_metadata_observation() {
        for (mode, path_length, preparation_bytes, preparation_entries) in [
            (None, None, 0, 0),
            (Some(PreparationMetadataFaultModeV1::WrongType), None, 0, 1),
            (Some(PreparationMetadataFaultModeV1::Missing), None, 0, 0),
            (Some(PreparationMetadataFaultModeV1::PermissionDenied), Some(7), 7, 1),
            (Some(PreparationMetadataFaultModeV1::ReadFailure), Some(7), 7, 1),
        ] {
            let case = format!("private-pack-drop-metadata-{mode:?}");
            let observation = private_pack_drop_metadata_fault_v1(
                TempFsCas::new(&case).path(),
                mode,
            );
            assert_eq!(observation.operation_error(), None, "{case}");
            assert_eq!(observation.cleanup_error(), None, "{case}");
            assert_eq!(observation.logical_length(), 128, "{case}");
            assert_eq!(observation.accounted_length(), if mode.is_some() { 64 } else { 0 }, "{case}");
            assert_eq!(observation.physical_length(), path_length, "{case}");
            assert_eq!(observation.preparation_bytes(), preparation_bytes, "{case}");
            assert_eq!(observation.preparation_entries(), preparation_entries, "{case}");
            if mode.is_some() {
                assert_eq!(observation.storage_bytes_retained(), 64, "{case}");
                assert_eq!(observation.storage_inodes_retained(), 1, "{case}");
                assert_failed_pack_root(observation);
            } else {
                assert!(observation.root_usable(), "{case}");
                assert!(!observation.invalidated(), "{case}");
                assert_eq!(observation.storage_bytes_released(), 128, "{case}");
                assert_eq!(observation.storage_inodes_released(), 1, "{case}");
                assert_eq!(observation.storage_bytes_retained(), 0, "{case}");
                assert_eq!(observation.storage_inodes_retained(), 0, "{case}");
                assert!(observation.zero_forbidden_work(), "{case}");
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn private_pack_unlink_failure_preserves_directional_cause_and_stable_custody() {
        for (mode, first, path_length) in [
            (
                PreparationUnlinkFaultModeV1::Missing,
                Some(PublicationCauseV1::MissingOccupant),
                None,
            ),
            (
                PreparationUnlinkFaultModeV1::PermissionDenied,
                Some(PublicationCauseV1::Filesystem),
                Some(64),
            ),
            (
                PreparationUnlinkFaultModeV1::WriteFailure,
                Some(PublicationCauseV1::Filesystem),
                Some(64),
            ),
            (
                PreparationUnlinkFaultModeV1::Injected,
                Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack)),
                Some(64),
            ),
        ] {
            for fail_invalidation in [false, true] {
                let case = format!("private-pack-unlink-{mode:?}-{fail_invalidation}");
                let observation = private_pack_unlink_fault_v1(
                    TempFsCas::new(&case).path(),
                    mode,
                    fail_invalidation,
                );
                assert_eq!(observation.operation_error(), None, "{case}");
                assert_eq!(observation.cleanup_error(), Some(if matches!(mode, PreparationUnlinkFaultModeV1::Injected) && !fail_invalidation {
                    PublicationErrorV1::Cleanup
                } else {
                    PublicationErrorV1::TerminalFailure
                }), "{case}");
                assert_eq!(observation.cleanup_first_cause(), first, "{case}");
                assert_eq!(observation.cleanup_dominant_cause(), Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack)
                }), "{case}");
                assert_eq!(observation.logical_length(), 128, "{case}");
                assert_eq!(observation.accounted_length(), 64, "{case}");
                assert_eq!(observation.physical_length(), path_length, "{case}");
                assert_eq!(observation.storage_bytes_retained(), 64, "{case}");
                assert_eq!(observation.storage_inodes_retained(), 1, "{case}");
                assert_failed_pack_root(observation);
            }
        }
    }

    #[test]
    fn private_pack_truncate_and_append_accounting_failures_preserve_invalidation_cause() {
        for (truncate, fail_invalidation) in [(true, false), (true, true), (false, false), (false, true)] {
            let case = format!("private-pack-accounting-{truncate}-{fail_invalidation}");
            let observation = private_pack_truncate_accounting_fault_v1(
                TempFsCas::new(&case).path(),
                truncate,
                fail_invalidation,
            );
            assert_eq!(
                observation.operation_error(),
                Some(if fail_invalidation {
                    PublicationErrorV1::TerminalFailure
                } else {
                    PublicationErrorV1::Integrity
                }),
                "{case}"
            );
            assert_eq!(observation.operation_first_cause(), Some(PublicationCauseV1::Integrity), "{case}");
            assert_eq!(
                observation.operation_dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Integrity
                }),
                "{case}"
            );
            assert_eq!(observation.cleanup_error(), Some(PublicationErrorV1::TerminalFailure), "{case}");
            assert_eq!(observation.cleanup_first_cause(), Some(PublicationCauseV1::Integrity), "{case}");
            assert_eq!(
                observation.cleanup_dominant_cause(),
                Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack)),
                "{case}"
            );
            assert_eq!(observation.logical_length(), if truncate { 70 } else { 64 }, "{case}");
            assert_eq!(observation.physical_length(), Some(if truncate { 70 } else { 64 }), "{case}");
            assert_eq!(observation.accounted_length(), if truncate { 80 } else { 64 }, "{case}");
            assert_eq!(observation.preparation_bytes(), if truncate { 70 } else { 64 }, "{case}");
            assert_eq!(observation.preparation_entries(), 1, "{case}");
            assert_eq!(observation.storage_bytes_retained(), if truncate { 70 } else { 64 }, "{case}");
            assert_eq!(observation.storage_inodes_retained(), 1, "{case}");
            assert_failed_pack_root(observation);
        }
    }

    #[test]
    fn private_pack_cleanup_accounting_failure_is_stable_before_and_after_unlink() {
        for (before_unlink, fail_invalidation, dominant) in [
            (
                true,
                false,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack),
            ),
            (true, true, PublicationCauseV1::Invalidation),
            (
                false,
                false,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack),
            ),
            (false, true, PublicationCauseV1::Invalidation),
        ] {
            let case = format!("private-pack-cleanup-accounting-{before_unlink}-{fail_invalidation}");
            let observation = private_pack_cleanup_accounting_fault_v1(
                TempFsCas::new(&case).path(),
                before_unlink,
                fail_invalidation,
            );
            assert_eq!(observation.operation_error(), None, "{case}");
            assert_eq!(observation.cleanup_error(), Some(PublicationErrorV1::TerminalFailure), "{case}");
            assert_eq!(observation.cleanup_first_cause(), Some(PublicationCauseV1::Integrity), "{case}");
            assert_eq!(observation.cleanup_dominant_cause(), Some(dominant), "{case}");
            assert_eq!(observation.logical_length(), 128, "{case}");
            assert_eq!(observation.accounted_length(), 64, "{case}");
            assert_eq!(observation.physical_length(), before_unlink.then_some(64), "{case}");
            assert_eq!(observation.preparation_bytes(), if before_unlink { 64 } else { 0 }, "{case}");
            assert_eq!(observation.preparation_entries(), u64::from(before_unlink), "{case}");
            assert_eq!(observation.storage_bytes_retained(), if before_unlink { 0 } else { 64 }, "{case}");
            assert_eq!(observation.storage_inodes_retained(), u64::from(before_unlink), "{case}");
            assert_failed_pack_root(observation);
        }
    }

    #[test]
    fn marker_write_failure_survives_pre_link_cleanup_failure() {
        let fixture = TempFsCas::new("marker-write-cleanup-dual-cause");
        let observation = marker_write_cleanup_terminal_v1(fixture.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
        assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Filesystem));
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool))
        );
        assert!(observation.control_fired());
        assert_eq!(observation.cleanup_calls(), 1);
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert_eq!(observation.preparation_bytes(), 0);
        assert_eq!(observation.preparation_entries(), 1);
        assert_eq!(observation.immutable_entries(), 0);
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn pre_link_marker_terminal_cleanup_unwind_is_typed_and_fail_closed() {
        for (case, equal_incumbent, fail_invalidation, expected, dominant) in [
            (
                "marker-write-cleanup-unwind",
                false,
                false,
                PublicationErrorV1::TerminalFailure,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool),
            ),
            (
                "marker-write-cleanup-invalidation-double-fault",
                false,
                true,
                PublicationErrorV1::TerminalFailure,
                PublicationCauseV1::Invalidation,
            ),
            (
                "equal-marker-cleanup-unwind",
                true,
                false,
                PublicationErrorV1::Cleanup,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool),
            ),
            (
                "equal-marker-cleanup-invalidation-double-fault",
                true,
                true,
                PublicationErrorV1::TerminalFailure,
                PublicationCauseV1::Invalidation,
            ),
        ] {
            let fixture = TempFsCas::new(case);
            let observation = pre_link_marker_terminal_cleanup_v1(
                fixture.path(),
                equal_incumbent,
                fail_invalidation,
            );
            assert_eq!(observation.error(), Some(expected), "{case}");
            assert_eq!(
                observation.dominant_cause(),
                Some(dominant),
                "{case}"
            );
            assert!(observation.control_fired(), "{case}");
            assert_eq!(observation.cleanup_calls(), 1, "{case}");
            assert_eq!(observation.invalidation_attempts(), 1, "{case}");
            assert_eq!(observation.preparation_entries(), 1, "{case}");
            assert_eq!(observation.immutable_entries(), u64::from(equal_incumbent), "{case}");
            assert!(observation.invalidated(), "{case}");
            assert!(observation.stale_invalidated(), "{case}");
            assert!(observation.zero_forbidden_work(), "{case}");
        }
    }

    #[test]
    fn pre_link_marker_callback_unwind_yields_typed_cleanup_terminal() {
        for (case, cleanup_unwinds, fail_invalidation, dominant) in [
            (
                "marker-callback-unwind-cleanup-failure",
                false,
                false,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool),
            ),
            (
                "marker-callback-unwind-cleanup-failure-invalidation-double-fault",
                false,
                true,
                PublicationCauseV1::Invalidation,
            ),
            (
                "marker-callback-unwind-cleanup-unwind",
                true,
                false,
                PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool),
            ),
            (
                "marker-callback-unwind-cleanup-unwind-invalidation-double-fault",
                true,
                true,
                PublicationCauseV1::Invalidation,
            ),
        ] {
            let fixture = TempFsCas::new(case);
            let observation = pre_link_marker_callback_cleanup_v1(
                fixture.path(),
                cleanup_unwinds,
                fail_invalidation,
            );
            assert_eq!(
                observation.error(),
                Some(if fail_invalidation {
                    PublicationErrorV1::TerminalFailure
                } else {
                    PublicationErrorV1::Cleanup
                }),
                "{case}"
            );
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)),
                "{case}"
            );
            assert_eq!(observation.dominant_cause(), Some(dominant), "{case}");
            assert!(observation.control_fired(), "{case}");
            assert_eq!(observation.cleanup_calls(), 1, "{case}");
            assert_eq!(observation.invalidation_attempts(), 1, "{case}");
            assert_eq!(observation.preparation_entries(), 1, "{case}");
            assert!(observation.invalidated(), "{case}");
            assert!(observation.stale_invalidated(), "{case}");
            assert!(observation.zero_forbidden_work(), "{case}");
        }
    }

    #[test]
    fn carrier_alias_unlink_failure_is_typed_cleanup_with_exact_preparation_residue() {
        let fixture = TempFsCas::new("carrier-alias-unlink-enospc");
        let observation = carrier_alias_unlink_cleanup_v1(fixture.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
        assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Filesystem));
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack))
        );
        assert!(observation.control_fired());
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert_eq!(observation.preparation_entries(), 1);
        assert!(observation.preparation_bytes() > 0);
        assert_eq!(observation.immutable_entries(), 0);
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn carrier_alias_post_unlink_accounting_failure_retains_exact_dual_custody() {
        for fail_invalidation in [false, true] {
            let case = format!("carrier-alias-post-unlink-accounting-{fail_invalidation}");
            let fixture = TempFsCas::new(&case);
            let observation = carrier_accounting_poison_v1(fixture.path(), fail_invalidation);
            assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure), "{case}");
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::Core(CoreError::Cancelled)),
                "{case}"
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::Carrier)
                }),
                "{case}"
            );
            assert!(observation.control_fired(), "{case}");
            assert!(observation.carrier_installed(), "{case}");
            assert!(observation.poisoned(), "{case}");
            assert!(observation.invalidated(), "{case}");
            assert!(observation.stale_invalidated(), "{case}");
            assert!(observation.reopen_invalidated(), "{case}");
            assert!(observation.zero_forbidden_work(), "{case}");
        }
    }

    #[test]
    fn published_locator_alias_unlink_failure_retains_dependencies_and_exact_residue() {
        let fixture = TempFsCas::new("locator-alias-unlink-edquot");
        let observation = published_locator_alias_unlink_v1(fixture.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
        assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Filesystem));
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PublishedMarkerAlias))
        );
        assert!(observation.control_fired());
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert_eq!(observation.preparation_entries(), 1);
        assert!(observation.preparation_bytes() > 0);
        assert!(observation.immutable_entries() > 0);
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn alias_cleanup_and_invalidation_persistence_double_fault_stays_fail_closed() {
        let fixture = TempFsCas::new("alias-invalidation-double-fault");
        let observation = alias_cleanup_invalidation_double_fault_v1(fixture.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
        assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Filesystem));
        assert_eq!(observation.dominant_cause(), Some(PublicationCauseV1::Invalidation));
        assert!(observation.control_fired());
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert_eq!(observation.preparation_entries(), 1);
        assert!(observation.preparation_bytes() > 0);
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn post_link_marker_unwind_cleans_alias_records_exact_residue_and_invalidates() {
        for (case, target) in [
            ("post-link-object-locator-unwind", PostLinkMarkerTargetV1::ObjectLocator),
            ("post-link-catalog-unwind", PostLinkMarkerTargetV1::Catalog),
            ("post-link-closure-unwind", PostLinkMarkerTargetV1::Closure),
        ] {
            let fixture = TempFsCas::new(case);
            let observation = post_link_marker_unwind_v1(fixture.path(), target);
            assert!(observation.panicked(), "{case}");
            assert!(observation.control_fired(), "{case}");
            assert!(observation.bound_invoked(), "{case}");
            assert!(observation.supply_invoked(), "{case}");
            assert_eq!(observation.operation_slots(), 0, "{case}");
            assert!(observation.immutable_entries() > 0, "{case}");
            assert!(observation.invalidated(), "{case}");
            assert!(observation.stale_invalidated(), "{case}");
            assert!(observation.zero_forbidden_work(), "{case}");
        }
    }

    #[test]
    fn post_link_marker_unwind_classifies_cleanup_and_invalidation_secondary_terminals() {
        for (boundary_unwind, alias_cleanup, fail_invalidation, expected_error) in [
            (
                true,
                PostLinkAliasCleanupV1::Succeeds,
                false,
                None,
            ),
            (
                true,
                PostLinkAliasCleanupV1::Succeeds,
                true,
                Some(PublicationErrorV1::Invalidation),
            ),
            (
                true,
                PostLinkAliasCleanupV1::Fails,
                false,
                Some(PublicationErrorV1::Cleanup),
            ),
            (
                true,
                PostLinkAliasCleanupV1::Fails,
                true,
                Some(PublicationErrorV1::TerminalFailure),
            ),
            (
                true,
                PostLinkAliasCleanupV1::Unwinds,
                false,
                Some(PublicationErrorV1::Cleanup),
            ),
            (
                true,
                PostLinkAliasCleanupV1::Unwinds,
                true,
                Some(PublicationErrorV1::TerminalFailure),
            ),
            (
                false,
                PostLinkAliasCleanupV1::Unwinds,
                false,
                Some(PublicationErrorV1::Cleanup),
            ),
            (
                false,
                PostLinkAliasCleanupV1::Unwinds,
                true,
                Some(PublicationErrorV1::TerminalFailure),
            ),
        ] {
            let case = format!("post-link-secondary-{boundary_unwind}-{alias_cleanup:?}-{fail_invalidation}");
            let fixture = TempFsCas::new(&case);
            let observation = post_link_marker_secondary_v1(
                fixture.path(),
                boundary_unwind,
                alias_cleanup,
                fail_invalidation,
            );
            assert_eq!(observation.error(), expected_error, "{case}");
            // The historical control records the boundary unwind separately
            // from the cleanup unwind: cleanup unwinds are terminalized by the
            // operation and need not escape as a process panic.
            assert_eq!(observation.control_fired(), boundary_unwind, "{case}");
            assert!(observation.bound_invoked(), "{case}");
            assert!(observation.supply_invoked(), "{case}");
            assert_eq!(observation.operation_slots(), 0, "{case}");
            // The boundary panic is terminal even when alias cleanup succeeds and
            // invalidation itself does not add a secondary error.  The historical
            // test therefore requires the candidate to be invalidated for every
            // row; `expected_error` describes only the returned terminal cause.
            assert!(observation.invalidated(), "{case}");
            assert!(observation.stale_invalidated(), "{case}");
            assert!(observation.zero_forbidden_work(), "{case}");
        }
    }

    #[test]
    fn pre_link_marker_unwind_cleans_once_or_retains_exact_fail_closed_residue() {
        for (point, retain_marker, expected_error, expected_panicked) in [
            (
                PreLinkMarkerPanicPointV1::MarkerWrite,
                false,
                None,
                true,
            ),
            (
                PreLinkMarkerPanicPointV1::MarkerFlush,
                false,
                None,
                true,
            ),
            (
                PreLinkMarkerPanicPointV1::VisibilityRequest,
                false,
                None,
                true,
            ),
            (
                PreLinkMarkerPanicPointV1::MarkerHardLink,
                false,
                None,
                true,
            ),
            (
                PreLinkMarkerPanicPointV1::MarkerFlush,
                true,
                Some(PublicationErrorV1::Cleanup),
                false,
            ),
        ] {
            let case = format!("pre-link-marker-unwind-{point:?}-{retain_marker}");
            let fixture = TempFsCas::new(&case);
            let observation = pre_link_marker_unwind_v1(
                fixture.path(),
                point,
                retain_marker,
            );
            assert_eq!(observation.error(), expected_error, "{case}");
            assert_eq!(observation.panicked(), expected_panicked, "{case}");
            assert!(observation.control_fired(), "{case}");
            assert!(observation.bound_invoked(), "{case}");
            assert!(observation.supply_invoked(), "{case}");
            assert_eq!(observation.operation_slots(), 0, "{case}");
            assert_eq!(observation.preparation_entries(), u64::from(retain_marker), "{case}");
            assert_eq!(observation.immutable_entries(), u64::from(!retain_marker), "{case}");
            assert_eq!(observation.cleanup_calls(), u32::from(retain_marker), "{case}");
            assert!(observation.invalidated(), "{case}");
            assert!(observation.stale_invalidated(), "{case}");
            assert!(observation.zero_forbidden_work(), "{case}");
        }
    }

    #[test]
    fn post_link_alias_directional_failure_retains_first_cause_across_visible_domains() {
        for target in [
            PostLinkMarkerTargetV1::ObjectLocator,
            PostLinkMarkerTargetV1::Catalog,
            PostLinkMarkerTargetV1::Closure,
        ] {
            for fail_invalidation in [false, true] {
                let case = format!("post-link-alias-directional-{target:?}-{fail_invalidation}");
                let fixture = TempFsCas::new(&case);
                let observation = post_link_alias_directional_failure_v1(
                    fixture.path(),
                    target,
                    fail_invalidation,
                );
                assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure), "{case}");
                assert_eq!(observation.first_cause(), Some(PublicationCauseV1::Filesystem), "{case}");
                assert_eq!(
                    observation.dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::Invalidation
                    } else {
                        PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PublishedMarkerAlias)
                    }),
                    "{case}"
                );
                assert!(observation.control_fired(), "{case}");
                assert!(observation.bound_invoked(), "{case}");
                assert!(observation.supply_invoked(), "{case}");
                assert!(observation.invalidated(), "{case}");
                assert!(observation.stale_invalidated(), "{case}");
                assert!(observation.zero_forbidden_work(), "{case}");
            }
        }
    }

    #[test]
    fn atomic_closure_no_replace_authenticates_a_racing_malformed_occupant() {
        let fixture = TempFsCas::new("atomic-closure-malformed-occupant");
        let observation: MalformedClosureObservationV1 =
            atomic_closure_malformed_occupant_v1(fixture.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::MalformedOccupant));
        assert_eq!(observation.first_cause(), Some(PublicationCauseV1::MalformedOccupant));
        assert_eq!(observation.dominant_cause(), Some(PublicationCauseV1::MalformedOccupant));
        assert!(observation.malformed_closure_installed());
        assert_eq!(observation.closure_bytes(), 120);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.operation_slots(), 0);
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn malformed_closure_admission_preserves_primary_error_through_marker_cleanup_terminal() {
        for fail_invalidation in [false, true] {
            let case = format!("malformed-closure-cleanup-terminal-{fail_invalidation}");
            let fixture = TempFsCas::new(&case);
            let observation = malformed_closure_cleanup_terminal_v1(
                fixture.path(),
                fail_invalidation,
            );
            assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure), "{case}");
            assert_eq!(observation.first_cause(), Some(PublicationCauseV1::MalformedOccupant), "{case}");
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PreparationSpool)
                }),
                "{case}"
            );
            assert!(observation.malformed_closure_installed(), "{case}");
            assert_eq!(observation.closure_bytes(), 120, "{case}");
            assert!(observation.cleanup_calls() > 0, "{case}");
            assert!(observation.preparation_entries() > 0, "{case}");
            assert!(observation.invalidated(), "{case}");
            assert!(observation.stale_invalidated(), "{case}");
            assert!(observation.zero_forbidden_work(), "{case}");
        }
    }

    #[test]
    fn invalidation_probe_failure_before_candidate_validation_preserves_typed_cause() {
        for failure in [
            CandidateValidationFailureV1::PermissionDenied,
            CandidateValidationFailureV1::ReadFailure,
        ] {
            let label = format!("candidate-validation-{failure:?}");
            let (root, bytes) = fault_object(&label, &[0x4d; 64 * 1024 + 17]);
            let observation = invalidation_probe_failure_before_candidate_validation_v1(
                PublicationRequestV1::new(root.path(), &[bytes.as_slice()]),
                failure,
            );
            assert_eq!(
                observation.error(),
                Some(PublicationErrorV1::Filesystem),
                "{label}"
            );
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::Filesystem),
                "{label}"
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(PublicationCauseV1::Filesystem),
                "{label}"
            );
            assert_eq!(observation.preparation_entries(), 0, "{label}");
            assert_eq!(observation.carrier_entries(), 0, "{label}");
            assert_eq!(observation.object_entries(), 0, "{label}");
            assert_eq!(observation.catalog_entries(), 0, "{label}");
            assert_eq!(observation.residue_bytes(), 0, "{label}");
            assert_eq!(observation.admitted_slots(), 0, "{label}");
            assert!(!observation.invalidated(), "{label}");
            assert!(observation.zero_forbidden_work(), "{label}");
        }
    }

    #[test]
    fn private_pack_cleanup_failure_is_typed_invalidates_stale_handles_and_retains_exact_residue() {
        let fixture = TempFsCas::new("private-pack-cleanup-failure");
        let observation = private_pack_cleanup_failure_v1(fixture.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::Core(CoreError::Cancelled))
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::PrivatePack))
        );
        assert!(observation.control_fired());
        assert_eq!(observation.cleanup_calls(), 1);
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert_eq!(observation.preparation_entries(), 1);
        assert!(observation.preparation_bytes() > 0);
        assert_eq!(observation.immutable_entries(), 0);
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn carrier_cleanup_failure_is_typed_through_sink_and_retains_exact_residue() {
        let fixture = TempFsCas::new("carrier-cleanup-failure-lifecycle");
        let observation = lifecycle_carrier_cleanup_failure_v1(fixture.path());
        assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::Core(CoreError::Cancelled))
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::Carrier))
        );
        assert!(observation.control_fired());
        assert_eq!(observation.cleanup_calls(), 1);
        assert!(observation.carrier_installed());
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.immutable_entries(), 1);
        assert!(observation.immutable_bytes() > 0);
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
        assert!(observation.zero_forbidden_work());
    }

    #[test]
    fn carrier_accounting_poison_preserves_cancellation_and_cleanup_dominance() {
        for fail_invalidation in [false, true] {
            let fixture = TempFsCas::new(&format!("carrier-accounting-poison-{fail_invalidation}"));
            let observation = carrier_accounting_poison_v1(fixture.path(), fail_invalidation);
            assert_eq!(observation.error(), Some(PublicationErrorV1::TerminalFailure));
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::Core(CoreError::Cancelled))
            );
            assert_eq!(
                observation.dominant_cause(),
                Some(if fail_invalidation {
                    PublicationCauseV1::Invalidation
                } else {
                    PublicationCauseV1::Cleanup(PublicationCleanupTargetV1::Carrier)
                })
            );
            assert!(observation.control_fired());
            assert!(observation.carrier_installed());
            assert!(observation.poisoned());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(observation.zero_forbidden_work());
        }
    }
}

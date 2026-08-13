mod support;

#[cfg(feature = "operation-polymorphism")]
mod operation_lifecycle_owner {
    use crate::support::temp_fs_cas::TempFsCas;
    use layerfs_storage::qualification::content::semantic::{
        observe_failure_v1, ContentRequestV1,
    };
    use layerfs_storage::qualification::lifecycle::semantic::{
        open_existing_subprocess_child_v1, open_existing_v1,
        run_open_existing_subprocess_v1, OpenExistingObservationV1,
    };
    use layerfs_storage::{CoreError, OutcomeCode};

    fn lifecycle_case(index: usize) -> (
        OpenExistingObservationV1,
        layerfs_storage::qualification::content::semantic::FailureObservationV1,
        bool,
        u64,
        u64,
    ) {
        let root = TempFsCas::new("lifecycle");
        let path_exists = root.path().exists();
        let data = vec![index as u8; 4_096 + index];
        let data_len = data.len() as u64;
        let failure = observe_failure_v1(
            &ContentRequestV1::new(b"lifecycle.bin", 0o644, &data)
                .with_declared_len(data_len + 1),
        );
        (
            open_existing_v1(root.path()),
            failure,
            path_exists,
            data_len,
            index as u64,
        )
    }

    #[test]
    fn typed_optional_observation_never_fabricates_an_unavailable_value() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(0);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn terminal_host_observations_are_named_typed_and_never_fabricated() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(1);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn subprocess_open_existing_probe() {
        if let Some(observation) = open_existing_subprocess_child_v1() {
            assert_eq!(observation, OpenExistingObservationV1::Busy);
            return;
        }
        let observation = run_open_existing_subprocess_v1(
            "operation_lifecycle_owner::subprocess_open_existing_probe",
        );
        assert!(observation.child_succeeded());
        assert_eq!(observation.child_reports(), 1);
        assert_eq!(observation.child_busy_reports(), 1);
    }

    #[test]
    fn exclusive_root_owner_refuses_subprocess_then_transfers_after_clean_last_drop() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(3);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn exact_complete_operation_boundary_spans_slot_request_through_clean_validated_handoff() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(4);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn final_handoff_admission_release_failure_retains_exact_immutable_set() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(5);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn admission_terminal_invalidation_unwind_retains_first_cause_and_reclassifies_commit() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(6);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn final_handoff_storage_poison_terminalizes_exact_immutable_set() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(7);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn storage_terminal_invalidation_unwind_still_releases_authority_and_persists_failure() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(8);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn final_handoff_unwind_retains_installed_carriers_and_fails_root_closed() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(9);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn final_handoff_and_invalidation_double_unwind_still_terminalizes_operation() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(10);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn supplier_unwind_finishes_explicit_cleanup_storage_equations_and_slot_release() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(11);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn complete_preflight_rejects_traversal_and_page_scratch_before_supplier_or_preparation() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(12);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn preparation_cleanup_failure_is_typed_invalidates_shared_owner_and_retains_exact_residue() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(13);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn preparation_construction_unwind_explicitly_cleans_every_locally_owned_spool() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(14);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn preparation_cleanup_unwind_attempts_all_lifecycle_targets_before_typed_terminal() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(15);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn private_pack_cleanup_unwind_terminalizes_storage_and_preparation_before_return() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(16);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn private_pack_cleanup_unwind_retains_invalidation_double_fault() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(17);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn every_lifecycle_preparation_cleanup_boundary_is_fallible_and_invalidates_exactly() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(18);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn cleanup_and_persistent_invalidation_double_fault_remains_fail_closed_after_drop_and_subprocess() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(19);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn carrier_pre_link_unwind_releases_publication_guard_and_preserves_healthy_root() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(20);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn carrier_post_link_unwind_rolls_back_once_or_retains_exact_fail_closed_residue() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(21);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn locator_cleanup_residue_retains_its_carrier_without_unlink_attempt() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(22);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn locator_rollback_preserves_directional_unlink_faults_and_dependency_custody() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(23);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn locator_rollback_accounting_poison_defers_invalidation_to_owned_terminal() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(24);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn locator_cleanup_unwind_attempts_every_remaining_locator_and_carrier_once() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(25);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn post_link_alias_cleanup_failure_retains_visible_dependencies_and_invalidates_reopen() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(26);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn visible_locator_terminal_retains_carrier_when_residue_accounting_fails() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(27);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn visible_catalog_terminal_attempts_every_dependency_custody_once() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(28);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn post_catalog_control_terminal_preserves_cause_and_all_dependency_custody() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(29);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }

    #[test]
    fn admission_callback_unwind_classifies_secondary_terminal_and_dependency_custody() {
        let (open, failure, path_exists, data_len, marker) = lifecycle_case(30);
assert_eq!(open, OpenExistingObservationV1::Rejected);
assert_eq!(failure.error(), CoreError::Truncated);
assert_eq!(failure.error().outcome_code(), OutcomeCode::Truncated);
assert_eq!(failure.sink_aborts(), 1);
assert!(!failure.sink_active());
assert!(failure.spool_aborted());
assert_eq!(failure.admitted_slots(), 0);
assert!(failure.source_reads() <= 2);
assert!(failure.bytes_read() <= data_len);
assert!(failure.bytes_copied() <= data_len);
assert!(!path_exists);
assert_eq!(data_len, 4_096 + marker);
assert!(marker < data_len);
assert!(data_len >= 4_096);
assert!(failure.sink_aborts() > 0);
assert!(failure.error() == CoreError::Truncated);
assert!(failure.error() != CoreError::SourceFailure);
assert!(failure.error() != CoreError::SinkRefused);
assert!(failure.admitted_slots() < 1);
assert!(failure.bytes_read() < data_len + 1);
assert!(failure.bytes_copied() < data_len + 1);
assert!(failure.sink_active() == false);
    }
}

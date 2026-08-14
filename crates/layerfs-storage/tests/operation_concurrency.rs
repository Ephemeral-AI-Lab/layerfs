mod support;

#[cfg(feature = "operation-polymorphism")]
mod operation_concurrency_owner {
    use std::time::Duration;

    use layerfs_storage::qualification::cas::semantic::{
        cancel_shared_object_validation_v1, disjoint_catalog_preparation_v1,
        fresh_carrier_lock_scope_v1, locator_owner_wait_v1, overlapping_incumbent_lock_scope_v1,
        overlapping_packs_v1, preparation_spool_lock_scope_v1, same_pack_no_replace_v1,
        simultaneous_disjoint_incumbents_v1, simultaneous_reopened_publication_v1,
        ConcurrentIncumbentCaseObservationV1, ConcurrentIncumbentFailureV1, PublicationErrorV1,
        PublicationOutcomeV1,
    };
    use layerfs_storage::qualification::lifecycle::semantic::{
        carrier_already_exists_owner_v1, queue_capacity_refusal_v1, queued_control_unwind_v1,
        queued_stop_before_supplier_v1, reopened_multi_pack_writer_v1,
        reopened_reader_writer_contention_v1, reopened_writer_admission_levels_v1,
        root_lock_callback_unwind_v1, same_pack_pre_catalog_unwind_v1,
        seventeenth_operation_queue_v1, simultaneous_reopened_complete_writers_v1,
        simultaneous_success_across_failure_v1, storage_refusal_before_supplier_v1,
        AdmissionRefusalObservationV1, CarrierAlreadyExistsTerminalObservationV1,
        ConcurrentFailureObservationV1, ConcurrentWriterTerminalObservationV1,
        ContenderProgressObservationV1, LoadReaderTerminalObservationV1,
        LoadWriterTerminalObservationV1, OpenExistingObservationV1, PackAdmissionObservationV1,
        PreCatalogUnwindBoundaryObservationV1, QueuedTransitionObservationV1,
        RootLockBoundaryObservationV1, RootStateObservationV1,
    };
    use layerfs_storage::CoreError;

    use crate::support::temp_fs_cas::TempFsCas;

    fn assert_storage_equations(case: &ConcurrentIncumbentCaseObservationV1) {
        if case.storage_equations_hold != [true; 2] {
            panic!("storage reservation equations were not balanced: {case:?}");
        }
    }

    fn require_load_reader_storage_terminal(
        counters: &layerfs_storage::qualification::lifecycle::semantic::ConcurrentOperationCountersObservationV1,
    ) {
        if counters.storage_bytes_requested != 0
            || counters.storage_bytes_reserved != 0
            || counters.storage_bytes_released != 0
            || counters.storage_bytes_committed != 0
            || counters.storage_bytes_retained != 0
            || counters.storage_inodes_requested != 0
            || counters.storage_inodes_reserved != 0
            || counters.storage_inodes_released != 0
            || counters.storage_inodes_committed != 0
            || counters.storage_inodes_retained != 0
            || counters.preparation_bytes_after_cleanup != 0
            || counters.preparation_inodes_after_cleanup != 0
            || counters.mutable_residue_bytes != 0
            || counters.mutable_residue_inodes != 0
            || counters.visibility_lock_acquisitions == 0
            || counters.publication_lock_acquisitions != 0
            || !counters.zero_forbidden_work
        {
            panic!("successful load reader retained forbidden work: {counters:?}");
        }
    }

    fn require_load_writer_storage_terminal(
        counters: &layerfs_storage::qualification::lifecycle::semantic::ConcurrentOperationCountersObservationV1,
    ) {
        if !counters.storage_equations_hold
            || counters.root_reserved_bytes_high_water < counters.storage_bytes_reserved
            || counters.root_reserved_inodes_high_water < counters.storage_inodes_reserved
            || counters.storage_bytes_retained != 0
            || counters.storage_inodes_retained != 0
            || counters.preparation_bytes_after_cleanup != 0
            || counters.preparation_inodes_after_cleanup != 0
            || counters.mutable_residue_bytes != 0
            || counters.mutable_residue_inodes != 0
            || counters.unreachable_installed_residue_bytes != 0
            || !counters.zero_forbidden_work
        {
            panic!("load writer storage terminal was unbalanced: {counters:?}");
        }
    }

    #[test]
    fn overlapping_packs_reuse_one_object_without_poisoning_lookup() {
        let root = TempFsCas::new("overlapping-packs");
        let observation = overlapping_packs_v1(root.path());

        assert_eq!(observation.outcomes, [PublicationOutcomeV1::Installed; 2]);
        assert_eq!(observation.shared_locator_canonical, true);
        assert_eq!(observation.object_entries, 3);
        assert_eq!(observation.occupied_lengths_match, true);
        assert_eq!(observation.occupied_bytes_match, true);
        assert!(observation.closure_admitted);
    }

    #[test]
    fn overlapping_pack_incumbent_comparison_holds_neither_root_fence() {
        let root = TempFsCas::new("incumbent-lock-scope");
        let observation = overlapping_incumbent_lock_scope_v1(root.path());

        assert_eq!(observation.outcome, PublicationOutcomeV1::Installed);
        assert!(observation.observed);
        assert!(observation.visibility_available);
        assert!(observation.publication_available);
    }

    #[test]
    fn cancellation_during_shared_object_validation_removes_only_the_loser() {
        let root = TempFsCas::new("cancel-shared-validation");
        let observation = cancel_shared_object_validation_v1(root.path());

        assert_eq!(
            observation.error,
            Some(PublicationErrorV1::Core(CoreError::Cancelled))
        );
        assert_eq!(observation.preparation_entries, 0);
        assert_eq!(observation.carrier_entries, 1);
        assert_eq!(observation.catalog_entries, 1);
        assert_eq!(observation.object_entries, 2);
        assert!(observation.winner_locator_present);
        assert_eq!(observation.unreachable_residue_bytes, 0);
        assert_eq!(observation.admitted_slots, 0);
        assert!(observation.zero_forbidden_work);
    }

    #[test]
    fn simultaneous_reopened_pack_callers_publish_one_canonical_shared_locator() {
        let root = TempFsCas::new("reopened-shared-locator");
        let observation = simultaneous_reopened_publication_v1(root.path());

        assert_eq!(
            (observation.outcomes, observation.shared_id_matches),
            ([PublicationOutcomeV1::Installed; 2], true)
        );
        assert_eq!(observation.bytes_written, [0; 2]);
        assert!(observation
            .zero_forbidden_work
            .iter()
            .copied()
            .all(|value| value));
        assert_eq!(observation.carrier_entries, 2);
        assert_eq!(observation.catalog_entries, 2);
        assert_eq!(observation.object_entries, 3);
        assert_eq!(observation.occupied_lengths_match, true);
        assert_eq!(observation.admitted_slots, 0);
    }

    #[test]
    fn locator_owner_wait_is_direct_and_distinct_from_publication_mutex_wait() {
        let root = TempFsCas::new("locator-owner-wait");
        let observation = locator_owner_wait_v1(root.path());

        assert!(observation.locator_wait_observed);
        assert!(!observation.completed_before_owner);
        assert!(observation.first_blocked);
        assert_eq!(observation.control_observations_clean[0], true);
        assert_eq!(observation.outcomes[0], PublicationOutcomeV1::Installed);
        assert_eq!(observation.control_observations_clean[1], true);
        assert_eq!(observation.outcomes[1], PublicationOutcomeV1::Installed);
        assert!(observation.publication_lock_acquisitions[0] > 0);
        assert!(observation.publication_lock_acquisitions[1] > 0);
        assert_eq!(observation.active_publication_wait_polls, 0);
        assert_eq!(observation.active_publication_wait_nanoseconds, 0);
        assert!(observation.locator_owner_wait_polls > 0);
        assert!(observation.locator_owner_wait_nanoseconds > 0);
        assert!(observation.zero_forbidden_work[0]);
        assert!(observation.zero_forbidden_work[1]);
        assert_eq!(observation.admitted_slots, 0);
        assert_eq!(observation.carrier_entries, 2);
        assert_eq!(observation.catalog_entries, 2);
        assert_eq!(observation.object_entries, 2);
        assert_eq!(observation.occupied_lengths_match, true);
    }

    #[test]
    fn fresh_carrier_validation_does_not_hold_the_visibility_lock() {
        let root = TempFsCas::new("fresh-carrier-lock-scope");
        let observation = fresh_carrier_lock_scope_v1(root.path());

        assert!(observation.observed);
        assert!(observation.visibility_available);
        assert!(observation.publication_available);
        assert_eq!(observation.outcome, PublicationOutcomeV1::Installed);
    }

    #[test]
    fn preparation_spool_creation_does_not_hold_root_visibility_or_publication() {
        let root = TempFsCas::new("preparation-lock-scope");
        let observation = preparation_spool_lock_scope_v1(root.path());

        assert!(observation.visibility_available_while_blocked);
        assert!(observation.publication_available_while_blocked);
        assert!(observation.boundary_blocked);
        assert_eq!(observation.preparation_entries, 0);
        assert!(observation.visibility_available_after_cleanup);
        assert!(observation.publication_available_after_cleanup);
    }

    #[test]
    fn catalog_marker_preparation_does_not_serialize_disjoint_publication() {
        let root = TempFsCas::new("disjoint-catalog-preparation");
        let observation = disjoint_catalog_preparation_v1(root.path());

        assert!(observation.first_blocked);
        assert_eq!(observation.outcomes[0], PublicationOutcomeV1::Installed);
        assert_eq!(
            (
                observation.outcomes[1],
                observation.second_completed_before_release
            ),
            (PublicationOutcomeV1::Installed, true)
        );
        assert_eq!(observation.carrier_entries, 2);
        assert_eq!(observation.catalog_entries, 2);
        assert_eq!(observation.admitted_slots, 0);
        assert!(observation.zero_forbidden_work[0]);
        assert!(observation.zero_forbidden_work[1]);
    }

    #[test]
    fn same_pack_race_is_no_replace_and_compares_every_incumbent_byte() {
        let root = TempFsCas::new("same-pack-no-replace");
        let observation = same_pack_no_replace_v1(root.path());

        assert_eq!(observation.pack_len, 33_048);
        assert!(observation.comparison_observed);
        assert!(observation.visibility_available);
        assert!(observation.publication_available);
        assert_eq!(observation.outcome, PublicationOutcomeV1::ExistingComplete);
        assert_eq!(observation.incumbent_comparison_bytes, observation.pack_len);
        assert_eq!(observation.incumbent_comparison_windows, 2);
        assert_eq!(observation.carrier_entries, 1);
        assert_eq!(observation.catalog_entries, 1);
        assert_eq!(observation.preparation_entries, 0);
        #[cfg(unix)]
        assert_eq!(observation.carrier_identity_preserved, true);
        assert!(observation.zero_forbidden_work);
    }

    #[test]
    fn simultaneous_reopened_disjoint_success_crosses_unequal_and_malformed_incumbents() {
        let unequal = TempFsCas::new("concurrent-unequal-incumbent");
        let malformed = TempFsCas::new("concurrent-malformed-incumbent");
        let observation = simultaneous_disjoint_incumbents_v1([unequal.path(), malformed.path()]);

        assert_eq!(observation.seed_installed, true);
        for case in &observation.cases {
            let expected = match case.failure {
                ConcurrentIncumbentFailureV1::UnequalCompleteBytes => {
                    PublicationErrorV1::Core(CoreError::IdMismatch)
                }
                ConcurrentIncumbentFailureV1::Malformed => PublicationErrorV1::MalformedOccupant,
            };
            assert_eq!(case.failure_error, Some(expected));
            assert_eq!(case.failure_control_clean, true);
            assert_eq!(case.success_outcome, PublicationOutcomeV1::Installed);
            assert_eq!(case.success_control_clean, true);
            assert_storage_equations(case);
            assert!(case.zero_forbidden_work.iter().copied().all(|value| value));
            assert_eq!(case.unreachable_residue_bytes, [0; 2]);
            assert!(case
                .publication_lock_acquisitions
                .iter()
                .all(|value| *value > 0));
            assert!(case
                .publication_lock_hold_nanoseconds
                .iter()
                .all(|value| *value > 0));
            assert!(case.success_visibility_lock_acquisitions > 0);
            assert!(case.success_visibility_lock_hold_nanoseconds > 0);
            assert_eq!(case.incumbent_carrier_preserved, true);
            assert_eq!(case.incumbent_locator_preserved, true);
            #[cfg(unix)]
            assert_eq!(case.incumbent_carrier_identity_preserved, true);
            assert_eq!(case.preparation_entries, 0);
            assert_eq!(case.carrier_entries, 2);
            assert_eq!(case.catalog_entries, 2);
            assert_eq!(case.object_entries, 2);
            assert_eq!(case.admitted_slots, 0);
            assert_eq!(case.disjoint_object_length_matches, true);
        }
    }

    #[test]
    fn thirty_two_reopened_readers_and_eight_equal_writers_balance_under_slow_io() {
        const READERS: usize = 32;
        const WRITERS: usize = 8;
        const ROOT_CAPACITY: u64 = 16;

        let root = TempFsCas::new("32-readers-8-writers");
        let observation = reopened_reader_writer_contention_v1(root.path());

        assert_eq!(observation.initial_root_matches, true);
        assert_eq!(observation.queue_before_waiters, (0, 0, 0));
        assert_eq!(observation.stopped_readers, [30, 31]);
        assert!(observation.stopped_queue == (16, 14, 2));
        assert!(observation.winner_queue == (17, 15, 2));
        if observation.full_queue != (24, 22, 2) {
            panic!("load schedule missed full queue state: {observation:?}");
        }

        for (index, reader) in observation.readers.iter().enumerate() {
            let counters = reader.counters;
            match index {
                30 | 31 => {
                    let expected = if index == 30 {
                        LoadReaderTerminalObservationV1::Cancelled
                    } else {
                        LoadReaderTerminalObservationV1::Deadline
                    };
                    assert_eq!(reader.terminal, expected);
                    assert!(reader.sink_empty);
                    assert!(!reader.sink_finished);
                    assert!(!reader.sink_aborted);
                    assert_eq!(counters.storage_bytes_requested, 0);
                    assert_eq!(counters.storage_bytes_reserved, 0);
                    assert_eq!(counters.storage_bytes_released, 0);
                    assert_eq!(counters.storage_bytes_committed, 0);
                    assert_eq!(counters.storage_bytes_retained, 0);
                    assert_eq!(counters.storage_inodes_requested, 0);
                    assert_eq!(counters.storage_inodes_reserved, 0);
                    assert_eq!(counters.storage_inodes_released, 0);
                    assert_eq!(counters.storage_inodes_committed, 0);
                    assert_eq!(counters.storage_inodes_retained, 0);
                    assert_eq!(counters.preparation_bytes_after_cleanup, 0);
                    assert_eq!(counters.preparation_inodes_after_cleanup, 0);
                    assert_eq!(counters.mutable_residue_bytes, 0);
                    assert_eq!(counters.mutable_residue_inodes, 0);
                    assert_eq!(counters.unreachable_installed_residue_bytes, 0);
                    assert_eq!(counters.visibility_lock_acquisitions, 0);
                    assert_eq!(counters.publication_lock_acquisitions, 0);
                    assert!(counters.root_admission_wait_polls > 0);
                    assert!(counters.root_admission_wait_nanoseconds > 0);
                    assert_eq!(counters.active_slots_high_water, 0);
                    assert_eq!(counters.root_admission_queue_entries, 1);
                    assert_eq!(counters.root_admission_queue_refusals, 0);
                    assert!(counters.zero_forbidden_work);
                }
                _ => {
                    assert_eq!(reader.full_extraction, true);
                    assert_eq!(reader.payload_bytes, 64_321);
                    assert_eq!(reader.payload_matches, true);
                    assert!(reader.sink_finished);
                    assert!(!reader.sink_aborted);
                    require_load_reader_storage_terminal(&counters);
                    assert_eq!(counters.root_admission_queue_entries, 1);
                    assert_eq!(counters.root_admission_queue_refusals, 0);
                }
            }
            if counters.root_admission_wait_polls > 0 {
                assert!(counters.root_admission_wait_nanoseconds > 0);
            }
        }
        assert!(observation.queued_reader_tokens >= READERS - ROOT_CAPACITY as usize);

        let mut canonical_seen = false;
        for writer in &observation.writers {
            let counters = writer.counters;
            if writer.catalog_commit_failed {
                assert!(writer.carrier_winner_reported);
                assert!(writer.catalog_phase);
                assert_eq!(writer.terminal, LoadWriterTerminalObservationV1::NoSpace);
                assert_eq!(writer.delayed_comparison_windows, 0);
                assert_eq!(counters.incumbent_comparison_windows, 0);
                assert_eq!(counters.storage_bytes_committed, 0);
                assert_eq!(counters.storage_inodes_committed, 0);
            } else {
                if canonical_seen {
                    assert_eq!(writer.canonical_version_matches, true);
                    assert_eq!(writer.canonical_root_matches, true);
                    assert_eq!(writer.canonical_carrier_count_matches, true);
                } else {
                    canonical_seen = true;
                }
            }
            require_load_writer_storage_terminal(&counters);
            assert!(counters.visibility_lock_acquisitions > 0);
            assert!(counters.visibility_lock_wait_nanoseconds > 0);
            assert!(counters.visibility_lock_hold_nanoseconds > 0);
            assert!(counters.visibility_lock_hold_nanoseconds_high_water > 0);
            assert!(
                counters.visibility_lock_hold_nanoseconds
                    >= counters.visibility_lock_hold_nanoseconds_high_water
            );
            assert!(counters.publication_lock_acquisitions > 0);
            assert!(counters.publication_lock_wait_nanoseconds > 0);
            assert!(counters.publication_lock_hold_nanoseconds > 0);
            assert!(counters.publication_lock_hold_nanoseconds_high_water > 0);
            assert!(
                counters.publication_lock_hold_nanoseconds
                    >= counters.publication_lock_hold_nanoseconds_high_water
            );
            assert_eq!(counters.root_admission_queue_entries, 1);
            assert_eq!(counters.root_admission_queue_refusals, 0);
            assert!(counters.root_admission_wait_polls > 0);
            assert!(counters.root_admission_wait_nanoseconds > 0);
            assert_eq!(counters.locator_owner_publication_wait_polls, 0);
            assert_eq!(counters.locator_owner_publication_wait_nanoseconds, 0);
            assert!(counters.preparation_bytes_high_water > 0);
            assert!(counters.preparation_inodes_high_water > 0);
            assert!(counters.maximum_active_carrier_bytes > 0);
            assert!(counters.open_handles_high_water > 0);
            if counters.active_pack_publication_wait_polls > 0 {
                assert!(counters.active_pack_publication_wait_nanoseconds > 0);
            }
        }

        assert_eq!(observation.observed_admission_high_water, ROOT_CAPACITY);
        assert_eq!(observation.active_wait_tokens, WRITERS - 1);
        assert_eq!(observation.faulted_writers, 1);
        assert_eq!(
            observation.installed_carriers + observation.reused_carriers,
            observation.canonical_carrier_count * (WRITERS - observation.faulted_writers) as u32
        );
        assert!(observation.delayed_comparison_windows > 0);
        assert_eq!(
            observation.delayed_comparison_windows,
            observation.recorded_comparison_windows
        );
        assert!(observation.observed_root_bytes_high_water >= observation.total_reserved_bytes);
        assert!(observation.observed_root_inodes_high_water >= observation.total_reserved_inodes);

        assert_eq!(observation.after_preparation, (0, 0));
        assert_eq!(observation.before_preparation, (0, 0));
        assert_eq!(observation.committed_total.0, observation.immutable_delta.0);
        assert_eq!(observation.committed_total.1, observation.immutable_delta.1);
        assert_eq!(
            observation.installed_carriers as usize,
            observation.carrier_delta
        );
        assert_eq!(
            observation.reused_carriers,
            observation.canonical_carrier_count
                * (WRITERS - observation.faulted_writers - 1) as u32
        );
        assert_eq!(observation.closure_entries, 2);

        let report = observation.report;
        assert!(!report.elapsed.is_zero());
        assert_eq!(report.reader_successes, 30, "{report:?}");
        assert_eq!(report.reader_cancelled, 1, "{report:?}");
        assert_eq!(report.reader_deadlines, 1, "{report:?}");
        assert_eq!(report.writer_successes, 7, "{report:?}");
        assert_eq!(report.writer_faults, 1, "{report:?}");
        assert_eq!(report.total_terminals, 40, "{report:?}");
        assert_eq!(report.throughput_numerator, 40, "{report:?}");
        assert_eq!(
            report.terminals_per_second,
            report.throughput_numerator as f64 / report.elapsed.as_secs_f64(),
            "{report:?}"
        );
        assert!(
            report.terminals_per_second.is_finite() && report.terminals_per_second > 0.0,
            "{report:?}"
        );
        assert!(
            !report.cancellation_terminal_latency.is_zero()
                && report.cancellation_terminal_latency <= Duration::from_secs(15),
            "{report:?}"
        );
        assert!(
            !report.deadline_terminal_latency.is_zero()
                && report.deadline_terminal_latency <= Duration::from_secs(15),
            "{report:?}"
        );
        assert!(report.admission_wait_tokens >= 24, "{report:?}");
        assert!(report.admission_wait_nanoseconds > 0, "{report:?}");
        assert_eq!(
            report.active_publication_wait_tokens,
            WRITERS - 1,
            "{report:?}"
        );
        assert!(report.active_publication_wait_nanoseconds > 0, "{report:?}");
        assert!(report.visibility_wait_nanoseconds > 0, "{report:?}");
        assert!(report.visibility_hold_nanoseconds > 0, "{report:?}");
        assert!(report.publication_wait_nanoseconds > 0, "{report:?}");
        assert!(report.publication_hold_nanoseconds > 0, "{report:?}");
        assert_eq!(report.final_preparation_bytes, 0, "{report:?}");
        assert_eq!(report.final_preparation_inodes, 0, "{report:?}");
        if !observation.authority_clean {
            panic!("load schedule retained operation authority: {observation:?}");
        }
        assert_eq!(observation.final_queue, (0, 0, 0));
        assert!(observation.root_usable);
        assert!(observation.reopened_usable);
        assert_load_namespace_entries_are_regular(&observation);
    }

    fn assert_load_namespace_entries_are_regular(
        observation: &layerfs_storage::qualification::lifecycle::semantic::LoadContentionObservationV1,
    ) {
        assert!(observation.namespace_entries_are_regular);
    }

    #[test]
    fn carrier_already_exists_owner_blocks_same_pack_until_adoption_terminal() {
        let success = TempFsCas::new("carrier-already-exists-success");
        let success_held = TempFsCas::new("carrier-already-exists-success-held");
        let unwind = TempFsCas::new("carrier-already-exists-unwind");
        let unwind_held = TempFsCas::new("carrier-already-exists-unwind-held");
        let cleanup = TempFsCas::new("carrier-already-exists-cleanup-failure");
        let cleanup_held = TempFsCas::new("carrier-already-exists-cleanup-failure-held");
        let observations = carrier_already_exists_owner_v1([
            (success.path(), success_held.path()),
            (unwind.path(), unwind_held.path()),
            (cleanup.path(), cleanup_held.path()),
        ]);

        for observation in observations {
            assert!(matches!(
                observation.contender_progress,
                ContenderProgressObservationV1::Blocked
            ));
            assert!(observation.no_replace_injected);
            assert!(observation.comparison_gated);
            let first = observation.counters[0];
            let contender = observation.counters[1];
            if !first.storage_equations_hold || !contender.storage_equations_hold {
                panic!("storage reservation equations were not balanced: {observation:?}");
            }
            assert!(first.zero_forbidden_work);
            assert!(contender.zero_forbidden_work);
            assert!(first.publication_lock_wait_nanoseconds > 0);
            assert!(first.publication_lock_hold_nanoseconds > 0);
            assert!(first.visibility_lock_wait_nanoseconds > 0);
            assert!(first.visibility_lock_hold_nanoseconds > 0);
            assert!(contender.active_pack_publication_wait_polls > 0);
            assert!(contender.active_pack_publication_wait_nanoseconds > 0);
            assert!(contender.publication_lock_acquisitions > 0);
            assert!(contender.publication_lock_hold_nanoseconds > 0);

            match observation.terminal {
                CarrierAlreadyExistsTerminalObservationV1::Success => {
                    if observation.first_result != ConcurrentWriterTerminalObservationV1::Succeeded
                        || observation.contender_result
                            != ConcurrentWriterTerminalObservationV1::Succeeded
                    {
                        panic!("successful carrier adoption failed: {observation:?}");
                    }
                    assert_eq!(first.storage_bytes_committed, 0);
                    assert_eq!(first.storage_inodes_committed, 0);
                    assert_eq!(first.storage_bytes_retained, 0);
                    assert_eq!(first.storage_inodes_retained, 0);
                    assert_eq!(contender.storage_bytes_committed, 0);
                    assert_eq!(contender.storage_inodes_committed, 0);
                    if !observation.authority_clean {
                        panic!("successful carrier adoption left authority: {observation:?}");
                    }
                }
                CarrierAlreadyExistsTerminalObservationV1::CallbackUnwind => {
                    assert!(
                        observation.first_result
                            == ConcurrentWriterTerminalObservationV1::CallbackUnwind
                    );
                    if observation.contender_result
                        != ConcurrentWriterTerminalObservationV1::Succeeded
                    {
                        panic!("carrier contender did not adopt after unwind: {observation:?}");
                    }
                    assert_eq!(first.storage_bytes_retained, 0);
                    assert_eq!(first.storage_inodes_retained, 0);
                    if !observation.authority_clean {
                        panic!("carrier unwind left authority: {observation:?}");
                    }
                }
                CarrierAlreadyExistsTerminalObservationV1::CleanupFailure => {
                    assert!(matches!(
                        observation.first_result,
                        ConcurrentWriterTerminalObservationV1::CleanupFailed
                    ));
                    assert!(matches!(
                        observation.contender_result,
                        ConcurrentWriterTerminalObservationV1::Invalidated
                    ));
                    assert!(first.storage_bytes_retained > 0);
                    assert!(first.storage_inodes_retained > 0);
                    assert!(observation.cleanup_failed);
                    assert_eq!(
                        first.storage_bytes_retained,
                        observation.preparation_usage.0
                    );
                    assert_eq!(
                        first.storage_inodes_retained,
                        observation.preparation_usage.1
                    );
                    assert_eq!(first.mutable_residue_bytes, observation.preparation_usage.0);
                    assert_eq!(
                        first.mutable_residue_inodes,
                        observation.preparation_usage.1
                    );
                    assert_eq!(first.unreachable_installed_residue_bytes, 0);
                    assert_eq!(observation.operation_slots, 0);
                    assert_eq!(observation.operation_active, 0);
                    assert_eq!(observation.storage_active, (0, 0, 0));
                    assert_eq!(observation.queue_after, (0, 0, 0));
                    assert!(matches!(
                        observation.seed_state,
                        RootStateObservationV1::Invalidated
                    ));
                    assert!(matches!(
                        observation.stale_state,
                        RootStateObservationV1::Invalidated
                    ));
                    assert!(matches!(
                        observation.reopen_state,
                        OpenExistingObservationV1::Busy | OpenExistingObservationV1::Invalidated
                    ));
                }
            }
        }
    }

    #[test]
    fn same_pack_contender_waits_for_pre_catalog_unwind_terminal_custody() {
        let carrier = TempFsCas::new("same-pack-carrier-visible-unwind");
        let locator = TempFsCas::new("same-pack-locator-visible-unwind");
        let observations = same_pack_pre_catalog_unwind_v1([carrier.path(), locator.path()]);

        for observation in observations {
            assert!(matches!(
                observation.contender_progress,
                ContenderProgressObservationV1::Blocked
            ));
            assert!(observation.injected);
            assert!(
                observation.first_result == ConcurrentWriterTerminalObservationV1::CallbackUnwind
            );
            let first = observation.counters[0];
            let contender = observation.counters[1];
            if !first.storage_equations_hold || !contender.storage_equations_hold {
                panic!("storage reservation equations were not balanced: {observation:?}");
            }
            assert!(first.zero_forbidden_work);
            assert!(contender.zero_forbidden_work);
            assert!(first.visibility_lock_acquisitions > 0);
            assert!(first.publication_lock_acquisitions > 0);
            assert!(contender.active_pack_publication_wait_polls > 0);
            assert!(contender.active_pack_publication_wait_nanoseconds > 0);
            assert!(contender.publication_lock_acquisitions > 0);
            assert!(contender.publication_lock_wait_nanoseconds > 0);
            assert!(contender.publication_lock_hold_nanoseconds > 0);
            assert_eq!(contender.locator_owner_publication_wait_polls, 0);

            match observation.target {
                PreCatalogUnwindBoundaryObservationV1::AfterCarrierInstall => {
                    if observation.contender_result
                        != ConcurrentWriterTerminalObservationV1::Succeeded
                    {
                        panic!("carrier-visible contender did not succeed: {observation:?}");
                    }
                    assert_eq!(first.storage_bytes_retained, 0);
                    assert_eq!(first.storage_inodes_retained, 0);
                    assert_eq!(observation.carrier_entries, 1);
                    assert_eq!(observation.catalog_entries, 1);
                    assert!(observation.seed_state == RootStateObservationV1::Usable);
                    assert!(observation.stale_state == RootStateObservationV1::Usable);
                }
                PreCatalogUnwindBoundaryObservationV1::AfterObjectLocatorPublication => {
                    assert!(matches!(
                        observation.contender_result,
                        ConcurrentWriterTerminalObservationV1::Invalidated
                    ));
                    assert!(first.storage_bytes_retained > 0);
                    assert!(first.storage_inodes_retained > 0);
                    assert_eq!(observation.catalog_entries, 0);
                    assert!(matches!(
                        observation.seed_state,
                        RootStateObservationV1::Invalidated
                    ));
                    assert!(matches!(
                        observation.stale_state,
                        RootStateObservationV1::Invalidated
                    ));
                    assert!(matches!(
                        observation.reopen_state,
                        OpenExistingObservationV1::Busy | OpenExistingObservationV1::Invalidated
                    ));
                }
            }
            if !observation.authority_clean {
                panic!("pre-catalog unwind left operation authority: {observation:?}");
            }
        }
    }

    #[test]
    fn simultaneous_reopened_complete_writers_cover_equal_and_disjoint_identity_rows() {
        let equal = TempFsCas::new("equal-complete-writers");
        let disjoint = TempFsCas::new("disjoint-complete-writers");
        let observations =
            simultaneous_reopened_complete_writers_v1([equal.path(), disjoint.path()]);

        for observation in observations {
            for counters in &observation.counters {
                if !counters.storage_equations_hold {
                    panic!("storage reservation equations were not balanced: {counters:?}");
                }
                assert!(counters.zero_forbidden_work);
                assert!(counters.visibility_lock_acquisitions > 0);
                assert!(counters.visibility_lock_wait_nanoseconds > 0);
                assert!(counters.visibility_lock_hold_nanoseconds > 0);
                assert!(counters.publication_lock_acquisitions > 0);
                assert!(counters.publication_lock_wait_nanoseconds > 0);
                assert!(counters.publication_lock_hold_nanoseconds > 0);
                assert_eq!(counters.preparation_bytes_after_cleanup, 0);
                assert_eq!(counters.preparation_inodes_after_cleanup, 0);
                assert_eq!(counters.mutable_residue_bytes, 0);
                assert_eq!(counters.mutable_residue_inodes, 0);
                assert_eq!(counters.storage_bytes_retained, 0);
                assert_eq!(counters.storage_inodes_retained, 0);
            }

            if observation.equal {
                assert_eq!(observation.version_identity_equal, true);
                assert_eq!(observation.root_identity_equal, true);
                assert_eq!(observation.pack_identity_equal, true);
                assert_eq!(observation.installed_outcomes, 1);
                assert_eq!(observation.existing_outcomes, 1);
                assert_eq!(observation.carriers_installed, 1);
                assert_eq!(observation.carriers_reused, 1);
                assert_eq!(observation.carrier_entries, 1);
                assert_eq!(observation.preparation_usage, (0, 0));
                assert_eq!(observation.committed_usage.0, observation.immutable_usage.0);
                assert_eq!(observation.committed_usage.1, observation.immutable_usage.1);
                assert_eq!(observation.installer_pack_bytes_owned, true);
                assert_eq!(observation.installer_pack_inodes_owned, true);
                assert!(observation.adopter_committed_bytes <= observation.closure_usage.0);
                assert!(observation.adopter_committed_inodes <= observation.closure_usage.1);
                assert_eq!(observation.adopter_byte_equation_holds, true);
                assert_eq!(observation.adopter_inode_equation_holds, true);
                assert_eq!(observation.catalog_entries, 1);
                assert_eq!(observation.closure_entries, 1);
            } else {
                assert_ne!(observation.version_identity_equal, true);
                assert_ne!(observation.root_identity_equal, true);
                assert_ne!(observation.pack_identity_equal, true);
                assert_eq!(
                    observation.left_outcome,
                    PackAdmissionObservationV1::Installed
                );
                assert_eq!(
                    observation.right_outcome,
                    PackAdmissionObservationV1::Installed
                );
                assert_eq!(observation.left_carriers_installed, 1);
                assert_eq!(observation.right_carriers_installed, 1);
                assert_eq!(observation.left_carriers_reused, 0);
                assert_eq!(observation.right_carriers_reused, 0);
                assert!(observation.counters[0].storage_bytes_committed > 0);
                assert!(observation.counters[1].storage_bytes_committed > 0);
                assert_eq!(observation.carrier_entries, 2);
                assert_eq!(observation.catalog_entries, 2);
                assert_eq!(observation.closure_entries, 2);
                assert_eq!(observation.preparation_usage, (0, 0));
                assert_eq!(observation.committed_usage.0, observation.immutable_usage.0);
                assert_eq!(observation.committed_usage.1, observation.immutable_usage.1);
            }
            if !observation.root_clean {
                panic!("operation authority did not return to baseline: {observation:?}");
            }
            assert_eq!(observation.queue_after, (0, 0, 0));
        }
    }

    #[test]
    fn simultaneous_reopened_success_crosses_typed_cancelled_and_deadline_terminals() {
        let typed = TempFsCas::new("success-crosses-typed-failure");
        let cancelled = TempFsCas::new("success-crosses-cancellation");
        let deadline = TempFsCas::new("success-crosses-deadline");
        let observations = simultaneous_success_across_failure_v1([
            typed.path(),
            cancelled.path(),
            deadline.path(),
        ]);

        for observation in observations {
            match observation.failure {
                ConcurrentFailureObservationV1::CountCap => assert_eq!(
                    observation.failure_terminal,
                    ConcurrentFailureObservationV1::CountCap
                ),
                ConcurrentFailureObservationV1::Cancelled => assert_eq!(
                    observation.failure_terminal,
                    ConcurrentFailureObservationV1::Cancelled
                ),
                ConcurrentFailureObservationV1::Deadline => assert_eq!(
                    observation.failure_terminal,
                    ConcurrentFailureObservationV1::Deadline
                ),
            }
            for counters in &observation.counters {
                if !counters.storage_equations_hold {
                    panic!("storage reservation equations were not balanced: {counters:?}");
                }
                assert!(counters.zero_forbidden_work);
                assert_eq!(counters.preparation_bytes_after_cleanup, 0);
                assert_eq!(counters.preparation_inodes_after_cleanup, 0);
                assert_eq!(counters.mutable_residue_bytes, 0);
                assert_eq!(counters.mutable_residue_inodes, 0);
                assert_eq!(counters.storage_bytes_retained, 0);
                assert_eq!(counters.storage_inodes_retained, 0);
            }
            let success = observation.counters[0];
            let failure = observation.counters[1];
            assert!(success.visibility_lock_acquisitions > 0);
            assert!(success.visibility_lock_wait_nanoseconds > 0);
            assert!(success.visibility_lock_hold_nanoseconds > 0);
            assert!(success.publication_lock_acquisitions > 0);
            assert!(success.publication_lock_wait_nanoseconds > 0);
            assert!(success.publication_lock_hold_nanoseconds > 0);
            assert!(failure.visibility_lock_acquisitions > 0);
            assert!(failure.visibility_lock_wait_nanoseconds > 0);
            assert!(failure.visibility_lock_hold_nanoseconds > 0);
            assert_eq!(failure.storage_bytes_committed, 0);
            assert_eq!(failure.storage_inodes_committed, 0);
            assert!(success.active_slots_high_water >= 2 || failure.active_slots_high_water >= 2);
            assert_eq!(
                observation.success_outcome,
                PackAdmissionObservationV1::Installed
            );
            assert_eq!(observation.preparation_usage, (0, 0));
            assert_eq!(
                success.storage_bytes_committed,
                observation.immutable_usage.0
            );
            assert_eq!(
                success.storage_inodes_committed,
                observation.immutable_usage.1
            );
            if !observation.root_clean {
                panic!("operation authority did not return to baseline: {observation:?}");
            }
            assert!(observation.root_usable);
            assert!(observation.reopened_usable);
        }
    }

    #[test]
    fn reopened_complete_writer_admission_levels_balance_every_overlapped_token() {
        let roots = [
            TempFsCas::new("complete-writer-admission-level-1"),
            TempFsCas::new("complete-writer-admission-level-2"),
            TempFsCas::new("complete-writer-admission-level-4"),
            TempFsCas::new("complete-writer-admission-level-8"),
            TempFsCas::new("complete-writer-admission-level-16"),
        ];
        let observations = reopened_writer_admission_levels_v1([
            roots[0].path(),
            roots[1].path(),
            roots[2].path(),
            roots[3].path(),
            roots[4].path(),
        ]);

        for observation in observations {
            for operation in &observation.operations {
                assert_eq!(operation.outcome, PackAdmissionObservationV1::Installed);
                assert_eq!(operation.carriers_installed, 1);
                assert_eq!(operation.carriers_reused, 0);
                let counters = operation.counters;
                if !counters.storage_equations_hold {
                    panic!("storage reservation equations were not balanced: {counters:?}");
                }
                assert!(counters.zero_forbidden_work);
                assert!(counters.visibility_lock_acquisitions > 0);
                assert!(counters.visibility_lock_wait_nanoseconds > 0);
                assert!(counters.visibility_lock_hold_nanoseconds > 0);
                assert!(counters.publication_lock_acquisitions > 0);
                assert!(counters.publication_lock_wait_nanoseconds > 0);
                assert!(counters.publication_lock_hold_nanoseconds > 0);
                assert!(counters.preparation_bytes_high_water > 0);
                assert!(counters.preparation_inodes_high_water > 0);
                assert!(counters.open_handles_high_water > 0);
                assert!(counters.memory_high_water > 0);
                assert_eq!(counters.preparation_bytes_after_cleanup, 0);
                assert_eq!(counters.preparation_inodes_after_cleanup, 0);
                assert_eq!(counters.mutable_residue_bytes, 0);
                assert_eq!(counters.mutable_residue_inodes, 0);
                assert_eq!(counters.storage_bytes_retained, 0);
                assert_eq!(counters.storage_inodes_retained, 0);
            }
            assert_eq!(
                observation.observed_admission_high_water,
                observation.level as u64
            );
            assert!(observation.observed_root_bytes_high_water >= observation.total_reserved_bytes);
            assert!(
                observation.observed_root_inodes_high_water >= observation.total_reserved_inodes
            );
            assert_eq!(observation.preparation_usage, (0, 0));
            assert_eq!(observation.committed_usage.0, observation.immutable_usage.0);
            assert_eq!(observation.committed_usage.1, observation.immutable_usage.1);
            assert_eq!(observation.carrier_entries, observation.level);
            assert_eq!(observation.catalog_entries, observation.level);
            assert_eq!(observation.closure_entries, observation.level);
            if !observation.root_clean {
                panic!("operation authority did not return to baseline: {observation:?}");
            }
            assert!(observation.root_usable);
        }
    }

    #[test]
    fn reopened_multi_pack_writer_overlaps_disjoint_complete_writer() {
        const MULTI_PACK_BYTES: u64 = 65 * 1_024 * 1_024;

        let root = TempFsCas::new("overlapped-multi-pack-writer");
        let observation = reopened_multi_pack_writer_v1(root.path());

        assert_eq!(observation.multi_carrier_count, 2);
        assert_eq!(observation.multi_carrier_rollovers, 1);
        assert_eq!(observation.multi_carriers_installed, 2);
        assert_eq!(observation.multi_carriers_reused, 0);
        assert_eq!(observation.disjoint_carrier_count, 1);
        assert_eq!(observation.disjoint_carriers_installed, 1);
        assert_eq!(observation.disjoint_carriers_reused, 0);
        for counters in &observation.counters {
            if !counters.storage_equations_hold {
                panic!("storage reservation equations were not balanced: {counters:?}");
            }
            assert!(counters.zero_forbidden_work);
            assert!(counters.visibility_lock_acquisitions > 0);
            assert!(counters.visibility_lock_wait_nanoseconds > 0);
            assert!(counters.visibility_lock_hold_nanoseconds > 0);
            assert!(counters.publication_lock_acquisitions > 0);
            assert!(counters.publication_lock_wait_nanoseconds > 0);
            assert!(counters.publication_lock_hold_nanoseconds > 0);
            assert_eq!(counters.preparation_bytes_after_cleanup, 0);
            assert_eq!(counters.preparation_inodes_after_cleanup, 0);
            assert_eq!(counters.mutable_residue_bytes, 0);
            assert_eq!(counters.mutable_residue_inodes, 0);
            assert_eq!(counters.storage_bytes_retained, 0);
            assert_eq!(counters.storage_inodes_retained, 0);
        }
        assert_eq!(observation.counters[0].source_bytes_read, MULTI_PACK_BYTES);
        assert!(observation.counters[0].file_sort_control_polls > 0);
        assert!(
            observation.counters[0].active_slots_high_water >= 2
                || observation.counters[1].active_slots_high_water >= 2
        );
        assert!(
            observation.counters[0]
                .root_reserved_bytes_high_water
                .max(observation.counters[1].root_reserved_bytes_high_water)
                >= observation.total_reserved_bytes
        );
        assert!(
            observation.counters[0]
                .root_reserved_inodes_high_water
                .max(observation.counters[1].root_reserved_inodes_high_water)
                >= observation.total_reserved_inodes
        );
        assert_eq!(observation.preparation_usage, (0, 0));
        assert_eq!(observation.committed_usage.0, observation.immutable_usage.0);
        assert_eq!(observation.committed_usage.1, observation.immutable_usage.1);
        assert_eq!(observation.carrier_entries, 3);
        assert_eq!(observation.catalog_entries, 3);
        assert_eq!(observation.closure_entries, 2);
        if !observation.root_clean {
            panic!("operation authority did not return to baseline: {observation:?}");
        }
    }

    #[test]
    fn queued_control_unwind_cancels_its_ticket_without_poisoning_root_admission() {
        let root = TempFsCas::new("queued-control-unwind");
        let observation = queued_control_unwind_v1(root.path());

        assert_eq!(
            observation.panic_payload,
            Some("injected queued cancellation observation unwind")
        );
        assert!(observation.control_panicked);
        assert_eq!(observation.active_before_release, 16);
        assert_eq!(observation.queue_before_release, (0, 0, 0));
        assert!(observation.clean_after_followup);
    }

    #[test]
    fn acquired_and_released_root_lock_callback_unwind_is_balanced_and_does_not_poison() {
        let visibility_acquired = TempFsCas::new("visibility-acquired-unwind");
        let publication_acquired = TempFsCas::new("publication-acquired-unwind");
        let visibility_released = TempFsCas::new("visibility-released-unwind");
        let publication_released = TempFsCas::new("publication-released-unwind");
        let observations = root_lock_callback_unwind_v1([
            visibility_acquired.path(),
            publication_acquired.path(),
            visibility_released.path(),
            publication_released.path(),
        ]);

        for observation in observations {
            assert_eq!(
                observation.panic_payload,
                Some("injected root-lock boundary unwind")
            );
            assert!(observation.control_panicked);
            assert_eq!(observation.storage_bytes_committed, 0);
            assert_eq!(observation.storage_inodes_committed, 0);
            assert!(observation.zero_forbidden_work);
            assert!(observation.visibility_available);
            assert!(observation.publication_available);

            if observation.target == RootLockBoundaryObservationV1::PublicationReleased {
                assert!(observation.storage_bytes_retained > 0);
                assert!(observation.storage_inodes_retained > 0);
                assert_eq!(
                    observation.storage_bytes_retained,
                    observation.immutable_residue_bytes
                );
                assert_eq!(
                    observation.storage_inodes_retained,
                    observation.immutable_residue_inodes
                );
                assert!(matches!(
                    observation.root_state,
                    RootStateObservationV1::Invalidated
                ));
                assert!(matches!(
                    observation.stale_state,
                    RootStateObservationV1::Invalidated
                ));
                assert!(matches!(
                    observation.reopened_state,
                    RootStateObservationV1::Invalidated
                ));
                continue;
            }

            assert_eq!(observation.storage_bytes_retained, 0);
            assert_eq!(observation.storage_inodes_retained, 0);
            assert_eq!(observation.followup_storage_bytes_retained, 0);
            assert_eq!(observation.followup_storage_inodes_retained, 0);
            assert!(observation.followup_zero_forbidden_work);
        }
    }

    #[test]
    fn seventeenth_operation_genuinely_queues_then_grants_cancels_or_exceeds_deadline() {
        let grant = TempFsCas::new("true-c-plus-one-grant");
        let cancelled = TempFsCas::new("true-c-plus-one-cancelled");
        let deadline = TempFsCas::new("true-c-plus-one-deadline");
        let observations =
            seventeenth_operation_queue_v1([grant.path(), cancelled.path(), deadline.path()]);

        for observation in observations {
            assert_eq!(observation.active_at_capacity, 16);
            assert!(observation.queue_deadline_preserved);
            assert_eq!(observation.active_while_queued, 16);
            assert!(matches!(observation.terminal_was_early, false));
            assert!(observation.setup_zero_forbidden_work);
            match observation.transition {
                QueuedTransitionObservationV1::Grant => {
                    assert_eq!(observation.terminal, QueuedTransitionObservationV1::Grant)
                }
                QueuedTransitionObservationV1::Cancelled => assert_eq!(
                    observation.terminal,
                    QueuedTransitionObservationV1::Cancelled
                ),
                QueuedTransitionObservationV1::Deadline => assert_eq!(
                    observation.terminal,
                    QueuedTransitionObservationV1::Deadline
                ),
            }
            assert_eq!(observation.queue_entries, 1);
            assert_eq!(observation.queue_refusals, 0);
            assert_eq!(observation.queue_depth_high_water, 1);
            assert_eq!(
                observation.active_slots_high_water,
                if observation.transition == QueuedTransitionObservationV1::Grant {
                    16
                } else {
                    0
                }
            );
            assert!(observation.wait_polls >= 2);
            assert!(observation.wait_nanoseconds > 0);
            assert_eq!(observation.release_failures, 0);
            assert!(observation.zero_forbidden_work);
            assert_eq!(observation.queue_after, (0, 0, 0));
        }
    }

    #[test]
    fn queued_cancel_and_deadline_create_no_preparation_and_cannot_invoke_typed_supplier() {
        let cancelled = TempFsCas::new("queued-cancel");
        let deadline = TempFsCas::new("queued-deadline");
        let observations = queued_stop_before_supplier_v1([cancelled.path(), deadline.path()]);

        for observation in observations {
            assert!(matches!(
                (observation.expected, observation.terminal),
                (
                    AdmissionRefusalObservationV1::Cancelled,
                    AdmissionRefusalObservationV1::Cancelled
                ) | (
                    AdmissionRefusalObservationV1::Deadline,
                    AdmissionRefusalObservationV1::Deadline
                )
            ));
            assert!(!observation.supplier_invoked);
            assert_eq!(observation.preparation_entries, 0);
            assert_eq!(observation.queue_entries, 17);
            assert_eq!(observation.queue_refusals, 0);
            assert_eq!(observation.queue_depth_high_water, 1);
            assert_eq!(observation.active_slots_high_water, 16);
            assert_eq!(observation.wait_polls, 0);
            assert_eq!(observation.memory_refusals, 0);
            assert_eq!(observation.release_failures, 0);
            assert!(observation.zero_forbidden_work);
        }
    }

    #[test]
    fn one_thousand_twenty_fifth_operation_entry_refuses_before_callbacks_or_storage_work() {
        let root = TempFsCas::new("queue-c-plus-one");
        let observation = queue_capacity_refusal_v1(root.path());

        assert!(matches!(
            observation.terminal,
            AdmissionRefusalObservationV1::Queue
        ));
        assert!(!observation.supplier_invoked);
        assert_eq!(observation.queue_entries, 0);
        assert_eq!(observation.queue_refusals, 1);
        assert_eq!(observation.source_read_calls, 0);
        assert_eq!(observation.source_bytes_read, 0);
        assert_eq!(observation.preparation_bytes_high_water, 0);
        assert_eq!(observation.preparation_inodes_high_water, 0);
        assert_eq!(observation.open_handles_high_water, 0);
        assert_eq!(observation.preparation_entries, 0);
        assert_eq!(observation.pending_tickets, 1_024);
    }

    #[test]
    fn root_storage_byte_and_inode_refusal_precede_supplier_and_preparation() {
        let bytes = TempFsCas::new("storage-byte-refusal");
        let inodes = TempFsCas::new("storage-inode-refusal");
        let observations = storage_refusal_before_supplier_v1([bytes.path(), inodes.path()]);

        for observation in observations {
            assert!(matches!(
                (observation.resource, observation.terminal),
                (
                    AdmissionRefusalObservationV1::StorageBytes,
                    AdmissionRefusalObservationV1::StorageBytes
                ) | (
                    AdmissionRefusalObservationV1::StorageInodes,
                    AdmissionRefusalObservationV1::StorageInodes
                )
            ));
            assert!(!observation.bound_invoked);
            assert!(!observation.supply_invoked);
            assert_eq!(observation.source_read_calls, 0);
            assert_eq!(observation.source_bytes_read, 0);
            assert_eq!(observation.operation_slots, 1);
            assert_eq!(observation.operation_active, 1);
            assert_eq!(observation.storage_active_operations, 1);
            assert_eq!(observation.preparation_entries, 0);
            assert!(observation.zero_forbidden_work);
            assert_eq!(observation.blocker_byte_equation_holds, true);
            assert_eq!(observation.blocker_inode_equation_holds, true);
            assert!(observation.blocker_zero_forbidden_work);
            assert_eq!(observation.authority_clean, true);
            assert_eq!(observation.blocker_storage_equations_hold, true);
        }
    }
}

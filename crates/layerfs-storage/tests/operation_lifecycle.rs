mod support;

#[cfg(feature = "operation-polymorphism")]
mod operation_lifecycle_owner {
    use crate::support::temp_fs_cas::TempFsCas;
    use layerfs_storage::qualification::cas::semantic::{
        PublicationCauseV1, PublicationCleanupTargetV1, PublicationErrorV1,
    };
    use layerfs_storage::qualification::lifecycle::semantic::{
        admission_callback_unwind_v1, carrier_post_link_unwind_v1, carrier_pre_link_unwind_v1,
        exact_complete_operation_boundary_v1, final_handoff_admission_poison_v1,
        final_handoff_storage_poison_v1, final_handoff_unwind_v1, locator_cleanup_residue_v1,
        locator_cleanup_unwind_v1, locator_rollback_accounting_poison_v1,
        locator_rollback_directional_fault_v1, open_existing_subprocess_child_v1, open_existing_v1,
        post_catalog_control_terminal_v1, post_link_marker_secondary_v1,
        preparation_cleanup_and_invalidation_failure_v1, preparation_cleanup_boundary_failure_v1,
        preparation_cleanup_failure_lifecycle_v1, preparation_cleanup_unwind_v1,
        preparation_construction_boundary_unwind_v1, preparation_free_unwind_v1,
        preparation_initialization_unwind_fault_v1, private_pack_cleanup_unwind_v1,
        probe_open_existing_subprocess_v1, run_exclusive_owner_transfer_v1,
        run_open_existing_subprocess_v1, typed_preparation_free_error_v1,
        visible_catalog_terminal_v1, AdmissionPanicBoundaryV1, AdmissionUnwindPrivateCleanupV1,
        CarrierCleanupAfterUnwindV1, CreateFaultObservationV1, LocatorRollbackUnlinkFaultModeV1,
        OpenExistingObservationV1, PostCatalogControlFailureV1, PostLinkAliasCleanupV1,
        PostLinkMarkerTargetV1, PreparationCleanupBoundaryV1, PreparationConstructionBoundaryV1,
        PreparationResidueV1, ResidueAccountingBoundaryV1, RollbackCleanupTargetV1,
        SubprocessObservationV1,
    };
    use layerfs_storage::qualification::resources::{
        terminal_optional_observations_v1, ObservationScopeV1, OptionalObservationStatusV1,
        OptionalU64ObservationV1, TerminalOptionalObservationsV1,
    };
    use layerfs_storage::CoreError;

    fn fresh_root(label: &str) -> TempFsCas {
        TempFsCas::new(label)
    }

    fn terminal_or_cleanup_error(fail_invalidation: bool) -> Option<PublicationErrorV1> {
        if fail_invalidation {
            Some(PublicationErrorV1::TerminalFailure)
        } else {
            Some(PublicationErrorV1::CleanupFailed)
        }
    }

    fn pre_link_callback_error(
        cleanup_unwinds: bool,
        fail_invalidation: bool,
    ) -> Option<PublicationErrorV1> {
        if cleanup_unwinds {
            None
        } else if fail_invalidation {
            Some(PublicationErrorV1::TerminalFailure)
        } else {
            Some(PublicationErrorV1::CleanupFailed)
        }
    }

    #[test]
    fn typed_optional_observation_never_fabricates_an_unavailable_value() {
        let observed = OptionalU64ObservationV1::observed(
            0,
            "direct zero-valued operation observation",
            ObservationScopeV1::Operation,
        );
        assert_eq!(observed.status(), OptionalObservationStatusV1::Observed);
        assert_eq!(observed.value(), Some(0));
        assert_eq!(observed.scope(), ObservationScopeV1::Operation);
        assert_eq!(
            observed.method(),
            "direct zero-valued operation observation"
        );

        for absent in [
            OptionalU64ObservationV1::unavailable(
                "platform observation unavailable",
                ObservationScopeV1::Host,
            ),
            OptionalU64ObservationV1::not_applicable(
                "observation does not apply",
                ObservationScopeV1::Process,
            ),
            OptionalU64ObservationV1::deferred(
                "observation deferred to a later milestone",
                ObservationScopeV1::Root,
            ),
        ] {
            assert_ne!(absent.status(), OptionalObservationStatusV1::Observed);
            assert_eq!(absent.value(), None);
            assert!(!absent.method().is_empty());
        }
    }

    #[test]
    fn terminal_host_observations_are_named_typed_and_never_fabricated() {
        let observations: TerminalOptionalObservationsV1 = terminal_optional_observations_v1();

        for observation in observations.all() {
            assert_eq!(
                observation.status(),
                OptionalObservationStatusV1::Unavailable
            );
            assert_eq!(observation.value(), None);
            assert!(!observation.method().is_empty());
        }

        for process in [
            observations.process_cpu_nanoseconds(),
            observations.allocator_live_bytes(),
            observations.allocator_high_water_bytes(),
            observations.rss_bytes(),
            observations.pss_bytes(),
            observations.page_cache_bytes(),
            observations.process_open_descriptors(),
        ] {
            assert_eq!(process.scope(), ObservationScopeV1::Process);
        }
        assert_eq!(
            observations.host_open_descriptors().scope(),
            ObservationScopeV1::Host
        );
        for root in [
            observations.filesystem_allocated_bytes(),
            observations.filesystem_allocated_blocks(),
            observations.filesystem_free_bytes(),
            observations.filesystem_quota_bytes(),
            observations.physical_inodes(),
        ] {
            assert_eq!(root.scope(), ObservationScopeV1::Root);
        }
    }

    #[test]
    fn subprocess_open_existing_probe() {
        if let Some((observation, expected)) = open_existing_subprocess_child_v1() {
            match expected {
                OpenExistingObservationV1::Busy => {
                    assert!(matches!(observation, OpenExistingObservationV1::Busy))
                }
                OpenExistingObservationV1::Invalidated => {
                    assert!(matches!(
                        observation,
                        OpenExistingObservationV1::Invalidated
                    ))
                }
                OpenExistingObservationV1::Opened | OpenExistingObservationV1::Rejected => {
                    assert!(
                        observation == OpenExistingObservationV1::Opened
                            || observation == OpenExistingObservationV1::Rejected
                    )
                }
            }
            return;
        }
        let observation = run_open_existing_subprocess_v1(
            "operation_lifecycle_owner::subprocess_open_existing_probe",
        );
        assert_probe_reports(observation);
    }

    fn assert_probe_reports(observation: SubprocessObservationV1) {
        assert!(observation.child_succeeded());
        assert!(observation.child_reports() >= 1);
        assert!(observation.child_busy_reports() >= 1);
    }

    fn assert_create_fault_authority_baseline(observation: CreateFaultObservationV1) {
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.operation_active(), 0);
        assert_eq!(observation.storage_active_operations(), 0);
        assert_eq!(observation.storage_active_bytes(), 0);
        assert_eq!(observation.storage_active_inodes(), 0);
    }

    fn assert_create_fault_storage_equations(observation: CreateFaultObservationV1) {
        assert_eq!(
            observation.storage_bytes_requested(),
            observation.storage_bytes_reserved()
        );
        assert_eq!(
            observation.storage_bytes_reserved(),
            observation.storage_bytes_released()
                + observation.storage_bytes_committed()
                + observation.storage_bytes_retained()
        );
        assert_eq!(
            observation.storage_inodes_requested(),
            observation.storage_inodes_reserved()
        );
        assert_eq!(
            observation.storage_inodes_reserved(),
            observation.storage_inodes_released()
                + observation.storage_inodes_committed()
                + observation.storage_inodes_retained()
        );
    }

    #[test]
    fn exclusive_root_owner_refuses_subprocess_then_transfers_after_clean_last_drop() {
        let observation = run_exclusive_owner_transfer_v1(
            "operation_lifecycle_owner::subprocess_open_existing_probe",
        );
        assert!(observation.busy_with_alias());
        assert!(observation.busy_after_alias_drop());
        assert!(observation.opened_after_owner_drop());
        assert!(observation.transferred_cleanly());
    }

    #[test]
    fn exact_complete_operation_boundary_spans_slot_request_through_clean_validated_handoff() {
        let fixture = fresh_root("exact-operation-boundary");
        let observation = exact_complete_operation_boundary_v1(fixture.path());
        assert!(observation.completed());
        assert_eq!(observation.starts(), 1);
        assert_eq!(observation.ends(), 1);
        assert!(observation.preparation_empty_at_start());
        assert!(observation.preparation_empty_at_end());
    }

    #[test]
    fn final_handoff_admission_release_failure_retains_exact_immutable_set() {
        let fixture = fresh_root("final-handoff-admission-release");
        let observation = final_handoff_admission_poison_v1(fixture.path(), false);
        assert!(matches!(
            observation.error(),
            Some(PublicationErrorV1::SynchronizationPoisoned)
        ));
        assert!(observation.control_fired());
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.operation_active(), 0);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
        assert!(observation.immutable_entries() >= 1);
        assert_eq!(observation.storage_active_operations(), 0);
        assert_eq!(observation.storage_active_bytes(), 0);
        assert_eq!(observation.storage_active_inodes(), 0);
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
            observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.immutable_entries()
        );
        assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
        assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
        assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
        assert_eq!(
            observation.residue_bytes(),
            observation.storage_bytes_retained()
        );
        assert!(observation.zero_forbidden_work());
        assert!(matches!(observation.invalidated(), true));
        assert!(matches!(observation.stale_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
    }

    #[test]
    fn admission_terminal_invalidation_unwind_retains_first_cause_and_reclassifies_commit() {
        let fixture = fresh_root("admission-terminal-invalidation-unwind");
        let observation = final_handoff_admission_poison_v1(fixture.path(), true);
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::SynchronizationPoisoned)
        );
        assert!(observation.poisoned());
        assert!(observation.control_fired());
        assert_eq!(observation.preparation_entries(), 0);
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert_eq!(
            observation.storage_bytes_retained(),
            observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.immutable_entries()
        );
        assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
        assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
        assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
        assert!(observation.zero_forbidden_work());
        assert!(matches!(observation.invalidated(), true));
        assert!(matches!(observation.stale_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
    }

    #[test]
    fn final_handoff_storage_poison_terminalizes_exact_immutable_set() {
        let fixture = fresh_root("final-handoff-storage-poison");
        let observation = final_handoff_storage_poison_v1(fixture.path(), false);
        assert!(matches!(
            observation.error(),
            Some(PublicationErrorV1::SynchronizationPoisoned)
        ));
        assert!(observation.control_fired());
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.operation_active(), 0);
        assert_eq!(observation.storage_active_operations(), 0);
        assert_eq!(observation.storage_active_bytes(), 0);
        assert_eq!(observation.storage_active_inodes(), 0);
        assert!(observation.poisoned());
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
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
            observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.immutable_entries()
        );
        assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
        assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
        assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
        assert!(observation.immutable_entries() >= 1);
        assert!(observation.immutable_bytes() >= 1);
        assert!(observation.zero_forbidden_work());
        assert!(matches!(observation.invalidated(), true));
        assert!(matches!(observation.stale_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
    }

    #[test]
    fn storage_terminal_invalidation_unwind_still_releases_authority_and_persists_failure() {
        let fixture = fresh_root("storage-terminal-invalidation-unwind");
        let observation = final_handoff_storage_poison_v1(fixture.path(), true);
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::SynchronizationPoisoned)
        );
        assert!(observation.carrier_installed());
        assert!(observation.poisoned());
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.operation_active(), 0);
        assert_eq!(observation.storage_active_operations(), 0);
        assert_eq!(observation.storage_active_bytes(), 0);
        assert_eq!(observation.storage_active_inodes(), 0);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
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
            observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.immutable_entries()
        );
        assert!(observation.zero_forbidden_work());
        assert!(matches!(observation.invalidated(), true));
        assert!(matches!(observation.stale_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
    }

    #[test]
    fn final_handoff_unwind_retains_installed_carriers_and_fails_root_closed() {
        let fixture = fresh_root("final-handoff-unwind");
        let observation = final_handoff_unwind_v1(fixture.path(), false);
        assert!(observation.panicked());
        assert!(observation.control_fired());
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
        assert!(observation.immutable_entries() >= 1);
        assert!(observation.immutable_bytes() >= 1);
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
            observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.immutable_entries()
        );
        assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
        assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
        assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
        assert!(observation.zero_forbidden_work());
        assert!(fixture.path().join("invalidated").is_dir());
        assert!(matches!(observation.invalidated(), true));
        assert!(matches!(observation.stale_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
    }

    #[test]
    fn final_handoff_and_invalidation_double_unwind_still_terminalizes_operation() {
        let fixture = fresh_root("final-handoff-invalidation-double-unwind");
        let observation = final_handoff_unwind_v1(fixture.path(), true);
        assert!(observation.panicked());
        assert!(observation.control_fired());
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
        assert!(observation.immutable_bytes() >= 1);
        assert!(observation.immutable_entries() >= 1);
        assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
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
            observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.immutable_entries()
        );
        assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
        assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
        assert!(observation.zero_forbidden_work());
        assert!(matches!(observation.invalidated(), true));
        assert!(matches!(observation.stale_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
    }

    #[test]
    fn supplier_unwind_finishes_explicit_cleanup_storage_equations_and_slot_release() {
        let fixture = fresh_root("supplier-unwind");
        let observation = preparation_free_unwind_v1(fixture.path());
        assert!(observation.panicked());
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.preparation_entries(), 0);
        assert!(observation.storage_bytes_requested() > 0);
        assert_eq!(
            observation.storage_bytes_requested(),
            observation.storage_bytes_reserved()
        );
        assert_eq!(
            observation.storage_bytes_reserved(),
            observation.storage_bytes_released()
                + observation.storage_bytes_committed()
                + observation.storage_bytes_retained()
        );
        assert_eq!(
            observation.storage_inodes_requested(),
            observation.storage_inodes_reserved()
        );
        assert_eq!(
            observation.storage_inodes_reserved(),
            observation.storage_inodes_released()
                + observation.storage_inodes_committed()
                + observation.storage_inodes_retained()
        );
        assert!(observation.followup_succeeded());
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert_eq!(observation.storage_bytes_retained(), 0);
        assert_eq!(observation.storage_inodes_retained(), 0);
        assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
        assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
        assert_eq!(observation.immutable_bytes(), 0);
        assert_eq!(observation.immutable_entries(), 0);
    }

    #[test]
    fn complete_preflight_rejects_traversal_and_page_scratch_before_supplier_or_preparation() {
        let fixture = fresh_root("preflight-refusal");
        let observation = typed_preparation_free_error_v1(fixture.path());
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::Core(CoreError::ResourceRefused))
        );
        assert!(!observation.supply_invoked());
        assert_eq!(observation.operation_slots(), 0);
        assert_eq!(observation.operation_active(), 0);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
        assert_eq!(observation.storage_active_operations(), 0);
        assert_eq!(observation.storage_active_bytes(), 0);
        assert_eq!(observation.storage_active_inodes(), 0);
        assert_eq!(observation.source_read_calls(), 0);
        assert_eq!(observation.storage_bytes_committed(), 0);
    }

    #[test]
    fn preparation_cleanup_failure_is_typed_invalidates_shared_owner_and_retains_exact_residue() {
        std::thread::Builder::new()
            .name("preparation-cleanup-failure".into())
            .spawn(|| {
                let fixture = fresh_root("preparation-cleanup-failure");
                let observation = preparation_cleanup_failure_lifecycle_v1(fixture.path());
                assert_eq!(observation.error(), Some(PublicationErrorV1::CleanupFailed));
                assert_eq!(observation.first_cause(), observation.dominant_cause());
                assert_eq!(
                    observation.dominant_cause(),
                    Some(PublicationCauseV1::CleanupFailed(
                        PublicationCleanupTargetV1::PreparationSpool
                    ))
                );
                assert!(observation.control_fired());
                assert_eq!(observation.cleanup_calls(), 1);
                assert_eq!(observation.preparation_entries(), 1);
                assert_eq!(
                    observation.preparation_residue(),
                    PreparationResidueV1::GlobalSeen
                );
                assert_eq!(observation.storage_bytes_committed(), 0);
                assert_eq!(observation.storage_inodes_committed(), 0);
                assert_eq!(
                    observation.storage_bytes_retained(),
                    observation.preparation_bytes() + observation.immutable_bytes()
                );
                assert_eq!(
                    observation.storage_inodes_retained(),
                    observation.preparation_entries() + observation.immutable_entries()
                );
                assert_eq!(
                    observation.mutable_preparation_residue_bytes(),
                    observation.preparation_bytes()
                );
                assert_eq!(
                    observation.mutable_preparation_residue_inodes(),
                    observation.preparation_entries()
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
                assert_eq!(
                    observation.preparation_bytes(),
                    observation.global_seen_table_bytes()
                );
                assert!(observation.global_seen_table_bytes() > 0);
                assert!(observation.zero_forbidden_work());
                assert!(observation.persistent_invalidation());
                assert!(observation.invalidated());
                assert!(observation.stale_invalidated());
                assert!(observation.reopen_invalidated());
            })
            .expect("spawn preparation-cleanup semantic owner")
            .join()
            .expect("join preparation-cleanup semantic owner");
    }

    #[test]
    fn preparation_construction_unwind_explicitly_cleans_every_locally_owned_spool() {
        for boundary in [
            PreparationConstructionBoundaryV1::CreateBuiltDirectories,
            PreparationConstructionBoundaryV1::CreateBuiltFiles,
            PreparationConstructionBoundaryV1::CreateGlobalSeen,
            PreparationConstructionBoundaryV1::CreateClosureObjects,
            PreparationConstructionBoundaryV1::CreatePackIndex,
            PreparationConstructionBoundaryV1::CreateChunkReferences,
            PreparationConstructionBoundaryV1::InitializeGlobalSeen,
            PreparationConstructionBoundaryV1::SetPermissions,
        ] {
            let fixture = fresh_root("preparation-construction-unwind");
            let observation = preparation_construction_boundary_unwind_v1(fixture.path(), boundary);
            assert!(observation.panicked());
            assert_eq!(
                observation.panic_payload(),
                Some("injected preparation boundary unwind")
            );
            assert!(observation.control_fired());
            assert_eq!(observation.preparation_entries(), 0);
            assert_eq!(
                observation.preparation_residue(),
                PreparationResidueV1::None
            );
            assert_create_fault_authority_baseline(observation);
            assert_create_fault_storage_equations(observation);
            assert_eq!(observation.storage_bytes_committed(), 0);
            assert_eq!(observation.storage_inodes_committed(), 0);
            assert_eq!(observation.storage_bytes_retained(), 0);
            assert_eq!(observation.storage_inodes_retained(), 0);
            assert!(observation.zero_forbidden_work());
            assert!(!observation.persistent_invalidation());
            assert!(!observation.invalidated());
            assert!(!observation.stale_invalidated());
            assert!(!observation.reopen_invalidated());
            assert!(observation.visibility_lock_available());
            assert!(observation.publication_lock_available());
            assert!(observation.followup_succeeded());
            assert!(observation.bound_invoked());
            assert!(observation.supply_invoked());
            assert!(!observation.poisoned());
        }
    }

    #[test]
    fn preparation_cleanup_unwind_attempts_all_lifecycle_targets_before_typed_terminal() {
        for fail_invalidation in [false, true] {
            for boundary in [
                PreparationCleanupBoundaryV1::BuiltDirectories,
                PreparationCleanupBoundaryV1::BuiltFiles,
                PreparationCleanupBoundaryV1::GlobalSeen,
                PreparationCleanupBoundaryV1::ClosureObjects,
                PreparationCleanupBoundaryV1::PackIndex,
                PreparationCleanupBoundaryV1::ChunkReferences,
                PreparationCleanupBoundaryV1::LocatorReceipts,
            ] {
                let fixture = fresh_root("preparation-cleanup-unwind");
                let observation =
                    preparation_cleanup_unwind_v1(fixture.path(), boundary, fail_invalidation);
                let expected = if fail_invalidation {
                    PublicationErrorV1::TerminalFailure
                } else {
                    PublicationErrorV1::CleanupFailed
                };
                assert_eq!(observation.error(), Some(expected));
                assert_eq!(
                    observation.first_cause(),
                    Some(PublicationCauseV1::CleanupFailed(
                        PublicationCleanupTargetV1::PreparationSpool
                    ))
                );
                let dominant = if fail_invalidation {
                    PublicationCauseV1::InvalidationFailed
                } else {
                    PublicationCauseV1::CleanupFailed(PublicationCleanupTargetV1::PreparationSpool)
                };
                assert_eq!(observation.dominant_cause(), Some(dominant));
                assert!(observation.control_fired());
                assert_eq!(observation.cleanup_calls(), 7);
                assert_eq!(
                    observation.invalidation_attempts(),
                    u32::from(fail_invalidation)
                );
                assert_create_fault_authority_baseline(observation);
                assert_eq!(observation.preparation_entries(), 1);
                assert_eq!(observation.carrier_entries(), 1);
                assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
                assert_create_fault_storage_equations(observation);
                assert_eq!(observation.storage_bytes_committed(), 0);
                assert_eq!(observation.storage_inodes_committed(), 0);
                assert_eq!(
                    observation.storage_bytes_retained(),
                    observation.preparation_bytes() + observation.immutable_bytes()
                );
                assert_eq!(
                    observation.storage_inodes_retained(),
                    observation.preparation_entries() + observation.immutable_entries()
                );
                assert!(observation.zero_forbidden_work());
                assert_eq!(observation.persistent_invalidation(), !fail_invalidation);
                assert!(observation.invalidated());
                assert!(observation.stale_invalidated());
                let reopened = open_existing_v1(fixture.path());
                assert!(matches!(
                    reopened,
                    OpenExistingObservationV1::Invalidated | OpenExistingObservationV1::Busy
                ));
            }
        }
    }

    fn assert_private_pack_cleanup_unwind_terminal(
        observation: CreateFaultObservationV1,
        fail_invalidation: bool,
    ) {
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::Filesystem)
        );
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::TerminalFailure)
        );
        assert!(observation.control_fired());
        assert_eq!(observation.cleanup_calls(), 1);
        assert_eq!(
            observation.invalidation_attempts(),
            u32::from(fail_invalidation)
        );
        let dominant = if fail_invalidation {
            PublicationCauseV1::InvalidationFailed
        } else {
            PublicationCauseV1::CleanupFailed(PublicationCleanupTargetV1::PrivatePack)
        };
        assert_eq!(observation.dominant_cause(), Some(dominant));
        assert_create_fault_authority_baseline(observation);
        assert_eq!(observation.preparation_entries(), 1);
        assert_eq!(
            observation.preparation_residue(),
            PreparationResidueV1::PrivatePack
        );
        assert!(observation.preparation_bytes() > 0);
        assert_create_fault_storage_equations(observation);
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert_eq!(
            observation.storage_bytes_retained(),
            observation.preparation_bytes() + observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.preparation_entries() + observation.immutable_entries()
        );
        assert!(observation.zero_forbidden_work());
        assert_eq!(observation.persistent_invalidation(), !fail_invalidation);
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
    }

    #[test]
    fn private_pack_cleanup_unwind_terminalizes_storage_and_preparation_before_return() {
        let fixture = fresh_root("private-pack-cleanup-unwind");
        let observation = private_pack_cleanup_unwind_v1(fixture.path(), false);
        assert_private_pack_cleanup_unwind_terminal(observation, false);
    }

    #[test]
    fn private_pack_cleanup_unwind_retains_invalidation_double_fault() {
        let fixture = fresh_root("private-pack-cleanup-invalidation-double-fault");
        let observation = private_pack_cleanup_unwind_v1(fixture.path(), true);
        assert_private_pack_cleanup_unwind_terminal(observation, true);
    }

    #[test]
    fn every_lifecycle_preparation_cleanup_boundary_is_fallible_and_invalidates_exactly() {
        for (boundary, residue) in [
            (
                PreparationCleanupBoundaryV1::BuiltDirectories,
                PreparationResidueV1::BuiltDirectories,
            ),
            (
                PreparationCleanupBoundaryV1::BuiltFiles,
                PreparationResidueV1::BuiltFiles,
            ),
            (
                PreparationCleanupBoundaryV1::GlobalSeen,
                PreparationResidueV1::GlobalSeen,
            ),
            (
                PreparationCleanupBoundaryV1::ClosureObjects,
                PreparationResidueV1::ClosureObjects,
            ),
            (
                PreparationCleanupBoundaryV1::PackIndex,
                PreparationResidueV1::PackIndex,
            ),
            (
                PreparationCleanupBoundaryV1::ChunkReferences,
                PreparationResidueV1::ChunkReferences,
            ),
            (
                PreparationCleanupBoundaryV1::LocatorReceipts,
                PreparationResidueV1::LocatorReceipts,
            ),
        ] {
            let fixture = fresh_root("all-preparation-cleanup-boundaries");
            let observation = preparation_cleanup_boundary_failure_v1(fixture.path(), boundary);
            assert_eq!(observation.error(), Some(PublicationErrorV1::CleanupFailed));
            assert_eq!(
                observation.first_cause(),
                Some(PublicationCauseV1::CleanupFailed(
                    PublicationCleanupTargetV1::PreparationSpool
                ))
            );
            assert_eq!(observation.first_cause(), observation.dominant_cause());
            assert!(observation.control_fired());
            assert_eq!(observation.cleanup_calls(), 7);
            assert_create_fault_authority_baseline(observation);
            assert_eq!(observation.preparation_entries(), 1);
            assert!(observation.preparation_bytes() > 0);
            assert_eq!(observation.preparation_residue(), residue);
            assert_eq!(
                observation.preparation_bytes(),
                observation.mutable_preparation_residue_bytes()
            );
            assert_eq!(
                observation.preparation_entries(),
                observation.mutable_preparation_residue_inodes()
            );
            assert_eq!(observation.storage_bytes_committed(), 0);
            assert_eq!(observation.storage_inodes_committed(), 0);
            assert_eq!(
                observation.storage_bytes_retained(),
                observation.preparation_bytes() + observation.immutable_bytes()
            );
            assert_eq!(
                observation.storage_inodes_retained(),
                observation.preparation_entries() + observation.immutable_entries()
            );
            assert_create_fault_storage_equations(observation);
            assert!(observation.zero_forbidden_work());
            assert!(observation.persistent_invalidation());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_invalidated());
            assert!(probe_open_existing_subprocess_v1(
                fixture.path(),
                "operation_lifecycle_owner::subprocess_open_existing_probe",
                OpenExistingObservationV1::Invalidated,
            ));
        }
    }

    #[test]
    fn cleanup_and_persistent_invalidation_double_fault_remains_fail_closed_after_drop_and_subprocess(
    ) {
        let fixture = fresh_root("cleanup-invalidation-double-fault");
        let observation = preparation_cleanup_and_invalidation_failure_v1(fixture.path(), true);
        assert_eq!(
            observation.error(),
            Some(PublicationErrorV1::TerminalFailure)
        );
        assert_eq!(
            observation.first_cause(),
            Some(PublicationCauseV1::CleanupFailed(
                PublicationCleanupTargetV1::PreparationSpool
            ))
        );
        assert_eq!(
            observation.dominant_cause(),
            Some(PublicationCauseV1::InvalidationFailed)
        );
        assert!(observation.control_fired());
        assert_eq!(observation.invalidation_attempts(), 1);
        assert_create_fault_authority_baseline(observation);
        assert_eq!(observation.preparation_entries(), 1);
        assert!(observation.preparation_bytes() > 0);
        assert_eq!(
            observation.preparation_residue(),
            PreparationResidueV1::GlobalSeen
        );
        assert_eq!(
            observation.preparation_bytes(),
            observation.global_seen_table_bytes()
        );
        assert_eq!(
            observation.mutable_preparation_residue_bytes(),
            observation.preparation_bytes()
        );
        assert!(!observation.persistent_invalidation());
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert_eq!(
            open_existing_v1(fixture.path()),
            OpenExistingObservationV1::Busy
        );
        assert!(probe_open_existing_subprocess_v1(
            fixture.path(),
            "operation_lifecycle_owner::subprocess_open_existing_probe",
            OpenExistingObservationV1::Busy,
        ));
    }

    fn observation_is_busy_or_invalidated(observation: OpenExistingObservationV1) -> bool {
        matches!(
            observation,
            OpenExistingObservationV1::Busy | OpenExistingObservationV1::Invalidated
        )
    }

    fn observation_is_opened(observation: OpenExistingObservationV1) -> bool {
        matches!(observation, OpenExistingObservationV1::Opened)
    }

    fn observation_is_rejected(observation: OpenExistingObservationV1) -> bool {
        matches!(observation, OpenExistingObservationV1::Rejected)
    }

    fn filesystem_fault_is_typed(
        observation: layerfs_storage::qualification::lifecycle::semantic::FilesystemFaultObservationV1,
    ) -> bool {
        matches!(
            observation.error(),
            Some(
                layerfs_storage::qualification::lifecycle::semantic::FilesystemFaultErrorV1::Filesystem(
                    _
                ) | layerfs_storage::qualification::lifecycle::semantic::FilesystemFaultErrorV1::Unsupported
            )
        )
    }

    #[test]
    fn carrier_pre_link_unwind_releases_publication_guard_and_preserves_healthy_root() {
        let fixture = fresh_root("carrier-pre-link-unwind");
        let observation = carrier_pre_link_unwind_v1(fixture.path());
        assert_eq!(
            observation.panic_payload(),
            Some("injected carrier pre-link unwind")
        );
        assert!(observation.control_fired());
        assert_create_fault_authority_baseline(observation);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
        assert_eq!(observation.immutable_entries(), 0);
        assert_eq!(observation.immutable_bytes(), 0);
        assert_create_fault_storage_equations(observation);
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert_eq!(observation.storage_bytes_retained(), 0);
        assert_eq!(observation.storage_inodes_retained(), 0);
        assert_eq!(observation.residue_bytes(), 0);
        assert!(observation.zero_forbidden_work());
        assert!(observation.followup_succeeded());
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert!(observation.visibility_lock_available());
        assert!(observation.publication_lock_available());
        assert!(!observation.invalidated());
        assert!(!observation.stale_invalidated());
        assert!(!observation.reopen_invalidated());
        assert!(!observation.reopen_rejected());
        assert!(!observation.persistent_invalidation());
        assert!(!observation.poisoned());
    }

    #[test]
    fn carrier_post_link_unwind_rolls_back_once_or_retains_exact_fail_closed_residue() {
        for (cleanup, fail_invalidation, overflow) in [
            (CarrierCleanupAfterUnwindV1::Succeeds, false, false),
            (CarrierCleanupAfterUnwindV1::Succeeds, false, true),
            (CarrierCleanupAfterUnwindV1::Fails, false, false),
            (CarrierCleanupAfterUnwindV1::Fails, true, false),
            (CarrierCleanupAfterUnwindV1::Unwinds, false, false),
            (CarrierCleanupAfterUnwindV1::Unwinds, true, false),
        ] {
            let retained = cleanup != CarrierCleanupAfterUnwindV1::Succeeds;
            let fixture = fresh_root("carrier-post-link-unwind");
            let observation =
                carrier_post_link_unwind_v1(fixture.path(), cleanup, fail_invalidation, overflow);
            if overflow {
                assert_eq!(
                    observation.error(),
                    Some(PublicationErrorV1::Core(CoreError::IntegerOverflow))
                );
            } else if retained {
                assert_eq!(
                    observation.error(),
                    Some(if fail_invalidation {
                        PublicationErrorV1::TerminalFailure
                    } else {
                        PublicationErrorV1::CleanupFailed
                    })
                );
                assert_eq!(
                    observation.first_cause(),
                    Some(PublicationCauseV1::CleanupFailed(
                        PublicationCleanupTargetV1::Carrier
                    ))
                );
                assert_eq!(
                    observation.dominant_cause(),
                    Some(if fail_invalidation {
                        PublicationCauseV1::InvalidationFailed
                    } else {
                        PublicationCauseV1::CleanupFailed(PublicationCleanupTargetV1::Carrier)
                    })
                );
            } else {
                assert!(observation.panicked());
                assert_eq!(
                    observation.panic_payload(),
                    Some("injected carrier post-link unwind")
                );
            }
            assert!(observation.control_fired());
            assert_eq!(observation.poisoned(), overflow);
            assert_eq!(observation.cleanup_calls(), 1);
            assert_eq!(observation.terminal_hook_calls(), 1);
            assert_eq!(observation.invalidation_attempts(), u32::from(retained));
            assert_create_fault_authority_baseline(observation);
            assert_eq!(observation.preparation_entries(), 0);
            assert_eq!(observation.preparation_bytes(), 0);
            assert_eq!(observation.carrier_entries(), u64::from(retained));
            assert_eq!(observation.immutable_entries(), u64::from(retained));
            assert_eq!(observation.immutable_bytes(), observation.carrier_bytes());
            assert_create_fault_storage_equations(observation);
            assert_eq!(observation.storage_bytes_committed(), 0);
            assert_eq!(observation.storage_inodes_committed(), 0);
            assert_eq!(
                observation.storage_bytes_retained(),
                observation.immutable_bytes()
            );
            assert_eq!(
                observation.storage_inodes_retained(),
                observation.immutable_entries()
            );
            assert_eq!(observation.residue_bytes(), observation.carrier_bytes());
            assert_eq!(
                observation.immutable_residue_bytes(),
                observation.immutable_bytes()
            );
            assert_eq!(
                observation.immutable_residue_inodes(),
                observation.immutable_entries()
            );
            assert!(observation.zero_forbidden_work());
            assert_eq!(observation.invalidated(), retained);
            assert_eq!(observation.stale_invalidated(), retained);
            assert_eq!(observation.reopen_rejected(), retained);
            assert_eq!(
                observation.persistent_invalidation(),
                retained && !fail_invalidation
            );
        }
    }

    #[test]
    fn locator_cleanup_residue_retains_its_carrier_without_unlink_attempt() {
        let fixture = fresh_root("locator-carrier-double-fault");
        let observation = locator_cleanup_residue_v1(fixture.path());
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
            Some(PublicationCauseV1::CleanupFailed(
                PublicationCleanupTargetV1::ObjectLocator
            ))
        );
        assert!(observation.control_fired());
        assert!(observation.bound_invoked());
        assert!(observation.supply_invoked());
        assert_eq!(observation.cleanup_calls(), 1);
        assert_eq!(observation.terminal_hook_calls(), 0);
        assert_create_fault_authority_baseline(observation);
        assert_eq!(observation.preparation_entries(), 0);
        assert_eq!(observation.preparation_bytes(), 0);
        assert_eq!(observation.carrier_entries(), 1);
        assert_eq!(observation.locator_entries(), 1);
        assert_eq!(
            observation.immutable_bytes(),
            observation.carrier_bytes() + observation.locator_bytes()
        );
        assert_eq!(observation.immutable_entries(), 2);
        assert_create_fault_storage_equations(observation);
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert_eq!(
            observation.storage_bytes_retained(),
            observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.immutable_entries()
        );
        assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
        assert_eq!(
            observation.immutable_residue_bytes(),
            observation.immutable_bytes()
        );
        assert_eq!(
            observation.immutable_residue_inodes(),
            observation.immutable_entries()
        );
        assert!(observation.zero_forbidden_work());
        assert!(observation.persistent_invalidation());
        assert!(observation.invalidated());
        assert!(observation.stale_invalidated());
        assert!(observation.reopen_invalidated());
    }

    #[cfg(unix)]
    #[test]
    fn locator_rollback_preserves_directional_unlink_faults_and_dependency_custody() {
        for mode in [
            LocatorRollbackUnlinkFaultModeV1::SampledUnsupported,
            LocatorRollbackUnlinkFaultModeV1::SampledWriteFailure,
            LocatorRollbackUnlinkFaultModeV1::PermissionDenied,
            LocatorRollbackUnlinkFaultModeV1::WriteFailure,
            LocatorRollbackUnlinkFaultModeV1::InjectedCleanup,
        ] {
            for fail_invalidation in [false, true] {
                let fixture = fresh_root("locator-rollback-directional-faults");
                let observation =
                    locator_rollback_directional_fault_v1(fixture.path(), mode, fail_invalidation);
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
                    Some(if fail_invalidation {
                        PublicationCauseV1::InvalidationFailed
                    } else {
                        PublicationCauseV1::CleanupFailed(PublicationCleanupTargetV1::ObjectLocator)
                    })
                );
                assert!(observation.control_fired());
                assert!(observation.bound_invoked());
                assert!(observation.supply_invoked());
                assert_eq!(observation.cleanup_calls(), 0);
                assert_create_fault_authority_baseline(observation);
                assert_create_fault_storage_equations(observation);
                assert_eq!(observation.preparation_entries(), 0);
                assert_eq!(observation.preparation_bytes(), 0);
                assert_eq!(observation.carrier_entries(), 1);
                assert!(observation.locator_entries() >= 1);
                assert_eq!(
                    observation.immutable_bytes(),
                    observation.carrier_bytes() + observation.locator_bytes()
                );
                assert_eq!(
                    observation.immutable_entries(),
                    observation.carrier_entries() + observation.locator_entries()
                );
                assert_eq!(observation.storage_bytes_committed(), 0);
                assert_eq!(observation.storage_inodes_committed(), 0);
                assert_eq!(
                    observation.storage_bytes_retained(),
                    observation.immutable_bytes()
                );
                assert_eq!(
                    observation.storage_inodes_retained(),
                    observation.immutable_entries()
                );
                assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
                assert_eq!(
                    observation.immutable_residue_bytes(),
                    observation.immutable_bytes()
                );
                assert_eq!(
                    observation.immutable_residue_inodes(),
                    observation.immutable_entries()
                );
                assert!(observation.zero_forbidden_work());
                assert!(observation.invalidated());
                assert!(observation.stale_invalidated());
                assert!(observation.reopen_rejected());
            }
        }
    }

    #[test]
    fn locator_rollback_accounting_poison_defers_invalidation_to_owned_terminal() {
        for fail_invalidation in [false, true] {
            let fixture = fresh_root("locator-rollback-accounting-poison");
            let observation =
                locator_rollback_accounting_poison_v1(fixture.path(), fail_invalidation);
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
                Some(if fail_invalidation {
                    PublicationCauseV1::InvalidationFailed
                } else {
                    PublicationCauseV1::CleanupFailed(PublicationCleanupTargetV1::Carrier)
                })
            );
            assert!(observation.poisoned());
            assert!(observation.control_fired());
            assert_eq!(observation.cleanup_calls(), 1);
            assert_create_fault_authority_baseline(observation);
            assert_create_fault_storage_equations(observation);
            assert_eq!(observation.preparation_entries(), 0);
            assert_eq!(observation.preparation_bytes(), 0);
            assert_eq!(observation.immutable_entries(), 0);
            assert_eq!(observation.immutable_bytes(), 0);
            assert_eq!(observation.storage_bytes_committed(), 0);
            assert_eq!(observation.storage_inodes_committed(), 0);
            assert_eq!(observation.storage_bytes_retained(), 0);
            assert_eq!(observation.storage_inodes_retained(), 0);
            assert_eq!(observation.residue_bytes(), 0);
            assert!(observation.zero_forbidden_work());
            assert!(observation.invalidated());
            assert!(observation.stale_invalidated());
            assert!(observation.reopen_rejected());
        }
    }

    #[test]
    fn locator_cleanup_unwind_attempts_every_remaining_locator_and_carrier_once() {
        for target in [
            RollbackCleanupTargetV1::ObjectLocator,
            RollbackCleanupTargetV1::Carrier,
        ] {
            for inject_accounting in [false, true] {
                for fail_invalidation in [false, true] {
                    let fixture = fresh_root("locator-cleanup-unwind");
                    let observation = locator_cleanup_unwind_v1(
                        fixture.path(),
                        target,
                        inject_accounting,
                        fail_invalidation,
                    );
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
                        Some(if fail_invalidation {
                            PublicationCauseV1::InvalidationFailed
                        } else {
                            PublicationCauseV1::CleanupFailed(match target {
                                RollbackCleanupTargetV1::ObjectLocator => {
                                    PublicationCleanupTargetV1::ObjectLocator
                                }
                                RollbackCleanupTargetV1::Carrier => {
                                    PublicationCleanupTargetV1::Carrier
                                }
                            })
                        })
                    );
                    assert!(observation.control_fired());
                    assert_eq!(observation.poisoned(), inject_accounting);
                    assert_eq!(observation.invalidation_attempts(), 1);
                    assert!(observation.cleanup_calls() > 1);
                    assert_eq!(
                        observation.terminal_hook_calls(),
                        u32::from(target == RollbackCleanupTargetV1::Carrier)
                    );
                    assert_create_fault_authority_baseline(observation);
                    assert_create_fault_storage_equations(observation);
                    assert_eq!(observation.preparation_entries(), 0);
                    assert_eq!(observation.preparation_bytes(), 0);
                    assert_eq!(observation.carrier_entries(), 1);
                    assert_eq!(
                        observation.locator_entries(),
                        u64::from(target == RollbackCleanupTargetV1::ObjectLocator)
                    );
                    assert_eq!(
                        observation.immutable_bytes(),
                        observation.carrier_bytes() + observation.locator_bytes()
                    );
                    assert_eq!(
                        observation.immutable_entries(),
                        observation.carrier_entries() + observation.locator_entries()
                    );
                    let expected_direct_residue =
                        if inject_accounting && target == RollbackCleanupTargetV1::Carrier {
                            0
                        } else {
                            observation.immutable_bytes()
                        };
                    assert_eq!(observation.residue_bytes(), expected_direct_residue);
                    assert_eq!(observation.storage_bytes_committed(), 0);
                    assert_eq!(observation.storage_inodes_committed(), 0);
                    assert_eq!(
                        observation.storage_bytes_retained(),
                        observation.immutable_bytes()
                    );
                    assert_eq!(
                        observation.storage_inodes_retained(),
                        observation.immutable_entries()
                    );
                    assert_eq!(
                        observation.immutable_residue_bytes(),
                        observation.immutable_bytes()
                    );
                    assert_eq!(
                        observation.immutable_residue_inodes(),
                        observation.immutable_entries()
                    );
                    assert!(observation.zero_forbidden_work());
                    assert!(observation.invalidated());
                    assert!(observation.stale_invalidated());
                    assert!(observation.reopen_rejected());
                }
            }
        }
    }

    #[test]
    fn post_link_alias_cleanup_failure_retains_visible_dependencies_and_invalidates_reopen() {
        let fixture = fresh_root("post-link-alias-cleanup");
        let observation = post_link_marker_secondary_v1(
            fixture.path(),
            false,
            PostLinkAliasCleanupV1::Fails,
            false,
        );
        assert_eq!(observation.error(), Some(PublicationErrorV1::CleanupFailed));
        assert!(
            observation.first_cause()
                == Some(PublicationCauseV1::CleanupFailed(
                    PublicationCleanupTargetV1::PublishedMarkerAlias
                ))
        );
        assert_eq!(observation.cleanup_calls(), 1);
        assert_eq!(observation.operation_slots(), 0);
        assert!(observation.preparation_entries() >= 1);
        assert_eq!(observation.storage_bytes_committed(), 0);
        assert_eq!(observation.storage_inodes_committed(), 0);
        assert!(observation.preparation_bytes() > 0);
        assert!(observation.immutable_entries() >= 1);
        assert_eq!(
            observation.storage_bytes_retained(),
            observation.preparation_bytes() + observation.immutable_bytes()
        );
        assert_eq!(
            observation.storage_inodes_retained(),
            observation.preparation_entries() + observation.immutable_entries()
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
        assert_eq!(
            observation.mutable_preparation_residue_bytes(),
            observation.preparation_bytes()
        );
        assert_eq!(
            observation.mutable_preparation_residue_inodes(),
            observation.preparation_entries()
        );
        assert!(observation.zero_forbidden_work());
        assert!(matches!(observation.invalidated(), true));
        assert!(matches!(observation.stale_invalidated(), true));
        assert!(matches!(observation.reopen_invalidated(), true));
    }

    #[test]
    fn visible_locator_terminal_retains_carrier_when_residue_accounting_fails() {
        for fail_invalidation in [false, true] {
            let fixture = fresh_root("visible-locator-residue-accounting");
            let observation = post_link_marker_secondary_v1(
                fixture.path(),
                false,
                PostLinkAliasCleanupV1::Fails,
                fail_invalidation,
            );
            let expected = if fail_invalidation {
                PublicationErrorV1::TerminalFailure
            } else {
                PublicationErrorV1::CleanupFailed
            };
            assert_eq!(observation.error(), Some(expected));
            assert!(observation.cleanup_calls() >= 1);
            assert!(observation.bound_invoked());
            assert!(observation.supply_invoked());
            assert!(
                observation.first_cause()
                    == Some(PublicationCauseV1::CleanupFailed(
                        PublicationCleanupTargetV1::PublishedMarkerAlias
                    ))
            );
            assert!(
                observation.dominant_cause() == Some(PublicationCauseV1::InvalidationFailed)
                    || observation.dominant_cause()
                        == Some(PublicationCauseV1::CleanupFailed(
                            PublicationCleanupTargetV1::PublishedMarkerAlias
                        ))
            );
            assert_eq!(observation.operation_slots(), 0);
            assert_eq!(observation.operation_active(), 0);
            assert_eq!(observation.storage_active_operations(), 0);
            assert_eq!(observation.storage_active_bytes(), 0);
            assert_eq!(observation.storage_active_inodes(), 0);
            assert_eq!(observation.storage_bytes_committed(), 0);
            assert_eq!(observation.storage_inodes_committed(), 0);
            assert!(observation.preparation_bytes() > 0);
            assert_eq!(
                observation.storage_bytes_retained(),
                observation.preparation_bytes() + observation.immutable_bytes()
            );
            assert_eq!(
                observation.storage_inodes_retained(),
                observation.preparation_entries() + observation.immutable_entries()
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
            assert_eq!(
                observation.mutable_preparation_residue_bytes(),
                observation.preparation_bytes()
            );
            assert_eq!(
                observation.mutable_preparation_residue_inodes(),
                observation.preparation_entries()
            );
            assert_eq!(observation.residue_bytes(), observation.immutable_bytes());
            assert_eq!(
                observation.storage_bytes_retained(),
                observation.mutable_preparation_residue_bytes() + observation.immutable_bytes()
            );
            assert_eq!(observation.preparation_entries(), 1);
            assert_eq!(observation.source_bytes_read(), 1);
            assert!(observation.zero_forbidden_work());
            assert!(matches!(observation.invalidated(), true));
            assert!(matches!(observation.stale_invalidated(), true));
            assert!(matches!(observation.reopen_invalidated(), true));
        }
    }

    #[test]
    fn visible_catalog_terminal_attempts_every_dependency_custody_once() {
        for accounting_boundary in [
            ResidueAccountingBoundaryV1::CatalogMarker,
            ResidueAccountingBoundaryV1::ObjectLocator,
            ResidueAccountingBoundaryV1::Carrier,
        ] {
            for directional_first_error in [false, true] {
                for fail_invalidation in [false, true] {
                    let fixture = fresh_root("visible-catalog-residue-accounting");
                    let observation = visible_catalog_terminal_v1(
                        fixture.path(),
                        accounting_boundary,
                        directional_first_error,
                        fail_invalidation,
                    );
                    let cleanup = PublicationCauseV1::CleanupFailed(
                        PublicationCleanupTargetV1::PublishedMarkerAlias,
                    );
                    let expected_error = if directional_first_error || fail_invalidation {
                        PublicationErrorV1::TerminalFailure
                    } else {
                        PublicationErrorV1::CleanupFailed
                    };
                    let expected_first = if directional_first_error {
                        PublicationCauseV1::Filesystem
                    } else {
                        cleanup
                    };
                    let expected_dominant = if fail_invalidation {
                        PublicationCauseV1::InvalidationFailed
                    } else {
                        cleanup
                    };

                    assert_eq!(
                        observation.error(),
                        Some(expected_error),
                        "{accounting_boundary:?} directional={directional_first_error} fail_invalidation={fail_invalidation} invalidation_attempts={}",
                        observation.invalidation_attempts()
                    );
                    assert_eq!(observation.first_cause(), Some(expected_first));
                    assert_eq!(observation.dominant_cause(), Some(expected_dominant));
                    assert!(observation.control_fired());
                    assert!(observation.poisoned());
                    assert_eq!(observation.cleanup_calls(), 1);
                    assert_eq!(observation.invalidation_attempts(), 1);
                    assert!(observation.bound_invoked());
                    assert!(observation.supply_invoked());
                    assert_eq!(observation.operation_slots(), 0);
                    assert_eq!(observation.operation_active(), 0);
                    assert_eq!(observation.storage_active_operations(), 0);
                    assert_eq!(observation.storage_active_bytes(), 0);
                    assert_eq!(observation.storage_active_inodes(), 0);
                    assert!(observation.visibility_lock_available());
                    assert!(observation.publication_lock_available());
                    assert_eq!(observation.carrier_entries(), 1);
                    assert!(observation.locator_entries() > 0);
                    assert_eq!(observation.catalog_entries(), 1);
                    assert_eq!(observation.closure_entries(), 0);
                    assert_eq!(observation.closure_bytes(), 0);
                    assert!(observation.preparation_bytes() > 0);
                    assert_eq!(observation.preparation_entries(), 1);
                    assert_eq!(
                        observation.immutable_bytes(),
                        observation.carrier_bytes()
                            + observation.locator_bytes()
                            + observation.catalog_bytes()
                    );
                    assert_eq!(
                        observation.immutable_entries(),
                        observation.carrier_entries()
                            + observation.locator_entries()
                            + observation.catalog_entries()
                    );
                    assert_eq!(observation.storage_bytes_committed(), 0);
                    assert_eq!(observation.storage_inodes_committed(), 0);
                    assert_eq!(
                        observation.storage_bytes_retained(),
                        observation.preparation_bytes() + observation.immutable_bytes()
                    );
                    assert_eq!(
                        observation.storage_inodes_retained(),
                        observation.preparation_entries() + observation.immutable_entries()
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
                    assert_eq!(
                        observation.mutable_preparation_residue_bytes(),
                        observation.preparation_bytes()
                    );
                    assert_eq!(
                        observation.mutable_preparation_residue_inodes(),
                        observation.preparation_entries()
                    );
                    assert_eq!(
                        observation.immutable_residue_bytes(),
                        observation.immutable_bytes()
                    );
                    assert_eq!(
                        observation.immutable_residue_inodes(),
                        observation.immutable_entries()
                    );
                    let missed_bytes = match accounting_boundary {
                        ResidueAccountingBoundaryV1::CatalogMarker => observation.catalog_bytes(),
                        ResidueAccountingBoundaryV1::ObjectLocator => observation.locator_bytes(),
                        ResidueAccountingBoundaryV1::Carrier => observation.carrier_bytes(),
                    };
                    assert_eq!(
                        observation.residue_bytes(),
                        observation.immutable_bytes() - missed_bytes
                    );
                    assert!(observation.zero_forbidden_work());
                    assert!(observation.invalidated());
                    assert!(observation.stale_invalidated());
                    assert!(observation.reopen_rejected());
                }
            }
        }
    }

    #[test]
    fn post_catalog_control_terminal_preserves_cause_and_all_dependency_custody() {
        for control_failure in [
            PostCatalogControlFailureV1::Cancelled,
            PostCatalogControlFailureV1::Deadline,
        ] {
            for accounting_boundary in [
                None,
                Some(ResidueAccountingBoundaryV1::CatalogMarker),
                Some(ResidueAccountingBoundaryV1::ObjectLocator),
                Some(ResidueAccountingBoundaryV1::Carrier),
            ] {
                for fail_invalidation in [false, true] {
                    if accounting_boundary.is_none() && fail_invalidation {
                        continue;
                    }
                    let fixture = fresh_root("post-catalog-control-custody");
                    let observation = post_catalog_control_terminal_v1(
                        fixture.path(),
                        control_failure,
                        accounting_boundary,
                        fail_invalidation,
                    );
                    let first = PublicationCauseV1::Core(match control_failure {
                        PostCatalogControlFailureV1::Cancelled => CoreError::Cancelled,
                        PostCatalogControlFailureV1::Deadline => CoreError::Deadline,
                    });
                    assert_eq!(
                        observation.error(),
                        Some(if fail_invalidation {
                            PublicationErrorV1::TerminalFailure
                        } else {
                            PublicationErrorV1::Core(match control_failure {
                                PostCatalogControlFailureV1::Cancelled => CoreError::Cancelled,
                                PostCatalogControlFailureV1::Deadline => CoreError::Deadline,
                            })
                        })
                    );
                    assert_eq!(observation.first_cause(), Some(first));
                    assert_eq!(
                        observation.dominant_cause(),
                        Some(if fail_invalidation {
                            PublicationCauseV1::InvalidationFailed
                        } else {
                            first
                        })
                    );
                    assert!(observation.control_fired());
                    assert_eq!(observation.poisoned(), accounting_boundary.is_some());
                    assert_eq!(observation.cleanup_calls(), 0);
                    assert_eq!(
                        observation.invalidation_attempts(),
                        u32::from(accounting_boundary.is_some())
                    );
                    assert!(observation.bound_invoked());
                    assert!(observation.supply_invoked());
                    assert_eq!(observation.operation_slots(), 0);
                    assert_eq!(observation.operation_active(), 0);
                    assert_eq!(observation.storage_active_operations(), 0);
                    assert_eq!(observation.storage_active_bytes(), 0);
                    assert_eq!(observation.storage_active_inodes(), 0);
                    assert!(observation.visibility_lock_available());
                    assert!(observation.publication_lock_available());
                    assert_eq!(observation.preparation_bytes(), 0);
                    assert_eq!(observation.preparation_entries(), 0);
                    assert_eq!(observation.carrier_entries(), 1);
                    assert!(observation.locator_entries() > 0);
                    assert_eq!(observation.catalog_entries(), 1);
                    assert_eq!(observation.closure_bytes(), 0);
                    assert_eq!(observation.closure_entries(), 0);
                    assert_eq!(
                        observation.immutable_bytes(),
                        observation.carrier_bytes()
                            + observation.locator_bytes()
                            + observation.catalog_bytes()
                    );
                    assert_eq!(
                        observation.immutable_entries(),
                        observation.carrier_entries()
                            + observation.locator_entries()
                            + observation.catalog_entries()
                    );
                    assert_eq!(observation.storage_bytes_committed(), 0);
                    assert_eq!(observation.storage_inodes_committed(), 0);
                    assert_eq!(
                        observation.storage_bytes_retained(),
                        observation.immutable_bytes()
                    );
                    assert_eq!(
                        observation.storage_inodes_retained(),
                        observation.immutable_entries()
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
                    assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
                    assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
                    assert_eq!(
                        observation.immutable_residue_bytes(),
                        observation.immutable_bytes()
                    );
                    assert_eq!(
                        observation.immutable_residue_inodes(),
                        observation.immutable_entries()
                    );
                    let missed_bytes = match accounting_boundary {
                        None => 0,
                        Some(ResidueAccountingBoundaryV1::CatalogMarker) => {
                            observation.catalog_bytes()
                        }
                        Some(ResidueAccountingBoundaryV1::ObjectLocator) => {
                            observation.locator_bytes()
                        }
                        Some(ResidueAccountingBoundaryV1::Carrier) => observation.carrier_bytes(),
                    };
                    assert_eq!(
                        observation.residue_bytes(),
                        observation.immutable_bytes() - missed_bytes
                    );
                    assert!(observation.zero_forbidden_work());
                    assert_eq!(observation.invalidated(), accounting_boundary.is_some());
                    assert_eq!(
                        observation.stale_invalidated(),
                        accounting_boundary.is_some()
                    );
                    assert_eq!(observation.reopen_rejected(), accounting_boundary.is_some());
                }
            }
        }
    }

    #[test]
    fn admission_callback_unwind_classifies_secondary_terminal_and_dependency_custody() {
        let mut cases = vec![
            (
                AdmissionPanicBoundaryV1::PublicationLockAcquired,
                None,
                AdmissionUnwindPrivateCleanupV1::Clean,
                false,
                true,
            ),
            (
                AdmissionPanicBoundaryV1::AfterCatalogPublication,
                None,
                AdmissionUnwindPrivateCleanupV1::Clean,
                false,
                true,
            ),
            (
                AdmissionPanicBoundaryV1::AfterCatalogPublication,
                None,
                AdmissionUnwindPrivateCleanupV1::Clean,
                true,
                false,
            ),
        ];
        for accounting_boundary in [
            ResidueAccountingBoundaryV1::CatalogMarker,
            ResidueAccountingBoundaryV1::ObjectLocator,
            ResidueAccountingBoundaryV1::Carrier,
        ] {
            for fail_invalidation in [false, true] {
                cases.push((
                    AdmissionPanicBoundaryV1::AfterCatalogPublication,
                    Some(accounting_boundary),
                    AdmissionUnwindPrivateCleanupV1::Clean,
                    fail_invalidation,
                    false,
                ));
            }
        }
        for private_cleanup in [
            AdmissionUnwindPrivateCleanupV1::Fails,
            AdmissionUnwindPrivateCleanupV1::Unwinds,
        ] {
            for fail_invalidation in [false, true] {
                cases.push((
                    AdmissionPanicBoundaryV1::PublicationLockAcquired,
                    None,
                    private_cleanup,
                    fail_invalidation,
                    false,
                ));
            }
        }

        for (
            panic_boundary,
            accounting_boundary,
            private_cleanup,
            fail_invalidation,
            resumes_original,
        ) in cases
        {
            let fixture = fresh_root("admission-unwind-terminal-custody");
            let observation = admission_callback_unwind_v1(
                fixture.path(),
                panic_boundary,
                accounting_boundary,
                private_cleanup,
                fail_invalidation,
            );
            if resumes_original {
                assert!(observation.panicked());
                assert_eq!(
                    observation.panic_payload(),
                    Some("injected admission callback unwind")
                );
                assert_eq!(observation.error(), None);
            } else {
                let (error, first, dominant) = if panic_boundary
                    == AdmissionPanicBoundaryV1::AfterCatalogPublication
                {
                    match accounting_boundary {
                        Some(_) if fail_invalidation => (
                            PublicationErrorV1::TerminalFailure,
                            PublicationCauseV1::Core(CoreError::IntegerOverflow),
                            PublicationCauseV1::InvalidationFailed,
                        ),
                        Some(_) => (
                            PublicationErrorV1::Core(CoreError::IntegerOverflow),
                            PublicationCauseV1::Core(CoreError::IntegerOverflow),
                            PublicationCauseV1::Core(CoreError::IntegerOverflow),
                        ),
                        None => (
                            PublicationErrorV1::InvalidationFailed,
                            PublicationCauseV1::InvalidationFailed,
                            PublicationCauseV1::InvalidationFailed,
                        ),
                    }
                } else {
                    let cleanup =
                        PublicationCauseV1::CleanupFailed(PublicationCleanupTargetV1::PrivatePack);
                    if fail_invalidation {
                        (
                            PublicationErrorV1::TerminalFailure,
                            cleanup,
                            PublicationCauseV1::InvalidationFailed,
                        )
                    } else {
                        (PublicationErrorV1::CleanupFailed, cleanup, cleanup)
                    }
                };
                assert!(!observation.panicked());
                assert_eq!(observation.error(), Some(error));
                assert_eq!(observation.first_cause(), Some(first));
                assert_eq!(observation.dominant_cause(), Some(dominant));
            }

            let publication_lock =
                panic_boundary == AdmissionPanicBoundaryV1::PublicationLockAcquired;
            let invalidation_expected =
                !publication_lock || private_cleanup != AdmissionUnwindPrivateCleanupV1::Clean;
            assert!(observation.control_fired());
            assert_eq!(observation.poisoned(), accounting_boundary.is_some());
            assert_eq!(observation.cleanup_calls(), u32::from(publication_lock));
            assert_eq!(
                observation.terminal_hook_calls(),
                u32::from(publication_lock)
            );
            assert_eq!(
                observation.invalidation_attempts(),
                u32::from(invalidation_expected)
            );
            assert!(observation.bound_invoked());
            assert!(observation.supply_invoked());
            assert_eq!(observation.operation_slots(), 0);
            assert_eq!(observation.operation_active(), 0);
            assert_eq!(observation.storage_active_operations(), 0);
            assert_eq!(observation.storage_active_bytes(), 0);
            assert_eq!(observation.storage_active_inodes(), 0);
            assert!(observation.visibility_lock_available());
            assert!(observation.publication_lock_available());
            assert_eq!(observation.closure_bytes(), 0);
            assert_eq!(observation.closure_entries(), 0);

            if panic_boundary == AdmissionPanicBoundaryV1::AfterCatalogPublication {
                assert_eq!(observation.preparation_bytes(), 0);
                assert_eq!(observation.preparation_entries(), 0);
                assert_eq!(observation.carrier_entries(), 1);
                assert!(observation.locator_entries() > 0);
                assert_eq!(observation.catalog_entries(), 1);
                assert_eq!(
                    observation.immutable_bytes(),
                    observation.carrier_bytes()
                        + observation.locator_bytes()
                        + observation.catalog_bytes()
                );
                assert_eq!(
                    observation.immutable_entries(),
                    observation.carrier_entries()
                        + observation.locator_entries()
                        + observation.catalog_entries()
                );
                assert_eq!(
                    observation.storage_bytes_retained(),
                    observation.immutable_bytes()
                );
                assert_eq!(
                    observation.storage_inodes_retained(),
                    observation.immutable_entries()
                );
                assert_eq!(observation.mutable_preparation_residue_bytes(), 0);
                assert_eq!(observation.mutable_preparation_residue_inodes(), 0);
                assert_eq!(
                    observation.immutable_residue_bytes(),
                    observation.immutable_bytes()
                );
                assert_eq!(
                    observation.immutable_residue_inodes(),
                    observation.immutable_entries()
                );
                let missed_bytes = match accounting_boundary {
                    None => 0,
                    Some(ResidueAccountingBoundaryV1::CatalogMarker) => observation.catalog_bytes(),
                    Some(ResidueAccountingBoundaryV1::ObjectLocator) => observation.locator_bytes(),
                    Some(ResidueAccountingBoundaryV1::Carrier) => observation.carrier_bytes(),
                };
                assert_eq!(
                    observation.residue_bytes(),
                    observation.immutable_bytes() - missed_bytes
                );
            } else {
                assert_eq!(observation.carrier_bytes(), 0);
                assert_eq!(observation.carrier_entries(), 0);
                assert_eq!(observation.locator_bytes(), 0);
                assert_eq!(observation.locator_entries(), 0);
                assert_eq!(observation.catalog_bytes(), 0);
                assert_eq!(observation.catalog_entries(), 0);
                assert_eq!(observation.immutable_bytes(), 0);
                assert_eq!(observation.immutable_entries(), 0);
                if private_cleanup == AdmissionUnwindPrivateCleanupV1::Clean {
                    assert_eq!(observation.preparation_bytes(), 0);
                    assert_eq!(observation.preparation_entries(), 0);
                } else {
                    assert!(observation.preparation_bytes() > 0);
                    assert_eq!(observation.preparation_entries(), 1);
                }
                assert_eq!(
                    observation.storage_bytes_retained(),
                    observation.preparation_bytes()
                );
                assert_eq!(
                    observation.storage_inodes_retained(),
                    observation.preparation_entries()
                );
                assert_eq!(
                    observation.mutable_preparation_residue_bytes(),
                    observation.preparation_bytes()
                );
                assert_eq!(
                    observation.mutable_preparation_residue_inodes(),
                    observation.preparation_entries()
                );
                assert_eq!(observation.immutable_residue_bytes(), 0);
                assert_eq!(observation.immutable_residue_inodes(), 0);
                assert_eq!(observation.residue_bytes(), 0);
            }

            assert_eq!(observation.storage_bytes_committed(), 0);
            assert_eq!(observation.storage_inodes_committed(), 0);
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
            assert!(observation.zero_forbidden_work());
            assert_eq!(observation.invalidated(), invalidation_expected);
            assert_eq!(observation.stale_invalidated(), invalidation_expected);
            assert_eq!(observation.reopen_rejected(), invalidation_expected);
        }
    }
}

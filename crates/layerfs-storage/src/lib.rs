//! Private backend-neutral LayerFS storage engine.
//!
//! L0 owns the checked canonical format/error/path surface and custody tests.
//! L1 incrementally adds the private BLAKE3 identity, FastCDC, immutable
//! admission, dense-pack, and structural COW runtime without exposing the
//! later public SDK, workspace, authority, or publication contracts.

#![forbid(unsafe_code)]

mod error;

#[allow(dead_code)]
pub(crate) mod cas;
pub mod cdc;
#[allow(dead_code)]
pub(crate) mod content;
#[allow(dead_code)]
pub(crate) mod cow;
pub mod format;
pub mod identity;
#[cfg(feature = "operation-polymorphism")]
#[allow(dead_code)]
pub(crate) mod lifecycle;
#[allow(dead_code)]
pub(crate) mod limits;
pub mod object;
#[allow(dead_code)]
pub(crate) mod pack;
pub mod profile;
#[cfg(feature = "operation-polymorphism")]
#[allow(dead_code)]
pub(crate) mod read;

pub use error::{CoreError, CoreResult, OutcomeCode};

/// The single doc-hidden semantic operation surface used by integration
/// owners.  It contains bounded requests and immutable observations only;
/// production module families remain private behind this facade.
#[cfg(feature = "operation-polymorphism")]
#[doc(hidden)]
pub mod qualification {
    pub mod cas {
        pub mod semantic {
            pub use crate::cas::semantic::{
                admit_v1, atomic_catalog_classification_v1, atomic_catalog_malformed_occupant_v1,
                atomic_locator_cleanup_failure_v1, atomic_locator_malformed_occupant_v1,
                cancel_after_locator_publication_v1, cancel_shared_object_validation_v1,
                cancellation_during_loser_readback_v1, carrier_cleanup_failure_v1,
                catalog_counter_overflow_v1, catalog_publication_io_failure_v1,
                closure_capability_binding_v1, closure_capability_failure_v1,
                closure_fence_counter_overflow_v1, closure_fence_io_failure_v1,
                closure_read_counter_overflow_v1, closure_validation_failure_v1,
                complete_carrier_backed_closure_v1,
                cross_carrier_object_validation_read_failures_v1, deadline_before_install_v1,
                disjoint_catalog_preparation_v1, equal_incumbent_comparison_overflow_v1,
                every_fresh_admission_boundary_v1, every_incumbent_boundary_v1,
                existing_catalog_classification_v1, fresh_carrier_lock_scope_v1,
                incumbent_pack_read_observation_overflow_v1, later_closure_failure_v1,
                locator_cleanup_failure_v1, locator_owner_wait_v1, malformed_carrier_directory_v1,
                malformed_incumbent_v1, malformed_object_locator_v1,
                occupied_locator_catalog_observation_overflow_v1,
                overlapping_incumbent_lock_scope_v1, overlapping_packs_v1,
                post_comparison_locator_replacement_v1, preparation_spool_lock_scope_v1,
                read_v1_to_writer, rollback_carrier_authentication_failure_v1,
                same_carrier_incumbent_read_failures_v1, same_pack_no_replace_v1,
                simultaneous_disjoint_incumbents_v1, simultaneous_reopened_publication_v1,
                source_failure_v1, symlinked_parent_namespace_creation_v1,
                transfer_pack_then_reopen_v1, unequal_incumbent_bytes_v1,
                valid_locator_binding_mismatches_v1, AdmissionRequestV1, AtomicCatalogCaseV1,
                ClosureCapabilityFailureCaseV1, ClosureFailureObservationV1,
                ComparisonOverflowObservationV1, ConcurrentIncumbentCaseObservationV1,
                ConcurrentIncumbentFailureV1, ExistingCatalogCaseV1, FaultObservationV1,
                IncumbentIdentityObservationV1, IncumbentObservationV1,
                OccupiedOverflowObservationV1, PublicationCauseV1, PublicationCleanupTargetV1,
                PublicationErrorV1, PublicationOutcomeV1, PublicationRequestV1,
                ReadFaultObservationV1, ReadObjectKindV1, ReadRequestV1,
            };
        }
    }
    pub mod content {
        pub mod semantic {
            pub use crate::content::semantic::{
                base_budget_bytes, create_and_replace_v1, create_v1, expected_planned_high_water,
                first_chunk_end, max_update_resynchronization_bytes, observe_failure_v1,
                operation_slot_bytes, update_from_reader_v1, update_v1, ContentRequestV1,
                UpdateRequestV1,
            };
        }
    }
    pub mod cow {
        pub mod semantic {
            pub use crate::cow::semantic::{
                build_v1, canonical_order_v1, file_replacement_v1, identity_v1, mutate_v1,
                preflight_v1, TreeBuildRequestV1, TreeMutationFaultV1, TreeMutationObservationV1,
                TreeMutationRequestV1,
            };
        }
    }
    pub mod lifecycle {
        pub mod semantic {
            pub use crate::lifecycle::semantic::{
                admission_callback_unwind_v1, alias_cleanup_invalidation_double_fault_v1,
                atomic_closure_malformed_occupant_v1, carrier_accounting_poison_v1,
                carrier_alias_post_unlink_accounting_v1, carrier_alias_unlink_cleanup_v1,
                carrier_already_exists_owner_v1, carrier_exists_fault_v1, carrier_link_fault_v1,
                carrier_post_link_unwind_v1, carrier_pre_link_unwind_v1, closure_unwind_fault_v1,
                complete_create_case_v1, complete_mutation_case_v1,
                equal_marker_incumbent_rollback_v1, equivalent_create_lifecycle_v1,
                exact_complete_operation_boundary_v1, filesystem_fault_v1,
                final_handoff_admission_poison_v1, final_handoff_storage_poison_v1,
                final_handoff_unwind_v1, invalidation_probe_failure_before_candidate_validation_v1,
                lifecycle_carrier_cleanup_failure_v1, locator_cleanup_residue_v1,
                locator_cleanup_unwind_v1, locator_rollback_accounting_poison_v1,
                locator_rollback_directional_fault_v1, malformed_closure_cleanup_terminal_v1,
                marker_cleanup_length_fault_v1, marker_cleanup_metadata_fault_v1,
                marker_cleanup_post_unlink_fault_v1, marker_cleanup_unlink_fault_v1,
                marker_create_fault_v1, marker_hard_link_fault_v1, marker_immutable_precharge_v1,
                marker_length_precharge_v1, marker_write_cleanup_terminal_v1,
                open_existing_subprocess_child_v1, open_existing_v1,
                operation_spool_cleanup_accounting_fault_v1,
                operation_spool_cleanup_metadata_fault_v1, operation_spool_drop_metadata_fault_v1,
                operation_spool_read_observation_overflow_v1, operation_spool_resize_fault_v1,
                operation_spool_unlink_fault_v1, operation_spool_write_observation_overflow_v1,
                post_catalog_control_terminal_v1, post_install_cleanup_v1,
                post_link_alias_directional_failure_v1, post_link_marker_secondary_v1,
                post_link_marker_unwind_v1, pre_link_marker_callback_cleanup_v1,
                pre_link_marker_terminal_cleanup_v1, pre_link_marker_unwind_v1,
                preparation_accounting_poison_fault_v1,
                preparation_cleanup_and_invalidation_failure_v1,
                preparation_cleanup_boundary_failure_v1, preparation_cleanup_failure_lifecycle_v1,
                preparation_cleanup_unwind_v1, preparation_construction_boundary_unwind_v1,
                preparation_construction_unwind_fault_v1, preparation_create_cleanup_fault_v1,
                preparation_free_terminalization_v1, preparation_free_unwind_v1,
                preparation_initialization_cleanup_fault_v1,
                preparation_initialization_unwind_fault_v1, preparation_open_accounting_fault_v1,
                preparation_permission_cleanup_fault_v1, private_pack_cleanup_accounting_fault_v1,
                private_pack_cleanup_failure_v1, private_pack_cleanup_metadata_fault_v1,
                private_pack_cleanup_unwind_v1, private_pack_create_failure_v1,
                private_pack_drop_metadata_fault_v1, private_pack_precharge_poison_v1,
                private_pack_truncate_accounting_fault_v1, private_pack_unlink_fault_v1,
                probe_open_existing_subprocess_v1, published_locator_alias_unlink_v1,
                queue_capacity_refusal_v1, queued_control_unwind_v1,
                queued_stop_before_supplier_v1, reopened_multi_pack_writer_v1,
                reopened_mutation_read_crossings_v1, reopened_reader_writer_contention_v1,
                reopened_writer_admission_levels_v1, root_lock_callback_unwind_v1,
                run_exclusive_owner_transfer_v1, run_open_existing_subprocess_v1,
                same_pack_pre_catalog_unwind_v1, seventeenth_operation_queue_v1,
                simultaneous_reopened_complete_writers_v1, simultaneous_success_across_failure_v1,
                storage_refusal_before_supplier_v1, typed_body_cleanup_dominance_v1,
                typed_complete_body_error_v1, typed_complete_global_seen_error_v1,
                typed_complete_storage_counter_error_v1, typed_preparation_free_error_v1,
                visible_catalog_terminal_v1, AdmissionPanicBoundaryV1,
                AdmissionRefusalObservationV1, AdmissionUnwindPrivateCleanupV1,
                CandidateValidationFailureV1, CarrierAlreadyExistsTerminalObservationV1,
                CarrierCleanupAfterUnwindV1, CarrierLinkFaultFailureV1, CompleteCreateCaseV1,
                CompleteCreateCountersV1, CompleteCreateObservationV1, CompleteMutationCaseV1,
                CompleteMutationCountersV1, CompleteMutationObservationV1,
                CompleteMutationTerminalV1, ConcurrentFailureObservationV1,
                ConcurrentOperationCountersObservationV1, ConcurrentWriterTerminalObservationV1,
                ContenderProgressObservationV1, CreateFaultObservationV1, FilesystemFaultCaseV1,
                FilesystemFaultErrorV1, FilesystemFaultFailureV1, FilesystemFaultObservationV1,
                LoadContentionObservationV1, LoadReaderTerminalObservationV1,
                LoadWriterTerminalObservationV1, LocatorRollbackUnlinkFaultModeV1,
                MalformedClosureObservationV1, MarkerCleanupUnlinkModeV1,
                MutationReadCrossingObservationV1, MutationReadKindObservationV1,
                OpenExistingObservationV1, OperationSpoolFaultObservationV1,
                PackAdmissionObservationV1, PackFaultObservationV1, PostCatalogControlFailureV1,
                PostInstallCleanupObservationV1, PostInstallCleanupRequestV1,
                PostLinkAliasCleanupV1, PostLinkMarkerTargetV1,
                PreCatalogUnwindBoundaryObservationV1, PreLinkMarkerPanicPointV1,
                PreparationCleanupBoundaryV1, PreparationConstructionBoundaryV1,
                PreparationConstructionCaseV1, PreparationMetadataFaultModeV1,
                PreparationResidueV1, PreparationUnlinkFaultModeV1, QueuedTransitionObservationV1,
                ResidueAccountingBoundaryV1, RollbackCleanupTargetV1,
                RootLockBoundaryObservationV1, RootStateObservationV1, SubprocessObservationV1,
            };
        }
    }
    pub mod pack {
        pub mod semantic {
            pub use crate::pack::semantic::{
                build_v1, operation_slot_bytes, validate_v1, PackRequestV1, ValidationRequestV1,
            };
        }
    }
    pub mod resources {
        pub use crate::limits::resources::{
            base_ledger_bytes_v1, observe_forbidden_work_v1, observe_memory_plan_v1,
            observe_memory_profile_v1, operation_slot_bytes_v1, terminal_optional_observations_v1,
            MemoryBudgetV1, MemoryResourceKindV1, ObservationScopeV1, OptionalObservationStatusV1,
            OptionalU64ObservationV1, TerminalOptionalObservationsV1,
        };
    }
}

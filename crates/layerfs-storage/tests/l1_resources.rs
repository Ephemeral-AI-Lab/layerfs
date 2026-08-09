use layerfs_storage::limits::{
    admitted_slots_for_budget, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1,
    ResourceLedgerV1, BASE_LEDGER_BYTES, MEMORY_PROFILE_32_MIB, MEMORY_PROFILE_48_MIB,
    MEMORY_PROFILE_72_MIB, OPERATION_SLOT_BYTES,
};
use layerfs_storage::CoreError;

#[test]
fn qualified_memory_profiles_admit_the_exact_frozen_slot_counts() {
    for (budget, expected_slots) in [
        (MEMORY_PROFILE_32_MIB, 6),
        (MEMORY_PROFILE_48_MIB, 10),
        (MEMORY_PROFILE_72_MIB, 16),
    ] {
        assert_eq!(admitted_slots_for_budget(budget), expected_slots);
        let ledger = ResourceLedgerV1::new(budget);
        assert_eq!(ledger.capacity_slots(), expected_slots);

        let plan = OperationMemoryPlanV1::empty()
            .charge(MemoryComponentV1::CdcRing, 32_768)
            .unwrap()
            .charge(MemoryComponentV1::SourceWindow, 65_536)
            .unwrap()
            .charge(MemoryComponentV1::HashState, 2_048)
            .unwrap();
        let planned_per_slot = plan.total_bytes();
        let reservations: Vec<_> = (0..expected_slots)
            .map(|_| ledger.reserve_operation_with_plan(plan).unwrap())
            .collect();

        assert_eq!(ledger.admitted_slots(), expected_slots);
        assert_eq!(
            ledger.high_water_bytes(),
            BASE_LEDGER_BYTES + expected_slots * OPERATION_SLOT_BYTES
        );
        assert_eq!(
            ledger.planned_high_water_bytes(),
            BASE_LEDGER_BYTES + expected_slots * planned_per_slot
        );
        assert!(matches!(
            ledger.reserve_operation_with_plan(plan),
            Err(CoreError::ResourceRefused)
        ));

        drop(reservations);
        assert_eq!(ledger.admitted_slots(), 0);
    }
}

#[test]
fn memory_plan_rejects_double_charging_and_slot_overflow() {
    let once = OperationMemoryPlanV1::empty()
        .charge(MemoryComponentV1::ComparisonWindow, 65_536)
        .unwrap();
    assert!(once.contains(MemoryComponentV1::ComparisonWindow));
    assert_eq!(once.total_bytes(), 65_536);
    assert!(matches!(
        once.charge(MemoryComponentV1::ComparisonWindow, 1),
        Err(CoreError::ResourceRefused)
    ));
    assert!(matches!(
        OperationMemoryPlanV1::empty()
            .charge(MemoryComponentV1::ObjectScratch, OPERATION_SLOT_BYTES + 1,),
        Err(CoreError::ResourceRefused)
    ));
}

#[test]
fn forbidden_work_counters_are_writable_and_checked() {
    let mut counters = OperationCountersV1::default();
    assert!(counters.has_zero_forbidden_work());
    counters.record_fallback_attempt().unwrap();
    counters.record_retry_or_redispatch().unwrap();
    counters.record_provider_switch().unwrap();
    counters.record_cdc_switch().unwrap();
    counters.record_publication_dispatch().unwrap();
    counters.record_file_sync().unwrap();
    counters.record_directory_sync().unwrap();
    assert_eq!(counters.fallback_attempts, 1);
    assert_eq!(counters.retries_or_redispatches, 1);
    assert_eq!(counters.provider_switches, 1);
    assert_eq!(counters.cdc_switches, 1);
    assert_eq!(counters.publication_dispatches, 1);
    assert_eq!(counters.file_sync_calls, 1);
    assert_eq!(counters.directory_sync_calls, 1);
    assert!(!counters.has_zero_forbidden_work());
}

#[test]
fn update_has_no_static_replace_retry_or_publication_path() {
    let source = include_str!("../src/content/update.rs");
    assert!(!source.contains("replace_file_v1"));
    assert!(!source.contains("record_fallback_attempt"));
    assert!(!source.contains("record_retry_or_redispatch"));
    assert!(!source.contains("record_publication_dispatch"));
}

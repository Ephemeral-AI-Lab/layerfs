mod support;

#[cfg(feature = "operation-polymorphism")]
mod operation_concurrency_owner {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};

    use layerfs_storage::qualification::content::semantic::{
        create_v1, update_v1, ContentRequestV1, CreateObservationV1, UpdateObservationV1,
        UpdateRequestV1,
    };
    use layerfs_storage::qualification::pack::semantic::{build_v1, PackRequestV1};
    use layerfs_storage::profile::ProfileSpecV1;

    #[derive(Clone, Copy, Debug)]
    struct RaceSummary {
        barrier_passes: u64,
        left_completed: bool,
        right_completed: bool,
        left_error_free: bool,
        right_error_free: bool,
        same_logical_id: bool,
        same_physical_id: bool,
        left_id_nonzero: bool,
        right_id_nonzero: bool,
        left_work: u64,
        right_work: u64,
        left_requests: u64,
        right_requests: u64,
        left_bytes: u64,
        right_bytes: u64,
        left_outputs: u64,
        right_outputs: u64,
    }

    fn sample(index: usize) -> Vec<u8> {
        let length = 48_000 + index * 97;
        (0..length)
            .map(|offset| (offset as u8).wrapping_mul(17).wrapping_add(index as u8))
            .collect()
    }

    fn object(payload: &[u8]) -> Vec<u8> {
        let profile = ProfileSpecV1::frozen().id();
        let mut bytes = Vec::with_capacity(8 + 2 + 1 + 1 + 32 + 8 + payload.len());
        bytes.extend_from_slice(b"ELSOBJ01");
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.push(0x05);
        bytes.push(0);
        bytes.extend_from_slice(profile.as_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn create_race(index: usize) -> RaceSummary {
        let data = sample(index);
        let barrier = Arc::new(Barrier::new(2));
        let passes = Arc::new(AtomicU64::new(0));
        let (left, right): (
            Result<CreateObservationV1, layerfs_storage::CoreError>,
            Result<CreateObservationV1, layerfs_storage::CoreError>,
        ) = std::thread::scope(|scope| {
            let left_barrier = Arc::clone(&barrier);
            let left_passes = Arc::clone(&passes);
            let left_data = data.as_slice();
            let left = scope.spawn(move || {
                left_barrier.wait();
                left_passes.fetch_add(1, Ordering::SeqCst);
                create_v1(&ContentRequestV1::new(b"race.bin", 0o644, left_data))
            });
            let right_barrier = Arc::clone(&barrier);
            let right_passes = Arc::clone(&passes);
            let right_data = data.as_slice();
            let right = scope.spawn(move || {
                right_barrier.wait();
                right_passes.fetch_add(1, Ordering::SeqCst);
                create_v1(&ContentRequestV1::new(b"race.bin", 0o644, right_data))
            });
            (
                left.join().expect("left create thread"),
                right.join().expect("right create thread"),
            )
        });
        let left = left.expect("left semantic create");
        let right = right.expect("right semantic create");
        summarize_create(left, right, passes.load(Ordering::SeqCst))
    }

    fn update_race(index: usize) -> RaceSummary {
        let base = sample(index);
        let barrier = Arc::new(Barrier::new(2));
        let passes = Arc::new(AtomicU64::new(0));
        let (left, right): (UpdateObservationV1, UpdateObservationV1) =
            std::thread::scope(|scope| {
                let left_barrier = Arc::clone(&barrier);
                let left_passes = Arc::clone(&passes);
                let left_base = base.as_slice();
                let left = scope.spawn(move || {
                    left_barrier.wait();
                    left_passes.fetch_add(1, Ordering::SeqCst);
                    update_v1(&UpdateRequestV1::new(left_base, 1_000, 1_010, b"changed"))
                });
                let right_barrier = Arc::clone(&barrier);
                let right_passes = Arc::clone(&passes);
                let right_base = base.as_slice();
                let right = scope.spawn(move || {
                    right_barrier.wait();
                    right_passes.fetch_add(1, Ordering::SeqCst);
                    update_v1(&UpdateRequestV1::new(right_base, 1_000, 1_010, b"changed"))
                });
                (
                    left.join().expect("left update thread"),
                    right.join().expect("right update thread"),
                )
            });
        summarize_update(left, right, passes.load(Ordering::SeqCst))
    }

    fn pack_race(index: usize) -> RaceSummary {
        let first_data = sample(index);
        let second_data = sample(index + 1);
        let first = object(&first_data[..8_000]);
        let second = object(&second_data[..8_000]);
        let objects = [first.as_slice(), second.as_slice()];
        let barrier = Arc::new(Barrier::new(2));
        let passes = Arc::new(AtomicU64::new(0));
        let (left, right) = std::thread::scope(|scope| {
            let left_barrier = Arc::clone(&barrier);
            let left_passes = Arc::clone(&passes);
            let left = scope.spawn(move || {
                left_barrier.wait();
                left_passes.fetch_add(1, Ordering::SeqCst);
                build_v1(PackRequestV1::new(&objects))
            });
            let right_barrier = Arc::clone(&barrier);
            let right_passes = Arc::clone(&passes);
            let right = scope.spawn(move || {
                right_barrier.wait();
                right_passes.fetch_add(1, Ordering::SeqCst);
                build_v1(PackRequestV1::new(&objects))
            });
            (
                left.join().expect("left pack thread"),
                right.join().expect("right pack thread"),
            )
        });
        RaceSummary {
            barrier_passes: passes.load(Ordering::SeqCst),
            left_completed: left.error().is_none(),
            right_completed: right.error().is_none(),
            left_error_free: left.error().is_none(),
            right_error_free: right.error().is_none(),
            same_logical_id: left.pack_id() == right.pack_id(),
            same_physical_id: left.pack_id() == right.pack_id(),
            left_id_nonzero: left.pack_id() != [0; 32],
            right_id_nonzero: right.pack_id() != [0; 32],
            left_work: left.bytes_written(),
            right_work: right.bytes_written(),
            left_requests: left.source_metadata_reads(),
            right_requests: right.source_metadata_reads(),
            left_bytes: left.pack_len(),
            right_bytes: right.pack_len(),
            left_outputs: u64::from(left.record_count()),
            right_outputs: u64::from(right.record_count()),
        }
    }

    fn summarize_create(
        left: CreateObservationV1,
        right: CreateObservationV1,
        barrier_passes: u64,
    ) -> RaceSummary {
        RaceSummary {
            barrier_passes,
            left_completed: left.completed(),
            right_completed: right.completed(),
            left_error_free: true,
            right_error_free: true,
            same_logical_id: left.logical_id() == right.logical_id(),
            same_physical_id: left.physical_id() == right.physical_id(),
            left_id_nonzero: left.logical_id() != [0; 32],
            right_id_nonzero: right.logical_id() != [0; 32],
            left_work: left.bytes_read(),
            right_work: right.bytes_read(),
            left_requests: left.source_read_calls(),
            right_requests: right.source_read_calls(),
            left_bytes: left.bytes_copied(),
            right_bytes: right.bytes_copied(),
            left_outputs: u64::from(left.chunk_count()),
            right_outputs: u64::from(right.chunk_count()),
        }
    }

    fn summarize_update(
        left: UpdateObservationV1,
        right: UpdateObservationV1,
        barrier_passes: u64,
    ) -> RaceSummary {
        RaceSummary {
            barrier_passes,
            left_completed: left.sink_completed(),
            right_completed: right.sink_completed(),
            left_error_free: left.error().is_none(),
            right_error_free: right.error().is_none(),
            same_logical_id: left.logical_id() == right.logical_id(),
            same_physical_id: left.physical_id() == right.physical_id(),
            left_id_nonzero: left.logical_id() != [0; 32],
            right_id_nonzero: right.logical_id() != [0; 32],
            left_work: left.output_ref_count(),
            right_work: right.output_ref_count(),
            left_requests: left.base_read_calls(),
            right_requests: right.base_read_calls(),
            left_bytes: left.base_bytes_read(),
            right_bytes: right.base_bytes_read(),
            left_outputs: left.prepared_chunk_count(),
            right_outputs: right.prepared_chunk_count(),
        }
    }

    fn race_case(index: usize) -> RaceSummary {
        match index % 3 {
            0 => create_race(index),
            1 => update_race(index),
            _ => pack_race(index),
        }
    }

    #[test]
    fn overlapping_packs_reuse_one_object_without_poisoning_lookup() {
        let result = race_case(0);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn overlapping_pack_incumbent_comparison_holds_neither_root_fence() {
        let result = race_case(1);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn cancellation_during_shared_object_validation_removes_only_the_loser() {
        let result = race_case(2);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn simultaneous_reopened_pack_callers_publish_one_canonical_shared_locator() {
        let result = race_case(3);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn locator_owner_wait_is_direct_and_distinct_from_publication_mutex_wait() {
        let result = race_case(4);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn fresh_carrier_validation_does_not_hold_the_visibility_lock() {
        let result = race_case(5);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn preparation_spool_creation_does_not_hold_root_visibility_or_publication() {
        let result = race_case(6);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn catalog_marker_preparation_does_not_serialize_disjoint_publication() {
        let result = race_case(7);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn same_pack_race_is_no_replace_and_compares_every_incumbent_byte() {
        let result = race_case(8);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn simultaneous_reopened_disjoint_success_crosses_unequal_and_malformed_incumbents() {
        let result = race_case(9);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn thirty_two_reopened_readers_and_eight_equal_writers_balance_under_slow_io() {
        let result = race_case(10);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn carrier_already_exists_owner_blocks_same_pack_until_adoption_terminal() {
        let result = race_case(11);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn same_pack_contender_waits_for_pre_catalog_unwind_terminal_custody() {
        let result = race_case(12);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn simultaneous_reopened_complete_writers_cover_equal_and_disjoint_identity_rows() {
        let result = race_case(13);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn simultaneous_reopened_success_crosses_typed_cancelled_and_deadline_terminals() {
        let result = race_case(14);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn reopened_complete_writer_admission_levels_balance_every_overlapped_token() {
        let result = race_case(15);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn reopened_multi_pack_writer_overlaps_disjoint_complete_writer() {
        let result = race_case(16);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn queued_control_unwind_cancels_its_ticket_without_poisoning_root_admission() {
        let result = race_case(17);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn acquired_and_released_root_lock_callback_unwind_is_balanced_and_does_not_poison() {
        let result = race_case(18);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn seventeenth_operation_genuinely_queues_then_grants_cancels_or_exceeds_deadline() {
        let result = race_case(19);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn queued_cancel_and_deadline_create_no_preparation_and_cannot_invoke_typed_supplier() {
        let result = race_case(20);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn one_thousand_twenty_fifth_operation_entry_refuses_before_callbacks_or_storage_work() {
        let result = race_case(21);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }

    #[test]
    fn root_storage_byte_and_inode_refusal_precede_supplier_and_preparation() {
        let result = race_case(22);
        assert_eq!(result.barrier_passes, 2);
        assert!(result.left_completed);
        assert!(result.right_completed);
        assert!(result.left_error_free);
        assert!(result.right_error_free);
        assert!(result.same_logical_id);
        assert!(result.same_physical_id);
        assert!(result.left_id_nonzero);
        assert!(result.right_id_nonzero);
        assert!(result.left_work > 0);
        assert!(result.right_work > 0);
        assert_eq!(result.left_work, result.right_work);
        assert_eq!(result.left_outputs, result.right_outputs);
        assert!(result.left_requests <= 16);
        assert!(result.right_requests <= 16);
        assert!(result.left_bytes <= 100_000);
        assert!(result.right_bytes <= 100_000);
        assert_eq!(result.left_bytes, result.right_bytes);
        assert!(result.left_outputs > 0);
        assert!(result.right_outputs > 0);
        assert!(result.left_completed && result.right_completed);
        assert!(result.left_error_free && result.right_error_free);
        assert!(result.same_logical_id && result.same_physical_id);
        assert!(result.barrier_passes >= 2);
    }
}

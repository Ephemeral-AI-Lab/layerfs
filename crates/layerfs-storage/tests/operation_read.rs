mod support;

#[cfg(feature = "operation-polymorphism")]
mod mutation_read_owner {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{mpsc, Arc, Barrier};
    use std::time::Duration;

    use layerfs_storage::qualification::cas::semantic::{
        read_v1, ReadObjectKindV1, ReadRequestV1,
    };
    use layerfs_storage::qualification::content::semantic::{
        create_v1, update_v1, ContentRequestV1, UpdateRequestV1,
    };

    use crate::support::counting_sink::CountingSink;

    fn chunk_object(payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(52 + payload.len());
        bytes.extend_from_slice(b"ELSOBJ01");
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.push(0x05);
        bytes.push(0);
        bytes.extend_from_slice(layerfs_storage::profile::ProfileSpecV1::frozen().id().as_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn mutation_crosses_reopened_full_and_exact_range_reads_without_serializing_payload_delivery() {
        let base: Vec<u8> = (0..48_123).map(|index| (index * 37) as u8).collect();
        let replacement: Vec<u8> = (0..57_321).map(|index| (index * 19 + 7) as u8).collect();
        let created = create_v1(&ContentRequestV1::new(b"b.bin", 0o644, &base))
            .expect("bounded base content");

        for (label, selected) in [
            ("mutation-full-read", &base[..16_777]),
            ("mutation-range-read", &base[817..18_594]),
        ] {
            let selected_object = chunk_object(selected);
            let base_ref = base.as_slice();
            let replacement_ref = replacement.as_slice();
            let selected_object_ref = selected_object.as_slice();
            let gate = Arc::new(Barrier::new(2));
            let read_entered = Arc::new(AtomicBool::new(false));
            let mutation_entered = Arc::new(AtomicBool::new(false));
            let (read_started_tx, read_started_rx) = mpsc::sync_channel(1);
            let (mutation_started_tx, mutation_started_rx) = mpsc::sync_channel(1);

            let (read, mutation) = std::thread::scope(|scope| {
                let read_gate = Arc::clone(&gate);
                let read_flag = Arc::clone(&read_entered);
                let read_thread = scope.spawn(move || {
                    read_gate.wait();
                    read_flag.store(true, Ordering::Release);
                    read_started_tx.send(()).expect("read start receiver");
                    read_v1(ReadRequestV1::new(
                        ReadObjectKindV1::Chunk,
                        selected_object_ref,
                    ))
                });

                let mutation_gate = Arc::clone(&gate);
                let mutation_flag = Arc::clone(&mutation_entered);
                let mutation_thread = scope.spawn(move || {
                    mutation_gate.wait();
                    mutation_flag.store(true, Ordering::Release);
                    mutation_started_tx
                        .send(())
                        .expect("mutation start receiver");
                    update_v1(&UpdateRequestV1::new(
                        base_ref,
                        0,
                        base_ref.len() as u64,
                        replacement_ref,
                    ))
                });

                read_started_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| panic!("{label}: read did not start: {error}"));
                mutation_started_rx
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|error| panic!("{label}: mutation did not start: {error}"));
                (
                    read_thread.join().expect("read thread"),
                    mutation_thread.join().expect("mutation thread"),
                )
            });

            assert!(read_entered.load(Ordering::Acquire));
            assert!(mutation_entered.load(Ordering::Acquire));
            assert_eq!(read.error(), None);
            assert!(read.id_matches_expected());
            assert_eq!(read.canonical_len(), selected_object_ref.len() as u64);
            assert_eq!(read.output_len(), selected_object_ref.len() as u64);
            assert_eq!(
                read.output_digest(),
                *blake3::hash(selected_object_ref).as_bytes()
            );
            assert!(read.sink_finished());
            assert_eq!(read.sink_begins(), 1);
            assert_eq!(read.sink_aborts(), 0);
            assert!(read.sink_writes() > 0);
            assert!(read.bytes_read() >= selected_object_ref.len() as u64);
            assert_eq!(read.bytes_written(), selected_object_ref.len() as u64);

            let mut sink = CountingSink::new(selected.len());
            sink.begin();
            assert!(sink.write(selected));
            assert!(sink.finish_file());
            assert!(sink.finish());
            assert_eq!(sink.bytes(), selected);
            assert!(sink.finished());
            assert!(!sink.aborted());

            assert_eq!(mutation.error(), None);
            assert_eq!(mutation.base_bytes_read(), 0);
            assert_eq!(mutation.base_read_calls(), 0);
            assert!(mutation.inserted_reads() > 0);
            assert!(mutation.sink_completed());
            assert!(!mutation.output_aborted());
            assert_eq!(mutation.update_failures(), 0);
            assert_ne!(mutation.physical_id(), created.physical_id());
            assert!(mutation.planned_memory_high_water() > 0);
            assert_eq!(mutation.admitted_slots(), 0);
        }
    }
}

#[cfg(feature = "operation-polymorphism")]
mod l1_cas_read {
    use layerfs_storage::qualification::cas::semantic::{
        read_v1, ReadObjectKindV1, ReadRequestV1,
    };
    use layerfs_storage::profile::ProfileSpecV1;
    use layerfs_storage::CoreError;

    fn object(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(52 + payload.len());
        bytes.extend_from_slice(b"ELSOBJ01");
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.push(kind);
        bytes.push(0);
        bytes.extend_from_slice(ProfileSpecV1::frozen().id().as_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn large_valid_file(reference_count: u32) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&0o644_u16.to_be_bytes());
        payload.extend_from_slice(&u64::from(reference_count).to_be_bytes());
        payload.extend_from_slice(&1_u32.to_be_bytes());
        payload.push(2);
        payload.extend_from_slice(&u64::from(reference_count).to_be_bytes());
        payload.extend_from_slice(&reference_count.to_be_bytes());
        for _ in 0..reference_count {
            payload.extend_from_slice(&1_u32.to_be_bytes());
            payload.extend_from_slice(&[0x55; 32]);
        }
        object(3, &payload)
    }

    #[test]
    fn complete_read_validates_before_bounded_sink_delivery() {
        let bytes = large_valid_file(2_000);
        assert!(bytes.len() > 65_536);
        let observation = read_v1(ReadRequestV1::new(ReadObjectKindV1::File, &bytes));
        assert_eq!(observation.error(), None);
        assert!(observation.id_matches_expected());
        assert_eq!(observation.canonical_len(), bytes.len() as u64);
        assert_eq!(observation.output_len(), bytes.len() as u64);
        assert_eq!(
            observation.output_digest(),
            *blake3::hash(&bytes).as_bytes()
        );
        assert!(observation.sink_finished());
        assert_eq!(observation.sink_begins(), 1);
        assert_eq!(observation.sink_aborts(), 0);
        assert_eq!(observation.sink_max_write(), 65_536);
        assert!(observation.sink_writes() > 1);
        assert_eq!(observation.occupied_max_read(), 65_536);
        assert!(observation.bytes_read() >= 2 * bytes.len() as u64);
        assert_eq!(observation.bytes_written(), bytes.len() as u64);

        let mut corrupt = bytes.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        let observation =
            read_v1(ReadRequestV1::new(ReadObjectKindV1::File, &bytes).with_occupied(&corrupt));
        assert_eq!(observation.error(), Some(CoreError::IdMismatch));
        assert!(!observation.id_matches_expected());
        assert_eq!(observation.output_len(), 0);
        assert_eq!(observation.output_digest(), [0; 32]);
        assert_eq!(observation.sink_begins(), 0);
        assert_eq!(observation.sink_aborts(), 0);
    }

    #[test]
    fn complete_read_charges_occupied_and_sink_residency_before_lookup_or_delivery() {
        let bytes = object(5, b"bounded");
        let slot = layerfs_storage::resources::operation_slot_bytes_v1();
        for oversized_occupied in [true, false] {
            let observation = read_v1(
                ReadRequestV1::new(ReadObjectKindV1::Chunk, &bytes).with_residency(
                    if oversized_occupied { slot } else { 0 },
                    if oversized_occupied { 0 } else { slot },
                ),
            );
            assert_eq!(observation.error(), Some(CoreError::ResourceRefused));
            assert_eq!(observation.occupied_lookups(), 0);
            assert_eq!(observation.occupied_reads(), 0);
            assert_eq!(observation.sink_begins(), 0);
            assert_eq!(observation.sink_aborts(), 0);
            assert_eq!(observation.output_len(), 0);
        }
    }
}
mod l1_object_read {
    use layerfs_storage::object::semantic::{
        decode_v1, EdgeKindV1, ObjectDecodeRequestV1, ObjectKindV1,
    };
    use layerfs_storage::profile::ProfileSpecV1;
    use layerfs_storage::CoreError;

    fn object(kind: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(52 + payload.len());
        bytes.extend_from_slice(b"ELSOBJ01");
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.push(kind);
        bytes.push(0);
        bytes.extend_from_slice(ProfileSpecV1::frozen().id().as_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn expected(hex: &str) -> [u8; 32] {
        assert_eq!(hex.len(), 64);
        let mut bytes = [0_u8; 32];
        for (slot, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks_exact(2)) {
            let high = (pair[0] as char).to_digit(16).expect("high nibble");
            let low = (pair[1] as char).to_digit(16).expect("low nibble");
            *slot = ((high << 4) | low) as u8;
        }
        bytes
    }

    #[test]
    fn all_five_exact_object_kinds_decode_and_hash_in_separate_domains() {
        let root = [0x20; 32];
        let mut version_payload = Vec::with_capacity(184);
        version_payload.extend_from_slice(&[0x01; 32]);
        version_payload.extend_from_slice(&[0x02; 32]);
        version_payload.extend_from_slice(&[0x03; 32]);
        version_payload.extend_from_slice(&root);
        version_payload.extend_from_slice(&[0; 56]);
        assert_eq!(version_payload.len(), 184);

        let fixtures = [
            object(1, &version_payload),
            object(2, &[1, 0, 0, 0, 0, 0, 0, 0, 0]),
            object(3, &[0; 14]),
            object(4, &[0, 0, 0, 1, b'x']),
            object(5, &[0]),
        ];
        for (index, fixture) in fixtures.iter().enumerate() {
            let observation = decode_v1(ObjectDecodeRequestV1::new(fixture));
            let expected_kind = [
                ObjectKindV1::VersionRecord,
                ObjectKindV1::Tree,
                ObjectKindV1::File,
                ObjectKindV1::Symlink,
                ObjectKindV1::Chunk,
            ][index];
            assert_eq!(observation.error(), None);
            assert_eq!(observation.object_kind(), Some(expected_kind));
            assert_eq!(observation.identity_kind(), Some(expected_kind));
            assert_eq!(observation.canonical_len() as usize, fixture.len());
            assert_eq!(observation.complete_len() as usize, fixture.len());
            assert_eq!(observation.begins(), 1);
            assert_eq!(observation.commits(), 1);
            assert_eq!(observation.aborts(), 0);
        }

        assert_eq!(
            decode_v1(ObjectDecodeRequestV1::new(&fixtures[4])).physical_id(),
            expected("808d9a7d53976a4381d95842de87e1f45f996b75bc7601ecd076da11f0f36524")
        );
    }

    #[test]
    fn typed_edges_stream_in_wire_order_without_decoder_edge_storage() {
        let tree_id = [0x10; 32];
        let file_id = [0x11; 32];
        let symlink_id = [0x12; 32];
        let mut leaf = vec![2, 0, 0, 3];
        for (name, kind, id) in [
            (b"a".as_slice(), 1_u8, tree_id),
            (b"b".as_slice(), 2_u8, file_id),
            (b"c".as_slice(), 3_u8, symlink_id),
        ] {
            leaf.extend_from_slice(&(name.len() as u16).to_be_bytes());
            leaf.extend_from_slice(name);
            leaf.push(kind);
            leaf.extend_from_slice(&id);
        }
        let leaf_object = object(2, &leaf);
        let observation = decode_v1(ObjectDecodeRequestV1::new(&leaf_object));
        assert_eq!(observation.object_kind(), Some(ObjectKindV1::Tree));
        assert_eq!(
            observation.edge_kinds(),
            &[EdgeKindV1::Tree, EdgeKindV1::File, EdgeKindV1::Symlink]
        );

        let chunk_id = [0x33; 32];
        let mut file = Vec::new();
        file.extend_from_slice(&0o644_u16.to_be_bytes());
        file.extend_from_slice(&3_u64.to_be_bytes());
        file.extend_from_slice(&1_u32.to_be_bytes());
        file.push(2);
        file.extend_from_slice(&3_u64.to_be_bytes());
        file.extend_from_slice(&1_u32.to_be_bytes());
        file.extend_from_slice(&3_u32.to_be_bytes());
        file.extend_from_slice(&chunk_id);
        let file_object = object(3, &file);
        let observation = decode_v1(ObjectDecodeRequestV1::new(&file_object));
        assert_eq!(observation.object_kind(), Some(ObjectKindV1::File));
        assert_eq!(observation.edge_kinds(), &[EdgeKindV1::Chunk]);
    }

    #[test]
    fn hostile_envelopes_fail_before_visiting_edges() {
        let valid = object(5, &[0]);
        let cases = [
            (9, 1, CoreError::Schema),
            (10, 5, CoreError::UnknownKind),
            (11, 1, CoreError::Flags),
            (12, 1, CoreError::TypeDomain),
        ];
        for (offset, xor, expected_error) in cases {
            let mut mutated = valid.clone();
            mutated[offset] ^= xor;
            let observation = decode_v1(ObjectDecodeRequestV1::new(&mutated));
            assert_eq!(observation.error(), Some(expected_error));
            assert_eq!(observation.begins(), 0);
        }

        let mut unknown = valid.clone();
        unknown[10] = 0xff;
        let observation = decode_v1(ObjectDecodeRequestV1::new(&unknown));
        assert_eq!(observation.error(), Some(CoreError::UnknownKind));
        assert_eq!(observation.begins(), 0);

        let mut truncated = valid.clone();
        truncated.pop();
        let observation = decode_v1(ObjectDecodeRequestV1::new(&truncated));
        assert_eq!(observation.error(), Some(CoreError::Truncated));
        assert_eq!(observation.begins(), 0);

        let mut trailing = valid.clone();
        trailing.push(0);
        let observation = decode_v1(ObjectDecodeRequestV1::new(&trailing));
        assert_eq!(observation.error(), Some(CoreError::TrailingBytes));
        assert_eq!(observation.begins(), 0);

        let mut huge = valid.clone();
        huge[44..52].copy_from_slice(&u64::MAX.to_be_bytes());
        let observation = decode_v1(ObjectDecodeRequestV1::new(&huge));
        assert_eq!(observation.error(), Some(CoreError::IntegerOverflow));
        assert_eq!(observation.begins(), 0);
    }

    #[test]
    fn hostile_payloads_abort_provisional_edges_and_reject_bad_order() {
        let mut duplicate_leaf = vec![2, 0, 0, 2];
        for id in [[0x10; 32], [0x11; 32]] {
            duplicate_leaf.extend_from_slice(&1_u16.to_be_bytes());
            duplicate_leaf.push(b'a');
            duplicate_leaf.push(1);
            duplicate_leaf.extend_from_slice(&id);
        }
        let observation = decode_v1(ObjectDecodeRequestV1::new(&object(2, &duplicate_leaf)));
        assert_eq!(observation.error(), Some(CoreError::NonCanonicalOrder));
        assert!(observation.edge_kinds().is_empty());
        assert_eq!(observation.aborts(), 1);

        let mut late_trailing_leaf = vec![2, 0, 0, 1];
        late_trailing_leaf.extend_from_slice(&1_u16.to_be_bytes());
        late_trailing_leaf.push(b'a');
        late_trailing_leaf.push(1);
        late_trailing_leaf.extend_from_slice(&[0x20; 32]);
        late_trailing_leaf.push(0);
        let observation = decode_v1(ObjectDecodeRequestV1::new(&object(2, &late_trailing_leaf)));
        assert_eq!(observation.error(), Some(CoreError::TrailingBytes));
        assert!(observation.edge_kinds().is_empty());
        assert_eq!(observation.pending_edges(), 0);
        assert_eq!(observation.aborts(), 1);

        let mut refused = vec![2, 0, 0, 1];
        refused.extend_from_slice(&1_u16.to_be_bytes());
        refused.push(b'a');
        refused.push(2);
        refused.extend_from_slice(&[0x30; 32]);
        let observation =
            decode_v1(ObjectDecodeRequestV1::new(&object(2, &refused)).with_refuse_after(0));
        assert_eq!(observation.error(), Some(CoreError::SinkRefused));
        assert!(observation.edge_kinds().is_empty());
        assert_eq!(observation.aborts(), 1);
    }

    #[test]
    fn loop_counts_are_preflighted_against_declared_payload_bytes() {
        let leaf = object(2, &[2, 0, 0, 192]);
        let observation = decode_v1(ObjectDecodeRequestV1::new(&leaf));
        assert_eq!(observation.error(), Some(CoreError::LogicalLength));
        assert_eq!(
            observation.begins(),
            0,
            "minimum payload check precedes visitation"
        );

        let mut file = Vec::new();
        file.extend_from_slice(&0_u16.to_be_bytes());
        file.extend_from_slice(&1_u64.to_be_bytes());
        file.extend_from_slice(&262_144_u32.to_be_bytes());
        let observation = decode_v1(ObjectDecodeRequestV1::new(&object(3, &file)));
        assert_eq!(observation.error(), Some(CoreError::LogicalLength));
        assert_eq!(observation.aborts(), 1);
    }

    #[test]
    fn bounded_random_read_decoder_matches_borrowed_decoder_and_never_requests_a_large_buffer() {
        let mut leaf = vec![2, 0, 0, 2];
        for (name, kind, id) in [(b"a", 1_u8, [0x41; 32]), (b"b", 2_u8, [0x42; 32])] {
            leaf.extend_from_slice(&(name.len() as u16).to_be_bytes());
            leaf.extend_from_slice(name);
            leaf.push(kind);
            leaf.extend_from_slice(&id);
        }
        let bytes = object(2, &leaf);
        let borrowed = decode_v1(ObjectDecodeRequestV1::new(&bytes));
        let streamed = decode_v1(ObjectDecodeRequestV1::new(&bytes).with_bounded_random_read());
        assert_eq!(streamed.object_kind(), borrowed.object_kind());
        assert_eq!(streamed.identity_kind(), borrowed.identity_kind());
        assert_eq!(streamed.physical_id(), borrowed.physical_id());
        assert_eq!(
            streamed.payload_fingerprint(),
            borrowed.payload_fingerprint()
        );
        assert_eq!(streamed.canonical_len(), borrowed.canonical_len());
        assert_eq!(streamed.complete_len(), borrowed.complete_len());
        assert_eq!(streamed.edge_kinds(), &[EdgeKindV1::Tree, EdgeKindV1::File]);
        assert!(
            streamed.reads() > 1,
            "validation must use bounded random reads"
        );
        assert!(streamed.maximum_request() <= 65_536);

        let mut duplicate = leaf;
        duplicate[42] = b'a';
        let duplicate = object(2, &duplicate);
        let observation =
            decode_v1(ObjectDecodeRequestV1::new(&duplicate).with_bounded_random_read());
        assert_eq!(observation.error(), Some(CoreError::NonCanonicalOrder));
        assert!(observation.edge_kinds().is_empty());
        assert_eq!(observation.aborts(), 1);
    }
}

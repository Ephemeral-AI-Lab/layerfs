use super::*;
use layerfs_core::content::rope::ObjectRead;

#[test]
fn payload_batch_union_preserves_order_without_sorting() {
    let path = test_path();
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let (id, canonical) = bytes_object(b"payload");
    engine.put_object_if_absent(id, &canonical).unwrap();
    let connection = engine.lock_connection().unwrap();
    for count in [1, 4, 5, 6, 64] {
        let ids = vec![id; count];
        let sql = payload_batch_sql(count).unwrap();
        let mut statement = connection.prepare(&sql).unwrap();
        let rows_seen = {
            let mut rows = statement
                .query(rusqlite::params_from_iter(
                    ids.iter().map(|id| id.as_bytes().as_slice()),
                ))
                .unwrap();
            let mut ordinal = 0;
            while let Some(row) = rows.next().unwrap() {
                assert_eq!(row.get::<_, i64>(0).unwrap(), ordinal);
                ordinal += 1;
            }
            ordinal
        };
        assert_eq!(rows_seen, count as i64);
        assert_eq!(statement.get_status(rusqlite::StatementStatus::Sort), 0);
    }
    drop(connection);
    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn trusted_payload_ranges_read_only_requested_bytes_in_order() {
    let path = test_path();
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let payload = b"abcdefgh";
    let (id, canonical) = bytes_object(payload);
    engine.put_object_if_absent(id, &canonical).unwrap();
    engine.reset_counters().unwrap();

    let requests = [(id, 2..5), (id, 0..0), (id, 8..8), (id, 0..8)];
    let mut observed = Vec::new();
    engine
        .get_authenticated_payload_ranges_batch(&requests, payload.len() as u64, |seen, bytes| {
            observed.push((seen, bytes.to_vec()));
            Ok(())
        })
        .unwrap();
    assert_eq!(
        observed,
        vec![
            (id, b"cde".to_vec()),
            (id, Vec::new()),
            (id, Vec::new()),
            (id, payload.to_vec()),
        ]
    );
    let counters = engine.counters().unwrap();
    assert_eq!(counters.payload_batch_queries, 1);
    assert_eq!(counters.payload_batch_references, 4);
    assert_eq!(counters.payload_batch_maximum, 4);
    assert_eq!(counters.fetched_rows, 4);
    assert_eq!(counters.fetched_row_authentication_passes, 0);
    assert_eq!(counters.fetched_row_role_decode_passes, 4);
    assert_eq!(counters.objects_validated, 4);
    assert_eq!(counters.object_bytes_read, (canonical.len() * 4) as u64);

    assert!(matches!(
        engine.get_authenticated_payload_ranges_batch(&[(id, 0..9)], 8, |_, _| Ok(())),
        Err(CoreError::InvalidRange { .. })
    ));
    assert!(matches!(
        engine.get_authenticated_payload_ranges_batch(&[(id, 0..8)], 7, |_, _| Ok(())),
        Err(CoreError::ChunkLengthMismatch)
    ));

    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn payload_range_batch_preserves_integrity_mode_and_callback_phases() {
    let path = test_path();
    let trusted = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let (first, first_canonical) = bytes_object(b"first");
    let (second, second_canonical) = bytes_object(b"second");
    trusted
        .put_object_if_absent(first, &first_canonical)
        .unwrap();
    trusted
        .put_object_if_absent(second, &second_canonical)
        .unwrap();
    trusted.reset_counters().unwrap();

    let requests = [(first, 1..4), (second, 2..5)];
    let mut calls = 0;
    assert!(matches!(
        trusted.get_authenticated_payload_ranges_batch(&requests, 6, |_, _| {
            calls += 1;
            if calls == 2 {
                Err(CoreError::InvalidRecord("callback"))
            } else {
                Ok(())
            }
        }),
        Err(CoreError::InvalidRecord("callback"))
    ));
    let counters = trusted.counters().unwrap();
    assert_eq!(counters.fetched_rows, 2);
    assert_eq!(counters.fetched_row_authentication_passes, 0);
    assert_eq!(counters.fetched_row_role_decode_passes, 2);
    assert_eq!(counters.objects_validated, 2);
    assert_eq!(
        counters.object_bytes_read,
        (first_canonical.len() + second_canonical.len()) as u64
    );
    drop(trusted);

    let mut tampered = second_canonical;
    *tampered.last_mut().unwrap() ^= 1;
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
            params![tampered, second.as_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);
    let verified = Engine::open(&path).unwrap();
    assert!(matches!(
        verified.get_authenticated_payload_ranges_batch(&[(second, 0..1)], 6, |_, _| Ok(())),
        Err(CoreError::IdentityMismatch)
    ));
    drop(verified);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn payload_range_batch_reports_the_exact_missing_ordinal() {
    let path = test_path();
    let engine = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let (present, canonical) = bytes_object(b"present");
    let missing = ObjectId::for_bytes(b"missing payload");
    engine.put_object_if_absent(present, &canonical).unwrap();

    let mut callbacks = 0;
    assert_eq!(
        engine.get_authenticated_payload_ranges_batch(
            &[(present, 0..1), (missing, 0..1)],
            7,
            |_, _| {
                callbacks += 1;
                Ok(())
            },
        ),
        Err(CoreError::MissingObject)
    );
    assert_eq!(callbacks, 1);

    callbacks = 0;
    assert_eq!(
        engine.get_authenticated_payload_ranges_batch(
            &[(missing, 0..1), (present, 0..1)],
            7,
            |_, _| {
                callbacks += 1;
                Ok(())
            },
        ),
        Err(CoreError::MissingObject)
    );
    assert_eq!(callbacks, 0);

    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn trusted_payload_range_batch_rejects_corrupt_framing() {
    let path = test_path();
    let trusted = Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
    let (id, mut canonical) = bytes_object(b"payload");
    trusted.put_object_if_absent(id, &canonical).unwrap();
    canonical[0] ^= 1;
    trusted
        .connection
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .execute(
            "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
            params![canonical, id.as_bytes().as_slice()],
        )
        .unwrap();

    assert!(matches!(
        trusted.get_authenticated_payload_ranges_batch(&[(id, 0..1)], 7, |_, _| Ok(())),
        Err(CoreError::Unsupported)
    ));

    drop(trusted);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn failed_single_callback_preserves_completed_fetch_and_authentication_only() {
    let path = test_path();
    let engine = Engine::open(&path).unwrap();
    let (id, canonical) = bytes_object(b"callback failure");
    engine.put_object_if_absent(id, &canonical).unwrap();
    engine.reset_counters().unwrap();

    assert!(matches!(
        engine.with_authenticated_canonical(id, |_| {
            Err::<(), _>(CoreError::InvalidRecord("callback failure"))
        }),
        Err(CoreError::InvalidRecord("callback failure"))
    ));
    let counters = engine.counters().unwrap();
    assert_eq!(counters.fetched_rows, 1);
    assert_eq!(counters.fetched_row_authentication_passes, 1);
    assert_eq!(counters.fetched_row_role_decode_passes, 0);

    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn failed_ordered_batch_preserves_each_completed_counter_phase() {
    let path = test_path();
    let engine = Engine::open(&path).unwrap();
    let (first, first_bytes) = bytes_object(b"first!");
    let (second, second_bytes) = bytes_object(b"second");
    let (_, substituted_bytes) = bytes_object(b"alter!");
    engine.put_object_if_absent(first, &first_bytes).unwrap();
    engine.put_object_if_absent(second, &second_bytes).unwrap();
    engine.reset_counters().unwrap();

    let mut callbacks = 0;
    assert!(matches!(
        engine.for_each_authenticated_payload_batch(&[first, second], |_, _| {
            callbacks += 1;
            if callbacks == 2 {
                Err(EngineError::InjectedFailure("batch callback"))
            } else {
                Ok(())
            }
        }),
        Err(EngineError::InjectedFailure("batch callback"))
    ));
    let counters = engine.counters().unwrap();
    assert_eq!(counters.fetched_rows, 2);
    assert_eq!(counters.fetched_row_authentication_passes, 2);
    assert_eq!(counters.fetched_row_role_decode_passes, 2);
    assert_eq!(counters.objects_validated, 2);
    assert_eq!(
        counters.object_bytes_read,
        (first_bytes.len() + second_bytes.len()) as u64
    );

    engine
        .connection
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .execute(
            "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
            params![substituted_bytes, second.as_bytes().as_slice()],
        )
        .unwrap();
    engine.reset_counters().unwrap();

    assert!(matches!(
        engine.for_each_authenticated_payload_batch(&[first, second], |_, _| Ok(())),
        Err(EngineError::MalformedObject { .. })
    ));
    let counters = engine.counters().unwrap();
    assert_eq!(counters.fetched_rows, 2);
    assert_eq!(counters.fetched_row_authentication_passes, 1);
    assert_eq!(counters.fetched_row_role_decode_passes, 1);
    assert_eq!(counters.objects_validated, 1);
    assert_eq!(counters.object_bytes_read, first_bytes.len() as u64);

    drop(engine);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn immutable_reuse_and_tamper_are_distinct() {
    let path = test_path();
    let (id, bytes) = bytes_object(b"immutable");
    let engine = Engine::open(&path).expect("open");
    assert_eq!(
        engine.put_object_if_absent(id, &bytes),
        Ok(PutOutcome::Created)
    );
    assert_eq!(
        engine.put_object_if_absent(id, &bytes),
        Ok(PutOutcome::Reused)
    );
    let changed = bytes_object(b"different").1;
    assert!(matches!(
        engine.put_object_if_absent(id, &changed),
        Err(EngineError::MalformedObject { .. })
    ));
    drop(engine);
    let connection = Connection::open(&path).expect("tamper connection");
    connection
        .execute(
            "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
            params![vec![1_u8, 2, 3], id.as_bytes().as_slice()],
        )
        .expect("tamper");
    drop(connection);
    let engine = Engine::open(&path).expect("reopen");
    assert!(matches!(
        engine.read_object_range(id, 0..1),
        Err(EngineError::ShortRead { .. })
            | Err(EngineError::IdentityMismatch { .. })
            | Err(EngineError::MalformedObject { .. })
    ));
    let _ = fs::remove_file(path);
}

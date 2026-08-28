use super::*;

#[test]
fn full_migration_faults_leave_the_legacy_source_and_no_candidate() {
    let source = test_path();
    let engine = Engine::open(&source).unwrap();
    let storage_id = engine.store_id().unwrap();
    drop(engine);
    let source_bytes = fs::read(&source).unwrap();

    for point in [
        "source_verified",
        "candidate_created",
        "history_copied",
        "releases_copied",
        "sync_copied",
        "membership_rebuilt",
        "foreign_keys_verified",
        "candidate_committed",
        "candidate_verified",
    ] {
        let candidate = test_path();
        assert!(matches!(
            crate::migration::migrate_legacy_durable_file_fault(
                &source,
                &candidate,
                storage_id,
                point,
            ),
            Err(EngineError::InjectedFailure(observed)) if observed == point
        ));
        assert!(!candidate.exists());
        for suffix in ["-journal", "-wal", "-shm"] {
            let mut sidecar = candidate.as_os_str().to_os_string();
            sidecar.push(suffix);
            assert!(!std::path::PathBuf::from(sidecar).exists());
        }
        assert_eq!(fs::read(&source).unwrap(), source_bytes);
    }

    let reopened = Engine::open(&source).unwrap();
    assert_eq!(reopened.store_id().unwrap(), storage_id);
    drop(reopened);
    fs::remove_file(source).unwrap();
}

#[test]
fn full_migration_cleanup_preserves_a_substituted_path() {
    let source = test_path();
    let candidate = test_path();
    let displaced = test_path();
    let engine = Engine::open(&source).unwrap();
    let storage_id = engine.store_id().unwrap();
    drop(engine);
    let source_bytes = fs::read(&source).unwrap();
    let replacement = b"not the migration-owned file";

    let result = crate::migration::migrate_legacy_durable_file_with_injector(
        &source,
        &candidate,
        storage_id,
        &mut |point| {
            if point == "candidate_created" {
                fs::rename(&candidate, &displaced).unwrap();
                fs::write(&candidate, replacement).unwrap();
                return Err(EngineError::InjectedFailure(point));
            }
            Ok(())
        },
    );
    assert!(matches!(
        result,
        Err(EngineError::InjectedFailure("candidate_created"))
    ));
    assert_eq!(fs::read(&candidate).unwrap(), replacement);
    assert!(FullStorage::open_durable(&displaced).is_ok());
    assert_eq!(fs::read(&source).unwrap(), source_bytes);

    fs::remove_file(source).unwrap();
    fs::remove_file(candidate).unwrap();
    fs::remove_file(displaced).unwrap();
}

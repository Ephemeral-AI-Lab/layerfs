//! Verified generation switching, maintenance, retention, and cleanup tests.

mod switching {
    use crate::generation::cleanup::{cleanup_owned_residue, recovery_residue_exists};
    use crate::generation::selector::{
        generation_filename, open_current, open_current_full_durable, read_selector, selector,
    };
    use crate::generation::switch::*;
    use crate::generation::{compact, directory, open_or_create, InstallBehavior, TestDriver};
    use crate::integrity::IntegrityMode;
    use crate::migration::migrate_selected_legacy_durable_generation;
    use crate::{Engine, EngineError, FULL_SCHEMA};
    use rusqlite::Connection;
    use std::fs;

    #[test]
    fn maintenance_lock_blocks_cleanup_and_lost_ack_reconciles_requested_selector() {
        let directory = directory("maintenance");
        let native = TestDriver::native();
        let mut engine = open_or_create(&directory, &native, IntegrityMode::Verified).unwrap();
        let live_scratch = crate::scratch::DiskTable::create_near(engine.path(), "live").unwrap();
        live_scratch.put(b"key", b"value").unwrap();
        assert!(try_acquire_maintenance(&directory).unwrap().is_none());
        drop(live_scratch);
        let candidate = directory.join(generation_filename(9));
        engine.compact_to(&candidate).unwrap();
        let candidate_engine = Engine::open(&candidate).unwrap();
        fs::write(
            directory.join("CURRENT.tmp"),
            selector(&candidate_engine, 9).unwrap().encode(),
        )
        .unwrap();
        engine.maintenance_pin.take();
        let maintenance = acquire_maintenance(&directory).unwrap();
        assert!(matches!(
            open_current(&directory, IntegrityMode::Verified),
            Err(EngineError::Sqlite {
                kind: crate::SqliteErrorKind::Busy | crate::SqliteErrorKind::Locked,
                ..
            })
        ));
        drop(maintenance);
        let cleanup = acquire_maintenance(&directory).unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        cleanup_owned_residue(&directory, &selected, None, &native).unwrap();
        drop(cleanup);
        assert!(candidate.exists());
        assert!(directory.join("CURRENT.tmp").exists());
        fs::remove_file(directory.join("CURRENT.tmp")).unwrap();
        fs::remove_file(candidate).unwrap();
        drop(engine);

        let engine = open_current(&directory, IntegrityMode::Verified).unwrap();
        let engine = compact(
            engine,
            &directory,
            &TestDriver::new(InstallBehavior::FailAfter),
        )
        .unwrap();
        assert_eq!(
            read_selector(&directory.join("CURRENT"))
                .unwrap()
                .generation,
            1
        );
        drop(engine);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn mismatched_compaction_source_never_cleans_target_residue() {
        let source_directory = directory("wrong-source-a");
        let target_directory = directory("wrong-source-b");
        let driver = TestDriver::native();
        let source = open_or_create(&source_directory, &driver, IntegrityMode::Verified).unwrap();
        let mut target =
            open_or_create(&target_directory, &driver, IntegrityMode::Verified).unwrap();
        let candidate = target_directory.join(generation_filename(1));
        target.compact_to(&candidate).unwrap();
        let candidate_engine = Engine::open(&candidate).unwrap();
        fs::write(
            target_directory.join("CURRENT.tmp"),
            selector(&candidate_engine, 1).unwrap().encode(),
        )
        .unwrap();
        target.maintenance_pin.take();
        drop(target);
        assert!(matches!(
            compact(source, &target_directory, &driver),
            Err(EngineError::InvalidRecord("compaction source generation"))
        ));
        assert!(candidate.exists());
        assert!(target_directory.join("CURRENT.tmp").exists());
        fs::remove_dir_all(source_directory).unwrap();
        fs::remove_dir_all(target_directory).unwrap();
    }

    #[test]
    fn preseeded_wal_maintenance_is_admitted_reconfigured_and_pinned() {
        let directory = directory("maintenance-wal");
        fs::create_dir(&directory).unwrap();
        let path = directory.join("MAINTENANCE.sqlite");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE maintenance_guard (
                    id INTEGER PRIMARY KEY CHECK (id = 1)
                 );
                 INSERT INTO maintenance_guard (id) VALUES (1);",
            )
            .unwrap();
        let pin = pin_connection(&directory).unwrap();
        assert!(try_acquire_maintenance(&directory).unwrap().is_none());
        assert_eq!(
            Connection::open(&path)
                .unwrap()
                .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                .unwrap()
                .to_ascii_lowercase(),
            "delete"
        );
        drop(pin);
        drop(acquire_maintenance(&directory).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn full_switch_retains_typed_legacy_rollback_and_reconciles_lost_ack() {
        let directory = directory("full-switch");
        let native = TestDriver::native();
        let mut legacy = open_or_create(&directory, &native, IntegrityMode::Verified).unwrap();
        let prior = read_selector(&directory.join("CURRENT")).unwrap();
        let retained_path = legacy.path().to_owned();
        legacy.maintenance_pin.take();
        drop(legacy);
        assert!(open_current_full_durable(&directory).is_err());

        let failed =
            migrate_selected_legacy_durable_generation(&directory, &TestDriver::fail_sync(1))
                .err()
                .expect("injected sync must fail");
        assert!(
            matches!(failed, EngineError::AmbiguousDurability),
            "unexpected switch failure: {failed:?}"
        );
        assert_eq!(read_selector(&directory.join("CURRENT")).unwrap(), prior);
        assert_eq!(read_selector(&directory.join("ROLLBACK")).unwrap(), prior);

        let opened = migrate_selected_legacy_durable_generation(
            &directory,
            &TestDriver::new(InstallBehavior::FailAfter),
        )
        .unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        assert_eq!(selected.schema_version, FULL_SCHEMA.schema_version as u32);
        assert_eq!(opened.storage_id(), prior.store_id);
        assert!(try_acquire_maintenance(&directory).unwrap().is_none());
        drop(opened);
        assert!(!recovery_residue_exists(&directory).unwrap());
        cleanup_owned_residue(&directory, &selected, Some(prior.generation), &native).unwrap();
        assert!(retained_path.exists());
        let pinned = open_current_full_durable(&directory).unwrap();
        assert!(try_acquire_maintenance(&directory).unwrap().is_none());
        drop(pinned);

        let replay = || migrate_selected_legacy_durable_generation(&directory, &native);
        drop(replay().unwrap());
        let rollback_path = directory.join("ROLLBACK");
        let rollback = fs::read(&rollback_path).unwrap();
        let mut wrong_rollback = prior.clone();
        wrong_rollback.generation = selected.generation;
        fs::write(&rollback_path, wrong_rollback.encode()).unwrap();
        assert!(replay().is_err());
        assert!(
            cleanup_owned_residue(&directory, &selected, Some(prior.generation), &native).is_err()
        );
        assert!(retained_path.exists());
        fs::write(&rollback_path, rollback).unwrap();
        let retained = fs::read(&retained_path).unwrap();
        fs::write(&retained_path, b"malformed retained generation").unwrap();
        assert!(replay().is_err());
        fs::write(&retained_path, retained).unwrap();
        fs::remove_file(&retained_path).unwrap();
        assert!(replay().is_err());
        fs::remove_dir_all(directory).unwrap();
    }
}

mod cleanup {
    use crate::generation::cleanup::*;
    use crate::generation::selector::{generation_filename, open_current, read_selector, selector};
    use crate::generation::switch::{acquire_maintenance, compact, open_or_create};
    use crate::generation::{directory, InstallBehavior, TestDriver};
    use crate::integrity::IntegrityMode;
    use crate::{Engine, EngineError};
    use layerfs_core::ObjectId;
    use rusqlite::Connection;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    fn current_generation(directory: &Path) -> u64 {
        read_selector(&directory.join("CURRENT"))
            .unwrap()
            .generation
    }

    fn legacy_store(label: &str, mode: IntegrityMode) -> (PathBuf, TestDriver, Engine) {
        let directory = directory(label);
        let driver = TestDriver::native();
        let engine = open_or_create(&directory, &driver, mode).unwrap();
        (directory, driver, engine)
    }

    #[test]
    fn scratch_crash_child() {
        let Some(store) = std::env::var_os("LAYERFS_SCRATCH_CRASH_STORE") else {
            return;
        };
        let table =
            crate::scratch::DiskTable::create_near(Path::new(&store), "crash-child").unwrap();
        table.put(b"pending", &vec![0xa5; 64 * 1024]).unwrap();
        std::process::exit(91);
    }

    #[test]
    fn unselected_generation_crash_child() {
        let Some(directory) = std::env::var_os("LAYERFS_UNSELECTED_GENERATION_CRASH") else {
            return;
        };
        let directory = directory.as_ref();
        let engine = open_current(directory, IntegrityMode::Verified).unwrap();
        engine
            .compact_to(&directory.join(generation_filename(1)))
            .unwrap();
        std::process::exit(93);
    }

    #[test]
    fn compaction_rejects_nonempty_legacy_state_without_erasing_it() {
        for legacy in ["root", "delta", "visible"] {
            let (directory, driver, engine) = legacy_store(
                &format!("legacy-{legacy}-compact"),
                IntegrityMode::TrustedLocalDev,
            );
            let selected_path = engine.path().to_owned();
            drop(engine);
            let root = ObjectId::for_bytes(b"legacy root");
            let child = ObjectId::for_bytes(b"legacy directory");
            let connection = Connection::open(&selected_path).unwrap();
            match legacy {
                "root" => connection.execute(
                    "INSERT INTO layerfs_roots (root_id, directory_object, parent_root) VALUES (?1, ?2, NULL)",
                    rusqlite::params![root.as_bytes().as_slice(), child.as_bytes().as_slice()],
                ),
                "delta" => connection.execute(
                    "INSERT INTO layerfs_deltas (delta_id, format_version, parent_root, child_root, payload) VALUES (?1, 0, NULL, ?2, X'00')",
                    rusqlite::params![root.as_bytes().as_slice(), child.as_bytes().as_slice()],
                ),
                "visible" => connection.execute(
                    "UPDATE layerfs_store_meta SET visible_root = ?1 WHERE store_id = 1",
                    rusqlite::params![root.as_bytes().as_slice()],
                ),
                _ => unreachable!(),
            }
            .unwrap();
            drop(connection);
            let before = fs::read(directory.join("CURRENT")).unwrap();
            let engine = open_current(&directory, IntegrityMode::TrustedLocalDev).unwrap();
            assert!(matches!(
                compact(engine, &directory, &driver),
                Err(EngineError::InvalidRecord("legacy compaction state"))
            ));
            assert_eq!(fs::read(directory.join("CURRENT")).unwrap(), before);
            assert!(!directory.join(generation_filename(1)).exists());
            let connection = Connection::open(&selected_path).unwrap();
            let preserved = match legacy {
                "root" => {
                    connection.query_row("SELECT EXISTS(SELECT 1 FROM layerfs_roots)", [], |row| {
                        row.get::<_, bool>(0)
                    })
                }
                "delta" => {
                    connection.query_row("SELECT EXISTS(SELECT 1 FROM layerfs_deltas)", [], |row| {
                        row.get::<_, bool>(0)
                    })
                }
                "visible" => connection.query_row(
                    "SELECT visible_root IS NOT NULL FROM layerfs_store_meta WHERE store_id = 1",
                    [],
                    |row| row.get::<_, bool>(0),
                ),
                _ => unreachable!(),
            }
            .unwrap();
            assert!(preserved, "{legacy} state was erased");
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn exact_next_generation_without_selector_is_reported_and_preserved() {
        let (directory, driver, engine) = legacy_store("unresolved-next", IntegrityMode::Verified);
        drop(engine);
        let candidate = directory.join(generation_filename(1));
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::generation_switch::cleanup::unselected_generation_crash_child",
            ])
            .env("LAYERFS_UNSELECTED_GENERATION_CRASH", &directory)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(93));
        assert!(matches!(
            open_or_create(&directory, &driver, IntegrityMode::Verified),
            Err(EngineError::UnresolvedGenerationResidue { generation: 1 })
        ));
        assert!(candidate.exists());
        let engine = open_current(&directory, IntegrityMode::Verified).unwrap();
        assert!(compact(engine, &directory, &driver).is_err());
        assert!(candidate.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_selector_install_recovers_prior_and_removes_only_owned_candidate() {
        let (directory, native, engine) = legacy_store("recovery", IntegrityMode::Verified);
        let store_id = engine.store_id().unwrap();
        assert!(compact(
            engine,
            &directory,
            &TestDriver::new(InstallBehavior::FailBefore),
        )
        .is_err());
        fs::write(directory.join("unknown-residue"), b"preserve").unwrap();
        let recovered = open_or_create(&directory, &native, IntegrityMode::Verified).unwrap();
        assert_eq!(recovered.store_id().unwrap(), store_id);
        assert_eq!(current_generation(&directory), 0);
        assert!(!directory.join("CURRENT.tmp").exists());
        assert!(!directory.join(generation_filename(1)).exists());
        assert!(directory.join("unknown-residue").exists());
        drop(recovered);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn post_visible_cleanup_sync_failure_is_ambiguous() {
        let directory = directory("cleanup-sync");
        let engine =
            open_or_create(&directory, &TestDriver::native(), IntegrityMode::Verified).unwrap();
        assert!(matches!(
            compact(engine, &directory, &TestDriver::fail_sync(1)),
            Err(EngineError::AmbiguousDurability)
        ));
        assert_eq!(current_generation(&directory), 1);
        drop(open_current(&directory, IntegrityMode::Verified).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exact_next_candidate_cleanup_preserves_valid_unrelated_temporary_selector() {
        let (directory, driver, mut engine) =
            legacy_store("unrelated-temp", IntegrityMode::Verified);
        let next_path = directory.join(generation_filename(1));
        engine.compact_to(&next_path).unwrap();
        let unrelated_path = directory.join(generation_filename(9));
        engine.compact_to(&unrelated_path).unwrap();
        let unrelated = Engine::open(&unrelated_path).unwrap();
        fs::write(
            directory.join("CURRENT.tmp"),
            selector(&unrelated, 9).unwrap().encode(),
        )
        .unwrap();
        engine.maintenance_pin.take();
        let maintenance = acquire_maintenance(&directory).unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        cleanup_owned_residue(&directory, &selected, None, &driver).unwrap();
        drop(maintenance);
        assert!(next_path.exists());
        assert!(unrelated_path.exists());
        assert!(directory.join("CURRENT.tmp").exists());
        drop(engine);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn selector_cleanup_refuses_path_substitution_after_inspection() {
        let (directory, _native, mut engine) =
            legacy_store("selector-substitute", IntegrityMode::Verified);
        let candidate = directory.join(generation_filename(1));
        engine.compact_to(&candidate).unwrap();
        let candidate_engine = Engine::open(&candidate).unwrap();
        fs::write(
            directory.join("CURRENT.tmp"),
            selector(&candidate_engine, 1).unwrap().encode(),
        )
        .unwrap();
        engine.maintenance_pin.take();
        let maintenance = acquire_maintenance(&directory).unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        assert!(cleanup_owned_residue(
            &directory,
            &selected,
            None,
            &TestDriver::substitute_on_remove(),
        )
        .is_err());
        assert_eq!(fs::read(&candidate).unwrap(), b"substitute");
        assert!(candidate.with_extension("custody-saved").exists());
        assert!(directory.join("CURRENT.tmp").exists());
        drop(maintenance);
        drop(engine);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn exclusive_maintenance_recovers_child_exit_hot_scratch() {
        let (directory, driver, engine) = legacy_store("scratch-crash", IntegrityMode::Verified);
        let store = engine.path().to_owned();
        drop(engine);
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::generation_switch::cleanup::scratch_crash_child",
            ])
            .env("LAYERFS_SCRATCH_CRASH_STORE", &store)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(91));
        let scratch = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("-crash-child-")
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "sqlite")
            })
            .unwrap();
        let journal = PathBuf::from(format!("{}-journal", scratch.display()));
        assert!(journal.exists());
        let maintenance = acquire_maintenance(&directory).unwrap();
        let selected = read_selector(&directory.join("CURRENT")).unwrap();
        let verified =
            crate::generation::selector::open_selected(&directory, IntegrityMode::Verified)
                .unwrap();
        crate::scratch::recover_owned_near(verified.path(), selected.store_id, &driver).unwrap();
        assert!(!scratch.exists());
        assert!(!journal.exists());
        drop(verified);
        drop(maintenance);
        drop(open_current(&directory, IntegrityMode::Verified).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }
}

//! Full generation visibility, restart, and custody qualification.

use super::create::test_support::{directory, InstallBehavior, TestDriver};
use super::selector::{full_selector, generation_filename, install, open_current_full_durable};
use super::switch::{
    compact_full_durable, compact_full_durable_with_injector, restore_full_durable_backup,
};
use crate::{EngineError, FullStorage};
use std::fs;

fn full_directory(label: &str) -> (std::path::PathBuf, FullStorage) {
    let directory = directory(label);
    fs::create_dir(&directory).unwrap();
    let path = directory.join(generation_filename(0));
    let created = FullStorage::create_durable(&path).unwrap();
    let selector = full_selector(&created, 0).unwrap();
    drop(created);
    install(&directory, selector, None, &TestDriver::native()).unwrap();
    let opened = open_current_full_durable(&directory).unwrap();
    (directory, opened)
}

#[test]
fn interrupted_full_compaction_reopens_prior_and_resumes_verified_candidate() {
    let (directory, storage) = full_directory("full-compact-restart");
    let prior = storage.path().to_owned();
    let error = match compact_full_durable_with_injector(
        storage,
        &directory,
        &TestDriver::native(),
        &mut |point| {
            if point == "rollback_visible" {
                Err(EngineError::InjectedFailure(point))
            } else {
                Ok(())
            }
        },
    ) {
        Ok(_) => panic!("injected compaction succeeded"),
        Err(error) => error,
    };
    assert_eq!(error, EngineError::InjectedFailure("rollback_visible"));
    let reopened = open_current_full_durable(&directory).unwrap();
    assert_eq!(reopened.path(), prior);
    let compacted = compact_full_durable(reopened, &directory, &TestDriver::native()).unwrap();
    assert!(compacted
        .path()
        .ends_with("generation-0000000000000001.sqlite"));
    assert!(prior.exists());
    drop(compacted);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn verified_full_candidate_path_substitution_fails_without_deleting_substitute() {
    let (directory, storage) = full_directory("full-compact-substitute");
    let candidate = directory.join(generation_filename(1));
    let saved = directory.join("custodied-candidate.sqlite");
    let result = compact_full_durable_with_injector(
        storage,
        &directory,
        &TestDriver::native(),
        &mut |point| {
            if point == "candidate_verified" {
                fs::rename(&candidate, &saved).unwrap();
                fs::write(&candidate, b"substitute").unwrap();
            }
            Ok(())
        },
    );
    assert!(matches!(result, Err(EngineError::InvalidRecord(_))));
    assert_eq!(fs::read(&candidate).unwrap(), b"substitute");
    assert!(saved.exists());
    assert!(open_current_full_durable(&directory).is_ok());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn full_restore_reconciles_lost_current_install_acknowledgement() {
    let (source, storage) = full_directory("full-restore-source");
    let backup = source.join("backup.sqlite");
    storage.backup_to(&backup).unwrap();
    drop(storage);
    let restored = directory("full-restore-lost-ack");
    let opened = restore_full_durable_backup(
        &backup,
        &restored,
        &TestDriver::new(InstallBehavior::FailAfter),
    )
    .unwrap();
    assert!(opened
        .path()
        .ends_with("generation-0000000000000000.sqlite"));
    drop(opened);
    fs::remove_dir_all(source).unwrap();
    fs::remove_dir_all(restored).unwrap();
}

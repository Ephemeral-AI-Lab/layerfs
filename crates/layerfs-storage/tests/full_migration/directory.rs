use super::fixture::{path, publish_valid_root};
use layerfs_storage::generation::{open_or_create, NativeGenerationDriver, StoreSelector};
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::{
    migration::{migrate_selected_legacy_durable_generation, rollback_selected_full_generation},
    BranchId, FullStorage, LayerId, LayerStackId, FULL_SCHEMA,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn selector(path: &Path) -> StoreSelector {
    StoreSelector::decode(&std::fs::read(path).unwrap()).unwrap()
}

fn generation_path(directory: &Path, generation: u64) -> PathBuf {
    directory.join(format!("generation-{generation:016x}.sqlite"))
}

fn exclusive_maintenance_available(directory: &Path) -> bool {
    let connection = Connection::open(directory.join("MAINTENANCE.sqlite")).unwrap();
    connection.busy_timeout(Duration::ZERO).unwrap();
    match connection.execute_batch("BEGIN EXCLUSIVE") {
        Ok(()) => {
            connection.execute_batch("ROLLBACK").unwrap();
            true
        }
        Err(_) => false,
    }
}

#[test]
fn directory_migration_switches_once_retains_rollback_and_replays_exactly() {
    let directory = path("selected-directory");
    let driver = NativeGenerationDriver;
    let legacy = open_or_create(&directory, &driver, IntegrityMode::Verified).unwrap();
    let prior = selector(&directory.join("CURRENT"));
    let prior_path = legacy.path().to_owned();
    assert_eq!(prior_path, generation_path(&directory, prior.generation));
    drop(legacy);

    let full = migrate_selected_legacy_durable_generation(&directory, &driver).unwrap();
    let current = selector(&directory.join("CURRENT"));
    assert_eq!(current.generation, prior.generation + 1);
    assert_eq!(current.schema_version, FULL_SCHEMA.schema_version as u32);
    assert_eq!(current.store_id, prior.store_id);
    assert_eq!(current.profile_id, prior.profile_id);
    assert_eq!(selector(&directory.join("ROLLBACK")), prior);
    assert!(prior_path.is_file());
    assert!(generation_path(&directory, current.generation).is_file());
    assert_eq!(full.storage_id(), prior.store_id);
    assert_eq!(full.path(), generation_path(&directory, current.generation));
    assert!(!exclusive_maintenance_available(&directory));
    drop(FullStorage::open_durable_verified(full.path()).unwrap());
    drop(full);
    assert!(exclusive_maintenance_available(&directory));

    let selected_path = generation_path(&directory, current.generation);
    let reopened = FullStorage::open_durable_verified(&selected_path).unwrap();
    assert_eq!(reopened.storage_id(), prior.store_id);
    drop(reopened);
    let replay = migrate_selected_legacy_durable_generation(&directory, &driver).unwrap();
    assert_eq!(replay.path(), selected_path);
    assert_eq!(selector(&directory.join("CURRENT")), current);
    assert_eq!(selector(&directory.join("ROLLBACK")), prior);
    assert!(!exclusive_maintenance_available(&directory));
    drop(replay);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn sealed_rollback_restores_exact_legacy_and_removes_full_generation() {
    let directory = path("selected-rollback");
    let driver = NativeGenerationDriver;
    let legacy = open_or_create(&directory, &driver, IntegrityMode::Verified).unwrap();
    let prior = selector(&directory.join("CURRENT"));
    let legacy_path = legacy.path().to_owned();
    let (root, _) = publish_valid_root(&legacy, "rollback", [0xc1; 32]);
    let stack = legacy
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xc2; 32]),
            LayerId::from_bytes([0xc3; 32]),
            "rollback",
            root,
        )
        .unwrap();
    let branch = legacy
        .product_create_top_level_branch(BranchId::from_bytes([0xc4; 32]), None, stack)
        .unwrap();
    drop(legacy);
    Connection::open(&legacy_path)
        .unwrap()
        .execute("DELETE FROM layerfs_refs", [])
        .unwrap();

    let full = migrate_selected_legacy_durable_generation(&directory, &driver).unwrap();
    let current = selector(&directory.join("CURRENT"));
    let full_path = full.path().to_owned();
    drop(full);
    let rolled = rollback_selected_full_generation(&directory, &driver).unwrap();
    assert_eq!(selector(&directory.join("CURRENT")), prior);
    assert!(!directory.join("ROLLBACK").exists());
    assert!(!full_path.exists());
    assert!(legacy_path.exists());
    assert_eq!(rolled.path(), legacy_path);
    assert_eq!(rolled.store_id().unwrap(), current.store_id);
    assert_eq!(
        rolled.product_branch_head(branch.branch_id).unwrap(),
        Some(branch)
    );
    assert_eq!(
        rolled
            .product_layer_stack_head(stack.layer_stack_id)
            .unwrap(),
        Some(stack)
    );
    drop(rolled);
    let reopened =
        layerfs_storage::generation::open_current(&directory, IntegrityMode::Verified).unwrap();
    assert_eq!(
        reopened.product_branch_head(branch.branch_id).unwrap(),
        Some(branch)
    );
    assert_eq!(
        reopened
            .product_layer_stack_head(stack.layer_stack_id)
            .unwrap(),
        Some(stack)
    );
    drop(reopened);

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rollback_rejects_a_selected_full_with_new_authoritative_state() {
    let directory = path("selected-rollback-mutated");
    let driver = NativeGenerationDriver;
    drop(open_or_create(&directory, &driver, IntegrityMode::Verified).unwrap());
    let prior = selector(&directory.join("CURRENT"));
    let full = migrate_selected_legacy_durable_generation(&directory, &driver).unwrap();
    let current = selector(&directory.join("CURRENT"));
    let full_path = full.path().to_owned();
    drop(full);
    let connection = Connection::open(&full_path).unwrap();
    assert_eq!(
        connection
            .execute(
                "UPDATE layerfs_store_meta SET next_inode_serial = next_inode_serial + 1",
                [],
            )
            .unwrap(),
        1
    );
    drop(connection);
    drop(FullStorage::open_durable_verified(&full_path).unwrap());

    assert!(rollback_selected_full_generation(&directory, &driver).is_err());
    assert_eq!(selector(&directory.join("CURRENT")), current);
    assert_eq!(selector(&directory.join("ROLLBACK")), prior);
    assert!(full_path.exists());

    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn rollback_retry_finishes_cleanup_after_current_was_already_restored() {
    let directory = path("selected-rollback-retry");
    let driver = NativeGenerationDriver;
    drop(open_or_create(&directory, &driver, IntegrityMode::Verified).unwrap());
    let prior = selector(&directory.join("CURRENT"));
    let full = migrate_selected_legacy_durable_generation(&directory, &driver).unwrap();
    let full_path = full.path().to_owned();
    drop(full);

    std::fs::write(directory.join("CURRENT"), prior.encode()).unwrap();
    assert!(directory.join("ROLLBACK").exists());
    assert!(full_path.exists());
    let rolled = rollback_selected_full_generation(&directory, &driver).unwrap();
    assert_eq!(rolled.path(), generation_path(&directory, prior.generation));
    assert_eq!(selector(&directory.join("CURRENT")), prior);
    assert!(!directory.join("ROLLBACK").exists());
    assert!(!full_path.exists());
    drop(rolled);

    std::fs::remove_dir_all(directory).unwrap();
}

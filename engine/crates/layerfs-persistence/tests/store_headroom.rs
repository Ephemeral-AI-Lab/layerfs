//! The free-space guard refuses a pack batch while its margin is still free
//! (#379), so the Store keeps room for metadata writes on a filling disk.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, ReserveRequest};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqlitePersistenceProfile, STORE_HEADROOM_BYTES,
};
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo},
    pack::{assemble, build_group, PackLane},
    port::*,
    StoragePolicy,
};
use std::{path::Path, sync::Arc};

const RECORD: usize = 4 * 1024 * 1024;
const MIB: u128 = 1024 * 1024;

fn config() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"store-headroom".to_vec(),
        cursor_key: [79; 32],
        incarnation: 1,
    }
}
fn available(path: &Path) -> u128 {
    let fs = nix::sys::statvfs::statvfs(path).unwrap();
    u128::from(fs.blocks_available()) * u128::from(fs.fragment_size())
}
/// One 4 MiB payload record alone in its pack, as a large file is saved.
fn singleton(pack_id: i64, n: u64) -> Publication {
    let mut record = vec![0u8; RECORD];
    record[..8].copy_from_slice(&n.to_be_bytes());
    let group = build_group(PackLane::Singleton, &[record], None).unwrap();
    let body = assemble(PackLane::Singleton, &[group]).unwrap();
    Publication {
        packs: vec![PublishedPack {
            info: PackInfo {
                pack_id,
                domain: PackDomain::Payload,
                key: ObjectKey::for_bytes(&body),
                length: body.len(),
            },
            body: Arc::new(body),
        }],
        objects: vec![ObjectLocation {
            object_id: ObjectId::for_bytes(&n.to_be_bytes()),
            role: ObjectRole::WholeFile,
            canonical_length: RECORD,
            pack_id,
            group_number: 0,
            record_number: 0,
        }],
        ..Publication::default()
    }
}

#[test]
#[ignore = "requires a dedicated disposable filesystem of 96 to 256 MiB in LAYERFS_TEST_DISK_ROOT"]
fn a_pack_batch_is_refused_while_the_margin_is_still_free() {
    let root = std::path::PathBuf::from(
        std::env::var_os("LAYERFS_TEST_DISK_ROOT").expect("dedicated filesystem required"),
    );
    let fs = nix::sys::statvfs::statvfs(&root).unwrap();
    let size = u128::from(fs.blocks()) * u128::from(fs.fragment_size());
    assert!(
        (96 * MIB..=256 * MIB).contains(&size),
        "refusing to fill a non-test filesystem of {size} bytes"
    );
    let path = root.join("headroom-store.sqlite");
    let h = Handles::create(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &config(),
    )
    .unwrap();
    let first = h
        .storage
        .reserve(Reserve {
            packs: 64,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    let mut published = 0u64;
    let refusal = loop {
        assert!(published < 64, "the filesystem never reached the margin");
        match h
            .storage
            .publish(&singleton(first + published as i64, published))
        {
            Ok(_) => published += 1,
            Err(error) => break error,
        }
    };
    let left = available(&root);
    assert_eq!(
        refusal,
        PersistenceError::Refused {
            status: "Capacity".to_owned()
        }
    );
    assert!(published > 0, "no batch fitted before the margin");
    // The refused batch wrote nothing, and the admitted ones left the margin
    // free apart from SQLite's page overhead.
    assert!(
        left + 8 * MIB >= u128::from(STORE_HEADROOM_BYTES),
        "only {left} bytes left"
    );
    // A metadata-only write still lands inside the margin.
    let scope = ObjectId::for_bytes(b"scope");
    h.history
        .reserve_inodes(&ReserveRequest { scope, count: 1 })
        .unwrap();
    println!(
        "filesystem_bytes={size} published_packs={published} record_bytes={RECORD} \
         available_after_refusal={left} margin={STORE_HEADROOM_BYTES} refusal={refusal:?}"
    );
}

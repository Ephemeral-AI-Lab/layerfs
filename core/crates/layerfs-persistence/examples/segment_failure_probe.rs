//! External checked-close uncertainty proof through the ordinary immutable publication port.
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePackLayout};
use layerfs_storage::{
    location::{PackDomain, PackInfo},
    pack::{assemble, build_group, PackLane},
    port::*,
    StoragePolicy,
};
use std::sync::Arc;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("fresh database required")?;
    let config = HistoryCatalogConfig {
        binding_key: b"segment-failure-proof".to_vec(),
        cursor_key: [71; 32],
        incarnation: 1,
    };
    let h = Handles::create(
        PersistenceConfig::sqlite(&path).with_sqlite_pack_layout(SqlitePackLayout::PayloadSegments),
        StoragePolicy::frozen_default(),
        &config,
    )?;
    let id = h
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })?
        .first_pack_id;
    let body = assemble(
        PackLane::WholeFile,
        &[build_group(PackLane::WholeFile, &[vec![19; 90000]], None)?],
    )?;
    let batch = Publication {
        packs: vec![PublishedPack {
            info: PackInfo {
                pack_id: id,
                domain: PackDomain::Payload,
                key: ObjectKey::for_bytes(&body),
                length: body.len(),
            },
            body: Arc::new(body),
        }],
        ..Publication::default()
    };
    assert_eq!(h.storage.publish(&batch), Err(PersistenceError::Uncertain));
    assert_eq!(
        h.storage.reserve(Reserve {
            packs: 1,
            ordinals: 0
        }),
        Err(PersistenceError::Uncertain)
    );
    let work = h.diagnostics()?;
    assert_eq!(work.segment_file_sync_calls, 1);
    assert_eq!(work.segment_write_bytes, batch.packs[0].info.length as u64);
    drop(h);
    let h = Handles::open_writable(
        PersistenceConfig::sqlite(&path),
        &config.binding_key,
        config.cursor_key,
    )?;
    let next = h
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })?
        .first_pack_id;
    assert_eq!(next, id + 1);
    assert_eq!(
        h.storage.read_packs(&[id], &mut Vec::new()),
        Err(PersistenceError::Missing)
    );
    let mut directory = path.to_os_string();
    directory.push(".payload");
    let files = std::fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0].metadata()?.len(),
        batch.packs[0].info.length as u64
    );
    println!("{{\"status\":\"PASS\",\"uncertain_publication\":true,\"quarantined\":true,\"retained_files\":1,\"reserved_id\":{id},\"next_id\":{next},\"referenced_body\":false}}");
    Ok(())
}

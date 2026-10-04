//! Real SQLite offset reads preserve the whole-pack digest and exact bytes.
mod support;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::{location::PackInfo, port::*, Storage, StoragePolicy};

struct Plan(PackReadChoice);
impl PackReadPlan for Plan {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        Ok(self.0.clone())
    }
}
#[test]
fn sqlite_selected_ranges_scan_once_close_and_match_the_full_body() {
    let t = support::Temp::new("pack-selection");
    let h = Handles::create(
        PersistenceConfig::sqlite(t.join("db")),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"pack-selection".to_vec(),
            cursor_key: [21; 32],
            incarnation: 1,
        },
    )
    .unwrap();
    let mut random = 871_321u64;
    let bytes: Vec<_> = (0..32_000)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random as u8
        })
        .collect();
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(&bytes).unwrap(),
    )
    .unwrap();
    let storage = Storage::new(h.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    let mut locations = Vec::new();
    h.storage.locate(&[object.id()], &mut locations).unwrap();
    let id = locations[0].pack.pack_id;
    let mut packs = Vec::new();
    h.storage.read_packs(&[id], &mut packs).unwrap();
    let body = packs[0].body();
    assert!(body.len() > 5000);
    let range = PackRange {
        offset: 4900,
        length: 100,
    };
    let before = h.diagnostics().unwrap();
    let selected = h
        .storage
        .read_pack_selection(id, &mut Plan(PackReadChoice::Ranges(vec![range])))
        .unwrap();
    let PersistedPackRead::Ranges(selected) = selected else {
        panic!("range strategy changed")
    };
    assert_eq!(selected.info(), packs[0].info());
    assert_eq!(
        selected.ranges()[0].1,
        body[range.offset..range.offset + range.length]
    );
    let work = h.diagnostics().unwrap();
    assert_eq!(work.blob_open_calls - before.blob_open_calls, 1);
    assert_eq!(work.blob_close_calls - before.blob_close_calls, 1);
    assert_eq!(
        work.blob_requested_bytes - before.blob_requested_bytes,
        body.len() as u64
    );
    assert_eq!(
        work.blob_read_bytes - before.blob_read_bytes,
        body.len() as u64
    );
    assert!(matches!(
        h.storage
            .read_pack_selection(i64::MAX, &mut Plan(PackReadChoice::Whole)),
        Err(PersistenceError::Missing)
    ));
    // Successful checked close/commit leaves the same session usable.
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
            .unwrap()[0],
        object.canonical()
    );
}

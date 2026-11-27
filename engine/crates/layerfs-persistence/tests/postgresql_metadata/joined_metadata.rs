//! Captured Commit's bounded PostgreSQL locate-and-pack read.
use crate::chains;

use chains::{noise, save, storage, url, whole, Store};
use layerfs_content::{
    filesystem::{
        name::path::PathName,
        tree::directory::{encode_directory_page, DirectoryPage},
    },
    FinalizedObject, ObjectRole,
};
use layerfs_storage::policy::PACK_LIMIT;

fn directory(name: &str, serial: u64) -> FinalizedObject {
    let page = DirectoryPage::Leaf {
        entries: vec![(PathName::new(name).unwrap(), serial)],
    };
    FinalizedObject::new(
        ObjectRole::DirectoryLeaf,
        encode_directory_page(&page).unwrap(),
    )
    .unwrap()
}

#[test]
fn joined_save_demand_reuses_one_verified_pack_and_leaves_payload_acquisition_separate() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "join_metadata");
    let objects = [
        directory("alpha", 2),
        directory("beta", 3),
        whole(&noise(71, 6_000)),
        whole(&noise(72, 6_000)),
    ];
    let existing = storage(&store.handles);
    save(&existing, objects.to_vec());

    let ids: Vec<_> = objects.iter().map(FinalizedObject::id).collect();
    let expected: Vec<_> = objects.iter().map(FinalizedObject::canonical).collect();
    let handles = store.open();
    let probe = storage(&handles);
    let save = probe.begin_save().unwrap().joining_metadata_reads();

    // The joined statement runs on this mixed demand but includes the shared
    // metadata pack once. The payload records use the usual acquisition.
    assert_eq!(save.read_objects(&ids).unwrap(), expected);
    let measured = probe.diagnostics();
    assert_eq!(
        (measured.joined_pack_locates, measured.joined_packs),
        (1, 1)
    );
    assert!((1..=PACK_LIMIT as u64).contains(&measured.joined_pack_bytes));
    assert!(measured.payload_read_bytes > 0 || measured.group_read_bytes > 0);

    let before = handles.diagnostics().unwrap();
    assert_eq!(save.read_objects(&ids).unwrap(), expected);
    let after = handles.diagnostics().unwrap();
    assert_eq!(after.round_trips, before.round_trips);
    assert_eq!(probe.diagnostics().joined_pack_locates, 1);
    assert_eq!(probe.diagnostics().joined_packs, 1);
}

#[test]
fn ineligible_pack_formats_keep_the_original_body_read() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "join_payload");
    let eraser = storage(&store.handles);
    let payload = [whole(&noise(73, 6_000)), whole(&noise(74, 6_000))];
    save(&eraser, payload.to_vec());
    let handles = store.open();
    let probe = storage(&handles);
    let pending = probe.begin_save().unwrap().joining_metadata_reads();
    assert_eq!(
        pending
            .read_objects(&payload.each_ref().map(FinalizedObject::id))
            .unwrap(),
        payload.each_ref().map(FinalizedObject::canonical)
    );
    let work = probe.diagnostics();
    assert_eq!((work.joined_pack_locates, work.joined_packs), (1, 0));
    assert_eq!(work.joined_pack_bytes, 0);
}

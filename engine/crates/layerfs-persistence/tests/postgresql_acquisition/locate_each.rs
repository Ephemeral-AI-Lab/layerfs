//! #818 H5: a per-object locate (`locate_chains_each`) lists a located row
//! whose pack row is gone instead of refusing the call, and answers the
//! other rows as `locate_chains` does; `locate_chains` still refuses the
//! call. Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "../support/pg/chains.rs"]
mod chains;

use chains::*;
use layerfs_storage::{location::LocatedChains, port::*};

#[test]
fn a_located_row_whose_pack_is_gone_is_unpacked_and_the_others_locate() {
    let Some(url) = url() else { return };
    let store = Store::new(&url, "unpacked");
    let storage = storage(&store.handles);
    // Two objects in two Saves, so in two packs.
    let (gone, kept) = (whole(&noise(818, 20_000)), whole(&noise(819, 20_000)));
    save(&storage, vec![gone.clone()]);
    save(&storage, vec![kept.clone()]);
    let pack = row(&store, gone.id()).location.pack_id;
    assert_ne!(pack, row(&store, kept.id()).location.pack_id);
    // A published pack is immutable and its locations reference it: the test
    // lifts both guards for its own session.
    let mut admin = store.admin();
    admin
        .batch_execute("SET session_replication_role = replica")
        .unwrap();
    let deleted = admin
        .execute("DELETE FROM pack WHERE pack_id = $1", &[&pack])
        .unwrap();
    assert_eq!(deleted, 1, "the gone object's pack row");

    let reader = store.open();
    let ids = [gone.id(), kept.id()];
    let (mut rows, mut chains, mut unpacked) = (Vec::new(), LocatedChains::default(), Vec::new());
    reader
        .storage
        .locate_chains_each(&ids, &mut rows, &mut chains, &mut unpacked)
        .unwrap();
    assert_eq!(unpacked, vec![gone.id()]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].location.object_id, kept.id());
    assert_eq!(
        rows[0].location.pack_id,
        row(&store, kept.id()).location.pack_id
    );

    // The call every other reader makes still refuses the page.
    let refused = reader.storage.locate_chains(&ids, &mut rows, &mut chains);
    assert_eq!(refused, Err(PersistenceError::Malformed));
    // A page with no gone pack answers alike either way.
    let (mut each, mut plain) = (Vec::new(), Vec::new());
    reader
        .storage
        .locate_chains_each(&ids[1..], &mut each, &mut chains, &mut unpacked)
        .unwrap();
    assert!(unpacked.is_empty());
    reader
        .storage
        .locate_chains(&ids[1..], &mut plain, &mut chains)
        .unwrap();
    assert_eq!(each, plain);
}

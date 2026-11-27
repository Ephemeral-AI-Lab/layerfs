//! #818 M3 review: a call's per-object reads ([`Cached`]'s
//! `read_canonical_each`) read each object once however often a demand
//! names it, give each object its own result, and keep their failure record
//! bounded: a demand that could overfill it is not made. SQLite (the
//! per-object demand is the storage reader's on both backends).
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_content::{AuthenticatedObjects, ContentError, ObjectId};
use layerfs_history::HistoryName;
use layerfs_store_service::read::{
    cached::{Cached, FULL, MEMO},
    snapshot,
};
use rt::Store;

/// An object no Store holds.
fn absent(n: u32) -> ObjectId {
    ObjectId::for_bytes(format!("absent object {n}").as_bytes())
}

#[test]
fn each_object_is_read_once_with_its_own_result_and_the_record_is_bounded() {
    let mut store = Store::sqlite("cached-each-rh");
    let source = store.dir.join("source");
    rt::build_tree(&source);
    store.init(&source, &mut |_, _| {});
    let opened = store.open("each");
    let service = &opened.service;
    let stack = service
        .store
        .published(|history| history.layer_stack_named(&HistoryName::new(rt::PROJECT).unwrap()))
        .unwrap()
        .expect("the project");
    let (reader, snap) = snapshot::open(&service.store, &stack, "main").unwrap();
    let cached = Cached::new(&reader).unwrap();
    let root = cached.read_canonical(snap.root).unwrap();

    // A named twice, then B: A's failure is A's, both times, and B is B.
    let a = absent(0);
    let each = cached.read_canonical_each(&[a, a, snap.root]).unwrap();
    assert_eq!(each.len(), 3);
    assert!(
        each[0].is_err() && each[1].is_err(),
        "A fails as A: {each:?}"
    );
    assert_eq!(each[0], each[1], "A's one failure, twice");
    assert_eq!(each[2].as_ref().ok(), Some(&root), "B is B");
    // A recorded failure is served to a later demand as a point read would
    // refuse, without reading A again.
    let point = cached.read_canonical_batch(&[a]).unwrap_err();
    assert_eq!(Err::<(), _>(point), each[0].clone().map(|_| ()));

    // The record holds at most MEMO objects: a per-object demand that could
    // overfill it is not made, and a point read still refuses as before.
    assert!(cached.room(MEMO - 1) && !cached.room(MEMO));
    let many: Vec<ObjectId> = (1..MEMO as u32).map(absent).collect();
    let each = cached.read_canonical_each(&many).unwrap();
    assert!(each.iter().all(Result::is_err), "every absent object fails");
    assert!(!cached.room(1), "the record is full");
    let past = absent(MEMO as u32 + 1);
    assert_eq!(cached.read_canonical_each(&[past]), Err(FULL));
    assert!(matches!(
        cached.read_canonical_batch(&[past]),
        Err(ContentError::MissingObject)
    ));
    // Recorded objects are still served from the record.
    assert_eq!(cached.read_canonical_each(&[a]).unwrap().len(), 1);
    opened.stop();
}

//! A difference that takes the full walk keeps that answer for its later
//! pages, as a planned one keeps its plan (#364 item 4): before, only plans
//! were kept, so every page of a difference too large to plan read the inode
//! tables up to `MAXIMUM_CHANGED` changes again before walking.
use layerfs_content::compare::Plan;
use layerfs_content::ObjectId;
use layerfs_store_service::read::plans::{find, keep, PLANS};
use std::sync::Arc;

fn id(n: u8) -> ObjectId {
    let mut bytes = [n; 32];
    bytes[31] = 1;
    ObjectId::from_bytes(&bytes).unwrap()
}

#[test]
fn a_full_walk_answer_is_kept_like_a_plan_and_let_go_like_one() {
    let unplanned = (id(1), id(2));
    let planned = (id(3), id(4));
    assert!(find(unplanned).is_none(), "nothing kept yet");
    keep(unplanned, None);
    keep(planned, Some(Arc::new(Plan::default())));
    assert!(matches!(find(unplanned), Some(None)), "full walk kept");
    assert!(matches!(find(planned), Some(Some(_))), "plan kept");
    // The least recently used answer goes first once the cache is full.
    for n in 0..PLANS as u8 {
        keep((id(10 + n), id(40 + n)), None);
    }
    assert!(find(unplanned).is_none() && find(planned).is_none());
}

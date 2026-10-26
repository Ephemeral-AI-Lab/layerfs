//! Plans of recent Commit differences, kept for their later pages (#352 F-012).
//!
//! A difference's plan (its changed entries) depends only on its two immutable
//! roots, so a page of the same two roots reuses it instead of planning the
//! whole difference again. At most [`PLANS`] plans are kept, least recently
//! used first out; each is bounded by Content's `MAXIMUM_CHANGED`. A
//! difference beyond it, or with an inode whose bindings are not all
//! recorded, takes the full walk: that answer is kept as `None` the same way,
//! so its later pages do not plan again (#364 item 4). A plan that was let go
//! is made again by the page that needs it.

use layerfs_content::compare::Plan;
use layerfs_content::ObjectId;
use std::sync::{Arc, Mutex};

/// Plans kept at once.
pub const PLANS: usize = 4;

type Key = (ObjectId, ObjectId);

/// A kept plan, or `None` where the difference takes the full walk.
pub type Kept = Option<Arc<Plan>>;

static RECENT: Mutex<Vec<(Key, Kept)>> = Mutex::new(Vec::new());

/// The kept answer of `key`, now the most recently used; `None` when none
/// is kept.
pub fn find(key: Key) -> Option<Kept> {
    let mut recent = RECENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let index = recent.iter().position(|(kept, _)| *kept == key)?;
    let entry = recent.remove(index);
    let plan = entry.1.clone();
    recent.push(entry);
    Some(plan)
}

/// Keeps `plan` for `key`, letting the least recently used one go when full.
pub fn keep(key: Key, plan: Kept) {
    let mut recent = RECENT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    recent.retain(|(kept, _)| *kept != key);
    if recent.len() == PLANS {
        recent.remove(0);
    }
    recent.push((key, plan));
}

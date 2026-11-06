//! Ordered difference of two immutable sorted trees that skips shared pages.
//!
//! Both trees are content-addressed B+trees in one canonical page grammar, so a
//! child page with the same identity on both sides holds the same rows. The
//! walk keeps one ordered frontier per side and expands a page only when its
//! identity differs from the page facing it. Unchanged subtrees are never read:
//! the pages read grow with the changed rows times the tree depth, not with the
//! size of either tree. Resident state is the two frontiers, bounded by depth
//! times page fan-out, and at most [`READ_AHEAD_PAGES`] pages read ahead.
//!
//! A page to expand is read together with the pages that follow it on its
//! side at the same level and that the walk will certainly expand too, in one
//! batched demand (#95): a Store then locates them and their pooled values in
//! one round trip instead of one per page. Every page read ahead is expanded
//! exactly once, so nothing is read that the walk would have skipped.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::{MAXIMUM_PAGE_BYTES, MAXIMUM_TREE_LEVEL};
use crate::filesystem::sorted::format::Format;
use crate::object::{AuthenticatedObjects, ObjectId};

/// Work one difference walk performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DifferenceWork {
    /// Pages read and decoded.
    pub pages_read: u64,
    /// Pages skipped because both sides held the same identity.
    pub pages_shared: u64,
    /// Rows compared.
    pub rows_compared: u64,
}

impl DifferenceWork {
    /// Adds another walk's counts.
    pub fn add(&mut self, other: Self) {
        self.pages_read = self.pages_read.saturating_add(other.pages_read);
        self.pages_shared = self.pages_shared.saturating_add(other.pages_shared);
        self.rows_compared = self.rows_compared.saturating_add(other.rows_compared);
    }
}

/// Pages one walk keeps read ahead at most, so their bodies stay within
/// `READ_AHEAD_PAGES * MAXIMUM_PAGE_BYTES` (1 MiB); also the most one
/// batched demand asks for.
pub const READ_AHEAD_PAGES: usize = 128;
const _: () = assert!(READ_AHEAD_PAGES * MAXIMUM_PAGE_BYTES <= 1024 * 1024);

enum Item<K, V> {
    Page { id: ObjectId, level: u8 },
    Row { key: K, value: V },
}

/// Calls `visit(key, left, right)` once for every key whose value differs,
/// in key order. An absent tree (`None`) is empty.
pub(crate) fn difference<F: Format>(
    reader: &dyn AuthenticatedObjects,
    left: Option<ObjectId>,
    right: Option<ObjectId>,
    work: &mut DifferenceWork,
    mut visit: impl FnMut(&F::Key, Option<&F::Value>, Option<&F::Value>) -> ContentResult<()>,
) -> ContentResult<()>
where
    F::Value: PartialEq,
{
    let start = |root: Option<ObjectId>| {
        root.map(|id| Item::Page {
            id,
            level: MAXIMUM_TREE_LEVEL,
        })
        .into_iter()
        .collect::<VecDeque<Item<F::Key, F::Value>>>()
    };
    let (mut l, mut r) = (start(left), start(right));
    let mut ahead = HashMap::new();
    loop {
        let expand = match (l.front(), r.front()) {
            (None, None) => return Ok(()),
            (Some(Item::Page { id: a, .. }), Some(Item::Page { id: b, .. })) if a == b => {
                l.pop_front();
                r.pop_front();
                work.pages_shared = work.pages_shared.saturating_add(1);
                continue;
            }
            (Some(Item::Page { level: a, .. }), Some(Item::Page { level: b, .. })) => {
                (a >= b, b >= a)
            }
            (Some(Item::Page { .. }), _) => (true, false),
            (_, Some(Item::Page { .. })) => (false, true),
            (Some(Item::Row { key: a, value: x }), Some(Item::Row { key: b, value: y })) => {
                work.rows_compared = work.rows_compared.saturating_add(1);
                match a.cmp(b) {
                    std::cmp::Ordering::Less => {
                        visit(a, Some(x), None)?;
                        l.pop_front();
                    }
                    std::cmp::Ordering::Greater => {
                        visit(b, None, Some(y))?;
                        r.pop_front();
                    }
                    std::cmp::Ordering::Equal => {
                        if x != y {
                            visit(a, Some(x), Some(y))?;
                        }
                        l.pop_front();
                        r.pop_front();
                    }
                }
                continue;
            }
            (Some(Item::Row { key, value }), None) => {
                visit(key, Some(value), None)?;
                l.pop_front();
                continue;
            }
            (None, Some(Item::Row { key, value })) => {
                visit(key, None, Some(value))?;
                r.pop_front();
                continue;
            }
        };
        if expand.0 {
            open::<F>(reader, &mut l, &r, &mut ahead, work)?;
        }
        if expand.1 {
            open::<F>(reader, &mut r, &l, &mut ahead, work)?;
        }
    }
}

/// Replaces the front page with its rows or child pages, in key order. A
/// page not read ahead is read with the pages [`ahead_of`] adds.
fn open<F: Format>(
    reader: &dyn AuthenticatedObjects,
    side: &mut VecDeque<Item<F::Key, F::Value>>,
    other: &VecDeque<Item<F::Key, F::Value>>,
    ahead: &mut HashMap<ObjectId, Vec<u8>>,
    work: &mut DifferenceWork,
) -> ContentResult<()> {
    let Some(Item::Page { id, level }) = side.pop_front() else {
        return Err(ContentError::InvalidRecord("difference frontier"));
    };
    let body = match ahead.remove(&id) {
        Some(body) => body,
        None => {
            let room = READ_AHEAD_PAGES.saturating_sub(ahead.len());
            let ids = ahead_of(id, level, side, other, ahead, room);
            if ids.len() == 1 {
                reader.read_canonical(id)?
            } else {
                let bodies = reader.read_canonical_batch(&ids)?;
                if bodies.len() != ids.len() {
                    return Err(ContentError::InvalidRecord("read-ahead cardinality"));
                }
                let mut bodies = ids.into_iter().zip(bodies);
                let (_, first) = bodies
                    .next()
                    .ok_or(ContentError::InvalidRecord("read-ahead cardinality"))?;
                ahead.extend(bodies);
                first
            }
        }
    };
    let wire = F::decode(&body)?;
    work.pages_read = work.pages_read.saturating_add(1);
    for entry in wire.entries.into_iter().rev() {
        let item = match (entry.child, entry.value) {
            (Some(child), None) if wire.level > 0 => Item::Page {
                id: child,
                level: wire.level - 1,
            },
            (None, Some(value)) if wire.level == 0 => Item::Row {
                key: entry.key,
                value,
            },
            _ => return Err(ContentError::InvalidRecord("sorted page row")),
        };
        side.push_front(item);
    }
    Ok(())
}

/// `id`, then the pages after it on `side` at the same `level` that the walk
/// will certainly expand, at most `room` in all. Only when no page of `other`
/// is above `level` can no hidden page of `other` share an identity with one
/// of them (equal identities are equal pages, so equal levels); a page whose
/// identity `other` shows now may be skipped as shared, so it is left out.
/// Every page taken then meets rows, lower pages or a different page and is
/// expanded. Stops at the first row or page of another level.
fn ahead_of<K, V>(
    id: ObjectId,
    level: u8,
    side: &VecDeque<Item<K, V>>,
    other: &VecDeque<Item<K, V>>,
    ahead: &HashMap<ObjectId, Vec<u8>>,
    room: usize,
) -> Vec<ObjectId> {
    let mut ids = vec![id];
    if room <= 1 {
        return ids;
    }
    let mut facing = HashSet::new();
    for item in other {
        if let Item::Page { id, level: above } = item {
            if *above > level {
                return ids;
            }
            facing.insert(*id);
        }
    }
    for item in side {
        match item {
            Item::Page {
                id: next,
                level: same,
            } if *same == level => {
                if ids.len() == room {
                    break;
                }
                if !facing.contains(next) && !ahead.contains_key(next) && !ids.contains(next) {
                    ids.push(*next);
                }
            }
            _ => break,
        }
    }
    ids
}

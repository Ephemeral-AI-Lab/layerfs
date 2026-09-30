//! Bounded retained mapping pages, independent of current navigation owners.
use std::collections::HashMap;

use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

use super::{decode_node_with_context, ExtentNode, MAX_NODE_OBJECT_BYTES, READ_NAVIGATION_WAVE};

/// Pages an ordinary cursor may retain between navigation waves.
pub const READ_NAVIGATION_CACHE_PAGES: usize = 2 * READ_NAVIGATION_WAVE;

/// Mapping pages retained for reuse while their entries remain in this cache.
///
/// Count and incoming Vec capacity are enforced at retention. The hash table's
/// metadata allowance and the provider's upstream allocations have separate
/// resource qualification; this cache is not a global or physical memory guard.
pub struct PageCache {
    pages: HashMap<(ObjectId, bool), Vec<u8>>,
    limit: usize,
}

/// Exact canonical ownership plus codec facts from its checked root context.
/// Private fields keep raw callers from bypassing the decoder before retention.
pub(crate) struct CheckedPage {
    canonical: Vec<u8>,
    root: bool,
    level: u8,
    logical_len: u64,
    extent_count: u64,
}

impl CheckedPage {
    pub(crate) fn canonical(&self) -> &[u8] {
        &self.canonical
    }

    pub(crate) fn check_summary(
        &self,
        level: u8,
        expected: Option<(u64, u64)>,
    ) -> ContentResult<()> {
        if self.level != level {
            return Err(ContentError::InvalidRecord("mapping level"));
        }
        if let Some((logical_len, extent_count)) = expected {
            if self.logical_len != logical_len || self.extent_count != extent_count {
                return Err(ContentError::InvalidRecord("mapping coverage"));
            }
        }
        Ok(())
    }

    pub(crate) fn copy_for(&self, root: bool) -> ContentResult<Self> {
        let canonical = PageCache::copy_page(&self.canonical)?;
        if root != self.root {
            return PageCache::decode_owned(canonical, root).map(|(page, _)| page);
        }
        Ok(Self {
            canonical,
            root,
            level: self.level,
            logical_len: self.logical_len,
            extent_count: self.extent_count,
        })
    }
}

impl Default for PageCache {
    fn default() -> Self {
        Self::new()
    }
}

impl PageCache {
    /// Cache bounded by the ordinary 64-page retained allowance.
    pub fn new() -> Self {
        Self::bounded(READ_NAVIGATION_CACHE_PAGES)
    }

    /// Cache bounded by `limit` pages; zero retains its one-page meaning.
    pub fn bounded(limit: usize) -> Self {
        Self {
            pages: HashMap::new(),
            limit: limit.max(1),
        }
    }

    /// The authenticated page `id` under `root` context, when retained.
    pub fn get(&self, id: ObjectId, root: bool) -> Option<&[u8]> {
        self.pages.get(&(id, root)).map(Vec::as_slice)
    }

    /// Checks and retains one authenticated mapping page.
    ///
    /// The supplying caller/provider owns identity authentication. This boundary
    /// validates canonical width, actual Vec capacity and root-context grammar
    /// before retention; replacement obeys the same owner checks as insertion.
    pub fn insert(&mut self, id: ObjectId, root: bool, canonical: Vec<u8>) -> ContentResult<()> {
        let (page, _) = Self::decode_owned(canonical, root)?;
        self.retain(id, page)
    }

    /// Prospective wholesale eviction; retention enforces the bound itself.
    pub fn make_room_for(&mut self, incoming: usize) {
        if self
            .pages
            .len()
            .checked_add(incoming)
            .is_none_or(|count| count > self.limit)
        {
            self.pages.clear();
        }
    }

    /// True when the cache retains no page owners.
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub(crate) fn decode_owned(
        canonical: Vec<u8>,
        root: bool,
    ) -> ContentResult<(CheckedPage, ExtentNode)> {
        Self::check_owner(&canonical)?;
        let node = decode_node_with_context(&canonical, root)?;
        let page = CheckedPage {
            canonical,
            root,
            level: node.level(),
            logical_len: node.logical_len(),
            extent_count: node.extent_count(),
        };
        Ok((page, node))
    }

    /// Retains bytes whose exact context/summary the calling pass has checked.
    pub(crate) fn retain(&mut self, id: ObjectId, page: CheckedPage) -> ContentResult<()> {
        let CheckedPage {
            canonical, root, ..
        } = page;
        Self::check_owner(&canonical)?;
        if !self.pages.contains_key(&(id, root)) {
            self.make_room_for(1);
            self.pages
                .try_reserve(1)
                .map_err(|_| ContentError::ResourceUnavailable {
                    what: "mapping.cache_table",
                })?;
        }
        self.pages.insert((id, root), canonical);
        Ok(())
    }

    pub(crate) fn copy_page(canonical: &[u8]) -> ContentResult<Vec<u8>> {
        if canonical.len() > MAX_NODE_OBJECT_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: MAX_NODE_OBJECT_BYTES,
                actual: canonical.len(),
            });
        }
        let mut owned = Vec::new();
        owned.try_reserve_exact(canonical.len()).map_err(|_| {
            ContentError::ResourceUnavailable {
                what: "mapping.page_copy",
            }
        })?;
        owned.extend_from_slice(canonical);
        Self::check_owner(&owned)?;
        Ok(owned)
    }

    fn check_owner(canonical: &Vec<u8>) -> ContentResult<()> {
        if canonical.len() > MAX_NODE_OBJECT_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: MAX_NODE_OBJECT_BYTES,
                actual: canonical.len(),
            });
        }
        if canonical.capacity() > MAX_NODE_OBJECT_BYTES {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "mapping.page_capacity",
                limit: MAX_NODE_OBJECT_BYTES as u64,
                actual: canonical.capacity() as u64,
            });
        }
        Ok(())
    }
}

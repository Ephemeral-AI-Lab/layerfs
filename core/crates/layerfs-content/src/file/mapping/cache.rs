//! Bounded retained mapping pages, independent of current navigation owners.
use std::collections::HashMap;

use crate::error::{ContentError, ContentResult};
use crate::object::{CanonicalBudget, CanonicalBuffer, ObjectId};

use super::cache_table::{CacheTableLayout, CacheTableLease, CacheTableMemory};
use super::{decode_node_with_context, ExtentNode, MAX_NODE_OBJECT_BYTES, READ_NAVIGATION_WAVE};

/// Pages an ordinary cursor may retain between navigation waves.
pub const READ_NAVIGATION_CACHE_PAGES: usize = 2 * READ_NAVIGATION_WAVE;

/// Mapping pages retained for reuse while their entries remain in this cache.
///
/// Count and incoming Vec capacity are enforced at retention. The hash table's
/// metadata allowance and the provider's upstream allocations have separate
/// resource qualification; this cache is not a global or physical memory guard.
pub struct PageCache {
    pages: HashMap<(ObjectId, bool), CanonicalBuffer>,
    limit: usize,
    budget: CanonicalBudget,
    table_memory: CacheTableMemory,
    // Last: map allocation/data are destroyed before its exclusive table credit.
    table_lease: Option<CacheTableLease>,
}

/// Exact canonical ownership plus codec facts from its checked root context.
/// Private fields keep raw callers from bypassing the decoder before retention.
pub(crate) struct CheckedPage {
    canonical: PageBytes,
    root: bool,
    level: u8,
    logical_len: u64,
    extent_count: u64,
}
enum PageBytes {
    Raw(Vec<u8>),
    Leased(CanonicalBuffer),
}
impl PageBytes {
    fn as_slice(&self) -> &[u8] {
        match self {
            Self::Raw(bytes) => bytes,
            Self::Leased(bytes) => bytes.as_slice(),
        }
    }
    fn capacity(&self) -> usize {
        match self {
            Self::Raw(bytes) => bytes.capacity(),
            Self::Leased(bytes) => bytes.capacity(),
        }
    }
    fn copy(&self) -> ContentResult<Self> {
        match self {
            Self::Raw(bytes) => PageCache::copy_page(bytes).map(Self::Raw),
            Self::Leased(bytes) => bytes.try_clone().map(Self::Leased),
        }
    }
}

impl CheckedPage {
    pub(crate) fn canonical(&self) -> &[u8] {
        self.canonical.as_slice()
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
        let canonical = self.canonical.copy()?;
        if root != self.root {
            return PageCache::decode_page(canonical, root).map(|(page, _)| page);
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
        Self::with_budget(limit, CanonicalBudget::compatibility())
    }

    /// Select shared returned/copied canonical data credit before cache use.
    /// This does not qualify HashMap metadata, decoded data or provider work.
    pub fn with_budget(limit: usize, budget: CanonicalBudget) -> Self {
        Self {
            pages: HashMap::new(),
            limit: limit.max(1),
            budget,
            table_memory: CacheTableMemory::new(),
            table_lease: None,
        }
    }

    /// Admission precedes first provider/copy/decode and allocates the table once.
    pub(crate) fn prepare(&mut self) -> ContentResult<()> {
        if self.table_lease.is_some() {
            return Ok(());
        }
        let layout = CacheTableLayout::new(self.limit)?;
        let lease = self.table_memory.reserve(layout.allocation_bytes)?;
        let mut pages = HashMap::new();
        pages
            .try_reserve(self.limit)
            .map_err(|_| ContentError::ResourceUnavailable {
                what: "mapping.cache_table",
            })?;
        if pages.capacity() != layout.capacity {
            return Err(ContentError::UnsupportedPolicy {
                field: "mapping pinned HashMap layout",
            });
        }
        self.pages = pages;
        self.table_lease = Some(lease);
        Ok(())
    }
    /// Compiled table allocation class for this exact retained page count.
    pub fn table_layout(&self) -> ContentResult<CacheTableLayout> {
        CacheTableLayout::new(self.limit)
    }
    /// Continuing metadata authority for actual map/drop observation.
    pub fn table_memory(&self) -> &CacheTableMemory {
        &self.table_memory
    }

    /// The authenticated page `id` under `root` context, when retained.
    pub fn get(&self, id: ObjectId, root: bool) -> Option<&[u8]> {
        self.pages.get(&(id, root)).map(CanonicalBuffer::as_slice)
    }

    /// Real canonical-data authority used to fund provider results and copies.
    pub fn budget(&self) -> &CanonicalBudget {
        &self.budget
    }

    /// Borrow the actual retained canonical allocation and its copy authority.
    pub fn get_owned(&self, id: ObjectId, root: bool) -> Option<&CanonicalBuffer> {
        self.pages.get(&(id, root))
    }

    /// Retain a supplied lease funded by this exact cache data authority.
    pub fn insert_owned(
        &mut self,
        id: ObjectId,
        root: bool,
        canonical: CanonicalBuffer,
    ) -> ContentResult<()> {
        self.prepare()?;
        let (page, _) = Self::decode_buffer(canonical, root)?;
        self.retain(id, page)
    }

    /// Checks and retains one authenticated mapping page.
    ///
    /// The supplying caller/provider owns identity authentication. This boundary
    /// validates canonical width, actual Vec capacity and root-context grammar
    /// before retention; replacement obeys the same owner checks as insertion.
    pub fn insert(&mut self, id: ObjectId, root: bool, canonical: Vec<u8>) -> ContentResult<()> {
        self.prepare()?;
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
        Self::decode_page(PageBytes::Raw(canonical), root)
    }

    pub(crate) fn decode_buffer(
        canonical: CanonicalBuffer,
        root: bool,
    ) -> ContentResult<(CheckedPage, ExtentNode)> {
        Self::decode_page(PageBytes::Leased(canonical), root)
    }

    fn decode_page(canonical: PageBytes, root: bool) -> ContentResult<(CheckedPage, ExtentNode)> {
        Self::check_owner(canonical.as_slice(), canonical.capacity())?;
        let node = decode_node_with_context(canonical.as_slice(), root)?;
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
        self.prepare()?;
        let CheckedPage {
            canonical, root, ..
        } = page;
        Self::check_owner(canonical.as_slice(), canonical.capacity())?;
        let canonical = match canonical {
            PageBytes::Raw(bytes) => CanonicalBuffer::compatibility(bytes, &self.budget)?,
            PageBytes::Leased(bytes) => {
                if !bytes.budget().same_authority(&self.budget) {
                    return Err(ContentError::InvalidRecord(
                        "mapping foreign canonical budget",
                    ));
                }
                bytes
            }
        };
        if !self.pages.contains_key(&(id, root)) {
            self.make_room_for(1);
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
        Self::check_owner(&owned, owned.capacity())?;
        Ok(owned)
    }

    fn check_owner(canonical: &[u8], capacity: usize) -> ContentResult<()> {
        if canonical.len() > MAX_NODE_OBJECT_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: MAX_NODE_OBJECT_BYTES,
                actual: canonical.len(),
            });
        }
        if capacity > MAX_NODE_OBJECT_BYTES {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "mapping.page_capacity",
                limit: MAX_NODE_OBJECT_BYTES as u64,
                actual: capacity as u64,
            });
        }
        Ok(())
    }
}

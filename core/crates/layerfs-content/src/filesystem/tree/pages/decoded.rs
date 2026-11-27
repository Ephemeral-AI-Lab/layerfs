//! Decoded inode and directory pages kept for reuse (#364 item 3).
//!
//! A read decodes a whole canonical page to find one row. A provider may
//! keep the decoded rows of pages it already authenticated, so a later read
//! of the same immutable page skips the copy and the decode. Nothing else is
//! skipped: the page-intrinsic checks ran when the page was decoded, and
//! every check that depends on where a read found the page (root or not,
//! the level and largest key its parent summarized, the left sibling's key)
//! runs again on every use. The cache holds a fixed budget of charged bytes
//! and releases the least recently used pages first; an evicted page costs
//! the same read and decode again and nothing else. No read needs it.
use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::MAXIMUM_PAGE_BYTES;
use crate::filesystem::name::path::PathName;
use crate::filesystem::tree::pages::format::{
    CompactDirectory, CompactInodes, Format, Wire, WireEntry,
};
use crate::object::inode_leaf::InodeValue;
use crate::object::{AuthenticatedObjects, ObjectId};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

/// Charged bytes of decoded pages one daemon keeps: a fixed accounting of
/// the stored rows (see [`DecodedPages::charge`]), not a measured allocation.
pub const DECODED_PAGE_BYTES: usize = 16 * 1024 * 1024;
/// What one kept page is charged beyond its rows: its map entries and ages.
const PAGE_CHARGE: usize = 128;

/// Cumulative observations of one cache.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DecodedWork {
    /// Page reads answered from a kept decode.
    pub hits: u64,
    /// Page reads that decoded canonical bytes.
    pub misses: u64,
    /// Pages released to stay within the budget.
    pub evictions: u64,
    /// Charged bytes kept now.
    pub charged: usize,
    /// Pages kept now.
    pub pages: usize,
}

/// One decoded page.
pub(crate) enum Decoded {
    Inodes(Wire<u64, InodeValue>),
    Directory(Wire<PathName, u64>),
}
/// The grammar a read decodes a page with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Role {
    Inodes,
    Directory,
}
impl Role {
    fn decode(self, canonical: &[u8]) -> ContentResult<Decoded> {
        Ok(match self {
            Self::Inodes => Decoded::Inodes(CompactInodes::decode(canonical)?),
            Self::Directory => Decoded::Directory(CompactDirectory::decode(canonical)?),
        })
    }
    fn of(page: &Decoded) -> Self {
        match page {
            Decoded::Inodes(_) => Self::Inodes,
            Decoded::Directory(_) => Self::Directory,
        }
    }
}

#[derive(Default)]
struct State {
    pages: HashMap<ObjectId, (Arc<Decoded>, u64, usize)>,
    ages: BTreeMap<u64, ObjectId>,
    epoch: u64,
    work: DecodedWork,
}

/// Decoded pages by object identity, within a fixed budget.
pub struct DecodedPages {
    budget: usize,
    state: Mutex<State>,
}
impl DecodedPages {
    /// An empty cache of at most `budget` charged bytes.
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            state: Mutex::new(State::default()),
        }
    }
    /// Cumulative counters and the current charge.
    pub fn work(&self) -> DecodedWork {
        self.state
            .lock()
            .map_or_else(|_| DecodedWork::default(), |state| state.work)
    }
    fn state(&self) -> ContentResult<std::sync::MutexGuard<'_, State>> {
        self.state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "decoded page cache",
            })
    }
    /// The kept page of `id` when it was decoded with `role`'s grammar.
    fn get(&self, id: ObjectId, role: Role) -> ContentResult<Option<Arc<Decoded>>> {
        let mut state = self.state()?;
        let State {
            pages,
            ages,
            epoch,
            work,
        } = &mut *state;
        let Some((page, age, _)) = pages
            .get_mut(&id)
            .filter(|(page, ..)| Role::of(page) == role)
        else {
            return Ok(None);
        };
        ages.remove(age);
        *epoch += 1;
        *age = *epoch;
        ages.insert(*epoch, id);
        work.hits += 1;
        Ok(Some(page.clone()))
    }
    fn put(&self, id: ObjectId, page: Arc<Decoded>) -> ContentResult<()> {
        let charge = Self::charge(&page);
        let mut state = self.state()?;
        state.work.misses += 1;
        if charge > self.budget || state.pages.contains_key(&id) {
            return Ok(());
        }
        while state.work.charged + charge > self.budget {
            let Some((_, oldest)) = state.ages.pop_first() else {
                break;
            };
            if let Some((_, _, released)) = state.pages.remove(&oldest) {
                state.work.charged -= released;
                state.work.evictions += 1;
            }
        }
        state.epoch += 1;
        let age = state.epoch;
        state.ages.insert(age, id);
        state.pages.insert(id, (page, age, charge));
        state.work.charged += charge;
        state.work.pages = state.pages.len();
        Ok(())
    }
    /// The rows' inline width plus every name's bytes, and [`PAGE_CHARGE`].
    fn charge(page: &Decoded) -> usize {
        fn rows<K, V>(wire: &Wire<K, V>, heap: impl Fn(&K) -> usize) -> usize {
            wire.entries.len() * std::mem::size_of::<WireEntry<K, V>>()
                + wire
                    .entries
                    .iter()
                    .map(|entry| heap(&entry.key))
                    .sum::<usize>()
        }
        PAGE_CHARGE
            + match page {
                Decoded::Inodes(wire) => rows(wire, CompactInodes::heap_bytes),
                Decoded::Directory(wire) => rows(wire, CompactDirectory::heap_bytes),
            }
    }
}

/// The decoded pages of `ids`, in order: kept ones from the reader's cache,
/// the rest read as one batch, size-checked, decoded with `role`'s grammar
/// and kept. Without a cache every page is read and decoded, as before.
pub(crate) fn pages(
    reader: &dyn AuthenticatedObjects,
    ids: &[ObjectId],
    role: Role,
) -> ContentResult<Vec<Arc<Decoded>>> {
    let cache = reader.decoded_pages();
    let mut found: Vec<Option<Arc<Decoded>>> = match cache {
        Some(cache) => ids
            .iter()
            .map(|id| cache.get(*id, role))
            .collect::<ContentResult<_>>()?,
        None => vec![None; ids.len()],
    };
    let missing: Vec<ObjectId> = ids
        .iter()
        .zip(&found)
        .filter(|(_, page)| page.is_none())
        .map(|(id, _)| *id)
        .collect();
    if missing.is_empty() {
        return Ok(found.into_iter().flatten().collect());
    }
    let canonical = reader.read_canonical_batch(&missing)?;
    if canonical.len() != missing.len() {
        return Err(ContentError::BatchCardinality {
            requested: missing.len(),
            returned: canonical.len(),
        });
    }
    let mut read = missing.into_iter().zip(canonical);
    for slot in found.iter_mut().filter(|slot| slot.is_none()) {
        let (id, bytes) = read.next().ok_or(ContentError::BatchCardinality {
            requested: ids.len(),
            returned: 0,
        })?;
        if bytes.len() > MAXIMUM_PAGE_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: MAXIMUM_PAGE_BYTES,
                actual: bytes.len(),
            });
        }
        let page = Arc::new(role.decode(&bytes)?);
        if let Some(cache) = cache {
            cache.put(id, page.clone())?;
        }
        *slot = Some(page);
    }
    Ok(found.into_iter().flatten().collect())
}

/// One page through [`pages`].
pub(crate) fn page(
    reader: &dyn AuthenticatedObjects,
    id: ObjectId,
    role: Role,
) -> ContentResult<Arc<Decoded>> {
    pages(reader, std::slice::from_ref(&id), role)?
        .pop()
        .ok_or(ContentError::BatchCardinality {
            requested: 1,
            returned: 0,
        })
}

impl Decoded {
    pub(crate) fn inode_wire(&self) -> ContentResult<&Wire<u64, InodeValue>> {
        match self {
            Self::Inodes(wire) => Ok(wire),
            Self::Directory(_) => Err(ContentError::InvalidRecord("decoded page role")),
        }
    }
    pub(crate) fn directory_wire(&self) -> ContentResult<&Wire<PathName, u64>> {
        match self {
            Self::Directory(wire) => Ok(wire),
            Self::Inodes(_) => Err(ContentError::InvalidRecord("decoded page role")),
        }
    }
}

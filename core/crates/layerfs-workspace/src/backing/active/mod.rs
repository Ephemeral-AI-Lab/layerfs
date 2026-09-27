//! Versioned, Workspace-owned mutable backing for the active generation.
mod extents;
mod generation;
mod index;
mod keyed;
mod pack;
mod page;
mod pages;
mod reclaim;
mod records;

pub use extents::{Extent, ExtentKind, ExtentPlan};
pub use generation::{ActiveBacking, ActiveSnapshot, ActiveStatus, ActiveWrite};
pub use index::{Index, IndexCandidate, IndexEntry, IndexSnapshot, ScanPage};
pub use pack::{PackedSlot, PreparedSlot, TinyPack};
pub use page::{Kind, Page, PageRef, PAGE_BYTES};
pub use pages::{PagePin, PageStore, StoreStatus};
pub use records::{dirty_key, inode_key, namespace_key, HotInode, NamespaceRecord};

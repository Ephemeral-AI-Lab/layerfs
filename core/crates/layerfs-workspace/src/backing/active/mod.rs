//! Versioned, Workspace-owned mutable backing for the active generation.
mod compaction;
mod extents;
mod generation;
mod hot_directory;
mod index;
mod keyed;
mod lifetime;
mod pack;
mod page;
mod pages;
mod reader;
mod reclaim;
mod records;
mod resolve;
mod splice;

pub use extents::{Extent, ExtentKind, ExtentPlan};
pub use generation::{ActiveBacking, ActivePublication, ActiveSnapshot, ActiveStatus, ActiveWrite};
pub use index::{Index, IndexCandidate, IndexEntry, IndexSnapshot, ScanPage};
pub use pack::{PackedSlot, PreparedSlot, TinyPack};
pub use page::{Kind, Page, PageRef, PAGE_BYTES};
pub use pages::{PagePin, PageStore, StoreStatus};
pub(crate) use reader::ActivePackReader;
pub use records::{dirty_key, inode_key, namespace_key, HotInode, NamespaceRecord};

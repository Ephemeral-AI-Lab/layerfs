//! Versioned, Workspace-owned mutable backing for the active generation.
mod index;
mod keyed;
mod pack;
mod page;
mod pages;

pub use index::{Index, IndexCandidate, IndexSnapshot};
pub use pack::{PackedSlot, PreparedSlot, TinyPack};
pub use page::{Kind, Page, PageRef, PAGE_BYTES};
pub use pages::{PagePin, PageStore, StoreStatus};

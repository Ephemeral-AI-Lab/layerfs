//! One shared C5 authority backed by the same Store session as physical packs,
//! on either engine.
use layerfs_history::CatalogId;
/// History operations retain separate bounded transaction acknowledgement units.
pub struct HistoryProvider {
    pub(crate) engine: crate::backend::Engine,
    pub(crate) catalog_id: CatalogId,
    pub(crate) incarnation: u64,
    pub(crate) cursor_key: [u8; 32],
    /// The Store's small-file threshold: only files at or above it (the
    /// chunked ones) keep a file digest (#91).
    pub(crate) digest_floor: u64,
    /// The Store's 32-byte `trace_salt`: the marker key is the keyed trace
    /// hash over the stack and the normalized path, never a Store-independent
    /// `BLAKE3(path)` (§4.3).
    pub(crate) trace_salt: [u8; 32],
    /// The budget every mark of this Store, and the compaction around it,
    /// sizes its buffers and caches from ([`super::objects::spill::Space`]).
    pub(crate) mark_budget: std::sync::atomic::AtomicU64,
}

impl HistoryProvider {
    /// Changes the budget the buffers and caches of this Store's marks and
    /// compactions are sized from. It only sizes them: the same steps run
    /// with every value, and what they find does not depend on it. No
    /// product caller sets it; the bounded-memory tests run the smallest
    /// budget on small Stores to cross the bounds a large Store crosses.
    #[doc(hidden)]
    pub fn set_mark_budget(&self, bytes: u64) {
        self.mark_budget
            .store(bytes, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn mark_budget(&self) -> u64 {
        self.mark_budget.load(std::sync::atomic::Ordering::Relaxed)
    }
}

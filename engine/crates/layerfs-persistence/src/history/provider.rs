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
}

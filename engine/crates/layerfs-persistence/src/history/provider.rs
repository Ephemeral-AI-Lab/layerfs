//! One shared C5 authority backed by the same Store session as physical packs,
//! on either engine.
use layerfs_history::CatalogId;
/// History operations retain separate bounded transaction acknowledgement units.
pub struct HistoryProvider {
    pub(crate) engine: crate::backend::Engine,
    pub(crate) catalog_id: CatalogId,
    pub(crate) incarnation: u64,
    pub(crate) cursor_key: [u8; 32],
}

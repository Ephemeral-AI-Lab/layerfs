//! One shared C5 authority backed by the same Store session as physical packs.
use crate::backend::Session;
use layerfs_history::CatalogId;
use std::sync::Arc;
/// History operations retain separate bounded transaction acknowledgement units.
pub struct HistoryProvider {
    pub(crate) session: Arc<Session>,
    pub(crate) catalog_id: CatalogId,
    pub(crate) incarnation: u64,
    pub(crate) cursor_key: [u8; 32],
}

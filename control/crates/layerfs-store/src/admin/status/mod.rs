//! What a Store reports: `store.get`, `store.list`, its state, formats,
//! stats, live reads and observed reach.
pub mod formats;
pub mod get;
pub mod list;
pub(crate) mod live;
pub mod observed;
pub mod state;
pub(crate) mod stats;

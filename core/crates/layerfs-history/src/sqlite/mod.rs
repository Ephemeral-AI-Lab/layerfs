//! Native SQLite provider for the history catalog.
//!
//! This module tree owns the atomic metadata boundary: one short transaction at
//! a time per catalog authority, immediate refusal when that boundary is busy,
//! and no transaction that spans a caller's upload, construction or content
//! save. Declarations and re-exports live here; the provider cell, the contract
//! surface and the statements live in the focused modules beside it.

mod allocation;
mod branch;
mod commit;
mod engine;
mod layerstack;
mod open;
mod query;
mod rows;
mod staging;

pub use engine::EngineParticipation;
pub use open::{
    create, create_participating, open_read_only, open_read_only_participating, open_writable,
    open_writable_participating, SqliteCatalog,
};

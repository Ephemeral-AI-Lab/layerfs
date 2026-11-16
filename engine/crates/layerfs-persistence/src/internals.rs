//! Test-only re-exports for unit checks that must live outside product source
//! (the product-boundary guard refuses test-only attributes in `src/`).
//!
//! Nothing in the product calls these; they exist so an integration test can
//! cover the same §7/§8 helpers the product source used to test in place.

#[cfg(feature = "postgresql")]
pub use crate::backend::postgresql::compact::{
    effective_rank, standby_satisfied, sync_name, sync_rank, Replica,
};
pub use crate::backend::sqlite::compact::{
    cap_delta_references, io_error, journal_bound_bytes, peak_temp_bytes, DELTA_BASE_REFERENCES_CAP,
};

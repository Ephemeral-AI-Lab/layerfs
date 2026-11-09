//! Pure control-slot and retained-failure values, observed by the integration
//! tests in `tests/`.
//!
//! `#[doc(hidden)]` keeps this module out of rustdoc; it is not access
//! control. The values are the daemon's state machine with no I/O seam, so
//! the only place a test can reach them is a re-export: product `src/` may
//! not carry a test-only `cfg` attribute (the product-boundary guard rejects
//! it). Keep the documented `application` exports to the startup API
//! (`Application`, `ApplicationError`, `ConnectionFailure`); anything a test
//! reads belongs here, behind this module's one name.

#[doc(hidden)]
pub use super::owner::{
    AdmissionClass, Finished, SlotClass, SlotCounts, SlotTicket, Slots, RESERVED_SLOTS,
};
#[doc(hidden)]
pub use super::retained::{RetainedFailure, RetainedFailures, RetainedSnapshot, LIMIT, TTL};

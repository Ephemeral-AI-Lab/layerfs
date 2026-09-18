//! Harness support: instruments, the trace writer and the time window.
//!
//! Nothing in this module reaches into product source. Every instrument here is an
//! external observation of a process the harness owns, which is the only place a
//! resource counter can live: a counter in product `src/` cannot fail an
//! assertion, so it is not evidence (`core/docs/architecture/10-counters.md`).

pub mod instruments;
pub mod phases;
pub mod trace;
pub mod window;

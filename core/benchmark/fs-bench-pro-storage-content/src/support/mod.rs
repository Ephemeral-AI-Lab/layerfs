//! Harness support: instruments, the trace writer and the time window.
//!
//! Nothing in this module reaches into product source. Every instrument here is an
//! external observation of a process the harness owns, which is the only place a
//! resource counter can live: a counter in product `src/` cannot fail an
//! assertion, so it is not evidence (`core/docs/architecture/10-counters.md`).

pub mod instruments;
// `phases` (with `window` and `instruments`) is a frozen first-party reference
// module: active core examples and tests compile the same file through `#[path]`
// and allow this same lint at their own include site. Its source stays unchanged.
#[allow(clippy::useless_format)]
pub mod phases;
pub mod trace;
pub mod window;

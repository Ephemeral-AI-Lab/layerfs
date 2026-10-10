//! Workload support: the digest, the provider and the independent oracle.
//!
//! The oracle is deliberately **not** built on the operation it checks. It derives
//! its expectation from the fixture recipe and from declared constants, never from
//! the mutated artifact, and it authenticates every read with an identity check
//! rather than trusting the bytes it is handed.

pub mod artifact;
// `digest`, `gitoid` and `history` (with `json`, `providers` and `oracle`) are
// frozen first-party reference modules: active core examples and tests compile
// these same files through `#[path]` and allow these same lints at their own
// include sites. Their source stays unchanged, so the lint is allowed here too.
#[allow(clippy::needless_range_loop)]
pub mod digest;
pub mod edits;
pub mod expected;
#[allow(clippy::needless_return)]
pub mod gitoid;
#[allow(clippy::type_complexity)]
pub mod history;
pub mod json;
pub mod oracle;
pub mod providers;
pub mod stream;

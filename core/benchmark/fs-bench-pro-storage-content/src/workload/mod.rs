//! Workload support: the digest, the provider and the independent oracle.
//!
//! The oracle is deliberately **not** built on the operation it checks. It derives
//! its expectation from the fixture recipe and from declared constants, never from
//! the mutated artifact, and it authenticates every read with an identity check
//! rather than trusting the bytes it is handed.

pub mod artifact;
pub mod digest;
pub mod expected;
pub mod gitoid;
pub mod history;
pub mod json;
pub mod oracle;
pub mod providers;

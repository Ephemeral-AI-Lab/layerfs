//! Stage 6 structural-complexity harness for the replacement C1/C2 core.
//!
//! The library exists so `tests/*.rs` has a seam: a binary crate cannot be
//! imported by an integration test. `main.rs` is a thin argv-to-one-op shim over
//! this library.
//!
//! Module map, and the one rule each module obeys:
//!
//! * [`registry`] — the frozen 220 rows and the cardinality self-check.
//! * [`families`] — one module per family, thin: rows only, no operation bodies.
//! * [`ops`] — the shape drivers, the only place operation bodies live.
//! * [`fixture`] — the three generators and the expected-result model.
//! * [`gates`] — pure decision logic; the highest-value test target.
//! * [`workload`] — providers, the independent oracle and the SHA-256 digest.
//! * [`support`] — instruments, the trace writer and the time window.
#![deny(missing_docs)]

pub mod families;
pub mod fixture;
pub mod gates;
pub mod ops;
pub mod registry;
pub mod support;
pub mod workload;

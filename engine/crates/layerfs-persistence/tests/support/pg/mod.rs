//! PostgreSQL test support: the test server's connection and schemas, a
//! fault-injecting proxy, old-format fixtures and a wire tap. The doctor's
//! oracle (`doctor.rs` with `conninfo`, `namespace`, `oracle_proxy`) and the
//! delta-chain fixtures (`chains.rs`) are loaded by the tests that use them.
mod connection;
#[allow(unused_imports)] // binaries that use no PostgreSQL support
pub use connection::*;
pub mod fault_proxy;
pub mod fixture;
pub mod wire_tap;

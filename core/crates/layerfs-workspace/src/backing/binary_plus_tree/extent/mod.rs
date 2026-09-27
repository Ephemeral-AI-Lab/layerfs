//! The length-indexed extent tree: format, path-copied splice and byte cursor.
pub mod cursor;
pub mod format;
pub mod splice;
mod telemetry;
pub use splice::*;

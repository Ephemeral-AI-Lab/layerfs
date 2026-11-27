//! Indexed reference reduction over a caller-owned backing: rows, windows and release.
//!
//! Entry module: declarations only.

pub(crate) mod reduce;
pub(crate) mod release;
pub(crate) mod rows;
pub(crate) mod window;
pub(crate) mod wire;

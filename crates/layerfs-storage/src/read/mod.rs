//! Private root extraction and exact-range read coordinators.
//!
//! Root traversal, file-range orchestration, and concrete occupied-object
//! reading have separate owners. None is a public SDK.

pub(crate) mod extraction;
mod object_reader;
mod range;

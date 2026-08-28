//! Legacy authenticated reads and single-publication logical mutation.

mod mutation;
mod resolution;
mod view;

pub(crate) use resolution::{namespace, resolve, ResolvedReadCache};

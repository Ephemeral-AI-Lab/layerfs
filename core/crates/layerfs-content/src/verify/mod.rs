//! Bounded contextual checks through the owning canonical decoders.

pub(crate) mod children;
pub(crate) mod decode;
mod inode;
mod mapping;
mod namespace;
pub(crate) mod window;

pub(super) use children::validate;
pub(super) use children::validate_unbound;

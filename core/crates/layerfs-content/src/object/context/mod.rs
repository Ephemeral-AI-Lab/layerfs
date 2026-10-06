//! Bounded contextual checks through the owning canonical decoders.

mod dispatch;
mod inode;
mod mapping;
mod namespace;
mod read;

pub(super) use dispatch::validate;

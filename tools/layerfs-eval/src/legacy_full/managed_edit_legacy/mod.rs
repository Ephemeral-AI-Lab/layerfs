//! Frozen managed edit recording, native mutation, and one-Publication replay.

mod native;
mod replay;
mod shift;
mod spool;
#[cfg(test)]
mod tests;

pub(crate) use native::{
    mutate_native, native_hard_link_key, rename_native, sync_pending, ManagedEdit,
};
pub(crate) use replay::replay;
pub(crate) use shift::{native_parent, shift_regular, shift_temp};
pub(crate) use spool::{spooled_metadata_len, write_spooled_metadata};

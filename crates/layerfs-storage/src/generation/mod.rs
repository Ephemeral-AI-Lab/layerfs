//! Durable installation boundary for Store generations.

pub(crate) mod cleanup;
pub(crate) mod create;
pub(crate) mod selector;
pub(crate) mod switch;

#[cfg(test)]
pub(crate) use create::test_support::{directory, InstallBehavior, TestDriver};

pub use create::{NativeGenerationDriver, StoreGenerationDriver};
pub use selector::{open_current, open_current_full_durable, StoreSelector, SELECTOR_BYTES};
pub use switch::{
    compact, compact_full_durable, open_or_create, open_or_create_with_legacy,
    restore_full_durable_backup,
};

#[cfg(test)]
mod full_tests;

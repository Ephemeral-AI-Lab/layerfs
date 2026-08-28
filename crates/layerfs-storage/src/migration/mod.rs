//! Explicit side-by-side Store-family migrations.

mod full;
mod verify;

pub use full::migrate_legacy_durable_file;
#[cfg(test)]
pub(crate) use full::{
    migrate_legacy_durable_file_fault, migrate_legacy_durable_file_with_injector,
};
pub(crate) use verify::VerifiedFullCandidate;
pub use verify::{migrate_selected_legacy_durable_generation, rollback_selected_full_generation};

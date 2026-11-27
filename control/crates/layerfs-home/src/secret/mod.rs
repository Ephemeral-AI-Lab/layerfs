//! Secrets in the home: worker and daemon keys (`keys`), the saved named
//! secrets in `secret/` (`saved`), and overwriting a secret before its
//! memory is freed (`wipe`).
pub mod keys;
pub mod saved;
pub mod wipe;

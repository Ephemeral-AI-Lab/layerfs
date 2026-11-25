//! `store upgrade` on a SQLite Store (#804 §5.1): the format read from the
//! header alone, the frozen texts of step 2 -> 3, the chain, its checks and
//! the run.
pub(crate) mod format;
pub(crate) mod frozen;
pub(crate) mod run;
pub(crate) mod step;
pub(crate) mod verify;

//! `store upgrade` on a PostgreSQL Store (#804 §5.2): message 1, the frozen
//! texts of step 3 -> 4, the chain and message 2.
pub(crate) mod check;
pub(crate) mod frozen;
pub(crate) mod run;
pub(crate) mod step;

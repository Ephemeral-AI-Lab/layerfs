pub(in crate::stage1_materialize) mod block;
pub(in crate::stage1_materialize) mod campaign;
pub(in crate::stage1_materialize) mod contract;
pub(in crate::stage1_materialize) mod equations;
pub(in crate::stage1_materialize) mod native;
pub(in crate::stage1_materialize) mod observation;
pub(in crate::stage1_materialize) mod projection;
mod run;

pub use block::{attribution_block, trusted_block};
pub use run::attribution_run;

#[cfg(test)]
mod tests;

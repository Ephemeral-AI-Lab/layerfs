pub(in crate::stage1_materialize) mod campaign;
pub(in crate::stage1_materialize) mod contract;
pub(in crate::stage1_materialize) mod disposition;
mod run;
pub(in crate::stage1_materialize) mod summary;

pub use run::acceptance_run;

#[cfg(test)]
mod tests;

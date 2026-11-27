//! Machine records: the record itself (`machine`), the built-in `native`
//! and `docker` records (`built_ins`), and the record as the adapters'
//! `Spec` (`adapter_spec`).
#[cfg(feature = "machine")]
pub(crate) mod adapter_spec;
#[cfg(feature = "machine")]
pub(crate) mod built_ins;
#[doc(hidden)]
pub mod machine;

#[doc(hidden)]
pub use machine::*; // #828 compat

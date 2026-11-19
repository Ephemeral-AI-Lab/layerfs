//! The answer the six doctor calls share (`answer`), reached as
//! `crate::doctor`: `home`, `machine`, `store`, `project`, `sandbox` and
//! `backup doctor`, each an entry of its own family (`<family>/doctor.rs`).
//! There is no `doctor` family (call surface 2.0); the leftover repair it
//! held is gone with `home.prune` (spec v20): LayerFS removes leftovers
//! itself.
pub(crate) mod answer;

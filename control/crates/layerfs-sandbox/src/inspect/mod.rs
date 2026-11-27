//! The Sandbox side of the read-only inspect calls (#179): where a request
//! runs and how many may run at once (`route`, `owner`), and the reads that
//! need a Sandbox's run folder (`overlay`, `process`). The other kinds and
//! the match on a request's kind are `layerfs-inspect`'s.
pub mod overlay;
pub mod owner;
pub mod process;
pub mod route;

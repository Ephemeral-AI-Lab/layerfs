//! The home, the control center (H §7.8): `layerfs.json`, `layerfs.lock`,
//! `machine/`, `store/`, `backup/`, `trace/` and `log/`. Only host
//! processes read or write it; guests never see it (§7.7).
mod layout;
pub mod lock;
mod migrate;
pub mod paths;
mod records;

pub use layout::{detect, ensure, HomeFile, Layout, LAYOUT_VERSION};
pub use migrate::{note_failure, open};
pub use records::{log_line, now_ms, private_dir, read_record, with_field, write_record};

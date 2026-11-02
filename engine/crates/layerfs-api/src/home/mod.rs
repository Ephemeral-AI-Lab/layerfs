//! The home, the control center (H §7.8): `layerfs.json`, `layerfs.lock`,
//! `machine/`, `store/`, `backup/`, `trace/` and `log/`. Only host
//! processes read or write it; guests never see it (§7.7).
mod import_env;
mod layout;
pub mod lock;
mod migrate;
pub mod paths;
mod place;
mod records;

pub use import_env::import_env;
pub use layout::{detect, ensure, HomeFile, Layout, LAYOUT_VERSION};
pub use migrate::{note_failure, open, open_from};
pub use records::{log_line, now_ms, private_dir, read_record, with_field, write_record};

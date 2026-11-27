//! The home, the control center (H §7.8): `layerfs.json`, `layerfs.lock`,
//! `machine/`, `store/`, `backup/`, `secret/`, `trace/` and `log/`. Only host
//! processes read or write it; guests never see it (§7.7).
mod capacity;
pub use capacity::{filesystem, filesystem_available, Filesystem};
pub mod guest;
pub mod layout;
pub(crate) mod native_names;
pub mod paths;

pub use super::record::file::{
    log_line, log_swept, now_ms, private_dir, read_record, traversable, versioned, with_field,
    write_entity, write_record, RECORD_VERSION,
};
pub use layout::{detect, ensure, open, opened, HomeFile, Layout, LAYOUT_VERSION};

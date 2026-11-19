//! The home, the control center (H §7.8): `layerfs.json`, `layerfs.lock`,
//! `machine/`, `store/`, `backup/`, `secret/`, `trace/` and `log/`. Only host
//! processes read or write it; guests never see it (§7.7).
mod capacity;
pub use capacity::{filesystem, filesystem_available, Filesystem};
pub mod entity_lock;
pub mod fields;
pub mod flock;
pub mod generation;
pub mod guest;
pub mod layout;
pub mod lock;
pub mod observed;
pub mod paths;
pub mod records;
pub mod secrets;

pub use layout::{detect, ensure, open, opened, HomeFile, Layout, LAYOUT_VERSION};
pub use records::{
    log_line, log_swept, now_ms, private_dir, read_record, traversable, versioned, with_field,
    write_entity, write_record, RECORD_VERSION,
};

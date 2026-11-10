//! The home, the control center (H §7.8): `layerfs.json`, `layerfs.lock`,
//! `machine/`, `store/`, `backup/`, `trace/` and `log/`. Only host
//! processes read or write it; guests never see it (§7.7).
mod capacity;
pub use capacity::filesystem_available;
pub mod entity_lock;
pub mod fields;
pub mod flock;
pub mod guest;
pub mod layout;
pub mod lock;
pub mod paths;
pub mod records;

pub use layout::{detect, ensure, opened, HomeFile, Layout, LAYOUT_VERSION};
pub use records::{
    log_line, now_ms, private_dir, read_record, traversable, versioned, with_field, write_entity,
    write_record, RECORD_VERSION,
};

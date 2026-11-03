//! The home, the control center (H §7.8): `layerfs.json`, `layerfs.lock`,
//! `machine/`, `store/`, `backup/`, `trace/` and `log/`. Only host
//! processes read or write it; guests never see it (§7.7).
pub mod entity_lock;
pub(crate) mod flock;
pub(crate) mod guest;
mod import_env;
mod layout;
pub mod lease;
pub mod lock;
mod migrate;
pub mod paths;
mod records;

pub use import_env::import_env;
pub use layout::{detect, ensure, HomeFile, Layout, LAYOUT_VERSION};
pub(crate) use migrate::layout_2_blockers;
pub use migrate::{note_failure, open, open_from};
pub use records::{
    log_line, now_ms, private_dir, read_record, traversable, versioned, with_field, write_entity,
    write_record, RECORD_VERSION,
};

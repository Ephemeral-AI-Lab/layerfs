//! The home, the control center (H §7.8): `layerfs.json`, `layerfs.lock`,
//! `machine/`, `store/`, `backup/`, `trace/` and `log/`. Only host
//! processes read or write it; guests never see it (§7.7).
mod layout;
mod records;

pub use layout::{detect, ensure, HomeFile, Layout, LAYOUT_VERSION};
pub use records::{private_dir, read_record, write_record};

//! macOS Darwin process and mount facts slot; implemented by the macOS native backend.
use crate::backend::native::process::Stat;
use std::{
    io,
    path::{Path, PathBuf},
};

const UNSUPPORTED: &str = "macOS native process and mount table";

pub(crate) fn stat(_pid: u32) -> io::Result<Stat> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

pub(crate) fn processes() -> io::Result<Vec<(u32, Stat)>> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

pub(crate) fn boot() -> io::Result<u128> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

pub(crate) fn mount_points() -> io::Result<Vec<PathBuf>> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

pub(crate) fn force_unmount(_point: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

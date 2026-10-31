//! Linux mount table and lazy detach slot; implemented by the Linux native backend.
use std::{
    io,
    path::{Path, PathBuf},
};

const UNSUPPORTED: &str = "Linux native mount table";

pub(crate) fn under(_directory: &Path) -> io::Result<Vec<PathBuf>> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

pub(crate) fn filesystem(_path: &Path) -> io::Result<String> {
    Err(io::Error::new(io::ErrorKind::Unsupported, UNSUPPORTED))
}

pub(crate) fn detach(directory: &Path) -> io::Result<()> {
    let _ = filesystem(directory);
    under(directory).map(drop)
}

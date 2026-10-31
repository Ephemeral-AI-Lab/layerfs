//! macOS mount table and forced detach slot; implemented by the macOS native backend.
use std::{
    io,
    path::{Path, PathBuf},
};

pub(crate) fn under(_directory: &Path) -> io::Result<Vec<PathBuf>> {
    super::mount_points()
}

pub(crate) fn detach(directory: &Path) -> io::Result<()> {
    for point in under(directory)? {
        super::force_unmount(&point)?;
    }
    Ok(())
}

//! Exclusive temporary-file ownership and positive-progress writes, no retries.
use crate::install_types::InstallWork;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

pub(super) fn paths(destination: &Path) -> io::Result<(PathBuf, PathBuf)> {
    if !destination.is_absolute()
        || destination
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "absolute normalized Store destination required",
        ));
    }
    let name = destination
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Store file name"))?;
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Store parent"))?
        .canonicalize()?;
    let mut temporary = name.to_os_string();
    temporary.push(".installing");
    Ok((parent.join(name), parent.join(temporary)))
}
pub(super) fn absent(destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Store destination exists",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
pub(super) fn claim(destination: &Path, temporary: &Path) -> io::Result<File> {
    absent(destination)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(temporary)
}
pub(super) fn write(file: &mut File, mut bytes: &[u8], work: &mut InstallWork) -> io::Result<()> {
    while !bytes.is_empty() {
        work.write_calls = work.write_calls.saturating_add(1);
        let size = file.write(bytes)?;
        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "Store install write",
            ));
        }
        work.written = work.written.saturating_add(size as u64);
        bytes = &bytes[size..];
    }
    Ok(())
}

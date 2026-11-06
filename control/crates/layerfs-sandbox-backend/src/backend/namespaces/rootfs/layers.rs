//! What one layer does to the unpacked tree: a whiteout hides what the layers
//! below placed, and a regular file, directory, symbol link or hard link is
//! written inside the root — never through a symbolic link, and never outside
//! the root it belongs to.
use super::{
    refuse,
    tar::{Entry, Kind},
};
use crate::RuntimeError;
use nix::fcntl::OFlag;
use std::{
    ffi::OsStr,
    fs::{self, DirBuilder, OpenOptions},
    io,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
};

/// One entry, applied to the unpacked tree: a whiteout, or a file, directory,
/// symbol link or hard link inside it.
pub(super) fn apply(
    root: &Path,
    entry: &Entry,
    data: &mut dyn io::Read,
) -> Result<(), RuntimeError> {
    let (Some(parent), Some(name)) = (entry.path.parent(), entry.path.file_name()) else {
        return Err(refuse("the image archive's entry has no name"));
    };
    let name = name.as_bytes();
    if entry.kind == Kind::Regular && name.starts_with(b".wh.") {
        return whiteout(root, parent, &name[b".wh.".len()..]);
    }
    let target = beneath_root(root, &entry.path)?;
    match &entry.kind {
        Kind::Directory => make_directory(&target, entry.mode),
        Kind::Regular => {
            remove_any(&target)?;
            write_file(&target, entry.mode, data)
        }
        Kind::Symlink(link) => {
            remove_any(&target)?;
            std::os::unix::fs::symlink(link, &target)?;
            Ok(())
        }
        Kind::HardLink(link) => {
            let source = beneath_root(root, link)?;
            if !fs::symlink_metadata(&source)?.file_type().is_file() {
                return Err(refuse(format!(
                    "the image archive links {} to {}, which is not a file in it",
                    entry.path.display(),
                    link.display()
                )));
            }
            remove_any(&target)?;
            fs::hard_link(&source, &target)?;
            Ok(())
        }
    }
}

/// Applies `.wh.`/`.wh..wh..opq` to `root`: the named entry, or everything in
/// the directory, is hidden from the layers below. A whiteout with nothing
/// below it removes nothing.
fn whiteout(root: &Path, parent: &Path, name: &[u8]) -> Result<(), RuntimeError> {
    // a whiteout hides what a lower layer placed; it never creates the
    // directory it names, so a layer that only hides a path changes nothing
    let Some(directory) = beneath_existing(root, parent)? else {
        return Ok(());
    };
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(refuse(
                "the image archive's whiteout is outside a directory",
            ))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    if name == b".wh..opq" {
        for entry in fs::read_dir(&directory)? {
            remove_any(&entry?.path())?;
        }
        return Ok(());
    }
    if name.starts_with(b".wh..wh.") {
        // aufs bookkeeping of an opaque directory's own metadata: not content
        return Ok(());
    }
    if name.is_empty() || name.contains(&b'/') {
        return Err(refuse("the image archive's whiteout name"));
    }
    let hidden = directory.join(OsStr::from_bytes(name));
    match fs::symlink_metadata(&hidden) {
        Ok(_) => remove_any(&hidden),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// `relative`'s place inside `root`, with every directory above it verified to
/// be one of this archive's own directories and never a symbolic link: an
/// entry can then only ever write inside the root it belongs to.
fn beneath_root(root: &Path, relative: &Path) -> Result<PathBuf, RuntimeError> {
    let components: Vec<Component> = relative.components().collect();
    if components.is_empty() {
        return Err(refuse("the image archive's entry has no name"));
    }
    let mut path = root.to_owned();
    for component in &components[..components.len() - 1] {
        let Component::Normal(name) = component else {
            return Err(refuse(
                "the image archive's entry is not a plain relative path",
            ));
        };
        path.push(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(refuse(format!(
                    "the image archive's entry {} passes through a symbolic link",
                    relative.display()
                )))
            }
            Ok(_) => {
                return Err(refuse(format!(
                    "the image archive's entry {} passes through a non-directory",
                    relative.display()
                )))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                DirBuilder::new().mode(0o755).create(&path)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let Some(Component::Normal(name)) = components.last() else {
        return Err(refuse(
            "the image archive's entry is not a plain relative path",
        ));
    };
    Ok(path.join(name))
}

/// `relative`'s place inside `root` when every component of it is already
/// there: `None` when one is not, and nothing is made.
fn beneath_existing(root: &Path, relative: &Path) -> Result<Option<PathBuf>, RuntimeError> {
    let mut path = root.to_owned();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(refuse(
                "the image archive's entry is not a plain relative path",
            ));
        };
        path.push(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(refuse(
                    "the image archive's entry passes through a symbolic link",
                ))
            }
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(Some(path))
}

/// A directory at `target`, with `mode` (0755 for a mode of zero).
fn make_directory(target: &Path, mode: u32) -> Result<(), RuntimeError> {
    let mode = if mode & 0o777 == 0 {
        0o755
    } else {
        mode & 0o777
    };
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            remove_any(target)?;
            DirBuilder::new().mode(mode).create(target)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            DirBuilder::new().mode(mode).create(target)?;
        }
        Err(error) => return Err(error.into()),
    }
    fs::set_permissions(target, fs::Permissions::from_mode(mode))?;
    Ok(())
}

/// A regular file at `target`, whose bytes are all of `data`.
fn write_file(target: &Path, mode: u32, data: &mut dyn io::Read) -> Result<(), RuntimeError> {
    let mode = mode & 0o777;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .custom_flags((OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC).bits())
        .open(target)?;
    io::copy(data, &mut file)?;
    fs::set_permissions(target, fs::Permissions::from_mode(mode))?;
    Ok(())
}

/// Removes whatever is at `path` without following it: a symbolic link is
/// removed as the link it is, never as what it names.
fn remove_any(path: &Path) -> Result<(), RuntimeError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(metadata) if metadata.is_dir() => Ok(fs::remove_dir_all(path)?),
        Ok(_) => Ok(fs::remove_file(path)?),
    }
}

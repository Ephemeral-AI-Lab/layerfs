//! Where unpacked images live and how one digest is unpacked exactly once:
//! `<root>/images/<64 hex>`, published by a rename, guarded by a per-digest
//! `flock`.
use super::{archive::unpack, refuse, ImageRoot};
use crate::{Image, RuntimeError};
use nix::fcntl::{Flock, FlockArg};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

/// The cache's own name under a runtime root: `<root>/images`.
const CACHE: &str = "images";

/// The cache directory below a runtime root, `<root>/images`, where unpacked
/// images and their locks live.
pub fn cache(root: &Path) -> PathBuf {
    root.join(CACHE)
}

/// The image's root under `cache`, unpacking `archive` first when this host
/// has no complete copy of that digest yet. One unpack per digest, however
/// many Sandboxes ask for it at once.
pub fn ensure(archive: &Path, image: &Image, cache: &Path) -> Result<ImageRoot, RuntimeError> {
    let cache = beneath(cache)?;
    let lock = lock(&cache, image)?;
    let directory = cache.join(image.hex());
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() => {
            drop(lock);
            return Ok(ImageRoot::new(image.clone(), directory));
        }
        Ok(_) => {
            return Err(refuse(format!(
                "the image cache {} is not a directory",
                directory.display()
            )))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    // a crash of an earlier unpack left only its temporary directory: it is
    // never the real name, so it is removed before this one is made
    remove_temporaries(&cache, image)?;
    let staging = cache.join(format!(
        ".tmp-{}-{}",
        image.hex(),
        crate::backend::identity::random()?.as_str()
    ));
    DirBuilder::new().mode(0o700).create(&staging)?;
    if let Err(cause) = unpack(archive, image, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(cause);
    }
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o555))?;
    match fs::rename(&staging, &directory) {
        Ok(()) => {}
        Err(error) => {
            // another Sandbox published the same digest first: its copy is
            // this one's too, and this one is the duplicate
            let _ = fs::remove_dir_all(&staging);
            match fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.is_dir() => {}
                _ => return Err(error.into()),
            }
        }
    }
    drop(lock);
    Ok(ImageRoot::new(image.clone(), directory))
}

/// The image's root under `cache` when this host already holds it, unpacking
/// nothing. A record whose image is gone cannot be reopened as the same root.
pub fn lookup(image: &Image, cache: &Path) -> Result<ImageRoot, RuntimeError> {
    let directory = cache.join(image.hex());
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() => Ok(ImageRoot::new(image.clone(), directory)),
        _ => Err(refuse(format!(
            "the unpacked image {} is not at {}: this Sandbox's root cannot be found, so it is \
             neither reopened nor deleted",
            image.as_str(),
            directory.display()
        ))),
    }
}

/// The cache directory, made 0700 and owned by this (root) process when it is
/// not there yet, verified to be a real directory when it is.
fn beneath(cache: &Path) -> Result<PathBuf, RuntimeError> {
    if !cache.is_absolute() {
        return Err(RuntimeError::Protocol(
            "image cache must be an absolute path",
        ));
    }
    match fs::symlink_metadata(cache) {
        Ok(metadata) if metadata.is_dir() => Ok(cache.to_owned()),
        Ok(_) => Err(refuse(format!(
            "the image cache {} is not a directory",
            cache.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = cache
                .parent()
                .ok_or(RuntimeError::Protocol("image cache parent"))?;
            fs::symlink_metadata(parent)?;
            DirBuilder::new().mode(0o700).create(cache)?;
            Ok(cache.to_owned())
        }
        Err(error) => Err(error.into()),
    }
}

/// The per-digest lock, held by the caller until the returned lock is dropped.
/// The file stays on disk: another process may be waiting on it.
fn lock(cache: &Path, image: &Image) -> Result<Flock<File>, RuntimeError> {
    let path = cache.join(format!("{}.lock", image.hex()));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits())
        .open(path)?;
    Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, error)| RuntimeError::Io(error.into()))
}

/// Removes the temporary directories a crashed unpack of `image` left; only
/// this digest's, and only while holding its lock.
fn remove_temporaries(cache: &Path, image: &Image) -> Result<(), RuntimeError> {
    let prefix = format!(".tmp-{}-", image.hex());
    for entry in fs::read_dir(cache)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(text) = name.to_str() else { continue };
        if !text.starts_with(&prefix) {
            continue;
        }
        let path = entry.path();
        if fs::symlink_metadata(&path)?.file_type().is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

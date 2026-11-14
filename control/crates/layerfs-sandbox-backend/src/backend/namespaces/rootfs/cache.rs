//! Where unpacked images live and how one digest is unpacked exactly once:
//! `<root>/images/<64 hex>.2`, published by a rename, guarded by a per-digest
//! `flock`. A Sandbox made before file times were restored keeps the
//! `<64 hex>` it was built on.
use super::{
    archive::{unpack, unpack_flat},
    refuse, ImageRoot,
};
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
/// The layout an unpack is published under, after the digest: `.2` restores
/// file times; a plain `<64 hex>` is an older unpack that did not.
const LAYOUT: &str = ".2";

/// The cache directory below a runtime root, `<root>/images`, where unpacked
/// images and their locks live.
pub fn cache(root: &Path) -> PathBuf {
    root.join(CACHE)
}

/// The image's root under `cache`, unpacking `archive` first when this host
/// has no complete copy of that digest yet. One unpack per digest, however
/// many Sandboxes ask for it at once.
pub fn ensure(archive: &Path, image: &Image, cache: &Path) -> Result<ImageRoot, RuntimeError> {
    ensure_held(archive, image, cache).map(|(root, _)| root)
}

/// [`ensure`], keeping a shared hold on the image's lock until the returned
/// [`ImageHold`] is dropped: a Sandbox's create keeps it until its
/// `helper.spec` names the image, so [`remove`] never takes an image a
/// Sandbox is about to use.
pub fn ensure_held(
    archive: &Path,
    image: &Image,
    cache: &Path,
) -> Result<(ImageRoot, ImageHold), RuntimeError> {
    publish(image, cache, |staging| unpack(archive, image, staging))
        .map(|(root, hold, _)| (root, hold))
}

/// The image's root under `cache`, unpacking the flat root filesystem tar
/// `tar` (`docker export`'s output, without its init layer) when this host
/// has no copy of `image` yet; `true` when this call unpacked it. The same
/// single-flight lock and rename as [`ensure`].
pub fn ensure_flat(
    tar: &Path,
    image: &Image,
    cache: &Path,
) -> Result<(ImageRoot, bool), RuntimeError> {
    publish(image, cache, |staging| unpack_flat(tar, staging))
        .map(|(root, _, unpacked)| (root, unpacked))
}

/// A shared hold on one image's lock: while it lives, [`remove`] skips the
/// image.
#[derive(Debug)]
pub struct ImageHold(#[allow(dead_code)] Flock<File>);

/// Publishes `image` under `cache` with `fill` (which writes the tree into
/// the staging directory it is given) unless it is there already, under
/// the image's exclusive lock, which is then turned shared and returned.
fn publish(
    image: &Image,
    cache: &Path,
    fill: impl FnOnce(&Path) -> Result<(), RuntimeError>,
) -> Result<(ImageRoot, ImageHold, bool), RuntimeError> {
    let cache = beneath(cache)?;
    let lock = lock(&cache, image)?;
    let directory = cache.join(name(image));
    let held = |lock: Flock<File>| -> Result<ImageHold, RuntimeError> {
        lock.relock(FlockArg::LockShared)
            .map_err(|error| RuntimeError::Io(error.into()))?;
        Ok(ImageHold(lock))
    };
    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() => {
            return Ok((ImageRoot::new(image.clone(), directory), held(lock)?, false));
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
        name(image),
        crate::ContainerId::random()?.as_str()
    ));
    DirBuilder::new().mode(0o700).create(&staging)?;
    if let Err(cause) = fill(&staging) {
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
    Ok((ImageRoot::new(image.clone(), directory), held(lock)?, true))
}

/// Removes `image`'s unpacked copies from `cache` (`<64 hex>.2` and an older
/// unpack's `<64 hex>`) unless `in_use` names one of them or another process
/// holds the image's lock (a create between its unpack and its spec):
/// `false` then, and nothing is removed. `true` when nothing of it is left.
pub fn remove(
    image: &Image,
    cache: &Path,
    in_use: impl Fn(&Path) -> bool,
) -> Result<bool, RuntimeError> {
    let cache = beneath(cache)?;
    let path = cache.join(format!("{}.lock", name(image)));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits())
        .open(path)?;
    let _lock = match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(lock) => lock,
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => return Ok(false),
        Err((_, error)) => return Err(RuntimeError::Io(error.into())),
    };
    let directories = [cache.join(name(image)), cache.join(image.hex())];
    if directories.iter().any(|directory| in_use(directory)) {
        return Ok(false);
    }
    for directory in directories {
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() => {
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
                fs::remove_dir_all(&directory)?;
            }
            Ok(_) => fs::remove_file(&directory)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(true)
}

/// The root a Sandbox of `image` was built on, `lower` as its spec records
/// it, when this host still holds it, unpacking nothing. `lower` must be one
/// of `cache`'s directories for that digest (`<64 hex>.2`, or an older
/// unpack's `<64 hex>`): a reopen never moves a Sandbox to another root, and
/// a record whose image is gone is refused.
pub fn lookup(image: &Image, cache: &Path, lower: &Path) -> Result<ImageRoot, RuntimeError> {
    let known = [image.hex().to_owned(), name(image)]
        .iter()
        .any(|name| cache.join(name) == lower);
    match fs::symlink_metadata(lower) {
        Ok(metadata) if known && metadata.is_dir() => {
            Ok(ImageRoot::new(image.clone(), lower.to_owned()))
        }
        _ => Err(refuse(format!(
            "the unpacked image {} is not at {} in {}: this Sandbox's root cannot be found, so \
             it is neither reopened nor deleted",
            image.as_str(),
            lower.display(),
            cache.display()
        ))),
    }
}

/// An unpack's name in the cache: the digest's hex and the layout.
fn name(image: &Image) -> String {
    format!("{}{LAYOUT}", image.hex())
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
            match DirBuilder::new().mode(0o700).create(cache) {
                // another unpack made it first
                Err(error)
                    if error.kind() == io::ErrorKind::AlreadyExists
                        && fs::symlink_metadata(cache)?.is_dir() => {}
                other => other?,
            }
            Ok(cache.to_owned())
        }
        Err(error) => Err(error.into()),
    }
}

/// The per-digest lock, held by the caller until the returned lock is dropped.
/// The file stays on disk: another process may be waiting on it.
fn lock(cache: &Path, image: &Image) -> Result<Flock<File>, RuntimeError> {
    let path = cache.join(format!("{}.lock", name(image)));
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
/// this digest's and layout's, and only while holding its lock.
fn remove_temporaries(cache: &Path, image: &Image) -> Result<(), RuntimeError> {
    let prefix = format!(".tmp-{}-", name(image));
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

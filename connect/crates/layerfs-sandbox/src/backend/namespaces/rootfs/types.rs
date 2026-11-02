//! The image's unpacked root handle and errors: [`ImageRoot`] and [`refuse`].
use crate::{Image, RuntimeError};
use std::{io, path::PathBuf};

/// The kernel's own limit on one path.
pub(crate) const PATH_MAX: usize = 4096;

/// One image's unpacked root, and the digest it is the root of.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageRoot {
    image: Image,
    directory: PathBuf,
}

impl ImageRoot {
    /// The digest this root is the unpacked image of.
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// The read-only directory the Sandbox's overlay takes as its lower layer.
    pub fn directory(&self) -> &std::path::Path {
        &self.directory
    }

    /// The root of `image` at `directory`, which is already unpacked there.
    pub(super) fn new(image: Image, directory: PathBuf) -> Self {
        Self { image, directory }
    }
}

/// A refusal with the exact reason, as the runtime's I/O cause: the runtime's
/// protocol causes are static and these name paths and digests.
pub(crate) fn refuse(message: impl Into<String>) -> RuntimeError {
    RuntimeError::Io(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

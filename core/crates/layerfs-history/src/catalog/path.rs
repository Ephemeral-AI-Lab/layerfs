//! The `project.remove_file` path grammar and its keys.
//!
//! The call accepts an absolute POSIX path and stores it as the normalized
//! relative path (`/a` and `a` are the same). A path is refused when it is
//! empty, `/`, `//a`, has an empty, `.` or `..` component, a trailing `/`, a
//! `\` or NUL byte, is longer than 4096 bytes, or has more than 256
//! components. The stored key is `BLAKE3(normalized path)`; the trace key is
//! keyed by the Store's `trace_salt` over the stack and the normalized path.

use crate::identity::LayerStackId;

/// The most bytes one normalized path may have.
pub const MAXIMUM_PATH_BYTES: usize = 4096;
/// The most components one normalized path may have.
pub const MAXIMUM_PATH_COMPONENTS: usize = 256;

/// A checked, normalized relative path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedPath {
    relative: String,
}

impl NormalizedPath {
    /// Checks `text` and returns its normalized relative path.
    ///
    /// One leading `/` is stripped; everything after it is the relative path.
    /// The returned value never starts or ends with `/` and never holds an
    /// empty, `.` or `..` component.
    pub fn new(text: &str) -> Result<Self, &'static str> {
        if text.is_empty() {
            return Err("path is empty");
        }
        if text.as_bytes().contains(&0) {
            return Err("path contains a NUL byte");
        }
        if text.contains('\\') {
            return Err("path contains a backslash");
        }
        let relative = text.strip_prefix('/').unwrap_or(text);
        if relative.is_empty() {
            return Err("path is the root");
        }
        if relative.starts_with('/') {
            return Err("path has an empty component");
        }
        let mut components = 0usize;
        for component in relative.split('/') {
            if component.is_empty() {
                return Err("path has an empty component");
            }
            if component == "." || component == ".." {
                return Err("path has a dot component");
            }
            components += 1;
        }
        if components > MAXIMUM_PATH_COMPONENTS {
            return Err("path has too many components");
        }
        if relative.len() > MAXIMUM_PATH_BYTES {
            return Err("path is too long");
        }
        Ok(Self {
            relative: relative.to_owned(),
        })
    }

    /// The normalized relative path, without a leading `/`.
    pub fn as_str(&self) -> &str {
        &self.relative
    }

    /// The normalized path bytes as stored in the marker.
    pub fn as_bytes(&self) -> &[u8] {
        self.relative.as_bytes()
    }

    /// `BLAKE3(normalized path)`, the marker key.
    pub fn key(&self) -> [u8; 32] {
        path_key(self.as_bytes())
    }
}

/// `BLAKE3(path)` over already normalized path bytes, the marker key.
pub fn path_key(path: &[u8]) -> [u8; 32] {
    *blake3::hash(path).as_bytes()
}

/// True when `path` is `marker` or lies below it at a `/` boundary
/// (`p == m OR p begins m ++ b"/"`), compared over bytes, never `||`/`LIKE`.
pub fn governs(marker: &[u8], path: &[u8]) -> bool {
    path == marker
        || (path.len() > marker.len() && path.starts_with(marker) && path[marker.len()] == b'/')
}

/// The key a trace hashes for one stack and path: keyed by the Store's
/// 32-byte `trace_salt` over the stack identity and the normalized path.
pub fn trace_key(salt: &[u8; 32], stack: LayerStackId, path: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(salt);
    hasher.update(stack.as_slice());
    hasher.update(path);
    *hasher.finalize().as_bytes()
}

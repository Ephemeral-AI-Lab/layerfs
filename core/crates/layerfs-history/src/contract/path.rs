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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_one_leading_slash_and_relative_paths() {
        assert_eq!(NormalizedPath::new("/a").unwrap().as_str(), "a");
        assert_eq!(NormalizedPath::new("a").unwrap().as_str(), "a");
        assert_eq!(NormalizedPath::new("/a/b").unwrap().as_str(), "a/b");
        assert_eq!(NormalizedPath::new("a-b_c.d").unwrap().as_str(), "a-b_c.d");
    }

    #[test]
    fn refuses_the_bad_grammar() {
        for text in [
            "", "/", "//a", "a//b", "a/", "./a", "a/./b", ".", "..", "/./a", "a\\b", "a\0b",
        ] {
            assert!(NormalizedPath::new(text).is_err(), "accepted {text:?}");
        }
    }

    #[test]
    fn refuses_long_paths_and_many_components() {
        assert!(NormalizedPath::new(&"a".repeat(MAXIMUM_PATH_BYTES)).is_ok());
        assert!(NormalizedPath::new(&"a".repeat(MAXIMUM_PATH_BYTES + 1)).is_err());
        let many = vec!["a"; MAXIMUM_PATH_COMPONENTS].join("/");
        assert!(NormalizedPath::new(&many).is_ok());
        let more = vec!["a"; MAXIMUM_PATH_COMPONENTS + 1].join("/");
        assert!(NormalizedPath::new(&more).is_err());
    }

    #[test]
    fn subtree_matches_only_at_a_slash_boundary() {
        assert!(governs(b"a", b"a"));
        assert!(governs(b"a", b"a/b"));
        assert!(!governs(b"a", b"ab"));
        assert!(!governs(b"ab", b"a"));
        assert!(governs(b"a/b", b"a/b/c"));
        assert!(!governs(b"a/b", b"a/bc"));
    }

    #[test]
    fn keys_are_stable_and_salted_traces_differ() {
        let path = NormalizedPath::new("/a/b").unwrap();
        assert_eq!(path.key(), path_key(b"a/b"));
        let stack = LayerStackId::from_authority([7; 16]);
        let first = trace_key(&[1; 32], stack, path.as_bytes());
        let second = trace_key(&[2; 32], stack, path.as_bytes());
        assert_ne!(first, second);
    }
}

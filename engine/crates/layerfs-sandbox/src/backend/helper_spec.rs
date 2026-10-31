//! The init stage's inputs: absolute paths only, one per line, in a 0600 file:
//! what it mounts, the three directories of the Sandbox root's overlay (the
//! unpacked image as its read-only lower layer, and the Sandbox's own upper
//! and work directories), the daemon it starts, and the cgroup it joins.
//!
//! A format, not a platform behaviour: it is compiled wherever the crate is and
//! tested without root; only the helper that reads it is Linux only.
//!
//! The daemon's setup (its keys and every secret in it) is a separate file the
//! helper never reads; this spec names only what the init stage mounts and
//! runs, so it carries no secret. Values may contain `=` (the key ends at the
//! first one); a path may not contain a control character, because the format
//! is line based.
use crate::RuntimeError;
use std::{
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};

/// The fields, in the order they are written.
const FIELDS: [&str; 9] = [
    "root", "sandbox", "store", "daemon", "config", "cgroup", "lower", "upper", "work",
];

/// One init stage's decoded inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperSpec {
    /// The private root the mount namespace pivots to; a tmpfs is mounted on
    /// it by the init stage. Sibling of `sandbox`, never inside it.
    pub root: PathBuf,
    /// The Sandbox directory, bound at its own path inside the private root,
    /// so the daemon's setup, overlay and Workspace mounts keep one naming.
    pub sandbox: PathBuf,
    /// The Store's directory, bound read-write: SQLite's WAL and the writers'
    /// directory lock then see the same inode as every other opener.
    pub store: PathBuf,
    /// The daemon executable inside the private root.
    pub daemon: PathBuf,
    /// The daemon's setup file, 0600.
    pub config: PathBuf,
    /// The Sandbox's cgroup v2 directory.
    pub cgroup: PathBuf,
    /// The unpacked image: the read-only lower layer of the Sandbox's root.
    pub lower: PathBuf,
    /// The Sandbox's own writable layer, and the overlay's work directory;
    /// both inside its private directory, deleted with it.
    pub upper: PathBuf,
    pub work: PathBuf,
}

impl HelperSpec {
    /// Encodes the six fields; refuses a path that is not absolute, has `.`
    /// or `..` components, or holds a control character.
    pub fn encode(&self) -> Result<Vec<u8>, RuntimeError> {
        let mut out = Vec::new();
        for (index, (field, path)) in self.fields().into_iter().enumerate() {
            check(path)?;
            if index >= LAYERS {
                check_overlay(path)?;
            }
            out.extend_from_slice(field.as_bytes());
            out.push(b'=');
            let text = path.to_str().ok_or(RuntimeError::Protocol(PATH))?;
            out.extend_from_slice(text.as_bytes());
            out.push(b'\n');
        }
        Ok(out)
    }

    /// Decodes a spec written by [`Self::encode`]: exactly the six fields,
    /// each once, in any order.
    pub fn parse(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() > MAX {
            return Err(RuntimeError::Protocol(PATH));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| RuntimeError::Protocol(PATH))?;
        let mut values: [Option<PathBuf>; 9] = Default::default();
        for line in text.lines() {
            let (key, value) = line.split_once('=').ok_or(RuntimeError::Protocol(FIELD))?;
            let index = FIELDS
                .iter()
                .position(|name| *name == key)
                .ok_or(RuntimeError::Protocol(FIELD))?;
            if values[index].is_some() {
                return Err(RuntimeError::Protocol(FIELD));
            }
            let path = PathBuf::from(value);
            check(&path)?;
            values[index] = Some(path);
        }
        let mut take = |index: usize| values[index].take().ok_or(RuntimeError::Protocol(FIELD));
        let spec = Self {
            root: take(0)?,
            sandbox: take(1)?,
            store: take(2)?,
            daemon: take(3)?,
            config: take(4)?,
            cgroup: take(5)?,
            lower: take(6)?,
            upper: take(7)?,
            work: take(8)?,
        };
        for path in [&spec.lower, &spec.upper, &spec.work] {
            check_overlay(path)?;
        }
        Ok(spec)
    }

    fn fields(&self) -> [(&'static str, &Path); 9] {
        [
            (FIELDS[0], &self.root),
            (FIELDS[1], &self.sandbox),
            (FIELDS[2], &self.store),
            (FIELDS[3], &self.daemon),
            (FIELDS[4], &self.config),
            (FIELDS[5], &self.cgroup),
            (FIELDS[6], &self.lower),
            (FIELDS[7], &self.upper),
            (FIELDS[8], &self.work),
        ]
    }
}

/// The largest spec accepted: six absolute paths, each well under a page.
const MAX: usize = 8192;

/// Rejects a path the line format cannot carry or the init stage cannot
/// mount: relative, `..`/`.`, a control character, or longer than [`MAX`].
fn check(path: &Path) -> Result<(), RuntimeError> {
    let text = path.to_str().ok_or(RuntimeError::Protocol(PATH))?;
    if !path.is_absolute()
        || text.len() > MAX
        || text
            .chars()
            .any(|c| c == '\n' || c == '\r' || c == '\0' || c == '\t')
        || (path.components()).any(|c| matches!(c, Component::CurDir | Component::ParentDir))
    {
        return Err(RuntimeError::Protocol(PATH));
    }
    Ok(())
}

/// The three fields from here on are the Sandbox root's overlay.
const LAYERS: usize = 6;

/// The three overlay paths travel in one mount option string, where `,`
/// separates options and `:` would separate layers, so neither may appear.
fn check_overlay(path: &Path) -> Result<(), RuntimeError> {
    let bytes = path.as_os_str().as_bytes();
    if bytes.iter().any(|byte| matches!(byte, b',' | b':')) {
        return Err(RuntimeError::Protocol("an overlay path holds a `,` or `:`"));
    }
    Ok(())
}

/// Path text is UTF-8, absolute and free of control characters.
const PATH: &str = "helper spec path";
/// A line that is not one known field, or a field given twice or not at all.
const FIELD: &str = "helper spec field";

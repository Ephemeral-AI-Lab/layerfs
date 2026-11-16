//! The init stage's inputs: absolute paths only, one per line, in a 0600 file:
//! the three directories of the Sandbox root's overlay (the unpacked image as
//! its read-only lower layer, and the Sandbox's own upper and work
//! directories), the daemon it starts, the cgroup it joins, and the host
//! directories it binds in (#535 §6.3: a sqlite Store's directory; none for
//! a PostgreSQL Store).
//!
//! A format, not a platform behaviour: it is compiled wherever the crate is and
//! tested without root; only the helper that reads it is Linux only. It is
//! not versioned: the launcher and the helper come from one bundle, and a
//! spec of another field set (the former single `store=` line) is refused.
//!
//! The daemon's setup (its keys and every secret in it) is a separate file the
//! helper never reads; this spec names only what the init stage mounts and
//! runs, so it carries no secret. Values may contain `=` (the key ends at the
//! first one); a path may not contain a control character, because the format
//! is line based. A bind is one `bind=<rw|ro>\t<host>\t<guest>` line, which a
//! tab (refused in every path) separates.
//!
//! An optional `runtime=` line names the Sandbox's copy of the bundle's
//! runtime layer (R5, `layerfs_bridge::sandbox_runtime`), which the init stage
//! binds read-only at `/.layerfs/runtime` and starts the daemon through. A
//! spec without it (one written before the runtime layer, or from a bundle
//! that has none) still parses, and that Sandbox reopens unchanged.
use crate::{Bind, RuntimeError};
use std::{
    path::{Component, Path, PathBuf},
};

/// The fields every spec has once, in the order they are written; `bind`
/// lines follow them, any number.
const FIELDS: [&str; 8] = [
    "root", "sandbox", "daemon", "config", "cgroup", "lower", "upper", "work",
];
/// The key of a bind line.
const BIND: &str = "bind";
/// The key of the optional runtime line, written once after the fields.
const RUNTIME: &str = "runtime";

/// One init stage's decoded inputs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperSpec {
    /// The private root the mount namespace pivots to; a tmpfs is mounted on
    /// it by the init stage. Sibling of `sandbox`, never inside it.
    pub root: PathBuf,
    /// The Sandbox directory, bound at its own path inside the private root,
    /// so the daemon's setup, overlay and Workspace mounts keep one naming.
    pub sandbox: PathBuf,
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
    /// The host directories bound in, in order, after the Sandbox
    /// directory: a sqlite Store's directory read-write at its own path
    /// (SQLite's WAL and the writers' directory lock then see the same inode
    /// as every other opener); none for a PostgreSQL Store.
    pub binds: Vec<Bind>,
    /// The Sandbox's copy of the bundle's runtime layer, bound read-only,
    /// nosuid and nodev at `/.layerfs/runtime`; the daemon then starts
    /// through its loader. `None` for a bundle without one, and for a spec
    /// written before it.
    pub runtime: Option<PathBuf>,
}

impl HelperSpec {
    /// Encodes the fields and the binds; refuses a path that is not
    /// absolute, has `.` or `..` components, or holds a control character.
    pub fn encode(&self) -> Result<Vec<u8>, RuntimeError> {
        let mut out = Vec::new();
        for (index, (field, path)) in self.fields().into_iter().enumerate() {
            check(path)?;
            if index >= LAYERS {
                check_overlay(path)?;
            }
            out.extend_from_slice(field.as_bytes());
            out.push(b'=');
            out.extend_from_slice(text(path)?.as_bytes());
            out.push(b'\n');
        }
        if let Some(runtime) = &self.runtime {
            check(runtime)?;
            out.extend_from_slice(format!("{RUNTIME}={}\n", text(runtime)?).as_bytes());
        }
        for bind in &self.binds {
            check(&bind.host)?;
            check(&bind.guest)?;
            let mode = if bind.read_write { "rw" } else { "ro" };
            let line = format!(
                "{BIND}={mode}\t{}\t{}\n",
                text(&bind.host)?,
                text(&bind.guest)?
            );
            out.extend_from_slice(line.as_bytes());
        }
        Ok(out)
    }

    /// Decodes a spec written by [`Self::encode`]: exactly the eight fields,
    /// each once, in any order, the runtime at most once, and any number of
    /// binds, kept in order.
    pub fn parse(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() > MAX {
            return Err(RuntimeError::Protocol(PATH));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| RuntimeError::Protocol(PATH))?;
        let mut values: [Option<PathBuf>; 8] = Default::default();
        let mut binds = Vec::new();
        let mut runtime = None;
        for line in text.lines() {
            let (key, value) = line.split_once('=').ok_or(RuntimeError::Protocol(FIELD))?;
            if key == BIND {
                binds.push(parse_bind(value)?);
                continue;
            }
            if key == RUNTIME {
                let path = PathBuf::from(value);
                check(&path)?;
                if runtime.replace(path).is_some() {
                    return Err(RuntimeError::Protocol(FIELD));
                }
                continue;
            }
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
            daemon: take(2)?,
            config: take(3)?,
            cgroup: take(4)?,
            lower: take(5)?,
            upper: take(6)?,
            work: take(7)?,
            binds,
            runtime,
        };
        for path in [&spec.lower, &spec.upper, &spec.work] {
            check_overlay(path)?;
        }
        Ok(spec)
    }

    /// The daemon's command line inside the Sandbox's root. With a runtime
    /// layer (R5) the daemon runs through its loader on its libraries only
    /// (`--inhibit-cache --library-path`), and `LAYERFS_SANDBOX_RUNTIME`
    /// tells it the runtime; without one it runs on the image's own loader
    /// and libraries, as before. The caller sets the rest of its environment.
    pub fn daemon_command(&self) -> Result<std::process::Command, RuntimeError> {
        use layerfs_bridge::sandbox_runtime::{command, ENVIRONMENT, GUEST};
        let mut daemon = match &self.runtime {
            Some(_) => {
                let mut daemon = command(Path::new(GUEST), &self.daemon).ok_or(
                    RuntimeError::Unsupported("this architecture has no Sandbox runtime layer"),
                )?;
                daemon.env(ENVIRONMENT, GUEST);
                daemon
            }
            None => std::process::Command::new(&self.daemon),
        };
        daemon.arg("--config").arg(&self.config);
        Ok(daemon)
    }

    fn fields(&self) -> [(&'static str, &Path); 8] {
        [
            (FIELDS[0], &self.root),
            (FIELDS[1], &self.sandbox),
            (FIELDS[2], &self.daemon),
            (FIELDS[3], &self.config),
            (FIELDS[4], &self.cgroup),
            (FIELDS[5], &self.lower),
            (FIELDS[6], &self.upper),
            (FIELDS[7], &self.work),
        ]
    }
}

/// One `bind=` value: `rw` or `ro`, the host path and the guest path,
/// separated by tabs.
fn parse_bind(value: &str) -> Result<Bind, RuntimeError> {
    let mut parts = value.split('\t');
    let (Some(mode), Some(host), Some(guest), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(RuntimeError::Protocol(FIELD));
    };
    let read_write = match mode {
        "rw" => true,
        "ro" => false,
        _ => return Err(RuntimeError::Protocol(FIELD)),
    };
    let (host, guest) = (PathBuf::from(host), PathBuf::from(guest));
    check(&host)?;
    check(&guest)?;
    Ok(Bind {
        host,
        guest,
        read_write,
    })
}

/// A path's text: UTF-8, else refused.
fn text(path: &Path) -> Result<&str, RuntimeError> {
    path.to_str().ok_or(RuntimeError::Protocol(PATH))
}

/// The largest spec accepted: the eight paths and the binds, each well
/// under a page.
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
const LAYERS: usize = 5;

/// The three overlay paths travel in one mount option string, where `,`
/// separates options and `:` would separate layers, so neither may appear.
fn check_overlay(path: &Path) -> Result<(), RuntimeError> {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    // Windows: the WTF-8 encoding of the path, the same bytes the OS names
    // it by; the format is line based and rejects control characters.
    #[cfg(not(unix))]
    let bytes = {
        use std::os::windows::ffi::OsStrExt;
        let wide = path.as_os_str().encode_wide().collect::<Vec<u16>>();
        let mut bytes = Vec::with_capacity(wide.len() * 2);
        for unit in wide {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    };
    if bytes.iter().any(|byte| matches!(byte, b',' | b':')) {
        return Err(RuntimeError::Protocol("an overlay path holds a `,` or `:`"));
    }
    Ok(())
}

/// Path text is UTF-8, absolute and free of control characters.
const PATH: &str = "helper spec path";
/// A line that is not one known field or a well-formed bind, or a field
/// given twice or not at all.
const FIELD: &str = "helper spec field";

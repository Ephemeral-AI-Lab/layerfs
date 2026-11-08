//! The caller's side of `project.init` into a sqlite Store on another
//! machine (#536 H11a): the folder is on the caller's host, so this process
//! walks it and copies it to the machine's worker in `project.stage`
//! requests of at most [`CHUNK`] bytes ([`super::tree`] records), as a
//! `store push` sends a Store file; the worker's Init then reads the copy.
//!
//! The walk refuses, before anything is sent, what Init would refuse of the
//! folder: an entry that is not a regular file, directory or symbolic link,
//! a name or link target LayerFS cannot hold, permission bits it cannot
//! store (set-user-ID, set-group-ID, sticky on a file). A file that changes
//! while it is read is refused, as Init refuses it. Each request is sent
//! once: a certain failure discards the copy with one more request; a lost
//! answer is `Unknown`, naming the copy it may leave on the machine.
//!
//! Parts go several at once ([`super::parts`]). The `D`, `W` and `L`
//! records apply in any order on the worker; the `M` records (modes and
//! times, which a later write or entry would undo) are gathered in parts of
//! their own, each sent only after every part before it was answered.
use super::parts::Parts;
use super::tree::{header, Record};
use crate::place::Worker;
use crate::sqlite::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::filesystem::symlink::SymlinkTarget;
use layerfs_content::PathName;
use layerfs_home::events::Events;
use layerfs_home::outcome::refused;
use layerfs_project::SourceRefusal;
use std::ffi::{OsStr, OsString};
use std::fs::{self, Metadata};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

// Windows: the grammar's paths are valid UTF-8 (PathName::from_utf8
// enforces it); the bytes face is UTF-8 bytes, per the port's grammar.
#[cfg(windows)]
mod os_bytes {
    use std::ffi::{OsStr, OsString};

    pub fn from_bytes(bytes: &[u8]) -> std::borrow::Cow<'_, OsStr> {
        let text = String::from_utf8_lossy(bytes).into_owned();
        std::borrow::Cow::Owned(OsString::from(text))
    }
    pub fn as_bytes(text: &OsStr) -> &[u8] {
        text.to_str().map(|s| s.as_bytes()).unwrap_or(&[])
    }
    pub fn into_vec(text: OsString) -> Vec<u8> {
        text.into_string().map(|s| s.into_bytes()).unwrap_or_default()
    }
}
#[cfg(windows)]
use os_bytes::{as_bytes, from_bytes, into_vec};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The smallest file piece worth a part's remaining room; less starts the
/// next part.
const MIN_PIECE: usize = 64 << 10;

/// What one copy names.
pub struct Copy<'a> {
    pub store: &'a str,
    pub name: &'a str,
    pub machine: &'a str,
    pub folder: &'a Path,
    /// The staging folder's id on the machine.
    pub id: String,
}

/// A fresh copy id: 16 hex digits.
pub fn new_id() -> Result<String> {
    let bytes = layerfs_home::helpers::random()?;
    Ok(layerfs_bridge::sdk_values::hex(&bytes[..8]))
}

/// Where copy `id` may stay on `machine` after an uncertain end.
pub fn left(machine: &str, id: &str) -> String {
    format!("incoming/init-{id} under machine {machine}'s worker root")
}

/// One entry the walk reaches, with its path relative to the folder.
enum Visit<'a> {
    /// A directory, before what it holds.
    Enter,
    /// A regular file.
    File(&'a Metadata),
    /// A symbolic link and its target.
    Link(&'a Metadata, &'a [u8]),
    /// A directory (the folder itself last), after what it holds.
    Leave(&'a Metadata),
}

/// What [`walk`] calls for each entry: its path relative to the folder,
/// its path on this host, and what it is.
type Visitor<'v> = dyn FnMut(&[u8], &Path, Visit<'_>) -> Result<()> + 'v;

struct Level {
    rel: Vec<u8>,
    meta: Metadata,
    children: Vec<OsString>,
}

fn io(path: &Path, error: std::io::Error) -> SdkRefusal {
    refused("Failed", format!("{}: {error}", path.display()))
}

fn children(path: &Path) -> Result<Vec<OsString>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(path).map_err(|e| io(path, e))? {
        names.push(entry.map_err(|e| io(path, e))?.file_name());
    }
    Ok(names)
}

/// The refusal Init gives for `path` of project `project`.
fn refusal(project: &str, path: &Path, reason: &dyn std::fmt::Display) -> SdkRefusal {
    refused(
        "InvalidInput",
        format!("adding {project} refused {}: {reason}", path.display()),
    )
}

/// Init's portable-mode rule: `mask` is the bits the kind may carry.
fn portable(project: &str, path: &Path, meta: &Metadata, mask: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let mode = meta.mode() & 0o7777;
        if mode & !mask != 0 {
            return Err(refusal(project, path, &SourceRefusal::Mode(mode)));
        }
        if !(0..1_000_000_000).contains(&meta.mtime_nsec()) {
            return Err(refusal(project, path, &SourceRefusal::Mtime));
        }
    }
    // Windows: the readonly attribute is the only mode bit; mtime needs no
    // range gate. The mask keeps its meaning for the records below.
    #[cfg(not(unix))]
    let _ = (project, path, meta, mask);
    Ok(())
}

/// The folder as the caller sees it: an existing directory, not a link.
pub fn root(folder: &Path) -> Result<Metadata> {
    let not = |what: &str| {
        refused(
            "InvalidInput",
            format!("the folder {} {what} on this host", folder.display()),
        )
    };
    match fs::symlink_metadata(folder) {
        Ok(meta) if meta.is_dir() => Ok(meta),
        Ok(_) => Err(not("is not a directory")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(not("does not exist")),
        Err(e) => Err(io(folder, e)),
    }
}

/// Visits every entry of `folder`, depth first, a directory's `Leave` after
/// its contents; refuses what Init refuses of project `project`.
fn walk(folder: &Path, project: &str, visit: &mut Visitor<'_>) -> Result<()> {
    let meta = root(folder)?;
    portable(project, folder, &meta, 0o1777)?;
    let mut stack = vec![Level {
        rel: Vec::new(),
        meta,
        children: children(folder)?,
    }];
    while let Some(level) = stack.last_mut() {
        let Some(name) = level.children.pop() else {
            let level = stack.pop().expect("a level");
            #[cfg(unix)]
            let path = folder.join(OsStr::from_bytes(&level.rel));
            #[cfg(windows)]
            let path = folder.join(from_bytes(&level.rel));
            visit(&level.rel, &path, Visit::Leave(&level.meta))?;
            continue;
        };
        let mut rel = level.rel.clone();
        if !rel.is_empty() {
            rel.push(b'/');
        }
        #[cfg(unix)]
        let name_bytes = name.as_bytes().to_vec();
        #[cfg(windows)]
        let name_bytes = as_bytes(name.as_os_str()).to_vec();
        rel.extend_from_slice(&name_bytes);
        #[cfg(unix)]
        let path = folder.join(OsStr::from_bytes(&rel));
        #[cfg(windows)]
        let path = folder.join(from_bytes(&rel));
        PathName::from_bytes(&name_bytes).map_err(|e| refusal(project, &path, &e))?;
        let meta = fs::symlink_metadata(&path).map_err(|e| io(&path, e))?;
        let kind = meta.file_type();
        if kind.is_symlink() {
            let target = fs::read_link(&path).map_err(|e| io(&path, e))?;
            #[cfg(unix)]
            let target = target.into_os_string().into_vec();
            #[cfg(windows)]
            let target = into_vec(target.into_os_string());
            SymlinkTarget::new(target.clone()).map_err(|e| refusal(project, &path, &e))?;
            visit(&rel, &path, Visit::Link(&meta, &target))?;
        } else if kind.is_dir() {
            portable(project, &path, &meta, 0o1777)?;
            visit(&rel, &path, Visit::Enter)?;
            let children = children(&path)?;
            stack.push(Level {
                rel,
                meta,
                children,
            });
        } else if kind.is_file() {
            portable(project, &path, &meta, 0o777)?;
            visit(&rel, &path, Visit::File(&meta))?;
        } else {
            let why = "a socket, FIFO or device file; Init takes regular files, directories and symbolic links";
            return Err(refusal(project, &path, &why));
        }
    }
    Ok(())
}

/// Walks `folder` once without sending: refuses what Init would refuse and
/// answers the bytes of its regular files.
pub fn measure(folder: &Path, project: &str) -> Result<u64> {
    let mut total = 0u64;
    walk(folder, project, &mut |_, _, visit| {
        if let Visit::File(meta) = visit {
            total = total.saturating_add(meta.len());
        }
        Ok(())
    })?;
    Ok(total)
}

/// Refuses `Capacity` unless the machine has the copy's `total` bytes and
/// a store push's headroom free where its Stores are; asks its worker.
pub fn room(worker: &mut Worker, copy: &Copy<'_>, total: u64) -> Result<()> {
    let stat = SdkValue::map([
        ("op", SdkValue::text("stat")),
        (
            "args",
            SdkValue::map([("store", SdkValue::text(copy.store))]),
        ),
    ]);
    let there = worker.call("store.file", stat, &mut |_| {})?;
    let machine = copy.machine;
    let Some(free) = there.get_int("free") else {
        return Err(refused(
            "Unsupported",
            format!("machine {machine}'s worker does not report its free disk (an older bundle)"),
        ));
    };
    let headroom = crate::sqlite::transfer::phases::HEADROOM.max(total / 10);
    let needed = total.saturating_add(headroom);
    let free = free.max(0) as u64;
    if free < needed {
        let mut refusal = refused(
            "Capacity",
            format!(
                "machine {machine} has {free} bytes free; a copy of {} needs {needed} ({total} bytes and {headroom} of headroom)",
                copy.folder.display()
            ),
        );
        let int = |n: u64| SdkValue::Int(n.min(i64::MAX as u64) as i64);
        refusal.detail = SdkValue::map([("needed", int(needed)), ("free", int(free))]);
        return Err(refusal);
    }
    Ok(())
}

struct Sender<'s, 'c> {
    events: &'s mut Events<'c>,
    parts: Parts,
    project: &'s str,
    /// The data part being filled (`D`, `W` and `L` records).
    data: Vec<u8>,
    /// File bytes in `data`.
    pending: u64,
    /// The `M` records not sent yet.
    meta: Vec<u8>,
    buffer: Vec<u8>,
}

impl Sender<'_, '_> {
    /// Sends the data part being filled, if it holds anything.
    fn flush(&mut self) -> Result<()> {
        if self.data.is_empty() {
            return Ok(());
        }
        let part = std::mem::take(&mut self.data);
        let bytes = std::mem::take(&mut self.pending);
        self.parts.send(self.events, part, bytes)
    }

    /// Sends the `M` records gathered, if any, once every part before them
    /// was answered: the data they follow is written by then.
    fn flush_meta(&mut self) -> Result<()> {
        self.flush()?;
        self.parts.settle(self.events);
        self.parts.check()?;
        if self.meta.is_empty() {
            return Ok(());
        }
        let part = std::mem::take(&mut self.meta);
        self.parts.send(self.events, part, 0)
    }

    /// Adds `record` to its part, sending that part first when it would
    /// not fit.
    fn push(&mut self, record: Record<'_>) -> Result<()> {
        let size = record.size();
        if let Record::Meta { .. } = record {
            if self.meta.len() + size > CHUNK as usize {
                self.flush_meta()?;
            }
            record.encode(&mut self.meta);
            return Ok(());
        }
        if self.data.len() + size > CHUNK as usize {
            self.flush()?;
        }
        record.encode(&mut self.data);
        Ok(())
    }

    fn visit(&mut self, rel: &[u8], path: &Path, visit: Visit<'_>) -> Result<()> {
        match visit {
            Visit::Enter => self.push(Record::Directory { path: rel }),
            Visit::File(meta) => {
                self.file(rel, path, meta)?;
                self.meta(rel, meta)
            }
            Visit::Link(meta, target) => {
                self.push(Record::Link { path: rel, target })?;
                self.meta(rel, meta)
            }
            Visit::Leave(meta) => self.meta(rel, meta),
        }
    }

    fn meta(&mut self, rel: &[u8], meta: &Metadata) -> Result<()> {
        // Windows face: writable files read 0644, dirs 0755 (the portable
        // mask); mtime carries seconds with nanoseconds folded in.
        #[cfg(unix)]
        let (mode, seconds, nanoseconds) = {
            use std::os::unix::fs::MetadataExt;
            (meta.mode() & 0o7777, meta.mtime(), meta.mtime_nsec() as u32)
        };
        #[cfg(windows)]
        let (mode, seconds, nanoseconds) = {
            let readonly = meta.permissions().readonly();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .unwrap_or_default();
            (
                if meta.is_dir() {
                    if readonly { 0o555 } else { 0o755 }
                } else if readonly {
                    0o444
                } else {
                    0o644
                },
                mtime.as_secs() as i64,
                mtime.subsec_nanos(),
            )
        };
        self.push(Record::Meta {
            path: rel,
            mode,
            seconds,
            nanoseconds,
        })
    }

    /// The file's bytes as `W` pieces; refused when it changes meanwhile.
    fn file(&mut self, rel: &[u8], path: &Path, meta: &Metadata) -> Result<()> {
        let changed = || refusal(self.project, path, &SourceRefusal::Changed);
        #[cfg(unix)]
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)
                .map_err(|e| io(path, e))?
        };
        // Windows: FILE_FLAG_OPEN_REPARSE_POINT opens the link itself;
        // symlink_metadata above has already refused one.
        #[cfg(windows)]
        let mut file = fs::OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|e| io(path, e))?;
        let opened = file.metadata().map_err(|e| io(path, e))?;
        if !same(meta, &opened) {
            return Err(changed());
        }
        let len = meta.len();
        let fixed = header(rel) + 8 + 4;
        let mut offset = 0u64;
        loop {
            let mut room = (CHUNK as usize).saturating_sub(self.data.len() + fixed);
            let left = len - offset;
            if room < MIN_PIECE.min(left as usize) {
                self.flush()?;
                room = CHUNK as usize - fixed;
            }
            let want = room.min(left as usize);
            self.buffer.resize(want, 0);
            let mut buffer = std::mem::take(&mut self.buffer);
            file.read_exact(&mut buffer).map_err(|e| match e.kind() {
                std::io::ErrorKind::UnexpectedEof => changed(),
                _ => io(path, e),
            })?;
            let record = Record::Write {
                path: rel,
                offset,
                bytes: &buffer,
            };
            let pushed = self.push(record);
            self.buffer = buffer;
            pushed?;
            offset += want as u64;
            self.pending += want as u64;
            if offset == len {
                break;
            }
        }
        let after = file.metadata().map_err(|e| io(path, e))?;
        if !same(meta, &after) {
            return Err(changed());
        }
        Ok(())
    }
}

/// The same unchanged regular file, as Init compares one.
fn same(a: &Metadata, b: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.len() == b.len()
            && a.mode() == b.mode()
            && (a.mtime(), a.mtime_nsec()) == (b.mtime(), b.mtime_nsec())
            && (a.ctime(), a.ctime_nsec()) == (b.ctime(), b.ctime_nsec())
    }
    // Windows: size, readonly mode and write time carry the comparison; the
    // file-index pair (volume serial, index) names the same file where the
    // import's StampFace has it.
    #[cfg(windows)]
    {
        a.len() == b.len()
            && a.permissions().readonly() == b.permissions().readonly()
            && a.modified().ok() == b.modified().ok()
            && a.created().ok() == b.created().ok()
    }
}

/// Copies `copy.folder` (`total` file bytes, from [`measure`]) to the
/// machine's staging folder, reporting `send` progress as parts are
/// answered. A certain failure discards the copy (one request) and answers
/// the first failure; a lost answer is `Unknown`, naming the copy it may
/// leave.
pub fn send(events: &mut Events<'_>, worker: &Worker, copy: &Copy<'_>, total: u64) -> Result<()> {
    crate::sqlite::family::progress(events, "send", Some((0, total)));
    let mut sender = Sender {
        events,
        parts: Parts::new(worker, copy, total),
        project: copy.name,
        data: Vec::new(),
        pending: 0,
        meta: Vec::new(),
        buffer: Vec::new(),
    };
    let walked = walk(copy.folder, copy.name, &mut |rel, path, visit| {
        sender.visit(rel, path, visit)
    });
    let walked = walked.and_then(|()| sender.flush_meta());
    let Sender { events, parts, .. } = sender;
    parts.end(events, walked)
}

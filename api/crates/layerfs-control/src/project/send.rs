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
use super::tree::{header, Record};
use crate::common::context::Context;
use crate::common::outcome::refused;
use crate::common::place::Worker;
use crate::machine::worker::Unreached;
use crate::store::transfer::steps::CHUNK;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::filesystem::symlink::SymlinkTarget;
use layerfs_content::PathName;
use layerfs_project::SourceRefusal;
use std::ffi::{OsStr, OsString};
use std::fs::{self, Metadata};
use std::io::Read;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The smallest file piece worth a part's remaining room; less starts the
/// next part.
const MIN_PIECE: usize = 64 << 10;

/// What one copy names.
pub(crate) struct Copy<'a> {
    pub store: &'a str,
    pub name: &'a str,
    pub machine: &'a str,
    pub folder: &'a Path,
    /// The staging folder's id on the machine.
    pub id: String,
}

/// A fresh copy id: 16 hex digits.
pub(crate) fn new_id() -> Result<String> {
    let bytes = crate::sandbox::registry::random()?;
    Ok(layerfs_bridge::sdk_values::hex(&bytes[..8]))
}

/// Where copy `id` may stay on `machine` after an uncertain end.
pub(crate) fn left(machine: &str, id: &str) -> String {
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
    let mode = meta.mode() & 0o7777;
    if mode & !mask != 0 {
        return Err(refusal(project, path, &SourceRefusal::Mode(mode)));
    }
    if !(0..1_000_000_000).contains(&meta.mtime_nsec()) {
        return Err(refusal(project, path, &SourceRefusal::Mtime));
    }
    Ok(())
}

/// The folder as the caller sees it: an existing directory, not a link.
pub(crate) fn root(folder: &Path) -> Result<Metadata> {
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
            let path = folder.join(OsStr::from_bytes(&level.rel));
            visit(&level.rel, &path, Visit::Leave(&level.meta))?;
            continue;
        };
        let mut rel = level.rel.clone();
        if !rel.is_empty() {
            rel.push(b'/');
        }
        rel.extend_from_slice(name.as_bytes());
        let path = folder.join(OsStr::from_bytes(&rel));
        PathName::from_bytes(name.as_bytes()).map_err(|e| refusal(project, &path, &e))?;
        let meta = fs::symlink_metadata(&path).map_err(|e| io(&path, e))?;
        let kind = meta.file_type();
        if kind.is_symlink() {
            let target = fs::read_link(&path).map_err(|e| io(&path, e))?;
            let target = target.into_os_string().into_vec();
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
pub(crate) fn measure(folder: &Path, project: &str) -> Result<u64> {
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
pub(crate) fn room(worker: &mut Worker, copy: &Copy<'_>, total: u64) -> Result<()> {
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
    let headroom = crate::store::transfer::phases::HEADROOM.max(total / 10);
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

struct Sender<'s, 'c, 'p> {
    context: &'s mut Context<'c>,
    worker: &'s mut Worker,
    copy: &'s Copy<'p>,
    /// The part being filled.
    part: Vec<u8>,
    /// File bytes in `part`.
    pending: u64,
    /// File bytes the worker has written.
    sent: u64,
    total: u64,
    /// Whether a part reached the worker.
    asked: bool,
    buffer: Vec<u8>,
}

impl Sender<'_, '_, '_> {
    fn request(&self, step: &str, part: Option<Vec<u8>>) -> SdkValue {
        let mut fields = vec![
            ("store", SdkValue::text(self.copy.store)),
            ("name", SdkValue::text(self.copy.name)),
            ("staged", SdkValue::text(&self.copy.id)),
            ("step", SdkValue::text(step)),
        ];
        if let Some(part) = part {
            fields.push(("part", SdkValue::Bytes(part)));
        }
        let fields = fields.into_iter().map(|(n, v)| (n.to_owned(), v));
        SdkValue::Map(fields.collect())
    }

    /// Sends the part being filled, if it holds anything.
    fn flush(&mut self) -> Result<()> {
        if self.part.is_empty() {
            return Ok(());
        }
        let part = std::mem::take(&mut self.part);
        let request = self.request("write", Some(part));
        // a worker not reached was sent nothing
        match self.worker.attempt("project.stage", request, &mut |_| {}) {
            Ok(outcome) => {
                self.asked = true;
                outcome?
            }
            Err(Unreached(refusal)) => return Err(refusal),
        };
        self.sent += std::mem::take(&mut self.pending);
        let done = Some((self.sent, self.total));
        crate::store::family::progress(self.context, "send", done);
        Ok(())
    }

    /// Adds `record`, sending the part first when it would not fit.
    fn push(&mut self, record: Record<'_>) -> Result<()> {
        if self.part.len() + record.size() > CHUNK as usize {
            self.flush()?;
        }
        record.encode(&mut self.part);
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
        self.push(Record::Meta {
            path: rel,
            mode: meta.mode() & 0o7777,
            seconds: meta.mtime(),
            nanoseconds: meta.mtime_nsec() as u32,
        })
    }

    /// The file's bytes as `W` pieces; refused when it changes meanwhile.
    fn file(&mut self, rel: &[u8], path: &Path, meta: &Metadata) -> Result<()> {
        let changed = || refusal(self.copy.name, path, &SourceRefusal::Changed);
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
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
            let mut room = (CHUNK as usize).saturating_sub(self.part.len() + fixed);
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
    a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.len() == b.len()
        && a.mode() == b.mode()
        && (a.mtime(), a.mtime_nsec()) == (b.mtime(), b.mtime_nsec())
        && (a.ctime(), a.ctime_nsec()) == (b.ctime(), b.ctime_nsec())
}

/// Copies `copy.folder` (`total` file bytes, from [`measure`]) to the
/// machine's staging folder, reporting `send` progress. A certain failure
/// discards the copy (one request) and answers the failure; a lost answer
/// is `Unknown`, naming the copy it may leave.
pub(crate) fn send(
    context: &mut Context<'_>,
    worker: &mut Worker,
    copy: &Copy<'_>,
    total: u64,
) -> Result<()> {
    crate::store::family::progress(context, "send", Some((0, total)));
    let mut sender = Sender {
        context,
        worker,
        copy,
        part: Vec::new(),
        pending: 0,
        sent: 0,
        total,
        asked: false,
        buffer: Vec::new(),
    };
    let walked = walk(copy.folder, copy.name, &mut |rel, path, visit| {
        sender.visit(rel, path, visit)
    });
    let Err(mut refusal) = walked.and_then(|()| sender.flush()) else {
        return Ok(());
    };
    let left = left(copy.machine, &copy.id);
    if refusal.code == "Unknown" {
        refusal.message = format!("{}; its copy may remain at {left}", refusal.message);
        return Err(refusal);
    }
    if !sender.asked {
        return Err(refusal);
    }
    let discard = sender.request("discard", None);
    if let Err(why) = sender.worker.call("project.stage", discard, &mut |_| {}) {
        refusal.message = format!(
            "{}; its copy was not discarded ({}) and may remain at {left}",
            refusal.message, why.message
        );
    }
    Err(refusal)
}

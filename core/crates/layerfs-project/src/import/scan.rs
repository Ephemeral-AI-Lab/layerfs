//! One operator-bound directory scan into the acquisition backing.
//!
//! Positions are assigned breadth first with each directory's children in name
//! byte order, so a parent's children are contiguous and ordered. Each observed
//! child has its attribute root, and a symlink its target root, constructed as
//! it is placed, so its row is written complete. One directory stream, one
//! fixed child buffer and one write window are resident; a directory wider than
//! the child buffer is written unpositioned and ordered by the backing.
use crate::backing::{placed_bytes, window_full, Backing, Bound, Pending, Stamp, WINDOW_ROWS};
use crate::error::content;
use crate::error::ProjectError as Failure;
use crate::metadata::build_metadata;
use crate::namespace::ImportProgress;
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::symlink::{emit_symlink, SymlinkTarget};
use layerfs_content::filesystem::FilesystemObjects;
use layerfs_content::{inode_leaf::InodeKind, ObjectId, PathName};
use layerfs_storage::port::acquisition::{EntryKey, NewEntry, Placed};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, Metadata},
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

/// What one completed scan placed in backing.
pub(crate) struct Scanned {
    /// Entries, including the root directory.
    pub entries: u64,
    /// True when at least one directory bound no child.
    pub childless: bool,
    /// Later native paths that share an earlier path's regular-file identity.
    pub aliases: usize,
}

/// Refuses a source that is not itself one real directory.
pub(crate) fn check_root(source: &Path) -> Result<Metadata, Failure> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Failure::InvalidInput);
    }
    Ok(metadata)
}

/// One observed directory child before it receives its acquisition position.
struct Child {
    name: Vec<u8>,
    kind: InodeKind,
    target: Option<Vec<u8>>,
    stamp: Stamp,
}
impl Child {
    /// Portable mode: the symlink grammar fixes 0777; other kinds stay exact.
    fn portable_mode(&self) -> Result<u32, Failure> {
        let (mode, mask) = match self.kind {
            InodeKind::Symlink => (0o777, 0o777),
            InodeKind::Directory => (self.stamp.mode & 0o7777, 0o1777),
            InodeKind::RegularFile => (self.stamp.mode & 0o7777, 0o777),
        };
        if mode & !mask != 0 || !(0..1_000_000_000).contains(&self.stamp.mtime_nsec) {
            return Err(Failure::InvalidInput);
        }
        Ok(mode)
    }
}

/// Attribute and symlink-target construction into the prerequisite save.
struct Roots<'o, 'p> {
    objects: &'o mut FilesystemObjects<'p>,
    // Adjacent equal portable fields have the same canonical metadata root.
    // One slot avoids an entry-count-sized cache on heterogeneous imports.
    previous: Option<((u8, u32, i64, u32), ObjectId)>,
}
impl Roots<'_, '_> {
    /// The complete backing row of `child` under `directory`; refuses a
    /// nonportable mode.
    fn entry(
        &mut self,
        child: Child,
        parent: Option<u64>,
        position: Option<u64>,
        directory: &Path,
    ) -> Result<NewEntry, Failure> {
        let nanos = child.stamp.mtime_nsec as u32;
        let key = (
            child.kind.code(),
            child.portable_mode()?,
            child.stamp.mtime,
            nanos,
        );
        let metadata_root = match self.previous {
            Some((previous, root)) if previous == key => root,
            _ => {
                let value = PortableMetadata {
                    mode: key.1,
                    mtime_seconds: key.2,
                    mtime_nanoseconds: nanos,
                };
                let root = build_metadata(self.objects, child.kind, value)?;
                self.previous = Some((key, root));
                root
            }
        };
        let target_root = match child.target {
            Some(target) => {
                let target = SymlinkTarget::new(target).map_err(content)?;
                Some(emit_symlink(self.objects, target).map_err(content)?)
            }
            None => None,
        };
        let path = || match parent {
            Some(_) => child_path(directory, &child.name),
            None => directory.as_os_str().as_bytes().to_vec(),
        };
        let (native_path, native) = match child.kind {
            InodeKind::Directory => (Some(path()), None),
            InodeKind::RegularFile => (Some(path()), Some(child.stamp.identity())),
            InodeKind::Symlink => (None, None),
        };
        Ok(NewEntry {
            key: EntryKey {
                parent,
                name: child.name,
            },
            position,
            kind: child.kind,
            metadata_root,
            target_root,
            native_path,
            native,
        })
    }
}

fn child_path(directory: &Path, name: &[u8]) -> Vec<u8> {
    directory
        .join(OsStr::from_bytes(name))
        .into_os_string()
        .into_vec()
}

/// Positions being assigned while directories are read.
struct Placement {
    next: u64,
    frontier: usize,
    bound: Bound,
    pending: Pending,
}
impl Placement {
    /// The next position, counted against the frontier when it is a directory.
    fn assign(&mut self, kind: InodeKind) -> Result<u64, Failure> {
        let id = self.next;
        self.next = id.checked_add(1).ok_or(Failure::Capacity)?;
        if kind == InodeKind::Directory {
            self.frontier += 1;
        }
        Ok(id)
    }
}

/// Places every entry of `source` in backing through the open prerequisite save.
pub(crate) fn scan(
    source: &Path,
    root: &Metadata,
    backing: &Backing<'_>,
    objects: &mut FilesystemObjects<'_>,
    progress: &mut ImportProgress,
) -> Result<Scanned, Failure> {
    let mut roots = Roots {
        objects,
        previous: None,
    };
    let mut placement = Placement {
        next: 1,
        frontier: 1,
        bound: Bound::default(),
        pending: Pending::default(),
    };
    let root = Child {
        name: Vec::new(),
        kind: InodeKind::Directory,
        target: None,
        stamp: Stamp::of(root),
    };
    let root = roots.entry(root, None, Some(0), source)?;
    placement
        .pending
        .push(backing, root, &mut placement.bound)?;
    let mut directories = backing.directories();
    let mut childless = false;
    let mut buffer: Vec<Child> = Vec::new();
    loop {
        // A directory window is read only after every placed entry is written.
        if directories.exhausted_window() {
            placement.pending.flush(backing, &mut placement.bound)?;
        }
        let Some(directory) = directories.next()? else {
            break;
        };
        progress.work.frontier = progress.work.frontier.max(placement.frontier);
        placement.frontier -= 1;
        progress.tick()?;
        let parent = directory.position;
        let path = PathBuf::from(OsString::from_vec(directory.native_path));
        let (mut count, mut wide) = (0usize, false);
        for child in fs::read_dir(&path)? {
            progress.tick()?;
            let child = child?;
            buffer.push(observe(&child.path(), child.file_name().as_bytes())?);
            count += 1;
            if buffer.len() == WINDOW_ROWS {
                // A wide directory is ordered by the backing, not by a resident sort.
                wide = true;
                progress.work.child_window_rows = WINDOW_ROWS;
                for child in buffer.drain(..) {
                    let entry = roots.entry(child, Some(parent), None, &path)?;
                    placement
                        .pending
                        .push(backing, entry, &mut placement.bound)?;
                }
            }
        }
        let work = &mut progress.work;
        work.directory_children = work.directory_children.max(count);
        work.child_window_rows = work.child_window_rows.max(buffer.len());
        childless |= count == 0;
        if wide {
            work.wide_directories += 1;
            for child in buffer.drain(..) {
                let entry = roots.entry(child, Some(parent), None, &path)?;
                placement
                    .pending
                    .push(backing, entry, &mut placement.bound)?;
            }
            placement.pending.flush(backing, &mut placement.bound)?;
            place_wide(backing, parent, &path, &mut placement, progress)?;
        } else {
            buffer.sort_unstable_by(|a, b| a.name.cmp(&b.name));
            for child in buffer.drain(..) {
                let id = placement.assign(child.kind)?;
                let entry = roots.entry(child, Some(parent), Some(id), &path)?;
                placement
                    .pending
                    .push(backing, entry, &mut placement.bound)?;
            }
        }
    }
    let work = &mut progress.work;
    work.entries = usize::try_from(placement.next).map_err(|_| Failure::Capacity)?;
    work.unique_files = placement.bound.unique;
    work.regular_aliases = placement.bound.aliases;
    work.jobs = placement.bound.unique + placement.bound.aliases;
    Ok(Scanned {
        entries: placement.next,
        childless,
        aliases: placement.bound.aliases,
    })
}

/// Gives a wide directory's children their positions in name order, one
/// bounded read window and one bounded write window at a time.
fn place_wide(
    backing: &Backing<'_>,
    parent: u64,
    directory: &Path,
    placement: &mut Placement,
    progress: &mut ImportProgress,
) -> Result<(), Failure> {
    let mut after: Option<Vec<u8>> = None;
    let mut placed: Vec<Placed> = Vec::new();
    let mut held = 0usize;
    loop {
        let window = backing.unplaced(parent, after.as_deref())?;
        let Some(last) = window.last() else {
            return backing.place(parent, &mut placed, &mut placement.bound);
        };
        after = Some(last.name.clone());
        for child in window {
            progress.tick()?;
            let native = child
                .native
                .map(|identity| (identity, child_path(directory, &child.name)));
            let child = Placed {
                position: placement.assign(child.kind)?,
                name: child.name,
                native,
            };
            let bytes = placed_bytes(&child);
            if window_full(placed.len(), held, bytes) {
                backing.place(parent, &mut placed, &mut placement.bound)?;
                held = 0;
            }
            held += bytes;
            placed.push(child);
        }
    }
}

/// Observes one child without following it; an unsupported kind is refused.
fn observe(path: &Path, name: &[u8]) -> Result<Child, Failure> {
    PathName::from_bytes(name).map_err(content)?;
    let metadata = fs::symlink_metadata(path)?;
    let (kind, target) = if metadata.file_type().is_symlink() {
        let target = super::source::read_link(path, &metadata)?;
        (InodeKind::Symlink, Some(target))
    } else if metadata.is_dir() {
        (InodeKind::Directory, None)
    } else if metadata.is_file() {
        (InodeKind::RegularFile, None)
    } else {
        return Err(Failure::Unsupported);
    };
    Ok(Child {
        name: name.to_vec(),
        kind,
        target,
        stamp: Stamp::of(&metadata),
    })
}

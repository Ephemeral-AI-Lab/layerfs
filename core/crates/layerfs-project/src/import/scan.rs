//! One operator-bound directory scan into backed acquisition order.
//!
//! Positions are assigned breadth first with each directory's children in name
//! byte order, so a parent's children are contiguous and ordered. Entries, the
//! directory frontier and regular-file identities are written to scratch runs;
//! one directory stream and one fixed child buffer are resident, and a directory
//! wider than that buffer is ordered by the backed sorter.
use crate::error::content;
use crate::error::ProjectError as Failure;
use crate::namespace::ImportProgress;
use crate::runs::{Cursor, Run, Sorter, Writer};
use crate::scratch::{
    by_identity, by_name, by_position, decode_frontier, frontier_record, pair_record, Child, Job,
    Scratch, Stamp, WINDOW_ROWS,
};
use layerfs_content::{inode_leaf::InodeKind, PathName};
use std::{
    ffi::OsStr,
    fs::{self, Metadata},
    os::unix::ffi::OsStrExt,
    path::Path,
};

/// What one completed scan placed in backing.
pub(crate) struct Scanned {
    /// Entries, including the root directory.
    pub entries: u64,
    /// True when at least one directory bound no child.
    pub childless: bool,
    /// Every entry in position order.
    pub entry_run: Run,
}

/// Regular-file identities after later paths were bound to their first.
pub(crate) struct Identities {
    /// First path of every native identity, in identity order.
    pub jobs: Run,
    /// Every later path with its first position, in position order.
    pub aliases: Run,
    /// First position and later-path count of each shared identity, by position.
    pub counts: Run,
}

/// Refuses a source that is not itself one real directory.
pub(crate) fn check_root(source: &Path) -> Result<Metadata, Failure> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Failure::InvalidInput);
    }
    Ok(metadata)
}

/// Positions being assigned while one directory's children are placed.
struct Placement {
    next: u64,
    frontier: usize,
    files: usize,
    entries: Writer,
    natives: Sorter,
}
impl Placement {
    fn place(
        &mut self,
        scratch: &mut Scratch,
        level: &mut Writer,
        parent: u64,
        directory: &Path,
        child: &Child,
    ) -> Result<(), Failure> {
        let id = self.next;
        self.next = id.checked_add(1).ok_or(Failure::Capacity)?;
        self.entries.push(&child.entry(id, parent)?)?;
        let path = directory.join(OsStr::from_bytes(&child.name));
        match child.kind {
            InodeKind::Directory => {
                self.frontier += 1;
                level.push(&frontier_record(id, &path))
            }
            InodeKind::RegularFile => {
                self.files += 1;
                let job = Job {
                    id,
                    canonical: id,
                    path,
                    stamp: child.stamp,
                };
                self.natives.push(scratch.backing(), &job.native_record())
            }
            InodeKind::Symlink => Ok(()),
        }
    }
}

/// Returns the entries and the regular-file identities in identity order.
pub(crate) fn scan(
    source: &Path,
    root: &Metadata,
    scratch: &mut Scratch,
    progress: &mut ImportProgress,
) -> Result<(Scanned, Run), Failure> {
    let root = Child {
        name: Vec::new(),
        kind: InodeKind::Directory,
        target: None,
        stamp: Stamp::of(root),
    };
    let mut placement = Placement {
        next: 1,
        frontier: 1,
        files: 0,
        entries: Writer::new(scratch.backing())?,
        natives: Sorter::new(by_identity, progress.deadline()),
    };
    placement.entries.push(&root.entry(0, 0)?)?;
    let mut level = Writer::new(scratch.backing())?;
    level.push(&frontier_record(0, source))?;
    let mut level = level.finish()?;
    let mut wide = Sorter::new(by_name, progress.deadline());
    let mut childless = false;
    let mut buffer: Vec<Child> = Vec::new();
    // One frontier run per depth: read to its end while the next is written.
    while !level.is_empty() {
        let mut deeper = Writer::new(scratch.backing())?;
        let mut pending = Cursor::new(&level, decode_frontier);
        while let Some((parent, directory)) = pending.next()? {
            progress.work.frontier = progress.work.frontier.max(placement.frontier);
            placement.frontier -= 1;
            progress.tick()?;
            let (mut count, mut spilled) = (0usize, false);
            for child in fs::read_dir(&directory)? {
                progress.tick()?;
                let child = child?;
                buffer.push(observe(&child.path(), child.file_name().as_bytes())?);
                count += 1;
                if buffer.len() == WINDOW_ROWS {
                    // A wide directory is ordered in backing, not by a resident sort.
                    for child in buffer.drain(..) {
                        wide.push(scratch.backing(), &child.record()?)?;
                    }
                    spilled = true;
                }
            }
            progress.work.directory_children = progress.work.directory_children.max(count);
            progress.work.child_vector_bytes = progress
                .work
                .child_vector_bytes
                .max(buffer.capacity() * std::mem::size_of::<Child>());
            childless |= count == 0;
            if spilled {
                for child in buffer.drain(..) {
                    wide.push(scratch.backing(), &child.record()?)?;
                }
                let ordered = wide.finish(scratch.backing())?;
                let mut children = Cursor::new(&ordered, Child::decode);
                while let Some(child) = children.next()? {
                    progress.tick()?;
                    placement.place(scratch, &mut deeper, parent, &directory, &child)?;
                }
            } else {
                buffer.sort_unstable_by(|a, b| a.name.cmp(&b.name));
                for child in buffer.drain(..) {
                    placement.place(scratch, &mut deeper, parent, &directory, &child)?;
                }
            }
        }
        progress.work.frontier_capacity_bytes = progress
            .work
            .frontier_capacity_bytes
            .max(pending.resident_bytes());
        drop(pending);
        level = deeper.finish()?;
    }
    let work = &mut progress.work;
    work.entries = usize::try_from(placement.next).map_err(|_| Failure::Capacity)?;
    work.jobs = placement.files;
    let natives = placement.natives.finish(scratch.backing())?;
    work.sort_capacity_bytes = work
        .sort_capacity_bytes
        .max(wide.resident_bytes())
        .max(placement.natives.resident_bytes());
    let scanned = Scanned {
        entries: placement.next,
        childless,
        entry_run: placement.entries.finish()?,
    };
    Ok((scanned, natives))
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

/// Binds every later path of one native identity to its first entry, leaving
/// one constructed regular file per `(dev, ino)`.
pub(crate) fn group_aliases(
    natives: Run,
    scratch: &mut Scratch,
    progress: &mut ImportProgress,
) -> Result<Identities, Failure> {
    let mut ordered = Cursor::new(&natives, Job::native);
    let mut jobs = Writer::new(scratch.backing())?;
    let mut aliases = Sorter::new(by_position, progress.deadline());
    let mut counts = Sorter::new(by_position, progress.deadline());
    // First position, its evidence and the later paths seen for it so far.
    let mut first: Option<(u64, Stamp, u64)> = None;
    let mut groups = 0;
    loop {
        let job = ordered.next()?;
        progress.tick()?;
        match (&mut first, &job) {
            (Some((canonical, stamp, later)), Some(job))
                if stamp.dev == job.stamp.dev && stamp.ino == job.stamp.ino =>
            {
                if *stamp != job.stamp {
                    return Err(Failure::InvalidInput);
                }
                let alias = Job {
                    id: job.id,
                    canonical: *canonical,
                    path: job.path.clone(),
                    stamp: job.stamp,
                };
                aliases.push(scratch.backing(), &alias.alias_record())?;
                *later += 1;
                continue;
            }
            (Some((canonical, _, later)), _) if *later > 0 => {
                counts.push(scratch.backing(), &pair_record(*canonical, *later))?;
            }
            _ => (),
        }
        let Some(job) = job else {
            break;
        };
        jobs.push(&job.native_record())?;
        first = Some((job.id, job.stamp, 0));
        groups += 1;
    }
    let work = &mut progress.work;
    work.unique_files = groups;
    work.regular_aliases = work.jobs - groups;
    work.job_capacity_bytes = ordered.resident_bytes();
    drop(ordered);
    let identities = Identities {
        jobs: jobs.finish()?,
        aliases: aliases.finish(scratch.backing())?,
        counts: counts.finish(scratch.backing())?,
    };
    work.sort_capacity_bytes = work
        .sort_capacity_bytes
        .max(aliases.resident_bytes())
        .max(counts.resident_bytes());
    Ok(identities)
}

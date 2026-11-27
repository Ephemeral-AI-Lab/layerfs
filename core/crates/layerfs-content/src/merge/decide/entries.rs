//! The decision between three entries, with the simulator's rules: an entry
//! is its kind plus, for a file or symlink, its content (two directories are
//! the same entry; their children decide). The entries are one serial on each
//! side, or what one name binds on each side (#355 M1).

use std::collections::HashMap;

use layerfs_telemetry::timer::{Active, TimingScope};

use super::super::rule::lww::Rules;
use super::super::{Conflict, ConflictKind, LwwDecision};
use super::text::{deletion_markers, merge3, Hunk};
use crate::compare::bytes::same_content;
use crate::compare::text::{as_text, MAXIMUM_TEXT_BYTES};
use crate::compare::tree::{EntryFacts, TreeTable};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::name::path::PathName;
use crate::filesystem::root::FilesystemRootId;
use crate::filesystem::tree::attribute::{read_portable, AttributeReadWork};
use crate::filesystem::tree::directory::read::lookup;
use crate::filesystem::tree::pages::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::AuthenticatedObjects;

/// Paths one side's resolution cache holds before it starts again: a cache,
/// so its bound changes work, never an answer.
const RESOLVED: usize = 1 << 16;

/// Base, ours and theirs, in that order.
pub(super) const BASE: usize = 0;
pub(super) const OURS: usize = 1;
pub(super) const THEIRS: usize = 2;

/// The result at one decided path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Pick {
    /// Whatever theirs holds (possibly nothing).
    Keep,
    /// Nothing.
    Absent,
    /// Our entry.
    Take(EntryFacts),
    /// New file bytes, spilled as text `text`, with the kind and metadata of
    /// `like`.
    Write { like: EntryFacts, text: u64 },
}

/// Path resolution in the three trees, with a bounded cache per side.
pub(super) struct Resolver<'r> {
    pub(super) reader: &'r dyn AuthenticatedObjects,
    pub(super) roots: [FilesystemRootId; 3],
    pub(super) trees: [TreeTable; 3],
    cache: [HashMap<String, Option<EntryFacts>>; 3],
}

/// Parent path and last name of an absolute path other than the root.
pub(super) fn split(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(0) => ("/", &path[1..]),
        Some(index) => (&path[..index], &path[index + 1..]),
        None => ("/", path),
    }
}

impl<'r> Resolver<'r> {
    pub(super) fn new(
        reader: &'r dyn AuthenticatedObjects,
        roots: [FilesystemRootId; 3],
    ) -> ContentResult<Self> {
        let open = |root| TreeTable::open(reader, root);
        Ok(Self {
            reader,
            roots,
            trees: [open(roots[0])?, open(roots[1])?, open(roots[2])?],
            cache: Default::default(),
        })
    }

    /// The entry at `path` in one tree.
    pub(super) fn entry(&mut self, side: usize, path: &str) -> ContentResult<Option<EntryFacts>> {
        if let Some(hit) = self.cache[side].get(path) {
            return Ok(*hit);
        }
        let tree = self.trees[side];
        let serial = if path == "/" {
            Some(tree.root_serial())
        } else {
            let (parent, name) = split(path);
            match self.entry(side, parent)? {
                Some(dir) if dir.value.kind == InodeKind::Directory => lookup(
                    self.reader,
                    DirectoryRoot(dir.value.content_root),
                    &PathName::new(name)?,
                )?,
                _ => None,
            }
        };
        let found = match serial {
            Some(serial) => Some(EntryFacts {
                serial,
                value: tree
                    .value(self.reader, serial)?
                    .ok_or(ContentError::InvalidRecord("bound inode record"))?,
            }),
            None => None,
        };
        if self.cache[side].len() >= RESOLVED {
            self.cache[side].clear();
        }
        self.cache[side].insert(path.to_owned(), found);
        Ok(found)
    }

    /// The serial's value in one tree.
    pub(super) fn value(&self, side: usize, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.trees[side].value(self.reader, serial)
    }
}

/// Everything one decision pass needs besides the resolver.
pub(super) struct Decider<'a> {
    pub(super) labels: [&'a str; 2],
    pub(super) rules: &'a Rules,
    pub(super) timing: &'a TimingScope<'a, Active>,
    pub(super) conflicts: Vec<Conflict>,
    pub(super) lww: Vec<LwwDecision>,
    /// The largest file whose bytes [`Self::same`] compares when the roots
    /// differ: unbounded before the lane; in the publication lane the text
    /// bound, which the lane's text merge already reads per side.
    pub(super) compared: u64,
}

fn is_file(entry: Option<EntryFacts>) -> bool {
    entry.is_some_and(|entry| entry.value.kind == InodeKind::RegularFile)
}

impl Decider<'_> {
    /// Same entry: both absent, or same kind and, below a directory, same
    /// content. A regular file's content is its bytes, whatever its root
    /// (#352 F-073): two sides that wrote the same bytes did not both change it.
    fn same(
        &self,
        reader: &dyn AuthenticatedObjects,
        x: Option<EntryFacts>,
        y: Option<EntryFacts>,
    ) -> ContentResult<bool> {
        match (x, y) {
            (None, None) => Ok(true),
            (Some(a), Some(b)) if a.value.kind == InodeKind::Directory => {
                Ok(b.value.kind == InodeKind::Directory)
            }
            (Some(a), Some(b)) => same_content(
                reader,
                &a.value,
                &b.value,
                self.compared,
                self.timing.child("merge.compare"),
            ),
            _ => Ok(false),
        }
    }

    /// Text of a regular file within the text bound, else `None`.
    pub(super) fn text(
        &self,
        reader: &dyn AuthenticatedObjects,
        entry: Option<EntryFacts>,
    ) -> ContentResult<Option<String>> {
        let Some(entry) = entry.filter(|entry| entry.value.kind == InodeKind::RegularFile) else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        match crate::file::read_all_bounded(
            reader,
            entry.value.content_root,
            MAXIMUM_TEXT_BYTES,
            &mut bytes,
            self.timing.child("merge.read"),
        ) {
            Err(ContentError::BoundedCapacityExceeded { .. }) => return Ok(None),
            other => {
                other?;
            }
        }
        Ok(as_text(&bytes).map(str::to_owned))
    }

    /// Modification time of the path's entry on one side; a deletion takes
    /// the time of the nearest surviving parent directory.
    fn time(
        &self,
        resolver: &mut Resolver<'_>,
        side: usize,
        path: &str,
    ) -> ContentResult<(i64, u32)> {
        let mut path = path.to_owned();
        loop {
            if let Some(entry) = resolver.entry(side, &path)? {
                let metadata = read_portable(
                    resolver.reader,
                    entry.value.metadata_root,
                    entry.value.kind,
                    &mut AttributeReadWork::default(),
                )?;
                return Ok((metadata.mtime_seconds, metadata.mtime_nanoseconds));
            }
            path = split(&path).0.to_owned();
        }
    }

    /// The pick between the base, ours and theirs entries `[b, o, t]`
    /// reported at `path`: one serial on every side, or the entries one name
    /// binds. `write` spills a merged file's bytes and returns their number.
    pub(super) fn decide(
        &mut self,
        resolver: &mut Resolver<'_>,
        path: &str,
        [b, o, t]: [Option<EntryFacts>; 3],
        write: &mut dyn FnMut(Vec<u8>) -> ContentResult<u64>,
    ) -> ContentResult<Pick> {
        let ours = || o.map_or(Pick::Absent, Pick::Take);
        let reader = resolver.reader;
        if self.same(reader, o, t)? || self.same(reader, b, o)? {
            return Ok(Pick::Keep);
        }
        if self.same(reader, b, t)? {
            return Ok(ours());
        }
        if let Some(rule) = self.rules.matching(path) {
            let kept_ours =
                self.time(resolver, OURS, path)? >= self.time(resolver, THEIRS, path)?;
            self.lww.push(LwwDecision {
                path: path.to_owned(),
                kept_ours,
                deleted: if kept_ours { o.is_none() } else { t.is_none() },
                rule: rule.pattern.clone(),
            });
            return Ok(if kept_ours { ours() } else { Pick::Keep });
        }
        let conflict = |kind, hunks: Vec<Hunk>| Conflict {
            path: path.to_owned(),
            kind,
            hunks,
        };
        if let (true, true, Some(like)) = (is_file(o), is_file(t), o) {
            let base_text = match b {
                Some(_) => self.text(reader, b)?,
                None => Some(String::new()),
            };
            let (ot, tt) = (self.text(reader, o)?, self.text(reader, t)?);
            let (Some(base_text), Some(ot), Some(tt)) = (base_text, ot, tt) else {
                self.conflicts
                    .push(conflict(ConflictKind::Binary, Vec::new()));
                return Ok(Pick::Take(like));
            };
            let merged = merge3(&base_text, &ot, &tt, self.labels);
            if !merged.hunks.is_empty() {
                let kind = if b.is_some() {
                    ConflictKind::Content
                } else {
                    ConflictKind::AddAdd
                };
                self.conflicts.push(conflict(kind, merged.hunks));
            }
            return Ok(Pick::Write {
                like,
                text: write(merged.text.into_bytes())?,
            });
        }
        let present = match (o, t) {
            (Some(kept), None) if kept.value.kind != InodeKind::Directory => Some((kept, true)),
            (None, Some(kept)) if kept.value.kind != InodeKind::Directory => Some((kept, false)),
            _ => None,
        };
        if let Some((kept, kept_ours)) = present {
            let kind = if kept_ours {
                ConflictKind::ModifyDelete
            } else {
                ConflictKind::DeleteModify
            };
            let text = self.text(reader, Some(kept))?;
            self.conflicts.push(conflict(kind, Vec::new()));
            let index = self.conflicts.len() - 1;
            return Ok(match text {
                Some(text) => {
                    let marked = deletion_markers(&text, kept_ours, self.labels);
                    let lines = text.split('\n').map(str::to_owned).collect::<Vec<_>>();
                    self.conflicts[index].hunks.push(Hunk {
                        line: 1,
                        ours: if kept_ours { lines.clone() } else { Vec::new() },
                        theirs: if kept_ours { Vec::new() } else { lines },
                    });
                    Pick::Write {
                        like: kept,
                        text: write(marked.into_bytes())?,
                    }
                }
                None if kept_ours => Pick::Take(kept),
                None => Pick::Keep,
            });
        }
        self.conflicts
            .push(conflict(ConflictKind::Type, Vec::new()));
        Ok(o.map_or(Pick::Keep, Pick::Take))
    }
}

//! Metadata of a serial both sides changed, merged field by field (#352
//! F-052). A chmod on one side and a content edit on the other both change
//! the value's metadata (the edit moves the mtime), so a merge of the whole
//! metadata kept only the decided entry's and dropped the chmod. Mode and
//! mtime now merge three-way each: a field one side changed takes that
//! side's value, and a field both changed takes the decided entry's. Modes
//! changed differently on both sides of a decided file are a `mode`
//! conflict. Values are compared decoded, so #360's root and inline forms of
//! equal metadata are no change.

use super::super::serial::walk::Ctx;
use super::super::{Conflict, ConflictKind};
use crate::compare::tree::same_inode;
use crate::error::ContentResult;
use crate::filesystem::tree::attribute::{
    portable_root, read_portable, AttributeReadWork, PortableMetadata,
};
use crate::object::inode_leaf::{InodeKind, InodeMetadata, InodeValue};

/// The value one field takes: one side's change, else the decided entry's.
fn field<T: PartialEq>(base: T, from: T, into: T, decided: T) -> T {
    if from == base {
        into
    } else if into == base {
        from
    } else {
        decided
    }
}

impl Ctx<'_, '_> {
    /// The metadata a serial gets from base `b`, `from`'s `f` and `into`'s
    /// `i`, when the decision gave it `value`.
    pub(super) fn merged_metadata(
        &mut self,
        [b, f, i]: [InodeValue; 3],
        value: InodeValue,
        path: Option<&str>,
    ) -> ContentResult<InodeMetadata> {
        if self.same_metadata(f, b)? {
            return Ok(i.metadata_root);
        }
        if self.same_metadata(i, b)? {
            return Ok(f.metadata_root);
        }
        let reader = self.reader;
        let read = |value: InodeValue| {
            read_portable(
                reader,
                value.metadata_root,
                value.kind,
                &mut AttributeReadWork::default(),
            )
        };
        let (pb, pf, pi, pv) = (read(b)?, read(f)?, read(i)?, read(value)?);
        let time = |p: PortableMetadata| (p.mtime_seconds, p.mtime_nanoseconds);
        let (mtime_seconds, mtime_nanoseconds) = field(time(pb), time(pf), time(pi), time(pv));
        let merged = PortableMetadata {
            mode: field(pb.mode, pf.mode, pi.mode, pv.mode),
            mtime_seconds,
            mtime_nanoseconds,
        };
        let modes_differ = pf.mode != pb.mode && pi.mode != pb.mode && pf.mode != pi.mode;
        if let (true, Some(path)) = (modes_differ, path) {
            if value.kind != InodeKind::Directory {
                self.decider.conflicts.push(Conflict {
                    path: path.to_owned(),
                    kind: ConflictKind::Mode,
                    hunks: Vec::new(),
                });
            }
        }
        // A stored form that already holds the merged fields is kept.
        for (side, portable) in [(value, pv), (f, pf), (i, pi)] {
            if portable == merged {
                return Ok(side.metadata_root);
            }
        }
        Ok(match value.metadata_root {
            InodeMetadata::Inline(_) => InodeMetadata::Inline(merged),
            InodeMetadata::Root(root) => InodeMetadata::Root(portable_root(
                reader,
                self.objects,
                value.kind,
                merged,
                Some(root),
            )?),
        })
    }

    /// True when `a`'s metadata is `b`'s by value: #360's root and inline
    /// forms of equal metadata differ in bytes but are no change, as
    /// `inode_changes` already judges them (`same_inode`).
    pub(super) fn same_metadata(&self, a: InodeValue, b: InodeValue) -> ContentResult<bool> {
        if a.metadata_root == b.metadata_root {
            return Ok(true);
        }
        let a = InodeValue {
            metadata_root: a.metadata_root,
            ..b
        };
        same_inode(self.reader, &a, &b, &mut AttributeReadWork::default())
    }
}

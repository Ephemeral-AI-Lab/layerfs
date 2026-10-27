//! Where merged serials are bound (#355 M1): paths named on demand, and the
//! structural corrections the name points cannot see on their own.
//!
//! - A serial `from` deleted that the merge keeps (a changed file kept by the
//!   deletion rules, or a directory `into` added to) keeps its binding, and
//!   each parent `from` also deleted survives with only what it keeps.
//! - A serial `into` lacks that the merge keeps is bound where `from` holds
//!   it; each missing parent is revived from `from` with only what it keeps.
//! - A serial `from` bound that `into` deleted is resurrected with its whole
//!   subtree as `from` holds it, as a path-wise merge would.
//! - A directory `from` moved that `into` also moved elsewhere, or whose new
//!   parent the result places inside it, is a conflict; `from`'s move of it
//!   is cancelled.

use std::collections::BTreeSet;

use super::decide::{split, BASE};
use super::names::{Listing, Mode, PathSlot};
use super::serial::{built, is_directory, listing, Ctx};
use super::spill::{
    single, Spill, SpillStack, CANCEL_MOVE, CANCEL_REMOVE, DONE, DROP, EXTRA, LOST, MOVED, REVIVE,
    REVIVE_ALL, SURVIVE, TARGET,
};
use super::{Conflict, ConflictKind};
use crate::compare::tree::{join, list_directory, locate};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;
use crate::object::inode_leaf::InodeValue;
use crate::object::ObjectId;

/// Paths one merge keeps named before it starts again.
const NAMED_PATHS: usize = 4096;

/// Children whose values are read together.
const CHILDREN: usize = 256;

/// Every key of one record kind, a window at a time.
fn each_key(
    spill: &Spill<'_>,
    kind: u32,
    mut each: impl FnMut([u8; 32]) -> ContentResult<()>,
) -> ContentResult<()> {
    let mut after = None;
    loop {
        let keys = spill.keys_after(kind, after)?;
        let Some(last) = keys.last().copied() else {
            return Ok(());
        };
        for key in keys {
            each(key)?;
        }
        after = Some(last);
    }
}

fn serial_of(key: &[u8; 32]) -> u64 {
    super::spill::first(key)
}

impl Ctx<'_, '_> {
    /// The path of `serial` on one side: from recorded bindings, else the
    /// breadth-first walk.
    pub(super) fn path(&mut self, side: usize, serial: u64) -> ContentResult<String> {
        let tree = self.resolver.trees[side];
        if serial == tree.root_serial() {
            return Ok("/".to_owned());
        }
        if let Some(path) = self.paths.get(&(side, serial)) {
            return Ok(path.clone());
        }
        let wanted = BTreeSet::from([serial]);
        let named = match self.hints {
            Some(hints) => crate::compare::name_from_hints(
                self.reader,
                hints,
                &tree,
                &wanted,
                &mut self.naming,
            )?,
            None => None,
        };
        let named = match named {
            Some(named) => named,
            None => {
                self.hinted = false;
                locate(self.reader, &tree, &wanted, usize::MAX)?
            }
        };
        let path = named
            .get(&serial)
            .cloned()
            .ok_or(ContentError::InvalidRecord("merge serial without a path"))?;
        if self.paths.len() >= NAMED_PATHS {
            self.paths.clear();
        }
        self.paths.insert((side, serial), path.clone());
        Ok(path)
    }

    /// The serial `side` binds at `path`.
    fn at(&mut self, side: usize, path: &str) -> ContentResult<u64> {
        Ok(self
            .resolver
            .entry(side, path)?
            .ok_or(ContentError::InvalidRecord("merge parent without an entry"))?
            .serial)
    }

    fn conflict(&mut self, path: String) {
        self.decider.conflicts.push(Conflict {
            path,
            kind: ConflictKind::Type,
            hunks: Vec::new(),
        });
    }

    /// True when `from` binds another serial at `serial`'s base path.
    pub(super) fn rebound_by_from(&mut self, serial: u64) -> ContentResult<bool> {
        let path = self.path(BASE, serial)?;
        Ok(self
            .resolver
            .entry(self.from, &path)?
            .is_some_and(|entry| entry.serial != serial))
    }

    /// Keeps a deleted `from` serial's binding and every parent `from`
    /// deleted on its path in `into`.
    pub(super) fn survive(&mut self, serial: u64) -> ContentResult<()> {
        if self.spill.get(CANCEL_REMOVE, single(serial))?.is_none() {
            self.spill
                .insert(CANCEL_REMOVE, single(serial), Vec::new())?;
        }
        let mut path = self.path(self.into, serial)?;
        while path != "/" {
            let parent_path = split(&path).0.to_owned();
            let parent = self.at(self.into, &parent_path)?;
            if self.value(self.from, parent)?.is_some()
                || self.spill.get(SURVIVE, single(parent))?.is_some()
            {
                break;
            }
            self.spill.insert(SURVIVE, single(parent), Vec::new())?;
            self.spill
                .insert(CANCEL_REMOVE, single(parent), Vec::new())?;
            path = parent_path;
        }
        Ok(())
    }

    /// Revives a directory `into` lacks, bound where `from` holds it.
    pub(super) fn revive(&mut self, serial: u64) -> ContentResult<()> {
        if self.spill.get(REVIVE, single(serial))?.is_some() {
            return Ok(());
        }
        self.spill.insert(REVIVE, single(serial), Vec::new())?;
        let path = self.path(self.from, serial)?;
        self.place(serial, &path)
    }

    /// Binds a serial `into` lacks where `from` holds it, reviving each
    /// missing parent.
    pub(super) fn place(&mut self, serial: u64, from_path: &str) -> ContentResult<()> {
        let (mut child, mut path) = (serial, from_path.to_owned());
        while path != "/" {
            let (parent_path, name) = split(&path);
            let (parent_path, name) = (parent_path.to_owned(), name.to_owned());
            let parent = self.at(self.from, &parent_path)?;
            self.extra(parent, &name, Some(child))?;
            if self.value(self.into, parent)?.is_some()
                || self.spill.get(REVIVE, single(parent))?.is_some()
            {
                break;
            }
            self.spill.insert(REVIVE, single(parent), Vec::new())?;
            child = parent;
            path = parent_path;
        }
        Ok(())
    }

    /// Marks every serial below a resurrected directory that `into` lacks.
    pub(super) fn resurrect_below(&mut self, root: ObjectId) -> ContentResult<()> {
        let spill = self.spill;
        let mut stack = SpillStack::new(spill);
        stack.push(spill, root.as_bytes().to_vec())?;
        while let Some(bytes) = stack.pop(spill)? {
            let root = ObjectId::from_bytes(&bytes)?;
            let mut children = Vec::new();
            list_directory(self.reader, root, |_, child| {
                children.push(child);
                Ok(true)
            })?;
            for batch in children.chunks(CHILDREN) {
                let into = self.values(self.into, batch)?;
                let base = self.values(BASE, batch)?;
                let from = self.values(self.from, batch)?;
                for (((child, i), b), f) in batch.iter().zip(into).zip(base).zip(from) {
                    if i.is_some()
                        || b.is_none()
                        || spill.get(REVIVE_ALL, single(*child))?.is_some()
                    {
                        continue;
                    }
                    spill.insert(REVIVE_ALL, single(*child), Vec::new())?;
                    if let Some(listing) = listing(f) {
                        stack.push(spill, listing.as_bytes().to_vec())?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Leaves a serial new on `from` unbound where the merge lost a binding
    /// of it, with every new serial below it. A file `from` hard-linked at
    /// other names is left unbound only once every one of its links is lost:
    /// until then a name the merge keeps still binds it, with its value.
    pub(super) fn drop_new(&mut self, serial: u64) -> ContentResult<()> {
        let spill = self.spill;
        if spill.get(DROP, single(serial))?.is_some() {
            return Ok(());
        }
        let value = self.value(self.from, serial)?;
        if !self.unlinked(serial, value)? {
            return Ok(());
        }
        spill.insert(DROP, single(serial), Vec::new())?;
        match listing(value) {
            Some(root) => self.drop_below(root),
            None => Ok(()),
        }
    }

    /// Leaves every serial new on `from` below the listing `root` unbound,
    /// as [`Self::drop_new`] does.
    pub(super) fn drop_below(&mut self, root: ObjectId) -> ContentResult<()> {
        let spill = self.spill;
        let mut stack = SpillStack::new(spill);
        stack.push(spill, root.as_bytes().to_vec())?;
        while let Some(bytes) = stack.pop(spill)? {
            let mut children = Vec::new();
            list_directory(self.reader, ObjectId::from_bytes(&bytes)?, |_, child| {
                children.push(child);
                Ok(true)
            })?;
            for batch in children.chunks(CHILDREN) {
                let base = self.values(BASE, batch)?;
                let into = self.values(self.into, batch)?;
                let from = self.values(self.from, batch)?;
                for (((child, b), i), f) in batch.iter().zip(base).zip(into).zip(from) {
                    if b.is_some()
                        || i.is_some()
                        || spill.get(DROP, single(*child))?.is_some()
                        || !self.unlinked(*child, f)?
                    {
                        continue;
                    }
                    spill.insert(DROP, single(*child), Vec::new())?;
                    if let Some(listing) = listing(f) {
                        stack.push(spill, listing.as_bytes().to_vec())?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Counts one lost binding of a serial new on `from`; true when it was
    /// the last of the links `from` gave it. Only a regular file has more
    /// than one.
    fn unlinked(&self, serial: u64, from: Option<InodeValue>) -> ContentResult<bool> {
        let links = from.map_or(1, |value| value.namespace_ref_count);
        if links <= 1 {
            return Ok(true);
        }
        let key = single(serial);
        let old = self.spill.get(LOST, key)?;
        let lost = match &old {
            Some(bytes) => u64::from_be_bytes(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| ContentError::InvalidRecord("merge lost links"))?,
            ),
            None => 0,
        } + 1;
        if lost >= links {
            return Ok(true);
        }
        let count = lost.to_be_bytes().to_vec();
        match old {
            None => self.spill.insert(LOST, key, count)?,
            Some(old) => self.spill.replace(LOST, key, old, count)?,
        }
        Ok(false)
    }

    /// Checks each directory `from` moved within what `into` holds.
    pub(super) fn moved(&mut self) -> ContentResult<()> {
        let mut moved = Vec::new();
        each_key(self.spill, MOVED, |key| {
            moved.push(serial_of(&key));
            Ok(())
        })?;
        for serial in moved {
            let (parent, name) = self.moved_to(serial)?.expect("listed");
            if self.value(self.into, serial)?.is_none() {
                continue;
            }
            let parent_path = self.path_anywhere(parent)?;
            let target = join(&parent_path, &PathName::new(&name)?);
            let base_path = self.path(BASE, serial)?;
            let into_path = self.path(self.into, serial)?;
            let cancelled = if into_path != base_path {
                // Both sides moved it, to different places.
                into_path != target
            } else {
                self.inside(serial, parent)?
            };
            if cancelled {
                self.spill.insert(CANCEL_MOVE, single(serial), Vec::new())?;
                self.conflict(target);
            }
        }
        Ok(())
    }

    fn moved_to(&self, serial: u64) -> ContentResult<Option<(u64, String)>> {
        let Some(bytes) = self.spill.get(MOVED, single(serial))? else {
            return Ok(None);
        };
        let (parent, name) = bytes
            .split_at_checked(8)
            .ok_or(ContentError::InvalidRecord("merge moved record"))?;
        Ok(Some((
            u64::from_be_bytes(parent.try_into().expect("eight bytes")),
            String::from_utf8(name.to_vec())
                .map_err(|_| ContentError::InvalidRecord("merge moved name"))?,
        )))
    }

    /// The path of a serial on `from` if it holds it, else on `into`.
    fn path_anywhere(&mut self, serial: u64) -> ContentResult<String> {
        if self.value(self.from, serial)?.is_some() {
            self.path(self.from, serial)
        } else {
            self.path(self.into, serial)
        }
    }

    /// True when the result would place `parent` inside `directory`.
    fn inside(&mut self, directory: u64, parent: u64) -> ContentResult<bool> {
        let root = self.resolver.trees[self.into].root_serial();
        let mut at = parent;
        let mut seen = BTreeSet::new();
        while at != root {
            if at == directory || !seen.insert(at) {
                return Ok(true);
            }
            at = match self.moved_to(at)? {
                Some((parent, _)) if self.spill.get(CANCEL_MOVE, single(at))?.is_none() => parent,
                _ => {
                    let side = if self.value(self.into, at)?.is_some() {
                        self.into
                    } else {
                        self.from
                    };
                    let path = self.path(side, at)?;
                    self.at(side, split(&path).0)?
                }
            };
        }
        Ok(false)
    }

    /// Decides the names of every directory that survives a deletion.
    pub(super) fn survivors(&mut self) -> ContentResult<()> {
        let mut survivors = Vec::new();
        each_key(self.spill, SURVIVE, |key| {
            survivors.push(serial_of(&key));
            Ok(())
        })?;
        for serial in survivors {
            let mut listing = Listing {
                target: serial,
                base: listing(self.value(BASE, serial)?),
                from: None,
                into: listing(self.value(self.into, serial)?),
                path: PathSlot::Name(self.into, serial),
            };
            self.names(&mut listing, Mode::Decide)?;
        }
        Ok(())
    }

    fn done(&self, serial: u64) -> ContentResult<bool> {
        Ok(self.spill.get(DONE, single(serial))?.is_some())
    }

    /// The header and value of a resurrected directory: every binding `from`
    /// holds in it, except a directory `into` holds elsewhere.
    pub(super) fn emit_resurrected(&mut self, serial: u64, from: InodeValue) -> ContentResult<()> {
        self.plan.fresh(serial)?;
        self.plan.value(serial, built(from)?)?;
        let mut writer = super::rows::ChangeWriter::new(serial);
        let mut children = Vec::new();
        list_directory(self.reader, from.content_root, |name, child| {
            children.push((name.clone(), child));
            Ok(true)
        })?;
        for batch in children.chunks(CHILDREN) {
            let serials: Vec<u64> = batch.iter().map(|(_, child)| *child).collect();
            let into = self.values(self.into, &serials)?;
            let from = self.values(self.from, &serials)?;
            for (((name, child), i), f) in batch.iter().zip(into).zip(from) {
                if (i.is_some() && is_directory(f))
                    || self
                        .spill
                        .get(super::spill::DROP, single(*child))?
                        .is_some()
                {
                    continue;
                }
                writer.push(self.spill, name, Some(*child))?;
            }
        }
        writer.finish(self.spill, &mut self.plan.counts)?;
        self.spill.insert(DONE, single(serial), Vec::new())
    }

    /// Absorbed directories' targets, resurrected and revived serials, and
    /// directories with only added bindings.
    pub(super) fn rest(&mut self) -> ContentResult<()> {
        let mut targets = Vec::new();
        each_key(self.spill, TARGET, |key| {
            targets.push(serial_of(&key));
            Ok(())
        })?;
        for target in targets {
            let bytes = self
                .spill
                .get(TARGET, single(target))?
                .ok_or(ContentError::InvalidRecord("merge target"))?;
            let number = |at: usize| {
                bytes
                    .get(at..at + 8)
                    .map(|slice| u64::from_be_bytes(slice.try_into().expect("eight bytes")))
            };
            let absorbed = number(0).ok_or(ContentError::InvalidRecord("merge target"))?;
            let base = match number(8) {
                Some(serial) => listing(self.value(BASE, serial)?),
                None => None,
            };
            let mut listing = Listing {
                target,
                base,
                from: listing(self.value(self.from, absorbed)?),
                into: listing(self.value(self.into, target)?),
                path: PathSlot::Name(self.into, target),
            };
            self.names(&mut listing, Mode::Emit)?;
        }
        let mut resurrected = Vec::new();
        each_key(self.spill, REVIVE_ALL, |key| {
            resurrected.push(serial_of(&key));
            Ok(())
        })?;
        for serial in resurrected {
            let from = self
                .value(self.from, serial)?
                .ok_or(ContentError::InvalidRecord("merge resurrected serial"))?;
            if is_directory(Some(from)) {
                if !self.done(serial)? {
                    self.emit_resurrected(serial, from)?;
                }
            } else if self
                .spill
                .get(super::spill::DROP, single(serial))?
                .is_none()
            {
                self.plan.fresh(serial)?;
                if !self.plan.valued(serial)? {
                    self.plan.value(serial, from)?;
                }
            }
        }
        let mut revived = Vec::new();
        each_key(self.spill, REVIVE, |key| {
            revived.push(serial_of(&key));
            Ok(())
        })?;
        for serial in revived {
            if self.done(serial)? {
                continue;
            }
            let from = self
                .value(self.from, serial)?
                .ok_or(ContentError::InvalidRecord("merge revived serial"))?;
            self.plan.fresh(serial)?;
            self.plan.value(serial, built(from)?)?;
            let mut listing = Listing {
                target: serial,
                base: None,
                from: None,
                into: None,
                path: PathSlot::Name(self.from, serial),
            };
            self.names(&mut listing, Mode::Emit)?;
        }
        let mut parents = Vec::new();
        each_key(self.spill, EXTRA, |key| {
            parents.push(serial_of(&key));
            Ok(())
        })?;
        for parent in parents {
            if self.done(parent)? {
                continue;
            }
            let mut listing = Listing {
                target: parent,
                base: None,
                from: None,
                into: listing(self.value(self.into, parent)?),
                path: PathSlot::Name(self.into, parent),
            };
            self.names(&mut listing, Mode::Emit)?;
        }
        Ok(())
    }
}

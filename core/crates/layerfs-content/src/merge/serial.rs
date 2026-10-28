//! Merge by serial (#355 M1).
//!
//! Only the `from` side's changes are walked: the page-skipping difference
//! of the base and `from` inode tables. Each changed serial's value on the
//! `into` side is read by serial. Unchanged there, the serial takes `from`'s
//! value; equal to `from`'s, nothing changes; otherwise both sides changed it
//! and it is decided: a file or symlink by the existing rules on the three
//! values, a directory by name. A directory's changed names (its *name
//! points*) bind what `from` bound unless `into` rebound the same name, which
//! is decided by the entries the name binds; a file `from` deleted and `into`
//! changed is decided like a deletion at its path. So a move follows its
//! serial: an edit inside a directory one side moved lands at the new path.
//!
//! The work follows `from`'s changes, never the tree; paths are named only
//! where a rule needs one (a decision's path, a deleted directory that keeps
//! a changed descendant, a revived parent, a moved directory's checks). Two
//! passes run over the changes: the first decides and writes its decisions
//! and corrections as records, the second writes the plan's directory rows
//! with them applied. [`super::places`] holds the structural corrections and
//! [`super::names`] the name points.

use layerfs_telemetry::timer::{Active, TimingScope};

use super::build::{Plan, Writer};
use super::codec::{put_entry, Cursor};
use super::decide::{Decider, Pick, Resolver, BASE, OURS, THEIRS};
use super::names::{Listing, Mode, PathSlot};
use super::spill::{single, Run, RunReader, RunWriter, Spill, DROP, REVIVE, REVIVE_ALL, SURVIVE};
use crate::compare::tree::{inode_changes, same_value, EntryFacts};
use crate::compare::{BindingHints, DifferenceWork, NamingWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::FilesystemObjects;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId};

/// Changed serials whose other values are read together.
const WINDOW: usize = 256;

/// One changed serial: its base, `from` and `into` values.
#[derive(Clone, Copy)]
pub(super) struct Change {
    pub(super) serial: u64,
    pub(super) b: Option<InodeValue>,
    pub(super) f: Option<InodeValue>,
    pub(super) i: Option<InodeValue>,
}

/// Both absent, or equal apart from the derived reference count.
pub(super) fn same(x: Option<InodeValue>, y: Option<InodeValue>) -> bool {
    match (x, y) {
        (None, None) => true,
        (Some(a), Some(b)) => same_value(&a, &b),
        _ => false,
    }
}

pub(super) fn is_directory(value: Option<InodeValue>) -> bool {
    value.is_some_and(|value| value.kind == InodeKind::Directory)
}

/// The listing of a directory value; `None` for anything else.
pub(super) fn listing(value: Option<InodeValue>) -> Option<ObjectId> {
    value
        .filter(|value| value.kind == InodeKind::Directory)
        .map(|value| value.content_root)
}

/// A new directory's value: the update builds its page from its bindings.
pub(super) fn built(value: InodeValue) -> ContentResult<InodeValue> {
    Ok(InodeValue {
        content_root: ObjectId::from_bytes(&[0; 32])?,
        namespace_ref_count: 1,
        ..value
    })
}

/// One merge's state.
pub(super) struct Ctx<'a, 'r> {
    pub(super) reader: &'a dyn AuthenticatedObjects,
    pub(super) resolver: Resolver<'a>,
    pub(super) from: usize,
    pub(super) into: usize,
    pub(super) spill: &'a Spill<'r>,
    pub(super) decider: Decider<'a>,
    pub(super) hints: Option<&'a dyn BindingHints>,
    pub(super) naming: NamingWork,
    /// True while every path was named from recorded bindings.
    pub(super) hinted: bool,
    pub(super) work: DifferenceWork,
    pub(super) plan: Plan<'a, 'r>,
    pub(super) writer: Writer<'a>,
    pub(super) objects: &'a mut FilesystemObjects<'a>,
    pub(super) paths: std::collections::HashMap<(usize, u64), String>,
    /// Changed serials decided, and the caller's bound on them.
    pub(super) decided: usize,
    pub(super) bound: Option<usize>,
}

/// Emits constructed file objects through the operation's own boundary.
struct Emit<'x, 'y>(&'x mut FilesystemObjects<'y>);

impl FinalizedConsumer for Emit<'_, '_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.emit(object).map(|_| ())
    }
}

impl Ctx<'_, '_> {
    /// The value `side` holds at `serial`.
    pub(super) fn value(&self, side: usize, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.resolver.value(side, serial)
    }

    /// Values of many serials on one side, in the order asked.
    pub(super) fn values(
        &self,
        side: usize,
        serials: &[u64],
    ) -> ContentResult<Vec<Option<InodeValue>>> {
        self.resolver.trees[side].values(self.reader, serials)
    }

    /// The decider's entries, base then ours then theirs, from entries given
    /// by side: `[base, from, into]`.
    pub(super) fn by_side(&self, [b, f, i]: [Option<EntryFacts>; 3]) -> [Option<EntryFacts>; 3] {
        if self.from == OURS {
            [b, f, i]
        } else {
            [b, i, f]
        }
    }

    /// Decides between three entries given as `[base, from, into]`; returns
    /// the result entry. A written file is constructed now and carries the
    /// written kind's serial `like` unless `serial` names the carrier.
    pub(super) fn decide(
        &mut self,
        path: &str,
        entries: [Option<EntryFacts>; 3],
    ) -> ContentResult<Option<Result<EntryFacts, (EntryFacts, InodeValue)>>> {
        let ordered = self.by_side(entries);
        let mut written = None;
        let pick = self
            .decider
            .decide(&mut self.resolver, path, ordered, &mut |bytes| {
                written = Some(bytes);
                Ok(0)
            })?;
        Ok(match pick {
            Pick::Keep => ordered[THEIRS].map(Ok),
            Pick::Absent => None,
            Pick::Take(entry) => Some(Ok(entry)),
            Pick::Write { like, .. } => {
                let bytes = written.ok_or(ContentError::InvalidRecord("merge written bytes"))?;
                let file = crate::file::construct_bytes(
                    self.writer.policy,
                    self.writer.capacities,
                    &bytes,
                    &mut Emit(self.objects),
                    self.writer.timing.child("merge.write"),
                )?;
                Some(Err((
                    like,
                    InodeValue {
                        kind: InodeKind::RegularFile,
                        content_root: file.root,
                        ..like.value
                    },
                )))
            }
        })
    }

    /// Counts one decided serial against the caller's bound.
    fn count(&mut self) -> ContentResult<()> {
        self.decided += 1;
        match self.bound {
            Some(limit) if self.decided > limit => Err(ContentError::BoundedCapacityExceeded {
                what: "merge.bound",
                limit: limit as u64,
                actual: self.decided as u64,
            }),
            _ => Ok(()),
        }
    }

    /// Both passes, then the plan is complete.
    pub(super) fn run(&mut self, timing: &TimingScope<'_, Active>) -> ContentResult<()> {
        let deferred = timing
            .child("merge.decide")
            .run(|_| self.changes(Mode::Decide))?
            .expect("a decide pass returns its deferred run");
        timing.child("merge.places").run(|_| {
            self.moved()?;
            self.deferred(deferred)?;
            self.survivors()
        })?;
        timing.child("merge.plan").run(|_| {
            self.changes(Mode::Emit)?;
            self.rest()
        })
    }

    /// One pass over `from`'s changed serials. Deciding, it returns the
    /// serials `into` lacks for [`Self::deferred`].
    fn changes(&mut self, mode: Mode) -> ContentResult<Option<Run>> {
        let reader = self.reader;
        let (base, side) = (self.resolver.trees[BASE], self.resolver.trees[self.from]);
        let mut window: Vec<(u64, Option<InodeValue>, Option<InodeValue>)> = Vec::new();
        let mut deferred = RunWriter::new(self.spill);
        let mut work = DifferenceWork::default();
        let mut failed = None;
        inode_changes(reader, &base, &side, &mut work, |serial, b, f| {
            window.push((serial, b.copied(), f.copied()));
            if window.len() == WINDOW {
                if let Err(error) = self.window(std::mem::take(&mut window), mode, &mut deferred) {
                    failed = Some(error);
                    return Err(ContentError::InvalidRecord("merge window"));
                }
            }
            Ok(())
        })
        .map_err(|error| failed.take().unwrap_or(error))?;
        self.window(window, mode, &mut deferred)?;
        if mode == Mode::Decide {
            self.work.add(work);
            Ok(Some(deferred.finish(self.spill)?))
        } else {
            Ok(None)
        }
    }

    fn window(
        &mut self,
        window: Vec<(u64, Option<InodeValue>, Option<InodeValue>)>,
        mode: Mode,
        deferred: &mut RunWriter,
    ) -> ContentResult<()> {
        if window.is_empty() {
            return Ok(());
        }
        let serials: Vec<u64> = window.iter().map(|(serial, _, _)| *serial).collect();
        let into = self.values(self.into, &serials)?;
        for ((serial, b, f), i) in window.into_iter().zip(into) {
            let change = Change { serial, b, f, i };
            if same(i, f) {
                continue;
            }
            match mode {
                Mode::Decide => self.decide_change(change, deferred)?,
                Mode::Emit => self.emit_change(change)?,
                Mode::Count => unreachable!("a count is per directory"),
            }
        }
        Ok(())
    }

    /// Deciding pass, one changed serial.
    fn decide_change(&mut self, c: Change, deferred: &mut RunWriter) -> ContentResult<()> {
        self.count()?;
        let one_sided = same(c.i, c.b);
        match (c.b, c.f, c.i) {
            (_, Some(f), _) if f.kind == InodeKind::Directory => {
                if let (Some(b), None) = (c.b, c.i) {
                    if let Some((path, there)) = self.replacement(c.serial, b, f)? {
                        let facts = |value| EntryFacts {
                            serial: c.serial,
                            value,
                        };
                        if self.absorb_replaced(&path, facts(b), facts(f), there)? {
                            return Ok(());
                        }
                    }
                }
                let mut listing = self.listing_of(c, PathSlot::Name(self.from, c.serial));
                self.names(&mut listing, Mode::Decide)?;
                if c.i.is_none() && c.b.is_some() {
                    self.defer(deferred, c)?;
                }
            }
            (_, Some(f), Some(i)) if !one_sided => {
                let path = self.path(self.from, c.serial)?;
                let entry = |value| {
                    Some(EntryFacts {
                        serial: c.serial,
                        value,
                    })
                };
                let result = self.decide(&path, [c.b.and_then(entry), entry(f), entry(i)])?;
                if let Some(value) = self.result_value(c, result, Some(&path))? {
                    if !same_value(&value, &i) {
                        self.plan.value(c.serial, value)?;
                    }
                }
            }
            (Some(_), Some(_), None) => self.defer(deferred, c)?,
            (Some(b), None, Some(i)) if !one_sided => self.deleted_and_changed(c.serial, b, i)?,
            _ => {}
        }
        Ok(())
    }

    fn defer(&self, deferred: &mut RunWriter, c: Change) -> ContentResult<()> {
        let (Some(b), Some(f)) = (c.b, c.f) else {
            return Err(ContentError::InvalidRecord("merge deferred change"));
        };
        let mut bytes = Vec::new();
        put_entry(
            &mut bytes,
            &EntryFacts {
                serial: c.serial,
                value: b,
            },
        );
        put_entry(
            &mut bytes,
            &EntryFacts {
                serial: c.serial,
                value: f,
            },
        );
        deferred.push(self.spill, &bytes)
    }

    /// The value a decision gives one serial present on both sides: the
    /// decided entry's, with its metadata merged field by field
    /// ([`Self::merged_metadata`]). `path` names a decided file, whose
    /// conflicting modes are reported there.
    pub(super) fn result_value(
        &mut self,
        c: Change,
        result: Option<Result<EntryFacts, (EntryFacts, InodeValue)>>,
        path: Option<&str>,
    ) -> ContentResult<Option<InodeValue>> {
        let Some(result) = result else {
            return Ok(None);
        };
        let mut value = match result {
            Ok(entry) => entry.value,
            Err((_, value)) => value,
        };
        if let (Some(b), Some(f), Some(i)) = (c.b, c.f, c.i) {
            if f.kind == i.kind && value.kind == i.kind {
                value.metadata_root = self.merged_metadata([b, f, i], value, path)?;
            }
        }
        Ok(Some(value))
    }

    /// The three listings of one changed directory, built on its own serial.
    fn listing_of(&self, c: Change, path: PathSlot) -> Listing {
        Listing {
            target: c.serial,
            base: listing(c.b),
            from: listing(c.f),
            into: listing(c.i),
            path,
        }
    }

    /// Writing pass, one changed serial.
    fn emit_change(&mut self, c: Change) -> ContentResult<()> {
        let s = c.serial;
        if self.absorbed(s)? {
            return Ok(());
        }
        let one_sided = same(c.i, c.b);
        match (c.b, c.f, c.i) {
            (_, Some(f), i) if f.kind == InodeKind::Directory => {
                if self.spill.get(REVIVE_ALL, single(s))?.is_some() {
                    return self.emit_resurrected(s, f);
                }
                let new = i.is_none();
                if new && c.b.is_some() && self.spill.get(REVIVE, single(s))?.is_none() {
                    return Ok(());
                }
                if new {
                    self.plan.fresh(s)?;
                    self.plan.value(s, built(f)?)?;
                } else if let Some(i) = i {
                    // Two directories are one entry: when both sides changed
                    // its metadata differently, theirs is kept.
                    let theirs = if self.into == THEIRS { i } else { f };
                    let metadata = if one_sided {
                        f.metadata_root
                    } else {
                        self.result_value(
                            c,
                            Some(Ok(EntryFacts {
                                serial: s,
                                value: theirs,
                            })),
                            None,
                        )?
                        .map_or(i.metadata_root, |value| value.metadata_root)
                    };
                    if metadata != i.metadata_root {
                        self.plan.value(
                            s,
                            InodeValue {
                                metadata_root: metadata,
                                ..i
                            },
                        )?;
                    }
                }
                let mut listing = self.listing_of(c, PathSlot::Name(self.from, s));
                self.names(&mut listing, Mode::Emit)?;
            }
            // A new file keeps `from`'s value, unless a decision at a name
            // gave this serial its result.
            (None, Some(f), None) if !self.plan.valued(s)? => {
                self.plan.fresh(s)?;
                self.plan.value(s, f)?;
            }
            (_, Some(f), Some(_)) if one_sided => self.plan.value(s, f)?,
            (Some(b), None, Some(_)) if is_directory(Some(b)) => {
                if self.spill.get(SURVIVE, single(s))?.is_some() {
                    let mut listing = self.listing_of(c, PathSlot::Name(self.into, s));
                    self.names(&mut listing, Mode::Emit)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The serials `into` lacks that `from` changed: decided once every
    /// moved directory is known, so a resurrected subtree takes `from`'s.
    fn deferred(&mut self, run: Run) -> ContentResult<()> {
        let mut reader = RunReader::new(run);
        while let Some(bytes) = reader.next(self.spill)? {
            let mut cursor = Cursor(&bytes);
            let (b, f) = (cursor.entry()?, cursor.entry()?);
            cursor.end()?;
            self.deferred_one(b, f)?;
        }
        Ok(())
    }

    fn deferred_one(&mut self, b: EntryFacts, f: EntryFacts) -> ContentResult<()> {
        let s = f.serial;
        if self.spill.get(REVIVE_ALL, single(s))?.is_some() {
            if f.value.kind != InodeKind::Directory {
                // A file `from` edited in a subtree `into` deleted: decided
                // as a deletion against an edit, as it is the other way.
                let path = self.path(self.from, s)?;
                match self.decide(&path, [Some(b), Some(f), None])? {
                    Some(Ok(kept)) => self.plan.value(s, kept.value)?,
                    Some(Err((_, value))) => self.plan.value(s, value)?,
                    None => self.spill.insert(DROP, single(s), Vec::new())?,
                }
            }
            return Ok(());
        }
        if f.value.kind == InodeKind::Directory {
            if let Some((path, there)) = self.replacement(s, b.value, f.value)? {
                return self.replaced_directory(&path, b, f, there);
            }
            let c = Change {
                serial: s,
                b: Some(b.value),
                f: Some(f.value),
                i: None,
            };
            let mut listing = self.listing_of(c, PathSlot::Name(self.from, s));
            if self.names(&mut listing, Mode::Count)? > 0 {
                self.revive(s)?;
            }
            return Ok(());
        }
        let from_path = self.path(self.from, s)?;
        if let Some(there) = self.rebound_at(s, &from_path)? {
            return self.replaced(&from_path, b, f, there);
        }
        let result = self.decide(&from_path, [Some(b), Some(f), None])?;
        let c = Change {
            serial: s,
            b: Some(b.value),
            f: Some(f.value),
            i: None,
        };
        if let Some(value) = self.result_value(c, result, None)? {
            self.plan.fresh(s)?;
            self.plan.value(s, value)?;
            self.place(s, &from_path)?;
        }
        Ok(())
    }

    /// The inode `into` binds at `path`, `from`'s path of a serial `into`
    /// lacks, when that is the serial's base path: another inode `into`
    /// put there, decided at the path.
    fn rebound_at(&mut self, serial: u64, path: &str) -> ContentResult<Option<EntryFacts>> {
        if path != self.path(BASE, serial)? {
            return Ok(None);
        }
        self.resolver.entry(self.into, path)
    }

    /// A directory `from` changed the names of and `into` lacks: its path
    /// and the inode `into` put there instead, if any.
    fn replacement(
        &mut self,
        serial: u64,
        b: InodeValue,
        f: InodeValue,
    ) -> ContentResult<Option<(String, EntryFacts)>> {
        if f.content_root == b.content_root {
            return Ok(None);
        }
        let path = self.path(self.from, serial)?;
        Ok(self.rebound_at(serial, &path)?.map(|there| (path, there)))
    }

    /// A deferred file whose path `into` rebound to `there`.
    fn replaced(
        &mut self,
        path: &str,
        b: EntryFacts,
        f: EntryFacts,
        there: EntryFacts,
    ) -> ContentResult<()> {
        let result = self.decide(path, [Some(b), Some(f), Some(there)])?;
        let carrier = self.carrier(result, Some(there))?;
        if carrier != Some(there.serial) {
            let (parent, name) = super::decide::split(path);
            let parent = self
                .resolver
                .entry(self.into, parent)?
                .ok_or(ContentError::InvalidRecord("merge replaced parent"))?;
            self.extra(parent.serial, name, carrier)?;
        }
        Ok(())
    }

    /// The serial that carries a decided entry at a name, with its value
    /// written when it is new or changed.
    pub(super) fn carrier(
        &mut self,
        result: Option<Result<EntryFacts, (EntryFacts, InodeValue)>>,
        here: Option<EntryFacts>,
    ) -> ContentResult<Option<u64>> {
        Ok(match result {
            None => None,
            Some(Ok(entry)) => Some(entry.serial),
            Some(Err((like, value))) => Some(self.plan.assign(
                &self.resolver,
                self.into,
                &mut self.writer,
                like,
                value,
                here,
            )?),
        })
    }

    /// A deleted `from` serial `into` changed: kept if the rules keep it.
    fn deleted_and_changed(&mut self, s: u64, b: InodeValue, i: InodeValue) -> ContentResult<()> {
        if self.rebound_by_from(s)? {
            // `from` bound something else at its name: decided there.
            return Ok(());
        }
        if b.kind == InodeKind::Directory {
            if self.keeps_names(b, i)? {
                self.spill.insert(SURVIVE, single(s), Vec::new())?;
                self.survive(s)?;
            }
            return Ok(());
        }
        let path = self.path(self.into, s)?;
        let entry = |value| Some(EntryFacts { serial: s, value });
        let result = self.decide(&path, [entry(b), None, entry(i)])?;
        let c = Change {
            serial: s,
            b: Some(b),
            f: None,
            i: Some(i),
        };
        if let Some(value) = self.result_value(c, result, None)? {
            if !same_value(&value, &i) {
                self.plan.value(s, value)?;
            }
            self.survive(s)?;
        }
        Ok(())
    }

    /// True when the directory `into` changed binds a name the base did not.
    fn keeps_names(&mut self, b: InodeValue, i: InodeValue) -> ContentResult<bool> {
        let mut kept = false;
        crate::compare::tree::binding_changes(
            self.reader,
            Some(b.content_root),
            Some(i.content_root),
            &mut self.work,
            |_, _, now| {
                kept |= now.is_some();
                Ok(())
            },
        )?;
        Ok(kept)
    }
}

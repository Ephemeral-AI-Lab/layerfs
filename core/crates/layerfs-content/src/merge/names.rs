//! Name points (#355 M1): the names a directory's listing changed on the
//! `from` side, merged into the `into` side's listing of the same directory.
//!
//! At each name, `b`, `f` and `i` are what the base, `from` and `into`
//! listings bind there. `into` binding what `from` binds changes nothing.
//! `into` binding what the base bound takes `from`'s binding: a name `from`
//! added, a serial `from` moved away, a serial `from` deleted (kept only when
//! a correction says it survives), or a replacement. A replacement of a file
//! `into` changed in place or of a directory whose names `into` changed, and
//! every name both sides rebound, is decided by
//! the entries the name binds; two new directories at one name are merged by
//! name into `into`'s (`from`'s is *absorbed*). The deciding pass records
//! each decision under its name; the writing pass reads them back.

use std::collections::HashMap;

use super::decide::split;
use super::rows::ChangeWriter;
use super::serial::{is_directory, listing, same, Ctx};
use super::spill::{
    single, ABSORB, CANCEL_MOVE, CANCEL_REMOVE, DONE, DROP, EXTRA, MOVED, NAMED, REVIVE_ALL, TARGET,
};
use crate::compare::tree::{binding_changes, join, EntryFacts};
use crate::compare::DifferenceWork;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::lookup;
use crate::filesystem::path::PathName;
use crate::filesystem::sorted::DirectoryRoot;
use crate::object::inode_leaf::InodeValue;
use crate::object::ObjectId;

/// Names read together.
const BATCH: usize = 256;

/// What a pass over a listing does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Mode {
    /// Decides and records.
    Decide,
    /// Writes the directory's header.
    Emit,
    /// Counts the names the header would hold.
    Count,
}

/// A directory's path, named only when a rule needs it.
pub(super) enum PathSlot {
    Known(String),
    Name(usize, u64),
}

/// One directory's three listings; the header is written for `target`.
pub(super) struct Listing {
    pub(super) target: u64,
    pub(super) base: Option<ObjectId>,
    pub(super) from: Option<ObjectId>,
    pub(super) into: Option<ObjectId>,
    pub(super) path: PathSlot,
}

/// Values of the serials one batch of names binds, by side.
#[derive(Default)]
struct Values {
    b: HashMap<u64, Option<InodeValue>>,
    f: HashMap<u64, Option<InodeValue>>,
    i: HashMap<u64, Option<InodeValue>>,
}

impl Values {
    fn get(map: &HashMap<u64, Option<InodeValue>>, serial: Option<u64>) -> Option<InodeValue> {
        serial.and_then(|serial| map.get(&serial).copied().flatten())
    }

    fn entry(map: &HashMap<u64, Option<InodeValue>>, serial: Option<u64>) -> Option<EntryFacts> {
        let serial = serial?;
        Self::get(map, Some(serial)).map(|value| EntryFacts { serial, value })
    }
}

/// One changed name: what the base, `from` and `into` bind there.
type Name = (PathName, Option<u64>, Option<u64>, Option<u64>);

/// The key of one name's decision.
fn named_key(target: u64, name: &PathName) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&target.to_be_bytes());
    hasher.update(name.as_bytes());
    *hasher.finalize().as_bytes()
}

fn put_binding(binding: Option<u64>) -> Vec<u8> {
    match binding {
        None => vec![0],
        Some(serial) => {
            let mut out = vec![1];
            out.extend_from_slice(&serial.to_be_bytes());
            out
        }
    }
}

fn read_binding(bytes: &[u8]) -> ContentResult<Option<u64>> {
    match bytes {
        [0] => Ok(None),
        [1, rest @ ..] if rest.len() == 8 => Ok(Some(u64::from_be_bytes(
            rest.try_into().expect("eight bytes"),
        ))),
        _ => Err(ContentError::InvalidRecord("merge binding record")),
    }
}

/// Bindings added to a directory outside its name points, by name.
pub(super) fn read_extras(bytes: &[u8]) -> ContentResult<Vec<(PathName, Option<u64>)>> {
    let mut cursor = super::codec::Cursor(bytes);
    let mut out = Vec::new();
    while !cursor.0.is_empty() {
        let name = PathName::new(&cursor.text()?)?;
        let flag = cursor.byte()?;
        let binding = match flag {
            0 => None,
            1 => Some(cursor.number()?),
            _ => return Err(ContentError::InvalidRecord("merge extra binding")),
        };
        out.push((name, binding));
    }
    Ok(out)
}

impl Ctx<'_, '_> {
    /// One pass over a listing's changed names; returns how many names the
    /// header holds (or would hold).
    pub(super) fn names(&mut self, listing: &mut Listing, mode: Mode) -> ContentResult<usize> {
        let mut out = (mode == Mode::Emit).then(|| ChangeWriter::new(listing.target));
        let mut extras = if mode == Mode::Decide {
            Vec::new()
        } else {
            self.extras(listing.target)?
        }
        .into_iter()
        .peekable();
        let mut count = 0usize;
        let mut batch: Vec<(PathName, Option<u64>, Option<u64>)> = Vec::new();
        let mut work = DifferenceWork::default();
        let reader = self.reader;
        if listing.base.is_some() || listing.from.is_some() {
            binding_changes(
                reader,
                listing.base,
                listing.from,
                &mut work,
                |name, was, now| {
                    batch.push((name.clone(), was, now));
                    if batch.len() == BATCH {
                        let names = self.read_names(listing, std::mem::take(&mut batch))?;
                        self.name_batch(listing, mode, names, &mut out, &mut extras, &mut count)?;
                    }
                    Ok(())
                },
            )?;
        }
        let names = self.read_names(listing, batch)?;
        self.name_batch(listing, mode, names, &mut out, &mut extras, &mut count)?;
        for (name, binding) in extras {
            count += 1;
            if let Some(out) = &mut out {
                out.push(self.spill, &name, binding)?;
            }
        }
        self.work.add(work);
        if let Some(out) = out {
            if count > 0 || listing.into.is_none() {
                out.finish(self.spill, &mut self.plan.counts)?;
                self.spill
                    .insert(DONE, single(listing.target), Vec::new())?;
            }
        }
        Ok(count)
    }

    /// Adds what `into` binds at each name.
    fn read_names(
        &self,
        listing: &Listing,
        batch: Vec<(PathName, Option<u64>, Option<u64>)>,
    ) -> ContentResult<Vec<Name>> {
        batch
            .into_iter()
            .map(|(name, b, f)| {
                let i = match listing.into {
                    Some(root) => lookup(self.reader, DirectoryRoot(root), &name)?,
                    None => None,
                };
                Ok((name, b, f, i))
            })
            .collect()
    }

    fn name_values(&self, names: &[Name]) -> ContentResult<Values> {
        let mut serials: Vec<u64> = names
            .iter()
            .flat_map(|(_, b, f, i)| [*b, *f, *i])
            .flatten()
            .collect();
        serials.sort_unstable();
        serials.dedup();
        let mut values = Values::default();
        for (side, map) in [
            (super::decide::BASE, &mut values.b),
            (self.from, &mut values.f),
            (self.into, &mut values.i),
        ] {
            for (serial, value) in serials.iter().zip(self.values(side, &serials)?) {
                map.insert(*serial, value);
            }
        }
        Ok(values)
    }

    fn name_batch(
        &mut self,
        listing: &mut Listing,
        mode: Mode,
        names: Vec<Name>,
        out: &mut Option<ChangeWriter>,
        extras: &mut std::iter::Peekable<std::vec::IntoIter<(PathName, Option<u64>)>>,
        count: &mut usize,
    ) -> ContentResult<()> {
        if names.is_empty() {
            return Ok(());
        }
        let values = self.name_values(&names)?;
        for (name, b, f, i) in names {
            if mode == Mode::Decide {
                self.decide_name(listing, &name, [b, f, i], &values)?;
                continue;
            }
            while let Some((extra, _)) = extras.peek() {
                if extra.as_bytes() < name.as_bytes() {
                    let (extra, binding) = extras.next().expect("peeked");
                    *count += 1;
                    if let Some(out) = out.as_mut() {
                        out.push(self.spill, &extra, binding)?;
                    }
                } else {
                    if extra.as_bytes() == name.as_bytes() {
                        extras.next();
                    }
                    break;
                }
            }
            let result = self.result_name(listing, &name, [b, f, i], &values)?;
            let changed = match listing.into {
                Some(_) => result != i,
                None => result.is_some(),
            };
            if changed {
                *count += 1;
                if let Some(out) = out.as_mut() {
                    out.push(self.spill, &name, result)?;
                }
            }
        }
        Ok(())
    }

    /// Deciding pass at one name.
    fn decide_name(
        &mut self,
        listing: &mut Listing,
        name: &PathName,
        [b, f, i]: [Option<u64>; 3],
        values: &Values,
    ) -> ContentResult<()> {
        if i == f {
            return Ok(());
        }
        if i == b {
            if let Some(added) = f {
                self.added(listing.target, name, added, values)?;
            }
            if let (Some(c), Some(_)) = (b, f) {
                let deleted = Values::get(&values.f, Some(c)).is_none();
                let changed = !same(
                    Values::get(&values.i, Some(c)),
                    Values::get(&values.b, Some(c)),
                );
                if deleted && changed {
                    self.collide(listing, name, [b, f, i], values, false)?;
                }
            }
            return Ok(());
        }
        self.collide(listing, name, [b, f, i], values, true)
    }

    /// A name both sides rebound, or a replacement of a changed file:
    /// decided by the entries it binds.
    fn collide(
        &mut self,
        listing: &mut Listing,
        name: &PathName,
        [b, f, i]: [Option<u64>; 3],
        values: &Values,
        note_added: bool,
    ) -> ContentResult<()> {
        let base = Values::entry(&values.b, b);
        let ours = Values::entry(&values.f, f);
        let theirs = Values::entry(&values.i, i);
        let path = join(&self.slot_path(listing)?, name);
        let key = named_key(listing.target, name);
        if let (Some(fe), Some(ie)) = (ours, theirs) {
            let absorb = is_directory(Some(fe.value))
                && is_directory(Some(ie.value))
                && Values::get(&values.i, Some(fe.serial)).is_none()
                && Values::get(&values.f, Some(ie.serial)).is_none()
                && self.spill.get(TARGET, single(ie.serial))?.is_none();
            if absorb {
                self.spill
                    .insert(NAMED, key, put_binding(Some(ie.serial)))?;
                return self.absorb(base, fe, ie, path);
            }
        }
        let result = self.decide(&path, [base, ours, theirs])?;
        if note_added {
            if let (Some(Ok(entry)), Some(added)) = (&result, f) {
                if entry.serial == added {
                    self.added(listing.target, name, added, values)?;
                }
            }
        }
        let carrier = self.carrier(result, theirs)?;
        if let Some(lost) = f.filter(|lost| carrier != Some(*lost)) {
            let new = Values::get(&values.b, Some(lost)).is_none()
                && Values::get(&values.i, Some(lost)).is_none();
            if new {
                self.drop_new(lost)?;
            }
        }
        self.spill.insert(NAMED, key, put_binding(carrier))
    }

    /// Two directories at one name: `from`'s `fe`'s names go into `into`'s
    /// `ie`, against the base's directory there.
    fn absorb(
        &mut self,
        base: Option<EntryFacts>,
        fe: EntryFacts,
        ie: EntryFacts,
        path: String,
    ) -> ContentResult<()> {
        let base_directory = base.filter(|entry| is_directory(Some(entry.value)));
        let mut target = fe.serial.to_be_bytes().to_vec();
        if let Some(entry) = base_directory {
            target.extend_from_slice(&entry.serial.to_be_bytes());
        }
        self.spill
            .insert(ABSORB, single(fe.serial), ie.serial.to_be_bytes().to_vec())?;
        self.spill.insert(TARGET, single(ie.serial), target)?;
        let mut inner = Listing {
            target: ie.serial,
            base: base_directory.map(|entry| entry.value.content_root),
            from: Some(fe.value.content_root),
            into: Some(ie.value.content_root),
            path: PathSlot::Known(path),
        };
        self.names(&mut inner, Mode::Decide)?;
        Ok(())
    }

    /// `from` changed the names of the directory `f`, which `into` replaced
    /// with `there` at its unchanged `path`: when `there` is a new directory,
    /// it takes `f`'s names, as the other direction decides it at the name.
    /// Called in the deciding pass instead of deciding `f`'s names against
    /// no listing, so each name is decided once. True when absorbed.
    pub(super) fn absorb_replaced(
        &mut self,
        path: &str,
        b: EntryFacts,
        f: EntryFacts,
        there: EntryFacts,
    ) -> ContentResult<bool> {
        if !is_directory(Some(there.value))
            || self.value(self.from, there.serial)?.is_some()
            || self.spill.get(TARGET, single(there.serial))?.is_some()
        {
            return Ok(false);
        }
        self.absorb(Some(b), f, there, path.to_owned())?;
        Ok(true)
    }

    /// The rest of [`Self::absorb_replaced`]'s case, once every moved
    /// directory is known: decided at the path. A kept `f` is resurrected
    /// with its subtree; a lost one leaves the serials new below it unbound.
    pub(super) fn replaced_directory(
        &mut self,
        path: &str,
        b: EntryFacts,
        f: EntryFacts,
        there: EntryFacts,
    ) -> ContentResult<()> {
        let result = self.decide(path, [Some(b), Some(f), Some(there)])?;
        let carrier = self.carrier(result, Some(there))?;
        if carrier == Some(f.serial) {
            self.resurrect(f.serial, Some(f.value))?;
        } else {
            self.drop_below(f.value.content_root)?;
        }
        if carrier != Some(there.serial) {
            let (parent, name) = split(path);
            let parent = self
                .resolver
                .entry(self.into, parent)?
                .ok_or(ContentError::InvalidRecord("merge replaced parent"))?;
            self.extra(parent.serial, name, carrier)?;
        }
        Ok(())
    }

    /// `from` bound `serial` at `name`: a serial new to the base needs
    /// nothing; one `into` deleted is resurrected from `from`; a directory
    /// `into` holds is checked for a second move and for a cycle.
    fn added(
        &mut self,
        target: u64,
        name: &PathName,
        serial: u64,
        values: &Values,
    ) -> ContentResult<()> {
        if Values::get(&values.b, Some(serial)).is_none() {
            return Ok(());
        }
        let from = Values::get(&values.f, Some(serial));
        if Values::get(&values.i, Some(serial)).is_none() {
            return self.resurrect(serial, from);
        }
        if is_directory(from) && self.spill.get(MOVED, single(serial))?.is_none() {
            let mut bytes = target.to_be_bytes().to_vec();
            bytes.extend_from_slice(name.as_bytes());
            self.spill.insert(MOVED, single(serial), bytes)?;
        }
        Ok(())
    }

    /// Writing pass at one name: the binding the result holds there.
    fn result_name(
        &mut self,
        listing: &Listing,
        name: &PathName,
        [b, f, i]: [Option<u64>; 3],
        values: &Values,
    ) -> ContentResult<Option<u64>> {
        if let Some(bytes) = self.spill.get(NAMED, named_key(listing.target, name))? {
            return read_binding(&bytes);
        }
        if i == f {
            return Ok(i);
        }
        if i != b {
            return Err(ContentError::InvalidRecord("merge name without a decision"));
        }
        let held = |kind: u32, serial: u64| -> ContentResult<bool> {
            Ok(self.spill.get(kind, single(serial))?.is_some())
        };
        Ok(match (b, f) {
            (None, Some(d)) => (!held(CANCEL_MOVE, d)?).then_some(d),
            (Some(c), None) => {
                let kept = if Values::get(&values.f, Some(c)).is_some() {
                    held(CANCEL_MOVE, c)?
                } else {
                    held(CANCEL_REMOVE, c)?
                };
                kept.then_some(c)
            }
            (Some(c), Some(d)) => Some(if held(CANCEL_MOVE, d)? { c } else { d }),
            (None, None) => None,
        })
    }

    /// The bindings recorded for `target` outside its name points, by name.
    fn extras(&self, target: u64) -> ContentResult<Vec<(PathName, Option<u64>)>> {
        let mut extras = match self.spill.get(EXTRA, single(target))? {
            Some(bytes) => read_extras(&bytes)?,
            None => Vec::new(),
        };
        extras.sort_by(|x, y| x.0.as_bytes().cmp(y.0.as_bytes()));
        Ok(extras)
    }

    /// Records one binding of `target` outside its name points.
    pub(super) fn extra(
        &mut self,
        target: u64,
        name: &str,
        binding: Option<u64>,
    ) -> ContentResult<()> {
        let key = single(target);
        let old = self.spill.get(EXTRA, key)?;
        if let Some(bytes) = &old {
            if read_extras(bytes)?
                .iter()
                .any(|(held, _)| held.as_str() == name)
            {
                return Ok(());
            }
        }
        let mut bytes = old.clone().unwrap_or_default();
        super::codec::put_text(&mut bytes, name)?;
        match binding {
            None => bytes.push(0),
            Some(serial) => {
                bytes.push(1);
                bytes.extend_from_slice(&serial.to_be_bytes());
            }
        }
        match old {
            None => self.spill.insert(EXTRA, key, bytes),
            Some(old) => self.spill.replace(EXTRA, key, old, bytes),
        }
    }

    /// True when `serial` is not written: absorbed into another directory,
    /// or new on `from` and left unbound.
    pub(super) fn absorbed(&self, serial: u64) -> ContentResult<bool> {
        Ok(self.spill.get(ABSORB, single(serial))?.is_some()
            || self.spill.get(DROP, single(serial))?.is_some())
    }

    /// The listing's path, named once.
    pub(super) fn slot_path(&mut self, listing: &mut Listing) -> ContentResult<String> {
        if let PathSlot::Name(side, serial) = listing.path {
            listing.path = PathSlot::Known(self.path(side, serial)?);
        }
        match &listing.path {
            PathSlot::Known(path) => Ok(path.clone()),
            PathSlot::Name(..) => unreachable!("named above"),
        }
    }

    /// Marks a serial `from` bound that `into` deleted as resurrected with
    /// its whole subtree as `from` holds it.
    fn resurrect(&mut self, serial: u64, from: Option<InodeValue>) -> ContentResult<()> {
        if self.spill.get(REVIVE_ALL, single(serial))?.is_some() {
            return Ok(());
        }
        self.spill.insert(REVIVE_ALL, single(serial), Vec::new())?;
        if let Some(root) = listing(from) {
            self.resurrect_below(root)?;
        }
        Ok(())
    }
}

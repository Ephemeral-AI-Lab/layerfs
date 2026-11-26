//! Positive physical metadata acquired on the same captured Commit's reader.
use crate::location::{
    ChainMember, LocatedChain, LocatedChains, LocatedObject, ObjectLocation, PackDirectory,
    PackDomain, PackInfo,
};
use layerfs_content::{ObjectId, ObjectRole};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    mem::size_of,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use super::Fetch;

const ROOTS: usize = 2048;
const MEMBERS: usize = 4096;
const PACKS: usize = 64;
const PREFIX_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy)]
struct Position {
    length: u32,
    pack: u16,
    group: u16,
    record: u16,
}

impl Position {
    fn new(pack: u16, length: usize, group: usize, record: usize) -> Option<Self> {
        Some(Self {
            length: length.try_into().ok()?,
            pack,
            group: group.try_into().ok()?,
            record: record.try_into().ok()?,
        })
    }
}

struct Root {
    id: ObjectId,
    role: ObjectRole,
    position: Position,
    first: u16,
    end: u16,
}

struct Directory {
    info: PackInfo,
    prefix: Box<[u8]>,
}

#[derive(Default)]
struct Capture {
    generation: Option<u64>,
    roots: Vec<Root>,
    members: Vec<Position>,
    packs: Vec<Directory>,
    prefix_bytes: usize,
}

impl Capture {
    fn pack(&mut self, answer: &LocatedChains, info: PackInfo) -> Option<u16> {
        if info.domain != PackDomain::Metadata {
            return None;
        }
        if let Some(at) = self
            .packs
            .iter()
            .position(|p| p.info.pack_id == info.pack_id)
        {
            return (self.packs[at].info == info).then_some(at as u16);
        }
        let directory = answer.directories.iter().find(|row| row.pack == info)?;
        if self.packs.len() == PACKS || self.prefix_bytes + directory.prefix.len() > PREFIX_BYTES {
            return None;
        }
        let at = self.packs.len() as u16;
        self.prefix_bytes += directory.prefix.len();
        self.packs.push(Directory {
            info,
            prefix: directory.prefix.as_slice().into(),
        });
        Some(at)
    }

    fn insert(&mut self, row: &LocatedObject, answer: &LocatedChains) -> Option<()> {
        let location = row.location;
        if !matches!(
            location.role,
            ObjectRole::InodeLeaf | ObjectRole::InodeBranch
        ) || location.base.is_some()
            || self.roots.len() == ROOTS
        {
            return None;
        }
        let at = self
            .roots
            .binary_search_by_key(&location.object_id, |root| root.id)
            .err()?;
        let chain = answer
            .chains
            .iter()
            .find(|chain| chain.root == location.object_id);
        let members = chain.map_or(&[][..], |chain| chain.members.as_slice());
        let first = self.members.len();
        if first + members.len() > MEMBERS
            || members.iter().any(|member| member.object_id.is_some())
        {
            return None;
        }
        let position = Position::new(
            self.pack(answer, row.pack)?,
            location.canonical_length,
            location.group_number,
            location.record_number,
        )?;
        let chain = members
            .iter()
            .map(|member| {
                Position::new(
                    self.pack(answer, member.pack)?,
                    member.canonical_length,
                    member.group_number,
                    member.record_number,
                )
            })
            .collect::<Option<Vec<_>>>()?;
        self.members.extend(chain);
        self.roots.insert(
            at,
            Root {
                id: location.object_id,
                role: location.role,
                position,
                first: first as u16,
                end: self.members.len() as u16,
            },
        );
        Some(())
    }

    fn answers(&self, roots: &[ObjectId]) -> Option<(Vec<LocatedObject>, LocatedChains)> {
        let sources = roots
            .iter()
            .map(|root| {
                self.roots
                    .binary_search_by_key(root, |row| row.id)
                    .ok()
                    .map(|at| &self.roots[at])
            })
            .collect::<Option<Vec<_>>>()?;
        let mut packs = BTreeSet::new();
        let mut locations = Vec::with_capacity(sources.len());
        let mut answer = LocatedChains::default();
        for source in sources {
            let pos = source.position;
            packs.insert(pos.pack);
            let info = self.packs[usize::from(pos.pack)].info;
            locations.push(LocatedObject {
                location: ObjectLocation {
                    object_id: source.id,
                    role: source.role,
                    canonical_length: pos.length as usize,
                    pack_id: info.pack_id,
                    group_number: usize::from(pos.group),
                    record_number: usize::from(pos.record),
                    base: None,
                },
                pack: info,
            });
            if source.first != source.end {
                let members = self.members[usize::from(source.first)..usize::from(source.end)]
                    .iter()
                    .map(|pos| {
                        packs.insert(pos.pack);
                        ChainMember {
                            object_id: None,
                            pack: self.packs[usize::from(pos.pack)].info,
                            group_number: usize::from(pos.group),
                            record_number: usize::from(pos.record),
                            canonical_length: pos.length as usize,
                        }
                    })
                    .collect();
                answer.chains.push(LocatedChain {
                    root: source.id,
                    members,
                });
            }
        }
        answer.directories.extend(packs.into_iter().map(|at| {
            let pack = &self.packs[usize::from(at)];
            PackDirectory {
                pack: pack.info,
                prefix: pack.prefix.to_vec(),
            }
        }));
        Some((locations, answer))
    }
}

/// An operation-owned prefetch budget for one captured base. The caller
/// supplies it only to the Workspace reader and Save of that exact Store
/// and Commit, while their mount still protects the immutable references.
/// This retains positive metadata and never retains an absence or body.
pub struct CapturedLocators {
    admission: Arc<AtomicBool>,
    captured: Mutex<Capture>,
}

pub(crate) enum LocatorMode {
    Record(Arc<CapturedLocators>),
    Reuse(Arc<CapturedLocators>),
}

pub(crate) struct LocatorScope<'a>(&'a RefCell<Option<LocatorMode>>, Option<LocatorMode>);

impl Drop for LocatorScope<'_> {
    fn drop(&mut self) {
        self.0.replace(self.1.take());
    }
}

impl Fetch {
    pub(crate) fn scoped_locators(&self, mode: Option<LocatorMode>) -> LocatorScope<'_> {
        LocatorScope(
            &self.captured_locators,
            self.captured_locators.replace(mode),
        )
    }

    pub(super) fn retained_locators(
        &self,
        roots: &[ObjectId],
    ) -> Option<(Vec<LocatedObject>, LocatedChains)> {
        let mode = self.captured_locators.borrow();
        match mode.as_ref()? {
            LocatorMode::Reuse(cache) => {
                cache.answers(self.metadata.captured_locator_generation()?, roots)
            }
            LocatorMode::Record(_) => None,
        }
    }

    pub(super) fn remember_captured_locators(
        &self,
        rows: &[LocatedObject],
        answer: &LocatedChains,
    ) {
        let mode = self.captured_locators.borrow();
        if let Some(LocatorMode::Record(cache)) = mode.as_ref() {
            if let Some(generation) = self.metadata.captured_locator_generation() {
                cache.remember(generation, rows, answer);
            }
        }
    }
}

const _: () = assert!(
    size_of::<CapturedLocators>()
        + ROOTS * size_of::<Root>()
        + MEMBERS * size_of::<Position>()
        + PACKS * size_of::<Directory>()
        + PREFIX_BYTES
        <= 196 * 1024
);

impl CapturedLocators {
    /// The mount owner clears `admission` before it releases the catalog
    /// admission that protects these locators.
    pub fn new(admission: Arc<AtomicBool>) -> Self {
        Self {
            admission,
            captured: Mutex::default(),
        }
    }

    pub(super) fn remember(&self, generation: u64, rows: &[LocatedObject], answer: &LocatedChains) {
        if !self.admission.load(Ordering::Acquire) {
            return;
        }
        let Ok(mut cache) = self.captured.lock() else {
            return;
        };
        let current = cache.generation.get_or_insert(generation);
        if *current != generation
            || !rows.iter().any(|row| {
                matches!(
                    row.location.role,
                    ObjectRole::InodeLeaf | ObjectRole::InodeBranch
                ) && row.pack.domain == PackDomain::Metadata
            })
        {
            return;
        }
        if cache.roots.capacity() == 0 {
            cache.roots = Vec::with_capacity(ROOTS);
            cache.members = Vec::with_capacity(MEMBERS);
            cache.packs = Vec::with_capacity(PACKS);
        }
        for row in rows {
            let _ = cache.insert(row, answer);
        }
    }

    pub(super) fn answers(
        &self,
        generation: u64,
        roots: &[ObjectId],
    ) -> Option<(Vec<LocatedObject>, LocatedChains)> {
        let cache = self.captured.lock().ok()?;
        if cache.generation != Some(generation) || !self.admission.load(Ordering::Acquire) {
            return None;
        }
        cache.answers(roots)
    }
}

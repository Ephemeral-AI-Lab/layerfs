//! A history walk's versions of one path, resolved in windows (#818 H5).
//!
//! Blame and `fs.revision` look the same path up in every version they
//! walk. Where the Store pays a round trip per demand and serves per-object
//! reads at once, a version the walk reaches opens a window: that version
//! and the Commits the walk's row window holds after it ([`Rows::upcoming`]),
//! 16, 32, 64 and then 128 versions, resolved level by level together
//! ([`PathAcross`]): one roots demand, one demand per inode level and one per
//! directory page level of the directory memo's misses.
//!
//! A window reads ahead of the walk, and the walk may stop before its end,
//! so it reads with [`AuthenticatedObjects::read_canonical_each`]: an object
//! that is missing, mismatched or undecodable is that object's failure, which
//! the call's caches record. If the window fails on such a failure, or on a
//! page that does not decode, each of its versions is walked alone when the
//! walk reaches it, through the same caches: a recorded failure is served
//! from the record, never read again, so it refuses only at a version that
//! reads that object, where the walk refused before. A failure of the demand
//! itself (the session, the transport) refuses at once, as a lost reply
//! does. A version whose strict walk refuses (an intermediate symlink, a
//! bound component without its record) is walked alone too, and refuses
//! there with today's refusal.
//!
//! Elsewhere (SQLite, or a reader without per-object demands) each version
//! is walked alone, as before.
use super::cached::Cached;
use crate::history::{lookup as args, rows::Rows};
use crate::store::Store;
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::compare::{AcrossWork, EntryFacts, PathAcross, PathAnswer};
use layerfs_content::filesystem::{path::PathName, FilesystemRootId};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_history::NodeId;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// The window widths, one after another, then the last for every window
/// after (D5).
const WIDTHS: [usize; 4] = [16, 32, 64, 128];

/// Whether a walk resolves its versions in windows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Batching {
    /// Where the Store pays a round trip per demand and the reader serves
    /// per-object reads at once: the product's choice.
    Provider,
    /// Always, whatever the reader: per-object reads one by one where the
    /// reader has no per-object demand. For tests of the windows' answers
    /// and refusal order; it costs more reads, never fewer.
    Forced,
}

impl Batching {
    fn on(self, store: &Store, reader: &Cached<'_>) -> bool {
        match self {
            Self::Provider => store.batched_reads() && reader.reads_each_at_once(),
            Self::Forced => true,
        }
    }
}

thread_local! {
    static WINDOWS: Cell<u64> = const { Cell::new(0) };
}

/// Version windows this thread has resolved together, for tests that check
/// a walk did read ahead.
pub fn windows_resolved() -> u64 {
    WINDOWS.with(Cell::get)
}

/// One walk's path lookups across versions.
pub struct Versions<'r, 'c> {
    reader: &'r Cached<'c>,
    path: &'r str,
    /// The path's components, when the walk reads in windows.
    across: Option<PathAcross>,
    windows: usize,
    /// The open window's answers by version.
    answers: HashMap<NodeId, Option<EntryFacts>>,
    /// The open window's versions walked alone.
    alone: HashSet<NodeId>,
}

impl<'r, 'c> Versions<'r, 'c> {
    /// The lookups of `path` for a walk over `store`. A path with an
    /// invalid component is walked alone in every version, so it refuses
    /// where it did.
    pub fn new(store: &Store, reader: &'r Cached<'c>, path: &'r str, batching: Batching) -> Self {
        let across = batching
            .on(store, reader)
            .then(|| {
                path.split('/')
                    .filter(|name| !name.is_empty())
                    .map(|name| PathName::new(name).ok())
                    .collect::<Option<Vec<_>>>()
            })
            .flatten()
            .map(|names| PathAcross::new(names, true));
        Self {
            reader,
            path,
            across,
            windows: 0,
            answers: HashMap::new(),
            alone: HashSet::new(),
        }
    }

    /// The entry at the path in `node`'s tree `root`, strictly, as
    /// [`Cached::entry`] finds it; `rows` names the versions a new window
    /// takes after `node`.
    pub fn entry(
        &mut self,
        rows: &Rows<'_>,
        node: NodeId,
        root: ObjectId,
    ) -> Result<Option<EntryFacts>, SdkRefusal> {
        if self.across.is_none() || self.alone.contains(&node) {
            return self.reader.entry(root, self.path);
        }
        if let Some(found) = self.answers.get(&node) {
            return Ok(*found);
        }
        let width = WIDTHS[self.windows.min(WIDTHS.len() - 1)];
        let mut versions = vec![(node, root)];
        versions.extend(rows.upcoming(width - 1));
        if versions.len() == 1 {
            return self.reader.entry(root, self.path);
        }
        self.window(&versions)?;
        match self.answers.get(&node) {
            Some(found) if !self.alone.contains(&node) => Ok(*found),
            _ => self.reader.entry(root, self.path),
        }
    }

    /// Resolves one window, keeping each version's answer or marking it to
    /// be walked alone; refuses only on a failure of a demand itself.
    fn window(&mut self, versions: &[(NodeId, ObjectId)]) -> Result<(), SdkRefusal> {
        self.windows += 1;
        WINDOWS.with(|count| count.set(count.get() + 1));
        self.answers.clear();
        self.alone.clear();
        let Some(across) = self.across.as_mut() else {
            return Ok(());
        };
        let roots: Vec<FilesystemRootId> = versions
            .iter()
            .map(|(_, root)| FilesystemRootId(*root))
            .collect();
        let each = Each {
            reader: self.reader,
            demand: RefCell::new(None),
        };
        let resolved = {
            let mut memo = self.reader.memo();
            across.resolve(&each, &roots, &mut memo, true, &mut AcrossWork::default())
        };
        match resolved {
            Ok(paths) => {
                for ((node, _), path) in versions.iter().zip(paths) {
                    match path.answer {
                        PathAnswer::Entry(facts) => {
                            self.answers.insert(*node, Some(facts));
                        }
                        PathAnswer::Absent => {
                            self.answers.insert(*node, None);
                        }
                        PathAnswer::Symlink { .. } | PathAnswer::Unbound { .. } => {
                            self.alone.insert(*node);
                        }
                    }
                }
                Ok(())
            }
            Err(error) => match each.demand.into_inner() {
                Some(failure) => Err(args::content(failure)),
                None => {
                    let _ = error;
                    self.alone.extend(versions.iter().map(|(node, _)| *node));
                    Ok(())
                }
            },
        }
    }
}

/// The window's reader: every demand is a per-object read through the
/// call's caches. A demand that fails as a whole is kept, so the window
/// refuses with it; an object's own failure fails only the window.
struct Each<'r, 'c> {
    reader: &'r Cached<'c>,
    demand: RefCell<Option<ContentError>>,
}

impl AuthenticatedObjects for Each<'_, '_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        match self.reader.read_canonical_each(ids) {
            Ok(each) => each.into_iter().collect(),
            Err(error) => {
                self.demand.replace(Some(error.clone()));
                Err(error)
            }
        }
    }
}

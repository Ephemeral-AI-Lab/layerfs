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
//! A window also reads ahead what the walk reads at its versions (#818 H6),
//! through the same per-object reads: its regular files' lengths in one
//! demand, then for blame the roots of the texts it compares while their
//! comparisons fit its byte budget, for `fs.revision` its entries' metadata,
//! at most [`ROOTS`] roots and [`super::cached::ROOT_BYTES`] held in one
//! demand. Each read
//! ahead is kept or held by the call's caches and served where the walk asks
//! for it, a recorded failure there too.
//!
//! Once the call's failure record could fill ([`Cached::room`]), no window
//! reads further: its versions are walked alone, each object read once where
//! the walk needs it.
//!
//! Elsewhere (SQLite, or a reader without per-object demands) each version
//! is walked alone, as before.
use super::cached::{batches, Cached, ROOTS};
use crate::history::{lookup as args, rows::Rows};
use crate::store::Store;
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::compare::{AcrossWork, EntryFacts, PathAcross, PathAnswer};
use layerfs_content::filesystem::{path::PathName, FilesystemRootId};
use layerfs_content::object::inode_leaf::InodeKind;
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
    /// Never: each version walked alone, as SQLite walks it. For tests that
    /// hold the windows' answers and refusal order to the lone walk's.
    Alone,
}

impl Batching {
    /// Whether this call plans its reads ahead over `store` and `reader`.
    pub fn on(self, store: &Store, reader: &Cached<'_>) -> bool {
        match self {
            Self::Provider => store.batched_reads() && reader.reads_each_at_once(),
            Self::Alone => false,
        }
    }
}

/// What a window reads ahead beside its versions' entries (#818 H6).
#[derive(Clone, Copy, Debug)]
pub enum Ahead {
    /// Blame: the walk compares the newer side's file `newer` with each
    /// older version whose file differs, while the comparisons' bytes fit
    /// `budget`; their lengths and texts' roots.
    Texts { newer: ObjectId, budget: u64 },
    /// `fs.revision`: the entries' lengths and metadata.
    Facts,
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
        ahead: Ahead,
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
        self.ahead(&versions, ahead)?;
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

impl Versions<'_, '_> {
    /// Reads ahead what the walk reads at the window's versions it answered,
    /// in walk order up to the first it did not.
    fn ahead(&self, versions: &[(NodeId, ObjectId)], ahead: Ahead) -> Result<(), SdkRefusal> {
        let entries: Vec<EntryFacts> = versions
            .iter()
            .map_while(|(node, _)| self.answers.get(node).copied().flatten())
            .collect();
        if entries.is_empty() {
            return Ok(());
        }
        let files = |entries: &[EntryFacts]| -> Vec<ObjectId> {
            entries
                .iter()
                .filter(|e| e.value.kind == InodeKind::RegularFile)
                .map(|e| e.value.content_root)
                .collect()
        };
        let reader = self.reader;
        match ahead {
            Ahead::Facts => {
                reader.lengths_each(&files(&entries))?;
                let mut metadata: Vec<ObjectId> = Vec::new();
                // An inline form needs no read; an attribute tree's root does.
                for root in entries.iter().filter_map(|e| e.value.metadata_root.root()) {
                    if metadata.len() < ROOTS && !metadata.contains(&root) {
                        metadata.push(root);
                    }
                }
                reader.hold_each(&metadata).map_err(args::content)
            }
            Ahead::Texts { newer, budget } => {
                // Blame stops at a version whose path is no regular file.
                let walk: Vec<EntryFacts> = entries
                    .into_iter()
                    .take_while(|e| e.value.kind == InodeKind::RegularFile)
                    .collect();
                let mut wanted = vec![newer];
                wanted.extend(files(&walk));
                reader.lengths_each(&wanted)?;
                let (mut previous, mut spent) = (newer, 0u64);
                let (mut texts, mut lengths) = (Vec::new(), Vec::new());
                for root in files(&walk) {
                    if root == previous {
                        continue;
                    }
                    let (Some(Ok(a)), Some(Ok(b))) = (reader.length(previous), reader.length(root))
                    else {
                        break;
                    };
                    spent = spent.saturating_add(a.saturating_add(b).saturating_mul(3));
                    if spent > budget {
                        break;
                    }
                    texts.push(root);
                    lengths.push(b);
                    previous = root;
                }
                let first = batches(&lengths).into_iter().next().unwrap_or_default();
                reader.hold_each(&texts[first]).map_err(args::content)
            }
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
        // No room to record these objects' failures: the window reads no
        // further and its versions are walked alone (not a demand failure).
        if !self.reader.room(ids.len()) {
            return Err(super::cached::FULL);
        }
        match self.reader.read_canonical_each(ids) {
            Ok(each) => each.into_iter().collect(),
            Err(error) => {
                self.demand.replace(Some(error.clone()));
                Err(error)
            }
        }
    }
}

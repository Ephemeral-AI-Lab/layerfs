//! Names a merge's changed inodes from recorded bindings, top down.
//!
//! Each changed inode's recorded `(parent, name)` bindings are checked against
//! the tree and climbed to the root, so every path of every changed inode is
//! found with work that follows the changed inodes times their depth, not the
//! tree. An inode is named by the path a breadth-first walk from the root would
//! reach first: the fewest components, then name order component by component,
//! which is the order [`locate`](crate::compare::tree::locate) lists in.
//!
//! The answer is complete only when every inode on the way has as many checked
//! bindings as its reference count. Otherwise, or past the depth and path
//! bounds, there is no answer and the caller names the inodes by the
//! breadth-first walk; a stale or absent hint never changes a name.

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use crate::compare::tree::TreeTable;
use crate::compare::BindingHints;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;
use crate::object::{AuthenticatedObjects, ObjectId};
use layerfs_telemetry::timer::TimingScope;

/// Components beyond which a climb gives up.
const DEPTH: usize = 4096;
/// Paths of one inode beyond which a climb gives up.
const PATHS: usize = 1024;

/// One inode's paths, each as its components from the root.
type Paths = Rc<Vec<Vec<PathName>>>;

/// What naming from recorded bindings did, summed over its calls (#352).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NamingWork {
    /// Inodes asked for.
    pub inodes: u64,
    /// Distinct inodes climbed: those asked for and their ancestors.
    pub climbed: u64,
    /// Recorded-binding reads, one per inode climbed.
    pub bindings: u64,
    /// Object read demands the climbs made of the reader.
    pub reads: u64,
}

impl NamingWork {
    /// Adds `other` to these totals.
    pub fn add(&mut self, other: &Self) {
        self.inodes += other.inodes;
        self.climbed += other.climbed;
        self.bindings += other.bindings;
        self.reads += other.reads;
    }
}

/// Counts the object read demands a climb makes; the bytes are the reader's.
struct Counted<'r> {
    inner: &'r dyn AuthenticatedObjects,
    reads: Cell<u64>,
}

impl AuthenticatedObjects for Counted<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.reads.set(self.reads.get() + 1);
        self.inner.read_canonical_batch(ids)
    }
    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        self.reads.set(self.reads.get() + 1);
        self.inner.read_canonical_batch_scoped(ids, scope)
    }
}

/// Climbs one tree's recorded bindings; each inode's paths are found once.
struct Climb<'r> {
    reader: &'r dyn AuthenticatedObjects,
    hints: &'r dyn BindingHints,
    tree: &'r TreeTable,
    /// Paths per inode; `None` when an inode cannot be named completely.
    paths: HashMap<u64, Option<Paths>>,
    bindings: u64,
}

impl Climb<'_> {
    /// Every path of `serial`, or `None` when its bindings, or an ancestor's,
    /// are not all recorded.
    fn paths(&mut self, serial: u64, depth: usize) -> ContentResult<Option<Paths>> {
        if serial == self.tree.root_serial() {
            return Ok(Some(Rc::new(vec![Vec::new()])));
        }
        if let Some(known) = self.paths.get(&serial) {
            return Ok(known.clone());
        }
        let found = self.climb(serial, depth)?;
        self.paths.insert(serial, found.clone());
        Ok(found)
    }

    fn climb(&mut self, serial: u64, depth: usize) -> ContentResult<Option<Paths>> {
        if depth >= DEPTH {
            return Ok(None);
        }
        let Some(value) = self.tree.value(self.reader, serial)? else {
            return Ok(None);
        };
        let mut checked = BTreeSet::new();
        self.bindings += 1;
        for (parent, name) in self.hints.bindings_of(serial)? {
            if self.tree.lookup(self.reader, parent, &name)? == Some(serial) {
                checked.insert((parent, name));
            }
        }
        if (checked.len() as u64) < value.namespace_ref_count.max(1) {
            return Ok(None);
        }
        let mut out = Vec::new();
        for (parent, name) in checked {
            let Some(above) = self.paths(parent, depth + 1)? else {
                return Ok(None);
            };
            for path in above.iter() {
                if out.len() == PATHS {
                    return Ok(None);
                }
                let mut path = path.clone();
                path.push(name.clone());
                out.push(path);
            }
        }
        Ok(Some(Rc::new(out)))
    }
}

/// The path [`locate`](crate::compare::tree::locate) would give each wanted
/// inode in `tree`, found from `hints` with work that follows the wanted
/// inodes times their depth; `None` when one of them cannot be named
/// completely from `hints` (the caller then uses `locate`). `work` gains what
/// this call did, whether or not it answers.
pub fn name_from_hints(
    reader: &dyn AuthenticatedObjects,
    hints: &dyn BindingHints,
    tree: &TreeTable,
    wanted: &BTreeSet<u64>,
    work: &mut NamingWork,
) -> ContentResult<Option<BTreeMap<u64, String>>> {
    let counted = Counted {
        inner: reader,
        reads: Cell::new(0),
    };
    let mut climb = Climb {
        reader: &counted,
        hints,
        tree,
        paths: HashMap::new(),
        bindings: 0,
    };
    let named = name_all(&mut climb, wanted);
    work.add(&NamingWork {
        inodes: wanted.len() as u64,
        climbed: climb.paths.len() as u64,
        bindings: climb.bindings,
        reads: counted.reads.get(),
    });
    named
}

fn name_all(
    climb: &mut Climb<'_>,
    wanted: &BTreeSet<u64>,
) -> ContentResult<Option<BTreeMap<u64, String>>> {
    let mut named = BTreeMap::new();
    for serial in wanted {
        let Some(paths) = climb.paths(*serial, 0)? else {
            return Ok(None);
        };
        let Some(first) = paths
            .iter()
            .min_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)))
        else {
            return Ok(None);
        };
        let path = if first.is_empty() {
            "/".to_owned()
        } else {
            // A name outside UTF-8 (the WTF-8 grammar) cannot ride a String
            // path: rendering it empty would collapse distinct entries and
            // every decision built on the path would be wrong. Refuse until
            // the byte-carrying merge/compare path lands (the grammar is
            // owner-ratified; the carriage is follow-up work).
            let mut path = String::new();
            for name in first {
                let text = std::str::from_utf8(name.as_bytes()).map_err(|_| {
                    ContentError::InvalidRecord("a name outside UTF-8 has no String path")
                })?;
                path.push('/');
                path.push_str(text);
            }
            path
        };
        named.insert(*serial, path);
    }
    Ok(Some(named))
}

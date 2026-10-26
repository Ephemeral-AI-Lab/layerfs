//! Depth-first namespace traversal at immutable roots, in name order, with
//! one memo per walk and, for a glob, only the folders it can match.
use super::snapshot;
use crate::history::lookup as args;
use crate::store::StorePorts;
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::{
    compare::{memo::Memo, EntryFacts},
    filesystem::{
        directory::read::{list_after, DirectoryReadWork},
        path::PathName,
        sorted::DirectoryRoot,
    },
    object::inode_leaf::{InodeKind, InodeValue},
    AuthenticatedObjects,
};

/// Names and inode demands acquired together, bounded independently of the tree.
const DIRECTORY_WINDOW: usize = 64;
const DIRECTORY_BYTES: usize = 8192;
type Child = (PathName, u64, Option<InodeValue>);

/// One directory frame retains one bounded window and its next-name boundary.
struct Frame {
    path: String,
    entry: EntryFacts,
    after: Option<PathName>,
    pending: std::vec::IntoIter<Child>,
    exhausted: bool,
}
impl Frame {
    fn new(path: String, entry: EntryFacts) -> Self {
        Self {
            path,
            entry,
            after: None,
            pending: Vec::new().into_iter(),
            exhausted: false,
        }
    }
}

/// The literal leading folders of an anchored glob, such as `a/b/` for
/// `a/b/**/*.js`: every path it matches starts with them. `None` when the
/// pattern is unanchored or starts with a wildcard. Follows the rule syntax
/// of `merge::Rule::glob`: a leading `!` and a trailing `/` are dropped, a
/// `/` anywhere else anchors, and only `*` and `?` are wildcards.
pub fn leading(pattern: &str) -> Option<String> {
    let body = pattern.strip_prefix('!').unwrap_or(pattern);
    let body = body.strip_suffix('/').unwrap_or(body);
    if !body.contains('/') {
        return None;
    }
    let body = body.strip_prefix('/').unwrap_or(body);
    let literal = &body[..body.find(['*', '?']).unwrap_or(body.len())];
    Some(literal[..literal.rfind('/')? + 1].to_owned())
}

/// True when some path below the folder `path` can start with `folders`.
fn agrees(folders: &str, path: &str) -> bool {
    let path = path.trim_start_matches('/');
    let below = if path.is_empty() {
        String::new()
    } else {
        format!("{path}/")
    };
    folders.starts_with(&below) || below.starts_with(folders)
}

pub struct Walker<'a> {
    /// Directory pages and the inode table's upper pages, read once per walk.
    memo: Memo<'a>,
    snap: &'a snapshot::Snap,
    stack: Vec<Frame>,
    top: Option<(String, EntryFacts)>,
    /// Folders every descended folder agrees with, from [`leading`].
    within: Option<String>,
}
impl<'a> Walker<'a> {
    pub fn new(
        reader: &'a StorePorts,
        snap: &'a snapshot::Snap,
        path: &str,
    ) -> Result<Self, SdkRefusal> {
        Self::within(reader, snap, path, None)
    }

    /// A walk that descends only into folders that agree with `within`. Its
    /// entries are the full walk's, less those no path starting with
    /// `within` can be, in the same order. A walk from above `within` starts
    /// at `within` itself: the folders on the way are shorter than it.
    /// Lookups, directory pages and inode values are read through `pages`.
    pub fn within(
        pages: &'a dyn AuthenticatedObjects,
        snap: &'a snapshot::Snap,
        path: &str,
        within: Option<String>,
    ) -> Result<Self, SdkRefusal> {
        let memo = Memo::new(pages);
        let entry = snapshot::existing(&memo, snap, path)?;
        let (mut stack, mut top) = (Vec::new(), None);
        if entry.value.kind != InodeKind::Directory {
            top = Some((path.to_owned(), entry));
        } else {
            match within.as_deref() {
                None => stack.push(Frame::new(path.to_owned(), entry)),
                Some(folders) if !agrees(folders, path) => {}
                Some(folders) => {
                    let deeper = format!("/{}", folders.trim_end_matches('/'));
                    let start = if deeper.len() > path.len() {
                        // A name the lookup refuses cannot be walked into.
                        snapshot::find(&memo, &snap.tree, &deeper)
                            .ok()
                            .flatten()
                            .filter(|entry| entry.value.kind == InodeKind::Directory)
                            .map(|entry| (deeper, entry))
                    } else {
                        Some((path.to_owned(), entry))
                    };
                    stack.extend(start.map(|(path, entry)| Frame::new(path, entry)));
                }
            }
        }
        Ok(Self {
            memo,
            snap,
            stack,
            top,
            within,
        })
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<(String, EntryFacts)>, SdkRefusal> {
        if let Some(top) = self.top.take() {
            return Ok(Some(top));
        }
        while let Some(frame) = self.stack.last_mut() {
            if frame.pending.len() == 0 && !frame.exhausted {
                let page = list_after(
                    &self.memo,
                    DirectoryRoot(frame.entry.value.content_root),
                    frame.after.as_ref(),
                    DIRECTORY_WINDOW,
                    DIRECTORY_BYTES,
                    &mut DirectoryReadWork::default(),
                )
                .map_err(args::content)?;
                frame.exhausted = page.continuation.is_none();
                frame.after = page.continuation;
                let serials = page
                    .entries
                    .iter()
                    .map(|(_, serial)| *serial)
                    .collect::<Vec<_>>();
                let values = self
                    .snap
                    .tree
                    .values(&self.memo, &serials)
                    .map_err(args::content)?;
                frame.pending = page
                    .entries
                    .into_iter()
                    .zip(values)
                    .map(|((name, serial), value)| (name, serial, value))
                    .collect::<Vec<_>>()
                    .into_iter();
            }
            let Some((name, serial, value)) = frame.pending.next() else {
                self.stack.pop();
                continue;
            };
            let path = layerfs_content::compare::tree::join(&frame.path, &name);
            let value = value.ok_or_else(|| args::failed("missing bound inode"))?;
            let entry = EntryFacts { serial, value };
            if value.kind == InodeKind::Directory
                && self.within.as_deref().is_none_or(|w| agrees(w, &path))
            {
                self.stack.push(Frame::new(path.clone(), entry));
            }
            return Ok(Some((path, entry)));
        }
        Ok(None)
    }
}

pub fn walk(
    reader: &StorePorts,
    snap: &snapshot::Snap,
    path: &str,
    visit: impl FnMut(&str, EntryFacts) -> Result<bool, SdkRefusal>,
) -> Result<(), SdkRefusal> {
    walk_within(reader, snap, path, None, visit)
}

/// [`walk`] through a [`Walker::within`].
pub fn walk_within(
    pages: &dyn AuthenticatedObjects,
    snap: &snapshot::Snap,
    path: &str,
    within: Option<String>,
    mut visit: impl FnMut(&str, EntryFacts) -> Result<bool, SdkRefusal>,
) -> Result<(), SdkRefusal> {
    let mut walker = Walker::within(pages, snap, path, within)?;
    while let Some((path, entry)) = walker.next()? {
        if !visit(&path, entry)? {
            break;
        }
    }
    Ok(())
}

//! The parent-path plan of one file call (#818 W6). The change list names
//! every path it touches before any change applies, so the base reads of
//! their walks are planned level by level: each level's directory pages
//! as one grouped demand, and the children found, with the child-directory
//! counts of those directories, as one inode wave. The reads only fill
//! the Workspace client's caches: every change still resolves its own
//! paths, the overlay first, so the answers stay the walk's own. Planned
//! only where the provider pays a round trip per demand (PostgreSQL).
use super::files::Change;
use super::mutation::{permitted, read_failure, Mutation, Result};
use super::recursive::overlap;
use crate::history::lookup::Arg;
use layerfs_content::filesystem::PathName;
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ContentResult;
use layerfs_workspace::BaseView;
use std::collections::BTreeMap;

/// Changes one plan covers (owner decision D10). `fs.apply` has no cap on
/// its changes, so a longer list is planned again every this many changes
/// and what one plan holds stays bounded.
pub const PLAN_PATHS: usize = 1_024;
/// Directories one planned level demand reads.
const PLAN_DIRECTORIES: usize = 128;
/// Name bytes one planned level demand looks up.
const PLAN_BYTES: usize = 1024 * 1024;

impl Mutation<'_> {
    /// Plans the base reads of `changes`' paths (at most [`PLAN_PATHS`]
    /// changes) before they apply. The plan stops at the first change
    /// refused before any read ([`refused_unread`]): that change answers its
    /// own refusal, as without a plan, and none after it applies; when it is
    /// the first, nothing is planned. A failed plan read thus refuses only
    /// at a change whose own walk reads.
    pub fn plan(&self, changes: &[Arg<Change<'_>>]) -> Result<()> {
        if !self.workspace.store.batched_reads() {
            return Ok(());
        }
        let planned = (changes.iter().position(refused_unread)).unwrap_or(changes.len());
        if planned == 0 {
            return Ok(());
        }
        let paths = (changes[..planned].iter())
            .filter_map(|change| change.as_ref().ok())
            .flat_map(|change| paths(change).into_iter().flatten());
        plan(self.view.base(), self.view.root_serial(), paths)
            .map_err(|error| read_failure(error.into()))
    }
}

/// Whether `change` is refused before it reads anything: its arguments, or
/// the first check of its operation (`Mutation::edit`'s empty `oldText`,
/// `mkdir`'s mode, `remove` of the root, `copy` and `rename` overlap).
fn refused_unread(change: &Arg<Change<'_>>) -> bool {
    match change {
        Err(_) => true,
        Ok(Change::Edit { old, .. }) => old.is_empty(),
        Ok(Change::Mkdir { path, mode }) => {
            permitted(path, InodeKind::Directory, mode.unwrap_or(0o755)).is_err()
        }
        Ok(Change::Rm { path, .. }) => *path == "/",
        Ok(Change::Cp { from, to } | Change::Mv { from, to }) => overlap(from, to).is_err(),
        Ok(Change::Write { .. } | Change::Chmod { .. }) => false,
        // A listed streamed write is refused before it reads (it has no
        // streamed input); streamed changes are never planned.
        Ok(Change::Streamed { .. }) => true,
    }
}

/// The paths one change resolves.
fn paths<'a>(change: &'a Change<'_>) -> [Option<&'a str>; 2] {
    match change {
        Change::Write { path, .. }
        | Change::Streamed { path, .. }
        | Change::Edit { path, .. }
        | Change::Mkdir { path, .. }
        | Change::Chmod { path, .. }
        | Change::Rm { path, .. } => [Some(path), None],
        Change::Cp { from, to } | Change::Mv { from, to } => [Some(from), Some(to)],
    }
}

/// One level's lookups: per parent directory, its child paths and names.
type Level<'a> = BTreeMap<&'a str, (&'a str, PathName)>;

fn plan<'a>(base: &BaseView, root: u64, paths: impl Iterator<Item = &'a str>) -> ContentResult<()> {
    // Per depth, each path prefix with its parent prefix and name; the
    // root is "".
    let mut levels = Vec::<Level<'a>>::new();
    for path in paths {
        let bounds: Vec<usize> = (path.match_indices('/').map(|(at, _)| at))
            .chain([path.len()])
            .collect();
        for depth in 0..bounds.len() - 1 {
            let Ok(name) = PathName::new(&path[bounds[depth] + 1..bounds[depth + 1]]) else {
                break;
            };
            if levels.len() == depth {
                levels.push(BTreeMap::new());
            }
            levels[depth].insert(&path[..bounds[depth + 1]], (&path[..bounds[depth]], name));
        }
    }
    let mut known = BTreeMap::from([("", base.inode(root)?.value)]);
    for level in levels {
        let mut parents: BTreeMap<&str, Vec<(&str, PathName)>> = BTreeMap::new();
        for (child, (parent, name)) in level {
            parents.entry(parent).or_default().push((child, name));
        }
        // One directory's names over one wave go in several groups.
        let parents = (parents.into_iter()).flat_map(|(parent, names)| {
            let mut names = names.into_iter().peekable();
            std::iter::from_fn(move || {
                names.peek()?;
                Some((
                    parent,
                    names
                        .by_ref()
                        .take(BaseView::PLAN_SERIALS)
                        .collect::<Vec<_>>(),
                ))
            })
        });
        let groups: Vec<(InodeValue, Vec<(&str, PathName)>)> = (parents.into_iter())
            .filter_map(|(parent, names)| {
                let value = known.get(parent)?;
                (value.kind == InodeKind::Directory).then_some((*value, names))
            })
            .collect();
        let mut rest = &groups[..];
        while !rest.is_empty() {
            let (window, after) = rest.split_at(window(rest));
            rest = after;
            let wanted: Vec<(InodeValue, Vec<PathName>)> = (window.iter())
                .map(|(value, names)| (*value, names.iter().map(|(_, n)| n.clone()).collect()))
                .collect();
            for ((_, names), found) in window.iter().zip(base.plan_level(&wanted)?) {
                for ((child, _), found) in names.iter().zip(found) {
                    if let Some((_, value)) = found {
                        known.insert(*child, value);
                    }
                }
            }
        }
    }
    Ok(())
}

/// How many of `groups` one level demand takes: at most
/// [`PLAN_DIRECTORIES`] directories, [`PLAN_BYTES`] of names and
/// [`BaseView::PLAN_SERIALS`] names, and at least one group (one
/// directory's names are split into groups of at most that many).
fn window(groups: &[(InodeValue, Vec<(&str, PathName)>)]) -> usize {
    let (mut bytes, mut names) = (0, 0);
    for (taken, (_, wanted)) in groups.iter().enumerate() {
        bytes += (wanted.iter())
            .map(|(_, name)| name.as_bytes().len())
            .sum::<usize>();
        names += wanted.len();
        let full = bytes > PLAN_BYTES || names > BaseView::PLAN_SERIALS;
        if taken == PLAN_DIRECTORIES || (taken > 0 && full) {
            return taken;
        }
    }
    groups.len()
}

//! Uncommitted changes that refuse an Unmount without `discard`.
//!
//! A Workspace is dirty when its active generation holds a change a Commit
//! would keep, a capture is still in flight, or its local base differs from
//! the history binding (an unpublished conflicted merge). The changed paths
//! are named from the same indexed windows a capture reads: a changed name is
//! joined to its parent's path, a changed file to its own, and a base
//! directory whose mode differs from the base to its own. Two kinds of row are
//! no change: an inode created locally that no name binds any more, and a
//! base directory whose mode equals the base, since its names are their own
//! rows and Commit ignores its mtime. A path is found among the active
//! bindings first, then by a bounded walk of the local base. At most
//! [`PATHS`] paths are named; the report says when some are not. The check is
//! an observation before terminal admission, so a write that lands after it
//! is closed with the Workspace.
use super::{registry::bound, Failure, Service};
use layerfs_bridge::control::{DirtyReport, WorkspaceToken};
use layerfs_content::compare::{locate, TreeTable};
use layerfs_content::filesystem::attributes::{read_portable, AttributeReadWork};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

/// Paths one report names.
const PATHS: usize = 256;
/// Changed rows one report reads, inodes and names each.
const ROWS: usize = 4096;
/// Base directories one path walk may list.
const WALK: usize = 4096;
/// Deepest local path resolved.
const DEPTH: usize = 256;

/// Paths of serials: local bindings first, then the local base.
struct Names {
    root: u64,
    local: BTreeMap<u64, (u64, String)>,
    base: BTreeMap<u64, String>,
}

impl Names {
    fn path(&self, serial: u64, depth: usize) -> Option<String> {
        if serial == self.root {
            return Some("/".to_owned());
        }
        if let Some((parent, name)) = self.local.get(&serial) {
            if depth < DEPTH {
                return self
                    .path(*parent, depth + 1)
                    .map(|parent| join(&parent, name));
            }
        }
        self.base.get(&serial).cloned()
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{parent}/{name}")
    }
}

impl Service {
    /// The uncommitted changes of one bound Workspace, or `None` when clean.
    pub(super) fn dirty(&self, token: WorkspaceToken) -> Result<Option<DirtyReport>, Failure> {
        let workspace = {
            let entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            bound(&entries, token)?.workspace.clone()
        };
        let route = workspace.route();
        let binding = workspace.snapshot().map_err(Failure::Workspace)?;
        let state = self.job(route, |client, route| client.state(route))?;
        let pending = workspace.pending_conflicts().map_err(Failure::Workspace)?;
        let moved = state.base_root != *binding.effective_root.as_bytes();
        if state.dirty_inodes == 0
            && state.dirty_directory_entries == 0
            && state.captured.is_none()
            && !moved
            && pending.is_none()
        {
            return Ok(None);
        }
        let mut truncated = false;
        let mut names = Vec::new();
        let mut after: Option<(u64, Vec<u8>)> = None;
        while names.len() < ROWS {
            let window = self.job(route, |client, route| {
                client.active_directory_entries(route, after.clone())
            })?;
            let Some(last) = window.last() else { break };
            after = Some((last.parent, last.name.clone()));
            names.extend(window);
        }
        let mut files = Vec::new();
        let mut directories = Vec::new();
        let mut rows = 0;
        let mut cursor = 0;
        while rows < ROWS {
            let window = self.job(route, |client, route| client.active_inodes(route, cursor))?;
            let Some(last) = window.last() else { break };
            cursor = last.serial;
            rows += window.len();
            for inode in window {
                let created = inode.born > state.installed as u64;
                if inode.kind != layerfs_overlay::InodeKind::Directory {
                    if !created || inode.nlink > 0 {
                        files.push(inode.serial);
                    }
                } else if !created {
                    directories.push((inode.serial, u32::from(inode.mode)));
                }
            }
        }
        truncated |= names.len() >= ROWS || rows >= ROWS;
        let reader = self.store.ports(binding.scope);
        let failed = |error: layerfs_content::ContentError| {
            Failure::Workspace(layerfs_workspace::WorkspaceError::Service(Box::new(error)))
        };
        let base = ObjectId::from_bytes(&state.base_root).map_err(failed)?;
        let tree = TreeTable::open(reader.as_ref(), FilesystemRootId(base)).map_err(failed)?;
        let mut work = AttributeReadWork::default();
        let mut modes = Vec::new();
        for (serial, mode) in directories {
            let same = match tree.value(reader.as_ref(), serial).map_err(failed)? {
                Some(value) => {
                    read_portable(reader.as_ref(), value.metadata_root, value.kind, &mut work)
                        .map_err(failed)?
                        .mode
                        == mode
                }
                None => false,
            };
            if !same {
                modes.push(serial);
            }
        }
        if names.is_empty()
            && files.is_empty()
            && modes.is_empty()
            && !truncated
            && state.captured.is_none()
            && !moved
            && pending.is_none()
        {
            return Ok(None);
        }
        let local = names
            .iter()
            .filter_map(|entry| {
                let serial = entry.serial?;
                Some((
                    serial,
                    (
                        entry.parent,
                        String::from_utf8_lossy(&entry.name).into_owned(),
                    ),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let wanted = names
            .iter()
            .map(|entry| entry.parent)
            .chain(local.values().map(|(parent, _)| *parent))
            .chain(files.iter().copied())
            .chain(modes.iter().copied())
            .filter(|serial| !local.contains_key(serial))
            .collect::<BTreeSet<_>>();
        let found = Names {
            root: tree.root_serial(),
            base: locate(reader.as_ref(), &tree, &wanted, WALK).map_err(failed)?,
            local,
        };
        let mut paths = BTreeSet::new();
        for entry in &names {
            match found.path(entry.parent, 0) {
                Some(parent) => {
                    paths.insert(join(&parent, &String::from_utf8_lossy(&entry.name)));
                }
                None => truncated = true,
            }
        }
        for serial in files.into_iter().chain(modes) {
            match found.path(serial, 0) {
                Some(path) => {
                    paths.insert(path);
                }
                None => truncated = true,
            }
        }
        paths.extend(pending.iter().flatten().cloned());
        truncated |= paths.len() > PATHS;
        Ok(Some(DirtyReport {
            token,
            paths: paths.into_iter().take(PATHS).collect(),
            inodes: state.dirty_inodes,
            entries: state.dirty_directory_entries,
            pending_merge: pending.is_some() || moved,
            truncated,
        }))
    }
}

//! What a Commit would capture now, as `workspace status` lists it (call
//! surface 2.0): one bounded read of the active generation's overlay rows,
//! the same indexed windows the Dirty check and a capture read, each
//! change named by its path with its kind and whether it is a folder.
//!
//! At most [`ROWS`] overlay rows are read, changed name bindings first,
//! then changed inodes; `partial` says rows were left unread. A changed
//! name is one entry: a whiteout over a name the base binds is `removed`, a
//! name the base did not bind is `added`, a name the base binds rebound to
//! another inode is `modified`; a folder renamed or removed is that one
//! entry. A changed file the base holds is `modified`; a base folder whose
//! mode changed is `modified`. A row that is no change (an inode created
//! and unbound again, a base folder whose mode equals the base) is no
//! entry. `under` keeps the entries at or below one folder, filtered within
//! the same read. Paths are found as the Dirty check finds them.
use super::dirty::{join, Names, WALK};
use super::{Failure, Service};
use crate::store::BoundWorkspace;
use layerfs_content::compare::{locate, TreeTable};
use layerfs_content::filesystem::attributes::{read_portable, AttributeReadWork};
use layerfs_content::filesystem::{FilesystemRootId, PathName};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

/// Overlay rows one read takes.
pub(crate) const ROWS: usize = 256;

/// One change: its path, `added`, `modified` or `removed`, and whether it
/// is a folder.
pub(crate) type Change = (String, &'static str, bool);

/// The changes one bounded read found.
pub(crate) struct Changed {
    /// The changes, in path order.
    pub entries: Vec<Change>,
    /// Whether overlay rows were left unread.
    pub partial: bool,
}

impl Service {
    /// The changes of bound Workspace `workspace`, at or below `under`.
    pub(crate) fn changed(
        &self,
        workspace: &BoundWorkspace,
        under: Option<&str>,
    ) -> Result<Changed, Failure> {
        let route = workspace.route();
        let binding = workspace.snapshot().map_err(Failure::Workspace)?;
        let state = self.job(route, |client, route| client.state(route))?;
        let mut partial = false;
        let mut names = Vec::new();
        let mut after: Option<(u64, Vec<u8>)> = None;
        while names.len() <= ROWS {
            let window = self.job(route, |client, route| {
                client.active_directory_entries(route, after.clone())
            })?;
            let Some(last) = window.last() else { break };
            after = Some((last.parent, last.name.clone()));
            names.extend(window);
        }
        if names.len() > ROWS {
            names.truncate(ROWS);
            partial = true;
        }
        let mut inodes = Vec::new();
        let mut cursor = 0;
        let budget = ROWS - names.len();
        while !partial && inodes.len() <= budget {
            let window = self.job(route, |client, route| client.active_inodes(route, cursor))?;
            let Some(last) = window.last() else { break };
            cursor = last.serial;
            inodes.extend(window);
        }
        if inodes.len() > budget {
            inodes.truncate(budget);
            partial = true;
        }
        let reader = self.store.ports(binding.scope);
        let failed = |error: layerfs_content::ContentError| {
            Failure::Workspace(layerfs_workspace::WorkspaceError::Service(Box::new(error)))
        };
        let base = ObjectId::from_bytes(&state.base_root).map_err(failed)?;
        let tree = TreeTable::open(reader.as_ref(), FilesystemRootId(base)).map_err(failed)?;
        let mut work = AttributeReadWork::default();
        // what each changed inode is: folder or not, and whether it is new
        let mut kinds: BTreeMap<u64, bool> = BTreeMap::new();
        let (mut files, mut modes) = (Vec::new(), Vec::new());
        let mut directories = Vec::new();
        for inode in &inodes {
            let folder = inode.kind == layerfs_overlay::InodeKind::Directory;
            kinds.insert(inode.serial, folder);
            let created = inode.born > state.installed as u64;
            match (folder, created) {
                (false, false) => files.push(inode.serial),
                (true, false) => directories.push((inode.serial, u32::from(inode.mode))),
                // a new inode is named by the binding that made it
                _ => {}
            }
        }
        let serials: Vec<u64> = directories.iter().map(|(serial, _)| *serial).collect();
        let values = tree.values(reader.as_ref(), &serials).map_err(failed)?;
        for ((serial, mode), value) in directories.into_iter().zip(values) {
            let same = match value {
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
        // a bound inode not among the changed ones is a base inode under a
        // new name: its kind is the base's
        let unknown: Vec<u64> = (names.iter())
            .filter_map(|entry| entry.serial)
            .filter(|serial| !kinds.contains_key(serial))
            .collect();
        let values = tree.values(reader.as_ref(), &unknown).map_err(failed)?;
        for (serial, value) in unknown.into_iter().zip(values) {
            let folder = value.is_some_and(|value| value.kind == InodeKind::Directory);
            kinds.insert(serial, folder);
        }
        let local = (names.iter())
            .filter_map(|entry| {
                let serial = entry.serial?;
                let name = String::from_utf8_lossy(&entry.name).into_owned();
                Some((serial, (entry.parent, name)))
            })
            .collect::<BTreeMap<_, _>>();
        let wanted = (names.iter())
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
        let mut changes: BTreeMap<String, (&'static str, bool)> = BTreeMap::new();
        for entry in &names {
            let Some(parent) = found.path(entry.parent, 0) else {
                partial = true;
                continue;
            };
            let path = join(&parent, &String::from_utf8_lossy(&entry.name));
            let (change, folder) = match (entry.serial, entry.inherited) {
                (Some(serial), true) => ("modified", kinds.get(&serial) == Some(&true)),
                (Some(serial), false) => ("added", kinds.get(&serial) == Some(&true)),
                (None, true) => {
                    // the base's own entry says whether a folder went
                    let name = PathName::from_bytes(&entry.name).ok();
                    let serial = match name {
                        Some(name) => tree
                            .lookup(reader.as_ref(), entry.parent, &name)
                            .map_err(failed)?,
                        None => None,
                    };
                    let value = match serial {
                        Some(serial) => tree.value(reader.as_ref(), serial).map_err(failed)?,
                        None => None,
                    };
                    let folder = value.is_some_and(|value| value.kind == InodeKind::Directory);
                    ("removed", folder)
                }
                // a name made and removed again changes nothing
                (None, false) => continue,
            };
            changes.insert(path, (change, folder));
        }
        let files = files.into_iter().map(|serial| (serial, false));
        for (serial, folder) in files.chain(modes.into_iter().map(|serial| (serial, true))) {
            match found.path(serial, 0) {
                Some(path) => {
                    changes.entry(path).or_insert(("modified", folder));
                }
                None => partial = true,
            }
        }
        let under = under
            .map(|folder| folder.trim_matches('/'))
            .filter(|folder| !folder.is_empty())
            .map(|folder| format!("/{folder}"));
        let inside = |path: &str| match under.as_deref() {
            None => true,
            Some(folder) => {
                path == folder
                    || path
                        .strip_prefix(folder)
                        .is_some_and(|rest| rest.starts_with('/'))
            }
        };
        let entries = (changes.into_iter())
            .filter(|(path, _)| inside(path))
            .map(|(path, (change, folder))| (path, change, folder))
            .collect();
        Ok(Changed { entries, partial })
    }
}

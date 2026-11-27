//! The lookup references the export holds for a stateless client: one per
//! handed-out serial, resolved again by name after collection, and given back
//! when the serial's last name is gone.
use super::state::{Kernel, Table, DEPTH, NAMES_HELD, RECENT_KEPT};
use super::wait::wait;
use crate::protocol::{attributes::Status, handle::HandleError};
use layerfs_content::{filesystem::PathName, object::inode_leaf::InodeKind};
use layerfs_vfs::{KernelReadOperation, ViewStat};
use std::{collections::HashSet, sync::atomic::Ordering};

impl Kernel {
    /// The serial a client handle names, holding its reference: a collected
    /// one is resolved again by its remembered name.
    pub fn decode(&self, handle: &[u8]) -> Result<u64, Status> {
        let serial = self.handles.decode(handle).map_err(|error| match error {
            HandleError::Bad => Status::BadHandle,
            HandleError::Stale => Status::Stale,
        })?;
        self.held_reference(serial)?;
        Ok(serial)
    }
    /// Ensures the export holds `serial`'s reference, resolving a collected
    /// serial again; marks it used.
    pub(crate) fn held_reference(&self, serial: u64) -> Result<(), Status> {
        if serial == self.root() {
            return Ok(());
        }
        if crate::apple_double::companions::companion(serial) {
            // The export's own companion holds no engine reference.
            return self.companion_stat(serial).map(|_| ());
        }
        {
            let mut table = self.table();
            if table.lookups.contains(&serial) {
                let tick = table.tick();
                table.used.insert(serial, tick);
                return Ok(());
            }
        }
        self.resolve(serial, 0)
    }
    /// One collected serial resolved by the LOOKUP of its remembered name,
    /// after its parent. Another serial, or none, there is a stale handle.
    fn resolve(&self, serial: u64, depth: usize) -> Result<(), Status> {
        let named = self.table().names.get(&serial).cloned();
        let Some((parent, name)) = named else {
            return Err(Status::Stale);
        };
        if depth >= DEPTH {
            return Err(Status::Stale);
        }
        let parent_held = parent == self.root() || self.table().lookups.contains(&parent);
        if !parent_held {
            self.resolve(parent, depth + 1)?;
        }
        let found = PathName::from_bytes(&name)
            .map_err(|_| Status::Stale)
            .and_then(|path| {
                self.observe(
                    parent,
                    None,
                    KernelReadOperation::Lookup { parent, name: path },
                )
                .map_err(|refused| refused.status)
            });
        match found {
            Ok(answer) => {
                self.entry_taken(&answer.stat, parent, &name);
                if answer.stat.serial == serial {
                    self.resolved.fetch_add(1, Ordering::Relaxed);
                    return Ok(());
                }
            }
            Err(Status::NoEnt | Status::Stale | Status::NotDir) => {}
            Err(status) => return Err(status),
        }
        let mut table = self.table();
        let remembered = table
            .names
            .get(&serial)
            .is_some_and(|(p, n)| *p == parent && *n == name);
        if remembered {
            table.unbind_serial(serial);
        }
        self.unresolved.fetch_add(1, Ordering::Relaxed);
        Err(Status::Stale)
    }
    /// A visit or mutation that decided `stat` as the entry `name` of
    /// `parent` took one engine lookup reference on it: the export keeps
    /// exactly one per serial, remembers the name and records a directory's
    /// parent. Beyond the bound, the least recently used are collected.
    pub(crate) fn entry_taken(&self, stat: &ViewStat, parent: u64, name: &[u8]) {
        let (extra, collect) = {
            let mut table = self.table();
            if stat.kind == InodeKind::Directory {
                table.parents.insert(stat.serial, parent);
            }
            let root = stat.serial == self.root();
            if !root {
                let tick = table.tick();
                table.used.insert(stat.serial, tick);
                if table.names.len() < NAMES_HELD || table.names.contains_key(&stat.serial) {
                    let named = (parent, name.to_vec());
                    if let Some(old) = table.names.insert(stat.serial, named.clone()) {
                        if old != named {
                            if stat.namespace_refs <= 1 {
                                // The inode has one name: every other name the
                                // index held for it is one it no longer has.
                                // (Its old keys stay a moment in a state whose
                                // nlink the engine last reported higher.)
                                table.unbind_name(&old, stat.serial);
                                for alias in table.aliases.remove(&stat.serial).unwrap_or_default()
                                {
                                    if table.by_name.get(&alias) == Some(&stat.serial) {
                                        table.by_name.remove(&alias);
                                    }
                                }
                            } else {
                                // A hard link: the old name still reaches the
                                // inode, so it stays, remembered per serial so
                                // that retiring the serial drops no other name.
                                table.remember_alias(stat.serial, &old);
                            }
                            // Held here: the table lock, and inside it the
                            // companion lock as everywhere else. Off unless the
                            // trace is on.
                            self.trace_bind(
                                parent,
                                name,
                                stat.serial,
                                match stat.namespace_refs <= 1 {
                                    true => "unbound",
                                    false => "alias",
                                },
                            );
                        }
                    }
                    table.bind_name(named, stat.serial);
                }
            }
            let extra = root || !table.lookups.insert(stat.serial);
            let collect = if table.lookups.len() > self.lookups_held {
                collectable(&mut table, self.lookups_held - self.lookups_held / 4)
            } else {
                Vec::new()
            };
            (extra, collect)
        };
        if extra {
            self.forget(stat.serial, 1);
        }
        self.collected
            .fetch_add(collect.len() as u64, Ordering::Relaxed);
        for serial in collect {
            self.forget(serial, 1);
        }
        self.observed(stat);
    }
    /// Records that `serial` is now reached as `new_name` in `to` rather than as
    /// `name` in `from` (RENAME). Only the name that moved leaves the index:
    /// other names of the inode, its hard links, stay reachable as they were.
    pub(crate) fn renamed(&self, serial: u64, from: u64, name: &[u8], to: u64, new_name: &[u8]) {
        let mut table = self.table();
        let named = (to, new_name.to_vec());
        let moved = (from, name.to_vec());
        table.unbind_name(&moved, serial);
        let previous = table
            .names
            .get_mut(&serial)
            .map(|slot| std::mem::replace(slot, named.clone()));
        if let Some(previous) = previous {
            // The name the inode was last recalled by did not move and is still
            // one of its names: overwriting the remembered name keeps it only
            // as an alias, which is what retiring the serial reads.
            if previous != moved && previous != named {
                table.remember_alias(serial, &previous);
            }
            table.bind_name(named, serial);
        }
    }
    pub(crate) fn forget(&self, serial: u64, count: u64) {
        if let Ok((services, _)) = self.request() {
            drop(wait(services.forget(self.mount, serial, count)));
        }
    }
    /// Gives back everything the export holds for `serial`, whose last name
    /// is gone: its descriptor, its reference and its parent record. Later
    /// handles of it are stale.
    pub(crate) fn unlinked(&self, serial: u64) {
        self.drop_unlinked(serial, false, None);
    }
    /// Drops table state for `serial` after a mutation that already retired
    /// its kernel lookup reference (and `retired_handle`, if given) in the
    /// publishing transaction.
    pub(crate) fn unlinked_retired(&self, serial: u64, retired_handle: Option<u64>) {
        self.drop_unlinked(serial, true, retired_handle);
    }
    fn drop_unlinked(&self, serial: u64, retired_in_job: bool, retired_handle: Option<u64>) {
        if serial == self.root() {
            return;
        }
        let (file, directories, held) = {
            let mut table = self.table();
            table.parents.remove(&serial);
            table.directory_stats.remove(&serial);
            table.unbind_serial(serial);
            table.used.remove(&serial);
            table.verifiers.retain(|_, entry| entry.1 != serial);
            let directories: Vec<u64> = table
                .directories
                .iter()
                .filter(|(_, entry)| entry.0 == serial)
                .map(|(handle, _)| *handle)
                .collect();
            for handle in &directories {
                table.directories.remove(handle);
            }
            (
                table.files.remove(&serial),
                directories,
                table.lookups.remove(&serial),
            )
        };
        if let Some(file) = file {
            if !retired_in_job || retired_handle != Some(file.handle) {
                self.close_file(serial, file.handle);
            }
        }
        for handle in directories {
            if !retired_in_job || retired_handle != Some(handle) {
                self.close_directory(serial, handle);
            }
        }
        if held && !retired_in_job {
            self.forget(serial, 1);
        }
        // An inode's last name is gone: so is its AppleDouble companion.
        self.companion_drop_inode(serial);
    }
    /// Records the parent of a directory seen in a listing or moved by RENAME.
    pub fn saw_directory(&self, serial: u64, parent: u64) {
        self.table().parents.insert(serial, parent);
    }
}
/// Takes the least recently used references out of the table until `keep`
/// remain, skipping recently used serials, serials that hold a descriptor
/// or a silly name, and
/// directories a held serial was reached through: the engine's own parent
/// index of a held inode never loses its link, so collection proceeds from
/// the leaves. Names and parent records stay for a later resolution.
fn collectable(table: &mut Table, keep: usize) -> Vec<u64> {
    let held_parents = table.lookups.iter().filter_map(|serial| {
        table
            .names
            .get(serial)
            .map(|named| named.0)
            .or_else(|| table.parents.get(serial).copied())
    });
    let busy: HashSet<u64> = table
        .silly
        .values()
        .copied()
        .chain(table.files.keys().copied())
        .chain(table.directories.values().map(|entry| entry.0))
        .chain(held_parents)
        .collect();
    let recent = table.tick.saturating_sub(RECENT_KEPT);
    let mut candidates: Vec<(u64, u64)> = table
        .lookups
        .iter()
        .filter(|serial| !busy.contains(serial))
        .map(|serial| (table.used.get(serial).copied().unwrap_or(0), *serial))
        .filter(|(used, _)| *used < recent)
        .collect();
    let excess = table.lookups.len().saturating_sub(keep);
    candidates.sort_unstable();
    candidates.truncate(excess);
    for (_, serial) in &candidates {
        table.lookups.remove(serial);
        table.used.remove(serial);
    }
    candidates.into_iter().map(|(_, serial)| serial).collect()
}

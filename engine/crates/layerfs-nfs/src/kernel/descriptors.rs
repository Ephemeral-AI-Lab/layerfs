//! The implicit descriptors the export opens for a stateless client: one
//! file descriptor per file and one directory descriptor per enumeration,
//! both bounded, the least recently used closed first.
use super::state::{Held, Kernel, OpenFile, DIRECTORIES_HELD, FILES_HELD};
use crate::kernel::wait::wait;
use crate::protocol::attributes::Status;
use layerfs_vfs::{KernelReadOperation, ViewStat};
use std::collections::HashSet;

impl Kernel {
    /// The facts this export last published or observed for `serial` through
    /// its implicit descriptor, when it holds one and still binds a name for
    /// it. A COMMIT answers from them instead of reading the inode again.
    ///
    /// A serial this export no longer binds a name for answers `None` and is
    /// read again, which then answers what the read answers. That is the case
    /// this export can see: a name another connection removed while this one
    /// kept its own binding still looks live here, so a COMMIT for it answers
    /// the facts last seen and succeeds. What is stale then is at most
    /// `actimeo` old, the reply's post-operation attributes are optional, and
    /// a client that reads, writes or looks the name up then gets the removal.
    pub(crate) fn held_stat(&self, serial: u64) -> Option<ViewStat> {
        let mut table = self.table();
        let tick = table.tick();
        let live = table.names.contains_key(&serial) || table.pinned(serial);
        if !live {
            return None;
        }
        let held = table.files.get_mut(&serial)?;
        held.tick = tick;
        Some(held.stat.clone())
    }
    /// The implicit descriptor of `serial`, opened (or reopened writable)
    /// when needed.
    pub(crate) fn open_file(&self, serial: u64, writable: bool) -> Result<OpenFile, Status> {
        {
            let mut table = self.table();
            let tick = table.tick();
            if let Some(held) = table.files.get_mut(&serial) {
                if held.writable || !writable {
                    held.tick = tick;
                    return Ok(OpenFile {
                        handle: held.handle,
                        size: held.stat.logical_len,
                    });
                }
            }
        }
        let operation = KernelReadOperation::Open { serial, writable };
        let answer = self
            .observe(serial, None, operation)
            .map_err(|refused| refused.status)?;
        let handle = answer.file.ok_or(Status::ServerFault)?;
        Ok(self.file_taken(serial, handle, &answer.stat, writable))
    }
    /// Records an open file descriptor (from `OPEN` or `CREATE`), evicting
    /// the oldest unpinned descriptor when the bound is reached.
    pub(crate) fn file_taken(
        &self,
        serial: u64,
        handle: u64,
        stat: &ViewStat,
        writable: bool,
    ) -> OpenFile {
        let limit = FILES_HELD.min((self.lookups_held / 2).max(1));
        let (evicted, closed, chosen) = {
            let mut table = self.table();
            let tick = table.tick();
            let current = table.files.get(&serial).cloned();
            match current {
                // Another connection opened a good enough one meanwhile.
                Some(theirs) if theirs.writable || !writable => {
                    (None, Some((serial, handle)), theirs)
                }
                _ => {
                    let evicted = if current.is_none() && table.files.len() >= limit {
                        let pinned: HashSet<u64> = table.silly.values().copied().collect();
                        oldest(
                            table
                                .files
                                .iter()
                                .filter(|(serial, _)| !pinned.contains(serial))
                                .map(|(serial, held)| (*serial, held.tick)),
                        )
                        .and_then(|serial| {
                            table
                                .files
                                .remove(&serial)
                                .map(|held| (serial, held.handle))
                        })
                    } else {
                        None
                    };
                    let held = Held {
                        handle,
                        stat: stat.clone(),
                        tick,
                        writable,
                    };
                    let chosen = held.clone();
                    table.files.insert(serial, held);
                    (evicted, current.map(|old| (serial, old.handle)), chosen)
                }
            }
        };
        if let Some((old, descriptor)) = evicted {
            self.close_file(old, descriptor);
        }
        if let Some((old, descriptor)) = closed {
            self.close_file(old, descriptor);
        }
        OpenFile {
            handle: chosen.handle,
            size: chosen.stat.logical_len,
        }
    }
    pub(crate) fn close_file(&self, serial: u64, handle: u64) {
        if let Ok((services, _)) = self.request() {
            drop(wait(services.close_file(self.mount, serial, handle)));
        }
    }
    /// A fresh implicit directory descriptor for an enumeration from cookie 0.
    pub fn open_directory(&self, serial: u64) -> Result<u64, Status> {
        if crate::apple_double::companions::companion(serial) {
            return Err(Status::NotDir);
        }
        let answer = self
            .observe(serial, None, KernelReadOperation::Opendir { serial })
            .map_err(|refused| refused.status)?;
        let handle = answer.directory.ok_or(Status::ServerFault)?;
        // The visit read the directory's own row: remember it, so a LOOKUP in
        // this directory answers its `dir_attributes` from memory.
        self.observed(&answer.stat);
        let evicted = {
            let mut table = self.table();
            let tick = table.tick();
            let evicted = if table.directories.len() >= DIRECTORIES_HELD {
                oldest(
                    table
                        .directories
                        .iter()
                        .map(|(handle, entry)| (*handle, entry.1)),
                )
                .and_then(|handle| {
                    table
                        .directories
                        .remove(&handle)
                        .map(|entry| (entry.0, handle))
                })
            } else {
                None
            };
            table.directories.insert(handle, (serial, tick));
            evicted
        };
        if let Some((old, descriptor)) = evicted {
            self.close_directory(old, descriptor);
        }
        Ok(handle)
    }
    /// The descriptor of a continuing enumeration, if it is still open.
    pub fn directory(&self, serial: u64, handle: u64) -> Option<u64> {
        let mut table = self.table();
        let tick = table.tick();
        let entry = table.directories.get_mut(&handle)?;
        if entry.0 != serial {
            return None;
        }
        entry.1 = tick;
        Some(handle)
    }
    pub(super) fn close_directory(&self, serial: u64, handle: u64) {
        if let Ok((services, _)) = self.request() {
            drop(wait(services.close_directory(self.mount, serial, handle)));
        }
    }
    /// Closes every implicit file descriptor that no silly name needs, so
    /// nothing the export opened is held across a Commit. Returns how many.
    pub fn close_files(&self) -> usize {
        let closing: Vec<(u64, u64)> = {
            let mut table = self.table();
            let pinned: HashSet<u64> = table.silly.values().copied().collect();
            let closing: Vec<(u64, u64)> = table
                .files
                .iter()
                .filter(|(serial, _)| !pinned.contains(serial))
                .map(|(serial, held)| (*serial, held.handle))
                .collect();
            for (serial, _) in &closing {
                table.files.remove(serial);
            }
            closing
        };
        for (serial, handle) in &closing {
            self.close_file(*serial, *handle);
        }
        closing.len()
    }
    /// Closes every enumeration descriptor; a continuing READDIR then
    /// answers `NFS3ERR_BAD_COOKIE` and the client lists again from the
    /// start. Used after a Commit installed another root. Returns how many.
    pub fn close_directories(&self) -> usize {
        let closing: Vec<(u64, u64)> = std::mem::take(&mut self.table().directories)
            .into_iter()
            .map(|(handle, (serial, _))| (serial, handle))
            .collect();
        for (serial, handle) in &closing {
            self.close_directory(*serial, *handle);
        }
        closing.len()
    }
}
fn oldest(entries: impl Iterator<Item = (u64, u64)>) -> Option<u64> {
    entries.min_by_key(|(_, tick)| *tick).map(|(key, _)| key)
}

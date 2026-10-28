//! Exact memory ownership counts; map allocation and process RSS are separate.
use super::OpenState;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OpenStateCounts {
    pub kernel_entries: u64,
    pub directory_parents: u64,
    pub file_handles: u64,
    pub file_reads: u64,
    pub lookup_owners: u64,
    pub file_custody: u64,
    pub orphan_files: u64,
    pub generation_waits: u64,
    pub mounts: u64,
    pub directory_handles: u64,
    pub cookie_pages: u64,
}
impl OpenStateCounts {
    /// Fixed-width values for an original observation, in the declared order.
    pub fn values(self) -> [u64; 11] {
        [
            self.kernel_entries,
            self.directory_parents,
            self.file_handles,
            self.file_reads,
            self.lookup_owners,
            self.file_custody,
            self.orphan_files,
            self.generation_waits,
            self.mounts,
            self.directory_handles,
            self.cookie_pages,
        ]
    }
}
impl OpenState {
    /// Each count is its map's length; the maps roll back with the turn.
    pub fn snapshot(&self) -> OpenStateCounts {
        OpenStateCounts {
            kernel_entries: self.kernel.rows.len() as u64,
            directory_parents: self.parents.rows.len() as u64,
            file_handles: self.files.len() as u64,
            file_reads: self.reads.len() as u64,
            lookup_owners: self.lookups.len() as u64,
            file_custody: self.custody.rows.len() as u64,
            orphan_files: self.orphans.rows.len() as u64,
            generation_waits: self.waits.rows.len() as u64,
            mounts: self.mount.is_some() as u64,
            directory_handles: self.directories.len() as u64,
            cookie_pages: self.cookies.values().map(|s| s.offsets.len() as u64).sum(),
        }
    }
}

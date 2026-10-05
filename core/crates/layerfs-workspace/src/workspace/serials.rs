//! Locally consumed inode serial ranges reserved through the owning allocator.
use crate::{Workspace, WorkspaceError, WorkspaceResult};
use layerfs_content::filesystem::identity::MAXIMUM_INODE_SERIAL;
use layerfs_content::ContentError;

/// Serials requested per refill. A refill window, not a limit on created
/// inodes; unused serials of a reservation are consumed, never recycled.
pub const SERIAL_REFILL: u64 = 1024;

/// The scope's authoritative allocator, such as the history catalog reached
/// through the runtime. Workspace never invents serials: each call returns one
/// consumed half-open range `[start, start + count)` unique in the scope.
pub trait InodeSerials {
    fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)>;
}
/// Unconsumed remainders as `(next, end)`. More than one exists only while
/// concurrent refills overlap; an exhausted range is dropped.
#[derive(Debug, Default)]
pub(crate) struct Serials {
    ranges: Vec<(u64, u64)>,
}
impl Workspace {
    /// One unused serial. Allocation is local while a reserved range lasts;
    /// a refill is one allocator call made outside every Workspace lock.
    pub fn next_serial(&self, allocator: &dyn InodeSerials) -> WorkspaceResult<u64> {
        {
            let mut serials = self
                .serials
                .lock()
                .map_err(|_| WorkspaceError::BindingPoisoned)?;
            if let Some((next, end)) = serials.ranges.last_mut() {
                let serial = *next;
                *next += 1;
                if *next == *end {
                    serials.ranges.pop();
                }
                return Ok(serial);
            }
        }
        let (start, count) = allocator.reserve(SERIAL_REFILL)?;
        let end = start
            .checked_add(count)
            .filter(|end| start != 0 && count != 0 && *end - 1 <= MAXIMUM_INODE_SERIAL)
            .ok_or(ContentError::InvalidRecord("inode serial reservation"))?;
        if start + 1 != end {
            self.serials
                .lock()
                .map_err(|_| WorkspaceError::BindingPoisoned)?
                .ranges
                .push((start + 1, end));
        }
        Ok(start)
    }
}

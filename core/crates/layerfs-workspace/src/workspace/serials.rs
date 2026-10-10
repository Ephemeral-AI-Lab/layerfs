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
/// Unconsumed remainders as `(next, end)`, taken from the last. More than one
/// exists while an early refill waits behind the remainder it was made for,
/// or while concurrent refills overlap; an exhausted range is dropped.
#[derive(Debug, Default)]
pub(crate) struct Serials {
    ranges: Vec<(u64, u64)>,
}
impl Workspace {
    /// One unused serial. Allocation is local while a reserved range lasts;
    /// a refill is one allocator call made outside every Workspace lock.
    pub fn next_serial(&self, allocator: &dyn InodeSerials) -> WorkspaceResult<u64> {
        self.next_serial_with_low_water(allocator, allocator, 0)
            .map(|(serial, _)| serial)
    }
    /// One unused serial, with an early refill below `low_water`. A serial
    /// taken from a local range that leaves fewer than `low_water` unconsumed
    /// serials is followed by one `early` call, outside every Workspace lock.
    /// Its failure is returned beside the serial, which stays consumed, and
    /// nothing is asked again. With no local range the one call is to
    /// `allocator` and no early call is made, so a call never makes two.
    pub fn next_serial_with_low_water(
        &self,
        allocator: &dyn InodeSerials,
        early: &dyn InodeSerials,
        low_water: u64,
    ) -> WorkspaceResult<(u64, Option<WorkspaceError>)> {
        let taken = {
            let mut serials = self
                .serials
                .lock()
                .map_err(|_| WorkspaceError::BindingPoisoned)?;
            match serials.ranges.last_mut() {
                Some((next, end)) => {
                    let serial = *next;
                    *next += 1;
                    if *next == *end {
                        serials.ranges.pop();
                    }
                    let left: u64 = serials.ranges.iter().map(|(next, end)| end - next).sum();
                    Some((serial, left))
                }
                None => None,
            }
        };
        let Some((serial, left)) = taken else {
            let (start, end) = reserve(allocator)?;
            self.keep(start + 1, end, false)?;
            return Ok((start, None));
        };
        if left >= low_water {
            return Ok((serial, None));
        }
        // The new range goes behind the remainder, which is consumed first.
        let refilled = reserve(early).and_then(|(start, end)| self.keep(start, end, true));
        Ok((serial, refilled.err()))
    }
    fn keep(&self, next: u64, end: u64, behind: bool) -> WorkspaceResult<()> {
        if next != end {
            let mut serials = self
                .serials
                .lock()
                .map_err(|_| WorkspaceError::BindingPoisoned)?;
            let at = if behind { 0 } else { serials.ranges.len() };
            serials.ranges.insert(at, (next, end));
        }
        Ok(())
    }
}
/// One allocator call for a refill window, checked, as `[start, end)`.
fn reserve(allocator: &dyn InodeSerials) -> WorkspaceResult<(u64, u64)> {
    let (start, count) = allocator.reserve(SERIAL_REFILL)?;
    let end = start
        .checked_add(count)
        .filter(|end| start != 0 && count != 0 && *end - 1 <= MAXIMUM_INODE_SERIAL)
        .ok_or(ContentError::InvalidRecord("inode serial reservation"))?;
    Ok((start, end))
}

use super::*;

pub(super) fn checked_add(left: u64, right: u64) -> VfsResult<u64> {
    left.checked_add(right).ok_or(VfsError::InvalidState)
}

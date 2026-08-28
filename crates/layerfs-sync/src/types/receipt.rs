use std::fmt;

#[derive(Debug)]
pub enum SyncError {
    Source(String),
    Destination(String),
    ResourceExhausted,
    CounterOverflow,
    SameStorage,
    InvalidResume,
    Progress(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SyncError {}

pub type Result<T> = std::result::Result<T, SyncError>;

pub(crate) fn add(left: u64, right: u64) -> Result<u64> {
    left.checked_add(right).ok_or(SyncError::CounterOverflow)
}

pub(crate) fn add_ns(left: u128, right: u128) -> Result<u128> {
    left.checked_add(right).ok_or(SyncError::CounterOverflow)
}

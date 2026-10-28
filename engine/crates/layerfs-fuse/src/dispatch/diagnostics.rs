//! Actual per-mount received request owners and completed/retained dispositions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MountWork {
    pub active: usize,
    pub retained: usize,
    pub requests: u64,
    pub completed: u64,
    pub terminal: bool,
}

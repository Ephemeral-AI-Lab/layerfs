//! Count actual receive, admitted, runnable, parked and retained ownership.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MountWork {
    pub received: usize,
    pub admitted: usize,
    pub queued: usize,
    pub running: usize,
    pub parked: usize,
    pub retained: usize,
    pub owned_input_bytes: usize,
    pub future_bytes: usize,
    pub receive_units: u64,
    pub completed: u64,
    pub steps: u64,
    pub wakes: u64,
    pub terminal: bool,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DispatchWork {
    pub configured_workers: usize,
    pub entered_workers: usize,
    pub live_workers: usize,
    pub mounts: usize,
    pub queued: usize,
    pub running: usize,
    pub parked: usize,
    pub retained: usize,
    pub stopping: bool,
    pub failed: bool,
}

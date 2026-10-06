//! Exact logical charges of one operation. Not pages, file growth or RSS.

/// Rows and payload bytes one operation holds and has moved.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AcquisitionWork {
    /// Working rows currently held.
    pub held_rows: u64,
    /// Payload bytes those rows currently hold.
    pub held_bytes: u64,
    /// Largest simultaneous held rows.
    pub peak_rows: u64,
    /// Largest simultaneous held payload bytes.
    pub peak_bytes: u64,
    /// Working rows removed so far.
    pub removed_rows: u64,
    /// Payload bytes of the removed rows.
    pub removed_bytes: u64,
}

/// Result of one bounded removal job.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Discarded {
    /// Rows this job removed.
    pub rows: u64,
    /// Payload bytes of those rows.
    pub bytes: u64,
    /// Working rows the operation still holds.
    pub remaining_rows: u64,
}

/// Acknowledged bounded cleanup, including whether the owner was released.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Disposal {
    /// Working rows and bytes removed by this job.
    pub discarded: Discarded,
    /// The empty operation record was removed in this same acknowledgment.
    pub released: bool,
}

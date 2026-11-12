//! The cursor that names a replica state and the receipts of an export and an
//! apply.

/// One replica state: its identity and the change sequence it reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeCursor {
    /// State identity.
    pub state: [u8; 32],
    /// Last change sequence this state includes.
    pub seq: u64,
}

/// What one export read and wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportReceipt {
    /// The cursor the bundle starts from.
    pub before: ChangeCursor,
    /// The cursor the bundle reaches; equal to `before` when nothing changed.
    pub after: ChangeCursor,
    /// Rows written at their current value.
    pub rows: u64,
    /// Keys written as deleted.
    pub deletes: u64,
    /// Change log entries read in the cursor range.
    pub log_entries: u64,
    /// Bundle file length; 0 when nothing changed and no file was written.
    pub bytes: u64,
    /// Digest of the bundle's rows and deletions.
    pub digest: [u8; 32],
}

/// What one apply installed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplyReceipt {
    /// The replica's cursor before the bundle.
    pub before: ChangeCursor,
    /// The replica's cursor after the bundle.
    pub after: ChangeCursor,
    /// Rows installed.
    pub rows: u64,
    /// Keys deleted.
    pub deletes: u64,
}

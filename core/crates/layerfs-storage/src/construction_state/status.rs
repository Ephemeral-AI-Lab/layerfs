//! Fixed public facts for exact private resource and failure custody.

use std::path::PathBuf;

/// Exact native identity captured from the owned descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeIdentity {
    /// Native filesystem device identifier.
    pub device: u64,
    /// Native inode identifier.
    pub inode: u64,
    /// Owning effective Unix user identifier.
    pub uid: u32,
    /// Complete native mode, including file type.
    pub mode: u32,
}

impl NativeIdentity {
    pub(crate) fn encode(self) -> [u8; 24] {
        let mut bytes = [0; 24];
        bytes[..8].copy_from_slice(&self.device.to_be_bytes());
        bytes[8..16].copy_from_slice(&self.inode.to_be_bytes());
        bytes[16..20].copy_from_slice(&self.uid.to_be_bytes());
        bytes[20..24].copy_from_slice(&self.mode.to_be_bytes());
        bytes
    }
}

/// One exact resource lifecycle; Unknown never authorizes destructive cleanup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScratchDisposition {
    /// Slot admitted before dependent file/SQL effects.
    Admitted,
    /// Fresh native owner and private SQL session are usable.
    Open,
    /// Ordered records have a known immutable terminal seal.
    Sealed,
    /// A definite failed operation retains its native owner and credit.
    Failed,
    /// Native or SQL acknowledgement is unproven; access/cleanup is denied.
    Unknown,
    /// One explicit release failed; it is never automatically retried.
    ReleaseFailed,
}

/// Bounded operator snapshot of one live or retained operation slot.
#[derive(Clone, Debug)]
pub struct ScratchOwnerStatus {
    /// Issued compact token, unique in the live C1 issuer.
    pub token: u64,
    /// Complete operation selector, never truncated into the token.
    pub selector: [u8; 32],
    /// Current native/SQL disposition.
    pub disposition: ScratchDisposition,
    /// Exact private database pathname; status does not authorize adoption.
    pub path: PathBuf,
    /// Captured base directory identity.
    pub parent: NativeIdentity,
    /// Captured private directory identity, absent before acknowledgement.
    pub directory: Option<NativeIdentity>,
    /// Captured file identity, absent before acknowledgement.
    pub file: Option<NativeIdentity>,
    /// Declared physical class retained until verified removal.
    pub reserved_bytes: u64,
    /// Last descriptor-observed allocated bytes; unavailable is explicit.
    pub allocated_bytes: Option<u64>,
    /// Original failure text, retained even after the typed error is handed off.
    pub failure: Option<String>,
    /// True when no mutation or destructive cleanup is permitted.
    pub quarantined: bool,
    /// True after the one explicit native release attempt starts.
    pub release_attempted: bool,
    /// Observed descriptor number after an ambiguous native close. It grants no
    /// descriptor authority and must never be reconstructed or closed again.
    pub failed_close_descriptor: Option<i32>,
    /// True when the session has been retained by the authority after drop.
    pub retained: bool,
}

/// Actual accepted private connection settings, not a process heap/RSS proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScratchProfile {
    /// Private LFCS application identity.
    pub application_id: i64,
    /// Private metadata grammar version.
    pub version: i64,
    /// SQLite bytes per page.
    pub page_size: i64,
    /// Negative KiB cache target, selected as -512.
    pub cache_size: i64,
    /// Disabled mmap limit.
    pub mmap_size: i64,
    /// Maximum logical database pages within the16MiB class.
    pub max_page_count: i64,
    /// Accepted synchronous mode, selected as OFF=0.
    pub synchronous: i64,
    /// Accepted temporary storage, selected as MEMORY=2.
    pub temp_store: i64,
    /// Accepted foreign-key enforcement, selected as ON=1.
    pub foreign_keys: i64,
    /// Accepted busy timeout, selected as zero.
    pub busy_timeout: i64,
}

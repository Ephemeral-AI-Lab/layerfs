//! Stable NFS file handles for one kernel mount.
//!
//! A handle is `[version][mount owner][serial][tag]`: 25 bytes, well inside
//! the 64-byte NFSv3 limit. The same serial always encodes to the same bytes
//! for the life of one mount, so a client may compare handles. The mount owner
//! is fresh for every engine mount; a handle from an earlier mount decodes as
//! stale. The tag is a keyed hash under a key drawn once per export, so a
//! local process cannot forge a handle for a serial it was never given. It is
//! not a cryptographic MAC (std SipHash, no third-party dependency); it raises
//! the cost of guessing, it does not authenticate the caller.
use std::{collections::hash_map::RandomState, hash::BuildHasher};

const VERSION: u8 = 1;
/// Encoded length of every handle this export issues.
pub const HANDLE_BYTES: usize = 25;

/// Why a client's handle was not accepted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HandleError {
    /// Not a handle this export ever issued (length, version or tag).
    Bad,
    /// Issued for an earlier mount of this export.
    Stale,
}

/// Encodes and checks the handles of one kernel mount.
pub struct Handles {
    owner: u64,
    key: RandomState,
}
impl Handles {
    pub fn new(owner: u64) -> Self {
        Self {
            owner,
            key: RandomState::new(),
        }
    }
    fn tag(&self, owner: u64, serial: u64) -> u64 {
        self.key.hash_one((VERSION, owner, serial))
    }
    pub fn encode(&self, serial: u64) -> [u8; HANDLE_BYTES] {
        let mut handle = [0; HANDLE_BYTES];
        handle[0] = VERSION;
        handle[1..9].copy_from_slice(&self.owner.to_be_bytes());
        handle[9..17].copy_from_slice(&serial.to_be_bytes());
        handle[17..25].copy_from_slice(&self.tag(self.owner, serial).to_be_bytes());
        handle
    }
    /// The serial a handle names. A handle of another mount is stale even
    /// when its tag cannot be checked, since its key is gone.
    pub fn decode(&self, handle: &[u8]) -> Result<u64, HandleError> {
        if handle.len() != HANDLE_BYTES || handle[0] != VERSION {
            return Err(HandleError::Bad);
        }
        let word = |range: std::ops::Range<usize>| {
            let mut bytes = [0; 8];
            bytes.copy_from_slice(&handle[range]);
            u64::from_be_bytes(bytes)
        };
        let (owner, serial, tag) = (word(1..9), word(9..17), word(17..25));
        if owner != self.owner {
            return Err(HandleError::Stale);
        }
        if serial == 0 || tag != self.tag(owner, serial) {
            return Err(HandleError::Bad);
        }
        Ok(serial)
    }
}

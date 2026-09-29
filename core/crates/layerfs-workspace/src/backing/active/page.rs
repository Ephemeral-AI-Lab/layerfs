//! Authenticated 4,096-byte private pages for the active backing.
use crate::WorkspaceError;
use sha2::{Digest, Sha256};

pub const PAGE_BYTES: usize = 4096;
pub const HEADER_BYTES: usize = 128;
pub const BODY_BYTES: usize = PAGE_BYTES - HEADER_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageRef {
    pub id: u64,
    pub epoch: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Kind {
    Pack = 1,
    IndexLeaf = 2,
    IndexBranch = 3,
    HotDirectory = 4,
}

impl Kind {
    fn magic(self) -> &'static [u8; 8] {
        match self {
            Self::Pack => b"LFSAPAK1",
            Self::IndexLeaf | Self::IndexBranch => b"LFSAIDX2",
            Self::HotDirectory => b"LFSAHOT2",
        }
    }

    fn version(self) -> u16 {
        match self {
            Self::Pack => 1,
            Self::IndexLeaf | Self::IndexBranch | Self::HotDirectory => 2,
        }
    }

    /// Index-forest pages share one locator kind check; the directory is a
    /// selector for targets rather than an indexed node.
    pub fn indexed(self) -> bool {
        matches!(self, Self::IndexLeaf | Self::IndexBranch)
    }
}

pub struct Page {
    pub bytes: [u8; PAGE_BYTES],
}

impl Page {
    pub fn new(
        kind: Kind,
        incarnation: [u8; 32],
        reference: PageRef,
        generation: u64,
        revision: u64,
        records: u16,
        body: &[u8],
    ) -> Result<Self, WorkspaceError> {
        if incarnation == [0; 32]
            || reference.id == 0
            || reference.epoch == 0
            || generation == 0
            || records == 0
            || body.is_empty()
            || body.len() > BODY_BYTES
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let mut bytes = [0; PAGE_BYTES];
        bytes[..8].copy_from_slice(kind.magic());
        bytes[8..10].copy_from_slice(&kind.version().to_be_bytes());
        bytes[10..12].copy_from_slice(&(kind as u16).to_be_bytes());
        bytes[12..44].copy_from_slice(&incarnation);
        bytes[44..52].copy_from_slice(&reference.id.to_be_bytes());
        bytes[52..60].copy_from_slice(&reference.epoch.to_be_bytes());
        bytes[60..68].copy_from_slice(&generation.to_be_bytes());
        bytes[68..76].copy_from_slice(&revision.to_be_bytes());
        bytes[76..78].copy_from_slice(&(body.len() as u16).to_be_bytes());
        bytes[78..80].copy_from_slice(&records.to_be_bytes());
        bytes[HEADER_BYTES..HEADER_BYTES + body.len()].copy_from_slice(body);
        let digest = Sha256::digest(bytes);
        bytes[80..112].copy_from_slice(&digest);
        Ok(Self { bytes })
    }

    pub fn verify(
        &self,
        kind: Kind,
        incarnation: [u8; 32],
        reference: PageRef,
    ) -> Result<&[u8], WorkspaceError> {
        let bytes = &self.bytes;
        let used = u16::from_be_bytes([bytes[76], bytes[77]]) as usize;
        let records = u16::from_be_bytes([bytes[78], bytes[79]]);
        if bytes[..8] != kind.magic()[..]
            || bytes[8..10] != kind.version().to_be_bytes()
            || bytes[10..12] != (kind as u16).to_be_bytes()
            || bytes[12..44] != incarnation
            || bytes[44..52] != reference.id.to_be_bytes()
            || bytes[52..60] != reference.epoch.to_be_bytes()
            || u64::from_be_bytes(bytes[60..68].try_into().map_err(|_| WorkspaceError::Io)?) == 0
            || used == 0
            || used > BODY_BYTES
            || records == 0
            || bytes[112..128].iter().any(|byte| *byte != 0)
            || bytes[HEADER_BYTES + used..].iter().any(|byte| *byte != 0)
        {
            return Err(WorkspaceError::Io);
        }
        let mut digest = Sha256::new();
        digest.update(&bytes[..80]);
        digest.update([0; 32]);
        digest.update(&bytes[112..]);
        if digest.finalize().as_slice() != &bytes[80..112] {
            return Err(WorkspaceError::Io);
        }
        Ok(&bytes[HEADER_BYTES..HEADER_BYTES + used])
    }

    pub fn records(&self) -> u16 {
        u16::from_be_bytes([self.bytes[78], self.bytes[79]])
    }
    pub fn generation(&self) -> u64 {
        u64::from_be_bytes(self.bytes[60..68].try_into().expect("fixed page header"))
    }
    pub fn revision(&self) -> u64 {
        u64::from_be_bytes(self.bytes[68..76].try_into().expect("fixed page header"))
    }
}

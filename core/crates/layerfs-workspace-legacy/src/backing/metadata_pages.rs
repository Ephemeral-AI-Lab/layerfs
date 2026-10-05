//! Immutable metadata page format. Page identities include a checked reuse epoch.
use crate::WorkspaceError;
use sha2::{Digest, Sha256};
pub const PAGE: usize = 4096;
pub const HEADER: usize = 128;
pub const MAX_CELLS: usize = 128;
pub const MIN_BODY: usize = 1024;
/// Declared format of the payload area of one page. A page states its own
/// version, so an older format is refused by its reader instead of being
/// reinterpreted under newer rules.
pub const FORMAT_CELLS: u8 = 0;
pub const FORMAT_PIECES: u8 = 1;
/// Declared structural ceiling of one page tree.
pub const LEVEL_LIMIT: u8 = 7;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageRef {
    pub slot: u32,
    pub epoch: u32,
}
impl PageRef {
    pub const NULL: Self = Self { slot: 0, epoch: 0 };
    pub fn bytes(self) -> [u8; 8] {
        let mut b = [0; 8];
        b[..4].copy_from_slice(&self.slot.to_be_bytes());
        b[4..].copy_from_slice(&self.epoch.to_be_bytes());
        b
    }
    pub fn parse(b: &[u8]) -> Result<Self, WorkspaceError> {
        if b.len() != 8 {
            return Err(WorkspaceError::Io);
        }
        let r = Self {
            slot: u32::from_be_bytes(b[..4].try_into().map_err(|_| WorkspaceError::Io)?),
            epoch: u32::from_be_bytes(b[4..].try_into().map_err(|_| WorkspaceError::Io)?),
        };
        if (r.slot == 0) != (r.epoch == 0) || r.slot > 65536 {
            return Err(WorkspaceError::Io);
        }
        Ok(r)
    }
}
/// The declared kind of one page. A cells page carries keyed metadata cells; a
/// pieces page carries the length-indexed extent sequence of one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    Cells,
    Pieces,
}
impl PageKind {
    fn format(self) -> u8 {
        match self {
            Self::Cells => FORMAT_CELLS,
            Self::Pieces => FORMAT_PIECES,
        }
    }
    fn declare(self, bytes: &mut [u8]) {
        bytes[49] = self.format();
        bytes[54] = u8::from(self == Self::Pieces);
    }
    /// The declared format of one page, without decoding its body.
    pub fn of(bytes: &[u8]) -> Result<Self, WorkspaceError> {
        verify(bytes)?;
        if &bytes[..8] != b"LFSWMTA1" || bytes[55] != 0 {
            return Err(WorkspaceError::Io);
        }
        match (bytes[49], bytes[54]) {
            (FORMAT_CELLS, 0) => Ok(Self::Cells),
            (FORMAT_PIECES, 1) => Ok(Self::Pieces),
            // An unknown version, or a body that does not match the version it
            // declares, is refused here rather than reinterpreted.
            _ => Err(WorkspaceError::Io),
        }
    }
}
pub(crate) fn open_page(
    kind: PageKind,
    level: u8,
    used: usize,
    incarnation: [u8; 32],
    r: PageRef,
) -> Result<[u8; PAGE], WorkspaceError> {
    if used == 0 || used > PAGE - HEADER || level > LEVEL_LIMIT {
        return Err(WorkspaceError::Capacity);
    }
    let mut bytes = [0; PAGE];
    bytes[..8].copy_from_slice(b"LFSWMTA1");
    bytes[8..40].copy_from_slice(&incarnation);
    bytes[40..48].copy_from_slice(&r.bytes());
    bytes[48] = level;
    kind.declare(&mut bytes);
    bytes[56..64].copy_from_slice(&(used as u64).to_be_bytes());
    Ok(bytes)
}
/// The shared declared byte count of one page body.
pub fn body_used(bytes: &[u8]) -> Result<usize, WorkspaceError> {
    let used = usize::try_from(u64::from_be_bytes(
        bytes[56..64].try_into().map_err(|_| WorkspaceError::Io)?,
    ))
    .map_err(|_| WorkspaceError::Io)?;
    if used == 0 || used > PAGE - HEADER || bytes[64..96].iter().any(|x| *x != 0) {
        return Err(WorkspaceError::Io);
    }
    Ok(used)
}
pub fn seal(bytes: &mut [u8]) {
    bytes[96..128].fill(0);
    let hash = Sha256::digest(&*bytes);
    bytes[96..128].copy_from_slice(&hash);
}
pub fn verify(bytes: &[u8]) -> Result<(), WorkspaceError> {
    if bytes.len() != PAGE {
        return Err(WorkspaceError::Io);
    }
    let mut hasher = Sha256::new();
    hasher.update(&bytes[..96]);
    hasher.update([0; 32]);
    hasher.update(&bytes[128..]);
    if hasher.finalize().as_slice() != &bytes[96..128] {
        return Err(WorkspaceError::Io);
    }
    Ok(())
}

// --- Relocated by slice 3.0: the two page-body codecs live in their own
// --- specialization now. These reexports keep every `metadata_pages::<item>`
// --- path in the crate working until nothing imports them.
pub use super::binary_plus_tree::extent::format::{
    decode_pieces_branch, decode_pieces_leaf, encode_pieces_branch, encode_pieces_leaf,
    piece_body_length, ChildRef, PieceRecord, MAX_EXTENT, RECORD,
};
pub use super::binary_plus_tree::keyed::format::{
    decode_cells_raw, dirty_key, entry_key, inode_key, namespace_key, result_key, Cell, PageData,
};

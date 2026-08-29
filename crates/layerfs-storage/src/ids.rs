use crate::{Result, StorageError};
use layerfs_content::ObjectId;
use std::fmt::{self, Write as _};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const BRANCH_TAG: u8 = 0x11;
const COMMIT_TAG: u8 = 0x12;
const STACK_HISTORY_TAG: u8 = 0x21;
const STACK_TAG: u8 = 0x22;
const LAYER_HISTORY_TAG: u8 = 0x31;
const LAYER_TAG: u8 = 0x32;

pub trait StorageId:
    Copy + Eq + Ord + std::hash::Hash + fmt::Debug + Send + Sync + 'static
{
    fn as_slice(&self) -> &[u8];
    fn from_slice(bytes: &[u8]) -> Result<Self>;
}

macro_rules! fixed_id {
    ($name:ident, $len:expr, $tag:expr) => {
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; $len]);

        impl $name {
            pub fn from_bytes(bytes: [u8; $len]) -> Result<Self> {
                if bytes[0] != $tag {
                    return Err(StorageError::Integrity("typed ID tag"));
                }
                Ok(Self(bytes))
            }

            pub const fn to_bytes(self) -> [u8; $len] {
                self.0
            }
        }

        impl StorageId for $name {
            fn as_slice(&self) -> &[u8] {
                &self.0
            }

            fn from_slice(bytes: &[u8]) -> Result<Self> {
                let bytes: [u8; $len] = bytes
                    .try_into()
                    .map_err(|_| StorageError::Integrity("typed ID length"))?;
                Self::from_bytes(bytes)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(stringify!($name))?;
                formatter.write_char('(')?;
                hex(&self.0, formatter)?;
                formatter.write_char(')')
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                hex(&self.0, formatter)
            }
        }

        impl std::str::FromStr for $name {
            type Err = StorageError;

            fn from_str(value: &str) -> Result<Self> {
                if value.len() != $len * 2 {
                    return Err(StorageError::Integrity("typed ID text"));
                }
                let mut bytes = [0; $len];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    bytes[index] = (from_hex(pair[0])? << 4) | from_hex(pair[1])?;
                }
                Self::from_bytes(bytes)
            }
        }
    };
}

fixed_id!(BranchId, 17, BRANCH_TAG);
fixed_id!(CommitId, 33, COMMIT_TAG);
fixed_id!(StackHistoryId, 49, STACK_HISTORY_TAG);
fixed_id!(StackId, 33, STACK_TAG);
fixed_id!(LayerHistoryId, 17, LAYER_HISTORY_TAG);
fixed_id!(LayerId, 33, LAYER_TAG);

impl BranchId {
    pub fn new() -> Self {
        let mut bytes = [0; 17];
        bytes[0] = BRANCH_TAG;
        bytes[1..].copy_from_slice(&uuid_v7());
        Self(bytes)
    }
}

impl Default for BranchId {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerHistoryId {
    pub fn new() -> Self {
        let mut bytes = [0; 17];
        bytes[0] = LAYER_HISTORY_TAG;
        bytes[1..].copy_from_slice(&uuid_v7());
        Self(bytes)
    }
}

impl Default for LayerHistoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl StackHistoryId {
    pub fn new(verification_key: &[u8; 32]) -> Self {
        let mut bytes = [0; 49];
        bytes[0] = STACK_HISTORY_TAG;
        bytes[1..33].copy_from_slice(blake3::hash(verification_key).as_bytes());
        bytes[33..].copy_from_slice(&uuid_v7());
        Self(bytes)
    }

    pub fn verification_key_digest(self) -> [u8; 32] {
        self.0[1..33].try_into().expect("fixed ID")
    }
}

impl CommitId {
    pub fn derive(
        root_id: ObjectId,
        parent_id: Option<Self>,
        merge_parent_id: Option<Self>,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/commit/v1\0");
        hasher.update(root_id.as_bytes());
        update_optional(&mut hasher, parent_id.as_ref().map(StorageId::as_slice));
        update_optional(
            &mut hasher,
            merge_parent_id.as_ref().map(StorageId::as_slice),
        );
        Self(tagged_hash(COMMIT_TAG, hasher.finalize()))
    }
}

impl StackId {
    pub fn derive(history_id: StackHistoryId, parent_id: Option<Self>, root_id: ObjectId) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/stack/v1\0");
        hasher.update(history_id.as_slice());
        update_optional(&mut hasher, parent_id.as_ref().map(StorageId::as_slice));
        hasher.update(root_id.as_bytes());
        Self(tagged_hash(STACK_TAG, hasher.finalize()))
    }
}

impl LayerId {
    pub fn derive(history_id: LayerHistoryId, parent_id: Option<Self>, root_id: ObjectId) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/layer/v1\0");
        hasher.update(history_id.as_slice());
        update_optional(&mut hasher, parent_id.as_ref().map(StorageId::as_slice));
        hasher.update(root_id.as_bytes());
        Self(tagged_hash(LAYER_TAG, hasher.finalize()))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BaseId {
    Layer(LayerId),
    Stack(StackId),
}

impl BaseId {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Layer(id) => id.as_slice(),
            Self::Stack(id) => id.as_slice(),
        }
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        match bytes.first() {
            Some(&LAYER_TAG) => Ok(Self::Layer(LayerId::from_slice(bytes)?)),
            Some(&STACK_TAG) => Ok(Self::Stack(StackId::from_slice(bytes)?)),
            _ => Err(StorageError::Integrity("Branch base tag")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceId {
    Branch(BranchId),
    Stack(StackId),
}

impl SourceId {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Branch(id) => id.as_slice(),
            Self::Stack(id) => id.as_slice(),
        }
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        match bytes.first() {
            Some(&BRANCH_TAG) => Ok(Self::Branch(BranchId::from_slice(bytes)?)),
            Some(&STACK_TAG) => Ok(Self::Stack(StackId::from_slice(bytes)?)),
            _ => Err(StorageError::Integrity("AddResult source tag")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResultId {
    Stack(StackId),
    Layer(LayerId),
}

impl ResultId {
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Stack(id) => id.as_slice(),
            Self::Layer(id) => id.as_slice(),
        }
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        match bytes.first() {
            Some(&STACK_TAG) => Ok(Self::Stack(StackId::from_slice(bytes)?)),
            Some(&LAYER_TAG) => Ok(Self::Layer(LayerId::from_slice(bytes)?)),
            _ => Err(StorageError::Integrity("AddResult result tag")),
        }
    }
}

fn tagged_hash(tag: u8, digest: blake3::Hash) -> [u8; 33] {
    let mut bytes = [0; 33];
    bytes[0] = tag;
    bytes[1..].copy_from_slice(digest.as_bytes());
    bytes
}

fn update_optional(hasher: &mut blake3::Hasher, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(value);
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn uuid_v7() -> [u8; 16] {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let millis = duration.as_millis().min((1_u128 << 48) - 1) as u64;
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/uuidv7/v1\0");
    hasher.update(&duration.as_nanos().to_be_bytes());
    hasher.update(&std::process::id().to_be_bytes());
    hasher.update(&serial.to_be_bytes());
    let random = hasher.finalize();
    let mut bytes = [0; 16];
    bytes[..6].copy_from_slice(&millis.to_be_bytes()[2..]);
    bytes[6..].copy_from_slice(&random.as_bytes()[..10]);
    bytes[6] = (bytes[6] & 0x0f) | 0x70;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    bytes
}

fn hex(bytes: &[u8], formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        formatter.write_char(char::from(HEX[usize::from(byte >> 4)]))?;
        formatter.write_char(char::from(HEX[usize::from(byte & 0x0f)]))?;
    }
    Ok(())
}

fn from_hex(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(StorageError::Integrity("typed ID text")),
    }
}

pub(crate) struct Encoder(pub(crate) Vec<u8>);

impl Encoder {
    pub(crate) const fn new() -> Self {
        Self(Vec::new())
    }
    pub(crate) fn byte(&mut self, value: u8) {
        self.0.push(value);
    }
    pub(crate) fn u32(&mut self, value: usize) {
        self.0.extend_from_slice(&(value as u32).to_be_bytes());
    }
    pub(crate) fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }
    pub(crate) fn raw(&mut self, value: &[u8]) {
        self.0.extend_from_slice(value);
    }
    pub(crate) fn bytes(&mut self, value: &[u8]) {
        self.u32(value.len());
        self.raw(value);
    }
    pub(crate) fn id(&mut self, value: &impl StorageId) {
        self.raw(value.as_slice());
    }
    pub(crate) fn optional_id(&mut self, value: Option<&impl StorageId>) {
        self.byte(u8::from(value.is_some()));
        if let Some(value) = value {
            self.id(value);
        }
    }
}

pub(crate) struct Decoder<'a>(crate::wire::ByteInput<'a>);

impl<'a> Decoder<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self(crate::wire::ByteInput::new(bytes))
    }
    pub(crate) fn byte(&mut self) -> Result<u8> {
        self.0.u8()
    }
    pub(crate) fn u32(&mut self) -> Result<usize> {
        Ok(self.0.u32()? as usize)
    }
    pub(crate) fn u64(&mut self) -> Result<u64> {
        self.0.u64()
    }
    pub(crate) fn raw(&mut self, len: usize) -> Result<&'a [u8]> {
        self.0.take(len)
    }
    pub(crate) fn bytes(&mut self) -> Result<&'a [u8]> {
        let len = self.u32()?;
        self.raw(len)
    }
    pub(crate) fn id<T: StorageId>(&mut self, len: usize) -> Result<T> {
        T::from_slice(self.raw(len)?)
    }
    pub(crate) fn optional_id<T: StorageId>(&mut self, len: usize) -> Result<Option<T>> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.id(len)?)),
            _ => Err(StorageError::Integrity("wire option")),
        }
    }
    pub(crate) fn done(&self) -> bool {
        self.0.done()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_uuid_bits_are_inspectable() {
        let branch = BranchId::new();
        assert_eq!(branch.as_slice()[0], BRANCH_TAG);
        assert_eq!(branch.as_slice()[7] >> 4, 7);
        assert_eq!(branch.as_slice()[9] >> 6, 2);
        assert_eq!(BranchId::from_slice(branch.as_slice()).unwrap(), branch);
        assert!(LayerId::from_slice(branch.as_slice()).is_err());
    }

    #[test]
    fn immutable_ids_are_domain_separated() {
        let root = ObjectId::for_bytes(b"root");
        let history = LayerHistoryId::new();
        let layer = LayerId::derive(history, None, root);
        let commit = CommitId::derive(root, None, None);
        assert_ne!(layer.as_slice(), commit.as_slice());
    }
}

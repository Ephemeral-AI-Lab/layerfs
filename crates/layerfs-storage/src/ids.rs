use crate::{Result, StorageError};
use layerfs_content::ObjectId;
use std::fmt::{self, Write as _};
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const BRANCH_TAG: u8 = 0x11;
const COMMIT_TAG: u8 = 0x12;
const LAYER_STACK_TAG: u8 = 0x31;
const LAYER_TAG: u8 = 0x32;

pub trait StorageId:
    Copy + Eq + Ord + std::hash::Hash + fmt::Debug + Send + Sync + 'static
{
    fn as_slice(&self) -> &[u8];
    fn from_slice(bytes: &[u8]) -> Result<Self>;
}

macro_rules! tagged_id {
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
                Self::from_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| StorageError::Integrity("typed ID length"))?,
                )
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(stringify!($name))?;
                formatter.write_char('(')?;
                write_hex(&self.0, formatter)?;
                formatter.write_char(')')
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_hex(&self.0, formatter)
            }
        }

        impl std::str::FromStr for $name {
            type Err = StorageError;

            fn from_str(value: &str) -> Result<Self> {
                Self::from_bytes(parse_hex(value)?)
            }
        }
    };
}

tagged_id!(BranchId, 17, BRANCH_TAG);
tagged_id!(CommitId, 33, COMMIT_TAG);
tagged_id!(LayerStackId, 17, LAYER_STACK_TAG);
tagged_id!(LayerId, 33, LAYER_TAG);

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StoreId([u8; 32]);

impl StoreId {
    pub fn random() -> Result<Self> {
        let mut bytes = [0; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(Self(bytes))
    }

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl StorageId for StoreId {
    fn as_slice(&self) -> &[u8] {
        &self.0
    }

    fn from_slice(bytes: &[u8]) -> Result<Self> {
        Ok(Self(
            bytes
                .try_into()
                .map_err(|_| StorageError::Integrity("StoreId length"))?,
        ))
    }
}

impl fmt::Debug for StoreId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StoreId(")?;
        write_hex(&self.0, formatter)?;
        formatter.write_char(')')
    }
}

impl fmt::Display for StoreId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(&self.0, formatter)
    }
}

impl std::str::FromStr for StoreId {
    type Err = StorageError;

    fn from_str(value: &str) -> Result<Self> {
        Ok(Self(parse_hex(value)?))
    }
}

impl BranchId {
    pub fn new() -> Self {
        Self(tagged_uuid(BRANCH_TAG))
    }
}

impl Default for BranchId {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerStackId {
    pub fn new() -> Self {
        Self(tagged_uuid(LAYER_STACK_TAG))
    }
}

impl Default for LayerStackId {
    fn default() -> Self {
        Self::new()
    }
}

impl CommitId {
    pub fn derive(root_id: ObjectId, parent_id: Option<Self>, base_layer_id: LayerId) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/commit/v2\0");
        hasher.update(root_id.as_bytes());
        update_optional(&mut hasher, parent_id.as_ref().map(StorageId::as_slice));
        hasher.update(base_layer_id.as_slice());
        Self(tagged_hash(COMMIT_TAG, hasher.finalize()))
    }
}

impl LayerId {
    pub fn derive(
        layer_stack_id: LayerStackId,
        parent_layer_id: Option<Self>,
        root_id: ObjectId,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/layer/v2\0");
        hasher.update(layer_stack_id.as_slice());
        update_optional(
            &mut hasher,
            parent_layer_id.as_ref().map(StorageId::as_slice),
        );
        hasher.update(root_id.as_bytes());
        Self(tagged_hash(LAYER_TAG, hasher.finalize()))
    }
}

fn tagged_hash(tag: u8, digest: blake3::Hash) -> [u8; 33] {
    let mut bytes = [0; 33];
    bytes[0] = tag;
    bytes[1..].copy_from_slice(digest.as_bytes());
    bytes
}

fn update_optional(hasher: &mut blake3::Hasher, value: Option<&[u8]>) {
    if let Some(value) = value {
        hasher.update(&[1]);
        hasher.update(value);
    } else {
        hasher.update(&[0]);
    }
}

fn tagged_uuid(tag: u8) -> [u8; 17] {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let millis = duration.as_millis().min((1_u128 << 48) - 1) as u64;
    let mut random = [0; 10];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .is_err()
    {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/uuidv7/fallback/v2\0");
        hasher.update(&duration.as_nanos().to_be_bytes());
        hasher.update(&std::process::id().to_be_bytes());
        hasher.update(&SERIAL.fetch_add(1, Ordering::Relaxed).to_be_bytes());
        random.copy_from_slice(&hasher.finalize().as_bytes()[..10]);
    }
    let mut bytes = [0; 17];
    bytes[0] = tag;
    bytes[1..7].copy_from_slice(&millis.to_be_bytes()[2..]);
    bytes[7..].copy_from_slice(&random);
    bytes[7] = (bytes[7] & 0x0f) | 0x70;
    bytes[9] = (bytes[9] & 0x3f) | 0x80;
    bytes
}

fn write_hex(bytes: &[u8], formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        formatter.write_char(char::from(HEX[usize::from(byte >> 4)]))?;
        formatter.write_char(char::from(HEX[usize::from(byte & 0x0f)]))?;
    }
    Ok(())
}

fn parse_hex<const N: usize>(value: &str) -> Result<[u8; N]> {
    if value.len() != N * 2 {
        return Err(StorageError::Integrity("typed ID text"));
    }
    let mut bytes = [0; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (from_hex(pair[0])? << 4) | from_hex(pair[1])?;
    }
    Ok(bytes)
}

fn from_hex(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(StorageError::Integrity("typed ID text")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_typed_uuidv7_and_v2_domain_separated() {
        let branch = BranchId::new();
        let stack = LayerStackId::new();
        assert_eq!(branch.as_slice()[7] >> 4, 7);
        assert_eq!(branch.as_slice()[9] >> 6, 2);
        let root = ObjectId::for_bytes(b"root");
        let layer = LayerId::derive(stack, None, root);
        let commit = CommitId::derive(root, None, layer);
        assert_ne!(layer.as_slice(), commit.as_slice());
        assert_eq!(branch.to_string().parse::<BranchId>().unwrap(), branch);
    }
}

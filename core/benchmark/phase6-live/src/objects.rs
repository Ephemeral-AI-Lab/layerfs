use crate::minio::{digest, Minio};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};
use layerfs_storage::{
    encoding::{
        codec::{CompressionWorkspace, DecompressionWorkspace},
        decode_canonical, encode_full, GroupCache,
    },
    pack::{
        assemble::{assemble_consuming, build_group},
        layout::PackLane,
    },
    sqlite::lookup::ObjectLocation,
    StorageCapacities,
};
use std::sync::Arc;

pub const ROLES: [ObjectRole; 13] = [
    ObjectRole::WholeFile,
    ObjectRole::Chunk,
    ObjectRole::ExtentLeaf,
    ObjectRole::ExtentBranch,
    ObjectRole::FileState,
    ObjectRole::InodeLeaf,
    ObjectRole::DirectoryLeaf,
    ObjectRole::DirectoryBranch,
    ObjectRole::InodeBranch,
    ObjectRole::FilesystemRoot,
    ObjectRole::AttributeLeaf,
    ObjectRole::AttributeBranch,
    ObjectRole::Symlink,
];
#[derive(Clone)]
pub struct Locator {
    pub id: ObjectId,
    pub role: ObjectRole,
    pub length: usize,
    pub pack: [u8; 32],
}
pub trait Locators: Send + Sync {
    fn lookup(&self, id: ObjectId) -> Result<Option<Locator>, String>;
    fn register(&self, row: &Locator) -> Result<(), String>;
}
#[derive(Clone)]
pub struct Reader {
    pub s3: Minio,
    pub locators: Arc<dyn Locators>,
}
fn err(message: String) -> ContentError {
    {
        eprintln!("object provider: {message}");
        ContentError::Io
    }
}
pub fn read_locator(s3: &Minio, row: &Locator) -> Result<Vec<u8>, String> {
    let pack = s3.get(&row.pack)?;
    let location = ObjectLocation {
        object_id: row.id,
        role: row.role,
        canonical_length: row.length,
        pack_id: i64::from_be_bytes(row.pack[..8].try_into().unwrap()) & i64::MAX,
        group_number: 0,
        record_number: 0,
    };
    let value = decode_canonical(
        &pack,
        &location,
        &StorageCapacities::from_policy(Default::default()).unwrap(),
        None,
        &mut DecompressionWorkspace::new().map_err(|e| e.to_string())?,
        &mut GroupCache::new(),
        &mut 0,
    )
    .map_err(|e| e.to_string())?;
    if ObjectId::for_bytes(&value) != row.id {
        return Err("canonical identity mismatch".into());
    }
    Ok(value)
}
impl AuthenticatedObjects for Reader {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if ids.len() > 4096 {
            return Err(ContentError::Io);
        }
        ids.iter().map(|id| self.read_canonical(*id)).collect()
    }
    fn read_canonical(&self, id: ObjectId) -> ContentResult<Vec<u8>> {
        let row = self
            .locators
            .lookup(id)
            .map_err(err)?
            .ok_or(ContentError::MissingObject)?;
        read_locator(&self.s3, &row).map_err(err)
    }
}
pub struct Consumer {
    pub reader: Reader,
    compression: CompressionWorkspace,
    pub objects: u64,
    pub bytes: u64,
    pub duplicates: u64,
}
impl Consumer {
    pub fn new(reader: Reader) -> Result<Self, String> {
        Ok(Self {
            reader,
            compression: CompressionWorkspace::new().map_err(|e| e.to_string())?,
            objects: 0,
            bytes: 0,
            duplicates: 0,
        })
    }
}
impl FinalizedConsumer for Consumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let id = object.id();
        if let Some(row) = self.reader.locators.lookup(id).map_err(err)? {
            if row.role != object.role() || self.reader.read_canonical(id)? != object.canonical() {
                return Err(err("exact CAS mismatch".into()));
            }
            self.duplicates += 1;
            return Ok(());
        }
        let (lane, record) = if object.role() == ObjectRole::InodeLeaf {
            // Explicit P6FULL3 physical profile; no pooled or delta claim.
            let mut bytes = vec![0];
            bytes.extend_from_slice(object.canonical());
            (PackLane::Ordinary, bytes)
        } else {
            let value = encode_full(
                object.canonical(),
                object.role(),
                &StorageCapacities::from_policy(Default::default()).unwrap(),
                &mut self.compression,
                &mut Default::default(),
            )
            .map_err(|e| err(e.to_string()))?;
            (value.lane, value.record)
        };
        let group = build_group(lane, &[record], Some(&mut self.compression))
            .map_err(|e| err(e.to_string()))?;
        let pack = assemble_consuming(lane, vec![group]).map_err(|e| err(e.to_string()))?;
        let hash = digest(&pack);
        self.reader.s3.put(&hash, &pack).map_err(err)?;
        self.reader
            .locators
            .register(&Locator {
                id,
                role: object.role(),
                length: object.canonical_len(),
                pack: hash,
            })
            .map_err(err)?;
        self.objects += 1;
        self.bytes += pack.len() as u64;
        Ok(())
    }
}

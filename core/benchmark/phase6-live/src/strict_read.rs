//! Explicit logical-use readers over global SQL metadata and MinIO payloads.
use crate::strict_client::Catalog;
use crate::{
    minio::Minio,
    strict_catalog::{CatalogScope, LogicalUse, PlacementDomain},
};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId, ObjectRole};
use layerfs_storage::{
    access::{ObjectLocation, ValueGroupRow},
    encoding::{
        delta::read::{BodyCaches, ChainCounters, Resolver},
        pool::PoolReader,
        DecompressionWorkspace, GroupCache,
    },
    PackAccess, StorageCapacities, StorageError, StorageResult,
};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

pub fn placement(logical_use: LogicalUse, role: ObjectRole) -> PlacementDomain {
    if logical_use == LogicalUse::RegularFileGraph
        && matches!(role, ObjectRole::WholeFile | ObjectRole::Chunk)
    {
        PlacementDomain::FilePayload
    } else {
        PlacementDomain::Metadata
    }
}

/// The scope is captured before construction or decoding. No backend probing.
pub struct Access<'a> {
    pub catalog: &'a dyn Catalog,
    pub provider: &'a Minio,
    pub scope: CatalogScope,
    pub logical_use: LogicalUse,
}
fn storage_error(message: String) -> StorageError {
    eprintln!("strict storage access: {message}");
    StorageError::Integrity("strict catalog/provider access")
}
impl PackAccess for Access<'_> {
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        let Some((role, _)) = self.catalog.descriptor(id).map_err(storage_error)? else {
            return Ok(None);
        };
        let mut scope = self.scope;
        scope.ceiling = scope.ceiling.min(ceiling);
        let domain = placement(self.logical_use, role);
        let selected = self
            .catalog
            .location(scope, domain, id)
            .map_err(storage_error)?;
        if selected.is_some()
            && !self
                .catalog
                .has_use(scope, id, self.logical_use)
                .map_err(storage_error)?
        {
            return Err(StorageError::Integrity("required logical use"));
        }
        Ok(selected.map(|row| row.location))
    }
    fn pack_bytes(&self, order: i64) -> StorageResult<Vec<u8>> {
        let domain = self
            .catalog
            .body_domain(self.scope, order)
            .map_err(storage_error)?;
        if self.logical_use == LogicalUse::MetadataGraph && domain != PlacementDomain::Metadata {
            return Err(StorageError::Integrity("metadata payload body refusal"));
        }
        let (digest, bytes) = self
            .catalog
            .body(self.scope, domain, order)
            .map_err(storage_error)?;
        match (domain, bytes) {
            (PlacementDomain::Metadata, Some(bytes)) => {
                if crate::minio::digest(&bytes) != digest {
                    return Err(StorageError::Integrity("SQL metadata body digest"));
                }
                Ok(bytes)
            }
            (PlacementDomain::FilePayload, None) => {
                self.provider.get(&digest).map_err(storage_error)
            }
            _ => Err(StorageError::Integrity("domain body shape")),
        }
    }
    fn group_for(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
        self.catalog
            .group_for(self.scope, ordinal)
            .map_err(storage_error)
    }
    fn metadata_window_start(&self) -> StorageResult<u32> {
        self.catalog
            .metadata_window_start(self.scope)
            .map_err(storage_error)
    }
    fn group_page(&self, from: u32, limit: usize) -> StorageResult<Vec<ValueGroupRow>> {
        self.catalog
            .group_page(self.scope, from, limit)
            .map_err(storage_error)
    }
}

/// One serialized owner shares bounded physical caches between both read orders.
pub struct ReadOwner {
    namespace: Option<u64>,
    packs: BTreeMap<i64, Vec<u8>>,
    pool: PoolReader,
    decoder: DecompressionWorkspace,
    pub counters: ChainCounters,
}
impl ReadOwner {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            namespace: None,
            packs: BTreeMap::new(),
            pool: PoolReader::new(),
            decoder: DecompressionWorkspace::new().map_err(|e| e.to_string())?,
            counters: Default::default(),
        })
    }
    pub fn read(
        &mut self,
        access: &Access<'_>,
        capacities: &StorageCapacities,
        ids: &[ObjectId],
    ) -> Result<Vec<Vec<u8>>, String> {
        if ids.len() > 4096 {
            return Err("strict read object window".into());
        }
        // Preflight every required placement before any canonical/body cache can answer.
        let mut total = 0usize;
        for id in ids {
            let row = access
                .location(*id, access.scope.ceiling)
                .map_err(|e| e.to_string())?
                .ok_or("required domain placement missing")?;
            total = total
                .checked_add(row.canonical_length)
                .ok_or("read byte overflow")?;
            if total > crate::read_window::BYTES {
                return Err("strict read byte window".into());
            }
        }
        let namespace = access.catalog.cache_namespace();
        if self.namespace != Some(namespace) {
            self.packs.clear();
            self.pool = PoolReader::new();
            self.namespace = Some(namespace);
        }
        let mut out = Vec::with_capacity(ids.len());
        let mut groups = GroupCache::new();
        for id in ids {
            let mut chain = ChainCounters::default();
            let mut resolver = Resolver::new(
                access,
                access.scope.ceiling,
                capacities,
                BodyCaches {
                    packs: &mut self.packs,
                    pool: &mut self.pool,
                },
                &mut groups,
                &mut self.decoder,
                &mut chain,
            );
            out.push(resolver.resolve(*id).map_err(|e| e.to_string())?.0);
            layerfs_storage::encoding::delta::read::accumulate(&mut self.counters, chain);
        }
        Ok(out)
    }
}

pub struct Reader {
    pub catalog: Arc<dyn Catalog>,
    pub provider: Minio,
    pub scope: CatalogScope,
    pub logical_use: LogicalUse,
    pub owner: std::rc::Rc<RefCell<ReadOwner>>,
    pub capacities: StorageCapacities,
}
impl AuthenticatedObjects for Reader {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let access = Access {
            catalog: self.catalog.as_ref(),
            provider: &self.provider,
            scope: self.scope,
            logical_use: self.logical_use,
        };
        self.owner
            .borrow_mut()
            .read(&access, &self.capacities, ids)
            .map_err(|message| {
                eprintln!("strict reader: {message}");
                ContentError::Io
            })
    }
}

/// A thread-safe immutable captured view for FUSE/retained-file access. Each
/// operation owns and releases its decode/body caches; no daemon-lifetime payload
/// cache survives a completed operation or doubles the construction owner's arena.
#[derive(Clone)]
pub struct WaveReader {
    pub catalog: Arc<dyn Catalog>,
    pub provider: Minio,
    pub scope: CatalogScope,
    pub logical_use: LogicalUse,
    pub capacities: StorageCapacities,
}
impl AuthenticatedObjects for WaveReader {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let access = Access {
            catalog: self.catalog.as_ref(),
            provider: &self.provider,
            scope: self.scope,
            logical_use: self.logical_use,
        };
        ReadOwner::new()
            .and_then(|mut owner| owner.read(&access, &self.capacities, ids))
            .map_err(|message| {
                eprintln!("strict captured wave: {message}");
                ContentError::Io
            })
    }
}

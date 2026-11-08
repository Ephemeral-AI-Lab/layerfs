//! Existing Storage reconstruction over borrowed snapshot mechanics.
use super::{transaction::View, Options};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_storage::{
    encoding::{
        delta::read::{BodyCaches, ChainCounters, Resolver},
        pool::PoolReader,
        DecompressionWorkspace, GroupCache, PackCache,
    },
    location::{ObjectLocation, SignatureRow},
    source::Source,
    StorageCapacities, StorageError, StorageResult,
};
use std::cell::RefCell;

pub(super) struct Input<'a> {
    pub view: &'a dyn View,
    pub options: Options<'a>,
}

impl Source for Input<'_> {
    fn ordered_dependencies(&self) -> bool {
        false
    }
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        self.options
            .check()
            .map_err(|_| StorageError::Integrity("diagnostic deadline"))?;
        let mut found = Vec::new();
        self.view.locate(&[id], &mut found)?;
        Ok(found
            .into_iter()
            .find(|r| r.location.object_id == id && r.location.pack_id <= ceiling)
            .map(|r| r.location))
    }
    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>> {
        self.options
            .check()
            .map_err(|_| StorageError::Integrity("diagnostic deadline"))?;
        let mut found = Vec::new();
        self.view.read_packs(&[pack_id], &mut found)?;
        let pack = found
            .pop()
            .ok_or(StorageError::Integrity("missing diagnostic pack"))?;
        if !found.is_empty() || pack.info().pack_id != pack_id {
            return Err(StorageError::Integrity("diagnostic pack binding"));
        }
        Ok(pack.into_parts().1)
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
    ) -> StorageResult<Vec<[u8; layerfs_content::inode_leaf::INODE_VALUE_BYTES]>> {
        let mut rows = Vec::new();
        self.view.inode_values(ordinals, &mut rows)?;
        ordinals
            .iter()
            .map(|ordinal| {
                rows.iter()
                    .find(|r| r.ordinal == *ordinal)
                    .map(|r| r.value)
                    .ok_or(StorageError::Integrity("missing diagnostic ordinal"))
            })
            .collect()
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        let mut out = Vec::new();
        self.view.signatures(&mut out)?;
        Ok(out)
    }
    fn write_signatures(&self, _: &[SignatureRow]) -> StorageResult<usize> {
        Err(StorageError::Integrity("read-only diagnostic"))
    }
}

pub(super) struct Reader<'a> {
    input: Input<'a>,
    ceiling: i64,
    capacities: StorageCapacities,
    state: RefCell<State>,
}
struct State {
    packs: PackCache,
    pool: PoolReader,
    groups: GroupCache,
    workspace: DecompressionWorkspace,
}
impl<'a> Reader<'a> {
    pub fn new(view: &'a dyn View, ceiling: i64, options: Options<'a>) -> StorageResult<Self> {
        let policy = view.policy()?;
        Ok(Self {
            input: Input { view, options },
            ceiling,
            capacities: StorageCapacities::from_policy(policy)?,
            state: RefCell::new(State {
                packs: PackCache::new(),
                pool: PoolReader::new(),
                groups: GroupCache::new(),
                workspace: DecompressionWorkspace::new()?,
            }),
        })
    }
    pub fn read(&self, id: ObjectId) -> StorageResult<Vec<u8>> {
        let State {
            packs,
            pool,
            groups,
            workspace,
        } = &mut *self.state.borrow_mut();
        let mut counters = ChainCounters::default();
        let mut resolver = Resolver::new(
            &self.input,
            self.ceiling,
            &self.capacities,
            BodyCaches { packs, pool },
            groups,
            workspace,
            &mut counters,
        );
        resolver.resolve(id).map(|(bytes, _)| bytes)
    }
}
impl AuthenticatedObjects for Reader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| {
                self.read(*id).map_err(|_| ContentError::ProviderFailure {
                    what: "diagnostic authenticated read",
                })
            })
            .collect()
    }
}

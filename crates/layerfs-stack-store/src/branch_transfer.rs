use crate::StackStore;
use layerfs_content::ObjectId;
use layerfs_storage::{
    closest_common_layer, closest_common_stack, BaseId, BaseSnapshot, BranchId, BranchRecord,
    CanonicalObject, CommitId, Fact, FactKind, LayerHistoryId, LayerHistoryRecord, LayerId,
    LayerRecord, ObjectSource, Result, StackHistoryId, StackHistoryRecord, StackId, StackRecord,
    StorageError, StoreEndpoint, TransferExchange, TransferIntent, TransferOutcome, TransferTarget,
};

pub(crate) type Membership<'a> =
    dyn FnMut(FactKind, &[Vec<u8>]) -> Result<layerfs_storage::MissingBitmap> + 'a;

impl ObjectSource for StackStore {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.db.read_object_row(id)
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        self.db.read_object_rows(ids)
    }

    fn visit_objects(
        &self,
        ids: &[ObjectId],
        visitor: &mut dyn FnMut(CanonicalObject) -> Result<()>,
    ) -> Result<()> {
        self.db.visit_object_rows(ids, visitor)
    }
}

impl StoreEndpoint for StackStore {
    fn store_identity(&self) -> Result<[u8; 32]> {
        Ok(self.db.identity())
    }

    fn inventory_page(
        &self,
        after: Option<ObjectId>,
        limit: u16,
    ) -> Result<layerfs_storage::InventoryPage> {
        self.db.inventory_page(after, limit)
    }

    fn storage_snapshot(&self) -> Result<layerfs_storage::StoreStorageSnapshot> {
        self.db.storage_snapshot()
    }

    fn begin_transfer(&self) -> Result<Box<dyn TransferTarget + '_>> {
        Ok(Box::new(StackTransfer {
            store: self,
            _permit: self.db.enter_operation()?,
        }))
    }

    fn transfer_exchange_unlocked(
        &self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
    ) -> Result<TransferExchange> {
        self.db
            .transfer_exchange(objects, facts, object_ids, fact_ids, false)
    }

    fn base_snapshot(&self, base_id: BaseId) -> Result<BaseSnapshot> {
        match base_id {
            BaseId::Layer(id) => {
                let layer = self.db.layer(id)?.ok_or(StorageError::NotFound("Layer"))?;
                Ok(BaseSnapshot {
                    base_id,
                    layer_history_id: layer.history_id,
                    root_id: layer.root_id,
                })
            }
            BaseId::Stack(id) => {
                let stack = self.db.stack(id)?.ok_or(StorageError::NotFound("Stack"))?;
                let history = self
                    .db
                    .stack_history(stack.history_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                let layer = self
                    .db
                    .layer(history.base_layer_id)?
                    .ok_or(StorageError::MissingBaseData)?;
                Ok(BaseSnapshot {
                    base_id,
                    layer_history_id: layer.history_id,
                    root_id: stack.root_id,
                })
            }
        }
    }

    fn common_base(&self, left: BaseId, right: BaseId) -> Result<BaseSnapshot> {
        let left_snapshot = self.base_snapshot(left)?;
        let right_snapshot = self.base_snapshot(right)?;
        if left_snapshot.layer_history_id != right_snapshot.layer_history_id {
            return Err(StorageError::NoCommonBase);
        }
        if let (BaseId::Stack(left_id), BaseId::Stack(right_id)) = (left, right) {
            let left_stack = self
                .db
                .stack(left_id)?
                .ok_or(StorageError::MissingBaseData)?;
            let right_stack = self
                .db
                .stack(right_id)?
                .ok_or(StorageError::MissingBaseData)?;
            if left_stack.history_id == right_stack.history_id {
                if let Some(id) = closest_common_stack(&self.db, left_id, right_id)? {
                    let stack = self.db.stack(id)?.ok_or(StorageError::MissingBaseData)?;
                    return Ok(BaseSnapshot {
                        base_id: BaseId::Stack(id),
                        layer_history_id: left_snapshot.layer_history_id,
                        root_id: stack.root_id,
                    });
                }
            }
        }
        let left_layer = base_layer_id(self, left)?;
        let right_layer = base_layer_id(self, right)?;
        let id = closest_common_layer(&self.db, left_layer, right_layer)?
            .ok_or(StorageError::NoCommonBase)?;
        let layer = self.db.layer(id)?.ok_or(StorageError::MissingBaseData)?;
        Ok(BaseSnapshot {
            base_id: BaseId::Layer(id),
            layer_history_id: layer.history_id,
            root_id: layer.root_id,
        })
    }

    fn branch_record(&self, branch_id: BranchId) -> Result<BranchRecord> {
        match self.db.branch(branch_id)? {
            Some(branch) => Ok(branch),
            None => self.parent.branch_record(branch_id),
        }
    }

    fn visit_commits(
        &self,
        branch_id: BranchId,
        membership: &mut Membership<'_>,
        visitor: &mut dyn FnMut(&[layerfs_storage::CommitRecord]) -> Result<()>,
    ) -> Result<()> {
        crate::commit_pull::visit_commits(self, branch_id, membership, visitor)
    }

    fn layer_history_record(&self, history_id: LayerHistoryId) -> Result<LayerHistoryRecord> {
        self.db
            .layer_history(history_id)?
            .ok_or(StorageError::NotFound("LayerHistory"))
    }

    fn visit_layers(
        &self,
        history_id: LayerHistoryId,
        through: LayerId,
        membership: &mut dyn FnMut(FactKind, &[Vec<u8>]) -> Result<layerfs_storage::MissingBitmap>,
        visitor: &mut dyn FnMut(&[LayerRecord]) -> Result<()>,
    ) -> Result<()> {
        self.db.visit_layers(history_id, through, &mut |page| {
            missing_page(
                FactKind::Layer,
                page,
                |row| row.id.to_bytes(),
                membership,
                visitor,
            )
        })
    }

    fn stack_history_record(&self, history_id: StackHistoryId) -> Result<StackHistoryRecord> {
        self.db
            .stack_history(history_id)?
            .ok_or(StorageError::NotFound("StackHistory"))
    }

    fn stack_record(&self, stack_id: StackId) -> Result<StackRecord> {
        match self.db.stack(stack_id)? {
            Some(stack) => Ok(stack),
            None => self.parent.stack_record(stack_id),
        }
    }

    fn visit_stacks(
        &self,
        history_id: StackHistoryId,
        through: StackId,
        membership: &mut dyn FnMut(FactKind, &[Vec<u8>]) -> Result<layerfs_storage::MissingBitmap>,
        visitor: &mut dyn FnMut(&[StackRecord]) -> Result<()>,
    ) -> Result<()> {
        self.db.visit_stacks(history_id, through, &mut |page| {
            missing_page(
                FactKind::Stack,
                page,
                |row| row.id.to_bytes(),
                membership,
                visitor,
            )
        })
    }

    fn add_stack(
        &self,
        stack_history_id: StackHistoryId,
        branch_id: BranchId,
        commit_id: CommitId,
    ) -> Result<layerfs_storage::AddResult<StackId>> {
        self.add_stack_to_history(stack_history_id, branch_id, commit_id)
    }
}

pub(crate) fn missing_page<T: Copy, const N: usize>(
    kind: FactKind,
    page: &[T],
    id: impl Fn(T) -> [u8; N],
    membership: &mut Membership<'_>,
    visitor: &mut dyn FnMut(&[T]) -> Result<()>,
) -> Result<()> {
    let mut ids = page
        .iter()
        .copied()
        .map(|row| id(row).to_vec())
        .collect::<Vec<_>>();
    ids.sort();
    let missing = membership(kind, &ids)?;
    missing.validate_tail(ids.len())?;
    let mut selected_ids = std::collections::BTreeSet::new();
    for (index, value) in ids.into_iter().enumerate() {
        if missing.is_missing(index)? {
            selected_ids.insert(value);
        }
    }
    let selected = page
        .iter()
        .copied()
        .filter(|row| selected_ids.contains(id(*row).as_slice()))
        .collect::<Vec<_>>();
    visitor(&selected)
}

struct StackTransfer<'a> {
    store: &'a StackStore,
    _permit: layerfs_storage::OperationPermit<'a>,
}

impl TransferTarget for StackTransfer<'_> {
    fn preflight_branch(
        &mut self,
        branch: BranchRecord,
        root: ObjectId,
    ) -> Result<(Option<CommitId>, bool, layerfs_storage::MissingBitmap)> {
        let (current, up_to_date) = self.store.db.preflight_branch_push(branch)?;
        Ok((current, up_to_date, self.store.db.missing_objects(&[root])?))
    }

    fn exchange(
        &mut self,
        objects: &[CanonicalObject],
        facts: &[Fact],
        object_ids: &[ObjectId],
        fact_ids: Option<(FactKind, &[Vec<u8>])>,
    ) -> Result<TransferExchange> {
        self.store
            .transfer_exchange_unlocked(objects, facts, object_ids, fact_ids)
    }

    fn finish(
        self: Box<Self>,
        objects: &[CanonicalObject],
        facts: &[Fact],
        intent: TransferIntent,
    ) -> Result<(TransferExchange, TransferOutcome)> {
        if matches!(intent, TransferIntent::Stack(_)) {
            return Err(StorageError::WrongSourceRoute);
        }
        self.store.db.finish_transfer_local(objects, facts, intent)
    }
}

fn base_layer_id(store: &StackStore, base: BaseId) -> Result<LayerId> {
    match base {
        BaseId::Layer(id) => Ok(id),
        BaseId::Stack(id) => {
            let stack = store.db.stack(id)?.ok_or(StorageError::MissingBaseData)?;
            Ok(store
                .db
                .stack_history(stack.history_id)?
                .ok_or(StorageError::MissingBaseData)?
                .base_layer_id)
        }
    }
}

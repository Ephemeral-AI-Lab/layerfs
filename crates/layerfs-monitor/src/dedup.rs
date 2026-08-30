use crate::{MonitorError, MonitorResult};
use layerfs_branch_store::BranchStore;
use layerfs_content::ObjectId;
use layerfs_layerstack_store::LayerStackStore;
use std::collections::VecDeque;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementAnalysis {
    pub role: String,
    pub object_count: u64,
    pub encoded_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DedupAnalysis {
    pub physical_cas_bytes: u64,
    pub union_cas_bytes: u64,
    pub cross_store_placement_bytes: u64,
    pub placement_factor: f64,
    pub placements: Vec<PlacementAnalysis>,
    pub local_cas: ExactOrUnavailable<LocalCasAnalysis>,
    pub transfer: ExactOrUnavailable<TransferAnalysis>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExactOrUnavailable<T> {
    Exact(T),
    Unavailable(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalCasAnalysis {
    pub candidate_bytes: u64,
    pub inserted_bytes: u64,
    pub reused_bytes: u64,
    pub saved_fraction: f64,
    pub logical_to_physical: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransferAnalysis {
    pub announced_bytes: u64,
    pub sent_bytes: u64,
    pub avoided_bytes: u64,
    pub avoided_fraction: f64,
}

pub(crate) fn analyze(
    layerstack: &LayerStackStore,
    branch: &BranchStore,
    operations: &[crate::OperationReceipt],
) -> MonitorResult<DedupAnalysis> {
    let mut cursors = [
        InventoryCursor::new("layerstack", layerstack),
        InventoryCursor::new("branch", branch),
    ];
    let mut union_cas_bytes = 0_u64;
    loop {
        let next = cursors
            .iter_mut()
            .filter_map(|cursor| cursor.peek().transpose())
            .collect::<MonitorResult<Vec<_>>>()?
            .into_iter()
            .map(|entry| entry.object_id)
            .min();
        let Some(next) = next else { break };
        let mut length = None;
        for cursor in &mut cursors {
            if cursor.peek()?.is_some_and(|entry| entry.object_id == next) {
                let entry = cursor.pop()?.expect("peeked inventory entry");
                if length.is_some_and(|known| known != entry.encoded_length) {
                    return Err(MonitorError::Integrity("Object length"));
                }
                length = Some(entry.encoded_length);
            }
        }
        union_cas_bytes = union_cas_bytes
            .checked_add(length.expect("selected inventory entry"))
            .ok_or(MonitorError::Integrity("unique bytes"))?;
    }
    let placements = cursors
        .into_iter()
        .map(|cursor| PlacementAnalysis {
            role: cursor.role.to_owned(),
            object_count: cursor.object_count,
            encoded_bytes: cursor.encoded_bytes,
        })
        .collect::<Vec<_>>();
    let physical_cas_bytes = placements
        .iter()
        .try_fold(0_u64, |total, placement| {
            total.checked_add(placement.encoded_bytes)
        })
        .ok_or(MonitorError::Integrity("physical bytes"))?;
    Ok(DedupAnalysis {
        physical_cas_bytes,
        union_cas_bytes,
        cross_store_placement_bytes: physical_cas_bytes.saturating_sub(union_cas_bytes),
        placement_factor: if union_cas_bytes == 0 {
            1.0
        } else {
            physical_cas_bytes as f64 / union_cas_bytes as f64
        },
        placements,
        local_cas: local_cas(operations)?,
        transfer: transfer(operations)?,
    })
}

fn local_cas(
    operations: &[crate::OperationReceipt],
) -> MonitorResult<ExactOrUnavailable<LocalCasAnalysis>> {
    let mut candidate = 0_u64;
    let mut inserted = 0_u64;
    let mut reused = 0_u64;
    for receipt in operations
        .iter()
        .flat_map(|operation| &operation.storage)
        .filter_map(|receipt| match receipt {
            layerfs_storage::StorageReceipt::Local(receipt) => Some(receipt),
            layerfs_storage::StorageReceipt::Transfer(_)
            | layerfs_storage::StorageReceipt::Durability(_)
            | layerfs_storage::StorageReceipt::WorkspaceCommit(_)
            | layerfs_storage::StorageReceipt::Push(_)
            | layerfs_storage::StorageReceipt::Database(_)
            | layerfs_storage::StorageReceipt::WorkspaceLifecycle(_) => None,
        })
    {
        receipt.validate()?;
        candidate = candidate
            .checked_add(receipt.objects.candidate_bytes)
            .ok_or(MonitorError::Integrity("local candidate bytes"))?;
        inserted = inserted
            .checked_add(receipt.objects.inserted_bytes)
            .ok_or(MonitorError::Integrity("local inserted bytes"))?;
        reused = reused
            .checked_add(receipt.objects.reused_bytes)
            .ok_or(MonitorError::Integrity("local reused bytes"))?;
    }
    if candidate == 0 {
        return Ok(ExactOrUnavailable::Unavailable(
            "no measured local candidate byte denominator",
        ));
    }
    if candidate != inserted + reused {
        return Err(MonitorError::Integrity("local CAS byte equation"));
    }
    Ok(ExactOrUnavailable::Exact(LocalCasAnalysis {
        candidate_bytes: candidate,
        inserted_bytes: inserted,
        reused_bytes: reused,
        saved_fraction: reused as f64 / candidate as f64,
        logical_to_physical: if inserted == 0 {
            f64::INFINITY
        } else {
            candidate as f64 / inserted as f64
        },
    }))
}

fn transfer(
    operations: &[crate::OperationReceipt],
) -> MonitorResult<ExactOrUnavailable<TransferAnalysis>> {
    let mut announced = 0_u64;
    let mut sent = 0_u64;
    let mut any = false;
    for set in operations
        .iter()
        .flat_map(|operation| &operation.storage)
        .filter_map(|receipt| match receipt {
            layerfs_storage::StorageReceipt::Transfer(receipt) => Some(receipt),
            layerfs_storage::StorageReceipt::Local(_)
            | layerfs_storage::StorageReceipt::Durability(_)
            | layerfs_storage::StorageReceipt::WorkspaceCommit(_)
            | layerfs_storage::StorageReceipt::Push(_)
            | layerfs_storage::StorageReceipt::Database(_)
            | layerfs_storage::StorageReceipt::WorkspaceLifecycle(_) => None,
        })
        .flat_map(|receipt| std::iter::once(&receipt.objects).chain(receipt.facts.values()))
    {
        set.validate()?;
        if set.announced_ids == 0 {
            continue;
        }
        let Some(bytes) = set.announced_bytes.exact() else {
            return Ok(ExactOrUnavailable::Unavailable(
                "announced object bytes were pruned without passive reads",
            ));
        };
        announced = announced
            .checked_add(bytes)
            .ok_or(MonitorError::Integrity("transfer announced bytes"))?;
        sent = sent
            .checked_add(set.sent_bytes)
            .ok_or(MonitorError::Integrity("transfer sent bytes"))?;
        any = true;
    }
    if !any || announced == 0 {
        return Ok(ExactOrUnavailable::Unavailable(
            "no measured transfer byte denominator",
        ));
    }
    if sent > announced {
        return Err(MonitorError::Integrity("transfer byte equation"));
    }
    let avoided = announced - sent;
    Ok(ExactOrUnavailable::Exact(TransferAnalysis {
        announced_bytes: announced,
        sent_bytes: sent,
        avoided_bytes: avoided,
        avoided_fraction: avoided as f64 / announced as f64,
    }))
}

trait InventorySource {
    fn page(
        &self,
        after: Option<ObjectId>,
    ) -> layerfs_storage::Result<layerfs_storage::InventoryPage>;
}

impl InventorySource for BranchStore {
    fn page(
        &self,
        after: Option<ObjectId>,
    ) -> layerfs_storage::Result<layerfs_storage::InventoryPage> {
        self.inventory_page(after, 512)
    }
}

impl InventorySource for LayerStackStore {
    fn page(
        &self,
        after: Option<ObjectId>,
    ) -> layerfs_storage::Result<layerfs_storage::InventoryPage> {
        self.inventory_page(after, 512)
    }
}

struct InventoryCursor<'a> {
    role: &'a str,
    store: &'a dyn InventorySource,
    entries: VecDeque<layerfs_storage::InventoryEntry>,
    after: Option<ObjectId>,
    complete: bool,
    object_count: u64,
    encoded_bytes: u64,
}

impl<'a> InventoryCursor<'a> {
    fn new(role: &'a str, store: &'a dyn InventorySource) -> Self {
        Self {
            role,
            store,
            entries: VecDeque::new(),
            after: None,
            complete: false,
            object_count: 0,
            encoded_bytes: 0,
        }
    }

    fn peek(&mut self) -> MonitorResult<Option<layerfs_storage::InventoryEntry>> {
        self.refill()?;
        Ok(self.entries.front().copied())
    }

    fn pop(&mut self) -> MonitorResult<Option<layerfs_storage::InventoryEntry>> {
        self.refill()?;
        let entry = self.entries.pop_front();
        if let Some(entry) = entry {
            self.object_count += 1;
            self.encoded_bytes = self
                .encoded_bytes
                .checked_add(entry.encoded_length)
                .ok_or(MonitorError::Integrity("placement bytes"))?;
        }
        Ok(entry)
    }

    fn refill(&mut self) -> MonitorResult<()> {
        if !self.entries.is_empty() || self.complete {
            return Ok(());
        }
        let page = self.store.page(self.after)?;
        if page.entries.is_empty() && page.continuation.is_some() {
            return Err(MonitorError::Integrity("empty inventory page"));
        }
        self.entries = page.entries.into();
        self.after = page.continuation;
        self.complete = self.after.is_none();
        Ok(())
    }
}

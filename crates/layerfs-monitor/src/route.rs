use crate::{MonitorError, MonitorResult, PlacementAnalysis};
use layerfs_branch_store::BranchStore;
use layerfs_content::ObjectId;
use layerfs_layer_store::LayerStore;
use layerfs_stack_store::StackStore;
use layerfs_storage::StoreEndpoint;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BranchStoreId([u8; 16]);

impl fmt::Display for BranchStoreId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("r:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for BranchStoreId {
    type Err = MonitorError;

    fn from_str(value: &str) -> MonitorResult<Self> {
        Ok(Self(parse_id(value, "r:")?))
    }
}

#[derive(Clone)]
pub struct MonitoredRoute {
    pub id: BranchStoreId,
    pub branch: BranchStore,
    pub stack: Option<Arc<StackStore>>,
    pub layer: Arc<LayerStore>,
}

impl MonitoredRoute {
    pub fn new(
        branch: BranchStore,
        stack: Option<Arc<StackStore>>,
        layer: Arc<LayerStore>,
    ) -> Self {
        let digest = ObjectId::for_bytes(branch.path().as_os_str().as_encoded_bytes()).to_bytes();
        Self {
            id: BranchStoreId(digest[..16].try_into().expect("fixed route id")),
            branch,
            stack,
            layer,
        }
    }

    pub(crate) fn inventories(&self) -> MonitorResult<(u64, Vec<PlacementAnalysis>)> {
        let mut stores: Vec<(&str, &dyn InventorySource)> = vec![("branch", &self.branch)];
        if let Some(stack) = &self.stack {
            stores.push(("stack", stack.as_ref()));
        }
        stores.push(("layer", self.layer.as_ref()));
        let mut cursors = stores
            .into_iter()
            .map(|(role, store)| InventoryCursor::new(role, store))
            .collect::<Vec<_>>();
        let mut unique_bytes = 0_u64;
        loop {
            let mut next = None;
            for cursor in &mut cursors {
                if let Some(entry) = cursor.peek()? {
                    next = Some(
                        next.map_or(entry.object_id, |next: ObjectId| next.min(entry.object_id)),
                    );
                }
            }
            let Some(next) = next else { break };
            let mut length = None;
            for cursor in &mut cursors {
                if cursor.peek()?.is_some_and(|entry| entry.object_id == next) {
                    let entry = cursor.pop()?.expect("peeked inventory entry");
                    if length.is_some_and(|length| length != entry.encoded_length) {
                        return Err(MonitorError::Integrity("Object length"));
                    }
                    length = Some(entry.encoded_length);
                }
            }
            unique_bytes = unique_bytes
                .checked_add(length.expect("selected inventory entry"))
                .ok_or(MonitorError::Integrity("unique bytes"))?;
        }
        Ok((
            unique_bytes,
            cursors
                .into_iter()
                .map(|cursor| PlacementAnalysis {
                    role: cursor.role.to_owned(),
                    object_count: cursor.object_count,
                    encoded_bytes: cursor.encoded_bytes,
                })
                .collect(),
        ))
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

impl InventorySource for StackStore {
    fn page(
        &self,
        after: Option<ObjectId>,
    ) -> layerfs_storage::Result<layerfs_storage::InventoryPage> {
        self.inventory_page(after, 512)
    }
}

impl InventorySource for LayerStore {
    fn page(
        &self,
        after: Option<ObjectId>,
    ) -> layerfs_storage::Result<layerfs_storage::InventoryPage> {
        self.inventory_page(after, 512)
    }
}

pub(crate) fn parse_id(value: &str, prefix: &str) -> MonitorResult<[u8; 16]> {
    let value = value.strip_prefix(prefix).ok_or(MonitorError::NotFound)?;
    if value.len() != 32 {
        return Err(MonitorError::NotFound);
    }
    let mut bytes = [0; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex(pair[0])? << 4) | hex(pair[1])?;
    }
    Ok(bytes)
}

fn hex(value: u8) -> MonitorResult<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(MonitorError::NotFound),
    }
}

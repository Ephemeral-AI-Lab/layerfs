use crate::{
    BranchHead, BranchId, Direction, DurableControlEndpoint, DurableEndpoint, LayerStackHead,
    RequestId, Result, SyncError, MAX_BATCH_OBJECTS,
};
use layerfs_core::ObjectId;
use layerfs_working_store::WorkingStore;
use std::time::Instant;

pub(crate) trait Source {
    fn storage_id(&self) -> [u8; 32];
    fn read(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>>;
}

pub(crate) trait Destination {
    fn storage_id(&self) -> [u8; 32];
    fn contains(&self, id: ObjectId) -> Result<bool>;
    fn accept(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()>;
}

impl Source for WorkingStore {
    fn storage_id(&self) -> [u8; 32] {
        self.storage_id()
    }

    fn read(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>> {
        self.sync_read_object(id, maximum)
            .map_err(|error| SyncError::Source(error.to_string()))
    }
}

impl Destination for WorkingStore {
    fn storage_id(&self) -> [u8; 32] {
        self.storage_id()
    }

    fn contains(&self, id: ObjectId) -> Result<bool> {
        self.sync_has_object(id)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()> {
        self.sync_accept_objects(
            owner_request_id,
            request_id,
            direction_name(direction),
            objects,
        )
        .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

pub(crate) struct EndpointSource<'a, T>(pub(crate) &'a T);

impl<T: DurableEndpoint> Source for EndpointSource<'_, T> {
    fn storage_id(&self) -> [u8; 32] {
        self.0.durable_storage_id()
    }

    fn read(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>> {
        self.0.read_object(id, maximum)
    }
}

pub(crate) struct EndpointDestination<'a, T>(pub(crate) &'a T);

impl<T: DurableEndpoint> Destination for EndpointDestination<'_, T> {
    fn storage_id(&self) -> [u8; 32] {
        self.0.durable_storage_id()
    }

    fn contains(&self, id: ObjectId) -> Result<bool> {
        self.0.contains_object(id)
    }

    fn accept(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()> {
        self.0
            .accept_objects(owner_request_id, request_id, direction, objects)
    }
}

const fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Fetch => "fetch",
        Direction::Push => "push",
    }
}

pub(crate) struct WorkingObjectPages<'a> {
    source: &'a WorkingStore,
    branch_id: BranchId,
    base: Option<BranchHead>,
    after: Option<ObjectId>,
    page: std::vec::IntoIter<ObjectId>,
    done: bool,
    pub(crate) error: Option<SyncError>,
    pub(crate) traversal_ns: u128,
}

impl<'a> WorkingObjectPages<'a> {
    pub(crate) fn new(
        source: &'a WorkingStore,
        branch_id: BranchId,
        base: Option<BranchHead>,
    ) -> Self {
        Self {
            source,
            branch_id,
            base,
            after: None,
            page: Vec::new().into_iter(),
            done: false,
            error: None,
            traversal_ns: 0,
        }
    }
}

impl Iterator for WorkingObjectPages<'_> {
    type Item = ObjectId;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(id) = self.page.next() {
                return Some(id);
            }
            if self.done || self.error.is_some() {
                return None;
            }
            let traversal = Instant::now();
            let page = self.source.branch_push_object_page(
                self.branch_id,
                self.base,
                self.after,
                MAX_BATCH_OBJECTS,
            );
            self.traversal_ns =
                match crate::types::add_ns(self.traversal_ns, traversal.elapsed().as_nanos()) {
                    Ok(total) => total,
                    Err(error) => {
                        self.error = Some(error);
                        return None;
                    }
                };
            let page = match page {
                Ok(page) => page,
                Err(error) => {
                    self.error = Some(SyncError::Source(error.to_string()));
                    return None;
                }
            };
            if page.is_empty() {
                self.done = true;
                return None;
            }
            if !valid_page(&page, self.after) {
                self.error = Some(SyncError::Source("invalid Working closure page".into()));
                return None;
            }
            self.after = page.last().copied();
            self.page = page.into_iter();
        }
    }
}

pub(crate) struct BranchObjectPages<'a, T> {
    source: &'a T,
    branch_id: BranchId,
    base: Option<BranchHead>,
    origin_stack_base: Option<LayerStackHead>,
    expected_head: BranchHead,
    expected_stack_head: LayerStackHead,
    after: Option<ObjectId>,
    pub(crate) page: std::vec::IntoIter<ObjectId>,
    done: bool,
    pub(crate) error: Option<SyncError>,
    pub(crate) traversal_ns: u128,
}

impl<'a, T: DurableControlEndpoint> BranchObjectPages<'a, T> {
    pub(crate) fn new(
        source: &'a T,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
    ) -> Self {
        Self {
            source,
            branch_id,
            base,
            origin_stack_base,
            expected_head,
            expected_stack_head,
            after: None,
            page: Vec::new().into_iter(),
            done: false,
            error: None,
            traversal_ns: 0,
        }
    }
}

impl<T: DurableControlEndpoint> Iterator for BranchObjectPages<'_, T> {
    type Item = ObjectId;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(id) = self.page.next() {
                return Some(id);
            }
            if self.done || self.error.is_some() {
                return None;
            }
            let traversal = Instant::now();
            let page = self.source.branch_fetch_object_page(
                self.branch_id,
                self.base,
                self.origin_stack_base,
                self.expected_head,
                self.expected_stack_head,
                self.after,
                MAX_BATCH_OBJECTS,
            );
            self.traversal_ns =
                match crate::types::add_ns(self.traversal_ns, traversal.elapsed().as_nanos()) {
                    Ok(total) => total,
                    Err(error) => {
                        self.error = Some(error);
                        return None;
                    }
                };
            let page = match page {
                Ok(page) => page,
                Err(error) => {
                    self.error = Some(error);
                    return None;
                }
            };
            if page.is_empty() {
                self.done = true;
                return None;
            }
            if !valid_page(&page, self.after) {
                self.error = Some(SyncError::Source("invalid Durable closure page".into()));
                return None;
            }
            self.after = page.last().copied();
            self.page = page.into_iter();
        }
    }
}

fn valid_page(page: &[ObjectId], after: Option<ObjectId>) -> bool {
    page.len() <= MAX_BATCH_OBJECTS
        && page
            .iter()
            .scan(after, |previous, id| {
                let ordered = previous.is_none_or(|previous| *id > previous);
                *previous = Some(*id);
                Some(ordered)
            })
            .all(|ordered| ordered)
}

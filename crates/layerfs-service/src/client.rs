use crate::protocol::request::{WireEnvelopeRef, WireRequest};
use crate::protocol::response::WireResponse;
use crate::transport::framing::{read_frame, write_frame};
use crate::{Result, ServiceError, MIN_BEARER_BYTES};
use layerfs_core::ObjectId;
use layerfs_durable_store::{
    BranchHead, BranchId, LayerCandidate, LayerId, LayerStackHead, LayerStackId,
    LayerStackMergeOutcome, LayerStackRollbackOutcome,
};
use layerfs_sync::{
    BranchPushBundle, BranchPushOutcome, BranchPushRequest, BranchRollbackOutcome,
    BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication, Direction,
    DurableControlEndpoint, DurableEndpoint, RequestId, SyncError, SyncTransferCounters,
};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

#[derive(Clone)]
pub struct RemoteEndpoint {
    address: SocketAddr,
    bearer: Vec<u8>,
    storage_id: [u8; 32],
}

impl RemoteEndpoint {
    pub fn connect(address: SocketAddr, bearer: &[u8]) -> Result<Self> {
        if !address.ip().is_loopback() || bearer.len() < MIN_BEARER_BYTES {
            return Err(ServiceError::InvalidConfiguration);
        }
        let endpoint = Self {
            address,
            bearer: bearer.to_vec(),
            storage_id: [0; 32],
        };
        match endpoint.call(WireRequest::StorageId)? {
            WireResponse::StorageId(storage_id) => Ok(Self {
                storage_id,
                ..endpoint
            }),
            _ => Err(ServiceError::Wire("storage identity response".into())),
        }
    }

    fn call(&self, request: WireRequest) -> Result<WireResponse> {
        let mut stream = TcpStream::connect_timeout(&self.address, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        write_frame(
            &mut stream,
            &WireEnvelopeRef {
                bearer: &self.bearer,
                expected_storage_id: (self.storage_id != [0; 32]).then_some(self.storage_id),
                request: &request,
            },
        )?;
        match read_frame::<WireResponse>(&mut stream)? {
            WireResponse::Error(error) => Err(ServiceError::Wire(error)),
            response => Ok(response),
        }
    }

    pub fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> Result<LayerStackHead> {
        match self.call(WireRequest::BootstrapLayerStack(
            stack,
            layer,
            name.to_owned(),
            root,
        ))? {
            WireResponse::LayerStackHead(head) => Ok(head),
            _ => Err(ServiceError::Wire("LayerStack bootstrap response".into())),
        }
    }

    fn sync_call(&self, request: WireRequest, source: bool) -> layerfs_sync::Result<WireResponse> {
        self.call(request).map_err(|error| {
            if source {
                SyncError::Source(error.to_string())
            } else {
                SyncError::Destination(error.to_string())
            }
        })
    }
}

impl DurableEndpoint for RemoteEndpoint {
    fn durable_storage_id(&self) -> [u8; 32] {
        self.storage_id
    }

    fn read_object(&self, id: ObjectId, maximum: usize) -> layerfs_sync::Result<Vec<u8>> {
        if maximum > layerfs_sync::MAX_BATCH_BYTES {
            return Err(SyncError::ResourceExhausted);
        }
        match self.sync_call(WireRequest::ReadObject(id, maximum), true)? {
            WireResponse::Object(bytes) => Ok(bytes),
            _ => Err(SyncError::Source("object response".into())),
        }
    }

    fn contains_object(&self, id: ObjectId) -> layerfs_sync::Result<bool> {
        match self.sync_call(WireRequest::ContainsObject(id), false)? {
            WireResponse::Bool(present) => Ok(present),
            _ => Err(SyncError::Destination("contains response".into())),
        }
    }

    fn accept_objects(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> layerfs_sync::Result<()> {
        for (id, canonical) in objects {
            match self.sync_call(
                WireRequest::AcceptObjects(
                    owner_request_id,
                    request_id,
                    direction,
                    vec![(*id, canonical.clone())],
                ),
                false,
            )? {
                WireResponse::Unit => {}
                _ => return Err(SyncError::Destination("object admission response".into())),
            }
        }
        Ok(())
    }

    fn abort_transfer(
        &self,
        owner_request_id: RequestId,
        direction: Direction,
    ) -> layerfs_sync::Result<u64> {
        match self.sync_call(
            WireRequest::AbortTransfer(owner_request_id, direction),
            false,
        )? {
            WireResponse::Count(rows) => Ok(rows),
            _ => Err(SyncError::Destination("transfer abort response".into())),
        }
    }
}

impl DurableControlEndpoint for RemoteEndpoint {
    fn branch_head(&self, branch_id: BranchId) -> layerfs_sync::Result<Option<BranchHead>> {
        match self.sync_call(WireRequest::BranchHead(branch_id), false)? {
            WireResponse::BranchHead(head) => Ok(head),
            _ => Err(SyncError::Source("Branch head response".into())),
        }
    }

    fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> layerfs_sync::Result<LayerStackHead> {
        RemoteEndpoint::bootstrap_layer_stack(self, stack, layer, name, root)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> layerfs_sync::Result<()> {
        match self.sync_call(
            WireRequest::StageBranchPush(
                transfer_id,
                page_sequence,
                data_request_id,
                bundle.clone(),
                counters,
            ),
            false,
        )? {
            WireResponse::Unit => Ok(()),
            _ => Err(SyncError::Destination("Branch Push stage response".into())),
        }
    }

    fn commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        match self.sync_call(WireRequest::CommitBranchPush(request, branch_id), false)? {
            WireResponse::BranchPush(outcome) => Ok(outcome),
            _ => Err(SyncError::Destination("Branch Push commit response".into())),
        }
    }

    fn reconcile_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        match self.sync_call(WireRequest::ReconcileBranchPush(request, accepted), false)? {
            WireResponse::BranchPush(outcome) => Ok(outcome),
            _ => Err(SyncError::Destination(
                "Branch Push reconciliation response".into(),
            )),
        }
    }

    fn record_replayed_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        match self.sync_call(
            WireRequest::RecordReplayedBranchPush(request, accepted),
            false,
        )? {
            WireResponse::BranchPush(outcome) => Ok(outcome),
            _ => Err(SyncError::Destination("Branch Push replay response".into())),
        }
    }

    fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> layerfs_sync::Result<BranchPushBundle> {
        match self.sync_call(
            WireRequest::ExportBranchFetch(branch_id, base, origin_stack_base),
            true,
        )? {
            WireResponse::BranchBundle(bundle) => Ok(bundle),
            _ => Err(SyncError::Source("Branch Fetch response".into())),
        }
    }

    fn branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<ObjectId>,
        limit: usize,
    ) -> layerfs_sync::Result<Vec<ObjectId>> {
        match self.sync_call(
            WireRequest::BranchFetchObjectPage(
                branch_id,
                base,
                origin_stack_base,
                expected_head,
                expected_stack_head,
                after,
                limit,
            ),
            true,
        )? {
            WireResponse::ObjectPage(page) => Ok(page),
            _ => Err(SyncError::Source("closure page response".into())),
        }
    }

    fn accept_child_branch_merge(
        &self,
        publication: ChildMergePublication,
    ) -> layerfs_sync::Result<ChildMergeOutcome> {
        match self.sync_call(WireRequest::AcceptChildBranchMerge(publication), false)? {
            WireResponse::ChildMerge(outcome) => Ok(outcome),
            _ => Err(SyncError::Destination("child merge response".into())),
        }
    }

    fn accept_branch_rollback(
        &self,
        publication: BranchRollbackPublication,
    ) -> layerfs_sync::Result<BranchRollbackOutcome> {
        match self.sync_call(WireRequest::AcceptBranchRollback(publication), false)? {
            WireResponse::BranchRollback(outcome) => Ok(outcome),
            _ => Err(SyncError::Destination("Branch rollback response".into())),
        }
    }

    fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> layerfs_sync::Result<LayerStackMergeOutcome> {
        match self.sync_call(
            WireRequest::AcceptLayerStackMerge(candidate, expected),
            false,
        )? {
            WireResponse::LayerStackMerge(outcome) => Ok(outcome),
            _ => Err(SyncError::Destination("LayerStack merge response".into())),
        }
    }

    fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> layerfs_sync::Result<LayerStackRollbackOutcome> {
        match self.sync_call(WireRequest::LayerStackRollback(expected, target), false)? {
            WireResponse::LayerStackRollback(outcome) => Ok(outcome),
            _ => Err(SyncError::Destination(
                "LayerStack rollback response".into(),
            )),
        }
    }
}

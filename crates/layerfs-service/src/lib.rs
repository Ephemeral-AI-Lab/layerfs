//! Authenticated owner of one DurableStore and its Sync server handlers.

#![forbid(unsafe_code)]

use layerfs_core::ObjectId;
use layerfs_durable_store::{
    BranchHead, BranchId, DurableError, DurableStore, LayerCandidate, LayerId, LayerStackHead,
    LayerStackId, LayerStackMergeOutcome, LayerStackRollbackOutcome,
};
use layerfs_sync::server::LocalDurable;
use layerfs_sync::{
    BranchPushBundle, BranchPushOutcome, BranchPushRequest, BranchRollbackOutcome,
    BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication, DurableControlEndpoint,
    DurableEndpoint, RequestId, SyncError, SyncTransferCounters,
};
use std::fmt;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::time::Duration;

pub const COMPONENT: &str = "layerfs-service";
pub const MIN_BEARER_BYTES: usize = 32;
pub const MAX_WIRE_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub enum ServiceError {
    Authentication,
    StorageIdentity,
    InvalidConfiguration,
    Durable(DurableError),
    Sync(SyncError),
    Io(std::io::Error),
    Wire(String),
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ServiceError {}

impl From<DurableError> for ServiceError {
    fn from(value: DurableError) -> Self {
        Self::Durable(value)
    }
}

impl From<SyncError> for ServiceError {
    fn from(value: SyncError) -> Self {
        Self::Sync(value)
    }
}

impl From<std::io::Error> for ServiceError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;

pub struct Service {
    durable: DurableStore,
    bearer_digest: [u8; 32],
}

impl Service {
    pub fn open(root: &Path, bearer: &[u8]) -> Result<Self> {
        if bearer.len() < MIN_BEARER_BYTES {
            return Err(ServiceError::InvalidConfiguration);
        }
        Ok(Self {
            durable: DurableStore::open(root)?,
            bearer_digest: *blake3::hash(bearer).as_bytes(),
        })
    }

    pub fn authenticate<'a>(&'a self, bearer: &[u8]) -> Result<AuthenticatedSession<'a>> {
        let presented = *blake3::hash(bearer).as_bytes();
        let difference = self
            .bearer_digest
            .iter()
            .zip(presented)
            .fold(0_u8, |difference, (expected, actual)| {
                difference | (expected ^ actual)
            });
        if difference != 0 {
            return Err(ServiceError::Authentication);
        }
        Ok(AuthenticatedSession { service: self })
    }

    pub fn compact(self) -> Result<Self> {
        Ok(Self {
            durable: self.durable.compact()?,
            bearer_digest: self.bearer_digest,
        })
    }
}

pub struct AuthenticatedSession<'a> {
    service: &'a Service,
}

impl AuthenticatedSession<'_> {
    pub fn storage_id(&self) -> [u8; 32] {
        self.service.durable.storage_id()
    }

    pub fn branch_head(&self, id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.service.durable.branch_head(id)?)
    }

    pub fn layer_stack_head(&self, id: LayerStackId) -> Result<Option<LayerStackHead>> {
        Ok(self.service.durable.layer_stack_head(id)?)
    }

    pub fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> Result<LayerStackHead> {
        Ok(self
            .service
            .durable
            .bootstrap_layer_stack(stack, layer, name, root)?)
    }

    pub fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome> {
        Ok(self
            .service
            .durable
            .accept_layer_stack_merge(candidate, expected)?)
    }

    pub fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome> {
        Ok(self
            .service
            .durable
            .layer_stack_rollback(expected, target)?)
    }
}

impl DurableEndpoint for AuthenticatedSession<'_> {
    fn durable_storage_id(&self) -> [u8; 32] {
        self.storage_id()
    }

    fn read_object(&self, id: ObjectId, maximum: usize) -> layerfs_sync::Result<Vec<u8>> {
        LocalDurable::new(&self.service.durable).read_object(id, maximum)
    }

    fn contains_object(&self, id: ObjectId) -> layerfs_sync::Result<bool> {
        LocalDurable::new(&self.service.durable).contains_object(id)
    }

    fn accept_objects(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: layerfs_sync::Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> layerfs_sync::Result<()> {
        LocalDurable::new(&self.service.durable).accept_objects(
            owner_request_id,
            request_id,
            direction,
            objects,
        )
    }

    fn abort_transfer(
        &self,
        owner_request_id: RequestId,
        direction: layerfs_sync::Direction,
    ) -> layerfs_sync::Result<u64> {
        LocalDurable::new(&self.service.durable).abort_transfer(owner_request_id, direction)
    }
}

impl DurableControlEndpoint for AuthenticatedSession<'_> {
    fn branch_head(&self, branch_id: BranchId) -> layerfs_sync::Result<Option<BranchHead>> {
        self.service
            .durable
            .branch_head(branch_id)
            .map_err(|error| SyncError::Source(error.to_string()))
    }
    fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> layerfs_sync::Result<LayerStackHead> {
        AuthenticatedSession::bootstrap_layer_stack(self, stack, layer, name, root)
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
        self.service
            .durable
            .stage_branch_push_page(
                transfer_id,
                page_sequence,
                data_request_id,
                bundle,
                counters,
            )
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        self.service
            .durable
            .commit_staged_branch_push(request, branch_id)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
    fn reconcile_branch_push(
        &self,
        request_id: RequestId,
        expected: Option<BranchHead>,
        accepted: BranchHead,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        self.service
            .durable
            .reconcile_branch_push(request_id, expected, accepted)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
    fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> layerfs_sync::Result<BranchPushBundle> {
        self.service
            .durable
            .export_branch_fetch(branch_id, base, origin_stack_base)
            .map_err(|error| SyncError::Source(error.to_string()))
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
        self.service
            .durable
            .branch_fetch_object_page(
                branch_id,
                base,
                origin_stack_base,
                expected_head,
                expected_stack_head,
                after,
                limit,
            )
            .map_err(|error| SyncError::Source(error.to_string()))
    }

    fn accept_child_branch_merge(
        &self,
        publication: ChildMergePublication,
    ) -> layerfs_sync::Result<ChildMergeOutcome> {
        self.service
            .durable
            .accept_child_branch_merge(publication)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_branch_rollback(
        &self,
        publication: BranchRollbackPublication,
    ) -> layerfs_sync::Result<BranchRollbackOutcome> {
        self.service
            .durable
            .accept_branch_rollback(publication)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> layerfs_sync::Result<LayerStackMergeOutcome> {
        AuthenticatedSession::accept_layer_stack_merge(self, candidate, expected)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> layerfs_sync::Result<LayerStackRollbackOutcome> {
        AuthenticatedSession::layer_stack_rollback(self, expected, target)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

#[derive(serde::Deserialize)]
struct WireEnvelope {
    bearer: Vec<u8>,
    expected_storage_id: Option<[u8; 32]>,
    request: WireRequest,
}

#[derive(serde::Serialize)]
struct WireEnvelopeRef<'a> {
    bearer: &'a [u8],
    expected_storage_id: Option<[u8; 32]>,
    request: &'a WireRequest,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[allow(clippy::large_enum_variant)]
enum WireRequest {
    StorageId,
    BranchHead(BranchId),
    BootstrapLayerStack(LayerStackId, LayerId, String, ObjectId),
    ReadObject(ObjectId, usize),
    ContainsObject(ObjectId),
    AcceptObjects(
        RequestId,
        RequestId,
        layerfs_sync::Direction,
        Vec<(ObjectId, Vec<u8>)>,
    ),
    AbortTransfer(RequestId, layerfs_sync::Direction),
    StageBranchPush(
        RequestId,
        u64,
        RequestId,
        BranchPushBundle,
        SyncTransferCounters,
    ),
    CommitBranchPush(BranchPushRequest, BranchId),
    ReconcileBranchPush(RequestId, Option<BranchHead>, BranchHead),
    ExportBranchFetch(BranchId, Option<BranchHead>, Option<LayerStackHead>),
    BranchFetchObjectPage(
        BranchId,
        Option<BranchHead>,
        Option<LayerStackHead>,
        BranchHead,
        LayerStackHead,
        Option<ObjectId>,
        usize,
    ),
    AcceptChildBranchMerge(ChildMergePublication),
    AcceptBranchRollback(BranchRollbackPublication),
    AcceptLayerStackMerge(LayerCandidate, LayerStackHead),
    LayerStackRollback(LayerStackHead, LayerId),
}

#[derive(serde::Deserialize, serde::Serialize)]
#[allow(clippy::large_enum_variant)]
enum WireResponse {
    StorageId([u8; 32]),
    BranchHead(Option<BranchHead>),
    LayerStackHead(LayerStackHead),
    Object(Vec<u8>),
    Bool(bool),
    Unit,
    Count(u64),
    BranchPush(BranchPushOutcome),
    BranchBundle(BranchPushBundle),
    ObjectPage(Vec<ObjectId>),
    ChildMerge(ChildMergeOutcome),
    BranchRollback(BranchRollbackOutcome),
    LayerStackMerge(LayerStackMergeOutcome),
    LayerStackRollback(LayerStackRollbackOutcome),
    Error(String),
}

/// Serves the authenticated Durable endpoint on a loopback listener. A TLS or
/// private-network proxy may carry the same framed endpoint to another host;
/// the built-in listener intentionally refuses cleartext non-loopback binds.
pub fn serve_loopback(root: &Path, bearer: &[u8], listener: TcpListener) -> Result<()> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(ServiceError::InvalidConfiguration);
    }
    let service = Service::open(root, bearer)?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        let response = match read_frame::<WireEnvelope>(&mut stream)
            .and_then(|envelope| dispatch(&service, envelope))
        {
            Ok(response) => response,
            Err(error) => WireResponse::Error(error.to_string()),
        };
        write_frame(&mut stream, &response)?;
    }
    Ok(())
}

fn dispatch(service: &Service, envelope: WireEnvelope) -> Result<WireResponse> {
    let session = service.authenticate(&envelope.bearer)?;
    let handshake = matches!(&envelope.request, WireRequest::StorageId);
    if handshake && envelope.expected_storage_id.is_some()
        || !handshake && envelope.expected_storage_id != Some(session.storage_id())
    {
        return Err(ServiceError::StorageIdentity);
    }
    Ok(match envelope.request {
        WireRequest::StorageId => WireResponse::StorageId(session.storage_id()),
        WireRequest::BranchHead(branch) => WireResponse::BranchHead(session.branch_head(branch)?),
        WireRequest::BootstrapLayerStack(stack, layer, name, root) => {
            WireResponse::LayerStackHead(session.bootstrap_layer_stack(stack, layer, &name, root)?)
        }
        WireRequest::ReadObject(id, maximum) => WireResponse::Object(
            session
                .read_object(id, maximum.min(layerfs_sync::MAX_BATCH_BYTES))
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::ContainsObject(id) => {
            WireResponse::Bool(session.contains_object(id).map_err(ServiceError::Sync)?)
        }
        WireRequest::AcceptObjects(owner, request, direction, objects) => {
            session
                .accept_objects(owner, request, direction, &objects)
                .map_err(ServiceError::Sync)?;
            WireResponse::Unit
        }
        WireRequest::AbortTransfer(owner, direction) => WireResponse::Count(
            session
                .abort_transfer(owner, direction)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::StageBranchPush(transfer, sequence, data_request, bundle, counters) => {
            session
                .stage_branch_push_page(transfer, sequence, data_request, &bundle, counters)
                .map_err(ServiceError::Sync)?;
            WireResponse::Unit
        }
        WireRequest::CommitBranchPush(request, branch) => WireResponse::BranchPush(
            session
                .commit_staged_branch_push(request, branch)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::ReconcileBranchPush(request, expected, accepted) => WireResponse::BranchPush(
            session
                .reconcile_branch_push(request, expected, accepted)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::ExportBranchFetch(branch, base, stack_base) => WireResponse::BranchBundle(
            session
                .export_branch_fetch(branch, base, stack_base)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::BranchFetchObjectPage(
            branch,
            base,
            stack_base,
            expected_head,
            expected_stack_head,
            after,
            limit,
        ) => WireResponse::ObjectPage(
            session
                .branch_fetch_object_page(
                    branch,
                    base,
                    stack_base,
                    expected_head,
                    expected_stack_head,
                    after,
                    limit,
                )
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::AcceptChildBranchMerge(publication) => WireResponse::ChildMerge(
            session
                .accept_child_branch_merge(publication)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::AcceptBranchRollback(publication) => WireResponse::BranchRollback(
            session
                .accept_branch_rollback(publication)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::AcceptLayerStackMerge(candidate, expected) => {
            WireResponse::LayerStackMerge(session.accept_layer_stack_merge(candidate, expected)?)
        }
        WireRequest::LayerStackRollback(expected, target) => {
            WireResponse::LayerStackRollback(session.layer_stack_rollback(expected, target)?)
        }
    })
}

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
        direction: layerfs_sync::Direction,
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
        direction: layerfs_sync::Direction,
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
        request_id: RequestId,
        expected: Option<BranchHead>,
        accepted: BranchHead,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        match self.sync_call(
            WireRequest::ReconcileBranchPush(request_id, expected, accepted),
            false,
        )? {
            WireResponse::BranchPush(outcome) => Ok(outcome),
            _ => Err(SyncError::Destination(
                "Branch Push reconciliation response".into(),
            )),
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

fn write_frame<T: serde::Serialize>(stream: &mut TcpStream, value: &T) -> Result<()> {
    let mut body = LimitedVec::new(MAX_WIRE_BYTES);
    serde_json::to_writer(&mut body, value)
        .map_err(|error| ServiceError::Wire(error.to_string()))?;
    if body.bytes.is_empty() {
        return Err(ServiceError::Wire("request limit".into()));
    }
    stream.write_all(&(body.bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&body.bytes)?;
    stream.flush()?;
    Ok(())
}

struct LimitedVec {
    bytes: Vec<u8>,
    limit: usize,
}

impl LimitedVec {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(64 * 1024)),
            limit,
        }
    }
}

impl Write for LimitedVec {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length > self.limit)
        {
            return Err(std::io::Error::other("wire frame limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn read_frame<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Result<T> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| ServiceError::Wire("request length".into()))?;
    if length == 0 || length > MAX_WIRE_BYTES {
        return Err(ServiceError::Wire("request limit".into()));
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|error| ServiceError::Wire(error.to_string()))
}

use crate::protocol::request::WireRequest;
use crate::{Result, ServiceError, MIN_BEARER_BYTES};
use layerfs_core::ObjectId;
use layerfs_durable_store::{
    BranchHead, BranchId, DurableStore, LayerCandidate, LayerId, LayerStackHead, LayerStackId,
    LayerStackMergeOutcome, LayerStackRollbackOutcome,
};
use std::path::Path;

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
    pub(crate) fn durable(&self) -> &DurableStore {
        &self.service.durable
    }

    pub fn storage_id(&self) -> [u8; 32] {
        self.durable().storage_id()
    }

    pub(crate) fn authorize_storage(
        &self,
        expected: Option<[u8; 32]>,
        request: &WireRequest,
    ) -> Result<()> {
        let handshake = matches!(request, WireRequest::StorageId);
        if handshake && expected.is_some() || !handshake && expected != Some(self.storage_id()) {
            return Err(ServiceError::StorageIdentity);
        }
        Ok(())
    }

    pub fn branch_head(&self, id: BranchId) -> Result<Option<BranchHead>> {
        Ok(self.durable().branch_head(id)?)
    }

    pub fn layer_stack_head(&self, id: LayerStackId) -> Result<Option<LayerStackHead>> {
        Ok(self.durable().layer_stack_head(id)?)
    }

    pub fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> Result<LayerStackHead> {
        Ok(self
            .durable()
            .bootstrap_layer_stack(stack, layer, name, root)?)
    }

    pub fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome> {
        Ok(self
            .durable()
            .accept_layer_stack_merge(candidate, expected)?)
    }

    pub fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome> {
        Ok(self.durable().layer_stack_rollback(expected, target)?)
    }
}

use layerfs_core::ObjectId;
use layerfs_durable_store::{
    BranchHead, BranchId, LayerCandidate, LayerId, LayerStackHead, LayerStackId,
};
use layerfs_sync::{
    BranchPushBundle, BranchPushRequest, BranchRollbackPublication, ChildMergePublication,
    Direction, RequestId, SyncTransferCounters,
};

#[derive(serde::Deserialize)]
pub(crate) struct WireEnvelope {
    pub(crate) bearer: Vec<u8>,
    pub(crate) expected_storage_id: Option<[u8; 32]>,
    pub(crate) request: WireRequest,
}

#[derive(serde::Serialize)]
pub(crate) struct WireEnvelopeRef<'a> {
    pub(crate) bearer: &'a [u8],
    pub(crate) expected_storage_id: Option<[u8; 32]>,
    pub(crate) request: &'a WireRequest,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum WireRequest {
    StorageId,
    BranchHead(BranchId),
    BootstrapLayerStack(LayerStackId, LayerId, String, ObjectId),
    ReadObject(ObjectId, usize),
    ContainsObject(ObjectId),
    AcceptObjects(RequestId, RequestId, Direction, Vec<(ObjectId, Vec<u8>)>),
    AbortTransfer(RequestId, Direction),
    StageBranchPush(
        RequestId,
        u64,
        RequestId,
        BranchPushBundle,
        SyncTransferCounters,
    ),
    CommitBranchPush(BranchPushRequest, BranchId),
    ReconcileBranchPush(BranchPushRequest, BranchHead),
    RecordReplayedBranchPush(BranchPushRequest, BranchHead),
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

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_durable_store::OperationVersionId;

    #[test]
    fn reconcile_codec_preserves_exact_push_identity() {
        let request_id = RequestId::from_bytes([0x31; 32]);
        let transfer_id = RequestId::from_bytes([0x32; 32]);
        let expected = BranchHead {
            branch_id: BranchId::from_bytes([0x33; 32]),
            generation: 7,
            operation_version_id: Some(OperationVersionId::from_bytes([0x34; 32])),
            root: ObjectId::for_bytes(b"expected"),
        };
        let accepted = BranchHead {
            generation: 8,
            operation_version_id: Some(OperationVersionId::from_bytes([0x35; 32])),
            root: ObjectId::for_bytes(b"accepted"),
            ..expected
        };
        let request = BranchPushRequest {
            request_id,
            transfer_id,
            candidate_digest: [0x36; 32],
            expected: Some(expected),
            counters: SyncTransferCounters {
                unique_bytes: 11,
                resumed_bytes: 12,
                retransmitted_bytes: 13,
            },
        };
        let encoded =
            crate::protocol::codec::encode(&WireRequest::ReconcileBranchPush(request, accepted))
                .unwrap();
        match crate::protocol::codec::decode::<WireRequest>(&encoded).unwrap() {
            WireRequest::ReconcileBranchPush(decoded_request, decoded_accepted) => {
                assert_eq!(decoded_request, request);
                assert_eq!(decoded_accepted, accepted);
            }
            _ => panic!("reconcile request variant"),
        }
    }
}

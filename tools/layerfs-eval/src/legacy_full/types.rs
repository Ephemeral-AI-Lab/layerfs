pub(crate) type RootId = layerfs_core::ObjectId;
pub(crate) type RefState = layerfs_storage::refs::RefState;
pub(crate) type IntegrityMode = layerfs_storage::integrity::IntegrityMode;
pub(crate) type OperationDiagnostics = layerfs_materialization::OperationCounters;
pub(crate) type OperationCounters = layerfs_materialization::OperationCounters;
pub(crate) type NativeOperationCounters = layerfs_materialization::NativeOperationCounters;
pub(crate) type NativeMetadata = layerfs_materialization::driver::NativeMetadata;
pub(crate) type NativeXattrs = layerfs_materialization::driver::NativeXattrs;
pub(crate) type NativeRoute = layerfs_materialization::NativeRoute;
pub(crate) type ProjectionFacts = layerfs_materialization::driver::ProjectionFacts;
pub(crate) type ProjectionCallFacts = layerfs_materialization::driver::ProjectionCallFacts;
pub(crate) type ProjectionCleanupFacts = layerfs_materialization::driver::ProjectionCleanupFacts;
pub(crate) type ProjectionReplaceFacts = layerfs_materialization::driver::ProjectionReplaceFacts;
pub(crate) type ProjectionSyncFacts = layerfs_materialization::driver::ProjectionSyncFacts;
pub(crate) type ProjectionTimer = layerfs_materialization::driver::ProjectionTimer;
pub(crate) type ProjectionTimerAvailability =
    layerfs_materialization::driver::ProjectionTimerAvailability;
pub(crate) type ProjectionWriteFacts = layerfs_materialization::driver::ProjectionWriteFacts;

pub(crate) const PRODUCT_BUFFER_BOUND_BYTES: usize =
    layerfs_materialization::driver::MAX_NATIVE_XATTR_BYTES;
pub(crate) const OPERATION_Q_BOUND_BYTES: u64 = layerfs_materialization::OPERATION_Q_BOUND_BYTES;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ManagedReplayStep {
    pub(crate) tree_level_before: Option<u8>,
    pub(crate) counters: OperationDiagnostics,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AcceptedSplice {
    pub(crate) before: RefState,
    pub(crate) after: RefState,
    pub(crate) path: layerfs_core::CanonicalPath,
    pub(crate) start: u64,
    pub(crate) delete_len: u64,
    pub(crate) insert_len: u64,
    pub(crate) old_len: u64,
    pub(crate) new_len: u64,
}

impl AcceptedSplice {
    pub(crate) fn before(&self) -> &RefState {
        &self.before
    }
    pub(crate) fn after(&self) -> &RefState {
        &self.after
    }
    pub(crate) fn path(&self) -> &layerfs_core::CanonicalPath {
        &self.path
    }
}

use layerfs_branch_store::BranchStore;
use layerfs_storage_core::{BranchId, Result};
use layerfs_workspace::Workspace;
use std::path::Path;

pub(crate) fn workspace(
    store: &BranchStore,
    branch: BranchId,
    spool: impl AsRef<Path>,
) -> Result<Workspace> {
    Workspace::open(store.clone(), branch, spool)
}

pub(crate) fn materialize(store: &BranchStore, branch: BranchId, destination: &Path) -> Result<()> {
    layerfs_materialization::materialize(store, branch, destination)
}

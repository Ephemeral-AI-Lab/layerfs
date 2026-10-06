//! Exact provision and original reply checks, with no Branch refresh or provider open.
use super::{BindingMismatch as Mismatch, ExpectedBinding, PersistenceBootstrap, UpstreamError};
use layerfs_sdk::client::RemoteBinding;

pub(super) fn provision(
    expected: &ExpectedBinding,
    bootstrap: &PersistenceBootstrap,
    host: [u8; 32],
) -> Result<(), UpstreamError> {
    if expected.runtime == [0; 32]
        || expected.local_peer == [0; 32]
        || expected.host_peer == [0; 32]
        || expected.provider_incarnation == 0
        || expected.root_serial == 0
        || expected.root_serial > i64::MAX as u64
        || expected.snapshot.branch.head_commit.is_some() != expected.snapshot.head_root.is_some()
        || expected.snapshot.effective_root
            != expected
                .snapshot
                .head_root
                .unwrap_or(expected.snapshot.base_root)
    {
        return Err(UpstreamError::Mismatch(Mismatch::Provision));
    }
    if host != expected.host_peer {
        return Err(UpstreamError::Mismatch(Mismatch::HostPeer));
    }
    if bootstrap.host_peer != expected.host_peer
        || bootstrap.runtime != expected.runtime
        || bootstrap.catalog != expected.catalog
        || bootstrap.provider_incarnation != expected.provider_incarnation
    {
        return Err(UpstreamError::Mismatch(Mismatch::Bootstrap));
    }
    if bootstrap.profile != expected.persistence {
        return Err(UpstreamError::Mismatch(Mismatch::Persistence));
    }
    expected
        .policy
        .validated()
        .map_err(UpstreamError::Storage)?;
    Ok(())
}
pub(super) fn received(
    expected: &ExpectedBinding,
    binding: &RemoteBinding,
) -> Result<(), UpstreamError> {
    if binding.peer != expected.local_peer
        || binding.runtime != expected.runtime
        || binding.catalog != expected.catalog
        || binding.incarnation != expected.provider_incarnation
        || binding.workspace != expected.workspace
    {
        return Err(UpstreamError::Mismatch(Mismatch::Authority));
    }
    if binding.snapshot != expected.snapshot {
        return Err(UpstreamError::Mismatch(Mismatch::Snapshot));
    }
    if binding.root_serial != expected.root_serial {
        return Err(UpstreamError::Mismatch(Mismatch::RootSerial));
    }
    Ok(())
}

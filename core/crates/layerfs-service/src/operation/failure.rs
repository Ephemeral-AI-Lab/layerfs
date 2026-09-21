//! Preserve the distinctions exposed by C1/C2; never parse diagnostic strings.
use layerfs_bridge::contract::{Code, Failure};
use layerfs_content::ContentError as C;
use layerfs_storage::StorageError as S;
pub fn content(e: C) -> Failure {
    match e {
        C::MissingObject => Code::MissingObject,
        C::PathNotFound => Code::PathNotFound,
        C::ProviderFailure { .. } | C::OutputRejected => Code::Provider,
        C::BoundedCapacityExceeded { .. }
        | C::ObjectLimitExceeded { .. }
        | C::ResourceUnavailable { .. }
        | C::PathLimitExceeded => Code::Capacity,
        C::UnsupportedProfile { .. } | C::UnsupportedPolicy { .. } => Code::Unsupported,
        C::Io => Code::Io,
        C::IdentityMismatch => Code::Integrity,
        _ => Code::InvalidInput,
    }
    .into()
}
pub fn storage(e: S) -> Failure {
    match e {
        S::Content(e) => content(e),
        S::ObjectMissing(_) | S::MissingDependency { .. } => Code::MissingObject.into(),
        S::OwnershipUnavailable | S::UninspectedState { .. } => Code::Ownership.into(),
        S::UnsupportedPolicy { .. } => Code::Unsupported.into(),
        S::CapacityExceeded { .. } => Code::Capacity.into(),
        S::UnknownOutcome { original } => {
            let mut f = storage(*original);
            f.unknown = true;
            f
        }
        S::CleanupFailed { original, cleanup } => {
            let mut f = storage(*original);
            let c = storage(*cleanup);
            f.unknown |= c.unknown;
            f.cleanup = Some(c.code);
            f
        }
        S::Engine(_) => Code::Provider.into(),
        S::Collision(_) | S::VisibilityCeiling { .. } | S::Unpublished(_) | S::Integrity(_) => {
            Code::Integrity.into()
        }
        S::Aborted => Code::InvalidInput.into(),
    }
}

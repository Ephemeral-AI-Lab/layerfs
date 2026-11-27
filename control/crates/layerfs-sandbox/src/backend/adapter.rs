//! The one extension point for a Sandbox backend (#535 §6.3, G2): a
//! crate-private, sealed trait with no default methods, reached through
//! [`adapter`], the one exhaustive match. Each backend folder holds its
//! `ADAPTER`: what is fixed per backend (its stored name, description and
//! capabilities). Launch and reopen stay on the providers the public
//! `Runtime` enum holds; `LaunchRequest` binds arrive in a later phase.
use crate::ProviderCapabilities;
use {
    super::{namespaces, process},
    layerfs_machine::table::backend::Backend,
};

/// What is fixed for one Sandbox backend beyond its name and description,
/// which the machine crate's [`Backend`] holds.
pub(crate) trait BackendAdapter: Sync {
    /// What this backend supports.
    fn capabilities(&self) -> ProviderCapabilities;
}

/// The backend's adapter: the one match over backends.
pub(crate) fn adapter(backend: Backend) -> &'static dyn BackendAdapter {
    match backend {
        Backend::Process => &process::ADAPTER,
        Backend::Namespaces => &namespaces::ADAPTER,
    }
}

/// What `backend` supports.
pub fn capabilities(backend: Backend) -> ProviderCapabilities {
    adapter(backend).capabilities()
}

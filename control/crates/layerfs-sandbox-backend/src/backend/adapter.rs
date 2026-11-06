//! The one extension point for a Sandbox backend (#535 §6.3, G2): a
//! crate-private, sealed trait with no default methods, reached through
//! [`adapter`], the one exhaustive match. Each backend folder holds its
//! `ADAPTER`: what is fixed per backend (its stored name, description and
//! capabilities). Launch and reopen stay on the providers the public
//! `Runtime` enum holds; `LaunchRequest` binds arrive in a later phase.
use super::{docker, host_process, namespaces, Backend};
use crate::ProviderCapabilities;

/// What is fixed for one Sandbox backend.
pub(crate) trait BackendAdapter: Sync {
    /// The value Sandbox records, `machine.platforms` and `SUPPORT.json`
    /// store and show (`host_process` is `"native"`, G6).
    fn stored_name(&self) -> &'static str;
    /// One line: how a Sandbox runs on this backend.
    fn description(&self) -> &'static str;
    /// What this backend supports.
    fn capabilities(&self) -> ProviderCapabilities;
}

/// The backend's adapter: the one match over backends.
pub(crate) fn adapter(backend: Backend) -> &'static dyn BackendAdapter {
    match backend {
        Backend::Docker => &docker::ADAPTER,
        Backend::HostProcess => &host_process::ADAPTER,
        Backend::Namespaces => &namespaces::ADAPTER,
    }
}

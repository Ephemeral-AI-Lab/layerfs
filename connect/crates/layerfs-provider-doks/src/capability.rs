//! The typed capability a DOKS machine declares (#551 B0-145, #554): the
//! same declaration the built-in ssh machine gets from its `namespaces`
//! backend, since this adapter reuses that machine unchanged.

use crate::record::PROVIDER;
use layerfs_machine::{HostOs, Platform};
use layerfs_sandbox::{ExecMode, IsolationClass, KernelScope, ProviderCapabilities};

/// One machine's provider declaration: the provider name (data, never a
/// `Platform` variant), the constructor core routes by, the isolation class
/// and the typed backend capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// This adapter's provider name, `doks`.
    pub provider: &'static str,
    /// The machine constructor the adapter reuses: the built-in `ssh`.
    pub platform: Platform,
    /// `Machine.kind`: the isolation class, `remote`.
    pub kind: IsolationClass,
    /// Where Sandbox work runs, `worker`.
    pub exec_mode: ExecMode,
    /// The kernel scope of the class, `separate-host`.
    pub kernel: KernelScope,
    /// The backend capability, identical to the ssh machine's.
    pub capability: ProviderCapabilities,
}

/// The capability a DOKS machine has on `host`: exactly the ssh machine's
/// `namespaces` backend declaration.
pub fn capability_for(host: HostOs) -> ProviderCapabilities {
    let backend = Platform::Ssh
        .backend(host)
        .expect("the ssh platform declares a backend on every named host");
    backend.capabilities()
}

/// [`capability_for`] on this host.
pub fn capability() -> ProviderCapabilities {
    capability_for(HostOs::current())
}

/// The adapter's declaration on this host: provider `doks`, platform `ssh`,
/// kind `remote`, exec mode `worker`, and the ssh capability.
pub fn declaration() -> Declaration {
    let host = HostOs::current();
    let kind = Platform::Ssh
        .isolation(host)
        .expect("the ssh platform declares an isolation class on every named host");
    let kernel = Platform::Ssh
        .kernel_scope(host)
        .expect("the ssh platform declares a kernel scope on every named host");
    Declaration {
        provider: PROVIDER,
        platform: Platform::Ssh,
        kind,
        exec_mode: Platform::Ssh.exec_mode(host),
        kernel,
        capability: capability_for(host),
    }
}

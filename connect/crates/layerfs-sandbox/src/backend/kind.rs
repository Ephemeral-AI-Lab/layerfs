//! How a Sandbox runs on a machine (`layerfs-machine`'s `Kind::backend`);
//! callers never choose it.

/// How a Sandbox runs on a machine; callers never choose it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// Host processes with loopback NFS (macOS).
    HostProcess,
    /// Linux namespaces and a cgroup.
    Namespaces,
    /// Docker containers.
    Docker,
}

//! The deployment paths the Engine and the daemon both name.

/// The daemon's entry point inside its Sandbox container, as the deployment
/// installs it. The create request, the observed-topology check and the
/// `ns-exec` helper argv all name this one string.
pub const DAEMON_ENTRY: &str = "/usr/local/bin/layerfs-daemon";

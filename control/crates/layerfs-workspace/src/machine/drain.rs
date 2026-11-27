//! The Workspace unmount a Sandbox close runs on each Workspace it still
//! holds ([`WorkspaceDrain`], declared by the Sandbox crate).
use layerfs_sandbox::lifecycle::serve::WorkspaceDrain;
use layerfs_sandbox::route::machine::Host;

/// [`crate::machine::serve::unmount`], discarding and forced.
pub struct Drain;

impl WorkspaceDrain for Drain {
    fn drain(&self, host: &Host, path: &str) {
        let _ = crate::machine::serve::unmount(host, Some(path), true, true);
    }
}

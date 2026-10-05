//! Sandbox custody through the composed owner.
use layerfs_api_core::{DeleteError, SandboxId, SandboxInfo};
use layerfs_bridge::contract::Failure;
use layerfs_sandbox::{CreateError, LogCapture, SandboxOwner};
use std::io::Write;

pub struct SandboxApi<'a> {
    owner: &'a SandboxOwner,
}
impl<'a> SandboxApi<'a> {
    pub fn new(owner: &'a SandboxOwner) -> Self {
        Self { owner }
    }

    /// Launch one owned sandbox from an immutable image digest.
    pub fn create(&self, image_id: &str, sandbox_name: &str) -> Result<SandboxId, CreateError> {
        self.owner.create(image_id, sandbox_name)
    }

    /// List this owner's sandboxes with their current observed status.
    pub fn list(&self) -> Result<Vec<SandboxInfo>, Failure> {
        self.owner.list()
    }

    /// Remove one owned sandbox, its container and its named Workspace volume.
    ///
    /// An unknown ID is refused: deletion can only ever name a sandbox this
    /// owner admitted. A partial outcome reports which resources remain.
    pub fn delete(&self, id: SandboxId) -> Result<(), DeleteError> {
        self.owner.delete(id)
    }

    /// Delete an owned sandbox, capturing up to 8 MiB of raw daemon stderr
    /// after stop. Capture status and cleanup status are independent.
    pub fn delete_with_logs(
        &self,
        id: SandboxId,
        output: &mut (dyn Write + Send),
    ) -> (Result<(), DeleteError>, LogCapture) {
        self.owner.delete_with_logs(id, output)
    }
}

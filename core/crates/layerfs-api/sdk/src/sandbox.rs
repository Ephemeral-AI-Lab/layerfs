use layerfs_api_core::{SandboxId, SandboxInfo};
use layerfs_bridge::contract::Failure;
use layerfs_sandbox::{CreateError, SandboxOwner};

pub struct SandboxApi<'a> {
    owner: &'a SandboxOwner,
}
impl<'a> SandboxApi<'a> {
    pub fn new(owner: &'a SandboxOwner) -> Self {
        Self { owner }
    }
    pub fn create(&self, image_id: &str, sandbox_name: &str) -> Result<SandboxId, CreateError> {
        self.owner.create(image_id, sandbox_name)
    }
    pub fn list(&self) -> Result<Vec<SandboxInfo>, Failure> {
        self.owner.list()
    }
}

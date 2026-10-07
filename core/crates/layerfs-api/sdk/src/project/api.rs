//! Project provisioning facade; no installed Store or host data service owner.
use crate::{InitFailure, InitRequest, InstallFailure, Installed, SealedProject};
use layerfs_bridge::native::Connection;
use layerfs_telemetry::timer::{Active, TimingScope};

/// Native Project Init/seal/install and authenticated fork/history organization.
/// The facade retains no Store handle. Each method delegates one operation.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProjectApi;
impl ProjectApi {
    /// Creates the stateless public Project facade.
    pub fn new() -> Self {
        Self
    }
    /// Imports the complete supported native tree, publishes genesis/Branch and seals.
    /// An explicit Disposable profile is required before any Store creation.
    pub fn init(
        &self,
        request: InitRequest,
        timer: &TimingScope<'_, Active>,
    ) -> Result<SealedProject, Box<InitFailure>> {
        super::init::initialize(request, timer)
    }
    /// Streams one closed file through an already authenticated connection once.
    /// The host never opens the installed Store; original failure custody is kept.
    pub fn install(
        &self,
        project: &SealedProject,
        connection: &mut Connection,
    ) -> Result<Installed, Box<InstallFailure>> {
        super::install::install(project, connection)
    }
}

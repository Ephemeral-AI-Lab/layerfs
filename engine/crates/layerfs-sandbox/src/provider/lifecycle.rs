//! One-attempt runtime lifecycle with acknowledged partial ownership on launch failure.
use super::{
    ExecExit, LaunchRequest, LocalRecord, ProviderCapabilities, SandboxId, SandboxInspection,
};
use crate::{ExecRequest, RuntimeError, WireFailure};
use std::{fmt, io::Write, net::SocketAddr};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchPhase {
    Validate,
    Create,
    Start,
    Listener,
}
/// The caller retains the exact resource even when launch fails after creating it.
pub struct LaunchFailure {
    pub phase: LaunchPhase,
    pub instance: Option<Box<dyn SandboxInstance>>,
    pub cause: RuntimeError,
}
impl fmt::Debug for LaunchFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchFailure")
            .field("phase", &self.phase)
            .field("identity", &self.instance.as_ref().map(|i| i.identity()))
            .field("cause", &self.cause)
            .finish()
    }
}
pub trait SandboxInstance: Send + Sync {
    fn identity(&self) -> SandboxId;
    fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>>;
    fn run(
        &self,
        request: &ExecRequest,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>>;
    fn track(&self, pid: u32, start_time: u64) -> Result<(), Box<WireFailure>>;
    fn log_tail(&self, lines: u16, cap: usize) -> Result<Vec<u8>, Box<WireFailure>>;
    fn inspect(&self) -> Result<SandboxInspection, Box<WireFailure>>;
    fn stop(&mut self, timeout_seconds: u16) -> Result<(), Box<WireFailure>>;
    fn delete(&mut self) -> Result<(), Box<WireFailure>>;
    fn scratch_directory(&self) -> String;
    /// Physical facts for the binding's existing record format, when this is local.
    fn local_record(&self) -> Option<LocalRecord>;
}
pub trait SandboxProvider: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities;
    fn launch(
        &self,
        request: LaunchRequest,
    ) -> Result<Box<dyn SandboxInstance>, Box<LaunchFailure>>;
    /// Reopen starts nothing and checks the recorded process identity.
    fn reopen(
        &self,
        id: SandboxId,
        record: &LocalRecord,
    ) -> Result<Box<dyn SandboxInstance>, Box<WireFailure>>;
    /// A known stopped record admits only the existing runtime's single Delete.
    fn reopen_stopped(
        &self,
        id: SandboxId,
        record: &LocalRecord,
    ) -> Result<Box<dyn SandboxInstance>, Box<WireFailure>>;
}

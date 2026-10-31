//! Linux namespace Sandbox handle slot; implemented by the namespaces backend.
use crate::backend::native::LocalProcess;
use crate::provider::{ExecExit, SandboxRecord};
use crate::{ContainerId, ExecRequest, RuntimeError, WireFailure};
use std::{io::Write, net::SocketAddr, path::PathBuf};

const UNSUPPORTED: &str = "the Linux namespaces Sandbox backend";

/// One acknowledged namespace Sandbox.
#[derive(Debug)]
pub struct Sandbox {
    id: ContainerId,
}

impl Sandbox {
    /// Its identity: random at create, recorded, never an Engine's.
    pub fn identity(&self) -> ContainerId {
        self.id
    }

    /// Its commands' scratch directory (1777), deleted with it.
    pub fn tmp(&self) -> PathBuf {
        PathBuf::new()
    }

    /// Its init stage process, once started.
    pub fn process(&self) -> Option<LocalProcess> {
        None
    }

    /// Its record, naming this backend so another one never reopens it.
    pub fn record(&self) -> Option<SandboxRecord> {
        None
    }

    /// The daemon's endpoint while its init stage runs.
    pub fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(UNSUPPORTED)))
    }

    /// Runs one command to its end inside the Sandbox's namespaces.
    pub fn run(
        &self,
        _request: &ExecRequest,
        _stdout: &mut (dyn Write + Send),
        _stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(UNSUPPORTED)))
    }

    /// Its cgroup owns every process the Sandbox started.
    pub fn track(&self, _pid: u32, _start_time: u64) -> Result<(), Box<WireFailure>> {
        Ok(())
    }

    /// The daemon output's last `lines` lines, keeping only the final `cap` bytes.
    pub fn log_tail(&self, _lines: u16, _cap: usize) -> Result<Vec<u8>, Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(UNSUPPORTED)))
    }

    /// Reaps the init stage this process started, if it exited.
    pub(crate) fn reap(&self) -> bool {
        true
    }

    /// Ends the Sandbox once.
    pub fn stop(&mut self, _timeout_seconds: u16) -> Result<(), Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(UNSUPPORTED)))
    }

    /// Deletes its directory and cgroup once, after a known stop.
    pub fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        Err(WireFailure::local(RuntimeError::Unsupported(UNSUPPORTED)))
    }
}

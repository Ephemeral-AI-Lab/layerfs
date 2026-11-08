//! The Windows face of the launched Sandbox owner: the runtime itself is a
//! unix surface (namespaces/host_process); #454 carries the Windows
//! host_process port. Records and ids flow; every runtime method answers
//! Unsupported until the port.
//! (Reached only on Windows: the module itself is cfg-gated at its use.)
use std::path::PathBuf;
use std::net::SocketAddr;

/// The failure of a launch that never ran on this host.
#[derive(Debug)]
pub struct LaunchFailure(pub String);
/// A wire failure twin for the stub face.
#[derive(Debug)]
pub struct WireFailure(pub String);

fn unsupported(what: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        format!("the {what} face waits for the Windows host_process port (#454)"),
    )
}

/// A launched Sandbox as the records name it.
#[derive(Clone, Debug)]
pub struct Sandbox {
    pub id: crate::types::SandboxId,
    pub root: PathBuf,
}

impl Sandbox {
    pub fn backend(&self) -> crate::types::kind::Backend {
        crate::types::kind::Backend::HostProcess
    }
    pub fn id(&self) -> crate::types::SandboxId {
        self.id
    }
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }
    pub fn identity(&self) -> crate::types::ContainerId {
        // An Engine identity needs a real engine answer; the port carries it.
        crate::types::ContainerId::parse("stub:windows:not-launched").expect("a static valid id")
    }
    pub fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        Err(Box::new(WireFailure("no endpoint on the stub face".into())))
    }
    pub fn exec_stop(&self, _workspace: &str, _exec: &str) -> Option<crate::stop::Stop> {
        None
    }
    pub fn stop(&mut self, _timeout_seconds: u16) -> Result<(), Box<WireFailure>> {
        Err(Box::new(WireFailure(unsupported("stop").to_string())))
    }
    pub fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        Err(Box::new(WireFailure(unsupported("delete").to_string())))
    }
    pub fn scratch_directory(&self) -> String {
        String::new()
    }
}

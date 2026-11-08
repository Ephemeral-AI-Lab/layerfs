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

// The Sandbox enum and its impl live in runtime.rs (the Windows variant
// and its arms are there); this module keeps the failure twins only.

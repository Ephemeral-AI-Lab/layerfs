use crate::transport::connection;
use crate::{Result, Service, ServiceError};
use std::net::TcpListener;
use std::path::Path;

/// Serves the authenticated Durable endpoint on a loopback listener. A TLS or
/// private-network proxy may carry the same framed endpoint to another host;
/// the built-in listener intentionally refuses cleartext non-loopback binds.
pub fn serve_loopback(root: &Path, bearer: &[u8], listener: TcpListener) -> Result<()> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(ServiceError::InvalidConfiguration);
    }
    let service = Service::open(root, bearer)?;
    for stream in listener.incoming() {
        connection::serve(&service, stream?)?;
    }
    Ok(())
}

use layerfs_api_core::SandboxId;
use layerfs_bridge::{
    adapters::native::{client::Client, connection::connect_until},
    contract::{
        Code, Failure, Operation, Request, Response, SandboxHelloWire, WORKSPACE_STATUS_PROFILE,
    },
};
use std::{
    io,
    net::SocketAddr,
    thread,
    time::{Duration, Instant},
};

pub(crate) fn hello(
    endpoint: SocketAddr,
    private: &[u8; 32],
    server: &[u8; 32],
    id: SandboxId,
) -> Result<SandboxHelloWire, Failure> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut client = Client::new(connect_until(endpoint, 1, private, server, deadline)?)?;
    let request = Request {
        id: 1,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms: 5_000,
        response_bytes: 0,
        operation: Operation::SandboxHello,
    };
    let response = client.call_until(&request, &mut &[][..], &mut io::sink(), deadline)?;
    let Response::SandboxHello(hello) = response else {
        return Err(Code::Integrity.into());
    };
    if hello.sandbox != id.0 {
        return Err(Code::Denied.into());
    }
    Ok(hello)
}

pub(crate) fn wait(
    endpoint: SocketAddr,
    private: &[u8; 32],
    server: &[u8; 32],
    id: SandboxId,
) -> Result<SandboxHelloWire, Failure> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match hello(endpoint, private, server, id) {
            Ok(hello) => return Ok(hello),
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(_) => thread::sleep(Duration::from_millis(25)),
        }
    }
}

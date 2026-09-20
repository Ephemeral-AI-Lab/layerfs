//! Native assembly; no Store, SQL or service dependency.
use layerfs_bridge::{
    adapters::native::{client::Client, connection::connect, pipe::key},
    contract::*,
};
use std::net::{SocketAddr, ToSocketAddrs};
fn env(name: &str) -> Result<String, Failure> {
    let s = std::env::var(name).map_err(|_| Code::InvalidInput)?;
    if s.len() > 256 {
        return Err(Code::Capacity.into());
    }
    Ok(s)
}
pub fn run() -> Result<(), Failure> {
    let address: SocketAddr = env("LAYERFS_ENDPOINT")?
        .to_socket_addrs()?
        .next()
        .ok_or(Code::InvalidInput)?;
    let selector = env("LAYERFS_SELECTOR")?
        .parse()
        .map_err(|_| Code::InvalidInput)?;
    let private = key(&env("LAYERFS_PRIVATE_KEY")?)?;
    let server = key(&env("LAYERFS_SERVER_KEY")?)?;
    let mut client = Client::new(connect(address, selector, &private, &server)?)?;
    let telemetry = crate::config::telemetry(2);
    crate::headless::run(&mut client, &telemetry)
}

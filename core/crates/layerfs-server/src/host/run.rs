//! Operator process assembly: configured keys, Store path and one acceptor.
use crate::host::{acceptor::Acceptor, config, store};
use crate::{Grant, Service, StoreAccess};
use layerfs_bridge::{
    adapters::native::{connection::Peer, pipe::key},
    contract::*,
};
use std::{net::SocketAddr, sync::Arc};

fn env(name: &str) -> Result<String, Failure> {
    let value = std::env::var(name).map_err(|_| Code::InvalidInput)?;
    if value.len() > 4096 {
        return Err(Code::Capacity.into());
    }
    Ok(value)
}

fn configured_peers() -> Result<(Vec<Peer>, Vec<Grant>), Failure> {
    let mut peers = Vec::new();
    let mut grants = Vec::new();
    for entry in env("LAYERFS_PEERS")?.split(';') {
        if peers.len() == 16 {
            return Err(Code::Capacity.into());
        }
        let values: Vec<_> = entry.split(',').collect();
        if values.len() != 4 {
            return Err(Code::InvalidInput.into());
        }
        let selector = values[0].parse::<u32>().map_err(|_| Code::InvalidInput)?;
        let public = key(values[1])?;
        let expires_unix = values[2].parse().map_err(|_| Code::InvalidInput)?;
        let operations = values[3].parse().map_err(|_| Code::InvalidInput)?;
        if peers.iter().any(|peer: &Peer| peer.selector == selector) {
            return Err(Code::InvalidInput.into());
        }
        peers.push(Peer {
            selector,
            public,
            expires_unix,
        });
        grants.push(Grant {
            public_key: public,
            operations,
            expires_unix,
        });
    }
    Ok((peers, grants))
}

pub fn run() -> Result<(), Failure> {
    let private = key(&env("LAYERFS_PRIVATE_KEY")?)?;
    let (peers, grants) = configured_peers()?;
    let store = store::open(std::path::Path::new(&env("LAYERFS_STORE")?))?;
    let capacity = store::capacity(&store)?;
    let runtime = config::telemetry(1);
    let mut service = Service::new(
        vec![StoreAccess {
            id: 1,
            store,
            grants,
            history: config::history()?,
        }],
        runtime.recorder(),
    )?;
    if let Some(root) = std::env::var_os("LAYERFS_IMPORT_ROOT") {
        service.set_import_root(std::path::Path::new(&root))?;
    }
    let endpoint: SocketAddr = env("LAYERFS_LISTEN")?
        .parse()
        .map_err(|_| Code::InvalidInput)?;
    let acceptor = Acceptor::bind(
        endpoint,
        capacity,
        private,
        peers,
        Arc::new(service),
        runtime,
    )?;
    layerfs_bridge::adapters::native::pipe::diagnostic(&format!(
        "layerfs-server ready {}\n",
        acceptor.endpoint()?
    ));
    acceptor.serve(crate::host::acceptor::Stop::Stdin)
}

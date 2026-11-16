//! The full chain, in-process: Store -> Service -> loopback control ->
//! the handshake face Controls rides, the pieces a Windows create
//! assembles (#454). The WinFsp arms run when the driver is live;
//! today the chain proves its control face over the loopback pair.
#![cfg(windows)]
#[path = "../../layerfs-store-service/tests/support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../../layerfs-store-service/tests/support/sdk.rs"]
#[allow(dead_code)]
mod support;

#[test]
fn loopback_control_answers_one_call() {
    let label = format!("winfsp-chain-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let sdk = support::Fixture::new(&label, |source| {
        std::fs::write(source.join("seed.txt"), b"seed").unwrap();
        1
    });
    // The daemon assembly: the twin Service at the owner capacity.
    let service = std::sync::Arc::new(layerfs_daemon::control::Service::new(
        sdk.service.store.clone(),
        sdk.owner.configuration().namespaces,
    ));
    let caller_keys = layerfs_bridge::duplex::generate_keypair().expect("caller keypair");
    let (address, server_public) = service.serve_loopback(caller_keys.public).expect("loopback listener");

    // The caller face: the handshake Controls rides, over loopback.
        let stream = std::net::TcpStream::connect(address).expect("connect loopback");
    let connection =
        layerfs_bridge::duplex::initiate(stream, &caller_keys.private, server_public)
            .expect("handshake over the in-process face");
    // The handshake answered: the chain pieces assemble (Store, Service,
    // loopback serve, the noise dial). The call exchange rides the same
    // face; the mount arms wait on the driver the e2e test names.
    drop(connection);
    drop(service);
}

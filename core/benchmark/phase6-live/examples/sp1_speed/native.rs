use super::{
    json::{object, Json},
    model::Result,
};
use layerfs_bridge::adapters::native::{
    connection::{self, Peer, VerifiedPeer},
    protocol::{Frame, Kind},
};
use phase6_live_probe::{
    strict_catalog::StrictCatalog,
    strict_remote::{NativeCatalog, Session},
    strict_wire,
};
use std::{
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
pub fn start(
    catalog: Arc<StrictCatalog>,
) -> Result<(Arc<NativeCatalog>, std::thread::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let endpoint = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .to_string();
    let peer = Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&[7; 32])
            .map_err(|e| e.to_string())?
            .public_key(),
        expires_unix: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs()
            + 60,
    };
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().map_err(|e| e.to_string())?;
        let mut channel =
            connection::accept(stream, &[8; 32], &[peer]).map_err(|e| e.to_string())?;
        while let Ok(frame) = channel.receive.read() {
            if frame.kind != Kind::Begin {
                return Err("native request kind".into());
            }
            let bytes = strict_wire::handle(&catalog, &frame.bytes)?;
            channel
                .send
                .write(&Frame {
                    kind: Kind::Success,
                    id: frame.id,
                    bytes,
                })
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    });
    Ok((
        Arc::new(NativeCatalog {
            endpoint,
            selector: 1,
            private: [7; 32],
            server: *VerifiedPeer::from_private(&[8; 32])
                .map_err(|e| e.to_string())?
                .public_key(),
            session: Arc::new(Mutex::new(Session::default())),
        }),
        server,
    ))
}
pub fn metrics(c: &NativeCatalog) -> Result<Json> {
    let s = c.statistics().map_err(|e| e.to_string())?;
    Ok(object([
        (
            "calls",
            Json::Array(s.calls.iter().copied().map(Json::from).collect()),
        ),
        (
            "request_ns",
            Json::Array(s.request_ns.iter().copied().map(Json::from).collect()),
        ),
        ("connect_attempts", s.connect_attempts.into()),
        ("connect_ns", s.connect_ns.into()),
    ]))
}
pub fn finish(handle: std::thread::JoinHandle<Result<()>>) -> Result<()> {
    handle.join().map_err(|_| "native service panic")?
}

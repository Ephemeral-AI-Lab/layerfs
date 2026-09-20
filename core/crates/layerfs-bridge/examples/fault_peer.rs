//! External authenticated malformed peer. Never part of the product executable.
use layerfs_bridge::adapters::native::{connection::connect, pipe::key, protocol::*};
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::var("LAYERFS_ENDPOINT")?.parse()?;
    let private = key(&std::env::var("LAYERFS_PRIVATE_KEY")?)?;
    let server = key(&std::env::var("LAYERFS_SERVER_KEY")?)?;
    for frame in [
        Frame {
            kind: Kind::Body,
            id: 1,
            bytes: vec![1],
        },
        Frame {
            kind: Kind::Begin,
            id: 0,
            bytes: vec![],
        },
        Frame {
            kind: Kind::Begin,
            id: 1,
            bytes: vec![255],
        },
    ] {
        let mut c = connect(endpoint, 1, &private, &server)?;
        c.send.write(&Frame {
            kind: Kind::Hello,
            id: 0,
            bytes: vec![0, 1],
        })?;
        assert_eq!(c.receive.read()?.kind, Kind::Hello);
        c.send.write(&frame)?;
        c.receive
            .deadline(Instant::now() + Duration::from_millis(500));
        if let Ok(reply) = c.receive.read() {
            assert_eq!(reply.kind, Kind::Failure);
        }
        c.receive.close();
    }
    println!("authenticated illegal-state/zero-id/truncated-metadata: PASS (external peer, daemon bypassed)");
    Ok(())
}

//! External authenticated malformed peer. Never part of the product executable.
use layerfs_bridge::adapters::native::{connection::connect, pipe::key, protocol::*};
use std::time::{Duration, Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::var("LAYERFS_ENDPOINT")?.parse()?;
    let private = key(&std::env::var("LAYERFS_PRIVATE_KEY")?)?;
    let server = key(&std::env::var("LAYERFS_SERVER_KEY")?)?;
    let rounds = std::env::var("LAYERFS_FAULT_PEER_ROUNDS")
        .ok()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(1);
    assert!((1..=16).contains(&rounds));
    for round in 0..rounds {
        for (index, frame) in [
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
        ]
        .into_iter()
        .enumerate()
        {
            let mut c = connect(endpoint, 1, &private, &server)
                .map_err(|e| format!("round {round} case {index} connect: {e:?}"))?;
            c.send
                .write(&Frame {
                    kind: Kind::Hello,
                    id: 0,
                    bytes: vec![0, 1],
                })
                .map_err(|e| format!("round {round} case {index} send Hello: {e:?}"))?;
            assert_eq!(
                c.receive
                    .read()
                    .map_err(|e| format!("round {round} case {index} receive Hello: {e:?}"))?
                    .kind,
                Kind::Hello
            );
            c.send
                .write(&frame)
                .map_err(|e| format!("round {round} case {index} send fault: {e:?}"))?;
            c.receive
                .deadline(Instant::now() + Duration::from_millis(500));
            if let Ok(reply) = c.receive.read() {
                assert_eq!(reply.kind, Kind::Failure);
            }
            c.receive.close();
        }
    }
    println!("authenticated illegal-state/zero-id/truncated-metadata: PASS (external peer, daemon bypassed)");
    Ok(())
}

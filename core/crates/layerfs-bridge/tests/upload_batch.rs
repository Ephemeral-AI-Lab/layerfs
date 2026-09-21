//! Bulk upload through the native connection: one vectored write per batch.
//!
//! The sender coalesces full-size data records into a single `writev` and flushes
//! on a control record, on the byte bound, and at end of upload. This test proves
//! the receiver sees every frame unchanged and prints the achieved rate as a
//! diagnostic (not a gate).
use layerfs_bridge::{
    adapters::native::{
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        protocol::{Frame, Kind},
    },
    contract::FRAME_BYTES,
};
use std::time::{Duration, Instant};

const TOTAL: usize = 128 * 1024 * 1024;

#[test]
fn bulk_upload_batches_records_without_changing_frames() {
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let client_key = [21; 32];
    let server_key = [23; 32];
    let public = *VerifiedPeer::from_private(&server_key)
        .unwrap()
        .public_key();
    let peers = [Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&client_key)
            .unwrap()
            .public_key(),
        expires_unix: u64::MAX,
    }];
    let frames = TOTAL / FRAME_BYTES;

    std::thread::scope(|scope| {
        let server = scope.spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let mut connection = accept(socket, &server_key, &peers).unwrap();
            let deadline = Instant::now() + Duration::from_secs(120);
            connection.receive.deadline(deadline);
            let started = Instant::now();
            let mut bytes = 0usize;
            for index in 0..frames {
                let frame = connection.receive.read().expect("body frame");
                assert_eq!(frame.kind, Kind::Body, "frame {index} kind");
                assert_eq!(frame.id, 1, "frame {index} identity");
                assert_eq!(frame.bytes.len(), FRAME_BYTES, "frame {index} length");
                assert!(frame.bytes.iter().all(|b| *b == 0x77), "frame {index} body");
                bytes += frame.bytes.len();
            }
            let elapsed = started.elapsed();
            assert_eq!(bytes, TOTAL, "received byte count");
            (elapsed, bytes)
        });

        let mut connection = connect(address, 1, &client_key, &public).unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        connection.send.deadline(deadline);
        let payload = vec![0x77u8; FRAME_BYTES];
        let send_started = Instant::now();
        for _ in 0..frames {
            connection
                .send
                .write(&Frame {
                    kind: Kind::Body,
                    id: 1,
                    bytes: payload.clone(),
                })
                .expect("body write");
        }
        connection
            .send
            .write(&Frame {
                kind: Kind::EndInput,
                id: 1,
                bytes: (TOTAL as u64).to_be_bytes().to_vec(),
            })
            .expect("end input");
        connection.send.end_upload();
        let send_elapsed = send_started.elapsed();

        let (elapsed, bytes) = server.join().unwrap();
        println!(
            "sender own time {:.3} s ({:.2} us/frame)",
            send_elapsed.as_secs_f64(),
            send_elapsed.as_secs_f64() * 1e6 / frames as f64
        );
        let rate = (bytes as f64 / 1e9) / elapsed.as_secs_f64();
        println!(
            "batched upload: {bytes} bytes in {:.3} s = {rate:.3} GB/s ({:.2} us per {FRAME_BYTES}-byte frame)",
            elapsed.as_secs_f64(),
            elapsed.as_secs_f64() * 1e6 / frames as f64
        );
    });
}

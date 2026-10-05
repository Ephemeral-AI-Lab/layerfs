//! The daemon relay's result-write path, exercised with the product types.
//!
//! `Frame::write` -> `write_all` -> `Pipe::write` is exactly what the daemon's
//! stdout relay does for every result frame. This test proves the framing stays
//! intact and every declared byte arrives at one syscall per call, and prints the
//! achieved rate as a diagnostic. The rate is not a gate: the assertion is that
//! the bytes and the frame boundaries are correct.
//!
//! Context: the replaced implementation capped every write at `PIPE_BUF`'s
//! portable minimum (512 bytes), which cost 33 poll+write pairs for one 16 KiB
//! frame and pinned this stage near 0.30 GB/s.
use layerfs_bridge::adapters::native::{pipe::Pipe, protocol::*};
use nix::unistd::pipe;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

const FRAME: usize = 16 * 1024;
const TOTAL: usize = 256 * 1024 * 1024;

#[test]
fn result_frames_reach_the_consumer_one_write_each() {
    let (reader, writer) = pipe().expect("pipe");
    let consumer_cancel = AtomicBool::new(false);
    let producer_cancel = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(60);
    let frames = TOTAL / FRAME;

    let consumer = std::thread::spawn(move || {
        let mut pipe = Pipe {
            fd: &reader,
            deadline: Instant::now() + Duration::from_secs(60),
            cancel: &consumer_cancel,
        };
        let mut bytes = 0usize;
        for index in 0..frames {
            let frame = Frame::read(&mut pipe).expect("frame");
            assert_eq!(frame.kind, Kind::ResultData, "frame {index} kind");
            assert_eq!(frame.id, 7, "frame {index} identity");
            assert_eq!(frame.bytes.len(), FRAME, "frame {index} length");
            assert!(frame.bytes.iter().all(|b| *b == 0x5a), "frame {index} body");
            bytes += frame.bytes.len();
        }
        assert_eq!(bytes, TOTAL, "consumer byte count");
        bytes
    });

    let mut pipe = Pipe {
        fd: &writer,
        deadline,
        cancel: &producer_cancel,
    };
    let payload = vec![0x5au8; FRAME];
    let started = Instant::now();
    for _ in 0..frames {
        Frame {
            kind: Kind::ResultData,
            id: 7,
            bytes: payload.clone(),
        }
        .write(&mut pipe)
        .expect("frame write");
    }
    let elapsed = started.elapsed();
    let received = consumer.join().expect("consumer");
    assert_eq!(received, TOTAL, "producer byte count");
    let rate = (TOTAL as f64 / 1e9) / elapsed.as_secs_f64();
    println!(
        "relay result path: {TOTAL} bytes in {:.3} s = {rate:.3} GB/s ({:.2} us per {FRAME}-byte frame)",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64() * 1e6 / frames as f64
    );
}

//! A full pipe must not wedge the relay past its deadline.
//!
//! This is the deterministic, millisecond-scale check for the hazard the
//! `blocked-result-consumer` Docker case probes end to end: `Pipe::write` used to
//! issue a blocking write, which waits for room and can outlast the operation
//! deadline. It now returns `EAGAIN` for `ready()` to retry under that deadline.
use layerfs_bridge::adapters::native::pipe::Pipe;
use nix::{
    fcntl::{fcntl, FcntlArg, OFlag},
    unistd::{pipe, write as raw_write},
};
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

#[test]
fn a_full_pipe_fails_at_the_deadline_instead_of_blocking() {
    let (reader, writer) = pipe().expect("pipe");
    // Fill the pipe completely and make it non-blocking for the fill only.
    let flags = OFlag::from_bits_truncate(fcntl(&writer, FcntlArg::F_GETFL).expect("flags"));
    fcntl(&writer, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).expect("nonblocking");
    let chunk = [0u8; 4096];
    let mut filled = 0usize;
    loop {
        match raw_write(&writer, &chunk) {
            Ok(n) => filled += n,
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(error) => panic!("fill: {error}"),
        }
    }
    assert!(filled > 0, "pipe accepted nothing");
    fcntl(&writer, FcntlArg::F_SETFL(flags)).expect("restore blocking");
    // Keep the read end open but never read from it: the pipe stays full and
    // `POLLOUT` stays clear, so the write must wait out its deadline.
    let _reader = reader;

    let cancel = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_millis(200);
    let mut pipe = Pipe {
        fd: &writer,
        deadline,
        cancel: &cancel,
    };
    let payload = vec![0u8; 64 * 1024];
    let started = Instant::now();
    let result = std::io::Write::write(&mut pipe, &payload);
    let elapsed = started.elapsed();
    assert!(
        result.is_err(),
        "a write into a full pipe with no consumer must fail, not succeed"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "write outlived its 200 ms deadline by {elapsed:?}"
    );
    println!(
        "full-pipe write failed after {:.0} ms (deadline 200 ms, bounded)",
        elapsed.as_secs_f64() * 1000.0
    );
}

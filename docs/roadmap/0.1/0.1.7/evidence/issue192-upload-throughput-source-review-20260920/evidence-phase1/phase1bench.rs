//! Phase 1 ladder: E2/E3 framing on a real socket, E5 daemon-relay pipe shape.
//! Counts the syscalls each variant issues; no crypto, no Store, no product code.
use std::io::{IoSlice, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;
const FRAME: usize = 16 * KIB; // contract FRAME_BYTES
const RECORD: usize = 256 * KIB; // candidate record size
const TOTAL: u64 = 1024 * MIB as u64; // 1 GiB per variant

fn fill(buf: &mut [u8], seed: u64) {
    let mut s = seed | 1;
    for c in buf.chunks_exact_mut(8) {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        c.copy_from_slice(&s.to_le_bytes());
    }
}

// ---------------------------------------------------------------- E2/E3: socket

#[derive(Clone, Copy, PartialEq)]
enum Variant {
    /// Today's bridge record: setsockopt per I/O + two writes (4-byte prefix is its own segment).
    Record2Setsockopt,
    /// Same writes, timeouts set once (F3).
    Record2,
    /// One writev per frame + one read per frame, timeouts once (F2+F3).
    Record1Vectored,
    /// 256 KiB records, each 4 x 64 KiB chunks, one writev per record (F1+F2+F3).
    Record256k,
    /// tdx1 TCP control shape: one write(16 KiB) per frame.
    Plain16k,
}

fn socket_run(variant: Variant) -> (f64, u64, u64) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let frame = if variant == Variant::Record256k { RECORD } else { FRAME };
    let frames = (TOTAL / frame as u64) as usize;
    let calls = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(2));

    let recv_calls = Arc::clone(&calls);
    let recv_barrier = Arc::clone(&barrier);
    let receiver = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_nodelay(true).unwrap();
        if variant == Variant::Record2Setsockopt {
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            recv_calls.fetch_add(1, Ordering::Relaxed);
        }
        let mut buf = vec![0u8; frame + 4];
        recv_barrier.wait();
        let mut got = 0u64;
        while got < TOTAL {
            match variant {
                Variant::Record2Setsockopt | Variant::Record2 => {
                    let mut h = [0u8; 4];
                    if variant == Variant::Record2Setsockopt {
                        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                        recv_calls.fetch_add(1, Ordering::Relaxed);
                    }
                    s.read_exact(&mut h).unwrap();
                    recv_calls.fetch_add(1, Ordering::Relaxed);
                    if variant == Variant::Record2Setsockopt {
                        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                        recv_calls.fetch_add(1, Ordering::Relaxed);
                    }
                    s.read_exact(&mut buf[..frame]).unwrap();
                    recv_calls.fetch_add(1, Ordering::Relaxed);
                }
                Variant::Plain16k => {
                    s.read_exact(&mut buf[..frame]).unwrap();
                    recv_calls.fetch_add(1, Ordering::Relaxed);
                }
                _ => {
                    s.read_exact(&mut buf[..frame + 4]).unwrap();
                    recv_calls.fetch_add(1, Ordering::Relaxed);
                }
            }
            got += frame as u64;
        }
    });

    let mut s = TcpStream::connect(addr).unwrap();
    s.set_nodelay(true).unwrap();
    let data = vec![0x5au8; frame];
    barrier.wait();
    let started = Instant::now();
    match variant {
        Variant::Record2Setsockopt | Variant::Record2 => {
            for _ in 0..frames {
                if variant == Variant::Record2Setsockopt {
                    s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                    calls.fetch_add(1, Ordering::Relaxed);
                }
                s.write_all(&(frame as u32).to_be_bytes()).unwrap();
                calls.fetch_add(1, Ordering::Relaxed);
                if variant == Variant::Record2Setsockopt {
                    s.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                    calls.fetch_add(1, Ordering::Relaxed);
                }
                s.write_all(&data).unwrap();
                calls.fetch_add(1, Ordering::Relaxed);
            }
        }
        Variant::Record1Vectored | Variant::Record256k => {
            let chunks = std::cmp::max(1, frame / (64 * KIB)); // 1 for 16 KiB, 4 for 256 KiB
            let chunk = frame / chunks;
            for _ in 0..frames {
                let head = (frame as u32).to_be_bytes();
                let mut storage = vec![IoSlice::new(&head)];
                for _ in 0..chunks {
                    storage.push(IoSlice::new(&data[..chunk]));
                }
                let mut slices = &mut storage[..];
                while !slices.is_empty() {
                    let n = s.write_vectored(slices).unwrap();
                    calls.fetch_add(1, Ordering::Relaxed);
                    if n == 0 {
                        panic!("write zero");
                    }
                    IoSlice::advance_slices(&mut slices, n);
                }
            }
        }
        Variant::Plain16k => {
            for _ in 0..frames {
                s.write_all(&data).unwrap();
                calls.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    receiver.join().unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    (elapsed, frames as u64, calls.load(Ordering::Relaxed))
}

// ---------------------------------------------------------------- E5: pipe relay

fn poll_ready(fd: &impl AsFd, flags: nix::poll::PollFlags) {
    use nix::poll::{poll, PollFd, PollTimeout};
    let mut fds = [PollFd::new(fd.as_fd(), flags)];
    loop {
        match poll(&mut fds, PollTimeout::from(100u16)) {
            Ok(n) if n > 0 => return,
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => panic!("poll: {e}"),
        }
    }
}

/// E5 models the two pipe stages of the product path separately.
/// "stdin-*"  : host driver -> daemon stdin -> `Frame::read` parse (1 B + 19 B + body, poll per read)
/// "stdout-*" : daemon `Pipe::write` -> stdout pipe (poll + write, capped) -> host driver read
fn relay_run(mode: &str) -> (f64, u64, u64) {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    use nix::unistd::{pipe, read, write};
    let (r, w) = pipe().unwrap();
    for fd in [&r, &w] {
        let flags = fcntl(fd, FcntlArg::F_GETFL).unwrap();
        let _ = fcntl(fd, FcntlArg::F_SETFL(OFlag::from_bits_retain(flags) | OFlag::O_NONBLOCK));
    }
    let frames = (TOTAL / FRAME as u64) as usize;
    let calls = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(2));
    let today = mode.ends_with("today");
    let stdin_side = mode.starts_with("stdin");
    let data = vec![0x33u8; FRAME];
    let mut header = [0u8; 20];
    header[..4].copy_from_slice(&(FRAME as u32).to_be_bytes());

    let prod_calls = Arc::clone(&calls);
    let prod_barrier = Arc::clone(&barrier);
    let producer = std::thread::spawn(move || {
        // writer rule: stdin side = ordinary large writes; stdout side = daemon Pipe::write cap.
        let cap = if stdin_side { 64 * KIB } else if today { 512 } else { 64 * KIB };
        let header_pieces: Vec<&[u8]> = if stdin_side && today {
            vec![&header[..1], &header[1..20]]
        } else {
            vec![&header[..]]
        };
        for _ in 0..frames {
            for piece in header_pieces.iter().copied().chain(std::iter::once(&data[..])) {
                let mut rest = piece;
                while !rest.is_empty() {
                    let n = rest.len().min(cap);
                    poll_ready(&w, nix::poll::PollFlags::POLLOUT);
                    prod_calls.fetch_add(1, Ordering::Relaxed);
                    match write(&w, &rest[..n]) {
                        Ok(count) => rest = &rest[count..],
                        Err(nix::errno::Errno::EAGAIN) => {}
                        Err(e) => panic!("write: {e}"),
                    }
                }
            }
        }
    });

    let cons_calls = Arc::clone(&calls);
    let cons_barrier = Arc::clone(&barrier);
    let consumer = std::thread::spawn(move || {
        cons_barrier.wait();
        let mut buf = vec![0u8; FRAME + 20];
        let mut got = 0u64;
        while got < TOTAL {
            // reader rule: stdin side = the daemon's Frame::read parse; stdout side = ordinary read.
            let wants: Vec<(usize, usize)> = if stdin_side && today {
                vec![(0, 1), (1, 20), (20, FRAME + 20)]
            } else {
                vec![(0, FRAME + 20)]
            };
            for (from, to) in wants {
                poll_ready(&r, nix::poll::PollFlags::POLLIN);
                cons_calls.fetch_add(1, Ordering::Relaxed);
                let mut filled = from;
                while filled < to {
                    match read(&r, &mut buf[filled..to]) {
                        Ok(0) => panic!("eof"),
                        Ok(n) => filled += n,
                        Err(nix::errno::Errno::EAGAIN) => {}
                        Err(e) => panic!("read: {e}"),
                    }
                }
            }
            got += FRAME as u64;
        }
    });

    barrier.wait();
    let started = Instant::now();
    producer.join().unwrap();
    consumer.join().unwrap();
    (started.elapsed().as_secs_f64(), calls.load(Ordering::Relaxed), frames as u64)
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    if which == "e2" {
        println!("E2/E3 framing ladder — 1 GiB, 127.0.0.1 loopback, nodelay, one sender + one receiver");
        println!("{:<22} {:>9} {:>10} {:>12} {:>14}", "variant", "seconds", "GB/s", "us/frame", "counted syscalls/frame");
        for (name, v) in [
            ("plain-16k (control)", Variant::Plain16k),
            ("record 2 writes+sockopt", Variant::Record2Setsockopt),
            ("record 2 writes", Variant::Record2),
            ("record 1 writev", Variant::Record1Vectored),
            ("record 256k writev", Variant::Record256k),
        ] {
            let (secs, frames, calls) = socket_run(v);
            let frame = if v == Variant::Record256k { RECORD } else { FRAME };
            println!(
                "{name:<22} {secs:>9.3} {:>10.3} {:>12.2} {:>14.2}",
                (TOTAL as f64 / 1e9) / secs,
                secs * 1e6 / frames as f64,
                calls as f64 / frames as f64
            );
        }
    } else if which == "e5" {
        println!("E5 relay ladder — 1 GiB through two pipes, poll-per-syscall both ends");
        println!("{:<28} {:>9} {:>10} {:>12} {:>14}", "variant", "seconds", "GB/s", "us/frame", "syscalls/frame");
        for name in ["stdin-today", "stdin-fixed", "stdout-today", "stdout-fixed"] {
            let (secs, calls, frames) = relay_run(name);
            let frames = frames as f64;
            println!(
                "{name:<28} {secs:>9.3} {:>10.3} {:>12.2} {:>14.2}",
                (TOTAL as f64 / 1e9) / secs,
                secs * 1e6 / frames,
                calls as f64 / frames
            );
        }
    } else {
        eprintln!("usage: phase1bench e2|e5");
    }
}

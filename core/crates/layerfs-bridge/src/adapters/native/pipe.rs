//! Deadline-aware native pipe I/O with cooperative cancellation and bounded writes.
use nix::{
    poll::{poll, PollFd, PollFlags, PollTimeout},
    unistd,
};
use std::{
    io::{self, Read, Write},
    os::fd::AsFd,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
pub struct Pipe<'a, T: AsFd> {
    pub fd: &'a T,
    pub deadline: Instant,
    pub cancel: &'a AtomicBool,
}
impl<T: AsFd> Pipe<'_, T> {
    /// Marks the descriptor non-blocking once, so `ready`'s deadline governs every
    /// wait instead of a blocking syscall outliving it.
    fn nonblocking(&self) -> io::Result<()> {
        use nix::fcntl::{fcntl, FcntlArg, OFlag};
        let flags = fcntl(self.fd, FcntlArg::F_GETFL).map_err(io::Error::other)?;
        let flags = OFlag::from_bits_truncate(flags);
        if flags.contains(OFlag::O_NONBLOCK) {
            return Ok(());
        }
        fcntl(self.fd, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))
            .map(|_| ())
            .map_err(io::Error::other)
    }
    fn ready(&self, flags: PollFlags) -> io::Result<()> {
        let progress = Instant::now() + Duration::from_millis(crate::contract::IO_PROGRESS_MS);
        let deadline = self.deadline.min(progress);
        loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(io::ErrorKind::TimedOut)?;
            let timeout = PollTimeout::try_from(remaining.as_millis().clamp(1, 100))
                .map_err(io::Error::other)?;
            let mut fds = [PollFd::new(self.fd.as_fd(), flags)];
            match poll(&mut fds, timeout) {
                Ok(n) if n > 0 => return Ok(()),
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => {}
                Err(e) => return Err(io::Error::other(e)),
            }
        }
    }
}
impl<T: AsFd> Read for Pipe<'_, T> {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        if b.is_empty() {
            return Ok(0);
        }
        self.ready(PollFlags::POLLIN)?;
        unistd::read(self.fd, b).map_err(io::Error::other)
    }
}
impl<T: AsFd> Write for Pipe<'_, T> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if b.is_empty() {
            return Ok(0);
        }
        // One syscall per call when the consumer has room. `PIPE_BUF` atomicity is
        // unnecessary here: exactly one product writer owns this descriptor (it
        // cannot interleave with itself) and every caller writes through
        // `write_all`, which advances on a short write. The portable 512-byte cap
        // this replaces cost 33 poll+write pairs for one 16 KiB frame (measured
        // 0.30 GB/s against 3.25 GB/s).
        //
        // The descriptor must be non-blocking for that to stay deadline-bounded: a
        // blocking write to a full pipe waits for room and can outlast the
        // operation deadline, which let a blocked result consumer hold a 250 ms
        // deadline past a 3 s bound before this was fixed.
        self.nonblocking()?;
        loop {
            self.ready(PollFlags::POLLOUT)?;
            match unistd::write(self.fd, b) {
                Err(nix::errno::Errno::EAGAIN) => {}
                result => return result.map_err(io::Error::other),
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn key(text: &str) -> Result<[u8; 32], crate::contract::Failure> {
    use crate::contract::Code;
    if text.len() != 64 {
        return Err(Code::InvalidInput.into());
    }
    let mut key = [0; 32];
    for (i, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let digit = |b: u8| match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        };
        key[i] = digit(pair[0]).ok_or(Code::InvalidInput)? * 16
            + digit(pair[1]).ok_or(Code::InvalidInput)?;
    }
    Ok(key)
}

/// Best-effort bounded ordinary diagnostics. Never wait for a pipe consumer.
/// The dedicated stderr description stays nonblocking for other diagnostic
/// writers too; product stdout is a separate descriptor and protocol channel.
pub fn diagnostic(message: &str) {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    if message.len() > 512 {
        return;
    }
    let stderr = io::stderr();
    let Ok(flags) = fcntl(&stderr, FcntlArg::F_GETFL) else {
        return;
    };
    if fcntl(
        &stderr,
        FcntlArg::F_SETFL(OFlag::from_bits_retain(flags) | OFlag::O_NONBLOCK),
    )
    .is_ok()
    {
        let _ = unistd::write(&stderr, message.as_bytes());
    }
}

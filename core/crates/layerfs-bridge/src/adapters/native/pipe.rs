//! Deadline-aware native pipe I/O with cooperative cancellation and bounded writes.
use nix::{
    poll::{poll, PollFd, PollFlags, PollTimeout},
    unistd,
};
use std::{
    io::{self, Read, Write},
    os::fd::AsFd,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
pub struct Pipe<'a, T: AsFd> {
    pub fd: &'a T,
    pub deadline: Instant,
    pub cancel: &'a AtomicBool,
}
impl<T: AsFd> Pipe<'_, T> {
    fn ready(&self, flags: PollFlags) -> io::Result<()> {
        loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let remaining = self
                .deadline
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
        self.ready(PollFlags::POLLOUT)?;
        // POSIX guarantees atomic pipe writes only through PIPE_BUF. 512 is the
        // portable minimum, and no other product writer shares this output pipe.
        unistd::write(self.fd, &b[..b.len().min(512)]).map_err(io::Error::other)
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

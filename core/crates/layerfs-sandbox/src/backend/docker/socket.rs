//! Safe readiness waits separate metadata I/O policy from unlimited command streams.
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use std::{
    io::{self, Read, Write},
    os::{fd::AsFd, unix::net::UnixStream},
    time::Duration,
};
pub(super) struct Socket {
    pub stream: UnixStream,
    pub wait: Option<Duration>,
}
impl Socket {
    pub fn new(stream: UnixStream, wait: Option<Duration>) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self { stream, wait })
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            stream: self.stream.try_clone()?,
            wait: self.wait,
        })
    }
    fn ready(&self, flags: PollFlags) -> io::Result<()> {
        let timeout = match self.wait {
            Some(d) => PollTimeout::try_from(d).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "control I/O wait width")
            })?,
            None => PollTimeout::NONE,
        };
        let mut fds = [PollFd::new(self.stream.as_fd(), flags)];
        let n = poll(&mut fds, timeout).map_err(io::Error::from)?;
        if n == 0 {
            return Err(io::ErrorKind::TimedOut.into());
        }
        if fds[0]
            .revents()
            .is_some_and(|f| f.contains(PollFlags::POLLNVAL))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid runtime socket descriptor",
            ));
        }
        Ok(())
    }
}
impl Read for Socket {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        self.ready(PollFlags::POLLIN)?;
        self.stream.read(out)
    }
}
impl Write for Socket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        self.ready(PollFlags::POLLOUT)?;
        self.stream.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

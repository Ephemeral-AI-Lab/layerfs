//! Safe readiness waits separate metadata I/O policy from unlimited command streams.
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use std::{
    io::{self, Read, Write},
    os::{
        fd::{AsFd, AsRawFd},
        unix::net::UnixStream,
    },
    path::Path,
    time::{Duration, Instant},
};
pub(super) struct Socket {
    pub stream: UnixStream,
    pub wait: Option<Duration>,
    pub deadline: Option<Instant>,
}
impl Socket {
    /// One nonblocking connect attempt; only genuine EINPROGRESS has a completion wait.
    pub fn connect(path: &Path, wait: Duration, deadline: Option<Instant>) -> io::Result<Self> {
        use nix::{
            errno::Errno,
            sys::socket::{connect, socket, AddressFamily, SockFlag, SockType, UnixAddr},
        };
        let address = UnixAddr::new(path).map_err(io::Error::from)?;
        let fd = socket(
            AddressFamily::Unix,
            SockType::Stream,
            SockFlag::empty(),
            None,
        )
        .map_err(io::Error::from)?;
        nix::fcntl::fcntl(
            &fd,
            nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC),
        )
        .map_err(io::Error::from)?;
        let stream = UnixStream::from(fd);
        stream.set_nonblocking(true)?;
        let result = connect(stream.as_raw_fd(), &address);
        let owner = Self {
            stream,
            wait: Some(wait),
            deadline,
        };
        match result {
            Ok(()) => Ok(owner),
            Err(Errno::EINPROGRESS) => {
                owner.ready(PollFlags::POLLOUT)?;
                match owner.stream.take_error()? {
                    Some(error) => Err(error),
                    None => Ok(owner),
                }
            }
            // Unix EAGAIN is failed admission, not a queued/pending connection.
            Err(error) => Err(error.into()),
        }
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            stream: self.stream.try_clone()?,
            wait: self.wait,
            deadline: self.deadline,
        })
    }
    fn ready(&self, flags: PollFlags) -> io::Result<()> {
        let wait = if let Some(deadline) = self.deadline {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))?;
            Some(self.wait.map_or(remaining, |d| d.min(remaining)))
        } else {
            self.wait
        };
        let timeout = match wait {
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

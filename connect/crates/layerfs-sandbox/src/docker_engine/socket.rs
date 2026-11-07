//! Safe readiness waits separate metadata I/O policy from unlimited command streams.
#[cfg(unix)]
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
#[cfg(unix)]
use std::{
    io::{self, Read, Write},
    os::{
        fd::{AsFd, AsRawFd},
        unix::net::UnixStream,
    },
    path::Path,
    time::{Duration, Instant},
};

/// One Engine connection: on unix a nonblocking AF_UNIX stream polled for
/// readiness; on Windows the Engine's byte-mode named pipe, readiness via
/// PeekNamedPipe. Both carry the caller's control wait and deadline.
#[cfg(unix)]
pub(crate) struct Socket {
    pub stream: UnixStream,
    pub wait: Option<Duration>,
    pub deadline: Option<Instant>,
}
#[cfg(unix)]
impl Socket {
    /// Half or full close of the stream, as std's Shutdown vocabulary.
    pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        self.stream.shutdown(how)
    }
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
    /// Whether an idle kept connection is still quiet: nothing to read, no
    /// hangup or error. A zero-wait poll; it sends nothing.
    pub fn quiet(&self) -> bool {
        let mut fds = [PollFd::new(self.stream.as_fd(), PollFlags::POLLIN)];
        matches!(poll(&mut fds, PollTimeout::ZERO), Ok(0))
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
#[cfg(unix)]
impl Read for Socket {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        self.ready(PollFlags::POLLIN)?;
        self.stream.read(out)
    }
}
#[cfg(unix)]
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

/// The Windows Engine endpoint: a byte-mode named pipe opened as a file.
/// The crate keeps deny(unsafe_code); this one audited FFI module opts in
/// (the control crate's lock surface keeps the same shape). Readiness is a
/// bounded PeekNamedPipe; a write's backpressure is the pipe's own blocking,
/// pending an overlapped-IO upgrade with real write timeouts.
#[cfg(windows)]
mod windows_pipe {
    #![allow(unsafe_code)]
    use std::{
        fs::File,
        io::{self, Read, Write},
        os::windows::io::AsRawHandle,
        path::Path,
        thread,
        time::{Duration, Instant},
    };

    #[allow(non_snake_case)]
    extern "system" {
        #[link_name = "PeekNamedPipe"]
        fn PeekNamedPipe(
            pipe: usize,
            buffer: *mut u8,
            size: u32,
            read: *mut u32,
            available: *mut u32,
            left: *mut u32,
        ) -> u32;
    }

    pub(crate) struct Socket {
        pub stream: File,
        pub wait: Option<Duration>,
        pub deadline: Option<Instant>,
    }

    impl Socket {
        /// One open attempt of the named pipe. A busy pipe (the Engine
        /// serving another exclusive client) is a failed admission - the
        /// same one-attempt discipline as the unix branch.
        pub fn connect(path: &Path, wait: Duration, deadline: Option<Instant>) -> io::Result<Self> {
            let stream = File::options().read(true).write(true).open(path)?;
            Ok(Self {
                stream,
                wait: Some(wait),
                deadline,
            })
        }

        /// Half or full close. A byte-mode pipe has no half-close; the
        /// handle close that follows a drop is the full close, so every
        /// Shutdown request maps to the same no-op here.
        pub fn shutdown(&self, _how: std::net::Shutdown) -> io::Result<()> {
            Ok(())
        }

        /// Whether an idle kept connection is still quiet: nothing to read.
        /// It sends nothing; a broken pipe reads as unquiet.
        pub fn quiet(&self) -> bool {
            matches!(self.available(), Ok(0))
        }

        pub fn try_clone(&self) -> io::Result<Self> {
            Ok(Self {
                stream: self.stream.try_clone()?,
                wait: self.wait,
                deadline: self.deadline,
            })
        }

        fn available(&self) -> io::Result<u32> {
            let mut bytes: u32 = 0;
            let ok = unsafe {
                PeekNamedPipe(
                    self.stream.as_raw_handle() as usize,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut bytes,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(bytes)
        }

        fn wait_ready(&self) -> io::Result<()> {
            let budget = self.wait.unwrap_or(Duration::from_millis(50));
            let end = self
                .deadline
                .map(|d| d.min(Instant::now() + budget))
                .unwrap_or_else(|| Instant::now() + budget);
            loop {
                if self.available()? > 0 {
                    return Ok(());
                }
                if Instant::now() >= end {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    impl Read for Socket {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if out.is_empty() {
                return Ok(0);
            }
            self.wait_ready()?;
            self.stream.read(out)
        }
    }
    impl Write for Socket {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.is_empty() {
                return Ok(0);
            }
            self.stream.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}

#[cfg(windows)]
pub(crate) use windows_pipe::Socket;

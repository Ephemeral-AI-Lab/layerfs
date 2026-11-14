//! Bounded system-name resolution without a detached resolver thread.
use std::io::{self, Read};
use std::net::{IpAddr, SocketAddr};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Resolves one address through the host's system resolver before `deadline`.
/// Numeric IPs need no helper. On macOS/Linux, a bounded system resolver child
/// is drained, killed and reaped on every exit. No shell or retry is used.
/// Under a Sandbox runtime layer (`sandbox_runtime`) the resolver is the
/// bundle's `getent`, run by the bundle's loader.
/// The original hostname must still be used for TLS identity verification.
pub fn resolve_within(host: &str, port: u16, deadline: Instant) -> io::Result<SocketAddr> {
    resolve_all_within(host, port, deadline)?
        .into_iter()
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "system resolver returned no usable address",
            )
        })
}

/// Resolves all system-returned addresses, in order and without duplicates.
/// Each caller decides its own one-attempt or multi-address connection policy.
pub fn resolve_all_within(host: &str, port: u16, deadline: Instant) -> io::Result<Vec<SocketAddr>> {
    if Instant::now() >= deadline {
        return Err(timeout());
    }
    if let Ok(address) = host.parse::<IpAddr>() {
        return Ok(vec![SocketAddr::new(address, port)]);
    }
    resolve_name(host, port, deadline)
}

#[cfg(unix)]
fn resolve_name(host: &str, port: u16, deadline: Instant) -> io::Result<Vec<SocketAddr>> {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    let mut command = if cfg!(target_os = "macos") {
        let mut command = Command::new("/usr/bin/dscacheutil");
        command.args(["-q", "host", "-a", "name", host]);
        command
    } else if cfg!(target_os = "linux") {
        // a daemon in a namespaces Sandbox runs the bundle's `getent` through
        // the bundle's loader: the image may have neither (alpine, distroless)
        let runtime = crate::sandbox_runtime::from_environment();
        let mut command = runtime
            .and_then(|runtime| {
                let getent = runtime.join("bin").join("getent");
                crate::sandbox_runtime::command(&runtime, getent)
            })
            .unwrap_or_else(|| Command::new("/usr/bin/getent"));
        command.args(["ahosts", host]);
        command
    } else {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "bounded system resolver unavailable on this platform",
        ));
    };
    let mut child = Resolver(
        command
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut output = child
        .0
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("resolver stdout unavailable"))?;
    fcntl(&output, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).map_err(io::Error::from)?;
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        if Instant::now() >= deadline {
            return Err(timeout());
        }
        drain(&mut output, &mut bytes, &mut buffer)?;
        if let Some(status) = child.0.try_wait()? {
            drain(&mut output, &mut bytes, &mut buffer)?;
            if !status.success() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "system resolver found no address",
                ));
            }
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| io::Error::other("invalid resolver output"))?;
            let mut addresses = Vec::new();
            for address in text.lines().filter_map(|line| {
                let word = if cfg!(target_os = "macos") {
                    line.strip_prefix("ip_address:")
                        .or_else(|| line.strip_prefix("ipv6_address:"))?
                        .trim()
                } else {
                    line.split_whitespace().next()?
                };
                word.parse::<IpAddr>().ok()
            }) {
                let socket = SocketAddr::new(address, port);
                if !addresses.contains(&socket) {
                    addresses.push(socket);
                }
            }
            if addresses.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "system resolver returned no usable address",
                ));
            }
            return Ok(addresses);
        }
        std::thread::sleep(
            Duration::from_millis(2).min(deadline.saturating_duration_since(Instant::now())),
        );
    }
}

#[cfg(not(unix))]
fn resolve_name(_host: &str, _port: u16, _deadline: Instant) -> io::Result<Vec<SocketAddr>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "bounded system resolver unavailable on this platform",
    ))
}

fn timeout() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "DNS resolution deadline exceeded")
}
struct Resolver(Child);
impl Drop for Resolver {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(unix)]
fn drain(output: &mut impl Read, bytes: &mut Vec<u8>, buffer: &mut [u8]) -> io::Result<()> {
    loop {
        match output.read(buffer) {
            Ok(0) => return Ok(()),
            Ok(count) => {
                if bytes.len() + count > 65536 {
                    return Err(io::Error::other("resolver output limit exceeded"));
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

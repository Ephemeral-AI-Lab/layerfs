//! macOS mount facts the process runtime reads from `mountinfo` on Linux: the
//! mount table (`mount`) and a forced unmount (`umount -f`). Each tool is one
//! run with a fixed path and the C locale, read once; nothing is retried.
//! An inspection read's table ([`mount_table`]) is the same tool under a
//! deadline (layerfs-project#178 security rule 8).
//! The process facts (`proc_pidinfo`, `sysctl`) are `layerfs_os::process`
//! (layerfs-project#112), below the drivers and the home.
use layerfs_os::process::Retained;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::LazyLock,
    time::{Duration, Instant},
};

const MOUNT: &str = "/sbin/mount";
const UMOUNT: &str = "/sbin/umount";
const UNMOUNT_WAIT: Duration = Duration::from_secs(60);
const REAP_WAIT: Duration = Duration::from_secs(5);
const OUTPUT: usize = 2048;

/// Children a `umount` KILL did not release. Their exits
/// are reaped by the shared retained queue
/// ([`layerfs_os::process::Retained`]), never by a second wait here.
static RETAINED: LazyLock<Retained> = LazyLock::new(Retained::default);

/// One tool's standard output, or its failure with what it said.
fn output(program: &str, args: &[&str]) -> io::Result<(bool, String)> {
    let out = Command::new(program)
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .output()?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

/// Every mount point now, from `mount`'s `<what> on <point> (<options>)`.
pub(crate) fn mount_points() -> io::Result<Vec<PathBuf>> {
    let (ran, text) = output(MOUNT, &[])?;
    if !ran {
        return Err(io::Error::other("mount listing failed"));
    }
    Ok(text.lines().filter_map(mount_point).collect())
}

fn mount_point(line: &str) -> Option<PathBuf> {
    mount_line(line).map(|(point, _)| point)
}

/// One line of `mount`, `<what> on <point> (<type>, <options>)`: the mount
/// point and its filesystem type.
pub(crate) fn mount_line(line: &str) -> Option<(PathBuf, String)> {
    let (_, rest) = line.split_once(" on ")?;
    let (point, said) = rest.rsplit_once(" (")?;
    let fs = said.split([',', ')']).next().unwrap_or("").trim();
    Some((PathBuf::from(point), fs.to_owned()))
}

/// What `mount` prints, for an inspection read: the shared bounded listing
/// ([`layerfs_os::process::mount_table`]). An error means the caller
/// reads without a table.
pub(crate) use layerfs_os::process::mount_table;

/// Forces the mount at `point` off once; a point no longer mounted is done.
pub(crate) fn force_unmount(point: &Path) -> io::Result<()> {
    RETAINED.reap();
    let mut child = Command::new(UMOUNT)
        .arg("-f")
        .arg(point)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child.stderr.take().and_then(|pipe| {
        // If nonblocking setup fails, drop the pipe rather than risk an
        // unbounded read while stopping a Sandbox.
        fcntl(&pipe, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))
            .ok()
            .map(|_| pipe)
    });
    let mut output = Vec::new();
    let deadline = Instant::now() + UNMOUNT_WAIT;
    let status = loop {
        unmount_output(&mut stderr, &mut output);
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "umount exceeded 60s",
                ))
            }
            Err(error) => break Err(error),
        }
    };
    let status = match status {
        Ok(status) => status,
        Err(error) => {
            let pid = child.id();
            let kill = match child.kill() {
                Ok(()) => "it was killed".to_owned(),
                Err(error) => format!("killing it failed: {error}"),
            };
            let deadline = Instant::now() + REAP_WAIT;
            let reaped = loop {
                unmount_output(&mut stderr, &mut output);
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Ok(None) | Err(_) => break None,
                }
            };
            let disposition = match reaped {
                Some(status) => {
                    RETAINED.record(pid, status);
                    format!("exit reaped ({status})")
                }
                None => {
                    RETAINED.retain(child);
                    "exit not observed after 5s; child custody retained (possible kernel wait)"
                        .into()
                }
            };
            return Err(io::Error::new(
                error.kind(),
                format!(
                    "umount -f {}: {error}; process {pid}: {kill}; {disposition}: {}",
                    point.display(),
                    String::from_utf8_lossy(&output).trim()
                ),
            ));
        }
    };
    unmount_output(&mut stderr, &mut output);
    if status.success() || !mount_points()?.iter().any(|p| p == point) {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "umount -f {}: {status}: {}",
        point.display(),
        String::from_utf8_lossy(&output).trim()
    )))
}

fn unmount_output(stderr: &mut Option<std::process::ChildStderr>, output: &mut Vec<u8>) {
    let Some(pipe) = stderr else { return };
    let mut bytes = [0; OUTPUT];
    if let Ok(count) = pipe.read(&mut bytes) {
        output.extend_from_slice(&bytes[..count.min(OUTPUT - output.len())]);
    }
}

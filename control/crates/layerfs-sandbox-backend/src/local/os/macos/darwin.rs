//! macOS mount facts the native runtime reads from `mountinfo` on Linux: the
//! mount table (`mount`) and a forced unmount (`umount -f`). Each tool is one
//! run with a fixed path and the C locale, read once; nothing is retried.
//! The process facts (`proc_pidinfo`, `sysctl`) are `layerfs_bridge::process`
//! (layerfs-project#112), below the drivers and the home.
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

const MOUNT: &str = "/sbin/mount";
const UMOUNT: &str = "/sbin/umount";
const UNMOUNT_WAIT: Duration = Duration::from_secs(60);
const REAP_WAIT: Duration = Duration::from_secs(5);
const OUTPUT: usize = 2048;

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
    let (_, rest) = line.split_once(" on ")?;
    let (point, _) = rest.rsplit_once(" (")?;
    Some(PathBuf::from(point))
}

static RETAINED: Mutex<Vec<Child>> = Mutex::new(Vec::new());
static REAPING: AtomicBool = AtomicBool::new(false);

fn reap_retained() -> bool {
    let mut held = RETAINED.lock().unwrap_or_else(|error| error.into_inner());
    held.retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_)) | Err(_)));
    held.is_empty()
}

/// Forces the mount at `point` off once; a point no longer mounted is done.
pub(crate) fn force_unmount(point: &Path) -> io::Result<()> {
    reap_retained();
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
            let kill = child.kill();
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
                Some(status) => format!("exit reaped ({status})"),
                None => {
                    RETAINED
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(child);
                    if !REAPING.swap(true, Ordering::AcqRel)
                        && std::thread::Builder::new()
                            .name("layerfs-darwin-reap".into())
                            .spawn(|| loop {
                                if reap_retained() {
                                    REAPING.store(false, Ordering::Release);
                                    if reap_retained() || REAPING.swap(true, Ordering::AcqRel) {
                                        break;
                                    }
                                }
                                std::thread::sleep(Duration::from_millis(20));
                            })
                            .is_err()
                    {
                        REAPING.store(false, Ordering::Release);
                    }
                    "exit not observed after 5s; child custody retained (possible kernel wait)"
                        .into()
                }
            };
            return Err(io::Error::new(
                error.kind(),
                format!(
                    "umount -f {}: {error}; pid={pid}; KILL={kill:?}; {disposition}: {}",
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

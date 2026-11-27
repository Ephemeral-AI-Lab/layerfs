//! The macOS mount table under a deadline: one `/sbin/mount` run with a fixed
//! path, the C locale and no input. The daemon's NFS route (Attach tracking,
//! failed-unmount checks, reclaim) and the macOS driver's inspection reads
//! share it (layerfs-project#178 security rule 8, layerfs-project#136).
//! `mount` lists the kernel's table as it is (`getfsstat` with `MNT_NOWAIT`)
//! and asks no mounted server, so it is not expected to wait; the deadline is
//! for a host where it does.
use super::process::Retained;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::{
    io::{self, Read},
    process::{Command, Stdio},
    sync::LazyLock,
    time::{Duration, Instant},
};

const MOUNT: &str = "/sbin/mount";
/// How long one listing waits for `mount` to print the table.
const TABLE_WAIT: Duration = Duration::from_millis(500);
/// How long it then waits for a `mount` it killed to end.
const TABLE_REAP: Duration = Duration::from_millis(500);
/// The most of the table that is read.
const TABLE: usize = 1 << 20;
const PAUSE: Duration = Duration::from_millis(2);

/// Listings a KILL did not release; the shared retained queue reaps them.
static RETAINED: LazyLock<Retained> = LazyLock::new(Retained::default);

/// What `mount` prints. A run that takes longer than [`TABLE_WAIT`] is
/// killed and is an error, as is one that fails; a killed run whose exit is
/// not seen within [`TABLE_REAP`] goes to the retained queue.
pub fn mount_table() -> io::Result<String> {
    RETAINED.reap();
    let mut child = Command::new(MOUNT)
        .env_clear()
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    // a pipe that cannot be read without waiting is not read
    let mut pipe = child.stdout.take().and_then(|pipe| {
        fcntl(&pipe, FcntlArg::F_SETFL(OFlag::O_NONBLOCK))
            .ok()
            .map(|_| pipe)
    });
    let mut text = Vec::new();
    let mut read = |text: &mut Vec<u8>| {
        let mut bytes = [0; 4096];
        while let Some(Ok(count @ 1..)) = pipe.as_mut().map(|pipe| pipe.read(&mut bytes)) {
            text.extend_from_slice(&bytes[..count.min(TABLE.saturating_sub(text.len()))]);
        }
    };
    let deadline = Instant::now() + TABLE_WAIT;
    let ended = loop {
        read(&mut text);
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(PAUSE),
            Ok(None) => {
                break Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "mount took too long",
                ))
            }
            Err(error) => break Err(error),
        }
    };
    match ended {
        Ok(status) if status.success() => {
            read(&mut text);
            Ok(String::from_utf8_lossy(&text).into_owned())
        }
        Ok(status) => Err(io::Error::other(format!("mount listing failed: {status}"))),
        Err(error) => {
            let pid = child.id();
            let _ = child.kill();
            let deadline = Instant::now() + TABLE_REAP;
            let reaped = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(PAUSE),
                    Ok(None) | Err(_) => break None,
                }
            };
            match reaped {
                Some(status) => RETAINED.record(pid, status),
                None => RETAINED.retain(child),
            }
            Err(error)
        }
    }
}

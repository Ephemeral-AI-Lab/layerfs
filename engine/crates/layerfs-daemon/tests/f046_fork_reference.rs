//! F-046: a process forked by another thread of the test process copies the
//! whole descriptor table. Until that child's exec closes its close-on-exec
//! copies, a mount file this thread had open still references the mount, and
//! the plain unmount gets the kernel's reversible EBUSY with no product fault.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/host_install.rs"]
mod support;
use layerfs_bridge::control::{ControlCode, MountPhase};
use layerfs_daemon::control::Failure;
use mutating::{harness, quiet, unmounted};
use nix::libc;
use std::{
    fs::{self, File},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::process::CommandExt,
    },
    path::Path,
    process::{Command, Stdio},
};

fn pipe() -> (OwnedFd, OwnedFd) {
    let mut ends = [0 as RawFd; 2];
    assert_eq!(
        unsafe { libc::pipe2(ends.as_mut_ptr(), libc::O_CLOEXEC) },
        0
    );
    unsafe { (OwnedFd::from_raw_fd(ends[0]), OwnedFd::from_raw_fd(ends[1])) }
}
/// One bounded read of the forked child's pid; the child writes it from
/// between fork and exec.
fn forked_pid(from: &OwnedFd) -> libc::pid_t {
    let mut poll = libc::pollfd {
        fd: from.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(
        unsafe { libc::poll(&mut poll, 1, 10_000) },
        1,
        "fork signal"
    );
    let mut pid = [0u8; 4];
    let read = unsafe { libc::read(from.as_raw_fd(), pid.as_mut_ptr().cast(), 4) };
    assert_eq!(read, 4);
    libc::pid_t::from_ne_bytes(pid)
}
/// Paths the process's descriptors name, as the kernel reports them.
fn descriptors(pid: libc::pid_t) -> Vec<String> {
    fs::read_dir(format!("/proc/{pid}/fd"))
        .unwrap()
        .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
        .map(|target| target.display().to_string())
        .collect()
}

#[test]
fn a_descriptor_copied_by_another_threads_fork_keeps_the_plain_unmount_busy() {
    let (f, h) = harness("-f046");
    let ready = h.mount(3);
    let root = Path::new(&ready.directory);
    let path = root.join("held");
    fs::write(&path, b"held").unwrap();
    let (signal_read, signal_write) = pipe();
    let (release_read, release_write) = pipe();
    let (signal, release, release_copy) = (
        signal_write.as_raw_fd(),
        release_read.as_raw_fd(),
        release_write.as_raw_fd(),
    );

    // This thread holds a mount file for a moment, as `fs::read` or a
    // tree walk does. Another thread forks meanwhile: an ordinary spawn whose
    // child, like every child between fork and exec, holds the copied table.
    let file = File::open(&path).unwrap();
    let spawner = std::thread::spawn(move || {
        let mut command = Command::new("true");
        command.stdin(Stdio::null());
        // Only async-signal-safe calls. The child drops its copy of the
        // release write end, reports its pid, and waits for the release
        // (or end of file, should the test thread unwind).
        unsafe {
            command.pre_exec(move || {
                libc::close(release_copy);
                let pid = libc::getpid().to_ne_bytes();
                libc::write(signal, pid.as_ptr().cast(), 4);
                let mut byte = 0u8;
                libc::read(release, (&mut byte as *mut u8).cast(), 1);
                Ok(())
            });
        }
        command.spawn().unwrap().wait().unwrap()
    });
    let child = forked_pid(&signal_read);
    drop(file);
    let held = path.display().to_string();
    let copies = descriptors(child);
    assert!(copies.contains(&held), "child {child} holds {copies:?}");
    quiet(&h, ready.token);

    // The test's own thread holds nothing: the copy alone keeps it busy.
    match h.try_unmount(ready.token) {
        Err(Failure::Kernel(failure)) => {
            assert_eq!(
                (failure.code, failure.phase),
                (ControlCode::Busy, "unmount:kernel")
            );
            println!("F046_REPRODUCED child={child} copies={copies:?} failure={failure:?}");
        }
        other => panic!("expected the kernel's Busy: {other:?}"),
    }
    let kernel = h.status(ready.token).kernel.unwrap();
    assert_eq!(kernel.phase, MountPhase::Ready);
    assert!(!kernel.detached);

    // Once the child's exec closes the copy, the same mount unmounts normally.
    drop(release_write);
    assert!(spawner.join().unwrap().success());
    let receipt = unmounted(&h, &ready);
    println!("F046_AFTER_EXEC receipt={receipt}");
    h.stop();
    f.cleanup();
}

/// The gate is taken once per thread: a second Harness on the holding thread
/// shares it (without that it would wait out the 110 s bound and panic), and
/// a Harness on another thread proceeds once the owning one stops.
#[test]
fn nested_harnesses_on_one_thread_share_the_gate() {
    let (outer_fixture, outer) = harness("-f046-outer");
    let (inner_fixture, inner) = harness("-f046-inner");
    let ready = inner.mount(4);
    unmounted(&inner, &ready);
    inner.stop();
    inner_fixture.cleanup();
    let ready = outer.mount(5);
    let after = std::thread::spawn(|| {
        let (f, h) = harness("-f046-after");
        h.stop();
        f.cleanup();
    });
    unmounted(&outer, &ready);
    outer.stop();
    outer_fixture.cleanup();
    after.join().unwrap();
}

//! Actual getent interruption in an explicitly owned Linux container.
#![cfg(target_os = "linux")]
use layerfs_bridge::network::resolve_all_within;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

struct Resolv(Vec<u8>);
impl Drop for Resolv {
    fn drop(&mut self) {
        std::fs::write("/etc/resolv.conf", &self.0).expect("restore owned container DNS");
    }
}
fn child_getent() -> Option<u32> {
    for task in std::fs::read_dir("/proc/self/task").ok()? {
        let text = std::fs::read_to_string(task.ok()?.path().join("children")).ok()?;
        for value in text.split_whitespace() {
            let pid = value.parse::<u32>().ok()?;
            if std::fs::read_link(format!("/proc/{pid}/exe"))
                .ok()
                .is_some_and(|p| p.file_name() == Some(std::ffi::OsStr::new("getent")))
            {
                return Some(pid);
            }
        }
    }
    None
}
#[test]
#[ignore = "requires disposable root-owned Docker container and isolated DNS"]
fn stalled_actual_getent_is_killed_reaped_and_no_resolver_thread_survives() {
    assert_eq!(
        std::env::var("LAYERFS_OWNED_RESOLVER_CONTAINER").as_deref(),
        Ok("1")
    );
    assert!(
        std::path::Path::new("/.dockerenv").exists(),
        "never change a host DNS configuration"
    );
    let _restore = Resolv(std::fs::read("/etc/resolv.conf").unwrap());
    let dns = std::net::UdpSocket::bind("127.0.0.1:53").unwrap();
    dns.set_read_timeout(Some(Duration::from_millis(10)))
        .unwrap();
    std::fs::write(
        "/etc/resolv.conf",
        b"nameserver 127.0.0.1\noptions attempts:1 timeout:5\n",
    )
    .unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let observed = Arc::new(AtomicU32::new(0));
    let packets = Arc::new(AtomicU32::new(0));
    let (flag, count) = (done.clone(), packets.clone());
    let server = std::thread::spawn(move || {
        let mut bytes = [0u8; 4096];
        while !flag.load(Ordering::Relaxed) {
            if dns.recv_from(&mut bytes).is_ok() {
                count.fetch_add(1, Ordering::Relaxed);
            }
        }
    });
    let (flag, found) = (done.clone(), observed.clone());
    let monitor = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            if let Some(pid) = child_getent() {
                found.store(pid, Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    let started = Instant::now();
    let result = resolve_all_within(
        "doctor-getent-stall.example",
        443,
        started + Duration::from_millis(150),
    );
    done.store(true, Ordering::Relaxed);
    server.join().unwrap();
    monitor.join().unwrap();
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(
        packets.load(Ordering::Relaxed) > 0,
        "real getent reached the owned stalled DNS server"
    );
    let pid = observed.load(Ordering::Relaxed);
    assert_ne!(pid, 0, "independently observed actual getent PID");
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "helper remained live or zombie"
    );
    assert_eq!(
        nix::sys::wait::waitpid(
            nix::unistd::Pid::from_raw(pid as i32),
            Some(nix::sys::wait::WaitPidFlag::WNOHANG)
        ),
        Err(nix::errno::Errno::ECHILD),
        "diagnostic already reaped its helper"
    );
    println!(
        "actual_getent=true dns_packets={} pid_reaped=true watchers_joined=true",
        packets.load(Ordering::Relaxed)
    );
}

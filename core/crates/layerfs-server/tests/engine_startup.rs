//! Real guarded native startup; readiness and reaped EOF shutdown are separate.
use layerfs_bridge::adapters::native::connection::VerifiedPeer;
#[cfg(target_os = "linux")]
use layerfs_storage::Store;
#[cfg(target_os = "linux")]
use layerfs_telemetry::timer::Timing;
use nix::poll::{poll, PollFd, PollFlags};
use std::{
    io::Read,
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "layerfs-engine-startup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(std::fs::canonicalize(path).unwrap())
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

struct OwnedChild(Child);
impl OwnedChild {
    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.0.try_wait().unwrap().is_some() {
                return self.0.wait().unwrap();
            }
            assert!(Instant::now() < deadline, "native child did not exit");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn diagnostic_line(&mut self) -> Vec<u8> {
        use std::os::fd::AsFd;
        let stderr = self.0.stderr.as_mut().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bytes = Vec::with_capacity(4096);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(!remaining.is_zero(), "native startup had no diagnostic");
            let mut fds = [PollFd::new(
                stderr.as_fd(),
                PollFlags::POLLIN | PollFlags::POLLHUP,
            )];
            let milliseconds = u16::try_from(remaining.as_millis().min(100))
                .unwrap()
                .max(1);
            if poll(&mut fds, milliseconds).unwrap() == 0 {
                continue;
            }
            let mut buffer = [0u8; 512];
            let count = stderr.read(&mut buffer).unwrap();
            assert!(
                count != 0,
                "native stderr ended before diagnostic: {bytes:?}"
            );
            assert!(
                bytes.len() + count <= 4096,
                "native diagnostic exceeded bound"
            );
            bytes.extend_from_slice(&buffer[..count]);
            if bytes.contains(&b'\n') {
                return bytes;
            }
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            self.0.kill().unwrap();
        }
        self.0.wait().unwrap();
    }
}

fn native(root: &Temp) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_layerfs-server"));
    command
        .env_clear()
        .current_dir(&root.0)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    command
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").unwrap();
    }
    text
}

#[test]
#[cfg(target_os = "linux")]
fn native_guard_precedes_host_configuration_and_has_no_store_effect_on_invalid_input() {
    let root = Temp::new();
    let mut child = OwnedChild(native(&root).spawn().unwrap());
    let diagnostic = child.diagnostic_line();
    assert!(!child.wait().success());
    let diagnostic = String::from_utf8(diagnostic).unwrap();
    assert!(diagnostic.starts_with("Error:"), "{diagnostic}");
    assert!(diagnostic.contains("InvalidInput"), "{diagnostic}");
    assert!(!diagnostic.contains("bootstrap refused"), "{diagnostic}");
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
}

#[test]
#[cfg(target_os = "linux")]
fn actual_native_binary_opens_real_store_after_guard_and_reaps_on_stdin_eof() {
    let root = Temp::new();
    let path = root.0.join("store.sqlite");
    let (store, _) = Timing::disabled("fixture", |scope| {
        Store::create(&path, Store::default_policy(), scope)
    });
    drop(store.unwrap());
    let peer = VerifiedPeer::from_private(&[19; 32]).unwrap();
    let mut command = native(&root);
    command
        .env("LAYERFS_PRIVATE_KEY", hex(&[23; 32]))
        .env(
            "LAYERFS_PEERS",
            format!("1,{},18446744073709551615,255", hex(peer.public_key())),
        )
        .env("LAYERFS_STORE", &path)
        .env("LAYERFS_LISTEN", "127.0.0.1:0")
        .env("LAYERFS_TELEMETRY", "off");
    let mut child = OwnedChild(command.spawn().unwrap());
    let diagnostic = String::from_utf8(child.diagnostic_line()).unwrap();
    let ready = diagnostic
        .strip_prefix("layerfs-server ready ")
        .expect(&diagnostic);
    let endpoint: std::net::SocketAddr = ready.trim().parse().unwrap();
    assert_eq!(endpoint.ip(), std::net::Ipv4Addr::LOCALHOST);
    assert_ne!(endpoint.port(), 0);
    drop(child.0.stdin.take());
    assert!(child.wait().success(), "native EOF shutdown failed");
    let (opened, _) = Timing::disabled("verify", |scope| Store::open(&path, scope));
    drop(opened.unwrap());
}

#[test]
#[cfg(target_os = "macos")]
fn selected_darwin_native_binary_refuses_before_host_or_store_effects() {
    for configured in [false, true] {
        let root = Temp::new();
        let path = root.0.join("store.sqlite");
        let marker = b"independent unchanged marker; host must never open this Store";
        let mut command = native(&root);
        if configured {
            std::fs::write(&path, marker).unwrap();
            let peer = VerifiedPeer::from_private(&[19; 32]).unwrap();
            command
                .env("LAYERFS_PRIVATE_KEY", hex(&[23; 32]))
                .env(
                    "LAYERFS_PEERS",
                    format!("1,{},18446744073709551615,255", hex(peer.public_key())),
                )
                .env("LAYERFS_STORE", &path)
                .env("LAYERFS_LISTEN", "127.0.0.1:0")
                .env("LAYERFS_HISTORY_CATALOG", root.0.join("history.sqlite"))
                .env("LAYERFS_TELEMETRY", "off");
        }
        let mut child = OwnedChild(command.spawn().unwrap());
        let diagnostic = String::from_utf8(child.diagnostic_line()).unwrap();
        assert!(!child.wait().success());
        assert!(
            diagnostic.starts_with("Error: SQLite bootstrap refused:"),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("SQLite required hard heap limit"),
            "{diagnostic}"
        );
        assert!(diagnostic.contains("Readback"), "{diagnostic}");
        assert!(
            diagnostic.contains("hard_limit_installed: false"),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("observed_hard_heap_limit: Some(0)"),
            "{diagnostic}"
        );
        assert_eq!(
            std::fs::read_dir(&root.0).unwrap().count(),
            usize::from(configured)
        );
        if configured {
            assert_eq!(std::fs::read(&path).unwrap(), marker);
        }
    }
}

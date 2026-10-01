//! Actual native run assembly borrows the established linked-provider engine guard.
use layerfs_storage::{engine::bootstrap_exclusive, StorageError, Store};
use layerfs_telemetry::timer::Timing;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn native_run_uses_guarded_store_and_configured_history_on_supported_provider() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "native_engine_participation_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("LAYERFS_NATIVE_ENGINE_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut b = Vec::new();
        stdout.take(8193).read_to_end(&mut b).unwrap();
        b
    });
    let err = std::thread::spawn(move || {
        let mut b = Vec::new();
        stderr.take(8193).read_to_end(&mut b).unwrap();
        b
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("native guarded child fixed bound");
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    child.wait().unwrap();
    let out = out.join().unwrap();
    let err = err.join().unwrap();
    assert!(out.len() <= 8192 && err.len() <= 8192);
    assert!(
        status.success(),
        "{} {}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    );
    let output = String::from_utf8_lossy(&out);
    assert!(
        output.contains("native engine SUPPORTED")
            || output.contains("native engine NOT_RUN unsupported provider")
    );
    eprintln!("{} {}", output, String::from_utf8_lossy(&err));
}
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            write!(&mut hex, "{byte:02x}").unwrap();
            hex
        })
}
#[test]
fn native_engine_participation_child() {
    if std::env::var("LAYERFS_NATIVE_ENGINE_CHILD").as_deref() != Ok("1") {
        return;
    }
    // SAFETY: one selected fresh process before any SQL or competing worker.
    let guard = match unsafe { bootstrap_exclusive() } {
        Ok(guard) => guard,
        Err(error) => {
            assert!(
                matches!(error.cause(), StorageError::UnsupportedPolicy { .. }),
                "unexpected native startup failure {error}"
            );
            println!(
                "native engine NOT_RUN unsupported provider: stage={:?} cause={} custody={:?}",
                error.stage(),
                error.cause(),
                error.custody()
            );
            return;
        }
    };
    let dir =
        std::env::temp_dir().join(format!("layerfs-native-guarded-run-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("store.sqlite");
    let store = Timing::disabled("create", |s| {
        Store::create_guarded(&path, Store::default_policy(), guard, s.child("create"))
    })
    .0
    .unwrap();
    assert!(std::ptr::eq(store.engine_guard().unwrap(), guard));
    drop(store);
    let private = [7; 32];
    let peer =
        layerfs_bridge::adapters::native::connection::VerifiedPeer::from_private(&private).unwrap();
    std::env::set_var("LAYERFS_PRIVATE_KEY", hex(&private));
    std::env::set_var(
        "LAYERFS_PEERS",
        format!("1,{},18446744073709551615,255", hex(peer.public_key())),
    );
    std::env::set_var("LAYERFS_STORE", &path);
    std::env::set_var("LAYERFS_LISTEN", "127.0.0.1:0");
    std::env::set_var("LAYERFS_TELEMETRY", "off");
    std::env::set_var("LAYERFS_HISTORY_CATALOG", dir.join("history.sqlite"));
    std::env::set_var("LAYERFS_HISTORY_BINDING", "actual guarded history binding");
    std::env::set_var("LAYERFS_HISTORY_CURSOR_KEY", hex(&[11; 32]));
    std::env::set_var("LAYERFS_HISTORY_CREATE", "1");
    std::env::set_var("LAYERFS_HISTORY_INCARNATION", "1");
    // Null stdin ends the ordinary acceptor only after real Store/history assembly.
    layerfs_server::host::run_guarded(guard).unwrap();
    assert!(dir.join("history.sqlite").exists());
    assert_eq!(
        guard.validate().unwrap().hard_heap_limit_bytes,
        32 * 1024 * 1024
    );
    std::fs::remove_dir_all(dir).unwrap();
    println!("native engine SUPPORTED: actual Store/history/run borrowed guard; combined shape/protected/physical UNQUALIFIED");
}

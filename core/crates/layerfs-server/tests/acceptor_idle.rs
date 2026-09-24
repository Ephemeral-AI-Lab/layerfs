//! The composed Server must stay idle when its process has no stdin writer.
use layerfs_server::{HistoryMode, Server, ServerConfig};
use layerfs_telemetry::runtime::Runtime;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn process_cpu() -> Duration {
    let mut usage = std::mem::MaybeUninit::<nix::libc::rusage>::uninit();
    assert_eq!(
        unsafe { nix::libc::getrusage(nix::libc::RUSAGE_SELF, usage.as_mut_ptr()) },
        0
    );
    let usage = unsafe { usage.assume_init() };
    let micros = (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) as u64 * 1_000_000
        + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as u64;
    Duration::from_micros(micros)
}

fn observe_closed_stdin() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-acceptor-idle-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let _temp = Temp(root.clone());
    let server = Server::create(ServerConfig {
        store_path: root.join("store.sqlite"),
        history_path: root.join("history.sqlite"),
        binding_key: b"acceptor-idle".to_vec(),
        incarnation: 1,
        cursor_key: [7; 32],
        history: HistoryMode::Create,
        service_host: "127.0.0.1".into(),
        runtime: Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    server.listen().unwrap();
    let before = process_cpu();
    let started = Instant::now();
    thread::sleep(Duration::from_secs(1));
    let elapsed = started.elapsed();
    let cpu = process_cpu().saturating_sub(before);
    server.shutdown();
    assert!(
        cpu.as_nanos() * 4 < elapsed.as_nanos(),
        "closed-stdin Flag acceptor burned {cpu:?} CPU in {elapsed:?} wall"
    );
}

#[test]
fn flag_acceptor_ignores_closed_stdin() {
    if std::env::var_os("LAYERFS_ACCEPTOR_IDLE_CHILD").is_some() {
        observe_closed_stdin();
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "flag_acceptor_ignores_closed_stdin",
            "--nocapture",
        ])
        .env("LAYERFS_ACCEPTOR_IDLE_CHILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdin.take()); // The child sees a real pipe HUP, not a synthetic flag.
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

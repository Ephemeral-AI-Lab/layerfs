//! The composed Server must stay idle when its process has no stdin writer.
use layerfs_server::{HistoryMode, Server, ServerConfig};
use layerfs_telemetry::runtime::Runtime;
use std::{
    io::{Read, Write},
    net::TcpStream,
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

fn server() -> (Temp, Server) {
    let root = std::env::temp_dir().join(format!(
        "layerfs-acceptor-idle-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let temp = Temp(root.clone());
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
    (temp, server)
}

fn observe_closed_stdin() {
    let (_temp, server) = server();
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
fn default_session_limit_includes_two_readers() {
    let (_temp, server) = server();
    let endpoint = server.listen().unwrap();
    let capacity = layerfs_bridge::contract::session_capacity(2);
    assert_eq!(capacity, 4);
    let mut held = Vec::new();
    for _ in 0..capacity {
        let mut socket = TcpStream::connect(endpoint).unwrap();
        socket.write_all(&1u32.to_be_bytes()).unwrap();
        held.push(socket);
    }
    // Ten acceptor poll periods, still below the five-second handshake limit.
    thread::sleep(Duration::from_secs(1));
    for socket in &held {
        socket.set_nonblocking(true).unwrap();
        let result = socket.peek(&mut [0]);
        assert!(
            result
                .as_ref()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::WouldBlock),
            "held handshake closed early: {result:?}"
        );
    }
    let mut refused = TcpStream::connect(endpoint).unwrap();
    refused
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut byte = [0];
    match refused.read(&mut byte) {
        Ok(0) => {}
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => {}
        result => panic!("excess session was not refused: {result:?}"),
    }
    server.shutdown();
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

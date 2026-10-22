#![cfg(target_os = "macos")]
//! Native Darwin process and runtime-directory Sandbox lifecycle tests.

use layerfs_sandbox::backend::darwin::{
    Darwin, ExecRequest, InputEnd, Sandbox, SandboxDisposition, SandboxRequest,
};
use layerfs_sandbox::CommandIdentity;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::{
    fs::{self, File},
    io::Write,
    net::TcpStream,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Barrier, OnceLock},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const IMAGE: &str = "sha256:66aeb6bbf7fc825686a0e6d8c8486d90c99d03ce81f854008f179a23bdf4097e";
const PORT: u16 = 30421;

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "layerfs-darwin-test-{prefix}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create tempdir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn mock_daemon_binary() -> &'static Path {
    static BIN: OnceLock<PathBuf> = OnceLock::new();
    BIN.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!(
            "layerfs-darwin-mock-daemon-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create mock daemon dir");
        let src = dir.join("mock_daemon.rs");
        let bin = dir.join("layerfs-daemon");
        let code = r#"
use std::{
    env,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process,
    thread,
    time::Duration,
};

fn read_u16_str<'a>(buf: &'a [u8], cursor: &mut usize) -> Option<&'a str> {
    let end_len = cursor.checked_add(2)?;
    if end_len > buf.len() {
        return None;
    }
    let len = u16::from_be_bytes([buf[*cursor], buf[*cursor + 1]]) as usize;
    *cursor = end_len;
    let end_str = cursor.checked_add(len)?;
    if end_str > buf.len() {
        return None;
    }
    let s = std::str::from_utf8(&buf[*cursor..end_str]).ok()?;
    *cursor = end_str;
    Some(s)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || args[1] != "--config" {
        eprintln!("fatal: expected --config <path>");
        process::exit(2);
    }
    let bytes = match fs::read(&args[2]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("fatal: failed to read config: {e}");
            process::exit(2);
        }
    };
    if !bytes.starts_with(b"LFSD\x05") {
        eprintln!("fatal: invalid daemon.setup magic or corrupted configuration");
        process::exit(2);
    }
    let mut cursor = 5usize;
    let Some(listen) = read_u16_str(&bytes, &mut cursor) else {
        eprintln!("fatal: missing listen field");
        process::exit(2);
    };
    let Some(store) = read_u16_str(&bytes, &mut cursor) else {
        eprintln!("fatal: missing store field");
        process::exit(2);
    };
    let Some(overlay) = read_u16_str(&bytes, &mut cursor) else {
        eprintln!("fatal: missing overlay field");
        process::exit(2);
    };
    let Some(mounts) = read_u16_str(&bytes, &mut cursor) else {
        eprintln!("fatal: missing mounts field");
        process::exit(2);
    };
    if store.starts_with("/layerfs-store/")
        || overlay.starts_with("/layerfs-local/")
        || mounts == "/workspaces"
    {
        eprintln!("fatal: container paths were not translated: store={store} overlay={overlay} mounts={mounts}");
        process::exit(2);
    }
    if !Path::new(mounts).is_dir() {
        eprintln!("fatal: mounts directory does not exist: {mounts}");
        process::exit(2);
    }
    let listener = match TcpListener::bind(listen) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("fatal: bind {listen} failed: {e}");
            process::exit(2);
        }
    };
    let local_addr = listener.local_addr().expect("local_addr");
    println!("LAYERFS_DAEMON_LISTEN {local_addr}");
    let _ = std::io::stdout().flush();
    for stream in listener.incoming() {
        match stream {
            Ok(mut s) => {
                let mut buf = [0u8; 16];
                let _ = s.read(&mut buf);
            }
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
}
"#;
        fs::write(&src, code).expect("write mock daemon source");
        let status = Command::new("rustc")
            .arg(&src)
            .arg("-O")
            .arg("-o")
            .arg(&bin)
            .status()
            .expect("invoke rustc for mock daemon");
        assert!(status.success(), "rustc mock daemon failed");
        bin
    })
}

fn encode_lfsd_config() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"LFSD\x05");
    for s in [
        &format!("0.0.0.0:{PORT}"),
        "/layerfs-store/global/store.sqlite",
        "/layerfs-local/overlay/overlay.sqlite",
        "/workspaces",
    ] {
        let len = u16::try_from(s.len()).expect("len");
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(s.as_bytes());
    }
    out.extend_from_slice(&[0x11u8; 32]);
    out.extend_from_slice(&[0x22u8; 32]);
    out.extend_from_slice(&501u32.to_be_bytes());
    out.extend_from_slice(&20u32.to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes());
    out.extend_from_slice(&2000u32.to_be_bytes());
    out.extend_from_slice(&(64u64 * 1024 * 1024).to_be_bytes());
    out.extend_from_slice(&16u32.to_be_bytes());
    out.extend_from_slice(&1024u32.to_be_bytes());
    out.extend_from_slice(&256u64.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.push(0);
    out
}

fn base_request() -> SandboxRequest {
    SandboxRequest {
        image: IMAGE.to_owned(),
        store_volume: "store-vol-main".to_owned(),
        port: PORT,
        labels: vec![
            ("layerfs.role".to_owned(), "sandbox".to_owned()),
            ("layerfs.project".to_owned(), "darwin-test".to_owned()),
        ],
    }
}

#[test]
fn darwin_sandbox_full_lifecycle_and_4mib_1mib_exec_streams() {
    let temp = TempDir::new("lifecycle");
    let darwin = Darwin::new(temp.path().join("sandboxes"), Duration::from_secs(5))
        .expect("darwin client")
        .naming("box-alpha")
        .expect("naming");
    let request = base_request();
    let mut sandbox = darwin
        .create_sandbox(request.clone())
        .expect("create sandbox");
    assert_eq!(
        sandbox.disposition(),
        SandboxDisposition {
            uploaded: false,
            started: false,
            stopped: false,
            deleted: false,
        }
    );

    let mut exe_file = File::open(mock_daemon_binary()).expect("open mock daemon");
    let config = encode_lfsd_config();
    let upload_work = sandbox
        .upload(&mut exe_file, &config)
        .expect("upload daemon and config");
    assert_eq!(upload_work.executable_read, upload_work.executable_bytes);
    assert_eq!(upload_work.tar_encoded, upload_work.tar_bytes);
    assert!(sandbox.disposition().uploaded);

    sandbox.start().expect("start daemon");
    assert!(sandbox.disposition().started);

    let obs = sandbox
        .wait_listener(Duration::from_secs(5))
        .expect("wait_listener");
    assert!(obs.marker_seen);
    assert!(obs.stdout > 0);

    let endpoint = sandbox.endpoint().expect("endpoint");
    assert!(endpoint.ip().is_loopback());
    assert!(endpoint.port() > 0);
    let _conn =
        TcpStream::connect_timeout(&endpoint, Duration::from_secs(2)).expect("connect to daemon");

    let ws_1 = temp.path().join("sandboxes/box-alpha/runtime/workspaces/1");
    fs::create_dir_all(&ws_1).expect("create workspace 1 dir");
    fs::write(ws_1.join("marker.txt"), b"workspace-root-ok\n").expect("write marker");

    let created = sandbox
        .create_exec(ExecRequest {
            identity: CommandIdentity { uid: 501, gid: 20 },
            arguments: vec![
                "/bin/sh".to_owned(),
                "-c".to_owned(),
                concat!(
                    "cat marker.txt; ",
                    "head -c 5 > stdin_captured.txt; ",
                    "dd if=/dev/zero bs=65536 count=64 2>/dev/null | tr '\\0' 'A'; ",
                    "dd if=/dev/zero bs=65536 count=16 2>/dev/null | tr '\\0' 'E' >&2; ",
                    "exit 19"
                )
                .to_owned(),
            ],
            environment: vec!["CUSTOM_KEY=darwin_val".to_owned()],
            directory: "/workspaces/1".to_owned(),
            stdin: true,
        })
        .expect("create_exec");

    let attached = created.start().expect("start exec");
    let (mut input, output, handle) = attached.into_parts().expect("into_parts");
    input.write_all(b"hello").expect("write stdin");
    let end = input.close().expect("close stdin");
    assert_eq!(
        end,
        InputEnd {
            sent: 5,
            saturated: false,
            input_failed: false,
        }
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let progress = output
        .copy_to(&mut stdout, &mut stderr)
        .expect("copy_to output");

    let expected_stdout_len = "workspace-root-ok\n".len() + (4 * 1024 * 1024);
    let expected_stderr_len = 1024 * 1024;
    assert_eq!(stdout.len(), expected_stdout_len);
    assert_eq!(stderr.len(), expected_stderr_len);
    assert_eq!(progress.stdout, expected_stdout_len as u64);
    assert_eq!(progress.stderr, expected_stderr_len as u64);
    assert!(stdout.starts_with(b"workspace-root-ok\nAAAA"));
    assert!(stderr.iter().all(|&b| b == b'E'));
    assert_eq!(
        fs::read(ws_1.join("stdin_captured.txt")).expect("read stdin_captured"),
        b"hello"
    );

    let inspect = handle.inspect().expect("inspect exec");
    assert!(!inspect.running);
    assert_eq!(inspect.known_root_exit(), Some(19));

    sandbox.stop(2).expect("stop sandbox");
    assert!(sandbox.disposition().stopped);
    sandbox.delete().expect("delete sandbox");
    assert!(sandbox.disposition().deleted);
    assert!(!temp.path().join("sandboxes/box-alpha").exists());
}

#[test]
fn darwin_six_concurrent_execs_run_independently() {
    let temp = TempDir::new("concurrent");
    let darwin = Darwin::new(temp.path().join("sandboxes"), Duration::from_secs(5))
        .expect("darwin")
        .naming("box-conc")
        .expect("naming");
    let request = base_request();
    let mut sandbox = darwin.create_sandbox(request).expect("create_sandbox");
    let mut exe_file = File::open(mock_daemon_binary()).expect("open mock daemon");
    sandbox
        .upload(&mut exe_file, &encode_lfsd_config())
        .expect("upload");
    sandbox.start().expect("start");
    sandbox
        .wait_listener(Duration::from_secs(5))
        .expect("wait_listener");

    let ws_1 = temp.path().join("sandboxes/box-conc/runtime/workspaces/1");
    fs::create_dir_all(&ws_1).expect("create workspace 1 dir");

    let barrier = Arc::new(Barrier::new(6));
    let mut created_list = Vec::new();
    for idx in 0..6u32 {
        let created = sandbox
            .create_exec(ExecRequest {
                identity: CommandIdentity { uid: 501, gid: 20 },
                arguments: vec![
                    "/bin/sh".to_owned(),
                    "-c".to_owned(),
                    format!("printf 'out-{idx}\\n'; printf 'err-{idx}\\n' >&2; exit {idx}"),
                ],
                environment: vec![format!("IDX={idx}")],
                directory: "/workspaces/1".to_owned(),
                stdin: false,
            })
            .expect("create_exec");
        created_list.push((idx, created));
    }

    let mut threads = Vec::new();
    for (idx, created) in created_list {
        let b = Arc::clone(&barrier);
        threads.push(thread::spawn(move || {
            b.wait();
            let attached = created.start().expect("start");
            let (_input, output, handle) = attached.into_parts().expect("into_parts");
            let mut out = Vec::new();
            let mut err = Vec::new();
            output.copy_to(&mut out, &mut err).expect("copy_to");
            assert_eq!(out, format!("out-{idx}\n").into_bytes());
            assert_eq!(err, format!("err-{idx}\n").into_bytes());
            let inspection = handle.inspect().expect("inspect");
            assert_eq!(inspection.known_root_exit(), Some(idx as i32));
        }));
    }

    for t in threads {
        t.join().expect("join exec thread");
    }

    sandbox.stop(1).expect("stop");
    sandbox.delete().expect("delete");
}

#[test]
fn darwin_reopen_distinguishes_live_stopped_and_gone() {
    let temp = TempDir::new("reopen");
    let root = temp.path().join("sandboxes");
    let darwin = Darwin::new(&root, Duration::from_secs(5))
        .expect("darwin")
        .naming("box-reopen")
        .expect("naming");
    let request = base_request();
    let mut sandbox = darwin
        .create_sandbox(request.clone())
        .expect("create_sandbox");
    let id = sandbox.identity();
    let mut exe_file = File::open(mock_daemon_binary()).expect("open mock daemon");
    sandbox
        .upload(&mut exe_file, &encode_lfsd_config())
        .expect("upload");
    sandbox.start().expect("start");
    sandbox
        .wait_listener(Duration::from_secs(5))
        .expect("wait_listener");
    let original_endpoint = sandbox.endpoint().expect("endpoint");

    let plain_client = Darwin::new(&root, Duration::from_secs(5)).expect("plain client");
    let reopened =
        Sandbox::reopen(plain_client.clone(), id, request.clone()).expect("reopen live sandbox");
    assert_eq!(reopened.endpoint().expect("endpoint"), original_endpoint);

    let state_file = fs::read_to_string(root.join("box-reopen/sandbox.state")).expect("read state");
    let pid: i32 = state_file
        .lines()
        .find_map(|l| l.strip_prefix("pid="))
        .expect("pid line")
        .parse()
        .expect("parse pid");
    kill(Pid::from_raw(pid), Signal::SIGKILL).expect("SIGKILL daemon");
    for _ in 0..50 {
        if kill(Pid::from_raw(pid), None).is_err() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }

    let stopped_err = Sandbox::reopen(plain_client.clone(), id, request.clone())
        .expect_err("reopen after SIGKILL must fail");
    assert!(
        stopped_err.container_stopped(),
        "expected container_stopped after SIGKILL, got: {stopped_err:?}"
    );
    assert!(!stopped_err.container_gone());

    let mut stopped_handle = Sandbox::stopped(plain_client.clone(), id, request.clone());
    stopped_handle.delete().expect("delete stopped sandbox");

    let gone_err =
        Sandbox::reopen(plain_client, id, request).expect_err("reopen after delete must fail");
    assert!(
        gone_err.container_gone(),
        "expected container_gone after directory removal, got: {gone_err:?}"
    );
    assert!(!gone_err.container_stopped());
}

#[test]
fn darwin_bad_config_fails_wait_listener_and_surfaces_log_tail() {
    let temp = TempDir::new("bad-config");
    let darwin =
        Darwin::new(temp.path().join("sandboxes"), Duration::from_secs(5)).expect("darwin");
    let request = base_request();
    let mut sandbox = darwin.create_sandbox(request).expect("create_sandbox");
    let mut exe_file = File::open(mock_daemon_binary()).expect("open mock daemon");
    sandbox
        .upload(&mut exe_file, b"INVALID_CONFIG_BYTES")
        .expect("upload bad config");
    sandbox.start().expect("start");

    let err = sandbox
        .wait_listener(Duration::from_secs(5))
        .expect_err("wait_listener should fail on bad config");
    assert!(!err.observation.marker_seen);

    let tail = sandbox.log_tail(20, 2048).expect("log_tail");
    let tail_text = String::from_utf8_lossy(&tail);
    assert!(
        tail_text.contains("fatal: invalid daemon.setup"),
        "unexpected log tail: {tail_text}"
    );

    sandbox.stop(1).expect("stop");
    sandbox.delete().expect("delete");
}

#[test]
fn darwin_seatbelt_sandbox_exec_blocks_setuid_ps_but_lifecycle_succeeds() {
    if std::env::var("LAYERFS_SEATBELT_CHILD").as_deref() == Ok("1") {
        let ps_out = Command::new("/bin/ps")
            .args(["-o", "lstart=", "-p", &std::process::id().to_string()])
            .output();
        assert!(
            !ps_out.is_ok_and(|o| o.status.success()),
            "expected setuid /bin/ps to fail inside sandbox-exec"
        );
        darwin_sandbox_full_lifecycle_and_4mib_1mib_exec_streams();
        return;
    }
    let _ = mock_daemon_binary();
    let exe = std::env::current_exe().expect("current_exe");
    let tmp = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
    let profile = format!(
        "(version 1) (allow default) (deny file-write*) (allow file-write* (literal \"/dev/null\") (subpath \"/private/tmp\") (subpath \"{}\"))",
        tmp.display()
    );
    let out = Command::new("/usr/bin/sandbox-exec")
        .args([
            "-p",
            &profile,
            exe.to_str().expect("exe str"),
            "--exact",
            "darwin_seatbelt_sandbox_exec_blocks_setuid_ps_but_lifecycle_succeeds",
            "--nocapture",
        ])
        .env("LAYERFS_SEATBELT_CHILD", "1")
        .output()
        .expect("spawn sandbox-exec child");
    assert!(
        out.status.success(),
        "seatbelt child failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

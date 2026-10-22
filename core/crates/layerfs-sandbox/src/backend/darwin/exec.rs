//! Darwin process Exec creation, multiplexed stdio streaming, and exit inspection.
use super::{
    super::json::Json,
    sandbox::{find_sandbox, is_process_alive, read_json_string, respond, write_mux_frame},
};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::Shutdown,
    os::unix::{net::UnixStream, process::ExitStatusExt},
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static NEXT_EXEC: AtomicU64 = AtomicU64::new(1);

struct ExecEntry {
    id: String,
    container_id: String,
    stdin: bool,
    working_dir: String,
    cmd: Vec<String>,
    env: Vec<String>,
    state: Mutex<ExecState>,
}

struct ExecState {
    running: bool,
    exit_code: Option<i32>,
    pid: u32,
}

fn registry() -> &'static Mutex<BTreeMap<String, Arc<ExecEntry>>> {
    static REG: OnceLock<Mutex<BTreeMap<String, Arc<ExecEntry>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn handle_create(
    root: &Path,
    container_id: &str,
    body: &[u8],
    stream: &mut UnixStream,
    keep_alive: bool,
) -> io::Result<()> {
    let Some((sandbox_dir, record)) = find_sandbox(root, container_id) else {
        return respond(
            stream,
            404,
            b"{\"message\":\"No such container\"}",
            keep_alive,
        );
    };
    if record.stopped || !is_process_alive(record.pid, &record.lstart) {
        return respond(
            stream,
            409,
            b"{\"message\":\"Container is not running\"}",
            keep_alive,
        );
    }
    let mut stdin = false;
    let mut working_dir = String::from("/");
    let mut cmd = Vec::new();
    let mut env = Vec::new();
    let mut cursor = io::Cursor::new(body);
    let mut j = Json::new(&mut cursor);
    let _ = j.object(|j, key| {
        if key.equals("AttachStdin") {
            stdin = j.boolean()?;
        } else if key.equals("WorkingDir") {
            working_dir = read_json_string(j)?;
        } else if key.equals("Cmd") {
            j.array(|j| {
                cmd.push(read_json_string(j)?);
                Ok(())
            })?;
        } else if key.equals("Env") {
            j.array(|j| {
                env.push(read_json_string(j)?);
                Ok(())
            })?;
        } else {
            j.skip(2)?;
        }
        Ok(())
    });

    let ws_host = sandbox_dir
        .join("runtime/workspaces")
        .to_string_lossy()
        .into_owned();
    working_dir = translate_workspaces_path(&working_dir, &ws_host);
    if !Path::new("/workspaces").exists() {
        for arg in &mut cmd {
            if arg.contains("/workspaces") {
                *arg = arg.replace("/workspaces", &ws_host);
            }
        }
        for item in &mut env {
            if item.contains("/workspaces") {
                *item = item.replace("/workspaces", &ws_host);
            }
        }
    }

    let seq = NEXT_EXEC.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let exec_id = format!("{:016x}{:016x}{:032x}", std::process::id(), seq, nanos);
    let entry = Arc::new(ExecEntry {
        id: exec_id.clone(),
        container_id: record.id,
        stdin,
        working_dir,
        cmd,
        env,
        state: Mutex::new(ExecState {
            running: false,
            exit_code: None,
            pid: 0,
        }),
    });
    registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(exec_id.clone(), entry);
    let resp = format!("{{\"Id\":\"{exec_id}\"}}");
    respond(stream, 201, resp.as_bytes(), keep_alive)
}

fn translate_workspaces_path(path: &str, ws_host: &str) -> String {
    if path == "/workspaces" {
        ws_host.to_owned()
    } else if let Some(rel) = path.strip_prefix("/workspaces/") {
        format!("{ws_host}/{rel}")
    } else {
        path.to_owned()
    }
}

pub(super) fn handle_start(exec_id: &str, mut stream: UnixStream) -> io::Result<()> {
    let entry = registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(exec_id)
        .cloned();
    let Some(entry) = entry else {
        return respond(&mut stream, 404, b"{\"message\":\"No such exec\"}", false);
    };
    write!(
        stream,
        "HTTP/1.1 101 UPGRADED\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nConnection: Upgrade\r\nUpgrade: tcp\r\n\r\n"
    )?;
    stream.flush()?;

    let mut command = Command::new(&entry.cmd[0]);
    command
        .args(&entry.cmd[1..])
        .current_dir(&entry.working_dir)
        .env_clear()
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| {
                "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin".to_owned()
            }),
        )
        .stdin(if entry.stdin {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for kv in &entry.env {
        if let Some((k, v)) = kv.split_once('=') {
            command.env(k, v);
        }
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let msg = format!("exec spawn failed: {error}\n");
            let _ = write_mux_frame(&mut stream, 2, msg.as_bytes());
            let _ = stream.flush();
            let mut state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
            state.running = false;
            state.exit_code = Some(127);
            let _ = stream.shutdown(Shutdown::Both);
            return Ok(());
        }
    };
    {
        let mut state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
        state.running = true;
        state.pid = child.id();
    }

    if let Some(mut child_stdin) = child.stdin.take() {
        if let Ok(mut stream_in) = stream.try_clone() {
            thread::spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = stream_in.read(&mut buf) {
                    if n == 0 || child_stdin.write_all(&buf[..n]).is_err() {
                        break;
                    }
                    let _ = child_stdin.flush();
                }
            });
        }
    }

    let writer = Arc::new(Mutex::new(stream));
    let out_writer = Arc::clone(&writer);
    let mut child_stdout = child.stdout.take().expect("piped stdout");
    let out_thread = thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = child_stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut guard = out_writer.lock().unwrap_or_else(|e| e.into_inner());
            if write_mux_frame(&mut *guard, 1, &buf[..n]).is_err() || guard.flush().is_err() {
                break;
            }
        }
    });

    let err_writer = Arc::clone(&writer);
    let mut child_stderr = child.stderr.take().expect("piped stderr");
    let err_thread = thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = child_stderr.read(&mut buf) {
            if n == 0 {
                break;
            }
            let mut guard = err_writer.lock().unwrap_or_else(|e| e.into_inner());
            if write_mux_frame(&mut *guard, 2, &buf[..n]).is_err() || guard.flush().is_err() {
                break;
            }
        }
    });

    let _ = out_thread.join();
    let _ = err_thread.join();

    let wait_deadline = Instant::now() + Duration::from_millis(200);
    let mut finished = false;
    while Instant::now() < wait_deadline {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status
                    .code()
                    .or_else(|| status.signal().map(|s| 128 + s))
                    .unwrap_or(1);
                let mut state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
                state.running = false;
                state.exit_code = Some(code);
                finished = true;
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(_) => break,
        }
    }
    if !finished {
        let entry_bg = Arc::clone(&entry);
        thread::spawn(move || {
            let code = match child.wait() {
                Ok(status) => status
                    .code()
                    .or_else(|| status.signal().map(|s| 128 + s))
                    .unwrap_or(1),
                Err(_) => 1,
            };
            let mut state = entry_bg.state.lock().unwrap_or_else(|e| e.into_inner());
            state.running = false;
            state.exit_code = Some(code);
        });
    }
    if let Ok(guard) = writer.lock() {
        let _ = guard.shutdown(Shutdown::Both);
    }
    Ok(())
}

pub(super) fn handle_inspect(
    exec_id: &str,
    stream: &mut UnixStream,
    keep_alive: bool,
) -> io::Result<()> {
    let entry = registry()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(exec_id)
        .cloned();
    let Some(entry) = entry else {
        return respond(stream, 404, b"{\"message\":\"No such exec\"}", keep_alive);
    };
    let (running, exit_code, pid) = {
        let state = entry.state.lock().unwrap_or_else(|e| e.into_inner());
        (state.running, state.exit_code, state.pid)
    };
    let exit_str = match exit_code {
        Some(c) => c.to_string(),
        None => "null".to_owned(),
    };
    let body = format!(
        "{{\"ID\":\"{}\",\"ContainerID\":\"{}\",\"Running\":{running},\"ExitCode\":{exit_str},\"Pid\":{pid}}}",
        entry.id, entry.container_id
    );
    respond(stream, 200, body.as_bytes(), keep_alive)
}

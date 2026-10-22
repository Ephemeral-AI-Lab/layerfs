//! Darwin Sandbox HTTP-over-socketpair server and host process lifecycle.
use super::super::{json::Json, strings::quoted, topology::SECCOMP};
use nix::{
    mount::{unmount, MntFlags},
    sys::signal::{kill, Signal},
    unistd::{Gid, Pid, Uid},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    net::SocketAddr,
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        net::UnixStream,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

static STATE_LOCK: Mutex<()> = Mutex::new(());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub(super) struct SandboxRecord {
    pub id: String,
    pub name: String,
    pub image: String,
    pub store_volume: String,
    pub port: u16,
    pub actual_port: u16,
    pub pid: u32,
    pub lstart: String,
    pub stopped: bool,
    pub labels: Vec<(String, String)>,
}

pub(crate) fn is_darwin_root(path: &Path) -> bool {
    if path == Path::new("/var/run/docker.sock") {
        return true;
    }
    use std::os::unix::fs::FileTypeExt;
    !fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_socket())
        && path.extension().and_then(|ext| ext.to_str()) != Some("sock")
        && path.file_name().and_then(|n| n.to_str()) != Some("sock")
}

pub(crate) fn connect(path: &Path) -> io::Result<UnixStream> {
    let root = if path == Path::new("/var/run/docker.sock") {
        std::env::temp_dir().join(format!("layerfs-darwin-{}", Uid::effective().as_raw()))
    } else {
        PathBuf::from(path)
    };
    let (client, server) = UnixStream::pair()?;
    client.set_nonblocking(true)?;
    thread::spawn(move || {
        let _ = serve(root, server);
    });
    Ok(client)
}

pub(super) fn serve(root: PathBuf, stream: UnixStream) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    let mut reader = BufReader::new(stream);
    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line)? == 0 {
            return Ok(());
        }
        let mut parts = request_line.trim_end().splitn(3, ' ');
        let Some(method) = parts.next() else {
            return Ok(());
        };
        let Some(path) = parts.next() else {
            return Ok(());
        };
        let mut content_length = 0usize;
        let mut closes = false;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    content_length = v.trim().parse().unwrap_or(0);
                } else if k.trim().eq_ignore_ascii_case("connection")
                    && v.trim().eq_ignore_ascii_case("close")
                {
                    closes = true;
                }
            }
        }
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;

        if method == "GET" && path.starts_with("/v1.54/volumes/") {
            let name = &path["/v1.54/volumes/".len()..];
            handle_volume(&root, name, reader.get_mut())?;
        } else if method == "POST" && path.starts_with("/v1.54/containers/create") {
            handle_create(&root, path, &body, reader.get_mut())?;
        } else if method == "PUT"
            && path.starts_with("/v1.54/containers/")
            && path.contains("/archive?")
        {
            let id = container_id_segment(path, "/archive?");
            handle_archive(&root, id, &body, reader.get_mut())?;
        } else if method == "POST"
            && path.starts_with("/v1.54/containers/")
            && path.ends_with("/start")
        {
            let id = container_id_segment(path, "/start");
            handle_start(&root, id, reader.get_mut())?;
        } else if method == "GET"
            && path.starts_with("/v1.54/containers/")
            && path.contains("/logs?")
        {
            let id = container_id_segment(path, "/logs?");
            return handle_logs(&root, id, path, &mut reader);
        } else if method == "GET"
            && path.starts_with("/v1.54/containers/")
            && path.ends_with("/json")
        {
            let id = container_id_segment(path, "/json");
            handle_inspect(&root, id, reader.get_mut())?;
        } else if method == "POST"
            && path.starts_with("/v1.54/containers/")
            && path.contains("/stop?")
        {
            let id = container_id_segment(path, "/stop?");
            handle_stop(&root, id, reader.get_mut())?;
        } else if method == "DELETE" && path.starts_with("/v1.54/containers/") {
            let id = container_id_segment(path, "?");
            handle_delete(&root, id, reader.get_mut())?;
        } else if method == "POST"
            && path.starts_with("/v1.54/containers/")
            && path.ends_with("/exec")
        {
            let id = container_id_segment(path, "/exec");
            super::exec::handle_create(&root, id, &body, reader.get_mut(), !closes)?;
        } else if method == "POST" && path.starts_with("/v1.54/exec/") && path.ends_with("/start") {
            let id = exec_id_segment(path, "/start");
            return super::exec::handle_start(id, reader.into_inner());
        } else if method == "GET" && path.starts_with("/v1.54/exec/") && path.ends_with("/json") {
            let id = exec_id_segment(path, "/json");
            super::exec::handle_inspect(id, reader.get_mut(), !closes)?;
        } else {
            respond(reader.get_mut(), 404, b"{\"message\":\"not found\"}", false)?;
        }
        if closes {
            return Ok(());
        }
    }
}

fn container_id_segment<'a>(path: &'a str, suffix: &str) -> &'a str {
    let rest = &path["/v1.54/containers/".len()..];
    rest.split(suffix).next().unwrap_or(rest)
}

fn exec_id_segment<'a>(path: &'a str, suffix: &str) -> &'a str {
    let rest = &path["/v1.54/exec/".len()..];
    rest.split(suffix).next().unwrap_or(rest)
}

pub(super) fn respond(
    stream: &mut UnixStream,
    status: u16,
    body: &[u8],
    keep_alive: bool,
) -> io::Result<()> {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        304 => "Not Modified",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Error",
    };
    let conn = if keep_alive { "keep-alive" } else { "close" };
    if status == 204 || status == 304 {
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nConnection: {conn}\r\n\r\n"
        )?;
    } else {
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: {conn}\r\n\r\n",
            body.len()
        )?;
        stream.write_all(body)?;
    }
    stream.flush()
}

fn handle_volume(root: &Path, name: &str, stream: &mut UnixStream) -> io::Result<()> {
    let vol_dir = root.join("_volumes").join(name);
    if !vol_dir.exists() {
        fs::create_dir_all(&vol_dir)?;
    }
    let mut body = Vec::new();
    body.extend_from_slice(b"{\"Name\":");
    quoted(&mut body, name)?;
    body.extend_from_slice(b",\"Driver\":\"local\",\"Scope\":\"local\",\"Options\":null}");
    respond(stream, 200, &body, false)
}

pub(super) fn read_json_string<R: Read>(j: &mut Json<R>) -> Result<String, crate::RuntimeError> {
    j.whitespace()?;
    j.expect(b'"')?;
    let mut raw = Vec::new();
    loop {
        match j.byte()? {
            b'"' => break,
            b'\\' => match j.byte()? {
                b'"' => raw.push(b'"'),
                b'\\' => raw.push(b'\\'),
                b'/' => raw.push(b'/'),
                b'b' => raw.push(b'\x08'),
                b'f' => raw.push(b'\x0c'),
                b'n' => raw.push(b'\n'),
                b'r' => raw.push(b'\r'),
                b't' => raw.push(b'\t'),
                b'u' => {
                    let mut n = 0u16;
                    for _ in 0..4 {
                        let b = j.byte()?;
                        let v = match b {
                            b'0'..=b'9' => b - b'0',
                            b'a'..=b'f' => b - b'a' + 10,
                            b'A'..=b'F' => b - b'A' + 10,
                            _ => return Err(crate::RuntimeError::Protocol("JSON unicode")),
                        };
                        n = n * 16 + u16::from(v);
                    }
                    if let Some(ch) = char::from_u32(u32::from(n)) {
                        let mut enc = [0u8; 4];
                        raw.extend_from_slice(ch.encode_utf8(&mut enc).as_bytes());
                    }
                }
                _ => return Err(crate::RuntimeError::Protocol("JSON escape")),
            },
            b => raw.push(b),
        }
    }
    String::from_utf8(raw).map_err(|_| crate::RuntimeError::Protocol("JSON UTF-8"))
}

fn handle_create(root: &Path, path: &str, body: &[u8], stream: &mut UnixStream) -> io::Result<()> {
    let name_param = path
        .split_once("?name=")
        .map(|(_, n)| n.split('&').next().unwrap_or(n).to_owned());
    let mut image = String::new();
    let mut labels = Vec::new();
    let mut store_volume = String::new();
    let mut port = 30421u16;
    let mut cursor = io::Cursor::new(body);
    let mut j = Json::new(&mut cursor);
    let _ = j.object(|j, key| {
        if key.equals("Image") {
            image = read_json_string(j)?;
        } else if key.equals("Labels") {
            j.object(|j, k| {
                let k_str = k.text().to_owned();
                let v_str = read_json_string(j)?;
                labels.push((k_str, v_str));
                Ok(())
            })?;
        } else if key.equals("ExposedPorts") {
            j.object(|j, k| {
                if let Some(p) = k.text().strip_suffix("/tcp") {
                    port = p.parse().unwrap_or(30421);
                }
                j.skip(1)
            })?;
        } else if key.equals("HostConfig") {
            j.object(|j, k| {
                if k.equals("Mounts") {
                    j.array(|j| {
                        j.object(|j, mk| {
                            if mk.equals("Source") {
                                store_volume = read_json_string(j)?;
                            } else {
                                j.skip(2)?;
                            }
                            Ok(())
                        })
                    })?;
                } else {
                    j.skip(3)?;
                }
                Ok(())
            })?;
        } else {
            j.skip(3)?;
        }
        Ok(())
    });

    let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let id = format!("{:016x}{:016x}{:032x}", std::process::id(), seq, nanos);
    let dir_name = name_param.unwrap_or_else(|| id.clone());
    let sandbox_dir = root.join(&dir_name);
    if sandbox_dir.exists() {
        return respond(
            stream,
            409,
            b"{\"message\":\"Conflict. The container name is already in use.\"}",
            false,
        );
    }
    let vol_dir = root.join("_volumes").join(&store_volume);
    if !vol_dir.exists() {
        fs::create_dir_all(&vol_dir)?;
    }
    create_dir_mode(root, 0o700)?;
    create_dir_mode(&sandbox_dir, 0o700)?;
    let runtime = sandbox_dir.join("runtime");
    create_dir_mode(&runtime, 0o700)?;
    create_dir_mode(&runtime.join("bin"), 0o700)?;
    create_dir_mode(&runtime.join("config"), 0o700)?;
    create_dir_mode(&runtime.join("overlay"), 0o700)?;
    create_dir_mode(&runtime.join("workspaces"), 0o755)?;

    let record = SandboxRecord {
        id: id.clone(),
        name: dir_name,
        image,
        store_volume,
        port,
        actual_port: 0,
        pid: 0,
        lstart: String::new(),
        stopped: false,
        labels,
    };
    save_record(&sandbox_dir, &record)?;
    let resp = format!("{{\"Id\":\"{id}\"}}");
    respond(stream, 201, resp.as_bytes(), false)
}

fn handle_archive(root: &Path, id: &str, tar: &[u8], stream: &mut UnixStream) -> io::Result<()> {
    let (sandbox_dir, record) = {
        let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Some(found) = find_sandbox(root, id) else {
            return respond(stream, 404, b"{\"message\":\"No such container\"}", false);
        };
        found
    };
    let runtime = sandbox_dir.join("runtime");
    let vol_raw = root.join("_volumes").join(&record.store_volume);
    let volume_dir = fs::canonicalize(&vol_raw).unwrap_or(vol_raw);
    let mut offset = 0usize;
    while offset + 512 <= tar.len() {
        let header = &tar[offset..offset + 512];
        offset += 512;
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let name_end = header[..100].iter().position(|&b| b == 0).unwrap_or(100);
        let entry_path = std::str::from_utf8(&header[..name_end]).unwrap_or("");
        let size_str = std::str::from_utf8(&header[124..136])
            .unwrap_or("")
            .trim_matches(|c: char| c == '\0' || c == ' ');
        let size = usize::from_str_radix(size_str, 8).unwrap_or(0);
        let typeflag = header[156];
        if offset + size > tar.len() {
            break;
        }
        let data = &tar[offset..offset + size];
        let padded = size.div_ceil(512) * 512;
        offset += padded;
        if typeflag == b'0' || typeflag == 0 {
            if entry_path == "layerfs-local/config/daemon.setup" {
                let translated = translate_daemon_setup(data, &volume_dir, &runtime);
                write_file_mode(&runtime.join("config/daemon.setup"), &translated, 0o600)?;
            } else if entry_path == "usr/local/bin/layerfs-daemon" {
                write_file_mode(&runtime.join("bin/layerfs-daemon"), data, 0o700)?;
            }
        }
    }
    respond(stream, 200, b"", false)
}

fn translate_daemon_setup(bytes: &[u8], volume_dir: &Path, runtime_dir: &Path) -> Vec<u8> {
    let fixed_len = if bytes.starts_with(b"LFSD\x05") {
        106usize
    } else if bytes.starts_with(b"LFSD\x04") {
        100usize
    } else {
        return bytes.to_vec();
    };
    let magic = &bytes[..5];
    let mut cursor = 5usize;
    let Some(listen) = read_u16_str(bytes, &mut cursor) else {
        return bytes.to_vec();
    };
    let Some(store) = read_u16_str(bytes, &mut cursor) else {
        return bytes.to_vec();
    };
    let Some(overlay) = read_u16_str(bytes, &mut cursor) else {
        return bytes.to_vec();
    };
    let Some(mounts) = read_u16_str(bytes, &mut cursor) else {
        return bytes.to_vec();
    };
    if cursor + fixed_len + 1 > bytes.len() {
        return bytes.to_vec();
    }
    let new_listen = if listen.starts_with("0.0.0.0:") {
        "127.0.0.1:0".to_owned()
    } else {
        listen.to_owned()
    };
    let new_store = if let Some(rel) = store.strip_prefix("/layerfs-store/") {
        volume_dir.join(rel).to_string_lossy().into_owned()
    } else {
        store.to_owned()
    };
    let new_overlay = if let Some(rel) = overlay.strip_prefix("/layerfs-local/") {
        runtime_dir.join(rel).to_string_lossy().into_owned()
    } else {
        overlay.to_owned()
    };
    let new_mounts = if mounts == "/workspaces" {
        runtime_dir
            .join("workspaces")
            .to_string_lossy()
            .into_owned()
    } else {
        mounts.to_owned()
    };
    let mut out = Vec::with_capacity(bytes.len() + 256);
    out.extend_from_slice(magic);
    for s in [&new_listen, &new_store, &new_overlay, &new_mounts] {
        write_u16_str(&mut out, s);
    }
    out.extend_from_slice(&bytes[cursor..cursor + 64]);
    out.extend_from_slice(&Uid::effective().as_raw().to_be_bytes());
    out.extend_from_slice(&Gid::effective().as_raw().to_be_bytes());
    out.extend_from_slice(&bytes[cursor + 72..cursor + fixed_len]);
    let has_existing = bytes[cursor + fixed_len];
    out.push(has_existing);
    let mut tail_cursor = cursor + fixed_len + 1;
    if has_existing == 1 {
        let Some(manifest_len) = read_u16(bytes, &mut tail_cursor) else {
            return bytes.to_vec();
        };
        if tail_cursor + manifest_len > bytes.len() {
            return bytes.to_vec();
        }
        let manifest = &bytes[tail_cursor..tail_cursor + manifest_len];
        if manifest.starts_with(b"LFSI\x01") && manifest.len() >= 9 {
            let mut m_cursor = 7usize;
            if read_u16_str(manifest, &mut m_cursor).is_some() {
                let mut new_m = Vec::with_capacity(manifest.len() + 128);
                new_m.extend_from_slice(&manifest[..7]);
                write_u16_str(&mut new_m, &new_store);
                new_m.extend_from_slice(&manifest[m_cursor..]);
                let m_len = u16::try_from(new_m.len()).unwrap_or(0);
                out.extend_from_slice(&m_len.to_be_bytes());
                out.extend_from_slice(&new_m);
                return out;
            }
        }
        out.extend_from_slice(&bytes[cursor + fixed_len + 1..]);
    }
    out
}

fn handle_start(root: &Path, id: &str, stream: &mut UnixStream) -> io::Result<()> {
    let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some((sandbox_dir, mut record)) = find_sandbox(root, id) else {
        return respond(stream, 404, b"{\"message\":\"No such container\"}", false);
    };
    let runtime = sandbox_dir.join("runtime");
    let stdout = File::create(runtime.join("daemon.stdout"))?;
    let stderr = File::create(runtime.join("daemon.stderr"))?;
    let mut child = Command::new(runtime.join("bin/layerfs-daemon"))
        .arg("--config")
        .arg(runtime.join("config/daemon.setup"))
        .current_dir(&runtime)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()?;
    let pid = child.id();
    let lstart = process_lstart(pid).unwrap_or_else(|| "kill0".to_owned());
    record.pid = pid;
    record.lstart = lstart;
    record.stopped = false;
    save_record(&sandbox_dir, &record)?;
    drop(_guard);
    let wait_dir = sandbox_dir.clone();
    thread::spawn(move || {
        let _ = child.wait();
        let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(mut rec) = load_record(&wait_dir) {
            if rec.pid == pid {
                rec.stopped = true;
                let _ = save_record(&wait_dir, &rec);
            }
        }
    });
    respond(stream, 204, b"", false)
}

fn handle_logs(
    root: &Path,
    id: &str,
    path: &str,
    reader: &mut BufReader<UnixStream>,
) -> io::Result<()> {
    let (sandbox_dir, record) = {
        let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Some(found) = find_sandbox(root, id) else {
            return respond(
                reader.get_mut(),
                404,
                b"{\"message\":\"No such container\"}",
                false,
            );
        };
        found
    };
    let runtime = sandbox_dir.join("runtime");
    let stdout_path = runtime.join("daemon.stdout");
    let stderr_path = runtime.join("daemon.stderr");
    if path.contains("follow=true") {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut found_port = None;
        loop {
            if let Ok(text) = fs::read_to_string(&stdout_path) {
                for line in text.lines() {
                    if let Some(rest) = line.strip_prefix("LAYERFS_DAEMON_LISTEN ") {
                        if let Ok(addr) = rest.trim().parse::<SocketAddr>() {
                            found_port = Some(addr.port());
                            break;
                        }
                    }
                }
            }
            if found_port.is_some() {
                break;
            }
            if !is_process_alive(record.pid, &record.lstart) || Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let stream = reader.get_mut();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nConnection: close\r\n\r\n"
        )?;
        if let Some(actual_port) = found_port {
            {
                let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                if let Some((dir, mut latest)) = find_sandbox(root, id) {
                    latest.actual_port = actual_port;
                    let _ = save_record(&dir, &latest);
                }
            }
            let marker = format!("LAYERFS_DAEMON_LISTEN 0.0.0.0:{}\n", record.port);
            write_mux_frame(stream, 1, marker.as_bytes())?;
            stream.flush()?;
            let mut one = [0u8; 1];
            let _ = reader.read(&mut one);
            Ok(())
        } else {
            if let Ok(err_bytes) = fs::read(&stderr_path) {
                if !err_bytes.is_empty() {
                    let _ = write_mux_frame(stream, 2, &err_bytes);
                }
            }
            stream.flush()
        }
    } else {
        let out_bytes = fs::read(&stdout_path).unwrap_or_default();
        let err_bytes = fs::read(&stderr_path).unwrap_or_default();
        let mut frames = Vec::new();
        if !out_bytes.is_empty() {
            write_mux_frame(&mut frames, 1, &out_bytes)?;
        }
        if !err_bytes.is_empty() {
            write_mux_frame(&mut frames, 2, &err_bytes)?;
        }
        let stream = reader.get_mut();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            frames.len()
        )?;
        stream.write_all(&frames)?;
        stream.flush()
    }
}

fn handle_inspect(root: &Path, id: &str, stream: &mut UnixStream) -> io::Result<()> {
    let record = {
        let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Some((_, record)) = find_sandbox(root, id) else {
            return respond(stream, 404, b"{\"message\":\"No such container\"}", false);
        };
        record
    };
    let running = !record.stopped && is_process_alive(record.pid, &record.lstart);
    let host_port = if record.actual_port != 0 {
        record.actual_port
    } else {
        record.port
    };
    let mut body = Vec::with_capacity(1024);
    body.extend_from_slice(b"{\"Id\":");
    quoted(&mut body, &record.id)?;
    body.extend_from_slice(b",\"Image\":");
    quoted(&mut body, &record.image)?;
    write!(
        body,
        ",\"State\":{{\"Running\":{running},\"Paused\":false,\"Restarting\":false,\"Dead\":false}},\"Config\":{{\"Labels\":{{"
    )?;
    for (i, (k, v)) in record.labels.iter().enumerate() {
        if i > 0 {
            body.push(b',');
        }
        quoted(&mut body, k)?;
        body.push(b':');
        quoted(&mut body, v)?;
    }
    body.extend_from_slice(
        b"},\"User\":\"0:0\",\"Tty\":false,\"Entrypoint\":[\"/usr/local/bin/layerfs-daemon\"],\"Cmd\":[\"--config\",\"/layerfs-local/config/daemon.setup\"]},\"HostConfig\":{\"Init\":true,\"Privileged\":false,\"CapAdd\":[\"CAP_SYS_ADMIN\"],\"Devices\":[{\"PathOnHost\":\"/dev/fuse\",\"PathInContainer\":\"/dev/fuse\",\"CgroupPermissions\":\"rwm\"}],\"SecurityOpt\":[\"no-new-privileges=true\",\"apparmor=unconfined\",",
    );
    quoted(&mut body, SECCOMP)?;
    body.extend_from_slice(
        b"],\"RestartPolicy\":{\"Name\":\"no\"},\"LogConfig\":{\"Type\":\"json-file\"}},\"Mounts\":[{\"Type\":\"volume\",\"Name\":",
    );
    quoted(&mut body, &record.store_volume)?;
    write!(
        body,
        ",\"Destination\":\"/layerfs-store\",\"RW\":true,\"Driver\":\"local\"}}],\"NetworkSettings\":{{\"Ports\":{{\"{}/tcp\":[{{\"HostIp\":\"127.0.0.1\",\"HostPort\":\"{host_port}\"}}]}}}}}}",
        record.port
    )?;
    respond(stream, 200, &body, false)
}

fn handle_stop(root: &Path, id: &str, stream: &mut UnixStream) -> io::Result<()> {
    let (sandbox_dir, mut record) = {
        let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Some(found) = find_sandbox(root, id) else {
            return respond(stream, 404, b"{\"message\":\"No such container\"}", false);
        };
        found
    };
    let was_alive = !record.stopped && is_process_alive(record.pid, &record.lstart);
    if was_alive && record.pid != 0 {
        let pid = Pid::from_raw(record.pid as i32);
        let _ = kill(pid, Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_millis(150);
        while is_process_alive(record.pid, &record.lstart) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if is_process_alive(record.pid, &record.lstart) {
            let _ = kill(pid, Signal::SIGKILL);
            let kill_deadline = Instant::now() + Duration::from_millis(150);
            while is_process_alive(record.pid, &record.lstart) && Instant::now() < kill_deadline {
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
    force_unmount_workspaces(&sandbox_dir.join("runtime/workspaces"));
    {
        let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        record.stopped = true;
        let _ = save_record(&sandbox_dir, &record);
    }
    respond(stream, if was_alive { 204 } else { 304 }, b"", false)
}

fn handle_delete(root: &Path, id: &str, stream: &mut UnixStream) -> io::Result<()> {
    let _guard = STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let Some((sandbox_dir, record)) = find_sandbox(root, id) else {
        return respond(stream, 404, b"{\"message\":\"No such container\"}", false);
    };
    if !record.stopped && is_process_alive(record.pid, &record.lstart) {
        return respond(
            stream,
            409,
            b"{\"message\":\"You cannot remove a running container\"}",
            false,
        );
    }
    force_unmount_workspaces(&sandbox_dir.join("runtime/workspaces"));
    fs::remove_dir_all(&sandbox_dir)?;
    respond(stream, 204, b"", false)
}

fn force_unmount_workspaces(workspaces_dir: &Path) {
    let Ok(entries) = fs::read_dir(workspaces_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(canon) = fs::canonicalize(&path) {
            let _ = unmount(&canon, MntFlags::MNT_FORCE);
        }
        let _ = unmount(&path, MntFlags::MNT_FORCE);
    }
}

pub(super) fn write_mux_frame(out: &mut impl Write, tag: u8, payload: &[u8]) -> io::Result<()> {
    let mut header = [0u8; 8];
    header[0] = tag;
    header[4..8].copy_from_slice(&(payload.len() as u32).to_be_bytes());
    out.write_all(&header)?;
    out.write_all(payload)
}

pub(super) fn find_sandbox(root: &Path, id_or_name: &str) -> Option<(PathBuf, SandboxRecord)> {
    let direct = root.join(id_or_name);
    if let Ok(rec) = load_record(&direct) {
        return Some((direct, rec));
    }
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(rec) = load_record(&path) {
            if rec.id == id_or_name || rec.name == id_or_name {
                return Some((path, rec));
            }
        }
    }
    None
}

fn save_record(sandbox_dir: &Path, rec: &SandboxRecord) -> io::Result<()> {
    let mut text = format!(
        "id={}\nname={}\nimage={}\nstore_volume={}\nport={}\nactual_port={}\npid={}\nlstart={}\nstopped={}\n",
        rec.id,
        rec.name,
        rec.image,
        rec.store_volume,
        rec.port,
        rec.actual_port,
        rec.pid,
        rec.lstart,
        rec.stopped
    );
    for (k, v) in &rec.labels {
        text.push_str(&format!("label={k}={v}\n"));
    }
    write_file_mode(&sandbox_dir.join("sandbox.state"), text.as_bytes(), 0o600)
}

fn load_record(sandbox_dir: &Path) -> io::Result<SandboxRecord> {
    let text = fs::read_to_string(sandbox_dir.join("sandbox.state"))?;
    let mut rec = SandboxRecord {
        id: String::new(),
        name: String::new(),
        image: String::new(),
        store_volume: String::new(),
        port: 30421,
        actual_port: 0,
        pid: 0,
        lstart: String::new(),
        stopped: false,
        labels: Vec::new(),
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("label=") {
            if let Some((k, v)) = rest.split_once('=') {
                rec.labels.push((k.to_owned(), v.to_owned()));
            }
        } else if let Some((k, v)) = line.split_once('=') {
            match k {
                "id" => rec.id = v.to_owned(),
                "name" => rec.name = v.to_owned(),
                "image" => rec.image = v.to_owned(),
                "store_volume" => rec.store_volume = v.to_owned(),
                "port" => rec.port = v.parse().unwrap_or(30421),
                "actual_port" => rec.actual_port = v.parse().unwrap_or(0),
                "pid" => rec.pid = v.parse().unwrap_or(0),
                "lstart" => rec.lstart = v.to_owned(),
                "stopped" => rec.stopped = v == "true",
                _ => {}
            }
        }
    }
    if rec.id.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "missing id"));
    }
    Ok(rec)
}

pub(super) fn is_process_alive(pid: u32, expected_lstart: &str) -> bool {
    if pid == 0 || expected_lstart.is_empty() {
        return false;
    }
    if kill(Pid::from_raw(pid as i32), None).is_err() {
        return false;
    }
    if expected_lstart == "kill0" {
        return true;
    }
    let Ok(output) = Command::new("/bin/ps")
        .args(["-o", "state=,lstart=", "-p", &pid.to_string()])
        .output()
    else {
        return true;
    };
    if !output.status.success() {
        return true;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let trimmed = text.trim();
    let Some((state, rest)) = trimmed.split_once(char::is_whitespace) else {
        return false;
    };
    !state.starts_with('Z') && rest.trim() == expected_lstart.trim()
}

fn process_lstart(pid: u32) -> Option<String> {
    let output = Command::new("/bin/ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

fn create_dir_mode(path: &Path, mode: u32) -> io::Result<()> {
    match fs::DirBuilder::new().mode(mode).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

fn write_file_mode(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let _ = fs::remove_file(path);
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)?;
    f.write_all(bytes)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

fn read_u16(buf: &[u8], cursor: &mut usize) -> Option<usize> {
    let end = cursor.checked_add(2)?;
    if end > buf.len() {
        return None;
    }
    let val = u16::from_be_bytes([buf[*cursor], buf[*cursor + 1]]) as usize;
    *cursor = end;
    Some(val)
}

fn read_u16_str<'a>(buf: &'a [u8], cursor: &mut usize) -> Option<&'a str> {
    let len = read_u16(buf, cursor)?;
    let end = cursor.checked_add(len)?;
    if end > buf.len() {
        return None;
    }
    let s = std::str::from_utf8(&buf[*cursor..end]).ok()?;
    *cursor = end;
    Some(s)
}

fn write_u16_str(out: &mut Vec<u8>, s: &str) {
    let len = u16::try_from(s.len()).unwrap_or(0);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

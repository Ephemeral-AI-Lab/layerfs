#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod linux {
    use layerfs_daemon::protocol::{
        self, DaemonTiming, ExecRequest, Exit, Kind, MountRequest, RemoteError, ServerHello,
        AUTH_OK_BYTES, BOUND_AUTH_BYTES, BOUND_OK_BYTES, CAPABILITY_PATH, CLIENT_AUTH_BYTES,
        MAX_CONTROL, SOCKET_PATH, WORKSPACE_ROOT,
    };
    use nix::mount::{umount2, MntFlags};
    use nix::sys::resource::{getrlimit, Resource};
    use nix::sys::signal::{killpg, Signal};
    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
    use nix::sys::stat::{umask, Mode};
    use nix::unistd::Pid;
    use std::collections::BTreeMap;
    use std::ffi::{OsStr, OsString};
    use std::fs;
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::path::{Component, Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    const FIXED_HELPER: &str = "/usr/local/bin/layerfs-fuse";

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Peer {
        pid: u32,
        uid: u32,
        gid: u32,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Binding {
        Unix(Peer),
        Tcp,
    }

    enum Listener {
        Unix(UnixListener),
        Tcp(TcpListener),
    }

    enum ControlStream {
        Unix(UnixStream),
        Tcp(TcpStream),
    }

    #[derive(Clone, Copy)]
    enum Wake {
        Unix,
        Tcp(u16),
    }

    struct Owner {
        binding: Binding,
        id: [u8; 16],
        stream: ControlStream,
    }

    struct Active {
        workspace_id: [u8; 16],
        pgid: i32,
        termination: Arc<Termination>,
    }

    struct State {
        owner_live: bool,
        owner: Binding,
        owner_id: [u8; 16],
        active: BTreeMap<[u8; 16], Active>,
        mounts: BTreeMap<[u8; 16], ActiveMount>,
    }

    struct ActiveMount {
        root: Vec<u8>,
        alive: bool,
        ready: bool,
        pgid: i32,
        termination: Arc<Termination>,
    }

    struct Shared {
        state: Mutex<State>,
        drained: Condvar,
        limit: usize,
        capability: [u8; 32],
        boot_id: [u8; 16],
    }

    struct ActiveGuard {
        shared: Arc<Shared>,
        id: [u8; 16],
    }

    struct MountGuard {
        shared: Arc<Shared>,
        id: [u8; 16],
    }

    struct Termination {
        reason: AtomicU8,
        finished: AtomicBool,
    }

    impl Listener {
        fn accept(&self) -> io::Result<ControlStream> {
            match self {
                Self::Unix(listener) => listener
                    .accept()
                    .map(|(stream, _)| ControlStream::Unix(stream)),
                Self::Tcp(listener) => listener.accept().and_then(|(stream, _)| {
                    stream.set_nodelay(true)?;
                    Ok(ControlStream::Tcp(stream))
                }),
            }
        }

        fn wake(&self) -> Wake {
            match self {
                Self::Unix(_) => Wake::Unix,
                Self::Tcp(listener) => Wake::Tcp(
                    listener
                        .local_addr()
                        .expect("bound daemon TCP listener")
                        .port(),
                ),
            }
        }
    }

    impl Wake {
        fn connect(self) {
            match self {
                Self::Unix => {
                    let _ = UnixStream::connect(SOCKET_PATH);
                }
                Self::Tcp(port) => {
                    let _ = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port));
                }
            }
        }
    }

    impl ControlStream {
        fn binding(&self) -> io::Result<Binding> {
            match self {
                Self::Unix(stream) => peer(stream).map(Binding::Unix),
                Self::Tcp(_) => Ok(Binding::Tcp),
            }
        }

        fn is_tcp(&self) -> bool {
            matches!(self, Self::Tcp(_))
        }

        fn try_clone(&self) -> io::Result<Self> {
            match self {
                Self::Unix(stream) => stream.try_clone().map(Self::Unix),
                Self::Tcp(stream) => stream.try_clone().map(Self::Tcp),
            }
        }

        fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            match self {
                Self::Unix(stream) => stream.set_read_timeout(timeout),
                Self::Tcp(stream) => stream.set_read_timeout(timeout),
            }
        }

        fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            match self {
                Self::Unix(stream) => stream.set_write_timeout(timeout),
                Self::Tcp(stream) => stream.set_write_timeout(timeout),
            }
        }

        fn shutdown(&self, how: Shutdown) -> io::Result<()> {
            match self {
                Self::Unix(stream) => stream.shutdown(how),
                Self::Tcp(stream) => stream.shutdown(how),
            }
        }
    }

    impl Read for ControlStream {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            match self {
                Self::Unix(stream) => stream.read(bytes),
                Self::Tcp(stream) => stream.read(bytes),
            }
        }
    }

    impl Write for ControlStream {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            match self {
                Self::Unix(stream) => stream.write(bytes),
                Self::Tcp(stream) => stream.write(bytes),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            match self {
                Self::Unix(stream) => stream.flush(),
                Self::Tcp(stream) => stream.flush(),
            }
        }
    }

    impl Termination {
        fn new() -> Self {
            Self {
                reason: AtomicU8::new(0),
                finished: AtomicBool::new(false),
            }
        }

        fn terminate(&self, pgid: i32, reason: u8) {
            if self.finished.load(Ordering::Acquire)
                || self
                    .reason
                    .compare_exchange(0, reason, Ordering::AcqRel, Ordering::Acquire)
                    .is_err()
            {
                return;
            }
            terminate_group(pgid);
        }
    }

    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            if let Ok(mut state) = self.shared.state.lock() {
                state.active.remove(&self.id);
                self.shared.drained.notify_all();
            }
        }
    }

    impl Drop for MountGuard {
        fn drop(&mut self) {
            if let Ok(mut state) = self.shared.state.lock() {
                state.mounts.remove(&self.id);
                self.shared.drained.notify_all();
            }
        }
    }

    pub fn run() -> io::Result<()> {
        umask(Mode::from_bits_truncate(0o077));
        let tcp = std::env::var("LAYERFS_DAEMON_TCP_LISTEN").ok();
        prepare_runtime(tcp.is_none())?;
        let capability = read_capability(Path::new(CAPABILITY_PATH))?;
        let boot_id = random()?;
        let listener = match tcp {
            Some(endpoint) => {
                let listener = TcpListener::bind(
                    endpoint
                        .parse::<SocketAddr>()
                        .map_err(|_| protocol::invalid("daemon TCP listen endpoint"))?,
                )?;
                Listener::Tcp(listener)
            }
            None => {
                let listener = UnixListener::bind(SOCKET_PATH)?;
                fs::set_permissions(SOCKET_PATH, fs::Permissions::from_mode(0o600))?;
                Listener::Unix(listener)
            }
        };
        let owner = authenticate_owner(&listener, &capability, boot_id)?;
        let limit = admission_limit()?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                owner_live: true,
                owner: owner.binding,
                owner_id: owner.id,
                active: BTreeMap::new(),
                mounts: BTreeMap::new(),
            }),
            drained: Condvar::new(),
            limit,
            capability,
            boot_id,
        });
        watch_owner(owner.stream, listener.wake(), shared.clone());

        loop {
            let stream = listener.accept()?;
            let accepted = Instant::now();
            let binding = stream.binding()?;
            let allowed = shared
                .state
                .lock()
                .map(|state| state.owner_live && state.owner == binding)
                .unwrap_or(false);
            if !allowed {
                let done = shared
                    .state
                    .lock()
                    .map(|state| !state.owner_live)
                    .unwrap_or(true);
                if done {
                    break;
                }
                continue;
            }
            let shared = shared.clone();
            std::thread::spawn(move || handle_stream(stream, accepted, shared));
        }

        let mut state = shared
            .state
            .lock()
            .map_err(|_| io::Error::other("daemon state"))?;
        while !state.active.is_empty() || !state.mounts.is_empty() {
            state = shared
                .drained
                .wait(state)
                .map_err(|_| io::Error::other("daemon shutdown"))?;
        }
        if matches!(listener, Listener::Unix(_)) {
            let _ = fs::remove_file(SOCKET_PATH);
        }
        Ok(())
    }

    fn prepare_runtime(unix: bool) -> io::Result<()> {
        let root = Path::new("/run/layerfs");
        if !root.exists() {
            fs::create_dir(root)?;
        }
        let metadata = fs::metadata(root)?;
        if !metadata.is_dir()
            || metadata.uid() != nix::unistd::geteuid().as_raw()
            || metadata.mode() & 0o777 != 0o700
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "daemon runtime directory protection",
            ));
        }
        if unix && Path::new(SOCKET_PATH).exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "daemon socket already exists",
            ));
        }
        Ok(())
    }

    fn authenticate_owner(
        listener: &Listener,
        capability: &[u8; 32],
        boot_id: [u8; 16],
    ) -> io::Result<Owner> {
        loop {
            let mut stream = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            let binding = stream.binding()?;
            let hello = ServerHello {
                boot_id,
                nonce: random()?,
            };
            let attempt = (|| {
                use std::io::Write as _;
                stream.write_all(&hello.encode())?;
                let mut auth = [0; CLIENT_AUTH_BYTES];
                stream.read_exact(&mut auth)?;
                let client_nonce: [u8; 32] = auth[..32].try_into().expect("client nonce width");
                let expected = match binding {
                    Binding::Unix(peer) => protocol::client_proof(
                        capability,
                        hello,
                        &client_nonce,
                        peer.pid,
                        peer.uid,
                        peer.gid,
                    ),
                    Binding::Tcp => protocol::tcp_client_proof(capability, hello, &client_nonce),
                };
                if !constant_time_equal(&auth[32..], &expected) {
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "daemon client proof",
                    ));
                }
                let owner_id = random()?;
                let proof = match binding {
                    Binding::Unix(peer) => protocol::server_proof(
                        capability,
                        hello,
                        &client_nonce,
                        peer.pid,
                        peer.uid,
                        peer.gid,
                        &owner_id,
                    ),
                    Binding::Tcp => {
                        protocol::tcp_server_proof(capability, hello, &client_nonce, &owner_id)
                    }
                };
                let mut ok = [0; AUTH_OK_BYTES];
                ok[..16].copy_from_slice(&owner_id);
                ok[16..].copy_from_slice(&proof);
                stream.write_all(&ok)?;
                Ok(owner_id)
            })();
            if let Ok(owner_id) = attempt {
                stream.set_read_timeout(None)?;
                stream.set_write_timeout(None)?;
                return Ok(Owner {
                    binding,
                    id: owner_id,
                    stream,
                });
            }
        }
    }

    fn watch_owner(mut stream: ControlStream, wake: Wake, shared: Arc<Shared>) {
        std::thread::spawn(move || {
            let mut byte = [0];
            while matches!(stream.read(&mut byte), Ok(1)) {}
            let active = {
                let Ok(mut state) = shared.state.lock() else {
                    return;
                };
                state.owner_live = false;
                let mut active = state
                    .active
                    .values()
                    .map(|active| (active.pgid, active.termination.clone()))
                    .collect::<Vec<_>>();
                active.extend(
                    state
                        .mounts
                        .values()
                        .map(|mount| (mount.pgid, mount.termination.clone())),
                );
                active
            };
            for (pgid, termination) in active {
                termination.terminate(pgid, 2);
            }
            wake.connect();
        });
    }

    fn authenticate_bound_stream(
        stream: &mut ControlStream,
        shared: &Shared,
    ) -> io::Result<protocol::Frame> {
        if !stream.is_tcp() {
            return protocol::read_frame(stream);
        }
        stream.set_read_timeout(Some(Duration::from_secs(2)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        let hello = ServerHello {
            boot_id: shared.boot_id,
            nonce: random()?,
        };
        stream.write_all(&hello.encode())?;
        let mut auth = [0; BOUND_AUTH_BYTES];
        stream.read_exact(&mut auth)?;
        let client_nonce: [u8; 32] = auth[..32].try_into().expect("client nonce width");
        let frame = protocol::read_frame(&mut *stream)?;
        let owner_id: [u8; 16] = frame
            .payload
            .get(..16)
            .ok_or_else(|| protocol::invalid("daemon bound owner"))?
            .try_into()
            .expect("owner id width");
        let expected = protocol::bound_client_proof(
            &shared.capability,
            hello,
            &client_nonce,
            &owner_id,
            frame.kind,
            &frame.payload,
        );
        let authorized = constant_time_equal(&auth[32..], &expected)
            && shared
                .state
                .lock()
                .map(|state| state.owner_live && state.owner_id == owner_id)
                .unwrap_or(false);
        if !authorized {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "daemon bound-stream proof",
            ));
        }
        let proof = protocol::bound_server_proof(
            &shared.capability,
            hello,
            &client_nonce,
            &owner_id,
            frame.kind,
            &frame.payload,
        );
        stream.write_all(&proof[..BOUND_OK_BYTES])?;
        stream.set_read_timeout(None)?;
        stream.set_write_timeout(None)?;
        Ok(frame)
    }

    fn handle_stream(mut stream: ControlStream, accepted: Instant, shared: Arc<Shared>) {
        let decode_started = Instant::now();
        let frame = match authenticate_bound_stream(&mut stream, &shared) {
            Ok(frame) => frame,
            Err(_) => {
                if !stream.is_tcp() {
                    send_error(&mut stream, RemoteError::InvalidRequest);
                }
                return;
            }
        };
        match frame.kind {
            Kind::Exec => handle_exec(stream, accepted, decode_started, frame.payload, shared),
            Kind::Mount => handle_mount(stream, frame.payload, shared),
            _ => send_error(&mut stream, RemoteError::InvalidRequest),
        }
    }

    fn handle_exec(
        mut stream: ControlStream,
        accepted: Instant,
        decode_started: Instant,
        payload: Vec<u8>,
        shared: Arc<Shared>,
    ) {
        let accept_bind_ns = elapsed_ns(accepted);
        let request = match ExecRequest::decode(&payload) {
            Ok(request) => request,
            Err(_) => {
                send_error(&mut stream, RemoteError::InvalidRequest);
                return;
            }
        };
        let decode_ns = elapsed_ns(decode_started);
        if validate_cwd(&request.cwd).is_err() {
            send_error(&mut stream, RemoteError::InvalidRequest);
            return;
        }
        let cwd_bytes = request.cwd;
        let cwd = PathBuf::from(OsString::from_vec(cwd_bytes.clone()));
        let argv = request
            .argv
            .into_iter()
            .map(OsString::from_vec)
            .collect::<Vec<_>>();
        let termination = Arc::new(Termination::new());
        let spawn_started = Instant::now();
        let mut child = {
            let Ok(mut state) = shared.state.lock() else {
                send_error(&mut stream, RemoteError::InfrastructureLost);
                return;
            };
            if !state.owner_live || state.owner_id != request.owner_id {
                send_error(&mut stream, RemoteError::Unauthorized);
                return;
            }
            if !state
                .mounts
                .get(&request.workspace_id)
                .is_some_and(|mount| mount.ready && mount.root == cwd_bytes)
            {
                send_error(&mut stream, RemoteError::InvalidRequest);
                return;
            }
            if state.active.len().saturating_add(state.mounts.len()) >= shared.limit {
                send_error(&mut stream, RemoteError::LimitExceeded);
                return;
            }
            if state.active.contains_key(&request.execution_id) {
                send_error(&mut stream, RemoteError::InvalidRequest);
                return;
            }
            let mut command = Command::new(&argv[0]);
            command
                .args(&argv[1..])
                .current_dir(&cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0);
            let child = match command.spawn() {
                Ok(child) => child,
                Err(_) => {
                    send_error(&mut stream, RemoteError::InvalidRequest);
                    return;
                }
            };
            state.active.insert(
                request.execution_id,
                Active {
                    workspace_id: request.workspace_id,
                    pgid: child.id() as i32,
                    termination: termination.clone(),
                },
            );
            child
        };
        let _guard = ActiveGuard {
            shared: shared.clone(),
            id: request.execution_id,
        };
        let spawn_ns = elapsed_ns(spawn_started);
        let pgid = child.id() as i32;
        let reader = match stream.try_clone() {
            Ok(reader) => reader,
            Err(_) => {
                termination.terminate(pgid, 2);
                let _ = child.wait();
                return;
            }
        };
        let writer = Arc::new(Mutex::new(stream));
        if write_locked(&writer, Kind::Started, &[]).is_err() {
            termination.terminate(pgid, 2);
            let _ = child.wait();
            return;
        }
        let stop_thread = watch_exec(reader, pgid, termination.clone());
        let stdout_bytes = Arc::new(AtomicU64::new(0));
        let stderr_bytes = Arc::new(AtomicU64::new(0));
        let stdout = child.stdout.take().map(|stdout| {
            pump(
                stdout,
                Kind::Stdout,
                writer.clone(),
                stdout_bytes.clone(),
                pgid,
                termination.clone(),
            )
        });
        let stderr = child.stderr.take().map(|stderr| {
            pump(
                stderr,
                Kind::Stderr,
                writer.clone(),
                stderr_bytes.clone(),
                pgid,
                termination.clone(),
            )
        });
        let runtime_started = Instant::now();
        let status = child.wait();
        let runtime_ns = elapsed_ns(runtime_started);
        let drain_started = Instant::now();
        if let Some(thread) = stdout {
            let _ = thread.join();
        }
        if let Some(thread) = stderr {
            let _ = thread.join();
        }
        let drain_ns = elapsed_ns(drain_started);
        let reason = termination.reason.load(Ordering::Acquire);
        match (status, reason) {
            (Ok(status), 0 | 1) => {
                let exit = Exit {
                    code: status.code(),
                    signal: status.signal(),
                    stopped: reason == 1,
                    stdout_bytes: stdout_bytes.load(Ordering::Relaxed),
                    stderr_bytes: stderr_bytes.load(Ordering::Relaxed),
                    timing: DaemonTiming {
                        accept_bind_ns,
                        decode_ns,
                        spawn_ns,
                        runtime_ns,
                        drain_ns,
                    },
                };
                let _ = write_locked(&writer, Kind::Exit, &exit.encode());
            }
            (_, 3) => send_error_locked(&writer, RemoteError::OutputFailed),
            _ => send_error_locked(&writer, RemoteError::InfrastructureLost),
        }
        termination.finished.store(true, Ordering::Release);
        if let Ok(stream) = writer.lock() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        let _ = stop_thread.join();
    }

    enum MountEvent {
        Ready(io::Result<Vec<u8>>),
        Close,
        Lost,
        Exited(io::Result<std::process::ExitStatus>),
    }

    fn handle_mount(mut stream: ControlStream, payload: Vec<u8>, shared: Arc<Shared>) {
        let request = match MountRequest::decode(&payload) {
            Ok(request) => request,
            Err(_) => {
                send_error(&mut stream, RemoteError::InvalidRequest);
                return;
            }
        };
        if validate_root(&request.root).is_err() {
            send_error(&mut stream, RemoteError::InvalidRequest);
            return;
        }
        let root = PathBuf::from(OsString::from_vec(request.root.clone()));
        let endpoint = OsString::from_vec(request.endpoint);
        let capability = hex(&request.capability);
        let termination = Arc::new(Termination::new());
        let (mut child, created_root) = {
            let Ok(mut state) = shared.state.lock() else {
                send_error(&mut stream, RemoteError::InfrastructureLost);
                return;
            };
            if !state.owner_live || state.owner_id != request.owner_id {
                send_error(&mut stream, RemoteError::Unauthorized);
                return;
            }
            if state.active.len().saturating_add(state.mounts.len()) >= shared.limit {
                send_error(&mut stream, RemoteError::LimitExceeded);
                return;
            }
            if state.mounts.contains_key(&request.workspace_id)
                || state
                    .mounts
                    .values()
                    .any(|mount| mount.root == request.root)
            {
                send_error(&mut stream, RemoteError::InvalidRequest);
                return;
            }
            let created_root = match prepare_mount_root(&root) {
                Ok(created) => created,
                Err(_) => {
                    send_error(&mut stream, RemoteError::InvalidRequest);
                    return;
                }
            };
            let mut command = Command::new(FIXED_HELPER);
            command
                .arg(endpoint)
                .arg(&capability)
                .arg(&root)
                .env("LAYERFS_FIXED_HELPER", "1")
                .env("LAYERFS_OWNED_HELPER", FIXED_HELPER)
                .env("LAYERFS_OWNED_ROOT", &root)
                .env("LAYERFS_OWNED_CAPABILITY", &capability)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .process_group(0);
            let child = match command.spawn() {
                Ok(child) => child,
                Err(_) => {
                    if created_root {
                        let _ = fs::remove_dir(&root);
                    }
                    send_error(&mut stream, RemoteError::InvalidRequest);
                    return;
                }
            };
            state.mounts.insert(
                request.workspace_id,
                ActiveMount {
                    root: request.root.clone(),
                    alive: true,
                    ready: false,
                    pgid: child.id() as i32,
                    termination: termination.clone(),
                },
            );
            (child, created_root)
        };
        let _guard = MountGuard {
            shared: shared.clone(),
            id: request.workspace_id,
        };
        let pgid = child.id() as i32;
        let Some(stdout) = child.stdout.take() else {
            termination.terminate(pgid, 2);
            let _ = child.wait();
            let _ = cleanup_mount(&root, created_root);
            send_error(&mut stream, RemoteError::InfrastructureLost);
            return;
        };
        let stderr = child.stderr.take().map(drain_mount_output);
        let reader = match stream.try_clone() {
            Ok(reader) => reader,
            Err(_) => {
                termination.terminate(pgid, 2);
                let _ = child.wait();
                let _ = cleanup_mount(&root, created_root);
                if let Some(stderr) = stderr {
                    let _ = stderr.join();
                }
                send_error(&mut stream, RemoteError::InfrastructureLost);
                return;
            }
        };
        let finished = Arc::new(AtomicBool::new(false));
        let (send, receive) = std::sync::mpsc::channel();
        let ready_send = send.clone();
        let ready_root = request.root.clone();
        let stdout = std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            let ready = read_helper_ready(&mut stdout, &ready_root);
            let _ = ready_send.send(MountEvent::Ready(ready));
            let _ = io::copy(&mut stdout, &mut io::sink());
        });
        let wait_send = send.clone();
        let wait_shared = shared.clone();
        let wait_workspace = request.workspace_id;
        let waiter = std::thread::spawn(move || {
            let exit = child.wait();
            finish_mount(&wait_shared, wait_workspace);
            let _ = wait_send.send(MountEvent::Exited(exit));
        });
        let lifecycle = watch_mount(
            reader,
            send,
            finished.clone(),
            shared.clone(),
            request.workspace_id,
        );

        let mut status = None;
        let mountinfo = match receive.recv_timeout(Duration::from_secs(10)) {
            Ok(MountEvent::Ready(Ok(mountinfo))) => Some(mountinfo),
            Ok(MountEvent::Exited(exit)) => {
                status = Some(exit);
                None
            }
            Ok(MountEvent::Ready(Err(_)) | MountEvent::Close | MountEvent::Lost) | Err(_) => None,
        };
        let Some(mountinfo) = mountinfo else {
            termination.terminate(pgid, 2);
            wait_mount_exit(&receive, &mut status);
            terminate_workspace_execs(&shared, request.workspace_id);
            let _ = cleanup_mount(&root, created_root);
            termination.finished.store(true, Ordering::Release);
            send_error(&mut stream, RemoteError::InfrastructureLost);
            finished.store(true, Ordering::Release);
            let _ = stream.shutdown(std::net::Shutdown::Both);
            let _ = waiter.join();
            let _ = stdout.join();
            if let Some(stderr) = stderr {
                let _ = stderr.join();
            }
            let _ = lifecycle.join();
            return;
        };
        let ready = shared.state.lock().is_ok_and(|mut state| {
            if !state.owner_live || termination.reason.load(Ordering::Acquire) != 0 {
                return false;
            }
            state
                .mounts
                .get_mut(&request.workspace_id)
                .is_some_and(|mount| {
                    if !mount.alive {
                        return false;
                    }
                    mount.ready = true;
                    true
                })
        });
        if !ready || protocol::write_frame(&mut stream, Kind::WorkspaceReady, &mountinfo).is_err() {
            termination.terminate(pgid, 2);
            wait_mount_exit(&receive, &mut status);
            terminate_workspace_execs(&shared, request.workspace_id);
            let _ = cleanup_mount(&root, created_root);
            termination.finished.store(true, Ordering::Release);
            finished.store(true, Ordering::Release);
            let _ = stream.shutdown(std::net::Shutdown::Both);
            let _ = waiter.join();
            let _ = stdout.join();
            if let Some(stderr) = stderr {
                let _ = stderr.join();
            }
            let _ = lifecycle.join();
            return;
        }

        let mut close = false;
        let mut lost = false;
        while status.is_none() || (!close && !lost) {
            let timeout = if close {
                Duration::from_secs(10)
            } else if status.is_some() {
                Duration::from_secs(2)
            } else {
                Duration::from_secs(24 * 60 * 60)
            };
            match receive.recv_timeout(timeout) {
                Ok(MountEvent::Close) if !close => {
                    deactivate_mount(&shared, request.workspace_id);
                    close = true;
                }
                Ok(MountEvent::Exited(exit)) => {
                    deactivate_mount(&shared, request.workspace_id);
                    status = Some(exit);
                }
                Ok(MountEvent::Lost) | Ok(MountEvent::Close) | Ok(MountEvent::Ready(_)) => {
                    deactivate_mount(&shared, request.workspace_id);
                    lost = true;
                    termination.terminate(pgid, 2);
                }
                Err(_) => {
                    deactivate_mount(&shared, request.workspace_id);
                    lost = true;
                    termination.terminate(pgid, 2);
                }
            }
        }
        terminate_workspace_execs(&shared, request.workspace_id);
        let cleanup = cleanup_mount(&root, created_root);
        let success = close
            && !lost
            && termination.reason.load(Ordering::Acquire) == 0
            && status.is_some_and(|status| status.is_ok_and(|status| status.success()))
            && cleanup.is_ok();
        termination.finished.store(true, Ordering::Release);
        if success {
            let _ = protocol::write_frame(&mut stream, Kind::WorkspaceClosed, &[]);
        } else if close && !lost {
            send_error(&mut stream, RemoteError::InfrastructureLost);
        }
        finished.store(true, Ordering::Release);
        let _ = stream.shutdown(std::net::Shutdown::Both);
        let _ = waiter.join();
        let _ = stdout.join();
        if let Some(stderr) = stderr {
            let _ = stderr.join();
        }
        let _ = lifecycle.join();
    }

    fn watch_mount(
        mut stream: ControlStream,
        send: std::sync::mpsc::Sender<MountEvent>,
        finished: Arc<AtomicBool>,
        shared: Arc<Shared>,
        workspace_id: [u8; 16],
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut close = false;
            loop {
                match protocol::read_frame(&mut stream) {
                    Ok(frame) if frame.kind == Kind::Close && !close => {
                        deactivate_mount(&shared, workspace_id);
                        close = true;
                        if send.send(MountEvent::Close).is_err() {
                            return;
                        }
                    }
                    _ if !finished.load(Ordering::Acquire) => {
                        deactivate_mount(&shared, workspace_id);
                        let _ = send.send(MountEvent::Lost);
                        return;
                    }
                    _ => return,
                }
            }
        })
    }

    fn wait_mount_exit(
        receive: &std::sync::mpsc::Receiver<MountEvent>,
        status: &mut Option<io::Result<std::process::ExitStatus>>,
    ) {
        while status.is_none() {
            match receive.recv_timeout(Duration::from_secs(2)) {
                Ok(MountEvent::Exited(exit)) => *status = Some(exit),
                Err(_) => return,
                _ => {}
            }
        }
    }

    fn watch_exec(
        mut stream: ControlStream,
        pgid: i32,
        termination: Arc<Termination>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || match protocol::read_frame(&mut stream) {
            Ok(frame) if frame.kind == Kind::Stop => termination.terminate(pgid, 1),
            _ if !termination.finished.load(Ordering::Acquire) => termination.terminate(pgid, 2),
            _ => {}
        })
    }

    fn pump<R: Read + Send + 'static>(
        mut reader: R,
        kind: Kind,
        writer: Arc<Mutex<ControlStream>>,
        count: Arc<AtomicU64>,
        pgid: i32,
        termination: Arc<Termination>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut bytes = [0; 16 * 1024];
            loop {
                let read = match reader.read(&mut bytes) {
                    Ok(0) => return,
                    Ok(read) => read,
                    Err(_) => {
                        termination.terminate(pgid, 3);
                        return;
                    }
                };
                count.fetch_add(read as u64, Ordering::Relaxed);
                if write_locked(&writer, kind, &bytes[..read]).is_err() {
                    termination.terminate(pgid, 3);
                    return;
                }
            }
        })
    }

    fn write_locked(writer: &Mutex<ControlStream>, kind: Kind, payload: &[u8]) -> io::Result<()> {
        let mut writer = writer
            .lock()
            .map_err(|_| io::Error::other("daemon stream writer"))?;
        protocol::write_frame(&mut *writer, kind, payload)
    }

    fn send_error_locked(writer: &Mutex<ControlStream>, error: RemoteError) {
        let _ = write_locked(writer, Kind::Error, &[error as u8]);
    }

    fn send_error(stream: &mut ControlStream, error: RemoteError) {
        let _ = protocol::write_frame(stream, Kind::Error, &[error as u8]);
    }

    fn read_helper_ready(reader: &mut impl BufRead, root: &[u8]) -> io::Result<Vec<u8>> {
        if read_bounded_line(reader, 32)? != b"READY" {
            return Err(protocol::invalid("FUSE helper readiness"));
        }
        let line = read_bounded_line(reader, MAX_CONTROL)?;
        let reported = line
            .strip_prefix(b"MOUNTINFO\t")
            .ok_or_else(|| protocol::invalid("FUSE helper mountinfo"))?;
        let actual = live_fuse_mount_line(root)?
            .ok_or_else(|| protocol::invalid("FUSE helper live mount"))?;
        if reported != actual {
            return Err(protocol::invalid("FUSE helper mount identity"));
        }
        Ok(actual)
    }

    fn read_bounded_line(reader: &mut impl BufRead, limit: usize) -> io::Result<Vec<u8>> {
        let mut output = Vec::new();
        loop {
            let (consumed, done) = {
                let bytes = reader.fill_buf()?;
                if bytes.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "FUSE helper line",
                    ));
                }
                let newline = bytes.iter().position(|byte| *byte == b'\n');
                let consumed = newline.map_or(bytes.len(), |index| index + 1);
                let copied = newline.unwrap_or(bytes.len());
                if output
                    .len()
                    .checked_add(copied)
                    .is_none_or(|length| length > limit)
                {
                    return Err(protocol::invalid("FUSE helper line length"));
                }
                output.extend_from_slice(&bytes[..copied]);
                (consumed, newline.is_some())
            };
            reader.consume(consumed);
            if done {
                return Ok(output);
            }
        }
    }

    fn drain_mount_output<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let _ = io::copy(&mut reader, &mut io::sink());
        })
    }

    fn validate_root(bytes: &[u8]) -> io::Result<()> {
        if bytes.contains(&0) {
            return Err(protocol::invalid("daemon Workspace root NUL"));
        }
        let path = Path::new(OsStr::from_bytes(bytes));
        if !path.is_absolute()
            || !path.starts_with(WORKSPACE_ROOT)
            || path == Path::new(WORKSPACE_ROOT)
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::CurDir | Component::Prefix(_)
                )
            })
        {
            return Err(protocol::invalid("daemon Workspace root"));
        }
        Ok(())
    }

    fn prepare_mount_root(root: &Path) -> io::Result<bool> {
        match fs::metadata(root) {
            Ok(metadata) if metadata.is_dir() => Ok(false),
            Ok(_) => Err(protocol::invalid("daemon Workspace root type")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(root)?;
                Ok(true)
            }
            Err(error) => Err(error),
        }
    }

    fn cleanup_mount(root: &Path, created_root: bool) -> io::Result<()> {
        if live_fuse_mount_line(root.as_os_str().as_bytes())?.is_some() {
            umount2(root, MntFlags::MNT_DETACH).map_err(io::Error::other)?;
        }
        if live_fuse_mount_line(root.as_os_str().as_bytes())?.is_some() {
            return Err(io::Error::other("daemon FUSE mount residue"));
        }
        if created_root {
            fs::remove_dir(root)?;
        }
        Ok(())
    }

    fn terminate_workspace_execs(shared: &Shared, workspace_id: [u8; 16]) {
        let active = shared
            .state
            .lock()
            .map(|state| {
                state
                    .active
                    .values()
                    .filter(|active| active.workspace_id == workspace_id)
                    .map(|active| (active.pgid, active.termination.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (pgid, termination) in active {
            termination.terminate(pgid, 2);
        }
    }

    fn deactivate_mount(shared: &Shared, workspace_id: [u8; 16]) {
        if let Ok(mut state) = shared.state.lock() {
            if let Some(mount) = state.mounts.get_mut(&workspace_id) {
                mount.ready = false;
            }
        }
    }

    fn finish_mount(shared: &Shared, workspace_id: [u8; 16]) {
        if let Ok(mut state) = shared.state.lock() {
            if let Some(mount) = state.mounts.get_mut(&workspace_id) {
                mount.alive = false;
                mount.ready = false;
            }
        }
    }

    fn hex(bytes: &[u8]) -> String {
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            use std::fmt::Write as _;
            let _ = write!(output, "{byte:02x}");
        }
        output
    }

    fn validate_cwd(bytes: &[u8]) -> io::Result<()> {
        validate_root(bytes)?;
        let path = Path::new(OsStr::from_bytes(bytes));
        if !fs::metadata(path).is_ok_and(|metadata| metadata.is_dir())
            || !is_live_fuse_mount(bytes)?
        {
            return Err(protocol::invalid("daemon Workspace cwd"));
        }
        Ok(())
    }

    fn is_live_fuse_mount(cwd: &[u8]) -> io::Result<bool> {
        Ok(live_fuse_mount_line(cwd)?.is_some())
    }

    fn live_fuse_mount_line(cwd: &[u8]) -> io::Result<Option<Vec<u8>>> {
        let mountinfo = fs::read("/proc/self/mountinfo")?;
        for line in mountinfo.split(|byte| *byte == b'\n') {
            let fields = line.split(|byte| *byte == b' ').collect::<Vec<_>>();
            let Some(separator) = fields.iter().position(|field| *field == b"-") else {
                continue;
            };
            if separator + 1 >= fields.len() || !fields[separator + 1].starts_with(b"fuse") {
                continue;
            }
            if fields
                .get(4)
                .is_some_and(|field| unescape_mount(field) == cwd)
            {
                return Ok(Some(line.to_vec()));
            }
        }
        Ok(None)
    }

    fn unescape_mount(value: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(value.len());
        let mut index = 0;
        while index < value.len() {
            if value[index] == b'\\' && index + 3 < value.len() {
                let digits = &value[index + 1..index + 4];
                if digits.iter().all(|byte| matches!(byte, b'0'..=b'7')) {
                    output.push(
                        (digits[0] - b'0') * 64 + (digits[1] - b'0') * 8 + (digits[2] - b'0'),
                    );
                    index += 4;
                    continue;
                }
            }
            output.push(value[index]);
            index += 1;
        }
        output
    }

    fn terminate_group(pgid: i32) {
        let pgid = Pid::from_raw(pgid);
        let _ = killpg(pgid, Signal::SIGTERM);
        std::thread::sleep(Duration::from_millis(100));
        let _ = killpg(pgid, Signal::SIGKILL);
    }

    fn peer(stream: &UnixStream) -> io::Result<Peer> {
        let credentials = getsockopt(stream, PeerCredentials).map_err(io::Error::other)?;
        Ok(Peer {
            pid: credentials
                .pid()
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "daemon peer PID"))?,
            uid: credentials.uid(),
            gid: credentials.gid(),
        })
    }

    fn admission_limit() -> io::Result<usize> {
        let (soft, _) = getrlimit(Resource::RLIMIT_NOFILE).map_err(io::Error::other)?;
        Ok(usize::try_from(soft.saturating_sub(32) / 8)
            .unwrap_or(256)
            .clamp(1, 256))
    }

    fn read_capability(path: &Path) -> io::Result<[u8; 32]> {
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() || metadata.mode() & 0o077 != 0 || metadata.len() != 32 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "daemon capability protection",
            ));
        }
        fs::read(path)?
            .try_into()
            .map_err(|_| protocol::invalid("daemon capability length"))
    }

    fn random<const N: usize>() -> io::Result<[u8; N]> {
        let mut bytes = [0; N];
        fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn constant_time_equal(left: &[u8], right: &[u8; 32]) -> bool {
        left.len() == 32
            && left
                .iter()
                .zip(right)
                .fold(0_u8, |different, (left, right)| different | (left ^ right))
                == 0
    }

    fn elapsed_ns(started: Instant) -> u64 {
        started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
    }
}

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = linux::run() {
        eprintln!("layerfs-daemon: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("layerfs-daemon: Linux is required");
    std::process::exit(1);
}

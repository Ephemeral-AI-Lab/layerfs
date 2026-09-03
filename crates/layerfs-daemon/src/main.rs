#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod linux {
    use layerfs_daemon::protocol::{
        self, CgroupResourceSample, DaemonTiming, ExecRequest, Exit, Kind, MountRequest,
        RemoteError, ResourceSampleFinishRequest, ResourceSampleRequest, ServerHello,
        AUTH_OK_BYTES, BOUND_AUTH_BYTES, BOUND_OK_BYTES, CAPABILITY_PATH, CGROUP_STAT_FIELDS,
        CLIENT_AUTH_BYTES, MAX_CONTROL, SOCKET_PATH, WORKSPACE_ROOT,
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
    use std::io::{self, BufRead, BufReader, Read, Seek, Write};
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
        samples: BTreeMap<[u8; 16], ResourceSampler>,
        sample_starting: bool,
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

    struct ResourceSampler {
        stop: Arc<AtomicBool>,
        threads: Vec<std::thread::JoinHandle<io::Result<()>>>,
        aggregate: Arc<Mutex<SampleAggregate>>,
        files: CgroupFiles,
        clock: Instant,
        origin_unix_ns: u64,
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
                samples: BTreeMap::new(),
                sample_starting: false,
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
        while !state.active.is_empty() || !state.mounts.is_empty() || !state.samples.is_empty() {
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
            let (active, samples) = {
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
                (active, std::mem::take(&mut state.samples))
            };
            for (pgid, termination) in active {
                termination.terminate(pgid, 2);
            }
            drop(samples);
            shared.drained.notify_all();
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
            Kind::ResourceSampleStart => {
                handle_resource_sample_start(stream, accepted, frame.payload, shared)
            }
            Kind::ResourceSampleFinish => {
                handle_resource_sample_finish(stream, frame.payload, shared)
            }
            _ => send_error(&mut stream, RemoteError::InvalidRequest),
        }
    }

    fn handle_resource_sample_start(
        mut stream: ControlStream,
        calibration_started: Instant,
        payload: Vec<u8>,
        shared: Arc<Shared>,
    ) {
        if stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .is_err()
            || stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .is_err()
        {
            return;
        }
        let Ok(request) = ResourceSampleRequest::decode(&payload) else {
            send_error(&mut stream, RemoteError::InvalidRequest);
            return;
        };
        let reserved = shared.state.lock().is_ok_and(|mut state| {
            if !state.owner_live
                || state.owner_id != request.owner_id
                || !state
                    .mounts
                    .get(&request.workspace_id)
                    .is_some_and(|mount| mount.ready)
                || state.sample_starting
                || !state.samples.is_empty()
            {
                return false;
            }
            state.sample_starting = true;
            true
        });
        if !reserved {
            send_error(&mut stream, RemoteError::InvalidRequest);
            return;
        }
        let sampler = match ResourceSampler::start(calibration_started) {
            Ok(sampler) => sampler,
            Err(_) => {
                if let Ok(mut state) = shared.state.lock() {
                    state.sample_starting = false;
                }
                send_error(&mut stream, RemoteError::InfrastructureLost);
                return;
            }
        };
        let accepted = shared.state.lock().is_ok_and(|mut state| {
            state.sample_starting = false;
            if !state.owner_live
                || state.owner_id != request.owner_id
                || !state
                    .mounts
                    .get(&request.workspace_id)
                    .is_some_and(|mount| mount.ready)
                || !state.samples.is_empty()
            {
                return false;
            }
            state.samples.insert(request.workspace_id, sampler);
            true
        });
        if accepted {
            if protocol::write_clock_response(
                &mut stream,
                1,
                elapsed_ns(calibration_started).saturating_add(1),
            )
            .is_err()
            {
                return;
            }
            // The bound start stream supplies clock probes after all setup is complete.
            // Disconnecting it does not disarm the independently owned sampler.
            for _ in 0..5 {
                let Ok(probe) = protocol::read_frame(&mut stream) else {
                    return;
                };
                let received = elapsed_ns(calibration_started).saturating_add(1);
                let valid = probe.kind == Kind::ResourceSampleClock
                    && shared.state.lock().is_ok_and(|state| {
                        state.owner_live
                            && state.owner_id == request.owner_id
                            && state
                                .samples
                                .get(&request.workspace_id)
                                .is_some_and(|sample| sample.clock == calibration_started)
                    });
                if !valid {
                    send_error(&mut stream, RemoteError::InvalidRequest);
                    return;
                }
                let sent = elapsed_ns(calibration_started).saturating_add(1);
                if protocol::write_clock_response(&mut stream, received, sent).is_err() {
                    return;
                }
            }
        } else {
            send_error(&mut stream, RemoteError::InvalidRequest);
        }
    }

    fn handle_resource_sample_finish(
        mut stream: ControlStream,
        payload: Vec<u8>,
        shared: Arc<Shared>,
    ) {
        let Ok(request) = ResourceSampleFinishRequest::decode(&payload) else {
            send_error(&mut stream, RemoteError::InvalidRequest);
            return;
        };
        let sampler = shared.state.lock().ok().and_then(|mut state| {
            (state.owner_live && state.owner_id == request.owner_id)
                .then(|| state.samples.remove(&request.workspace_id))
                .flatten()
        });
        let Some(sampler) = sampler else {
            send_error(&mut stream, RemoteError::InvalidRequest);
            return;
        };
        match sampler.finish(
            request.t0_unix_ns,
            request.t3_unix_ns,
            request.uncertainty_ns,
        ) {
            Ok(sample) => {
                let _ = protocol::write_frame(&mut stream, Kind::ResourceSample, &sample.encode());
            }
            Err(_) => send_error(&mut stream, RemoteError::InfrastructureLost),
        }
        shared.drained.notify_all();
    }

    struct CgroupFiles {
        current: fs::File,
        peak: fs::File,
        swap: fs::File,
        events: fs::File,
        stat: fs::File,
    }

    #[derive(Clone, Copy)]
    struct CgroupPoint {
        current: u64,
        lifetime_peak: u64,
        swap: u64,
        oom: u64,
        oom_kill: u64,
        stat: [u64; CGROUP_STAT_FIELDS],
    }

    #[derive(Clone, Copy)]
    struct SampleRecord {
        unix_ns: u64,
        point: CgroupPoint,
    }

    struct SampleAggregate {
        baseline_endpoint: CgroupPoint,
        records: Vec<SampleRecord>,
        overflow: bool,
    }

    impl ResourceSampler {
        fn start(clock: Instant) -> io::Result<Self> {
            const THREADS: usize = 2;
            let mut files = CgroupFiles::open()?;
            let provisional = files.read()?;
            let origin_unix_ns = 1_u64;
            let stop = Arc::new(AtomicBool::new(false));
            let aggregate = Arc::new(Mutex::new(SampleAggregate::new(
                provisional,
                origin_unix_ns,
            )));
            let mut threads = Vec::with_capacity(THREADS);
            for _ in 0..THREADS {
                let mut thread_files = CgroupFiles::open()?;
                let thread_stop = stop.clone();
                let thread_aggregate = aggregate.clone();
                threads.push(std::thread::spawn(move || {
                    while !thread_stop.load(Ordering::Acquire) {
                        let point = thread_files.read_phase(provisional)?;
                        let elapsed = elapsed_ns(clock);
                        thread_aggregate
                            .lock()
                            .map_err(|_| io::Error::other("cgroup sample aggregate"))?
                            .record(point, origin_unix_ns.saturating_add(elapsed));
                    }
                    Ok(())
                }));
            }
            let warmup_deadline = Instant::now() + Duration::from_secs(2);
            while aggregate
                .lock()
                .map_err(|_| io::Error::other("cgroup sample aggregate"))?
                .records
                .len()
                < THREADS * 2
            {
                if threads.iter().any(|thread| thread.is_finished())
                    || Instant::now() >= warmup_deadline
                {
                    stop.store(true, Ordering::Release);
                    for thread in threads {
                        let _ = thread.join();
                    }
                    return Err(io::Error::other("cgroup sampler warmup"));
                }
                std::thread::yield_now();
            }
            let baseline = files.read()?;
            aggregate
                .lock()
                .map_err(|_| io::Error::other("cgroup sample aggregate"))?
                .reset(baseline, origin_unix_ns.saturating_add(elapsed_ns(clock)));
            Ok(Self {
                stop,
                threads,
                aggregate,
                files,
                clock,
                origin_unix_ns,
            })
        }

        fn finish(
            mut self,
            t0_unix_ns: u64,
            t3_unix_ns: u64,
            uncertainty_ns: u64,
        ) -> io::Result<CgroupResourceSample> {
            self.stop.store(true, Ordering::Release);
            for thread in self.threads.drain(..) {
                thread
                    .join()
                    .map_err(|_| io::Error::other("cgroup sampler thread"))??;
            }
            let final_point = self.files.read()?;
            let mut aggregate = self
                .aggregate
                .lock()
                .map_err(|_| io::Error::other("cgroup sample aggregate"))?;
            if aggregate.records.len() == SampleAggregate::MAX_RECORDS {
                aggregate.records.pop();
            }
            aggregate.record(
                final_point,
                self.origin_unix_ns.saturating_add(elapsed_ns(self.clock)),
            );
            if (t0_unix_ns, t3_unix_ns, uncertainty_ns) == (0, 0, 0) {
                let start = aggregate
                    .records
                    .first()
                    .ok_or_else(|| io::Error::other("missing window start"))?
                    .unix_ns;
                let end = aggregate
                    .records
                    .last()
                    .ok_or_else(|| io::Error::other("missing window end"))?
                    .unix_ns;
                aggregate.report(final_point, start, end, 0, 2)
            } else {
                aggregate.report(final_point, t0_unix_ns, t3_unix_ns, uncertainty_ns, 2)
            }
        }
    }

    impl Drop for ResourceSampler {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            for thread in self.threads.drain(..) {
                let _ = thread.join();
            }
        }
    }

    impl SampleAggregate {
        const MAX_RECORDS: usize = 16_384;

        fn new(baseline: CgroupPoint, unix_ns: u64) -> Self {
            let mut records = Vec::with_capacity(Self::MAX_RECORDS);
            records.push(SampleRecord {
                unix_ns,
                point: baseline,
            });
            Self {
                baseline_endpoint: baseline,
                records,
                overflow: false,
            }
        }

        fn reset(&mut self, baseline: CgroupPoint, unix_ns: u64) {
            self.baseline_endpoint = baseline;
            self.records.clear();
            self.records.push(SampleRecord {
                unix_ns,
                point: baseline,
            });
            self.overflow = false;
        }

        fn record(&mut self, point: CgroupPoint, unix_ns: u64) {
            if self
                .records
                .last()
                .is_some_and(|record| unix_ns <= record.unix_ns)
            {
                return;
            }
            if self.records.len() == Self::MAX_RECORDS {
                self.overflow = true;
                return;
            }
            self.records.push(SampleRecord { unix_ns, point });
        }

        fn report(
            &self,
            endpoint: CgroupPoint,
            t0_unix_ns: u64,
            t3_unix_ns: u64,
            uncertainty_ns: u64,
            threads: u64,
        ) -> io::Result<CgroupResourceSample> {
            let t0_lo = t0_unix_ns
                .checked_sub(uncertainty_ns)
                .ok_or_else(|| io::Error::other("T0 clock bound"))?;
            let t0_hi = t0_unix_ns
                .checked_add(uncertainty_ns)
                .ok_or_else(|| io::Error::other("T0 clock bound"))?;
            let t3_lo = t3_unix_ns
                .checked_sub(uncertainty_ns)
                .ok_or_else(|| io::Error::other("T3 clock bound"))?;
            let t3_hi = t3_unix_ns
                .checked_add(uncertainty_ns)
                .ok_or_else(|| io::Error::other("T3 clock bound"))?;
            let before = self
                .records
                .iter()
                .rposition(|record| record.unix_ns <= t0_lo)
                .unwrap_or(0);
            let after = self
                .records
                .iter()
                .position(|record| record.unix_ns >= t3_hi)
                .unwrap_or(self.records.len().saturating_sub(1));
            if after < before || self.records.is_empty() {
                return Err(io::Error::other("cgroup sample boundary order"));
            }
            let selected = &self.records[before..=after];
            let baseline = selected[0];
            let final_sample = selected[selected.len() - 1];
            let mut peak = baseline.point;
            let mut dirty_writeback_peak =
                baseline.point.stat[3].saturating_add(baseline.point.stat[4]);
            let mut interior = 0_u64;
            let mut interior_sample_ns = 0;
            let mut gaps = Vec::with_capacity(selected.len().saturating_sub(1));
            for pair in selected.windows(2) {
                gaps.push(pair[1].unix_ns.saturating_sub(pair[0].unix_ns));
            }
            for record in selected {
                if record.unix_ns > t0_hi && record.unix_ns < t3_lo {
                    interior = interior.saturating_add(1);
                    interior_sample_ns = record.unix_ns;
                }
                if record.unix_ns < t0_lo || record.unix_ns > t3_hi {
                    continue;
                }
                peak.current = peak.current.max(record.point.current);
                peak.lifetime_peak = peak.lifetime_peak.max(record.point.lifetime_peak);
                peak.swap = peak.swap.max(record.point.swap);
                for index in 0..CGROUP_STAT_FIELDS {
                    peak.stat[index] = peak.stat[index].max(record.point.stat[index]);
                }
                dirty_writeback_peak = dirty_writeback_peak
                    .max(record.point.stat[3].saturating_add(record.point.stat[4]));
            }
            gaps.sort_unstable();
            let maximum_gap_ns = gaps.iter().copied().max().unwrap_or(u64::MAX);
            let sample_interval_ns = gaps.get(gaps.len() / 2).copied().unwrap_or(u64::MAX);
            let dirty_writeback_baseline =
                baseline.point.stat[3].saturating_add(baseline.point.stat[4]);
            Ok(CgroupResourceSample {
                memory_current_baseline: baseline.point.current,
                memory_current_peak: peak.current,
                memory_current_final: final_sample.point.current,
                memory_incremental_peak: peak.current.saturating_sub(baseline.point.current),
                memory_lifetime_peak_baseline: baseline.point.lifetime_peak,
                memory_lifetime_peak_final: final_sample.point.lifetime_peak,
                swap_baseline: baseline.point.swap,
                swap_peak: peak.swap,
                swap_final: final_sample.point.swap,
                oom_baseline: self.baseline_endpoint.oom,
                oom_final: endpoint.oom,
                oom_delta: endpoint.oom.saturating_sub(self.baseline_endpoint.oom),
                oom_kill_baseline: self.baseline_endpoint.oom_kill,
                oom_kill_final: endpoint.oom_kill,
                oom_kill_delta: endpoint
                    .oom_kill
                    .saturating_sub(self.baseline_endpoint.oom_kill),
                dirty_writeback_baseline,
                dirty_writeback_peak,
                dirty_writeback_incremental_peak: dirty_writeback_peak
                    .saturating_sub(dirty_writeback_baseline),
                sample_interval_ns,
                sample_count: selected.len() as u64,
                first_sample_ns: baseline.unix_ns,
                last_sample_ns: final_sample.unix_ns,
                maximum_sample_gap_ns: maximum_gap_ns,
                started_ns: t0_unix_ns,
                finished_ns: t3_unix_ns,
                sampler_thread_count: threads,
                interior_sample_ns,
                stat_baseline: baseline.point.stat,
                stat_peak: peak.stat,
                stat_final: final_sample.point.stat,
                t0_boundary_sampled: baseline.unix_ns <= t0_lo
                    && t0_hi - baseline.unix_ns <= 1_000_000,
                t3_boundary_sampled: final_sample.unix_ns >= t3_hi
                    && final_sample.unix_ns - t3_lo <= 1_000_000,
                interior_sampled: interior != 0,
                sample_overflow: self.overflow,
            })
        }
    }

    impl CgroupFiles {
        fn open() -> io::Result<Self> {
            Ok(Self {
                current: fs::File::open("/sys/fs/cgroup/memory.current")?,
                peak: fs::File::open("/sys/fs/cgroup/memory.peak")?,
                swap: fs::File::open("/sys/fs/cgroup/memory.swap.current")?,
                events: fs::File::open("/sys/fs/cgroup/memory.events")?,
                stat: fs::File::open("/sys/fs/cgroup/memory.stat")?,
            })
        }

        fn read(&mut self) -> io::Result<CgroupPoint> {
            let mut buffer = [0_u8; 4096];
            let current = read_cgroup_number(&mut self.current, &mut buffer)?;
            let lifetime_peak = read_cgroup_number(&mut self.peak, &mut buffer)?;
            let swap = read_cgroup_number(&mut self.swap, &mut buffer)?;
            let events = read_cgroup_text(&mut self.events, &mut buffer)?;
            let oom = cgroup_key(events, "oom")?;
            let oom_kill = cgroup_key(events, "oom_kill")?;
            let stat_text = read_cgroup_text(&mut self.stat, &mut buffer)?;
            let mut stat = [0_u64; CGROUP_STAT_FIELDS];
            for (index, name) in [
                "anon",
                "file",
                "shmem",
                "file_dirty",
                "file_writeback",
                "kernel",
                "slab",
                "sock",
            ]
            .into_iter()
            .enumerate()
            {
                stat[index] = cgroup_key(stat_text, name)?;
            }
            Ok(CgroupPoint {
                current,
                lifetime_peak,
                swap,
                oom,
                oom_kill,
                stat,
            })
        }

        fn read_phase(&mut self, prior: CgroupPoint) -> io::Result<CgroupPoint> {
            let mut buffer = [0_u8; 4096];
            let current = read_cgroup_number(&mut self.current, &mut buffer)?;
            let lifetime_peak = read_cgroup_number(&mut self.peak, &mut buffer)?;
            let swap = read_cgroup_number(&mut self.swap, &mut buffer)?;
            let stat_text = read_cgroup_text(&mut self.stat, &mut buffer)?;
            let mut stat = [0_u64; CGROUP_STAT_FIELDS];
            for (index, name) in [
                "anon",
                "file",
                "shmem",
                "file_dirty",
                "file_writeback",
                "kernel",
                "slab",
                "sock",
            ]
            .into_iter()
            .enumerate()
            {
                stat[index] = cgroup_key(stat_text, name)?;
            }
            Ok(CgroupPoint {
                current,
                lifetime_peak,
                swap,
                stat,
                ..prior
            })
        }
    }

    fn read_cgroup_number(file: &mut fs::File, buffer: &mut [u8]) -> io::Result<u64> {
        read_cgroup_text(file, buffer)?
            .trim()
            .parse()
            .map_err(|_| protocol::invalid("cgroup number"))
    }

    fn read_cgroup_text<'a>(file: &mut fs::File, buffer: &'a mut [u8]) -> io::Result<&'a str> {
        file.rewind()?;
        let read = file.read(buffer)?;
        std::str::from_utf8(&buffer[..read]).map_err(|_| protocol::invalid("cgroup text"))
    }

    fn cgroup_key(text: &str, key: &str) -> io::Result<u64> {
        text.lines()
            .find_map(|line| {
                let (name, value) = line.split_once(' ')?;
                (name == key).then_some(value)
            })
            .ok_or_else(|| protocol::invalid("cgroup key"))?
            .parse()
            .map_err(|_| protocol::invalid("cgroup value"))
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
            (status, reason) => {
                eprintln!("layerfs-daemon: execution failed reason={reason} status={status:?}");
                send_error_locked(&writer, RemoteError::InfrastructureLost)
            }
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
        let guard = MountGuard {
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
        let status_success = status
            .as_ref()
            .is_some_and(|status| status.as_ref().is_ok_and(|status| status.success()));
        let success = close
            && !lost
            && termination.reason.load(Ordering::Acquire) == 0
            && status_success
            && cleanup.is_ok();
        if !success {
            eprintln!(
                "layerfs-daemon: mount failed close={close} lost={lost} reason={} status={status:?} cleanup={cleanup:?}",
                termination.reason.load(Ordering::Acquire),
            );
        }
        termination.finished.store(true, Ordering::Release);
        let _ = waiter.join();
        let _ = stdout.join();
        if let Some(stderr) = stderr {
            let _ = stderr.join();
        }
        if success {
            acknowledge_mount_close(&mut stream, &finished, lifecycle, guard);
            return;
        } else if close && !lost {
            send_error(&mut stream, RemoteError::InfrastructureLost);
        }
        finished.store(true, Ordering::Release);
        let _ = stream.shutdown(std::net::Shutdown::Both);
        let _ = lifecycle.join();
    }

    fn acknowledge_mount_close(
        stream: &mut ControlStream,
        finished: &AtomicBool,
        lifecycle: std::thread::JoinHandle<()>,
        guard: MountGuard,
    ) {
        // A caller may remount this workspace/root as soon as it receives the ACK.
        // Retire every old callback and its registry reservation before publishing it.
        finished.store(true, Ordering::Release);
        let _ = stream.shutdown(Shutdown::Read);
        let _ = lifecycle.join();
        drop(guard);
        let _ = protocol::write_frame(&mut *stream, Kind::WorkspaceClosed, &[]);
        let _ = stream.shutdown(Shutdown::Both);
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
            let _ = io::copy(&mut reader, &mut io::stderr().lock());
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn workspace_closed_waits_for_watcher_and_releases_mount_reservation() {
            let id = [7; 16];
            let root = b"/workspace/remount".to_vec();
            let shared = Arc::new(Shared {
                state: Mutex::new(State {
                    owner_live: true,
                    owner: Binding::Tcp,
                    owner_id: [1; 16],
                    active: BTreeMap::new(),
                    mounts: BTreeMap::from([(
                        id,
                        ActiveMount {
                            root: root.clone(),
                            alive: false,
                            ready: false,
                            pgid: 0,
                            termination: Arc::new(Termination::new()),
                        },
                    )]),
                    samples: BTreeMap::new(),
                    sample_starting: false,
                }),
                drained: Condvar::new(),
                limit: 1,
                capability: [0; 32],
                boot_id: [0; 16],
            });
            let guard = MountGuard {
                shared: shared.clone(),
                id,
            };
            let (server, mut client) = UnixStream::pair().unwrap();
            let mut server = ControlStream::Unix(server);
            let finished = Arc::new(AtomicBool::new(false));
            let (events, received) = std::sync::mpsc::channel();
            let watcher = watch_mount(
                server.try_clone().unwrap(),
                events,
                finished.clone(),
                shared.clone(),
                id,
            );
            protocol::write_frame(&mut client, Kind::Close, &[]).unwrap();
            assert!(matches!(
                received.recv_timeout(Duration::from_secs(2)),
                Ok(MountEvent::Close)
            ));
            let (draining, drained) = std::sync::mpsc::channel();
            let (release, released) = std::sync::mpsc::channel();
            let lifecycle = std::thread::spawn(move || {
                watcher.join().unwrap();
                draining.send(()).unwrap();
                // Hold the old watcher at its final return, exposing the old ACK race.
                let _ = released.recv();
            });
            let handler = std::thread::spawn(move || {
                acknowledge_mount_close(&mut server, &finished, lifecycle, guard);
            });
            drained.recv_timeout(Duration::from_secs(2)).unwrap();
            client.set_nonblocking(true).unwrap();
            let early_ack = protocol::read_frame(&mut client);
            let waiting =
                matches!(&early_ack, Err(error) if error.kind() == io::ErrorKind::WouldBlock);
            release.send(()).unwrap();
            assert!(
                waiting,
                "WorkspaceClosed arrived before watcher/registry cleanup"
            );
            client.set_nonblocking(false).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            assert_eq!(
                protocol::read_frame(&mut client).unwrap().kind,
                Kind::WorkspaceClosed
            );
            let state = shared.state.lock().unwrap();
            assert!(!state.mounts.contains_key(&id));
            assert!(!state.mounts.values().any(|mount| mount.root == root));
            drop(state);
            handler.join().unwrap();
        }

        fn point(current: u64, swap: u64, dirty: u64, writeback: u64) -> CgroupPoint {
            let mut stat = [0; CGROUP_STAT_FIELDS];
            stat[3] = dirty;
            stat[4] = writeback;
            CgroupPoint {
                current,
                lifetime_peak: current,
                swap,
                oom: 2,
                oom_kill: 3,
                stat,
            }
        }

        #[test]
        fn broader_window_retains_native_peak_and_reports_large_gaps() {
            let mut aggregate = SampleAggregate::new(point(10, 0, 0, 0), 100);
            aggregate.record(point(20, 0, 2, 3), 5_000_100);
            let mut endpoint = point(15, 0, 1, 1);
            endpoint.lifetime_peak = 25;
            aggregate.record(endpoint, 10_000_100);
            let report = aggregate.report(endpoint, 100, 10_000_100, 0, 2).unwrap();
            assert_eq!(report.started_ns, 100);
            assert_eq!(report.finished_ns, 10_000_100);
            assert_eq!(report.memory_lifetime_peak_final, 25);
            assert_eq!(report.memory_current_peak, 20);
            assert_eq!(report.maximum_sample_gap_ns, 5_000_000);
            assert_eq!(report.sample_count, 3);
        }

        #[test]
        fn merged_resource_samples_bind_boundaries_and_operands() {
            let baseline = point(10, 0, 2, 3);
            let mut aggregate = SampleAggregate::new(baseline, 100);
            aggregate.reset(baseline, 200);
            aggregate.record(point(999, 9, 99, 99), 199);
            assert_eq!(aggregate.records.len(), 1);
            aggregate.record(point(20, 1, 7, 5), 300);
            aggregate.record(point(15, 0, 4, 4), 400);
            let mut endpoint = point(15, 0, 4, 4);
            endpoint.oom = 4;
            endpoint.oom_kill = 4;
            let report = aggregate.report(endpoint, 250, 350, 0, 2).unwrap();
            assert_eq!(report.sample_count, 3);
            assert_eq!(report.memory_current_peak, 20);
            assert_eq!(report.memory_incremental_peak, 10);
            assert_eq!(report.swap_peak, 1);
            assert_eq!(report.dirty_writeback_baseline, 5);
            assert_eq!(report.dirty_writeback_peak, 12);
            assert_eq!(report.dirty_writeback_incremental_peak, 7);
            assert_eq!(report.oom_delta, 2);
            assert_eq!(report.oom_kill_delta, 1);
            assert_eq!(report.sample_interval_ns, 100);
            assert_eq!(report.maximum_sample_gap_ns, 100);
            assert!(report.t0_boundary_sampled);
            assert!(report.t3_boundary_sampled);
            assert!(report.interior_sampled);
            assert!(!report.sample_overflow);
            assert!(
                !aggregate
                    .report(endpoint, 50, 350, 0, 2)
                    .unwrap()
                    .t0_boundary_sampled
            );
            assert!(
                !aggregate
                    .report(endpoint, 250, 500, 0, 2)
                    .unwrap()
                    .t3_boundary_sampled
            );
            assert_eq!(report.memory_lifetime_peak_final, 15);
            let uncertain = aggregate.report(endpoint, 350, 370, 60, 2).unwrap();
            assert_eq!(uncertain.memory_current_peak, 20);
            assert!(!uncertain.interior_sampled);
            assert_eq!(uncertain.interior_sample_ns, 0);
            assert!(!uncertain.t3_boundary_sampled);
        }
    }
}

fn main() {
    if version_requested() {
        return;
    }
    #[cfg(target_os = "linux")]
    {
        if let Err(error) = linux::run() {
            eprintln!("layerfs-daemon: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("layerfs-daemon: Linux is required");
        std::process::exit(1);
    }
}

fn version_requested() -> bool {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() == 1 && arguments[0] == "--version" {
        println!("layerfs-daemon {}", env!("CARGO_PKG_VERSION"));
        true
    } else {
        false
    }
}

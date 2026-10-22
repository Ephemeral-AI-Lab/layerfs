//! One export on one loopback TCP listener: MOUNT and NFS on the same port.
//!
//! Each accepted connection is served by its own thread, one call at a time,
//! in arrival order. A call's reply is written first; only after that single
//! send attempt returned are the publication tickets of the mutations it
//! decided released (see [`crate::change::Tickets`]). Stop closes the listener and every connection, joins
//! their threads and only then gives back the synthetic kernel's holdings,
//! so no call is running when references are released.
use crate::{
    change::Tickets,
    kernel::{Kernel, Released},
    mount,
    nfs::{self, Policy},
    rpc::{self, Accept, Refused},
};
use layerfs_fuse::ports::MountServices;
use layerfs_overlay::KernelMount;
use std::{
    io,
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

/// What the export shows and allows.
#[derive(Clone, Debug)]
pub struct ExportConfig {
    /// The path MNT accepts, normally "/".
    pub path: String,
    /// The identity every inode is shown as.
    pub uid: u32,
    pub gid: u32,
    /// Changing procedures answer `NFS3ERR_ROFS`.
    pub read_only: bool,
    /// Refuse to create macOS AppleDouble `._*` names (see [`crate::names`]).
    pub refuse_apple_double: bool,
    /// Lookup references kept before the least recently used are collected
    /// (see [`crate::kernel`]); [`crate::kernel::LOOKUPS_HELD`] by default.
    pub lookups_held: usize,
}
impl ExportConfig {
    /// A writable export of "/" shown as `uid`:`gid`, refusing AppleDouble
    /// names, with the default reference bound.
    pub fn writable(uid: u32, gid: u32) -> Self {
        Self {
            path: "/".into(),
            uid,
            gid,
            read_only: false,
            refuse_apple_double: true,
            lookups_held: crate::kernel::LOOKUPS_HELD,
        }
    }
}

/// Calls served and time spent, per procedure.
pub struct Counters {
    nfs: [(AtomicU64, AtomicU64); 22],
    mount: [(AtomicU64, AtomicU64); 6],
}
impl Default for Counters {
    fn default() -> Self {
        Self {
            nfs: std::array::from_fn(|_| Default::default()),
            mount: std::array::from_fn(|_| Default::default()),
        }
    }
}
/// One procedure's totals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Count {
    pub procedure: &'static str,
    pub calls: u64,
    pub nanoseconds: u64,
}
const MOUNT_PROCEDURES: [&str; 6] = ["MNT_NULL", "MNT", "DUMP", "UMNT", "UMNTALL", "EXPORT"];
impl Counters {
    fn charge(&self, program: u32, procedure: u32, started: Instant) {
        let slot = match program {
            nfs::PROGRAM => self.nfs.get(procedure as usize),
            mount::PROGRAM => self.mount.get(procedure as usize),
            _ => None,
        };
        if let Some((calls, time)) = slot {
            calls.fetch_add(1, Ordering::Relaxed);
            let spent = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
            time.fetch_add(spent, Ordering::Relaxed);
        }
    }
    /// Procedures called at least once.
    pub fn snapshot(&self) -> Vec<Count> {
        let rows = |names: &[&'static str], slots: &[(AtomicU64, AtomicU64)]| {
            names
                .iter()
                .zip(slots)
                .map(|(name, (calls, time))| Count {
                    procedure: name,
                    calls: calls.load(Ordering::Relaxed),
                    nanoseconds: time.load(Ordering::Relaxed),
                })
                .filter(|count| count.calls > 0)
                .collect::<Vec<_>>()
        };
        let mut all = rows(&MOUNT_PROCEDURES, &self.mount);
        all.extend(rows(&nfs::PROCEDURES, &self.nfs));
        all
    }
}

struct Shared {
    kernel: Kernel,
    /// The WRITE/COMMIT verifier: fresh for every export.
    verifier: [u8; 8],
    config: ExportConfig,
    counters: Counters,
    stopping: AtomicBool,
    connections: Mutex<Vec<(TcpStream, JoinHandle<()>)>>,
}
/// A running export.
pub struct Server {
    address: SocketAddr,
    shared: Arc<Shared>,
    accept: Option<JoinHandle<()>>,
}
impl Server {
    /// Serves the engine mount `mount` over `services` on `listener`.
    pub fn start(
        listener: TcpListener,
        services: Arc<dyn MountServices>,
        mount: KernelMount,
        config: ExportConfig,
    ) -> io::Result<Self> {
        let address = listener.local_addr()?;
        let shared = Arc::new(Shared {
            kernel: Kernel::new(services, mount, config.uid, config.gid)
                .refusing_apple_double(config.refuse_apple_double)
                .holding(config.lookups_held),
            verifier: fresh_verifier(),
            config,
            counters: Counters::default(),
            stopping: AtomicBool::new(false),
            connections: Mutex::default(),
        });
        let accepting = shared.clone();
        let accept = thread::Builder::new()
            .name("layerfs-nfs-accept".into())
            .spawn(move || accept_loop(&listener, &accepting))?;
        Ok(Self {
            address,
            shared,
            accept: Some(accept),
        })
    }
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub fn counters(&self) -> Vec<Count> {
        self.shared.counters.snapshot()
    }
    /// The synthetic kernel's current holdings.
    pub fn held(&self) -> Released {
        self.shared.kernel.held()
    }
    /// Closes the implicit file descriptors no silly name needs, for example
    /// before a Commit; the next READ or WRITE reopens one. Returns how many.
    pub fn close_files(&self) -> usize {
        self.shared.kernel.close_files()
    }
    /// After a Commit installed a root other than the one the client saw:
    /// closes the implicit file and enumeration descriptors, so the next
    /// READ reopens the installed file and a continuing READDIR restarts.
    /// Attributes the client cached are revalidated on its own schedule
    /// (`actimeo`, close-to-open). Returns (files, directories) closed.
    pub fn installed(&self) -> (usize, usize) {
        (
            self.shared.kernel.close_files(),
            self.shared.kernel.close_directories(),
        )
    }
    /// After an install: marks the stale `serials` the client may cache and
    /// returns their paths below the mount point, for the mode-preserving
    /// SETATTR that makes the client drop its cache (see [`crate::refresh`]).
    pub fn refresh(&self, serials: impl IntoIterator<Item = u64>) -> Vec<std::path::PathBuf> {
        use std::os::unix::ffi::OsStrExt;
        (self.shared.kernel.refresh(serials).into_iter())
            .map(|names| {
                (names.iter())
                    .map(|name| std::ffi::OsStr::from_bytes(name))
                    .collect()
            })
            .collect()
    }
    /// The write verifier this export answers WRITE and COMMIT with.
    pub fn verifier(&self) -> [u8; 8] {
        self.shared.verifier
    }
    /// Stops accepting, closes every connection, joins every thread, then
    /// releases the synthetic kernel's descriptors and references. The engine
    /// mount itself stays with the caller, who revokes it afterwards.
    pub fn stop(mut self) -> Released {
        self.shutdown()
    }
    fn shutdown(&mut self) -> Released {
        let Some(accept) = self.accept.take() else {
            return Released::default();
        };
        self.shared.stopping.store(true, Ordering::Release);
        // Wake the blocking accept; it sees the flag and returns.
        drop(TcpStream::connect(self.address));
        drop(accept.join());
        let connections = std::mem::take(
            &mut *self
                .shared
                .connections
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()),
        );
        for (stream, _) in &connections {
            drop(stream.shutdown(Shutdown::Both));
        }
        for (_, thread) in connections {
            drop(thread.join());
        }
        self.shared.kernel.release()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>) {
    for stream in listener.incoming() {
        if shared.stopping.load(Ordering::Acquire) {
            return;
        }
        let Ok(stream) = stream else { continue };
        drop(stream.set_nodelay(true));
        let Ok(control) = stream.try_clone() else {
            continue;
        };
        let serving = shared.clone();
        let spawned = thread::Builder::new()
            .name("layerfs-nfs-connection".into())
            .spawn(move || connection(stream, &serving));
        if let Ok(thread) = spawned {
            let mut connections = shared
                .connections
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            // A closed connection's thread has ended; forget it.
            connections.retain(|(_, thread)| !thread.is_finished());
            connections.push((control, thread));
        }
    }
}
fn connection(mut stream: TcpStream, shared: &Shared) {
    while let Ok(Some(record)) = rpc::read_record(&mut stream) {
        if shared.stopping.load(Ordering::Acquire) {
            return;
        }
        let mut tickets = Tickets::default();
        let Some(reply) = answer(shared, &record, &mut tickets) else {
            tickets.replied(&shared.kernel);
            continue;
        };
        // The single reply attempt, then the release of every ticket it
        // owed, whether or not the reply reached the client.
        let sent = rpc::write_record(&mut stream, &reply);
        tickets.replied(&shared.kernel);
        if sent.is_err() {
            return;
        }
    }
}
/// A verifier no earlier export of this process or machine is likely to
/// have used: the clock and the process, under a per-process random key.
fn fresh_verifier() -> [u8; 8] {
    use std::hash::BuildHasher;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    std::collections::hash_map::RandomState::new()
        .hash_one((now, std::process::id()))
        .to_be_bytes()
}
/// The reply to one record, or none when the record is not a call.
/// Mutations the call decided go to `tickets`.
fn answer(shared: &Shared, record: &[u8], tickets: &mut Tickets) -> Option<Vec<u8>> {
    let started = Instant::now();
    let (call, mut arguments) = match rpc::parse_call(record) {
        Ok(parsed) => parsed,
        Err(Refused::Reply(reply)) => return Some(reply),
        Err(Refused::Undecodable) => return None,
    };
    let mut out = rpc::accepted(call.xid, Accept::Success);
    let result = match (call.program, call.version) {
        (nfs::PROGRAM, nfs::VERSION) => nfs::call(
            &shared.kernel,
            Policy {
                read_only: shared.config.read_only,
            },
            call.procedure,
            &call.credentials,
            &mut arguments,
            &mut out,
            tickets,
            shared.verifier,
        ),
        (mount::PROGRAM, mount::VERSION) => mount::call(
            &shared.kernel,
            &shared.config.path,
            call.procedure,
            &mut arguments,
            &mut out,
        ),
        (nfs::PROGRAM, _) => Err(Accept::ProgramMismatch { low: 3, high: 3 }),
        (mount::PROGRAM, _) => Err(Accept::ProgramMismatch { low: 3, high: 3 }),
        _ => Err(Accept::ProgramUnavailable),
    };
    let reply = match result {
        Ok(()) => out.into_bytes(),
        Err(status) => rpc::accepted(call.xid, status).into_bytes(),
    };
    shared
        .counters
        .charge(call.program, call.procedure, started);
    Some(reply)
}

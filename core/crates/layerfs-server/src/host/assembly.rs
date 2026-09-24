//! One concrete host authority: Store, history, Service, listener and owner.
//!
//! `Server::create` prepares a fresh Store for a first Init; `Server::open`
//! opens a prepared clone. Both build the same authority: the authorized
//! `Service`, a bounded native acceptor, the peer identities an SDK or daemon
//! uses, and the sandbox owner that launches and reaps sandbox containers.
use crate::host::{acceptor::Acceptor, store};
use crate::{Service, StoreAccess};
use layerfs_bridge::{
    adapters::native::connection::{Peer, VerifiedPeer},
    contract::{Code, Failure},
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_sandbox::{OwnerConfig, SandboxOwner};
use layerfs_storage::Store;
use layerfs_telemetry::{runtime::Runtime, timer::Timing};
use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

/// One Store id used by every operation this Server answers.
pub const PRIMARY_STORE: u32 = 1;
const SELECTOR: u32 = 1;
const ACCEPT_SETTLE_MS: u64 = 20;

/// Everything a composed authority needs; no field has a default.
#[derive(Clone)]
pub struct ServerConfig {
    pub store_path: PathBuf,
    pub history_path: PathBuf,
    /// Stable authority binding key of the history catalog.
    pub binding_key: Vec<u8>,
    pub incarnation: u64,
    pub cursor_key: [u8; 32],
    /// Sandbox container endpoint as seen from the container.
    pub service_host: String,
    /// Shared host-process recorder and resource monitor.
    pub runtime: Runtime,
    /// One shared diagnostic run identity; absent keeps daemon telemetry off.
    pub telemetry_run: Option<u128>,
}

/// The single composed authority used by the SDK and the operator process.
pub struct Server {
    service: Arc<Service>,
    runtime: Runtime,
    store: Store,
    host_private: [u8; 32],
    daemon_private: [u8; 32],
    service_private: [u8; 32],
    control_private: [u8; 32],
    config: ServerConfig,
    listener: Mutex<Listening>,
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
struct Listening {
    endpoint: Option<SocketAddr>,
    acceptor: Option<JoinHandle<()>>,
}

impl Server {
    /// Prepare a fresh Store and history catalog for a first Init.
    pub fn create(config: ServerConfig) -> Result<Self, Failure> {
        let store = Timing::disabled("create", |scope| {
            store::create(&config.store_path, scope.child("store"))
        })
        .0?;
        Self::assemble(config, store)
    }

    /// Open a prepared Store clone and its history catalog.
    pub fn open(config: ServerConfig) -> Result<Self, Failure> {
        let store = store::open(&config.store_path)?;
        Self::assemble(config, store)
    }

    fn assemble(config: ServerConfig, store: Store) -> Result<Self, Failure> {
        if config.binding_key.is_empty()
            || config.binding_key.len() > 128
            || config.incarnation == 0
        {
            return Err(Code::InvalidInput.into());
        }
        let history = store::history(
            &config.history_path,
            &HistoryCatalogConfig {
                binding_key: config.binding_key.clone(),
                incarnation: config.incarnation,
                cursor_key: config.cursor_key,
            },
        )?;
        let host_private = random_key()?;
        let daemon_private = random_key()?;
        let service_private = random_key()?;
        let control_private = random_key()?;
        let host_public = VerifiedPeer::from_private(&host_private)?
            .public_key()
            .to_owned();
        let daemon_public = VerifiedPeer::from_private(&daemon_private)?
            .public_key()
            .to_owned();
        let service = Service::new(
            vec![StoreAccess {
                id: PRIMARY_STORE,
                store: store.clone(),
                history: Some(Arc::clone(&history)),
                grants: store::grants(&[host_public, daemon_public])?,
            }],
            config.runtime.recorder(),
        )?;
        Ok(Self {
            service: Arc::new(service),
            runtime: config.runtime.clone(),
            store,
            host_private,
            daemon_private,
            service_private,
            control_private,
            config,
            listener: Mutex::new(Listening::default()),
            stop: Arc::new(AtomicBool::new(false)),
        })
    }

    /// The authorized operation handler behind every SDK API object.
    pub fn service(&self) -> &Service {
        &self.service
    }

    /// The Store id this Server answers for.
    pub fn store(&self) -> u32 {
        PRIMARY_STORE
    }

    /// The shared host-process recorder and resource monitor.
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// The agent-facing host peer identity an SDK caller is authorized as.
    pub fn peer(&self) -> Result<VerifiedPeer, Failure> {
        Ok(VerifiedPeer::from_private(&self.host_private)?)
    }

    /// Bind the loopback endpoint sandboxes call back on, and serve it.
    pub fn listen(&self) -> Result<SocketAddr, Failure> {
        let mut listener = self.listener.lock().map_err(|_| Code::Io)?;
        if let Some(endpoint) = listener.endpoint {
            return Ok(endpoint);
        }
        let daemon_public = VerifiedPeer::from_private(&self.daemon_private)?
            .public_key()
            .to_owned();
        let acceptor = Acceptor::bind(
            SocketAddr::from(([127, 0, 0, 1], 0)),
            store::capacity(&self.store)?,
            self.service_private,
            vec![Peer {
                selector: SELECTOR,
                public: daemon_public,
                expires_unix: u64::MAX,
            }],
            Arc::clone(&self.service),
            self.runtime.clone(),
        )?;
        let endpoint = acceptor.endpoint()?;
        let stop = Arc::clone(&self.stop);
        let worker = thread::Builder::new()
            .name("layerfs-server-accept".into())
            .stack_size(2 * 1024 * 1024)
            .spawn(move || {
                let _ = acceptor.serve(crate::host::acceptor::Stop::Flag(&stop));
            })?;
        listener.endpoint = Some(endpoint);
        listener.acceptor = Some(worker);
        Ok(endpoint)
    }

    /// The bound endpoint, when the listener is running.
    pub fn endpoint(&self) -> Result<SocketAddr, Failure> {
        self.listener
            .lock()
            .map_err(|_| Code::Io)?
            .endpoint
            .ok_or_else(|| Failure::from(Code::Busy))
    }

    /// Assemble the sandbox owner bound to this Server's authority.
    pub fn owner(&self) -> Result<SandboxOwner, Failure> {
        let endpoint = self.endpoint()?;
        let service_public = VerifiedPeer::from_private(&self.service_private)?
            .public_key()
            .to_owned();
        SandboxOwner::new(OwnerConfig {
            service_endpoint: format!("{}:{}", self.config.service_host, endpoint.port()),
            service_selector: SELECTOR,
            service_private: self.daemon_private,
            service_public,
            control_private: self.control_private,
            store: PRIMARY_STORE,
            telemetry_run: self.config.telemetry_run,
            telemetry: self.runtime.clone(),
        })
    }

    /// Stop admission and wait for the acceptor to release its owners.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        let worker = self
            .listener
            .lock()
            .map(|mut listener| listener.acceptor.take())
            .ok();
        if let Some(worker) = worker.flatten() {
            thread::sleep(std::time::Duration::from_millis(ACCEPT_SETTLE_MS));
            let _ = worker.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn random_key() -> Result<[u8; 32], Failure> {
    use std::io::Read;
    let mut value = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut value)?;
    if value.iter().all(|byte| *byte == 0) {
        return Err(Code::Io.into());
    }
    Ok(value)
}

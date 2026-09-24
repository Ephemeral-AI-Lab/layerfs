use crate::{docker, readiness};
use layerfs_api_core::{SandboxId, SandboxInfo, SandboxStatus, WorkspaceId};
use layerfs_bridge::{
    adapters::native::client::Client,
    contract::{Code, Failure, Operation, Response},
};
use layerfs_telemetry::runtime::Runtime;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    net::{SocketAddr, TcpListener},
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct OwnerConfig {
    pub service_endpoint: String,
    pub service_selector: u32,
    pub service_private: [u8; 32],
    pub service_public: [u8; 32],
    pub control_private: [u8; 32],
    pub store: u32,
    /// One shared diagnostic run identity; absent keeps daemon telemetry off.
    pub telemetry_run: Option<u128>,
    /// Host process recorder shared with the application Service and SDK.
    pub telemetry: Runtime,
}

/// The validated routing identity one control operation is addressed to.
///
/// `instance` is the daemon instance the sandbox was admitted with and is
/// revalidated before every operation. `workspace` and `incarnation` are set
/// for a bound Workspace and are the selector the daemon checks.
#[derive(Clone)]
pub struct ControlRoute {
    pub sandbox: SandboxId,
    pub instance: [u8; 32],
    pub workspace: Option<(WorkspaceId, [u8; 32])>,
}
impl ControlRoute {
    /// The Workspace selector and incarnation this route is bound to.
    ///
    /// A route resolved for a sandbox alone has no bound Workspace; an
    /// operation that needs one refuses instead of inventing a selector.
    pub fn selector(&self) -> Result<(Vec<u8>, [u8; 32]), Failure> {
        let (id, incarnation) = self
            .workspace
            .as_ref()
            .ok_or_else(|| Failure::from(Code::InvalidInput))?;
        Ok((id.0.as_bytes().to_vec(), *incarnation))
    }
}

#[derive(Clone)]
pub struct Binding {
    pub sandbox: SandboxId,
    pub instance: [u8; 32],
}

#[derive(Clone)]
pub struct WorkspaceBinding {
    pub id: WorkspaceId,
    pub incarnation: [u8; 32],
    pub sandbox: SandboxId,
    pub instance: [u8; 32],
}

#[derive(Debug)]
pub enum RouteError {
    Failure(Failure),
    Stale,
    NotFound,
}
impl From<Failure> for RouteError {
    fn from(value: Failure) -> Self {
        Self::Failure(value)
    }
}

#[derive(Debug)]
pub struct CreateError {
    pub sandbox: Option<SandboxId>,
    pub cause: Failure,
    /// Container or daemon custody may exist; list and inspect this ID.
    pub retained: bool,
}

#[derive(Clone)]
struct Record {
    name: String,
    container: String,
    daemon_public: [u8; 32],
    endpoint: SocketAddr,
    instance: Option<[u8; 32]>,
}

#[derive(Default)]
struct Registry {
    sandboxes: BTreeMap<SandboxId, Record>,
    workspaces: BTreeMap<String, WorkspaceBinding>,
}

pub struct SandboxOwner {
    config: OwnerConfig,
    registry: Mutex<Registry>,
    sessions: crate::session::Sessions,
}
impl SandboxOwner {
    pub fn new(config: OwnerConfig) -> Result<Self, Failure> {
        if config.service_endpoint.is_empty()
            || config.store == 0
            || config.telemetry_run == Some(0)
        {
            return Err(Code::InvalidInput.into());
        }
        Ok(Self {
            config,
            registry: Mutex::new(Registry::default()),
            sessions: crate::session::Sessions::default(),
        })
    }

    pub fn create(&self, image_id: &str, name: &str) -> Result<SandboxId, CreateError> {
        let id = SandboxId(random::<16>().map_err(|cause| CreateError {
            sandbox: None,
            cause,
            retained: false,
        })?);
        if !docker::valid_image(image_id)
            || name.is_empty()
            || name.len() > 63
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(CreateError {
                sandbox: None,
                cause: Code::InvalidInput.into(),
                retained: false,
            });
        }
        let daemon_public =
            layerfs_bridge::adapters::native::connection::VerifiedPeer::from_private(
                &self.config.service_private,
            )
            .map_err(|cause| CreateError {
                sandbox: None,
                cause,
                retained: false,
            })?
            .public_key()
            .to_owned();
        let container = format!("layerfs-{id}");
        let mut registry = self.registry.lock().map_err(|_| CreateError {
            sandbox: None,
            cause: Code::Io.into(),
            retained: false,
        })?;
        let reservation =
            TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map_err(|cause| CreateError {
                sandbox: None,
                cause: cause.into(),
                retained: false,
            })?;
        let endpoint = reservation.local_addr().map_err(|cause| CreateError {
            sandbox: None,
            cause: cause.into(),
            retained: false,
        })?;
        if registry.sandboxes.contains_key(&id)
            || registry
                .sandboxes
                .values()
                .any(|record| record.endpoint == endpoint)
        {
            return Err(CreateError {
                sandbox: None,
                cause: Code::Busy.into(),
                retained: false,
            });
        }
        registry.sandboxes.insert(
            id,
            Record {
                name: name.into(),
                container: container.clone(),
                daemon_public,
                endpoint,
                instance: None,
            },
        );
        drop(registry);
        drop(reservation);
        if let Err(cause) = self.observe(2003, "owner.docker_launch", || {
            docker::launch(
                &self.config,
                image_id,
                name,
                id,
                &container,
                endpoint.port(),
            )
        }) {
            return Err(CreateError {
                sandbox: Some(id),
                cause,
                retained: true,
            });
        }
        let published = self
            .observe(2004, "owner.docker_port", || docker::port(&container))
            .map_err(|cause| CreateError {
                sandbox: Some(id),
                cause,
                retained: true,
            })?;
        if published != endpoint {
            return Err(CreateError {
                sandbox: Some(id),
                cause: Code::Integrity.into(),
                retained: true,
            });
        }
        let hello = self
            .observe(2005, "owner.daemon_ready", || {
                readiness::wait(endpoint, &self.config.control_private, &daemon_public, id)
            })
            .map_err(|cause| CreateError {
                sandbox: Some(id),
                cause,
                retained: true,
            })?;
        self.observe(2006, "owner.shell_ready", || {
            docker::shell_ready(&container)
        })
        .map_err(|cause| CreateError {
            sandbox: Some(id),
            cause,
            retained: true,
        })?;
        let mut registry = self.registry.lock().map_err(|_| CreateError {
            sandbox: Some(id),
            cause: Code::Io.into(),
            retained: true,
        })?;
        let record = registry.sandboxes.get_mut(&id).ok_or_else(|| CreateError {
            sandbox: Some(id),
            cause: Code::Io.into(),
            retained: true,
        })?;
        record.instance = Some(hello.instance);
        Ok(id)
    }

    pub fn list(&self) -> Result<Vec<SandboxInfo>, Failure> {
        let records: Vec<_> = self
            .registry
            .lock()
            .map_err(|_| Code::Io)?
            .sandboxes
            .iter()
            .map(|(id, record)| (*id, record.clone()))
            .collect();
        records
            .iter()
            .map(|(id, record)| {
                let status = if !docker::running(&record.container)? {
                    SandboxStatus::Stopped
                } else if let Some(instance) = record.instance {
                    match readiness::hello(
                        record.endpoint,
                        &self.config.control_private,
                        &record.daemon_public,
                        *id,
                    ) {
                        Ok(hello) if hello.instance == instance => SandboxStatus::Ready,
                        Ok(_) => SandboxStatus::Stale,
                        Err(_) => SandboxStatus::Unconfirmed,
                    }
                } else {
                    SandboxStatus::Unconfirmed
                };
                Ok(SandboxInfo {
                    id: *id,
                    name: record.name.clone(),
                    status,
                })
            })
            .collect()
    }

    pub fn lookup(&self, id: SandboxId) -> Result<(Binding, Client), RouteError> {
        let (binding, client) = self.checked_lookup(id)?;
        Ok((binding, client))
    }

    /// Resolves one sandbox to its validated binding without opening a session.
    ///
    /// The binding carries the daemon instance the sandbox was admitted with,
    /// which every later operation is validated against.
    pub fn lookup_route(&self, id: SandboxId) -> Result<(Binding, SocketAddr), RouteError> {
        let record = self
            .registry
            .lock()
            .map_err(|_| RouteError::Failure(Code::Io.into()))?
            .sandboxes
            .get(&id)
            .cloned()
            .ok_or(RouteError::NotFound)?;
        // A sandbox whose daemon instance was never confirmed has no validated
        // route; the caller must complete a checked lookup first.
        let instance = record
            .instance
            .ok_or(RouteError::Failure(Code::Busy.into()))?;
        Ok((
            Binding {
                sandbox: id,
                instance,
            },
            record.endpoint,
        ))
    }

    /// Opens one checked control session and returns the validated binding.
    pub fn checked_lookup(&self, id: SandboxId) -> Result<(Binding, Client), RouteError> {
        let record = self
            .registry
            .lock()
            .map_err(|_| RouteError::Failure(Code::Io.into()))?
            .sandboxes
            .get(&id)
            .cloned()
            .ok_or(RouteError::NotFound)?;
        let (hello, client) = self.observe(2002, "owner.hello", || {
            readiness::hello_session(
                record.endpoint,
                &self.config.control_private,
                &record.daemon_public,
                id,
            )
        })?;
        if record
            .instance
            .is_some_and(|instance| hello.instance != instance)
        {
            return Err(RouteError::Stale);
        }
        if record.instance.is_none() {
            docker::shell_ready(&record.container)?;
            let mut registry = self
                .registry
                .lock()
                .map_err(|_| RouteError::Failure(Code::Io.into()))?;
            let current = registry
                .sandboxes
                .get_mut(&id)
                .ok_or(RouteError::NotFound)?;
            if current
                .instance
                .is_some_and(|instance| instance != hello.instance)
            {
                return Err(RouteError::Stale);
            }
            current.instance = Some(hello.instance);
        }
        let instance = hello.instance;
        Ok((
            Binding {
                sandbox: id,
                instance,
            },
            client,
        ))
    }

    /// Runs one Workspace operation on a bounded active control session.
    ///
    /// `route` keys the retention: a session is reusable only for the same
    /// sandbox and daemon instance, so a restarted daemon cannot answer an
    /// operation addressed to its predecessor. The operation is one attempt:
    /// any failure discards the session, and a successful one retains it for
    /// the next rapid call. Nothing is ever resent.
    pub fn control_call(
        &self,
        route: ControlRoute,
        deadline_ms: u32,
        operation: impl FnOnce() -> Operation,
    ) -> Result<Response, RouteError> {
        let deadline = Instant::now() + Duration::from_millis(u64::from(deadline_ms));
        let mut lease = self
            .sessions
            .lease(route.sandbox, route.instance, deadline, || {
                let record = self
                    .registry
                    .lock()
                    .map_err(|_| Failure::from(Code::Io))?
                    .sandboxes
                    .get(&route.sandbox)
                    .cloned()
                    .ok_or_else(|| Failure::from(Code::NotFound))?;
                if record
                    .instance
                    .is_some_and(|instance| instance != route.instance)
                {
                    return Err(Code::Denied.into());
                }
                self.observe(2002, "owner.hello", || {
                    readiness::hello_session(
                        record.endpoint,
                        &self.config.control_private,
                        &record.daemon_public,
                        route.sandbox,
                    )
                })
            })
            .map_err(RouteError::Failure)?;
        let result = crate::session::call(&mut lease.client, lease.id, deadline_ms, operation());
        match result {
            Ok(response) => {
                self.sessions
                    .retain(route.sandbox, route.instance, lease.id + 1, lease.client);
                Ok(response)
            }
            Err(failure) => {
                self.sessions.discard();
                // A refusal is only a stale route when the live daemon no
                // longer reports the instance this route was bound to. An
                // ordinary operation failure keeps its own cause.
                if self.daemon_moved(&route) {
                    return Err(RouteError::Stale);
                }
                Err(RouteError::Failure(failure))
            }
        }
    }

    /// True when the sandbox's live daemon reports a different instance than
    /// `route` was bound to. An unreachable or unreadable daemon is not proof
    /// of a move, so it reports false and the caller keeps its own failure.
    fn daemon_moved(&self, route: &ControlRoute) -> bool {
        let Ok(record) = self
            .registry
            .lock()
            .map(|registry| registry.sandboxes.get(&route.sandbox).cloned())
        else {
            return false;
        };
        let Some(record) = record else {
            return false;
        };
        readiness::hello(
            record.endpoint,
            &self.config.control_private,
            &record.daemon_public,
            route.sandbox,
        )
        .is_ok_and(|hello| hello.instance != route.instance)
    }

    pub fn bind_workspace(
        &self,
        binding: &Binding,
        id: WorkspaceId,
        incarnation: [u8; 32],
    ) -> Result<(), RouteError> {
        let mut registry = self
            .registry
            .lock()
            .map_err(|_| RouteError::Failure(Code::Io.into()))?;
        if registry.workspaces.contains_key(&id.0) {
            return Err(RouteError::Failure(Code::Busy.into()));
        }
        registry.workspaces.insert(
            id.0.clone(),
            WorkspaceBinding {
                id,
                incarnation,
                sandbox: binding.sandbox,
                instance: binding.instance,
            },
        );
        Ok(())
    }

    /// Resolves one bound Workspace to its validated control route.
    ///
    /// The route carries the daemon instance recorded when the Workspace was
    /// bound. A restarted daemon reports a different instance, so its
    /// predecessor's route is rejected as stale before any operation is sent.
    pub fn workspace(&self, id: &WorkspaceId) -> Result<ControlRoute, RouteError> {
        let (sandbox, instance, incarnation) = {
            let registry = self
                .registry
                .lock()
                .map_err(|_| RouteError::Failure(Code::Io.into()))?;
            let route = registry
                .workspaces
                .get(&id.0)
                .cloned()
                .ok_or(RouteError::NotFound)?;
            (route.sandbox, route.instance, route.incarnation)
        };
        // The registry's recorded instance is the identity this Workspace was
        // bound to. The daemon is authoritative about its own instance and is
        // revalidated by the session layer before any operation runs, so a
        // restarted container cannot be mistaken for its predecessor here.
        let record = self
            .registry
            .lock()
            .map_err(|_| RouteError::Failure(Code::Io.into()))?
            .sandboxes
            .get(&sandbox)
            .cloned()
            .ok_or(RouteError::NotFound)?;
        if record.instance.is_none_or(|live| live != instance) {
            return Err(RouteError::Stale);
        }
        Ok(ControlRoute {
            sandbox,
            instance,
            workspace: Some((id.clone(), incarnation)),
        })
    }

    fn observe<T>(
        &self,
        key: u64,
        label: &'static str,
        call: impl FnOnce() -> Result<T, Failure>,
    ) -> Result<T, Failure> {
        let (result, diagnostic) = self.config.telemetry.recorder().run(key, label, |_| call());
        self.config.telemetry.publish(diagnostic);
        result
    }
}

pub fn random<const N: usize>() -> Result<[u8; N], Failure> {
    let mut value = [0; N];
    File::open("/dev/urandom")?.read_exact(&mut value)?;
    if value.iter().all(|byte| *byte == 0) {
        return Err(Code::Io.into());
    }
    Ok(value)
}

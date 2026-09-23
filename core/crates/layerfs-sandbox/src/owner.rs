use crate::{docker, readiness};
use layerfs_api_core::{SandboxId, SandboxInfo, SandboxStatus, WorkspaceId};
use layerfs_bridge::contract::{Code, Failure};
use std::{collections::BTreeMap, fs::File, io::Read, net::SocketAddr, sync::Mutex};

#[derive(Clone)]
pub struct OwnerConfig {
    pub service_endpoint: String,
    pub service_selector: u32,
    pub service_private: [u8; 32],
    pub service_public: [u8; 32],
    pub control_private: [u8; 32],
    pub store: u32,
}

#[derive(Clone)]
pub struct Binding {
    pub sandbox: SandboxId,
    pub instance: [u8; 32],
    pub endpoint: SocketAddr,
    pub selector: u32,
    pub private: [u8; 32],
    pub server: [u8; 32],
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
}
impl SandboxOwner {
    pub fn new(config: OwnerConfig) -> Result<Self, Failure> {
        if config.service_endpoint.is_empty() || config.store == 0 {
            return Err(Code::InvalidInput.into());
        }
        Ok(Self {
            config,
            registry: Mutex::new(Registry::default()),
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
        let record = Record {
            name: name.into(),
            container: container.clone(),
            daemon_public,
            instance: None,
        };
        let mut registry = self.registry.lock().map_err(|_| CreateError {
            sandbox: None,
            cause: Code::Io.into(),
            retained: false,
        })?;
        if registry.sandboxes.contains_key(&id) {
            return Err(CreateError {
                sandbox: None,
                cause: Code::Busy.into(),
                retained: false,
            });
        }
        registry.sandboxes.insert(id, record);
        drop(registry);
        if let Err(cause) = docker::launch(&self.config, image_id, name, id, &container) {
            return Err(CreateError {
                sandbox: Some(id),
                cause,
                retained: true,
            });
        }
        let endpoint = docker::port(&container).map_err(|cause| CreateError {
            sandbox: Some(id),
            cause,
            retained: true,
        })?;
        let hello = readiness::wait(endpoint, &self.config.control_private, &daemon_public, id)
            .map_err(|cause| CreateError {
                sandbox: Some(id),
                cause,
                retained: true,
            })?;
        docker::shell_ready(&container).map_err(|cause| CreateError {
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
                    match docker::port(&record.container).and_then(|endpoint| {
                        readiness::hello(
                            endpoint,
                            &self.config.control_private,
                            &record.daemon_public,
                            *id,
                        )
                    }) {
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

    pub fn lookup(&self, id: SandboxId) -> Result<Binding, RouteError> {
        let record = self
            .registry
            .lock()
            .map_err(|_| RouteError::Failure(Code::Io.into()))?
            .sandboxes
            .get(&id)
            .cloned()
            .ok_or(RouteError::NotFound)?;
        let endpoint = docker::port(&record.container)?;
        let hello = readiness::hello(
            endpoint,
            &self.config.control_private,
            &record.daemon_public,
            id,
        )?;
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
        Ok(Binding {
            sandbox: id,
            instance,
            endpoint,
            selector: 1,
            private: self.config.control_private,
            server: record.daemon_public,
        })
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

    pub fn workspace(&self, id: &WorkspaceId) -> Result<(Binding, WorkspaceBinding), RouteError> {
        let route = self
            .registry
            .lock()
            .map_err(|_| RouteError::Failure(Code::Io.into()))?
            .workspaces
            .get(&id.0)
            .cloned()
            .ok_or(RouteError::NotFound)?;
        let binding = self.lookup(route.sandbox)?;
        if binding.instance != route.instance {
            return Err(RouteError::Stale);
        }
        Ok((binding, route))
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

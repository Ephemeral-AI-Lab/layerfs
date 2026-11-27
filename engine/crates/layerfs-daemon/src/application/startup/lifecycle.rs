//! One startup lifecycle owner, Store transfer and bounded connection custody.
use super::super::connection::retained::{RetainedFailures, RetainedSnapshot};
use super::super::connection::slots::{SlotClass, SlotCounts, Slots};
use super::super::ApplicationError;
use crate::{
    bootstrap::{open_store_observed, OpenedStore},
    install::outcome::{InstallFailure, InstalledStore},
    workspace::Service,
    Owner, OwnerConfig,
};
use layerfs_overlay::ProfileConfig;
use layerfs_persistence::{
    PersistenceConfig, PostgresSelection, SqlitePersistenceProfile, SynchronousCommit,
};
use layerfs_protocol::daemon::{DaemonPhase, DaemonStatus, HelloRequest};
use layerfs_protocol::{duplex as transport, provision::daemon_setup::DaemonSetup};
use layerfs_storage::ReservationBlocks;
use std::{
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::Instant,
};

pub(crate) struct Startup {
    pub phase: DaemonPhase,
    pub service: Option<Arc<Service>>,
    pub installed: Option<InstalledStore>,
    pub existing: Option<OpenedStore>,
    pub install_failure: Option<Box<InstallFailure>>,
}
pub(crate) struct State {
    pub startup: Startup,
    pub slots: Slots,
    pub retained: RetainedFailures,
    /// One detached or joined worker per slot. A reclaimed or expired slot
    /// detaches its stuck worker (its handle is dropped here) so the accept
    /// loop never joins a worker that a Store/fence wait still holds.
    pub workers: Vec<Option<JoinHandle<()>>>,
}
/// One real daemon application; Store/Overlay are initialized once and reused.
pub struct Application {
    pub(crate) setup: DaemonSetup,
    pub(crate) instance: [u8; 32],
    pub(crate) owner: Owner,
    /// Fixed shared kernel workers, entered before control readiness.
    pub(super) kernel: Option<Arc<crate::workspace::KernelServing>>,
    /// NFS serving of the native route, when selected.
    pub(super) export: Option<Arc<crate::workspace::ExportServing>>,
    pub(crate) state: Mutex<State>,
    pub(crate) changed: Condvar,
    /// Per-slot socket fences, keyed by the admission generation that owns
    /// them: the accept loop closes one to reclaim an idle slot or to expire
    /// an in-flight request (#547 slices 4-5). A generation key lets a stale
    /// worker's late `clear_fence` skip a newer worker's fence.
    pub(crate) fences: Mutex<Vec<Option<(u64, transport::CloseHandle)>>>,
}

impl Drop for Application {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(service) = state.startup.service.take() {
                service.close_bound();
            }
        }
    }
}
impl Application {
    pub(super) fn start(setup: DaemonSetup) -> Result<Arc<Self>, ApplicationError> {
        let l = setup.limits;
        let usize_value = |n| {
            usize::try_from(n).map_err(|_| {
                ApplicationError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "resource width",
                ))
            })
        };
        let started = Owner::start_observed(
            std::path::Path::new(&setup.overlay),
            ProfileConfig {
                pager_kib: l.pager_kib,
                max_pages: None,
            },
            OwnerConfig {
                namespaces: l.namespaces as usize,
            },
        );
        if started.result.is_err() {
            return Err(ApplicationError::Owner(Box::new(started)));
        }
        let owner = started.result.expect("original successful Overlay start");
        let instance = match transport::generate_keypair() {
            Ok(keys) => keys.public,
            Err(cause) => {
                return Err(ApplicationError::AfterOverlay {
                    owner: Box::new(owner),
                    cause: Box::new(ApplicationError::Channel(cause)),
                })
            }
        };
        let cache_bytes = match usize_value(l.cache_bytes) {
            Ok(value) => value,
            Err(cause) => {
                return Err(ApplicationError::AfterOverlay {
                    owner: Box::new(owner),
                    cause: Box::new(cause),
                })
            }
        };
        let selected = match &setup.store {
            layerfs_protocol::DaemonStore::Sqlite {
                install: Some(m),
                locator,
            } => Some((
                layerfs_persistence::StoreSelection::Sqlite(
                    PersistenceConfig::sqlite(locator)
                        .with_sqlite_profile(SqlitePersistenceProfile::Normal),
                ),
                m.binding.as_slice(),
                m.cursor_key,
            )),
            layerfs_protocol::DaemonStore::Postgres {
                url,
                schema,
                synchronous_commit,
                authority,
            } => Some((
                layerfs_persistence::StoreSelection::Postgres(postgres(
                    url,
                    schema,
                    synchronous_commit,
                )),
                authority.binding.as_slice(),
                authority.cursor_key,
            )),
            layerfs_protocol::DaemonStore::Sqlite { install: None, .. } => None,
        };
        let existing = match selected {
            Some((selection, binding, cursor_key)) => match open_store_observed(
                selection,
                binding,
                cursor_key,
                l.namespaces as usize,
                cache_bytes,
                ReservationBlocks::default(),
            ) {
                Ok(mut opened) => {
                    // #768: the daemon mounts, so its writer prepares the
                    // mount and unmount statements now, in one flight.
                    if let Err(cause) = opened.prepare_mounts() {
                        return Err(ApplicationError::AfterOverlay {
                            owner: Box::new(owner),
                            cause: Box::new(store_failure(&setup.store, cause)),
                        });
                    }
                    if matches!(setup.store, layerfs_protocol::DaemonStore::Postgres { .. }) {
                        // the status carries one printable word: `postgresql/17.6`
                        opened.sqlite_version = opened
                            .sqlite_version
                            .split_whitespace()
                            .take(2)
                            .collect::<Vec<_>>()
                            .join("/")
                            .chars()
                            .take(64)
                            .collect();
                    }
                    Some(opened)
                }
                Err(cause) => {
                    return Err(ApplicationError::AfterOverlay {
                        owner: Box::new(owner),
                        cause: Box::new(store_failure(&setup.store, cause)),
                    });
                }
            },
            None => None,
        };
        let kernel = match super::serving::assemble(&setup) {
            Ok(kernel) => kernel,
            Err(cause) => {
                return Err(ApplicationError::AfterOverlay {
                    owner: Box::new(owner),
                    cause: Box::new(ApplicationError::Io(cause)),
                })
            }
        };
        let export = match super::serving::assemble_export(&setup) {
            Ok(export) => export,
            Err(cause) => {
                return Err(ApplicationError::AfterOverlay {
                    owner: Box::new(owner),
                    cause: Box::new(ApplicationError::Io(cause)),
                })
            }
        };
        let service = existing.as_ref().map(|opened| {
            super::serving::service(
                opened.store.clone(),
                &owner,
                kernel.as_ref(),
                export.as_ref(),
                l.serial_low_water,
            )
        });
        let phase = if service.is_some() {
            DaemonPhase::ControlReady
        } else {
            DaemonPhase::InstallPending
        };
        let count = usize::from(l.connections) + 1;
        Ok(Arc::new(Self {
            setup,
            instance,
            owner,
            kernel,
            export,
            state: Mutex::new(State {
                startup: Startup {
                    phase,
                    service,
                    installed: None,
                    existing,
                    install_failure: None,
                },
                slots: Slots::new(count),
                retained: RetainedFailures::default(),
                workers: (0..count).map(|_| None).collect(),
            }),
            changed: Condvar::new(),
            fences: Mutex::new((0..count).map(|_| None).collect()),
        }))
    }
    /// Copies actual startup facts. It never opens/refreshes a Store or claims FUSE Ready.
    pub fn status(&self) -> Result<DaemonStatus, ApplicationError> {
        let state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
        Ok(self.observation(&state))
    }
    pub(super) fn observation(&self, state: &State) -> DaemonStatus {
        let startup = &state.startup;
        let store_sqlite = startup
            .installed
            .as_ref()
            .map(|v| v.opened.sqlite_version.clone())
            .or_else(|| startup.existing.as_ref().map(|v| v.sqlite_version.clone()))
            .or_else(|| {
                startup
                    .install_failure
                    .as_ref()
                    .and_then(|f| f.opened.as_ref())
                    .map(|v| v.sqlite_version.clone())
            });
        DaemonStatus {
            instance: self.instance,
            phase: startup.phase,
            overlay_sqlite: Some(self.owner.profile().sqlite_version.clone()),
            store_sqlite,
        }
    }
    pub(crate) fn hello(
        &self,
        request: HelloRequest,
        class: SlotClass,
    ) -> Result<DaemonStatus, crate::workspace::Failure> {
        use crate::workspace::Failure;
        use layerfs_protocol::daemon::ControlCode;
        let mut state = self.state.lock().map_err(|_| Failure::Poisoned)?;
        if class == SlotClass::Reserved
            && matches!(
                state.startup.phase,
                DaemonPhase::InstallPending | DaemonPhase::Installing
            )
        {
            return Err(Failure::Rejected(
                ControlCode::Capacity,
                "startup control cannot occupy reserved installer slot",
            ));
        }
        if request
            .expected_instance
            .is_some_and(|i| i != self.instance)
        {
            return Err(Failure::Rejected(
                ControlCode::Invalid,
                "stale daemon incarnation",
            ));
        }
        while request.wait_for_store
            && matches!(
                state.startup.phase,
                DaemonPhase::InstallPending | DaemonPhase::Installing
            )
        {
            state = self.changed.wait(state).map_err(|_| Failure::Poisoned)?;
        }
        Ok(self.observation(&state))
    }
    /// The bounded retained-failure store, pruned at this observation.
    pub fn retained_connection_failures(&self) -> Result<RetainedSnapshot, ApplicationError> {
        let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
        Ok(state.retained.snapshot(Instant::now()))
    }

    /// Vacant and in-use control slots, by class.
    pub fn connection_slots(&self) -> Result<SlotCounts, ApplicationError> {
        let state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
        Ok(state.slots.counts())
    }
}

/// A failed Store open or mount prepare: a PostgreSQL connection that is
/// gone (`Closed`) names the server's host; anything else is the Store's own
/// error.
fn store_failure(
    store: &layerfs_protocol::DaemonStore,
    cause: layerfs_storage::StorageError,
) -> ApplicationError {
    let closed = matches!(
        &cause,
        layerfs_storage::StorageError::Io(io)
            if matches!(
                io.get_ref().and_then(|e| {
                    e.downcast_ref::<layerfs_storage::port::PersistenceError>()
                }),
                Some(layerfs_storage::port::PersistenceError::Refused { status })
                    if status == "Closed"
            )
    );
    match (store, closed) {
        (layerfs_protocol::DaemonStore::Postgres { url, .. }, true) => {
            ApplicationError::PostgresUnreachable {
                host: postgres_host(url),
            }
        }
        _ => ApplicationError::Store(cause),
    }
}

/// The PostgreSQL Store selected (#506): Store reads and writes go to the
/// server. A Sandbox daemon opens it with the light check (#63): the schema
/// was fully checked when its Store was connected, and the caller's reach
/// check read its binding just before this daemon started.
fn postgres_host(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else {
        return String::new();
    };
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    match host_port.strip_prefix('[') {
        Some(v6) => v6.split_once(']').map_or("", |(host, _)| host).to_owned(),
        None => host_port
            .rsplit_once(':')
            .map_or(host_port, |(host, _)| host)
            .to_owned(),
    }
}

fn postgres(url: &str, schema: &str, synchronous_commit: &str) -> PostgresSelection {
    let setting = match synchronous_commit {
        "local" => SynchronousCommit::Local,
        "remote_write" => SynchronousCommit::RemoteWrite,
        "on" => SynchronousCommit::On,
        "remote_apply" => SynchronousCommit::RemoteApply,
        _ => SynchronousCommit::Off,
    };
    PostgresSelection::new(url.to_string(), schema.to_string())
        .with_synchronous_commit(setting)
        .with_check(layerfs_persistence::PostgresCheck::Light)
}

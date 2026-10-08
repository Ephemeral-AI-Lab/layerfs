//! One startup lifecycle owner, Store transfer and bounded connection custody.
use super::{ApplicationError, ConnectionFailure};
use crate::{
    bootstrap::{open_store_observed_with_limits, OpenedStore},
    control::Service,
    install_types::{InstallFailure, InstalledStore},
    Owner, OwnerConfig,
};
use layerfs_bridge::{
    control::{DaemonPhase, DaemonStatus, HelloRequest},
    daemon_setup::DaemonSetup,
    native,
};
use layerfs_overlay::ProfileConfig;
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::ReservationBlocks;
use std::{
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

pub(super) enum Slot {
    Vacant,
    Running,
    Retained(Box<ConnectionFailure>),
}
pub(super) struct Startup {
    pub phase: DaemonPhase,
    pub service: Option<Arc<Service>>,
    pub installed: Option<InstalledStore>,
    pub existing: Option<OpenedStore>,
    pub install_failure: Option<Box<InstallFailure>>,
}
pub(super) struct State {
    pub startup: Startup,
    pub slots: Vec<Slot>,
}
/// One real daemon application; Store/Overlay are initialized once and reused.
pub struct Application {
    pub(super) setup: DaemonSetup,
    pub(super) instance: [u8; 32],
    pub(super) owner: Owner,
    pub(super) state: Mutex<State>,
    pub(super) changed: Condvar,
    pub(super) workers: Mutex<Vec<Option<JoinHandle<()>>>>,
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
                bytes: usize_value(l.owner_bytes)?,
                lifecycle_reserve: usize_value(l.lifecycle_reserve)?,
                namespaces: l.namespaces as usize,
                jobs_per_namespace: l.ordinary_jobs as usize,
                lifecycle_jobs_per_namespace: l.lifecycle_jobs as usize,
            },
        );
        if started.result.is_err() {
            return Err(ApplicationError::Owner(Box::new(started)));
        }
        let owner = started.result.expect("original successful Overlay start");
        let instance = match native::generate_keypair() {
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
        let existing = match &setup.existing_store {
            Some(m) => match open_store_observed_with_limits(
                PersistenceConfig::sqlite(&setup.store)
                    .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
                &m.binding,
                m.cursor_key,
                usize::from(l.read_handles),
                cache_bytes,
                ReservationBlocks::default(),
                super::config::read_limits(l),
            ) {
                Ok(opened) => Some(opened),
                Err(cause) => {
                    return Err(ApplicationError::AfterOverlay {
                        owner: Box::new(owner),
                        cause: Box::new(ApplicationError::Store(cause)),
                    })
                }
            },
            None => None,
        };
        let service = existing
            .as_ref()
            .map(|opened| Arc::new(Service::new(opened.store.clone(), &owner)));
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
            state: Mutex::new(State {
                startup: Startup {
                    phase,
                    service,
                    installed: None,
                    existing,
                    install_failure: None,
                },
                slots: (0..count).map(|_| Slot::Vacant).collect(),
            }),
            changed: Condvar::new(),
            workers: Mutex::new((0..count).map(|_| None).collect()),
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
    pub(super) fn hello(
        &self,
        request: HelloRequest,
        slot: usize,
    ) -> Result<DaemonStatus, crate::control::Failure> {
        use crate::control::Failure;
        use layerfs_bridge::control::ControlCode;
        let mut state = self.state.lock().map_err(|_| Failure::Poisoned)?;
        if slot == usize::from(self.setup.limits.connections)
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
    /// Transfers original failed connection custody out only at an explicit operator request.
    /// Taking a receipt does not resolve its unknowns, replay it or remove artifacts.
    pub fn take_connection_failure(
        &self,
        slot: usize,
    ) -> Result<Option<Box<ConnectionFailure>>, ApplicationError> {
        let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
        let current = state.slots.get_mut(slot).ok_or_else(|| {
            ApplicationError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "connection slot",
            ))
        })?;
        if !matches!(current, Slot::Retained(_)) {
            return Ok(None);
        }
        let Slot::Retained(failure) = std::mem::replace(current, Slot::Vacant) else {
            unreachable!()
        };
        self.changed.notify_all();
        Ok(Some(failure))
    }
}

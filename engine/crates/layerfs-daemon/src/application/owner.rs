//! One startup lifecycle owner, Store transfer and bounded connection custody.
use super::retained::{RetainedFailure, RetainedFailures, RetainedSnapshot};
use super::{ApplicationError, ConnectionFailure};
use crate::{
    bootstrap::{open_store_observed, OpenedStore},
    control::Service,
    install_types::{InstallFailure, InstalledStore},
    Owner, OwnerConfig,
};
use layerfs_bridge::{
    control::{DaemonPhase, DaemonStatus, HelloRequest},
    daemon_setup::DaemonSetup,
    duplex as transport,
};
use layerfs_overlay::ProfileConfig;
use layerfs_persistence::{
    PersistenceConfig, PostgresSelection, SqlitePersistenceProfile, SynchronousCommit,
};
use layerfs_storage::ReservationBlocks;
use std::{
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::Instant,
};

pub(super) enum Slot {
    Vacant,
    Running,
}

/// What finishing one connection did with its failure, if any.
pub(super) enum Finished {
    /// The worker ended without a failure.
    Clean,
    /// The failure was written to the daemon log and needs no record.
    Released(Box<ConnectionFailure>),
    /// The failure was recorded in the bounded store; its slot is already free.
    Retained(RetainedFailure),
}

/// The fixed control-slot table and the bounded retained-failure store beside it.
///
/// A retained failure never owns a slot: `finish` always leaves it `Vacant`
/// and only records the failure. Admission capacity therefore cannot leak
/// (#540).
pub(super) struct Slots {
    slots: Vec<Slot>,
    retained: RetainedFailures,
}

impl Slots {
    pub(super) fn new(count: usize) -> Self {
        Self {
            slots: (0..count).map(|_| Slot::Vacant).collect(),
            retained: RetainedFailures::default(),
        }
    }

    /// Claims one vacant slot, marking it `Running`, or `None` when every
    /// slot is in use.
    pub(super) fn admit(&mut self) -> Option<usize> {
        let slot = self
            .slots
            .iter()
            .position(|value| matches!(value, Slot::Vacant))?;
        self.slots[slot] = Slot::Running;
        Some(slot)
    }

    /// Ends one worker: release its slot and, for a retained failure, record it.
    pub(super) fn finish(
        &mut self,
        slot: usize,
        result: Result<(), ConnectionFailure>,
        now: Instant,
    ) -> Finished {
        let Some(current) = self.slots.get_mut(slot) else {
            return Finished::Clean;
        };
        *current = Slot::Vacant;
        match result {
            Ok(()) => Finished::Clean,
            Err(failure) if releases_after_logging(&failure) => {
                Finished::Released(Box::new(failure))
            }
            Err(failure) => Finished::Retained(self.retained.record(&failure, now)),
        }
    }

    pub(super) fn snapshot(&mut self, now: Instant) -> RetainedSnapshot {
        self.retained.snapshot(now)
    }

    /// Vacant and running (in-use) slots.
    pub(super) fn counts(&self) -> (usize, usize) {
        let vacant = self
            .slots
            .iter()
            .filter(|value| matches!(value, Slot::Vacant))
            .count();
        (vacant, self.slots.len() - vacant)
    }
}

/// A failure with no decoded product attempt or custody: the existing daemon
/// log line is the whole record, and the slot is released.
fn releases_after_logging(failure: &ConnectionFailure) -> bool {
    failure.original.is_none()
        && failure.received.is_none()
        && matches!(
            failure.cause,
            ApplicationError::Channel(_) | ApplicationError::Io(_)
        )
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
    pub slots: Slots,
}
/// One real daemon application; Store/Overlay are initialized once and reused.
pub struct Application {
    pub(super) setup: DaemonSetup,
    pub(super) instance: [u8; 32],
    pub(super) owner: Owner,
    /// Fixed shared kernel workers, entered before control readiness.
    pub(super) kernel: Option<Arc<crate::control::KernelServing>>,
    /// NFS serving of the native route, when selected.
    pub(super) export: Option<Arc<crate::control::ExportServing>>,
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
            layerfs_bridge::DaemonStore::Sqlite {
                install: Some(m),
                locator,
            } => Some((
                layerfs_persistence::StoreSelection::Sqlite(
                    PersistenceConfig::sqlite(locator)
                        .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
                ),
                m.binding.as_slice(),
                m.cursor_key,
            )),
            layerfs_bridge::DaemonStore::Postgres {
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
            layerfs_bridge::DaemonStore::Sqlite { install: None, .. } => None,
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
                    if matches!(setup.store, layerfs_bridge::DaemonStore::Postgres { .. }) {
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
                        cause: Box::new(ApplicationError::Store(cause)),
                    })
                }
            },
            None => None,
        };
        let kernel = match super::filesystem::assemble(&setup) {
            Ok(kernel) => kernel,
            Err(cause) => {
                return Err(ApplicationError::AfterOverlay {
                    owner: Box::new(owner),
                    cause: Box::new(ApplicationError::Io(cause)),
                })
            }
        };
        let export = match super::filesystem::assemble_export(&setup) {
            Ok(export) => export,
            Err(cause) => {
                return Err(ApplicationError::AfterOverlay {
                    owner: Box::new(owner),
                    cause: Box::new(ApplicationError::Io(cause)),
                })
            }
        };
        let service = existing.as_ref().map(|opened| {
            super::filesystem::service(
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
    /// The bounded retained-failure store, pruned at this observation.
    pub fn retained_connection_failures(&self) -> Result<RetainedSnapshot, ApplicationError> {
        let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
        Ok(state.slots.snapshot(Instant::now()))
    }

    /// Vacant and in-use control slots.
    pub fn connection_slots(&self) -> Result<(usize, usize), ApplicationError> {
        let state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
        Ok(state.slots.counts())
    }
}

/// The PostgreSQL Store selected (#506): Store reads and writes go to the
/// server.
fn postgres(url: &str, schema: &str, synchronous_commit: &str) -> PostgresSelection {
    let setting = match synchronous_commit {
        "local" => SynchronousCommit::Local,
        "remote_write" => SynchronousCommit::RemoteWrite,
        "on" => SynchronousCommit::On,
        "remote_apply" => SynchronousCommit::RemoteApply,
        _ => SynchronousCommit::Off,
    };
    PostgresSelection::new(url.to_string(), schema.to_string()).with_synchronous_commit(setting)
}

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

/// Slots reserved for the installer (and, later, a presence lease); ordinary
/// controls never consume them.
pub const RESERVED_SLOTS: usize = 1;

/// The fixed class of one slot index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlotClass {
    /// Ordinary authenticated control connections.
    Pool,
    /// Installer today, a presence lease later.
    Reserved,
}

impl SlotClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pool => "pool",
            Self::Reserved => "reserved",
        }
    }
}

/// What a caller is admitted for; maps to the slot class it may use.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionClass {
    /// An ordinary control connection.
    ControlPool,
    /// The one-time Store installer.
    Install,
}

impl AdmissionClass {
    fn slot_class(self) -> SlotClass {
        match self {
            Self::ControlPool => SlotClass::Pool,
            Self::Install => SlotClass::Reserved,
        }
    }
}

/// One admission slot: free, or running one worker generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SlotState {
    Vacant,
    Running { generation: u64, class: SlotClass },
}

/// Proof that one admission owns a slot and generation. Every path ends in
/// exactly one `Slots::finish` with this ticket; a stale ticket changes
/// nothing, so a duplicate or late finish cannot free a newer worker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlotTicket {
    pub index: usize,
    pub generation: u64,
    pub class: SlotClass,
}

/// What finishing one connection did with its failure, if any.
pub enum Finished {
    /// The worker ended without a failure.
    Clean,
    /// The failure was written to the daemon log and needs no record.
    Released(Box<ConnectionFailure>),
    /// The failure was recorded in the bounded store; its slot is already free.
    Retained(Box<RetainedFailure>),
    /// A stale or duplicate finish: nothing changed.
    Stale,
}

/// Admission counts by class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlotCounts {
    pub pool_vacant: usize,
    pub pool_running: usize,
    pub reserved_vacant: usize,
    pub reserved_running: usize,
}

/// The fixed control-slot table; the bounded retained-failure store lives
/// beside it in [`State`].
///
/// A retained failure never owns a slot: `finish` always leaves it `Vacant`
/// and only describes the failure. Admission capacity therefore cannot leak
/// (#540). The layout is `connections` `Pool` entries followed by
/// [`RESERVED_SLOTS`] `Reserved` entries; a class only ever admits within its
/// own range.
pub struct Slots {
    slots: Vec<SlotState>,
    pool: usize,
    next_generation: u64,
}

impl Slots {
    pub fn new(count: usize) -> Self {
        let pool = count.saturating_sub(RESERVED_SLOTS);
        let mut slots = Vec::with_capacity(pool + RESERVED_SLOTS);
        slots.resize(pool, SlotState::Vacant);
        slots.resize(pool + RESERVED_SLOTS, SlotState::Vacant);
        Self {
            slots,
            pool,
            next_generation: 0,
        }
    }

    fn class_of(&self, index: usize) -> SlotClass {
        if index < self.pool {
            SlotClass::Pool
        } else {
            SlotClass::Reserved
        }
    }

    /// Claims one vacant slot of `class`, or `None` when that class is full.
    /// A pool admission never takes a reserved slot and vice versa.
    pub fn admit(&mut self, class: AdmissionClass) -> Option<SlotTicket> {
        let wanted = class.slot_class();
        let index = self.slots.iter().enumerate().find_map(|(index, value)| {
            (self.class_of(index) == wanted && matches!(value, SlotState::Vacant)).then_some(index)
        })?;
        self.generation_ticket(index, wanted)
    }

    fn generation_ticket(&mut self, index: usize, class: SlotClass) -> Option<SlotTicket> {
        self.next_generation = self.next_generation.saturating_add(1);
        let generation = self.next_generation;
        *self.slots.get_mut(index)? = SlotState::Running { generation, class };
        Some(SlotTicket {
            index,
            generation,
            class,
        })
    }

    /// Ends one worker: releases its slot and, for a retained failure, returns
    /// the already-described record for the caller to store. A ticket whose
    /// generation or class no longer matches changes nothing.
    pub fn finish(
        &mut self,
        ticket: SlotTicket,
        result: Result<(), ConnectionFailure>,
        disposition: Option<Disposition>,
    ) -> Finished {
        let running = matches!(
            self.slots.get(ticket.index),
            Some(SlotState::Running { generation, class })
                if *generation == ticket.generation && *class == ticket.class
        );
        if !running {
            return Finished::Stale;
        }
        self.slots[ticket.index] = SlotState::Vacant;
        match result {
            Ok(()) => Finished::Clean,
            Err(failure) => match disposition {
                Some(Disposition::Released) => Finished::Released(Box::new(failure)),
                Some(Disposition::Retained(record)) => Finished::Retained(record),
                None => unreachable!("a finished failure is classified before the lock"),
            },
        }
    }

    /// Admission counts for each class.
    pub fn counts(&self) -> SlotCounts {
        let mut counts = SlotCounts {
            pool_vacant: 0,
            pool_running: 0,
            reserved_vacant: 0,
            reserved_running: 0,
        };
        for (index, value) in self.slots.iter().enumerate() {
            match (self.class_of(index), value) {
                (SlotClass::Pool, SlotState::Vacant) => counts.pool_vacant += 1,
                (SlotClass::Pool, SlotState::Running { .. }) => counts.pool_running += 1,
                (SlotClass::Reserved, SlotState::Vacant) => counts.reserved_vacant += 1,
                (SlotClass::Reserved, SlotState::Running { .. }) => counts.reserved_running += 1,
            }
        }
        counts
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

/// The terminal disposition of one finished connection failure.
#[derive(Debug)]
pub enum Disposition {
    /// Logged only: the slot is already free and no record is kept.
    Released,
    /// A bounded retained record is kept in the store.
    Retained(Box<RetainedFailure>),
}

/// Classifies one finished failure and builds the bounded record for a
/// retained one. Pure: no lock and no store access. `owner` names the worker
/// in a retained record; `now` is reserved for the in-flight deadline a later
/// slice adds.
pub fn disposition(
    failure: &ConnectionFailure,
    ticket: SlotTicket,
    owner: &str,
    _now: Instant,
) -> Disposition {
    if releases_after_logging(failure) {
        Disposition::Released
    } else {
        Disposition::Retained(Box::new(RetainedFailure::describe(
            failure,
            ticket.class,
            ticket.generation,
            owner,
        )))
    }
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
    pub retained: RetainedFailures,
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
                retained: RetainedFailures::default(),
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
        class: SlotClass,
    ) -> Result<DaemonStatus, crate::control::Failure> {
        use crate::control::Failure;
        use layerfs_bridge::control::ControlCode;
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

/// The PostgreSQL Store selected (#506): Store reads and writes go to the
/// server. A Sandbox daemon opens it with the light check (#63): the schema
/// was fully checked when its Store was connected, and the caller's reach
/// check read its binding just before this daemon started.
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

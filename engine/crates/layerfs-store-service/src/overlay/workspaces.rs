//! Closed template and a bounded roster of independently served Workspace files.
use super::{
    backing::Backing,
    owner::{Owner, OwnerClient, OwnerConfig, OwnerError},
    state::Shared,
};
use crate::OwnerWork;
use layerfs_overlay::{Overlay, ProfileConfig};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

enum Slot {
    Starting([u8; 32]),
    Live {
        owner: Arc<Mutex<Option<Owner>>>,
        incarnation: [u8; 32],
    },
    Retiring {
        incarnation: [u8; 32],
    },
    Failed {
        cause: Arc<OwnerError>,
        incarnation: [u8; 32],
    },
}
pub(super) enum CleanupTarget {
    Client(OwnerClient),
    Queued,
    Gone,
}
struct State {
    slots: BTreeMap<i64, Slot>,
    next: i64,
    stopping: bool,
    retired: OwnerWork,
}

pub struct Workspaces {
    template: PathBuf,
    profile: ProfileConfig,
    config: OwnerConfig,
    template_bytes: u64,
    template_file: Mutex<layerfs_overlay::ClosedFile>,
    template_origin: layerfs_overlay::AllocationWork,
    state: Mutex<State>,
}

pub(super) fn start(path: &Path, profile: ProfileConfig, config: OwnerConfig) -> crate::OwnerStart {
    let started = Instant::now();
    let mut startup = Arc::new(layerfs_overlay::CreationWork::default());
    let mut reported = false;
    let result = (|| {
        if config.namespaces == 0 || profile.pager_kib == 0 {
            return Err(OwnerError::InvalidAdmission);
        }
        let created = Overlay::create_observed(path, profile);
        startup = Arc::new(created.work);
        reported = true;
        let db = created.result.map_err(OwnerError::Overlay)?;
        let actual = db.profile().clone();
        let allocation = db.allocation().map_err(OwnerError::Overlay)?;
        let receipt = Arc::get_mut(&mut startup).expect("unshared startup receipt");
        receipt.sql = db.diagnostics();
        receipt.allocation = db.allocation_work();
        receipt.allocation_state = Some(Ok(allocation));
        let template_file = db.close_database_file().map_err(OwnerError::Overlay)?;
        let template_origin = template_file.allocation_work();
        let pool = Arc::new(Workspaces {
            template: path.to_owned(),
            profile,
            config,
            template_bytes: allocation.logical_bytes,
            template_file: Mutex::new(template_file),
            template_origin,
            state: Mutex::new(State {
                slots: BTreeMap::new(),
                next: 1,
                stopping: false,
                retired: OwnerWork::default(),
            }),
        });
        Ok(Owner {
            client: OwnerClient {
                context: super::owner::Context::Roster(pool),
            },
            profile: actual,
        })
    })();
    crate::OwnerStart {
        result,
        startup,
        creation_reported: reported,
        elapsed_ns: started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
    }
}

impl Workspaces {
    pub fn configuration(&self) -> OwnerConfig {
        self.config
    }
    pub fn client(&self, namespace: i64) -> Result<OwnerClient, OwnerError> {
        let state = self.state.lock().map_err(|_| OwnerError::Stopped)?;
        if state.stopping {
            return Err(OwnerError::Stopped);
        }
        match state.slots.get(&namespace) {
            Some(Slot::Live { owner, .. }) => owner
                .lock()
                .map_err(|_| OwnerError::Stopped)?
                .as_ref()
                .map(Owner::client)
                .ok_or(OwnerError::Stopped),
            Some(Slot::Failed { cause, .. }) => Err(OwnerError::Retained(cause.clone())),
            _ => Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Stale)),
        }
    }
    fn fail(&self, namespace: i64, cause: OwnerError) -> OwnerError {
        let cause = Arc::new(cause);
        if let Ok(mut state) = self.state.lock() {
            let incarnation = match state.slots.get(&namespace) {
                Some(Slot::Starting(incarnation))
                | Some(Slot::Live { incarnation, .. })
                | Some(Slot::Retiring { incarnation, .. })
                | Some(Slot::Failed { incarnation, .. }) => *incarnation,
                None => [0; 32],
            };
            state.slots.insert(
                namespace,
                Slot::Failed {
                    cause: cause.clone(),
                    incarnation,
                },
            );
        }
        OwnerError::Retained(cause)
    }
    pub(super) fn open(self: &Arc<Self>, incarnation: [u8; 32]) -> Result<OwnerClient, OwnerError> {
        if incarnation == [0; 32] {
            return Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Invalid(
                "zero incarnation",
            )));
        }
        let physical = self
            .template_file
            .lock()
            .map_err(|_| OwnerError::Stopped)?
            .allocation()
            .map_err(OwnerError::Overlay)?;
        if physical.logical_bytes != self.template_bytes {
            return Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Invalid(
                "template size",
            )));
        }
        let namespace = {
            let mut state = self.state.lock().map_err(|_| OwnerError::Stopped)?;
            if state.stopping {
                return Err(OwnerError::Stopped);
            }
            if state.slots.values().any(|slot| match slot {
                Slot::Starting(stored)
                | Slot::Live {
                    incarnation: stored,
                    ..
                }
                | Slot::Retiring {
                    incarnation: stored,
                    ..
                }
                | Slot::Failed {
                    incarnation: stored,
                    ..
                } => *stored == incarnation,
            }) {
                return Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Invalid(
                    "duplicate incarnation",
                )));
            }
            if state.slots.len() >= self.config.namespaces {
                return Err(OwnerError::AdmissionFull);
            }
            let namespace = state.next;
            state.next = namespace
                .checked_add(1)
                .ok_or(OwnerError::IdentityExhausted)?;
            state.slots.insert(namespace, Slot::Starting(incarnation));
            namespace
        };
        let path = self
            .template
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("{namespace}.sqlite"));
        let shared = Arc::new(Shared::new(namespace));
        let owner =
            match Owner::start_copy(&path, &self.template, self.profile, shared.clone()).result {
                Ok(owner) => owner,
                Err(cause) => return Err(self.fail(namespace, cause)),
            };
        let backing = match Backing::new(path, namespace, incarnation, Arc::downgrade(self)) {
            Ok(backing) => Arc::new(backing),
            Err(cause) => return Err(self.fail(namespace, OwnerError::Io(cause))),
        };
        shared
            .state
            .lock()
            .map_err(|_| OwnerError::Stopped)?
            .backing = Some(backing);
        let client = owner.client();
        self.state
            .lock()
            .map_err(|_| OwnerError::Stopped)?
            .slots
            .insert(
                namespace,
                Slot::Live {
                    owner: Arc::new(Mutex::new(Some(owner))),
                    incarnation,
                },
            );
        Ok(client)
    }
    pub(super) fn cleanup(
        &self,
        namespace: i64,
        incarnation: [u8; 32],
    ) -> Result<CleanupTarget, OwnerError> {
        let state = self.state.lock().map_err(|_| OwnerError::Stopped)?;
        if state.stopping {
            return Err(OwnerError::Stopped);
        }
        if namespace <= 0 || namespace >= state.next || incarnation == [0; 32] {
            return Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Stale));
        }
        match state.slots.get(&namespace) {
            Some(Slot::Live {
                owner,
                incarnation: stored,
            }) if *stored == incarnation => owner
                .lock()
                .map_err(|_| OwnerError::Stopped)?
                .as_ref()
                .map(Owner::client)
                .map(CleanupTarget::Client)
                .ok_or(OwnerError::Stopped),
            Some(Slot::Retiring {
                incarnation: stored,
                ..
            }) if *stored == incarnation => Ok(CleanupTarget::Queued),
            Some(Slot::Failed { cause, .. }) => Err(OwnerError::Retained(cause.clone())),
            None => Ok(CleanupTarget::Gone),
            _ => Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Stale)),
        }
    }
    fn clients(&self) -> Result<Vec<OwnerClient>, OwnerError> {
        let state = self.state.lock().map_err(|_| OwnerError::Stopped)?;
        state
            .slots
            .values()
            .filter_map(|slot| match slot {
                Slot::Live { owner, .. } => Some(
                    owner
                        .lock()
                        .map_err(|_| OwnerError::Stopped)
                        .and_then(|owner| {
                            owner.as_ref().map(Owner::client).ok_or(OwnerError::Stopped)
                        }),
                ),
                _ => None,
            })
            .collect()
    }
    pub fn diagnostics(&self) -> Result<OwnerWork, OwnerError> {
        let mut total = self.state.lock().map_err(|_| OwnerError::Stopped)?.retired;
        for client in self.clients()? {
            let mut work = client.diagnostics()?;
            // The pool's terminal count is the successful file deletion, not
            // reclamation of a namespace row inside a private connection.
            work.closed_namespaces = 0;
            total.accumulate(&work);
        }
        total.allocation_foreground.accumulate(
            self.template_file
                .lock()
                .map_err(|_| OwnerError::Stopped)?
                .allocation_work()
                .since(self.template_origin),
        );
        Ok(total)
    }
    pub fn maintenance_failure(
        &self,
    ) -> Result<Option<Arc<layerfs_overlay::OverlayError>>, OwnerError> {
        for client in self.clients()? {
            if let Some(cause) = client.maintenance_failure()? {
                return Ok(Some(cause));
            }
        }
        Ok(None)
    }
    pub fn retire(&self, namespace: i64) -> Result<(), OwnerError> {
        self.retire_with(namespace, true)
    }
    fn retire_with(&self, namespace: i64, terminal: bool) -> Result<(), OwnerError> {
        let owner = {
            let mut state = self.state.lock().map_err(|_| OwnerError::Stopped)?;
            let Some(Slot::Live { owner, incarnation }) = state.slots.get(&namespace) else {
                return Err(OwnerError::Stopped);
            };
            let incarnation = *incarnation;
            let owner = owner
                .lock()
                .map_err(|_| OwnerError::Stopped)?
                .take()
                .ok_or(OwnerError::Stopped)?;
            let backing = owner
                .client
                .connection()?
                .state
                .lock()
                .map_err(|_| OwnerError::Stopped)?
                .backing
                .as_ref()
                .ok_or(OwnerError::Stopped)?
                .clone();
            if terminal {
                backing.terminal();
            }
            state
                .slots
                .insert(namespace, Slot::Retiring { incarnation });
            owner
        };
        let client = owner.client();
        let result = owner.stop();
        let mut observed = client.diagnostics()?;
        observed.closed_namespaces = 0;
        self.state
            .lock()
            .map_err(|_| OwnerError::Stopped)?
            .retired
            .accumulate(&observed);
        match result {
            Ok(()) => Ok(()),
            Err(cause) => Err(self.fail(namespace, cause)),
        }
    }
    pub fn deleted(
        &self,
        namespace: i64,
        _incarnation: [u8; 32],
        result: Result<(), (std::io::Error, std::fs::File)>,
    ) {
        if let Ok(mut state) = self.state.lock() {
            match result {
                Ok(()) => {
                    state.slots.remove(&namespace);
                    state.retired.closed_namespaces =
                        state.retired.closed_namespaces.saturating_add(1);
                }
                Err((cause, backing)) => {
                    state.slots.insert(
                        namespace,
                        Slot::Failed {
                            cause: Arc::new(OwnerError::Deletion { cause, backing }),
                            incarnation: _incarnation,
                        },
                    );
                }
            }
        }
    }
    pub fn stop(&self) -> Result<(), OwnerError> {
        let namespaces = {
            let mut state = self.state.lock().map_err(|_| OwnerError::Stopped)?;
            if state.stopping {
                return Ok(());
            }
            state.stopping = true;
            state
                .slots
                .iter()
                .filter_map(|(namespace, slot)| {
                    matches!(slot, Slot::Live { .. }).then_some(*namespace)
                })
                .collect::<Vec<_>>()
        };
        let mut first = None;
        for namespace in namespaces {
            if let Err(cause) = self.retire_with(namespace, false) {
                first.get_or_insert(cause);
            }
        }
        match first {
            Some(cause) => Err(cause),
            None => Ok(()),
        }
    }
}

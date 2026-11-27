//! Bounded control-slot admission: slot classes, phases and tickets, and the
//! disposition of one finished connection failure.
use super::super::{ApplicationError, ConnectionFailure};
use super::retained::RetainedFailure;
use std::time::{Duration, Instant};

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
    /// The one-time Store installer, admitted only during startup.
    Install,
    /// The recovery class: the reserved slot once the daemon is ready, used
    /// only when the pool has no vacant slot (#547 slice 3).
    Recovery,
}

impl AdmissionClass {
    fn slot_class(self) -> SlotClass {
        match self {
            Self::ControlPool => SlotClass::Pool,
            Self::Install | Self::Recovery => SlotClass::Reserved,
        }
    }
}

/// The lifecycle phase of one admitted (non-vacant) slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Phase {
    /// Authenticating before the first request. Nothing here is reapable.
    Busy,
    /// Blocked in `receive()` awaiting the next request: nothing is in
    /// flight, so the daemon may reclaim it once `idle_ttl` passes (#547).
    Idle { since: Instant },
    /// Executing one decoded request; past `deadline` its effect is unknown
    /// and the daemon expires it (#547 slice 5).
    InFlight { started: Instant, deadline: Instant },
}

/// One admission slot: free, or running one worker generation in a phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SlotState {
    Vacant,
    Running {
        generation: u64,
        class: SlotClass,
        phase: Phase,
    },
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
/// beside it in the startup lifecycle `State`.
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
        *self.slots.get_mut(index)? = SlotState::Running {
            generation,
            class,
            phase: Phase::Busy,
        };
        Some(SlotTicket {
            index,
            generation,
            class,
        })
    }

    /// Marks one running slot as blocked in `receive()`; a stale ticket
    /// changes nothing.
    pub fn idle(&mut self, ticket: SlotTicket, since: Instant) -> bool {
        let running = matches!(
            self.slots.get(ticket.index),
            Some(SlotState::Running { generation, class, .. })
                if *generation == ticket.generation && *class == ticket.class
        );
        if running {
            self.slots[ticket.index] = SlotState::Running {
                generation: ticket.generation,
                class: ticket.class,
                phase: Phase::Idle { since },
            };
        }
        running
    }

    /// Marks one running slot as executing again (not reapable); a stale
    /// ticket changes nothing.
    pub fn busy(&mut self, ticket: SlotTicket) -> bool {
        let running = matches!(
            self.slots.get(ticket.index),
            Some(SlotState::Running { generation, class, .. })
                if *generation == ticket.generation && *class == ticket.class
        );
        if running {
            self.slots[ticket.index] = SlotState::Running {
                generation: ticket.generation,
                class: ticket.class,
                phase: Phase::Busy,
            };
        }
        running
    }

    /// Marks one running slot as executing one decoded request with its
    /// expiry deadline (#547 slice 5); a stale ticket changes nothing.
    pub fn inflight(&mut self, ticket: SlotTicket, started: Instant, deadline: Instant) -> bool {
        let running = matches!(
            self.slots.get(ticket.index),
            Some(SlotState::Running { generation, class, .. })
                if *generation == ticket.generation && *class == ticket.class
        );
        if running {
            self.slots[ticket.index] = SlotState::Running {
                generation: ticket.generation,
                class: ticket.class,
                phase: Phase::InFlight { started, deadline },
            };
        }
        running
    }

    /// Frees every pool slot whose worker has been parked in `receive()` for
    /// at least `idle_ttl`, returning their tickets. The caller closes their
    /// sockets; the woken workers finish with a stale ticket and change
    /// nothing (invariant 1). Reserved slots are never reaped: recovery is
    /// closed after one request and install is one-time.
    pub fn reap_idle(&mut self, now: Instant, idle_ttl: Duration) -> Vec<SlotTicket> {
        let pool = self.pool;
        let mut reaped = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if index >= pool {
                break;
            }
            if let SlotState::Running {
                generation,
                class,
                phase: Phase::Idle { since },
            } = slot
            {
                if now.saturating_duration_since(*since) >= idle_ttl {
                    reaped.push(SlotTicket {
                        index,
                        generation: *generation,
                        class: *class,
                    });
                    *slot = SlotState::Vacant;
                }
            }
        }
        reaped
    }

    /// Frees every pool slot whose in-flight request passed its `deadline`,
    /// returning their tickets so the caller closes their sockets and records
    /// one `Expired` retained failure each (#547 slice 5). The interrupted
    /// operation's effect is unknown, so it is never released as clean. The
    /// reserved slot is never expired automatically: install is one-time and
    /// recovery is closed after one request.
    pub fn reap_expired(&mut self, now: Instant) -> Vec<SlotTicket> {
        let pool = self.pool;
        let mut expired = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if index >= pool {
                break;
            }
            if let SlotState::Running {
                generation,
                class,
                phase: Phase::InFlight { deadline, .. },
            } = slot
            {
                if now >= *deadline {
                    expired.push(SlotTicket {
                        index,
                        generation: *generation,
                        class: *class,
                    });
                    *slot = SlotState::Vacant;
                }
            }
        }
        expired
    }

    /// Frees one running slot whose generation matches, returning its ticket;
    /// `None` when the slot is vacant or a newer worker owns it (#547 slice
    /// 5). Unlike `finish`, this is operator-initiated: the caller closes the
    /// socket and records `Reclaimed`. Any class may be reclaimed.
    pub fn reclaim(&mut self, slot: usize, generation: u64) -> Option<SlotTicket> {
        match self.slots.get(slot) {
            Some(SlotState::Running {
                generation: owned,
                class,
                ..
            }) if *owned == generation => {
                let ticket = SlotTicket {
                    index: slot,
                    generation: *owned,
                    class: *class,
                };
                self.slots[slot] = SlotState::Vacant;
                Some(ticket)
            }
            _ => None,
        }
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
            Some(SlotState::Running { generation, class, .. })
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

/// The recovery operation set the reserved slot serves once the daemon is
/// ControlReady (#547 slice 3): close, workspace status, the operator read
/// model and reclaim (#547 slice 5). Everything else is refused `Capacity`
/// and the connection is closed.
pub fn recovery_request(request: &layerfs_protocol::daemon::Request) -> bool {
    use layerfs_protocol::daemon::Request;
    match request {
        Request::Status(_) | Request::Unmount(_) => true,
        Request::Sdk(call) => {
            call.method == layerfs_protocol::daemon::OPERATION
                || call.method == layerfs_protocol::daemon::RECLAIM
        }
        _ => false,
    }
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

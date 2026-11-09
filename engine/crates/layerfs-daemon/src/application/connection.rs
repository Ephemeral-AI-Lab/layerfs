//! Bounded real authenticated connection workers and retained original failures.
use super::{
    owner::{
        disposition, recovery_request, AdmissionClass, Finished, SlotClass, SlotCounts, SlotTicket,
    },
    retained::{RetainedFailure, RetainedSnapshot},
    Application, ApplicationError, ConnectionFailure,
};
use layerfs_bridge::{
    control::{Call, DaemonPhase, Reply, Request, RECLAIM},
    duplex::{
        self as transport,
        admission::{self as admission_notice, AdmissionNotice, AdmissionReason},
    },
    initial_record::InitialRecord,
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

/// Bounded conversion of an admission count to the notice's one byte.
fn saturating(count: usize) -> u8 {
    u8::try_from(count).unwrap_or(u8::MAX)
}

/// Writes one advisory pre-handshake capacity notice and drains what the
/// caller may still send, so it reads the notice rather than a reset. Every
/// socket operation is bounded; failure is ignored (the socket ends anyway).
fn write_capacity_notice(stream: &mut TcpStream, notice: &AdmissionNotice) {
    let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    if admission_notice::write(stream, notice).is_err() {
        return;
    }
    let mut sink = [0u8; 512];
    let _ = stream.read(&mut sink);
}

/// A synthetic failure for a worker that panicked: bounded, and recorded
/// through the same one-finish path as any other failure.
fn panic_failure(ticket: SlotTicket, payload: Box<dyn std::any::Any + Send>) -> ConnectionFailure {
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_owned());
    let mut message = format!("connection worker panicked: {message}");
    if message.len() > 512 {
        let mut end = 512;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push('…');
    }
    ConnectionFailure {
        slot: ticket.index,
        received: None,
        original: None,
        cause: ApplicationError::Worker(message),
        fence_error: None,
        diagnostic_error: None,
    }
}

/// Owns one admission ticket. Exactly one `finish` happens: the explicit
/// call, or `Drop` during an unwind or a spawn failure.
struct SlotGuard {
    application: Arc<Application>,
    ticket: SlotTicket,
    owner: String,
    done: bool,
}

impl SlotGuard {
    fn finish(&mut self, result: Result<(), ConnectionFailure>) {
        if self.done {
            return;
        }
        self.done = true;
        let now = Instant::now();
        // Classify and build the bounded record before taking the state lock.
        let classified = result
            .as_ref()
            .err()
            .map(|failure| disposition(failure, self.ticket, &self.owner, now));
        // Poison recovery here only releases the slot and records; normal
        // admission still reports `Poisoned` rather than trusting corrupt state.
        let finished = {
            let mut state = self
                .application
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let finished = state.slots.finish(self.ticket, result, classified);
            let finished = match finished {
                Finished::Retained(record) => {
                    let recorded = state.retained.record(*record, now);
                    Finished::Retained(Box::new(recorded))
                }
                other => other,
            };
            self.application.changed.notify_all();
            finished
        };
        log_finished(finished);
    }
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        if !self.done {
            self.finish(Err(ConnectionFailure {
                slot: self.ticket.index,
                received: None,
                original: None,
                cause: ApplicationError::Worker("dropped before completion".to_owned()),
                fence_error: None,
                diagnostic_error: None,
            }));
        }
    }
}

/// One bounded daemon-log line per terminal transition.
fn log_finished(finished: Finished) {
    match finished {
        Finished::Clean | Finished::Stale => {}
        Finished::Released(failure) => {
            let _ = writeln!(std::io::stderr().lock(), "{failure:?}");
        }
        Finished::Retained(record) => {
            let _ = writeln!(std::io::stderr().lock(), "{record}");
        }
    }
}

impl Application {
    /// Accepts ordinary authenticated controls on one listener. The listener and
    /// each admitted worker own their exact sockets; no command execution occurs.
    /// A failed connection releases its slot at once; a compact record of a
    /// failure that used to retain custody stays in the bounded store.
    pub fn serve(self: &Arc<Self>, listener: TcpListener) -> Result<(), ApplicationError> {
        loop {
            let (mut stream, _) = listener.accept().map_err(ApplicationError::Io)?;
            let ticket = {
                let idle_ttl = Duration::from_millis(u64::from(self.setup.limits.idle_ttl_ms));
                let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                // In-flight expiry (#547 slice 5): a pool slot whose decoded
                // request is still executing past `request_deadline` has an
                // unknown effect; free it, close its socket and record
                // `Expired` so the caller observes
                // `Unavailable { outcome: unknown }` rather than a silent hang.
                let expired = state.slots.reap_expired(Instant::now());
                if !expired.is_empty() {
                    drop(state);
                    self.expire_in_flight(&expired);
                    state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                }
                let mut picked = state.slots.admit(AdmissionClass::ControlPool);
                // Idle reclamation (#547 slice 4): a pool slot whose worker
                // is parked in `receive()` past `idle_ttl` holds nothing in
                // flight, so free it under admission pressure.
                if picked.is_none() {
                    let reaped = state.slots.reap_idle(Instant::now(), idle_ttl);
                    if !reaped.is_empty() {
                        drop(state);
                        self.close_slot_fences(&reaped);
                        for ticket in &reaped {
                            let _ = writeln!(
                                std::io::stderr().lock(),
                                "layerfs-daemon: reclaimed idle control slot {}",
                                ticket.index
                            );
                        }
                        state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                        picked = state.slots.admit(AdmissionClass::ControlPool);
                    }
                }
                // Recovery class (#547 slice 3): the reserved slot admits one
                // connection when the pool is full. During startup it is the
                // installer; once ready it serves only the recovery set.
                if picked.is_none() {
                    picked = match state.startup.phase {
                        DaemonPhase::InstallPending | DaemonPhase::Installing => {
                            state.slots.admit(AdmissionClass::Install)
                        }
                        DaemonPhase::ControlReady => state.slots.admit(AdmissionClass::Recovery),
                        _ => None,
                    };
                }
                match picked {
                    Some(ticket) => ticket,
                    None => {
                        let counts = state.slots.counts();
                        let retained = state.retained.len();
                        state.retained.reap(Instant::now());
                        let reason = if state.startup.phase != DaemonPhase::ControlReady {
                            AdmissionReason::Starting
                        } else {
                            AdmissionReason::NoPoolSlot
                        };
                        let notice = AdmissionNotice::new(
                            reason,
                            saturating(retained),
                            saturating(counts.pool_vacant),
                            saturating(counts.reserved_vacant),
                        );
                        drop(state);
                        // No authenticated operation has entered; tell the
                        // caller why once, then end the socket (#540).
                        write_capacity_notice(&mut stream, &notice);
                        continue;
                    }
                }
            };
            let previous = {
                let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                state.workers[ticket.index].take()
            };
            if let Some(worker) = previous {
                worker.join().map_err(|_| {
                    ApplicationError::Io(std::io::Error::other(
                        "connection worker panicked; custody unestablished",
                    ))
                })?;
            }
            let application = self.clone();
            let owner = self.owner_label(ticket);
            // The guard owns the ticket: if the worker body unwinds (or the
            // thread never starts), it releases the slot and records exactly
            // once. The normal path calls `finish` explicitly, so no path can
            // leak a slot (#540).
            let guard = SlotGuard {
                application: self.clone(),
                ticket,
                owner,
                done: false,
            };
            let worker = thread::Builder::new()
                .name(format!("layerfs-control-{}", ticket.index))
                .spawn(move || {
                    let mut guard = guard;
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        application.connection(stream, ticket)
                    }))
                    .unwrap_or_else(|payload| Err(panic_failure(ticket, payload)));
                    application.clear_fence(ticket);
                    guard.finish(result);
                });
            match worker {
                Ok(worker) => {
                    self.state
                        .lock()
                        .map_err(|_| ApplicationError::Poisoned)?
                        .workers[ticket.index] = Some(worker)
                }
                Err(_cause) => {
                    // Spawning dropped the closure, so the guard's `Drop`
                    // already released the slot and recorded the failure.
                }
            }
        }
    }

    /// A bounded owner label for one worker's retained record.
    fn owner_label(&self, ticket: SlotTicket) -> String {
        format!(
            "{:02x}{:02x}{:02x}{:02x}:{}:layerfs-control-{}",
            self.instance[0],
            self.instance[1],
            self.instance[2],
            self.instance[3],
            std::process::id(),
            ticket.index
        )
    }

    /// Stores the socket fence of one worker, keyed by its admission
    /// generation, so the accept loop can close it to reclaim an idle
    /// connection or expire an in-flight request (#547 slices 4-5).
    fn set_fence(&self, ticket: SlotTicket, fence: transport::CloseHandle) {
        if let Ok(mut fences) = self.fences.lock() {
            if let Some(entry) = fences.get_mut(ticket.index) {
                *entry = Some((ticket.generation, fence));
            }
        }
    }

    /// Removes one worker's socket fence. A stale worker that finished after
    /// its slot was reclaimed or expired skips a newer worker's fence because
    /// the generation no longer matches (#547 slice 5).
    fn clear_fence(&self, ticket: SlotTicket) {
        if let Ok(mut fences) = self.fences.lock() {
            if let Some(entry) = fences.get_mut(ticket.index) {
                if matches!(entry, Some((generation, _)) if *generation == ticket.generation) {
                    *entry = None;
                }
            }
        }
    }

    /// Closes the socket fence of each named worker generation, waking one
    /// parked in `receive()` or a fence wait. A missing or stale fence (the
    /// worker already ended or its slot was re-admitted) is skipped.
    fn close_slot_fences(&self, tickets: &[SlotTicket]) {
        let mut fences = self
            .fences
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for ticket in tickets {
            if let Some(entry) = fences.get_mut(ticket.index) {
                if matches!(entry, Some((generation, _)) if *generation == ticket.generation) {
                    if let Some((_, fence)) = entry.take() {
                        let _ = fence.close();
                    }
                }
            }
        }
    }

    /// Marks one worker's slot as parked in `receive()` (reapable).
    fn mark_idle(&self, ticket: SlotTicket) {
        if let Ok(mut state) = self.state.lock() {
            state.slots.idle(ticket, Instant::now());
        }
    }

    /// Marks one worker's slot as executing one decoded request with its
    /// expiry deadline (#547 slice 5).
    fn mark_inflight(&self, ticket: SlotTicket, started: Instant, deadline: Instant) {
        if let Ok(mut state) = self.state.lock() {
            state.slots.inflight(ticket, started, deadline);
        }
    }

    /// Frees every slot whose in-flight request passed `request_deadline`
    /// (#547 slice 5): closes its socket (waking a worker parked in socket
    /// I/O; a worker parked elsewhere finishes stale when it returns) and
    /// records one `Expired` retained failure each. The interrupted
    /// operation's effect is unknown, so it is never released as clean.
    fn expire_in_flight(&self, tickets: &[SlotTicket]) {
        let now = Instant::now();
        self.close_slot_fences(tickets);
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for ticket in tickets {
            // Detach the stuck worker: its eventual finish is stale, and the
            // accept loop must never join a worker a Store/fence wait holds.
            state.workers[ticket.index] = None;
            let owner = self.owner_label(*ticket);
            let recorded = state
                .retained
                .record(RetainedFailure::expired(*ticket, &owner), now);
            let _ = writeln!(std::io::stderr().lock(), "{recorded}");
        }
        self.changed.notify_all();
    }

    fn connection(&self, stream: TcpStream, ticket: SlotTicket) -> Result<(), ConnectionFailure> {
        let slot = ticket.index;
        let timeout = Some(Duration::from_millis(u64::from(
            self.setup.limits.handshake_ms,
        )));
        let socket_failure = |cause| ConnectionFailure {
            slot,
            received: None,
            original: None,
            cause: ApplicationError::Io(cause),
            fence_error: None,
            diagnostic_error: None,
        };
        stream.set_read_timeout(timeout).map_err(socket_failure)?;
        stream.set_write_timeout(timeout).map_err(socket_failure)?;
        // Keep one descriptor to clear admission I/O waits after the first owned request.
        let settings = stream.try_clone().map_err(socket_failure)?;
        let mut connection =
            transport::accept(stream, &self.setup.private_key, self.setup.control_peer).map_err(
                |cause| ConnectionFailure {
                    slot,
                    received: None,
                    original: None,
                    cause: ApplicationError::Channel(cause),
                    fence_error: None,
                    diagnostic_error: None,
                },
            )?;
        // Publish one socket fence so the accept loop can reclaim this slot
        // when its worker parks in `receive()` past `idle_ttl` (#547).
        let fence = connection
            .close_handle()
            .map_err(|cause| ConnectionFailure {
                slot,
                received: None,
                original: None,
                cause: ApplicationError::Channel(cause),
                fence_error: None,
                diagnostic_error: None,
            })?;
        self.set_fence(ticket, fence);
        let mut first = true;
        loop {
            // An established connection parked for the next request is
            // reapable: nothing is in flight. The first receive stays under
            // the handshake deadline and is never idle.
            if !first {
                self.mark_idle(ticket);
            }
            let request = {
                let bytes = match connection.receive.receive() {
                    Ok(bytes) => bytes,
                    Err(cause) => {
                        return Err(ConnectionFailure {
                            slot,
                            received: None,
                            original: None,
                            cause: ApplicationError::Channel(cause),
                            fence_error: None,
                            diagnostic_error: None,
                        })
                    }
                };
                match InitialRecord::decode(bytes) {
                    Ok(request) => request,
                    Err(cause) => {
                        let received = Some(bytes.to_vec());
                        let fence_error = connection.send.close().err();
                        return Err(ConnectionFailure {
                            slot,
                            received,
                            original: None,
                            cause: ApplicationError::Protocol(cause),
                            fence_error,
                            diagnostic_error: None,
                        });
                    }
                }
            };
            // A request arrived: the slot is executing one decoded request
            // under an explicit deadline (in-flight, #547 slice 5), not
            // reapable as idle.
            let started = Instant::now();
            let deadline =
                started + Duration::from_millis(u64::from(self.setup.limits.request_deadline_ms));
            self.mark_inflight(ticket, started, deadline);
            if first {
                if let Err(cause) = settings
                    .set_read_timeout(None)
                    .and_then(|()| settings.set_write_timeout(None))
                {
                    let mut failure = socket_failure(cause);
                    failure.original = Some(request);
                    return Err(failure);
                }
                first = false;
            }
            match request {
                InitialRecord::Install(manifest) => {
                    let original = manifest.clone();
                    if !self
                        .install(&mut connection, manifest, slot)
                        .map_err(|mut failure| {
                            failure.original = Some(InitialRecord::Install(original));
                            failure
                        })?
                    {
                        return Ok(());
                    }
                }
                InitialRecord::Control(call) => {
                    // Write bodies a call declares follow its record; no call
                    // here reads them, so they are dropped before the answer.
                    let bodies = super::bodies::Bodies::of(&call.request, &mut connection.receive);
                    let (outcome, diagnostic_error, telemetry) =
                        if matches!(call.request, Request::Observed { .. }) {
                            self.observed_control(&call, slot, ticket.class)
                        } else {
                            (self.control(&call, ticket.class), None, None)
                        };
                    if let Err(cause) = bodies.finish() {
                        return Err(ConnectionFailure {
                            slot,
                            received: None,
                            original: None,
                            cause: ApplicationError::Channel(cause),
                            fence_error: None,
                            diagnostic_error,
                        });
                    }
                    let session_end =
                        matches!(call.request, Request::EndSession) && outcome.is_ok();
                    let reserved_refusal = ticket.class == SlotClass::Reserved
                        && matches!(
                            &outcome,
                            Err(crate::control::Failure::Rejected(
                                layerfs_bridge::control::ControlCode::Capacity,
                                _
                            ))
                        );
                    // A recovery-class connection serves exactly one request
                    // then closes (invariant 4), whatever that request's
                    // outcome, unless it was already refused Capacity above.
                    let recovery_served = ticket.class == SlotClass::Reserved
                        && !matches!(call.request, Request::Hello(_) | Request::EndSession)
                        && !reserved_refusal;
                    let ready = reported_ready(&outcome);
                    let committed = committed_token(&call.request);
                    let served = match crate::control::answer_call(
                        &mut connection,
                        call,
                        outcome,
                        telemetry,
                    ) {
                        Ok(served) if ready => {
                            self.reader_after_ready();
                            served
                        }
                        Ok(served) => served,
                        Err(cause) => {
                            if let Some(token) = committed {
                                self.reclaim_after_commit(token);
                            }
                            return Err(ConnectionFailure {
                                slot,
                                received: None,
                                original: None,
                                cause: ApplicationError::Control(cause),
                                fence_error: None,
                                diagnostic_error,
                            });
                        }
                    };
                    if let Some(token) = committed {
                        self.reclaim_after_commit(token);
                    }
                    if let Some(error) = diagnostic_error {
                        return Err(ConnectionFailure {
                            slot,
                            received: None,
                            original: None,
                            cause: ApplicationError::RetainedControl(Box::new(served)),
                            fence_error: connection.send.close().err(),
                            diagnostic_error: Some(error),
                        });
                    }
                    // A failed operation with unresolved custody is held, not
                    // dropped merely because its refusal was sent successfully.
                    if let Err(cause) = &served.outcome {
                        if cause.uncertain() {
                            return Err(ConnectionFailure {
                                slot,
                                received: None,
                                original: None,
                                cause: ApplicationError::RetainedControl(Box::new(served)),
                                fence_error: connection.send.close().err(),
                                diagnostic_error: None,
                            });
                        }
                    }
                    if session_end {
                        connection
                            .receive
                            .wait_peer_end()
                            .map_err(|ending| ConnectionFailure {
                                slot,
                                received: ending.received.map(|byte| vec![byte]),
                                original: None,
                                cause: ApplicationError::SessionEnd {
                                    served: Box::new(served),
                                    ending,
                                },
                                fence_error: None,
                                diagnostic_error: None,
                            })?;
                        return Ok(());
                    }
                    drop(served);
                    if reserved_refusal || recovery_served {
                        connection.send.close().map_err(|cause| ConnectionFailure {
                            slot,
                            received: None,
                            original: None,
                            cause: ApplicationError::Channel(cause),
                            fence_error: None,
                            diagnostic_error: None,
                        })?;
                        return Ok(());
                    }
                }
            }
        }
    }
    fn control(
        &self,
        call: &Call,
        class: SlotClass,
    ) -> Result<crate::control::Success, crate::control::Failure> {
        self.control_operation(&call.request, class)
    }
    /// After the daemon reported ready (a `Hello` answered ControlReady was
    /// sent), opens one idle Store reader on a thread of its own, so the
    /// first mount takes it instead of connecting (#64, owner decision
    /// 2026-11-06). It never delays readiness, opens at most once per Store
    /// (later calls return at once) and only where readers are shared
    /// (PostgreSQL). A failure is written to the daemon's log and counted
    /// (`ReuseWork::prefill_failures`), never tried again.
    fn reader_after_ready(&self) {
        let Ok(state) = self.state.lock() else {
            return;
        };
        let Some(service) = state.startup.service.clone() else {
            return;
        };
        drop(state);
        let store = service.store.clone();
        let spawned = std::thread::Builder::new()
            .name("layerfs-idle-reader".into())
            .spawn(move || {
                if let Some(Err(error)) = store.prefill_reader() {
                    eprintln!(
                        "layerfs-daemon: idle Store reader after readiness not opened: {error}"
                    );
                }
            });
        if let Err(error) = spawned {
            eprintln!("layerfs-daemon: idle Store reader thread not started: {error}");
        }
    }
    /// After a Commit was answered (or its answer failed), drains on this
    /// thread the overlay reclamation its releases left queued (owner
    /// approval, 2026-11-07). The connection reads its next call after it.
    fn reclaim_after_commit(&self, token: layerfs_bridge::control::WorkspaceToken) {
        let service = match self.state.lock() {
            Ok(state) => state.startup.service.clone(),
            Err(_) => return,
        };
        if let Some(service) = service {
            service.reclaim_after_commit(token);
        }
    }
    pub(super) fn control_operation(
        &self,
        request: &Request,
        class: SlotClass,
    ) -> Result<crate::control::Success, crate::control::Failure> {
        use crate::control::{Failure, Success};
        use layerfs_bridge::control::{ControlCode, DaemonPhase};
        if matches!(request, Request::EndSession) {
            return Ok(Success::reply(Reply::SessionEnded));
        }
        if let Request::Hello(request) = request {
            return self
                .hello(*request, class)
                .map(|status| Success::reply(Reply::Hello(status)));
        }
        let state = self.state.lock().map_err(|_| Failure::Poisoned)?;
        if class == SlotClass::Reserved && state.startup.phase != DaemonPhase::ControlReady {
            return Err(Failure::Rejected(
                ControlCode::Capacity,
                "reserved installer slot",
            ));
        }
        let service = state.startup.service.clone();
        let phase = state.startup.phase;
        drop(state);
        // The recovery-class reserved slot (ControlReady) serves only the
        // recovery operation set; anything else is refused Capacity and the
        // connection is closed (#547 slice 3).
        if class == SlotClass::Reserved && !recovery_request(request) {
            return Err(Failure::Rejected(
                ControlCode::Capacity,
                "reserved recovery slot",
            ));
        }
        // Operator-visible retained failures and the reclaim verb live with
        // the application, not with the Store control Service, so answer
        // these here (#540, #547 slice 5).
        if let Request::Sdk(call) = request {
            if call.method == layerfs_bridge::control::OPERATION {
                return self
                    .connection_failures()
                    .map(|value| Success::reply(Reply::Sdk(value)));
            }
            if call.method == RECLAIM {
                return self
                    .reclaim(call)
                    .map(|value| Success::reply(Reply::Sdk(value)));
            }
        }
        match service {
            Some(service) => service.execute_control(request),
            None if phase == DaemonPhase::Retained => Err(Failure::Rejected(
                ControlCode::Unknown,
                "original Store startup custody retained",
            )),
            None => Err(Failure::Rejected(
                ControlCode::Busy,
                "Store control Service not ready",
            )),
        }
    }

    /// `backend.daemon.reclaim { slot, generation }` (#547 slice 5): closes
    /// the named worker's socket, records `Reclaimed`, and frees its slot. It
    /// is generation-checked (a vacant slot or a newer worker answers
    /// `Invalid`), one-shot, never restarts the daemon and never touches a
    /// Workspace.
    fn reclaim(
        &self,
        call: &layerfs_bridge::control::SdkCall,
    ) -> Result<layerfs_bridge::control::SdkValue, crate::control::Failure> {
        use crate::control::Failure;
        use layerfs_bridge::control::{ControlCode, SdkValue};
        let slot = match call
            .args
            .get_int("slot")
            .and_then(|value| usize::try_from(value).ok())
        {
            Some(slot) => slot,
            None => {
                return Err(Failure::Rejected(
                    ControlCode::Invalid,
                    "reclaim names the slot to close",
                ))
            }
        };
        let generation = match call
            .args
            .get_int("generation")
            .and_then(|value| u64::try_from(value).ok())
        {
            Some(generation) => generation,
            None => {
                return Err(Failure::Rejected(
                    ControlCode::Invalid,
                    "reclaim names the generation to close",
                ))
            }
        };
        // Free the named slot and detach its worker under one lock, so the
        // accept loop can never join a worker this reclaim leaves running.
        // Then close the socket and record `Reclaimed`.
        let (ticket, owner) = {
            let mut state = self.state.lock().map_err(|_| Failure::Poisoned)?;
            match state.slots.reclaim(slot, generation) {
                Some(ticket) => {
                    state.workers[ticket.index] = None;
                    (ticket, self.owner_label(ticket))
                }
                None => {
                    return Err(Failure::Rejected(
                        ControlCode::Invalid,
                        "reclaim generation mismatch",
                    ))
                }
            }
        };
        self.close_slot_fences(std::slice::from_ref(&ticket));
        let now = Instant::now();
        let recorded = {
            let mut state = self.state.lock().map_err(|_| Failure::Poisoned)?;
            let recorded = state
                .retained
                .record(RetainedFailure::reclaimed(ticket, &owner), now);
            self.changed.notify_all();
            recorded
        };
        let _ = writeln!(std::io::stderr().lock(), "{recorded}");
        Ok(SdkValue::map([
            ("outcome", SdkValue::text("reclaimed")),
            ("slot", SdkValue::Int(slot as i64)),
            ("generation", SdkValue::Int(generation as i64)),
        ]))
    }

    /// The operator read model of control admission and retained failures.
    fn connection_failures(
        &self,
    ) -> Result<layerfs_bridge::control::SdkValue, crate::control::Failure> {
        use crate::control::Failure;
        let counts = self.connection_slots().map_err(|_| Failure::Poisoned)?;
        let snapshot = self
            .retained_connection_failures()
            .map_err(|_| Failure::Poisoned)?;
        Ok(capacity_report(counts, snapshot).to_sdk())
    }
}

/// The daemon's control-capacity read model from its slot counts and its
/// bounded retained-failure snapshot (#540). Pure: no lock and no store.
fn capacity_report(
    counts: SlotCounts,
    snapshot: RetainedSnapshot,
) -> layerfs_bridge::control::CapacityModel {
    use layerfs_bridge::control::{AdmissionModel, RetainedFailureModel, RetainedModel};
    let failures = snapshot
        .failures
        .iter()
        .map(|failure| RetainedFailureModel {
            slot: failure.slot as i64,
            class: failure.class.to_owned(),
            generation: failure.generation as i64,
            owner: failure.owner.clone(),
            received_bytes: failure.received_bytes.map(|bytes| bytes as i64),
            request: failure.request.map(str::to_owned),
            reply_sent: failure.reply_sent,
            cause: failure.cause.clone(),
            fence_error: failure.fence_error.clone(),
            diagnostic_error: failure.diagnostic_error.clone(),
            age_ms: failure.age.as_millis() as i64,
            expires_in_ms: failure.expires_in.as_millis() as i64,
        });
    layerfs_bridge::control::CapacityModel {
        admission: AdmissionModel {
            pool_vacant: counts.pool_vacant as i64,
            pool_running: counts.pool_running as i64,
            reserved_vacant: counts.reserved_vacant as i64,
            reserved_running: counts.reserved_running as i64,
        },
        retained: RetainedModel {
            limit: snapshot.limit as i64,
            ttl_ms: snapshot.ttl.as_millis() as i64,
            count: snapshot.failures.len() as i64,
            dropped: snapshot.dropped as i64,
            recorded: snapshot.recorded as i64,
            failures: failures.collect(),
        },
    }
}

/// The Workspace of a Commit request, observed or not.
fn committed_token(request: &Request) -> Option<layerfs_bridge::control::WorkspaceToken> {
    match request {
        Request::Commit(control) => Some(control.token),
        Request::Observed { request, .. } => committed_token(request),
        _ => None,
    }
}

/// Whether `outcome` answers a `Hello` with the daemon ControlReady.
fn reported_ready(outcome: &Result<crate::control::Success, crate::control::Failure>) -> bool {
    matches!(outcome, Ok(success) if matches!(
        &success.reply,
        Reply::Hello(status) if status.phase == layerfs_bridge::control::DaemonPhase::ControlReady
    ))
}

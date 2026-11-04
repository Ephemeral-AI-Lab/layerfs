//! Bounded real authenticated connection workers and retained original failures.
use super::{
    owner::{AdmissionClass, Finished, SlotClass, SlotTicket},
    retained::RetainedFailure,
    Application, ApplicationError, ConnectionFailure,
};
use layerfs_bridge::{
    control::{Call, DaemonPhase, Reply, Request},
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
        // Build the bounded record before taking the state lock.
        let prebuilt = result.as_ref().err().map(|failure| {
            RetainedFailure::describe(
                failure,
                self.ticket.class,
                self.ticket.generation,
                &self.owner,
            )
        });
        // Poison recovery here only releases the slot and records; normal
        // admission still reports `Poisoned` rather than trusting corrupt state.
        let finished = {
            let mut state = self
                .application
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let finished = state
                .slots
                .finish(self.ticket, result, prebuilt, Instant::now());
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
                let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                let picked = state.slots.admit(AdmissionClass::ControlPool).or_else(|| {
                    // Only while the one-time installer may still run: an
                    // ordinary control never takes the reserved slot.
                    if matches!(
                        state.startup.phase,
                        DaemonPhase::InstallPending | DaemonPhase::Installing
                    ) {
                        state.slots.admit(AdmissionClass::Install)
                    } else {
                        None
                    }
                });
                match picked {
                    Some(ticket) => ticket,
                    None => {
                        let counts = state.slots.counts();
                        let retained = state.slots.visible();
                        state.slots.reap(Instant::now());
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
            let previous = self
                .workers
                .lock()
                .map_err(|_| ApplicationError::Poisoned)?[ticket.index]
                .take();
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
                    guard.finish(result);
                });
            match worker {
                Ok(worker) => {
                    self.workers
                        .lock()
                        .map_err(|_| ApplicationError::Poisoned)?[ticket.index] = Some(worker)
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
        let mut first = true;
        loop {
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
                            self.observed_control(&call, slot)
                        } else {
                            (self.control(&call, slot), None, None)
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
                    let served = match crate::control::answer_call(
                        &mut connection,
                        call,
                        outcome,
                        telemetry,
                    ) {
                        Ok(served) => served,
                        Err(cause) => {
                            return Err(ConnectionFailure {
                                slot,
                                received: None,
                                original: None,
                                cause: ApplicationError::Control(cause),
                                fence_error: None,
                                diagnostic_error,
                            })
                        }
                    };
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
                    if reserved_refusal {
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
        slot: usize,
    ) -> Result<crate::control::Success, crate::control::Failure> {
        self.control_operation(&call.request, slot)
    }
    pub(super) fn control_operation(
        &self,
        request: &Request,
        slot: usize,
    ) -> Result<crate::control::Success, crate::control::Failure> {
        use crate::control::{Failure, Success};
        use layerfs_bridge::control::{ControlCode, DaemonPhase};
        if matches!(request, Request::EndSession) {
            return Ok(Success::reply(Reply::SessionEnded));
        }
        if let Request::Hello(request) = request {
            return self
                .hello(*request, slot)
                .map(|status| Success::reply(Reply::Hello(status)));
        }
        let state = self.state.lock().map_err(|_| Failure::Poisoned)?;
        if slot == usize::from(self.setup.limits.connections)
            && state.startup.phase != DaemonPhase::ControlReady
        {
            return Err(Failure::Rejected(
                ControlCode::Capacity,
                "reserved installer slot",
            ));
        }
        let service = state.startup.service.clone();
        let phase = state.startup.phase;
        drop(state);
        // Operator-visible retained failures live with the application, not
        // with the Store control Service, so answer this one here.
        if let Request::Sdk(call) = request {
            if call.method == CONNECTION_FAILURES {
                return self
                    .connection_failures()
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

    /// The operator read model of control admission and retained failures.
    fn connection_failures(
        &self,
    ) -> Result<layerfs_bridge::control::SdkValue, crate::control::Failure> {
        use crate::control::Failure;
        use layerfs_bridge::control::SdkValue;
        let counts = self.connection_slots().map_err(|_| Failure::Poisoned)?;
        let snapshot = self
            .retained_connection_failures()
            .map_err(|_| Failure::Poisoned)?;
        let failures = snapshot
            .failures
            .iter()
            .map(|failure| {
                SdkValue::map([
                    ("slot", SdkValue::Int(failure.slot as i64)),
                    ("class", SdkValue::text(failure.class)),
                    ("generation", SdkValue::Int(failure.generation as i64)),
                    ("owner", SdkValue::text(&failure.owner)),
                    (
                        "received_bytes",
                        failure
                            .received_bytes
                            .map_or(SdkValue::Null, |bytes| SdkValue::Int(bytes as i64)),
                    ),
                    (
                        "request",
                        failure.request.map_or(SdkValue::Null, SdkValue::text),
                    ),
                    ("reply_sent", SdkValue::Bool(failure.reply_sent)),
                    ("cause", SdkValue::text(&failure.cause)),
                    (
                        "fence_error",
                        failure
                            .fence_error
                            .as_deref()
                            .map_or(SdkValue::Null, SdkValue::text),
                    ),
                    (
                        "diagnostic_error",
                        failure
                            .diagnostic_error
                            .as_deref()
                            .map_or(SdkValue::Null, SdkValue::text),
                    ),
                    ("age_ms", SdkValue::Int(failure.age.as_millis() as i64)),
                    (
                        "expires_in_ms",
                        SdkValue::Int(failure.expires_in.as_millis() as i64),
                    ),
                ])
            })
            .collect();
        Ok(SdkValue::map([
            (
                "admission",
                SdkValue::map([
                    ("pool_vacant", SdkValue::Int(counts.pool_vacant as i64)),
                    ("pool_running", SdkValue::Int(counts.pool_running as i64)),
                    (
                        "reserved_vacant",
                        SdkValue::Int(counts.reserved_vacant as i64),
                    ),
                    (
                        "reserved_running",
                        SdkValue::Int(counts.reserved_running as i64),
                    ),
                ]),
            ),
            (
                "retained",
                SdkValue::map([
                    ("limit", SdkValue::Int(snapshot.limit as i64)),
                    ("ttl_ms", SdkValue::Int(snapshot.ttl.as_millis() as i64)),
                    ("count", SdkValue::Int(snapshot.failures.len() as i64)),
                    ("dropped", SdkValue::Int(snapshot.dropped as i64)),
                    ("recorded", SdkValue::Int(snapshot.recorded as i64)),
                    ("failures", SdkValue::List(failures)),
                ]),
            ),
        ]))
    }
}

/// Operator/control method for the daemon's control admission and retained
/// connection failures.
pub(super) const CONNECTION_FAILURES: &str = "backend.daemon.connection_failures";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::owner::{Slots, Startup, State};
    use crate::{Owner, OwnerConfig};
    use layerfs_bridge::{
        control::{Answer, Call, DaemonPhase, HelloRequest, Reply, Request},
        daemon_setup::{DaemonLimits, DaemonSetup},
        store::DaemonStore,
        KernelRoute,
    };
    use layerfs_overlay::ProfileConfig;
    use std::{
        net::TcpStream,
        path::PathBuf,
        process,
        sync::{Condvar, Mutex},
        time::{Duration, Instant},
    };

    const CONNECTIONS: u16 = 8;
    const CLIENT_PRIVATE: [u8; 32] = [71; 32];
    const SERVER_PRIVATE: [u8; 32] = [72; 32];

    fn temp_directory(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "layerfs-540-{}-{label}-{}",
            process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(directory.join("mounts")).unwrap();
        directory
    }

    /// A real `Application` over an empty Store path: Hello and EndSession
    /// need no Store service, and no kernel serving is assembled.
    fn application(directory: &std::path::Path) -> Arc<Application> {
        let owner = Owner::start(
            &directory.join("overlay.sqlite"),
            ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let setup = DaemonSetup {
            listen: "127.0.0.1:0".into(),
            private_key: SERVER_PRIVATE,
            control_peer: layerfs_bridge::duplex::public_key(&CLIENT_PRIVATE).unwrap(),
            route: KernelRoute::Nfs,
            overlay: directory.join("overlay.sqlite").display().to_string(),
            mounts: directory.join("mounts").display().to_string(),
            command_uid: 501,
            command_gid: 20,
            limits: DaemonLimits {
                connections: CONNECTIONS,
                handshake_ms: 2000,
                cache_bytes: 1024 * 1024,
                namespaces: 16,
                pager_kib: 1024,
                serial_low_water: 256,
            },
            store: DaemonStore::Sqlite {
                locator: directory.join("store.sqlite").display().to_string(),
                install: None,
            },
        };
        let slots = usize::from(CONNECTIONS) + 1;
        Arc::new(Application {
            instance: layerfs_bridge::duplex::public_key(&SERVER_PRIVATE).unwrap(),
            setup,
            owner,
            kernel: None,
            export: None,
            state: Mutex::new(State {
                startup: Startup {
                    // No Store service in this fixture; InstallPending is the
                    // observable phase whose Hello reply needs no Store facts.
                    phase: DaemonPhase::InstallPending,
                    service: None,
                    installed: None,
                    existing: None,
                    install_failure: None,
                },
                slots: Slots::new(slots),
            }),
            changed: Condvar::new(),
            workers: Mutex::new((0..slots).map(|_| None).collect()),
        })
    }

    fn connect(address: std::net::SocketAddr) -> transport::Connection {
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        transport::initiate(
            stream,
            &CLIENT_PRIVATE,
            layerfs_bridge::duplex::public_key(&SERVER_PRIVATE).unwrap(),
        )
        .unwrap()
    }

    fn until(what: &str, check: impl Fn() -> bool) {
        let started = Instant::now();
        while !check() {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "timed out waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Eight-plus clients send bytes and die before reading a reply. Every
    /// retained failure must free its slot, be stored, and leave the daemon
    /// admitting a new authenticated client (#540).
    #[test]
    fn retained_failures_do_not_consume_control_slots() {
        let directory = temp_directory("slots");
        let application = application(&directory);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let serving = application.clone();
        std::thread::spawn(move || {
            let _ = serving.serve(listener);
        });

        let rounds = usize::from(CONNECTIONS) + 4;
        for round in 0..rounds {
            let mut connection = connect(address);
            // Not a decodable initial record: the daemon counts a received
            // request and retains its exact cause. The client then disappears
            // without reading any reply.
            connection
                .send
                .send(b"client-died-before-reading-reply")
                .unwrap();
            drop(connection);
            until("the retained failure to be recorded", || {
                application
                    .retained_connection_failures()
                    .map(|snapshot| snapshot.failures.len() == round + 1)
                    .unwrap_or(false)
            });
            let counts = application.connection_slots().unwrap();
            assert!(
                counts.pool_vacant > 0,
                "round {round}: a retained failure consumed the last pool slot"
            );
        }

        let snapshot = application.retained_connection_failures().unwrap();
        assert_eq!(snapshot.failures.len(), rounds);
        assert!(snapshot.dropped <= 1);
        let operator = application.connection_failures().unwrap();
        let retained = operator.get("retained").unwrap();
        assert_eq!(retained.get_int("limit"), Some(64));
        assert_eq!(retained.get_int("recorded"), Some(rounds as i64));
        assert_eq!(
            operator
                .get("admission")
                .and_then(|v| v.get_int("pool_vacant")),
            Some(usize::from(CONNECTIONS) as i64)
        );
        assert_eq!(
            retained.get("failures").map(|v| match v {
                layerfs_bridge::control::SdkValue::List(items) => items.len(),
                _ => 0,
            }),
            Some(rounds)
        );

        let record = &snapshot.failures[0];
        assert_eq!(record.slot, 0);
        assert_eq!(record.class, "pool");
        assert!(record.generation > 0);
        assert!(
            record.owner.contains("layerfs-control-0"),
            "{}",
            record.owner
        );
        assert!(record.received_bytes.is_some());
        assert!(record.cause.contains("kernel request"), "{}", record.cause);
        assert!(!record.reply_sent);

        // A new authenticated client is admitted and receives its Hello reply.
        let mut connection = connect(address);
        let hello = Call {
            id: 1,
            request: Request::Hello(HelloRequest {
                expected_instance: Some(application.instance),
                wait_for_store: false,
            }),
        };
        connection.send.send(&hello.encode().unwrap()).unwrap();
        let answer = Answer::decode(connection.receive.receive().unwrap()).unwrap();
        match answer.reply {
            Reply::Hello(status) => assert_eq!(status.phase, DaemonPhase::InstallPending),
            other => panic!("unexpected Hello reply: {other:?}"),
        }
        drop(connection);
    }

    /// The guard is the spawn-failure/panic path: dropping it without an
    /// explicit finish releases the slot and records exactly once.
    #[test]
    fn dropping_the_guard_releases_the_slot_and_records_once() {
        let directory = temp_directory("guard");
        let application = application(&directory);
        let ticket = {
            let mut state = application.state.lock().unwrap();
            state.slots.admit(AdmissionClass::ControlPool).unwrap()
        };
        {
            let _guard = SlotGuard {
                application: application.clone(),
                ticket,
                owner: "test:pid:layerfs-control-0".to_owned(),
                done: false,
            };
            // dropped here without an explicit finish
        }
        let counts = application.connection_slots().unwrap();
        assert_eq!(counts.pool_running, 0);
        assert_eq!(counts.pool_vacant, usize::from(CONNECTIONS));
        let snapshot = application.retained_connection_failures().unwrap();
        assert_eq!(snapshot.failures.len(), 1);
        assert_eq!(snapshot.failures[0].slot, ticket.index);
        assert!(
            snapshot.failures[0].cause.contains("dropped"),
            "{}",
            snapshot.failures[0].cause
        );
    }

    /// With every slot held, the accept loop writes the advisory notice the
    /// client classifies instead of a bare EOF (#540).
    #[test]
    fn no_vacant_slot_writes_the_capacity_notice() {
        let directory = temp_directory("notice");
        let application = application(&directory);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let serving = application.clone();
        std::thread::spawn(move || {
            let _ = serving.serve(listener);
        });
        let tickets = {
            let mut state = application.state.lock().unwrap();
            let mut tickets = (0..CONNECTIONS)
                .map(|_| state.slots.admit(AdmissionClass::ControlPool).unwrap())
                .collect::<Vec<_>>();
            tickets.push(state.slots.admit(AdmissionClass::Install).unwrap());
            tickets
        };
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let error = match transport::initiate(
            stream,
            &CLIENT_PRIVATE,
            layerfs_bridge::duplex::public_key(&SERVER_PRIVATE).unwrap(),
        ) {
            Ok(_) => panic!("a full daemon admitted a connection"),
            Err(error) => error,
        };
        match error {
            transport::ChannelError::Admission(notice) => {
                assert_eq!(
                    notice.reason,
                    layerfs_bridge::duplex::admission::AdmissionReason::Starting
                );
                assert_eq!(notice.pool_vacant, 0);
                assert_eq!(notice.reserved_vacant, 0);
            }
            other => panic!("expected a capacity notice, got {other:?}"),
        }
        let mut state = application.state.lock().unwrap();
        for ticket in tickets {
            state.slots.finish(ticket, Ok(()), None, Instant::now());
        }
    }

    /// Sixteen concurrent failures finish without deadlock or leak, and a
    /// new authenticated connection is admitted afterwards.
    #[test]
    fn concurrent_failures_do_not_deadlock_or_leak() {
        let directory = temp_directory("concurrent");
        let application = application(&directory);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let serving = application.clone();
        std::thread::spawn(move || {
            let _ = serving.serve(listener);
        });
        let mut threads = Vec::new();
        for _ in 0..16 {
            threads.push(std::thread::spawn(move || {
                for _ in 0..4 {
                    // Under a real failure storm the pool can be momentarily
                    // full; admission refusal is precisely non-terminal, so
                    // retry until this round is admitted.
                    loop {
                        let stream =
                            TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut connection = match transport::initiate(
                            stream,
                            &CLIENT_PRIVATE,
                            layerfs_bridge::duplex::public_key(&SERVER_PRIVATE).unwrap(),
                        ) {
                            Ok(connection) => connection,
                            Err(transport::ChannelError::Admission(_)) => {
                                std::thread::sleep(Duration::from_millis(5));
                                continue;
                            }
                            Err(error) => panic!("unexpected open failure: {error:?}"),
                        };
                        connection
                            .send
                            .send(b"client-died-before-reading-reply")
                            .unwrap();
                        drop(connection);
                        break;
                    }
                }
            }));
        }
        for thread in threads {
            thread.join().unwrap();
        }
        until("every concurrent worker to finish", || {
            application
                .connection_slots()
                .map(|counts| counts.pool_running == 0)
                .unwrap_or(false)
        });
        let counts = application.connection_slots().unwrap();
        assert_eq!(counts.pool_running, 0);
        assert_eq!(counts.pool_vacant, usize::from(CONNECTIONS));
        let snapshot = application.retained_connection_failures().unwrap();
        // Some connections die before their first decoded request and are
        // released with a log line rather than retained; the invariant is
        // that the ring stays bounded and its counters are exact.
        assert!(snapshot.failures.len() <= 64);
        assert!(snapshot.failures.len() >= 1);
        assert_eq!(
            snapshot.recorded,
            snapshot.failures.len() as u64 + snapshot.dropped
        );

        let mut connection = connect(address);
        let hello = Call {
            id: 1,
            request: Request::Hello(HelloRequest {
                expected_instance: Some(application.instance),
                wait_for_store: false,
            }),
        };
        connection.send.send(&hello.encode().unwrap()).unwrap();
        let answer = Answer::decode(connection.receive.receive().unwrap()).unwrap();
        assert!(matches!(answer.reply, Reply::Hello(_)));
    }
}

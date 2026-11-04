//! Bounded real authenticated connection workers and retained original failures.
use super::{owner::Finished, Application, ApplicationError, ConnectionFailure};
use layerfs_bridge::{
    control::{Call, Reply, Request},
    duplex::{self as transport},
    initial_record::InitialRecord,
};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

impl Application {
    /// Accepts ordinary authenticated controls on one listener. The listener and
    /// each admitted worker own their exact sockets; no command execution occurs.
    /// A failed connection releases its slot at once; a compact record of a
    /// failure that used to retain custody stays in the bounded store.
    pub fn serve(self: &Arc<Self>, listener: TcpListener) -> Result<(), ApplicationError> {
        loop {
            let (stream, _) = listener.accept().map_err(ApplicationError::Io)?;
            let slot = {
                let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                let Some(slot) = state.slots.admit() else {
                    // No authenticated operation has entered; admission ends this
                    // socket once, rather than allocating an unbounded worker.
                    drop(stream);
                    continue;
                };
                slot
            };
            let previous = self
                .workers
                .lock()
                .map_err(|_| ApplicationError::Poisoned)?[slot]
                .take();
            if let Some(worker) = previous {
                worker.join().map_err(|_| {
                    ApplicationError::Io(std::io::Error::other(
                        "connection worker panicked; custody unestablished",
                    ))
                })?;
            }
            let application = self.clone();
            let worker = thread::Builder::new()
                .name(format!("layerfs-control-{slot}"))
                .spawn(move || {
                    let result = application.connection(stream, slot);
                    // Poison recovery records this exact result only; poisoned
                    // state never authorizes another product operation. The slot
                    // becomes Vacant even for a retained failure (#540): failure
                    // records live in a bounded store, not in admission capacity.
                    let finished = {
                        let mut state = application.state.lock().unwrap_or_else(|e| e.into_inner());
                        let finished = state.slots.finish(slot, result, Instant::now());
                        application.changed.notify_all();
                        finished
                    };
                    // Every retained failure is visible in daemon.log, in the
                    // same ConnectionFailure style as the released ones.
                    match finished {
                        Finished::Clean => {}
                        Finished::Released(failure) => {
                            let _ = writeln!(std::io::stderr().lock(), "{failure:?}");
                        }
                        Finished::Retained(record) => {
                            let _ = writeln!(std::io::stderr().lock(), "{record}");
                        }
                    }
                });
            match worker {
                Ok(worker) => {
                    self.workers
                        .lock()
                        .map_err(|_| ApplicationError::Poisoned)?[slot] = Some(worker)
                }
                Err(cause) => {
                    // The worker thread never ran. Release the slot and keep the
                    // same bounded operator-visible record as a failed worker.
                    let failure = ConnectionFailure {
                        slot,
                        received: None,
                        original: None,
                        cause: ApplicationError::Io(cause),
                        fence_error: None,
                        diagnostic_error: None,
                    };
                    let finished = {
                        let mut state =
                            self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                        let finished = state.slots.finish(slot, Err(failure), Instant::now());
                        self.changed.notify_all();
                        finished
                    };
                    match finished {
                        Finished::Retained(record) => {
                            let _ = writeln!(std::io::stderr().lock(), "{record}");
                        }
                        Finished::Released(failure) => {
                            let _ = writeln!(std::io::stderr().lock(), "{failure:?}");
                        }
                        Finished::Clean => {}
                    }
                }
            }
        }
    }
    fn connection(&self, stream: TcpStream, slot: usize) -> Result<(), ConnectionFailure> {
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
                    let reserved_refusal = slot == usize::from(self.setup.limits.connections)
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
        let (vacant, running) = self.connection_slots().map_err(|_| Failure::Poisoned)?;
        let snapshot = self
            .retained_connection_failures()
            .map_err(|_| Failure::Poisoned)?;
        let failures = snapshot
            .failures
            .iter()
            .map(|failure| {
                SdkValue::map([
                    ("slot", SdkValue::Int(failure.slot as i64)),
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
                ])
            })
            .collect();
        Ok(SdkValue::map([
            (
                "admission",
                SdkValue::map([
                    ("vacant", SdkValue::Int(vacant as i64)),
                    ("running", SdkValue::Int(running as i64)),
                ]),
            ),
            ("limit", SdkValue::Int(snapshot.limit as i64)),
            ("ttl_ms", SdkValue::Int(snapshot.ttl.as_millis() as i64)),
            ("dropped", SdkValue::Int(snapshot.dropped as i64)),
            ("failures", SdkValue::List(failures)),
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
            let (vacant, _) = application.connection_slots().unwrap();
            assert!(
                vacant > 0,
                "round {round}: a retained failure consumed the last slot"
            );
        }

        let snapshot = application.retained_connection_failures().unwrap();
        assert_eq!(snapshot.failures.len(), rounds);
        assert!(snapshot.dropped <= 1);
        let operator = application.connection_failures().unwrap();
        assert_eq!(operator.get_int("limit"), Some(64));
        assert_eq!(
            operator.get("admission").and_then(|v| v.get_int("vacant")),
            Some(usize::from(CONNECTIONS + 1) as i64)
        );
        assert_eq!(
            operator.get("failures").map(|v| match v {
                layerfs_bridge::control::SdkValue::List(items) => items.len(),
                _ => 0,
            }),
            Some(rounds)
        );

        let record = &snapshot.failures[0];
        assert_eq!(record.slot, 0);
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
}

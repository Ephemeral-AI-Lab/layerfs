//! Bounded real authenticated connection workers and retained original failures.
use super::{owner::Slot, Application, ApplicationError, ConnectionFailure};
use layerfs_bridge::{
    control::{Call, Reply, Request},
    initial_record::InitialRecord,
    native::{self},
};
use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

impl Application {
    /// Accepts ordinary authenticated controls on one listener. The listener and
    /// each admitted worker own their exact sockets; no command execution occurs.
    /// A failed connection keeps its bounded slot/custody until explicit transfer.
    pub fn serve(self: &Arc<Self>, listener: TcpListener) -> Result<(), ApplicationError> {
        loop {
            let (stream, _) = listener.accept().map_err(ApplicationError::Io)?;
            let slot = {
                let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                let Some(slot) = state.slots.iter().position(|v| matches!(v, Slot::Vacant)) else {
                    // No authenticated operation has entered; admission ends this
                    // socket once, rather than allocating an unbounded worker.
                    drop(stream);
                    continue;
                };
                state.slots[slot] = Slot::Running;
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
                    let mut result = application.connection(stream, slot);
                    if let Err(failure) = &mut result {
                        if failure.original.is_none()
                            && failure.received.is_none()
                            && matches!(
                                failure.cause,
                                ApplicationError::Channel(_) | ApplicationError::Io(_)
                            )
                        {
                            // No decoded product attempt/custody exists at this boundary.
                            // Transfer this bounded original diagnostic to the daemon
                            // stderr owner before releasing capacity. Failed transfer
                            // keeps the original receipt and its distinct output error.
                            match writeln!(std::io::stderr().lock(), "{failure:?}") {
                                Ok(()) => result = Ok(()),
                                Err(error) => failure.diagnostic_error = Some(error),
                            }
                        }
                    }
                    {
                        // Poison recovery retains this exact result only; poisoned
                        // state never authorizes another product operation.
                        let mut state = application.state.lock().unwrap_or_else(|e| e.into_inner());
                        state.slots[slot] = match result {
                            Ok(()) => Slot::Vacant,
                            Err(cause) => Slot::Retained(Box::new(cause)),
                        };
                        application.changed.notify_all();
                    }
                });
            match worker {
                Ok(worker) => {
                    self.workers
                        .lock()
                        .map_err(|_| ApplicationError::Poisoned)?[slot] = Some(worker)
                }
                Err(cause) => {
                    let mut state = self.state.lock().map_err(|_| ApplicationError::Poisoned)?;
                    state.slots[slot] = Slot::Retained(Box::new(ConnectionFailure {
                        slot,
                        received: None,
                        original: None,
                        cause: ApplicationError::Io(cause),
                        fence_error: None,
                        diagnostic_error: None,
                    }));
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
            native::accept(stream, &self.setup.private_key, self.setup.control_peer).map_err(
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
                    let (outcome, diagnostic_error) =
                        if matches!(call.request, Request::Observed { .. }) {
                            self.observed_control(&call, slot)
                        } else {
                            (self.control(&call, slot), None)
                        };
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
                    let served = match crate::control::answer_call(&mut connection, call, outcome) {
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
}
